//! HTTP/WS/Job 계약을 클러스터 원자로 묶는다.

use crate::config::PathPolicy;
use crate::domain::capability_keys::{
    canonical_capability_key, endpoint_class_capability_key, is_leaf_capability_key,
};
use crate::domain::contract_path::{capability_key_from_path, normalize_contract_path};
use crate::facts::{
    CodeUnitKind, Entrypoint, EntrypointKind, FactStore, ResourceAccess, ResourceKind,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

#[derive(Debug, Clone)]
pub(crate) struct Capability {
    pub key: String,
    pub entrypoint_ids: Vec<String>,
    pub resource_ids: Vec<String>,
    pub unit_ids: Vec<String>,
    pub contract_paths: BTreeSet<String>,
}

pub(crate) fn build(store: &FactStore, path_policy: &PathPolicy) -> Vec<Capability> {
    let unit_capability_keys = unit_capability_keys(store, path_policy);
    let mut buckets: BTreeMap<String, CapabilityBuilder> = BTreeMap::new();

    for entrypoint in &store.entrypoints {
        if !is_capability_entrypoint_kind(&entrypoint.kind) {
            continue;
        }
        let Some(unit) = store.unit(&entrypoint.unit_id) else {
            continue;
        };
        let production = path_policy.is_production_path(&unit.relative_path);
        let archived = path_policy.is_archived_path(&unit.relative_path);
        if !production && !archived {
            continue;
        }
        let Some(raw_key) = (match entrypoint.kind {
            EntrypointKind::Job => job_capability_key(entrypoint),
            EntrypointKind::Rpc => rpc_capability_key(entrypoint, store)
                .or_else(|| contract_capability_key(entrypoint))
                .or_else(|| unit_capability_keys.get(&entrypoint.unit_id).cloned()),
            _ => http_capability_key(entrypoint, store, &unit_capability_keys),
        }) else {
            continue;
        };
        let key = canonical_capability_key(&raw_key);
        buckets
            .entry(key.clone())
            .or_insert_with(|| CapabilityBuilder::new(&key))
            .add_entrypoint(entrypoint, &entrypoint.unit_id, production);
    }

    for resource in &store.resources {
        if !matches!(
            resource.kind,
            ResourceKind::ExternalApi | ResourceKind::WebSocket
        ) {
            continue;
        }
        let Some(unit) = store.unit(&resource.unit_id) else {
            continue;
        };
        if !path_policy.is_production_path(&unit.relative_path) {
            continue;
        }
        let Some(raw_key) = capability_key_from_path(&resource.name) else {
            continue;
        };
        let key = canonical_capability_key(&raw_key);
        buckets
            .entry(key.clone())
            .or_insert_with(|| CapabilityBuilder::new(&key))
            .add_resource(resource);
    }

    assign_client_homes(&mut buckets, store, path_policy);

    let mut capabilities = buckets
        .into_values()
        .map(CapabilityBuilder::finish)
        .collect::<Vec<_>>();
    capabilities.sort_by(|left, right| left.key.cmp(&right.key));
    capabilities
}

fn assign_client_homes(
    buckets: &mut BTreeMap<String, CapabilityBuilder>,
    store: &FactStore,
    path_policy: &PathPolicy,
) {
    let handler_units: HashSet<String> = buckets
        .values()
        .flat_map(|bucket| bucket.unit_ids.iter().cloned())
        .collect();
    let mut counts: HashMap<String, BTreeMap<String, usize>> = HashMap::new();
    for resource in &store.resources {
        if !matches!(
            resource.kind,
            ResourceKind::ExternalApi | ResourceKind::WebSocket
        ) {
            continue;
        }
        if handler_units.contains(&resource.unit_id) {
            continue;
        }
        let Some(unit) = store.unit(&resource.unit_id) else {
            continue;
        };
        if !path_policy.is_production_path(&unit.relative_path) {
            continue;
        }
        let Some(key) = capability_key_from_path(&resource.name) else {
            continue;
        };
        let key = canonical_capability_key(&key);
        if !buckets.contains_key(&key) {
            continue;
        }
        *counts
            .entry(resource.unit_id.clone())
            .or_default()
            .entry(key)
            .or_default() += 1;
    }
    for (unit_id, key_counts) in counts {
        let Some(home_key) = key_counts
            .iter()
            .max_by(|(left_key, left_count), (right_key, right_count)| {
                left_count.cmp(right_count).then(right_key.cmp(left_key))
            })
            .map(|(key, _)| key.clone())
        else {
            continue;
        };
        if let Some(bucket) = buckets.get_mut(&home_key) {
            bucket.unit_ids.insert(unit_id);
        }
    }
}

fn is_capability_entrypoint_kind(kind: &EntrypointKind) -> bool {
    matches!(
        kind,
        EntrypointKind::Http
            | EntrypointKind::WebSocket
            | EntrypointKind::Rpc
            | EntrypointKind::Job
    )
}

fn contract_capability_key(entrypoint: &Entrypoint) -> Option<String> {
    let raw = entrypoint.path.as_deref().unwrap_or(&entrypoint.name);
    capability_key_from_path(raw).map(|key| canonical_capability_key(&key))
}

fn http_capability_key(
    entrypoint: &Entrypoint,
    store: &FactStore,
    unit_capability_keys: &HashMap<String, String>,
) -> Option<String> {
    if let Some(key) = contract_capability_key(entrypoint)
        .filter(|key| !is_leaf_capability_key(key) && !is_transport_capability_key(key))
    {
        return Some(key);
    }
    if let Some(key) = handler_class_capability_key(entrypoint, store) {
        return Some(key);
    }
    contract_capability_key(entrypoint)
        .or_else(|| unit_capability_keys.get(&entrypoint.unit_id).cloned())
}

fn is_transport_capability_key(key: &str) -> bool {
    matches!(
        key,
        "shop-api" | "admin-api" | "gateway" | "graphql" | "connect" | "rpc"
    )
}

fn handler_class_capability_key(entrypoint: &Entrypoint, store: &FactStore) -> Option<String> {
    let unit = store.unit(&entrypoint.unit_id)?;
    let mut current = Some(unit);
    while let Some(unit) = current {
        if unit.kind == CodeUnitKind::Class {
            if let Some(key) = endpoint_class_capability_key(&unit.name, "Controller")
                .or_else(|| endpoint_class_capability_key(&unit.name, "Resolver"))
                .or_else(|| endpoint_class_capability_key(&unit.name, "Endpoint"))
                .or_else(|| endpoint_class_capability_key(&unit.name, "Service"))
                .or_else(|| endpoint_class_capability_key(&unit.name, "Handler"))
            {
                return Some(canonical_capability_key(&key));
            }
        }
        current = unit
            .parent_id
            .as_deref()
            .and_then(|parent_id| store.unit(parent_id));
    }
    None
}

fn rpc_capability_key(entrypoint: &Entrypoint, store: &FactStore) -> Option<String> {
    let unit = store.unit(&entrypoint.unit_id)?;
    let parent_id = unit.parent_id.as_deref()?;
    let parent = store.unit(parent_id)?;
    if parent.kind != CodeUnitKind::Class {
        return None;
    }
    endpoint_class_capability_key(&parent.name, "Endpoint")
        .or_else(|| endpoint_class_capability_key(&parent.name, "Service"))
}

fn unit_capability_keys(store: &FactStore, path_policy: &PathPolicy) -> HashMap<String, String> {
    let mut candidates: HashMap<String, Vec<(String, usize)>> = HashMap::new();

    for entrypoint in &store.entrypoints {
        if !is_capability_entrypoint_kind(&entrypoint.kind)
            || entrypoint.kind == EntrypointKind::Job
        {
            continue;
        }
        let Some(unit) = store.unit(&entrypoint.unit_id) else {
            continue;
        };
        if !path_policy.is_production_path(&unit.relative_path)
            && !path_policy.is_archived_path(&unit.relative_path)
        {
            continue;
        }
        let raw = entrypoint.path.as_deref().unwrap_or(&entrypoint.name);
        let Some(key) = (match entrypoint.kind {
            EntrypointKind::Rpc => {
                rpc_capability_key(entrypoint, store).or_else(|| capability_key_from_path(raw))
            }
            _ => capability_key_from_path(raw),
        }) else {
            continue;
        };
        let segment_count = normalize_contract_path(raw)
            .map(|path| path.matches('/').count())
            .unwrap_or(0);
        candidates
            .entry(entrypoint.unit_id.clone())
            .or_default()
            .push((key, segment_count));
    }

    candidates
        .into_iter()
        .filter_map(|(unit_id, mut keys)| {
            keys.sort_by(|left, right| {
                right
                    .1
                    .cmp(&left.1)
                    .then_with(|| capability_key_rank(&right.0).cmp(&capability_key_rank(&left.0)))
                    .then_with(|| left.0.cmp(&right.0))
            });
            keys.first()
                .map(|(key, _)| (unit_id, canonical_capability_key(key)))
        })
        .collect()
}

fn capability_key_rank(key: &str) -> u8 {
    if is_leaf_capability_key(key) {
        0
    } else {
        1
    }
}

fn job_capability_key(entrypoint: &Entrypoint) -> Option<String> {
    let raw = entrypoint
        .path
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&entrypoint.name);
    let normalized = normalize_contract_path(raw).unwrap_or_else(|| raw.to_ascii_lowercase());
    let key = format!("job:{}", normalized.trim_start_matches('/'));
    if key == "job:" || key == "job:<dynamic>" {
        return None;
    }
    Some(key)
}

