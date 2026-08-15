//! HTTP 계약 동치로 entrypoint를 기능 하나로 묶는다.

use crate::domain::contract_path::{
    contract_identity, handler_mount_suffix_match, mounted_path_suffix_match,
    normalize_contract_path,
};
use crate::facts::Entrypoint;
use std::collections::BTreeMap;

/// 같은 바깥 계약을 가리키는 entrypoint 묶음이다.
pub(crate) struct ContractEntrypointGroup<'a> {
    pub entrypoints: Vec<&'a Entrypoint>,
}

/// 정규화된 경로와 호환 가능한 메서드로 entrypoint를 묶는다.
pub(crate) fn group_entrypoints_by_contract<'a>(
    entrypoints: impl IntoIterator<Item = &'a Entrypoint>,
) -> Vec<ContractEntrypointGroup<'a>> {
    let entrypoints: Vec<&'a Entrypoint> = entrypoints.into_iter().collect();
    if entrypoints.is_empty() {
        return Vec::new();
    }

    let mut parent: Vec<usize> = (0..entrypoints.len()).collect();
    for left in 0..entrypoints.len() {
        for right in (left + 1)..entrypoints.len() {
            if same_contract(entrypoints[left], entrypoints[right]) {
                union(&mut parent, left, right);
            }
        }
    }

    let mut groups: BTreeMap<usize, Vec<&'a Entrypoint>> = BTreeMap::new();
    for index in 0..entrypoints.len() {
        groups
            .entry(find(&mut parent, index))
            .or_default()
            .push(entrypoints[index]);
    }

    groups
        .into_values()
        .map(|entrypoints| ContractEntrypointGroup { entrypoints })
        .collect()
}

pub(crate) fn contract_feature_key(entrypoint: &Entrypoint) -> String {
    let raw = entrypoint.path.as_deref().unwrap_or(&entrypoint.name);
    contract_identity(entrypoint.method.as_deref(), raw)
        .map(|identity| format!("contract:{identity}"))
        .unwrap_or_else(|| format!("entrypoint:{}", entrypoint.id))
}

fn same_contract(left: &Entrypoint, right: &Entrypoint) -> bool {
    if !methods_compatible(left.method.as_deref(), right.method.as_deref()) {
        return false;
    }

    let left_path_raw = entrypoint_path(left);
    let right_path_raw = entrypoint_path(right);
    let left_identity = contract_identity(left.method.as_deref(), left_path_raw);
    let right_identity = contract_identity(right.method.as_deref(), right_path_raw);
    if let (Some(left), Some(right)) = (&left_identity, &right_identity) {
        if left == right {
            return true;
        }
    }

    let left_normalized = normalize_contract_path(left_path_raw);
    let right_normalized = normalize_contract_path(right_path_raw);
    match (&left_normalized, &right_normalized) {
        (Some(left_path), Some(right_path)) => {
            if left_path == right_path {
                return true;
            }
            if left.unit_id == right.unit_id
                && (mounted_path_suffix_match(left_path, right_path)
                    || handler_mount_suffix_match(left_path, right_path))
            {
                return true;
            }
        }
        (Some(_), None) | (None, Some(_)) => {
            if left.unit_id == right.unit_id {
                return true;
            }
        }
        (None, None) => {}
    }

    false
}

fn entrypoint_path(entrypoint: &Entrypoint) -> &str {
    entrypoint.path.as_deref().unwrap_or(&entrypoint.name)
}

fn methods_compatible(left: Option<&str>, right: Option<&str>) -> bool {
    let left = normalize_method(left);
    let right = normalize_method(right);
    left == right || left == "*" || right == "*"
}

fn normalize_method(method: Option<&str>) -> String {
    method
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_uppercase())
        .unwrap_or_else(|| "*".to_string())
}

fn find(parent: &mut [usize], index: usize) -> usize {
    if parent[index] != index {
        let root = find(parent, parent[index]);
        parent[index] = root;
    }
    parent[index]
}

