//! Python 호출 사실을 framework entrypoint로 materialize하는 공통 도구다.

use super::arguments::{join_paths, route_path, string_literal};
use crate::facts::{Entrypoint, EntrypointKind, FactStore};
use crate::languages::common::metadata::stable_id;
use std::collections::HashSet;

pub(crate) fn source_file_id<'a>(facts: &'a FactStore, unit_id: &str) -> Option<&'a str> {
    facts.unit(unit_id).map(|unit| unit.file_id.as_str())
}

/// 호출 인자의 handler/view 표현을 같은 파일 또는 프로젝트 유닛으로 해석한다.
pub(crate) fn resolve_target_unit(
    facts: &FactStore,
    source_file_id: &str,
    expression: Option<&str>,
) -> Option<String> {
    let expression = expression?.trim();
    let expression = string_literal(expression).unwrap_or_else(|| expression.to_string());
    if expression.is_empty() || expression.starts_with("include(") {
        return None;
    }
    let local_name = expression
        .rsplit_once('.')
        .map(|(_, value)| value)
        .or_else(|| expression.rsplit_once("::").map(|(_, value)| value))
        .unwrap_or(&expression)
        .trim();

    let same_file = facts
        .units
        .values()
        .filter(|unit| unit.file_id == source_file_id)
        .filter(|unit| unit.name == local_name || unit.qualified_name.ends_with(local_name))
        .map(|unit| unit.id.clone())
        .collect::<Vec<_>>();
    if same_file.len() == 1 {
        return same_file.into_iter().next();
    }

    let imported_targets = facts
        .bindings
        .iter()
        .filter(|binding| binding.local_name == expression || binding.local_name == local_name)
        .map(|binding| {
            binding
                .target_name
                .rsplit('.')
                .next()
                .unwrap_or(&binding.target_name)
        })
        .collect::<HashSet<_>>();
    let global = facts
        .units
        .values()
        .filter(|unit| {
            unit.name == local_name
                || imported_targets
                    .iter()
                    .any(|target| *target == unit.name.as_str())
        })
        .map(|unit| unit.id.clone())
        .collect::<Vec<_>>();
    (global.len() == 1).then(|| global[0].clone())
}

/// `Route(...)`, `path(...)`, `router.register(...)`처럼 호출 사실로 표현되는
/// entrypoint의 materialize 규칙이다.
pub(crate) struct CallEntrypointSpec<'a> {
    pub(crate) framework_id: &'a str,
    pub(crate) kind: EntrypointKind,
    pub(crate) method: &'a str,
    pub(crate) path_argument_index: usize,
    pub(crate) target_argument_index: Option<usize>,
    pub(crate) evidence_kind: &'a str,
}

/// 호출 사실을 공통 entrypoint로 추가한다. 경로가 동적이면 `path`를 `None`으로
/// 남기고 `<dynamic>`이라는 표시만 붙여 누락과 정적 확정을 구분한다.
pub(crate) fn add_call_entrypoint(
    facts: &mut FactStore,
    call_index: usize,
    spec: CallEntrypointSpec<'_>,
) {
    let Some(call) = facts.call_sites.get(call_index).cloned() else {
        return;
    };
    let Some(source_unit) = facts.unit(&call.source_unit_id) else {
        return;
    };
    let path = call
        .arguments
        .get(spec.path_argument_index)
        .and_then(|argument| string_literal(argument))
        .or_else(|| route_path(&call.arguments));
    let path = path.map(|path| join_paths("", &path));
    let display_path = path.clone().unwrap_or_else(|| "<dynamic>".to_string());
    let unit_id = spec
        .target_argument_index
        .and_then(|index| {
            resolve_target_unit(
                facts,
                source_unit.file_id.as_str(),
                call.arguments.get(index).map(String::as_str),
            )
        })
        .unwrap_or_else(|| call.source_unit_id.clone());
    let id = stable_id(
        "entry",
        &format!(
            "{}:{}:{}:{}",
            call.id, spec.method, display_path, spec.framework_id
        ),
    );
    if facts.entrypoints.iter().any(|entry| entry.id == id) {
        return;
    }
    facts.entrypoints.push(Entrypoint {
        id,
        unit_id,
        kind: spec.kind,
        name: display_path.clone(),
        method: Some(spec.method.to_string()),
        path,
        framework_id: Some(spec.framework_id.to_string()),
        evidence: call
            .evidence
            .into_iter()
            .map(|mut evidence| {
                evidence.kind = spec.evidence_kind.to_string();
                evidence.value = display_path.clone();
                evidence
            })
            .collect(),
    });
}
