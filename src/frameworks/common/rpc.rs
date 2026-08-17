//! 호출 기반 RPC 등록을 공통 RPC 진입점으로 materialize하는 도구.
//!
//! RPC 프레임워크는 생성 코드와 trait/interface 구현에 의존하는 경우가 많아
//! 서비스 구현체를 항상 정적으로 확정할 수 없다. 이 모듈은 확정 가능한 등록
//! 호출과 서비스 이름을 보존하고, 핸들러 유닛을 찾지 못하면 등록 호출이 있는
//! 유닛에 진입점을 귀속한다. 따라서 미해결 경계를 숨기지 않는다.

use crate::facts::{CodeUnitKind, Entrypoint, EntrypointKind, Evidence, FactStore};
use crate::frameworks::registry::detector::FrameworkDetection;
use crate::languages::common::metadata::stable_id;
use std::collections::{HashMap, HashSet};

use super::FrameworkApplicabilityIndex;

/// 하나의 RPC 프레임워크에서 서비스 등록 호출을 찾는 규칙이다.
#[derive(Debug, Clone)]
pub struct RpcRegistrationRule {
    pub framework_id: &'static str,
    /// 마지막 callee segment가 정확히 일치해야 하는 이름들이다.
    pub call_names: &'static [&'static str],
    /// 마지막 callee segment가 이 접두사와 접미사를 모두 가져야 한다.
    pub call_prefix: Option<&'static str>,
    pub call_suffix: Option<&'static str>,
    /// 서비스/구현체를 가리키는 인자 위치다.
    pub service_argument_index: Option<usize>,
    /// 서비스 이름을 등록 함수 이름에서 추출할지 여부다.
    pub service_name_from_callee: bool,
}

/// 감지된 프레임워크의 등록 호출을 RPC 진입점으로 추가한다.
pub fn add_rpc_registrations(
    facts: &mut FactStore,
    detections: &[FrameworkDetection],
    rule: RpcRegistrationRule,
) {
    if !detections
        .iter()
        .any(|detection| detection.id == rule.framework_id)
    {
        return;
    }
    let applicability = FrameworkApplicabilityIndex::new(detections);

    let mut existing_ids = facts
        .entrypoints
        .iter()
        .map(|entrypoint| entrypoint.id.clone())
        .collect::<HashSet<_>>();

    for call in facts.call_sites.clone() {
        let Some(source_unit) = facts.unit(&call.source_unit_id) else {
            continue;
        };
        if !applicability.applies(facts, &call.source_unit_id, rule.framework_id) {
            continue;
        }

        let call_name = last_segment(&call.callee);
        if !matches_call(call_name, &rule) {
            continue;
        }

        let service_name = if rule.service_name_from_callee {
            service_name_from_registration(call_name)
        } else {
            rule.service_argument_index
                .and_then(|index| call.arguments.get(index))
                .map(|argument| service_name_from_argument(argument))
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| "<dynamic>".to_string())
        };
        let target_unit_id = rule
            .service_argument_index
            .and_then(|index| call.arguments.get(index))
            .and_then(|argument| {
                resolve_service_unit(facts, source_unit.file_id.as_str(), argument)
            });

        let id = stable_id(
            "entry",
            &format!("{}:rpc:{}:{}", call.id, rule.framework_id, service_name),
        );
        if !existing_ids.insert(id.clone()) {
            continue;
        }

        let display_name = if service_name.is_empty() {
            "<dynamic>"
        } else {
            service_name.as_str()
        };
        let evidence = call
            .evidence
            .first()
            .cloned()
            .map(|mut evidence| {
                evidence.kind = "rpcRegistration".to_string();
                evidence.value = display_name.to_string();
                evidence
            })
            .into_iter()
            .collect();

        facts.entrypoints.push(Entrypoint {
            id,
            unit_id: target_unit_id.unwrap_or_else(|| call.source_unit_id.clone()),
            kind: EntrypointKind::Rpc,
            name: display_name.to_string(),
            method: Some("RPC".to_string()),
            path: None,
            framework_id: Some(rule.framework_id.to_string()),
            evidence,
        });
    }
}

