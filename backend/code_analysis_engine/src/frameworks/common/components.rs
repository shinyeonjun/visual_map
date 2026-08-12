//! UI 프레임워크가 제공하는 컴포넌트 경계를 CodeUnit modifier로 보존한다.

use crate::facts::{CodeUnitKind, FactStore};
use crate::frameworks::registry::detector::FrameworkDetection;
use std::collections::HashSet;

use super::FrameworkApplicabilityIndex;

#[derive(Debug, Clone)]
pub struct ComponentMarkerRule {
    pub framework_id: &'static str,
    pub modifier: &'static str,
    pub decorator_names: &'static [&'static str],
    pub call_names: &'static [&'static str],
    pub signature_tokens: &'static [&'static str],
}

/// decorator·호출·상속 시그니처 중 실제로 존재하는 근거만 사용해 컴포넌트
/// marker를 추가한다. 분석 모델에 없는 새 UI 전용 유닛 종류를 만들지 않고
/// 기존 CodeUnit 상세 정보와 호환된다.
pub fn mark_components(
    facts: &mut FactStore,
    detections: &[FrameworkDetection],
    rule: ComponentMarkerRule,
) {
    if !detections
        .iter()
        .any(|detection| detection.id == rule.framework_id)
    {
        return;
    }
    let applicability = FrameworkApplicabilityIndex::new(detections);
    let mut unit_ids = HashSet::new();
    for decorator in &facts.decorators {
        if !rule
            .decorator_names
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&decorator.name))
        {
            continue;
        }
        if applicability.applies(facts, &decorator.unit_id, rule.framework_id) {
            unit_ids.insert(decorator.unit_id.clone());
        }
    }

    if !rule.call_names.is_empty() {
        for call in &facts.call_sites {
            let call_name = call
                .callee
                .rsplit_once('.')
                .map(|(_, name)| name)
                .or_else(|| call.callee.rsplit_once("::").map(|(_, name)| name))
                .unwrap_or(&call.callee);
            if !rule
                .call_names
                .iter()
                .any(|name| name.eq_ignore_ascii_case(call_name))
            {
                continue;
            }
            if applicability.applies(facts, &call.source_unit_id, rule.framework_id) {
                unit_ids.insert(call.source_unit_id.clone());
            }
        }
    }

    if !rule.signature_tokens.is_empty() {
        for unit in facts.units.values() {
            if !matches!(
                unit.kind,
                CodeUnitKind::Class
                    | CodeUnitKind::Struct
                    | CodeUnitKind::Record
                    | CodeUnitKind::Interface
            ) || !applicability.applies(facts, &unit.id, rule.framework_id)
            {
                continue;
            }
            let haystack = format!(
                "{} {}",
                unit.name,
                unit.signature.as_deref().unwrap_or_default()
            );
            if rule.signature_tokens.iter().any(|token| {
                haystack
                    .to_ascii_lowercase()
                    .contains(&token.to_ascii_lowercase())
            }) {
                unit_ids.insert(unit.id.clone());
            }
        }
    }

    for unit_id in unit_ids {
        let Some(unit) = facts.units.get_mut(&unit_id) else {
            continue;
        };
        if !unit
            .modifiers
            .iter()
            .any(|modifier| modifier == rule.modifier)
        {
            unit.modifiers.push(rule.modifier.to_string());
            unit.modifiers.sort();
        }
    }
}