struct CapabilityBuilder {
    key: String,
    entrypoint_ids: BTreeSet<String>,
    resource_ids: BTreeSet<String>,
    unit_ids: BTreeSet<String>,
    contract_paths: BTreeSet<String>,
}

impl CapabilityBuilder {
    fn new(key: &str) -> Self {
        Self {
            key: key.to_string(),
            entrypoint_ids: BTreeSet::new(),
            resource_ids: BTreeSet::new(),
            unit_ids: BTreeSet::new(),
            contract_paths: BTreeSet::new(),
        }
    }

    fn add_entrypoint(&mut self, entrypoint: &Entrypoint, unit_id: &str, include_unit: bool) {
        self.entrypoint_ids.insert(entrypoint.id.clone());
        if include_unit {
            self.unit_ids.insert(unit_id.to_string());
        }
        if let Some(path) = entrypoint.path.as_deref().and_then(normalize_contract_path) {
            self.contract_paths.insert(path);
        }
    }

    fn add_resource(&mut self, resource: &ResourceAccess) {
        self.resource_ids.insert(resource.id.clone());
        if let Some(path) = normalize_contract_path(&resource.name) {
            self.contract_paths.insert(path);
        }
    }

    fn finish(self) -> Capability {
        Capability {
            key: self.key,
            entrypoint_ids: self.entrypoint_ids.into_iter().collect(),
            resource_ids: self.resource_ids.into_iter().collect(),
            unit_ids: self.unit_ids.into_iter().collect(),
            contract_paths: self.contract_paths,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::build;
    use crate::config::PathPolicy;
    use crate::facts::{
        AccessMode, CodeUnit, CodeUnitKind, Entrypoint, EntrypointKind, FactStore, ResourceAccess,
        ResourceKind, SourceSpan,
    };
    use crate::model::Language;
    use std::collections::HashSet;

    fn file_unit(id: &str, relative_path: &str) -> CodeUnit {
        CodeUnit {
            id: id.into(),
            kind: CodeUnitKind::File,
            name: id.into(),
            qualified_name: id.into(),
            file_id: format!("file:{id}"),
            relative_path: relative_path.into(),
            language: Language::Python,
            parent_id: None,
            span: SourceSpan::new(format!("file:{id}"), relative_path, 1, 1, 1, 1),
            body_span: None,
            signature: None,
            parameters: Vec::new(),
            return_type: None,
            visibility: Default::default(),
            modifiers: Vec::new(),
            exported: true,
        }
    }

    fn http_entrypoint(id: &str, unit_id: &str, method: &str, path: &str) -> Entrypoint {
        Entrypoint {
            id: id.into(),
            unit_id: unit_id.into(),
            kind: EntrypointKind::Http,
            name: path.into(),
            method: Some(method.into()),
            path: Some(path.into()),
            framework_id: None,
            evidence: Vec::new(),
        }
    }

    fn rpc_entrypoint(id: &str, unit_id: &str, name: &str, path: Option<&str>) -> Entrypoint {
        Entrypoint {
            id: id.into(),
            unit_id: unit_id.into(),
            kind: EntrypointKind::Rpc,
            name: name.into(),
            method: Some("RPC".into()),
            path: path.map(str::to_string),
            framework_id: Some("dart.serverpod".into()),
            evidence: Vec::new(),
        }
    }

    fn class_unit(id: &str, name: &str, relative_path: &str, parent_id: Option<&str>) -> CodeUnit {
        CodeUnit {
            id: id.into(),
            kind: CodeUnitKind::Class,
            name: name.into(),
            qualified_name: name.into(),
            file_id: format!("file:{id}"),
            relative_path: relative_path.into(),
            language: Language::Dart,
            parent_id: parent_id.map(str::to_string),
            span: SourceSpan::new(format!("file:{id}"), relative_path, 1, 1, 1, 1),
            body_span: None,
            signature: Some(format!("class {name} extends Endpoint")),
            parameters: Vec::new(),
            return_type: None,
            visibility: Default::default(),
            modifiers: Vec::new(),
            exported: true,
        }
    }

    fn method_unit(id: &str, name: &str, parent_id: &str, relative_path: &str) -> CodeUnit {
        CodeUnit {
            id: id.into(),
            kind: CodeUnitKind::Method,
            name: name.into(),
            qualified_name: format!("{parent_id}::{name}"),
            file_id: format!("file:{id}"),
            relative_path: relative_path.into(),
            language: Language::Dart,
            parent_id: Some(parent_id.into()),
            span: SourceSpan::new(format!("file:{id}"), relative_path, 1, 1, 1, 1),
            body_span: None,
            signature: Some(format!("Future<void> {name}()")),
            parameters: Vec::new(),
            return_type: None,
            visibility: Default::default(),
            modifiers: Vec::new(),
            exported: true,
        }
    }

    fn fetch_resource(id: &str, unit_id: &str, name: &str) -> ResourceAccess {
        ResourceAccess {
            id: id.into(),
            unit_id: unit_id.into(),
            kind: ResourceKind::ExternalApi,
            name: name.into(),
            mode: AccessMode::Read,
            evidence: Vec::new(),
        }
    }

    #[test]
    fn 같은_라우터_함수의_서로_다른_경로는_각각_능력_키를_가진다() {
        let mut store = FactStore::default();
        store.units.insert(
            "unit:router".into(),
            file_unit("unit:router", "api/server.go"),
        );
        store.entrypoints.push(http_entrypoint(
            "entry:users",
            "unit:router",
            "POST",
            "/users",
        ));
        store.entrypoints.push(http_entrypoint(
            "entry:accounts",
            "unit:router",
            "GET",
            "/accounts",
        ));
        store.entrypoints.push(http_entrypoint(
            "entry:transfers",
            "unit:router",
            "POST",
            "/transfers",
        ));

        let capabilities = build(&store, &PathPolicy::default());
        let keys: HashSet<_> = capabilities.iter().map(|cap| cap.key.as_str()).collect();
        assert!(keys.contains("users"));
        assert!(keys.contains("accounts"));
        assert!(keys.contains("transfers"));
    }

    #[test]
    fn 스크립트_경로의_엔트리포인트는_능력_원자에서_제외된다() {
        let mut store = FactStore::default();
        store.units.insert(
            "unit:seed".into(),
            file_unit("unit:seed", "scripts/seed.py"),
        );
        store
            .entrypoints
            .push(http_entrypoint("entry:cli", "unit:seed", "POST", "/users"));

        let capabilities = build(&store, &PathPolicy::default());
        assert!(capabilities.is_empty());
    }

    #[test]
    fn 같은_계약_키로_서버_엔트리와_fetch_리소스를_묶는다() {
        let mut store = FactStore::default();
        store.units.insert(
            "unit:server".into(),
            file_unit("unit:server", "src/api/handler.ts"),
        );
        store.units.insert(
            "unit:client".into(),
            file_unit("unit:client", "web/client.ts"),
        );
        store.entrypoints.push(http_entrypoint(
            "entry:server",
            "unit:server",
            "POST",
            "/billing/invoices",
        ));
        store.resources.push(fetch_resource(
            "resource:fetch",
            "unit:client",
            "/billing/invoices",
        ));

        let capabilities = build(&store, &PathPolicy::default());
        assert_eq!(capabilities.len(), 1);
        assert_eq!(capabilities[0].key, "billing");
        assert_eq!(capabilities[0].entrypoint_ids.len(), 1);
        assert_eq!(capabilities[0].resource_ids.len(), 1);
        assert_eq!(capabilities[0].unit_ids.len(), 2);
    }

    #[test]
    fn 클라_유닛은_가장_많이_호출한_계약에만_소속된다() {
        let mut store = FactStore::default();
        store.units.insert(
            "unit:billing".into(),
            file_unit("unit:billing", "src/billing.py"),
        );
        store
            .units
            .insert("unit:auth".into(), file_unit("unit:auth", "src/auth.py"));
        store.units.insert(
            "unit:client".into(),
            file_unit("unit:client", "web/client.ts"),
        );
        store.entrypoints.push(http_entrypoint(
            "entry:billing",
            "unit:billing",
            "GET",
            "/billing/x",
        ));
        store.entrypoints.push(http_entrypoint(
            "entry:auth",
            "unit:auth",
            "GET",
            "/auth/me",
        ));
        for index in 0..3 {
            store.resources.push(fetch_resource(
                &format!("resource:billing-{index}"),
                "unit:client",
                "/billing/x",
            ));
        }
        store
            .resources
            .push(fetch_resource("resource:auth", "unit:client", "/auth/me"));

        let capabilities = build(&store, &PathPolicy::default());
        let billing = capabilities
            .iter()
            .find(|capability| capability.key == "billing")
            .expect("billing 계약이 있어야 한다");
        let auth = capabilities
            .iter()
            .find(|capability| capability.key == "auth")
            .expect("auth 계약이 있어야 한다");

        assert!(billing.unit_ids.contains(&"unit:client".to_string()));
        assert!(!auth.unit_ids.contains(&"unit:client".to_string()));
        assert_eq!(auth.resource_ids.len(), 1);
        assert_eq!(billing.resource_ids.len(), 3);
    }

    #[test]
    fn 같은_핸들러의_긴_경로_키가_짧은_마운트_키보다_우선한다() {
        let mut store = FactStore::default();
        store.units.insert(
            "unit:start".into(),
            file_unit("unit:start", "server/routes/start.py"),
        );
        store.entrypoints.push(http_entrypoint(
            "entry:mounted",
            "unit:start",
            "POST",
            "/api/v1/sessions/{session_id}/start",
        ));
        store.entrypoints.push(http_entrypoint(
            "entry:nested",
            "unit:start",
            "POST",
            "/{session_id}/start",
        ));

        let capabilities = build(&store, &PathPolicy::default());
        assert_eq!(capabilities.len(), 2);
        let keys: HashSet<_> = capabilities.iter().map(|cap| cap.key.as_str()).collect();
        assert!(keys.contains("sessions"));
        assert!(keys.contains("start"));
    }

    #[test]
    fn tauri_command는_rpc_능력_원자가_된다() {
        let mut store = FactStore::default();
        store.units.insert(
            "unit:analyze".into(),
            file_unit("unit:analyze", "src-tauri/src/analysis.rs"),
        );
        store.entrypoints.push(Entrypoint {
            id: "entry:analyze".into(),
            unit_id: "unit:analyze".into(),
            kind: EntrypointKind::Rpc,
            name: "analyze_project".into(),
            method: Some("TAURI_COMMAND".into()),
            path: None,
            framework_id: Some("rust.tauri".into()),
            evidence: Vec::new(),
        });

        let capabilities = build(&store, &PathPolicy::default());
        assert_eq!(capabilities.len(), 1);
        assert_eq!(capabilities[0].key, "analyze_project");
        assert_eq!(capabilities[0].entrypoint_ids, vec!["entry:analyze"]);
    }

    #[test]
    fn serverpod_endpoint_메서드는_클래스_키로_묶인다() {
        let mut store = FactStore::default();
        store.units.insert(
            "unit:class".into(),
            class_unit(
                "unit:class",
                "AuthenticationEndpoint",
                "server/lib/authentication_endpoint.dart",
                None,
            ),
        );
        store.units.insert(
            "unit:auth".into(),
            method_unit(
                "unit:auth",
                "authenticate",
                "unit:class",
                "server/lib/authentication_endpoint.dart",
            ),
        );
        store.units.insert(
            "unit:create".into(),
            method_unit(
                "unit:create",
                "createUser",
                "unit:class",
                "server/lib/authentication_endpoint.dart",
            ),
        );
        store.entrypoints.push(rpc_entrypoint(
            "entry:auth",
            "unit:auth",
            "authenticate",
            Some("/authenticate"),
        ));
        store.entrypoints.push(rpc_entrypoint(
            "entry:create",
            "unit:create",
            "createUser",
            Some("/createUser"),
        ));

        let capabilities = build(&store, &PathPolicy::default());
        assert_eq!(capabilities.len(), 1);
        assert_eq!(capabilities[0].key, "authentication");
        assert_eq!(capabilities[0].entrypoint_ids.len(), 2);
        assert!(capabilities[0].contract_paths.contains("/authenticate"));
        assert!(capabilities[0].contract_paths.contains("/createuser"));
    }
}