/// Serverpod처럼 특정 base class의 메서드 자체가 RPC 계약이 되는 프레임워크를
/// 정적 RPC 진입점으로 변환한다.
pub fn add_endpoint_methods(
    facts: &mut FactStore,
    detections: &[FrameworkDetection],
    framework_id: &str,
    base_type: &str,
) {
    if !detections
        .iter()
        .any(|detection| detection.id == framework_id)
    {
        return;
    }
    let applicability = FrameworkApplicabilityIndex::new(detections);
    let endpoint_classes = facts
        .units
        .values()
        .filter(|unit| {
            unit.kind == CodeUnitKind::Class
                && unit
                    .signature
                    .as_deref()
                    .is_some_and(|signature| signature.contains(base_type))
                && applicability.applies(facts, &unit.id, framework_id)
        })
        .map(|unit| unit.id.clone())
        .collect::<Vec<_>>();

    let mut methods_by_parent: HashMap<String, Vec<_>> = HashMap::new();
    for unit in facts.units.values().filter(|unit| {
        matches!(unit.kind, CodeUnitKind::Method | CodeUnitKind::Function)
            && !unit.name.starts_with('_')
    }) {
        if let Some(parent_id) = unit.parent_id.as_deref() {
            methods_by_parent
                .entry(parent_id.to_string())
                .or_default()
                .push(unit.clone());
        }
    }

    for class_id in endpoint_classes {
        let Some(class_unit) = facts.unit(&class_id) else {
            continue;
        };
        if !class_unit.name.ends_with("Endpoint") {
            continue;
        }
        let methods = methods_by_parent
            .get(&class_id)
            .cloned()
            .unwrap_or_default();
        for method in methods {
            if !is_server_endpoint_method(&method) {
                continue;
            }
            let rpc_path = format!("/{}", method.name);
            let id = stable_id(
                "entry",
                &format!("endpoint:{}:{}:{}", framework_id, method.id, method.name),
            );
            if facts.entrypoints.iter().any(|entrypoint| {
                entrypoint.id == id
                    || (entrypoint.framework_id.as_deref() == Some(framework_id)
                        && entrypoint.unit_id == method.id
                        && entrypoint.kind == EntrypointKind::Rpc)
            }) {
                continue;
            }
            facts.entrypoints.push(Entrypoint {
                id,
                unit_id: method.id.clone(),
                kind: EntrypointKind::Rpc,
                name: rpc_path.clone(),
                method: Some("RPC".to_string()),
                path: Some(rpc_path),
                framework_id: Some(framework_id.to_string()),
                evidence: vec![Evidence::new(
                    "endpointMethod",
                    method.name,
                    method.span.clone(),
                )],
            });
        }
    }
}

fn is_server_endpoint_method(method: &crate::facts::CodeUnit) -> bool {
    let path = method.relative_path.replace('\\', "/").to_ascii_lowercase();
    if path.contains("_client/")
        || path.ends_with("/client.dart")
        || path.contains("/protocol/client.")
    {
        return false;
    }
    true
}

fn matches_call(call_name: &str, rule: &RpcRegistrationRule) -> bool {
    if rule
        .call_names
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(call_name))
    {
        return true;
    }
    let prefix_matches = rule
        .call_prefix
        .is_none_or(|prefix| call_name.starts_with(prefix));
    let suffix_matches = rule
        .call_suffix
        .is_none_or(|suffix| call_name.ends_with(suffix));
    (rule.call_prefix.is_some() || rule.call_suffix.is_some()) && prefix_matches && suffix_matches
}

fn last_segment(callee: &str) -> &str {
    callee
        .rsplit_once('.')
        .map(|(_, value)| value)
        .or_else(|| callee.rsplit_once("::").map(|(_, value)| value))
        .or_else(|| callee.rsplit_once("->").map(|(_, value)| value))
        .unwrap_or(callee)
}

fn service_name_from_registration(call_name: &str) -> String {
    call_name
        .strip_prefix("Register")
        .unwrap_or(call_name)
        .strip_suffix("Server")
        .unwrap_or(call_name.strip_prefix("Register").unwrap_or(call_name))
        .to_string()
}

fn service_name_from_argument(argument: &str) -> String {
    let mut value = argument.trim();
    while let Some(stripped) = value.strip_prefix('&') {
        value = stripped.trim_start();
    }
    if let Some(stripped) = value.strip_prefix("new ") {
        value = stripped.trim_start();
    }

    if let Some((constructor, _)) = value.split_once("::new") {
        value = constructor;
    } else if let Some((constructor, _)) = value.split_once(".new") {
        value = constructor;
    }
    if let Some((constructor, _)) = value.split_once('(') {
        value = constructor;
    }

    let value = value
        .trim_matches(|character: char| {
            matches!(
                character,
                '&' | '*' | '(' | ')' | '<' | '>' | '{' | '}' | ';' | ','
            )
        })
        .trim();
    let value = value
        .rsplit_once("::")
        .map(|(_, name)| name)
        .or_else(|| value.rsplit_once('.').map(|(_, name)| name))
        .unwrap_or(value)
        .trim();
    strip_service_suffix(value).to_string()
}

fn strip_service_suffix(value: &str) -> &str {
    ["Server", "Service", "Svc"]
        .iter()
        .find_map(|suffix| value.strip_suffix(suffix).filter(|name| !name.is_empty()))
        .unwrap_or(value)
}

