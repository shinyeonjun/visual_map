//! 상속 기반 프레임워크 계약을 공통 entrypoint로 변환한다.
//!
//! C++·Dart·Java처럼 decorator가 아니라 기반 클래스의 virtual/override
//! 메서드가 외부 경계를 정의하는 프레임워크를 위한 보조 계층이다. 상속
//! 이름이 실제 코드 유닛 시그니처에 있을 때만 적용하고, 구현 메서드를
//! 찾지 못하면 entrypoint를 추정하지 않는다.

use crate::facts::{CodeUnitKind, Entrypoint, EntrypointKind, Evidence, FactStore};
use crate::frameworks::registry::detector::FrameworkDetection;
use crate::languages::common::metadata::stable_id;

use super::FrameworkApplicabilityIndex;

#[derive(Debug, Clone)]
pub struct InheritedMethodEntrypointRule {
    pub framework_id: &'static str,
    pub base_type_tokens: &'static [&'static str],
    pub method_names: &'static [&'static str],
    pub kind: EntrypointKind,
    pub method: &'static str,
}

/// 기반 클래스와 그 직접 구현 메서드를 외부 경계로 materialize한다.
pub fn add_inherited_method_entrypoints(
    facts: &mut FactStore,
    detections: &[FrameworkDetection],
    rule: InheritedMethodEntrypointRule,
) {
    if !detections
        .iter()
        .any(|detection| detection.id == rule.framework_id)
    {
        return;
    }

    let applicability = FrameworkApplicabilityIndex::new(detections);
    let base_ids = facts
        .units
        .values()
        .filter(|unit| {
            matches!(
                unit.kind,
                CodeUnitKind::Class | CodeUnitKind::Struct | CodeUnitKind::Interface
            ) && applicability.applies(facts, &unit.id, rule.framework_id)
        })
        .filter(|unit| {
            let signature = unit.signature.as_deref().unwrap_or_default();
            rule.base_type_tokens.iter().any(|token| {
                signature
                    .to_ascii_lowercase()
                    .contains(&token.to_ascii_lowercase())
            })
        })
        .map(|unit| unit.id.clone())
        .collect::<Vec<_>>();

    let methods = facts
        .units
        .values()
        .filter(|unit| {
            matches!(unit.kind, CodeUnitKind::Method | CodeUnitKind::Function)
                && rule
                    .method_names
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(&unit.name))
                && unit.parent_id.is_some()
        })
        .cloned()
        .collect::<Vec<_>>();

    for base_id in base_ids {
        for method in methods
            .iter()
            .filter(|method| method.parent_id.as_deref() == Some(base_id.as_str()))
        {
            let id = stable_id(
                "entry",
                &format!(
                    "inherited:{}:{}:{}",
                    rule.framework_id, method.id, rule.method
                ),
            );
            if facts.entrypoints.iter().any(|entrypoint| {
                entrypoint.id == id
                    || (entrypoint.framework_id.as_deref() == Some(rule.framework_id)
                        && entrypoint.unit_id == method.id
                        && entrypoint.kind == rule.kind)
            }) {
                continue;
            }
            facts.entrypoints.push(Entrypoint {
                id,
                unit_id: method.id.clone(),
                kind: rule.kind.clone(),
                name: method.name.clone(),
                method: Some(rule.method.to_string()),
                path: None,
                framework_id: Some(rule.framework_id.to_string()),
                evidence: vec![Evidence::new(
                    "inheritedMethod",
                    method.name.clone(),
                    method.span.clone(),
                )],
            });
        }
    }
}