fn union(parent: &mut [usize], left: usize, right: usize) {
    let left_root = find(parent, left);
    let right_root = find(parent, right);
    if left_root != right_root {
        parent[right_root] = left_root;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{EntrypointKind, Evidence};

    fn entrypoint(id: &str, method: Option<&str>, path: &str) -> Entrypoint {
        entrypoint_with_unit(id, method, path, format!("unit-{id}"))
    }

    fn entrypoint_with_unit(
        id: &str,
        method: Option<&str>,
        path: &str,
        unit_id: String,
    ) -> Entrypoint {
        Entrypoint {
            id: id.into(),
            name: id.into(),
            kind: EntrypointKind::Http,
            method: method.map(str::to_string),
            path: Some(path.into()),
            unit_id,
            framework_id: None,
            evidence: Vec::<Evidence>::new(),
        }
    }

    #[test]
    fn 짧은_경로와_api_접두_경로는_같은_계약이다() {
        let short = entrypoint("ep-short", Some("GET"), "/health");
        let long = entrypoint("ep-long", Some("GET"), "/api/v1/health");
        assert!(same_contract(&short, &long));
    }

    #[test]
    fn 메서드가_없는_경로는_명시_메서드와_같은_계약으로_묶인다() {
        let wildcard = entrypoint("ep-wildcard", None, "/health");
        let explicit = entrypoint("ep-get", Some("GET"), "/health");
        assert!(same_contract(&wildcard, &explicit));
    }

    #[test]
    fn 다른_http_메서드는_같은_계약이_아니다() {
        let get = entrypoint("ep-get", Some("GET"), "/users");
        let post = entrypoint("ep-post", Some("POST"), "/users");
        assert!(!same_contract(&get, &post));
    }

    #[test]
    fn 동치_묶음이_하나의_그룹이_된다() {
        let entrypoints = vec![
            entrypoint("ep-short", Some("GET"), "/health"),
            entrypoint("ep-long", Some("GET"), "/api/v1/health"),
            entrypoint("ep-users", Some("POST"), "/users"),
        ];
        let groups = group_entrypoints_by_contract(entrypoints.iter());
        let mut sizes: Vec<_> = groups.iter().map(|group| group.entrypoints.len()).collect();
        sizes.sort_unstable();
        assert_eq!(sizes, vec![1, 2]);
    }

    #[test]
    fn 해석_불가_경로끼리는_같은_계약이_아니다() {
        let root = entrypoint("ep-root", Some("GET"), "/");
        let session = entrypoint("ep-session", Some("GET"), "/{session_id}");
        assert!(!same_contract(&root, &session));
    }

    #[test]
    fn 같은_핸들러의_마운트_경로는_같은_계약이다() {
        let mounted = entrypoint_with_unit(
            "ep-mounted",
            Some("POST"),
            "/api/v1/sessions/{session_id}/start",
            "unit-start".into(),
        );
        let nested = entrypoint_with_unit(
            "ep-nested",
            Some("POST"),
            "/{session_id}/start",
            "unit-start".into(),
        );
        assert!(same_contract(&mounted, &nested));
    }

    #[test]
    fn 다른_핸들러의_접미_유사_경로는_같은_계약이_아니다() {
        let admin = entrypoint("ep-admin", Some("GET"), "/admin/users");
        let users = entrypoint("ep-users", Some("GET"), "/users");
        assert!(!same_contract(&admin, &users));
    }

    #[test]
    fn 같은_핸들러의_정적_마운트_경로는_같은_계약이다() {
        let mounted = entrypoint_with_unit(
            "ep-mounted",
            Some("GET"),
            "/api/v1/history/timeline",
            "unit-history".into(),
        );
        let nested = entrypoint_with_unit(
            "ep-nested",
            Some("GET"),
            "/timeline",
            "unit-history".into(),
        );
        assert!(same_contract(&mounted, &nested));
    }

    #[test]
    fn 해석_불가_경로는_같은_핸들러의_파싱_가능_경로에_붙는다() {
        let full = entrypoint_with_unit(
            "ep-full",
            Some("GET"),
            "/api/v1/sessions/{session_id}",
            "unit-session".into(),
        );
        let mount = entrypoint_with_unit(
            "ep-mount",
            Some("GET"),
            "/{session_id}",
            "unit-session".into(),
        );
        assert!(same_contract(&full, &mount));
    }
}