fn resolve_service_unit(facts: &FactStore, source_file_id: &str, argument: &str) -> Option<String> {
    let name = service_name_from_argument(argument);
    if name.is_empty() || name == "<dynamic>" {
        return None;
    }

    let same_file = facts
        .units
        .values()
        .filter(|unit| unit.file_id == source_file_id)
        .filter(|unit| unit.name == name || strip_service_suffix(&unit.name) == name)
        .map(|unit| unit.id.clone())
        .collect::<Vec<_>>();
    if same_file.len() == 1 {
        return same_file.into_iter().next();
    }

    let global = facts
        .units
        .values()
        .filter(|unit| unit.name == name || strip_service_suffix(&unit.name) == name)
        .map(|unit| unit.id.clone())
        .collect::<Vec<_>>();
    (global.len() == 1).then(|| global[0].clone())
}

#[cfg(test)]
mod tests {
    use super::add_endpoint_methods;
    use crate::facts::{
        CodeUnit, CodeUnitKind, EntrypointKind, Evidence, FactStore, SourceSpan,
    };
    use crate::frameworks::registry::capabilities::FrameworkKind;
    use crate::frameworks::registry::detector::FrameworkDetection;
    use crate::model::Language;

    fn detection(file_id: &str) -> Vec<FrameworkDetection> {
        vec![FrameworkDetection {
            id: "dart.serverpod".into(),
            display_name: "Serverpod".into(),
            kind: FrameworkKind::Web,
            languages: vec!["dart".into()],
            capabilities: vec![],
            parent: None,
            confidence: 1.0,
            evidence: vec![Evidence::new(
                "marker",
                "package:serverpod",
                SourceSpan::new(file_id, file_id, 1, 1, 1, 1),
            )],
        }]
    }

    fn endpoint_class(id: &str, path: &str) -> CodeUnit {
        CodeUnit {
            id: id.into(),
            kind: CodeUnitKind::Class,
            name: "GreetingEndpoint".into(),
            qualified_name: "GreetingEndpoint".into(),
            file_id: format!("file:{id}"),
            relative_path: path.into(),
            language: Language::Dart,
            parent_id: None,
            span: SourceSpan::new(format!("file:{id}"), path, 1, 1, 1, 1),
            body_span: None,
            signature: Some("class GreetingEndpoint extends Endpoint".into()),
            parameters: Vec::new(),
            return_type: None,
            visibility: Default::default(),
            modifiers: Vec::new(),
            exported: true,
        }
    }

    fn endpoint_method(id: &str, parent_id: &str, path: &str) -> CodeUnit {
        CodeUnit {
            id: id.into(),
            kind: CodeUnitKind::Method,
            name: "hello".into(),
            qualified_name: format!("{parent_id}::hello"),
            file_id: format!("file:{id}"),
            relative_path: path.into(),
            language: Language::Dart,
            parent_id: Some(parent_id.into()),
            span: SourceSpan::new(format!("file:{id}"), path, 1, 1, 1, 1),
            body_span: None,
            signature: Some("Future<void> hello()".into()),
            parameters: Vec::new(),
            return_type: None,
            visibility: Default::default(),
            modifiers: Vec::new(),
            exported: true,
        }
    }

    #[test]
    fn endpoint_메서드는_rpc_경로를_가진다() {
        let mut facts = FactStore::default();
        facts
            .units
            .insert("class:greeting".into(), endpoint_class("class:greeting", "server/lib/greeting_endpoint.dart"));
        facts.units.insert(
            "method:hello".into(),
            endpoint_method("method:hello", "class:greeting", "server/lib/greeting_endpoint.dart"),
        );

        add_endpoint_methods(&mut facts, &detection("file:class:greeting"), "dart.serverpod", "Endpoint");

        assert_eq!(facts.entrypoints.len(), 1);
        assert_eq!(facts.entrypoints[0].kind, EntrypointKind::Rpc);
        assert_eq!(facts.entrypoints[0].path.as_deref(), Some("/hello"));
        assert_eq!(facts.entrypoints[0].method.as_deref(), Some("RPC"));
    }

    #[test]
    fn client_stub_메서드는_rpc_진입점에서_제외된다() {
        let mut facts = FactStore::default();
        facts.units.insert(
            "class:client".into(),
            endpoint_class("class:client", "lib/src/protocol/client.dart"),
        );
        facts.units.insert(
            "method:hello".into(),
            endpoint_method("method:hello", "class:client", "lib/src/protocol/client.dart"),
        );

        add_endpoint_methods(&mut facts, &detection("file:class:greeting"), "dart.serverpod", "Endpoint");

        assert!(facts.entrypoints.is_empty());
    }
}
