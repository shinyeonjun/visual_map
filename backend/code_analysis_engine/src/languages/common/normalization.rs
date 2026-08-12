use crate::facts::{CodeUnitKind, FactBundle, ReferenceKind, ResolutionStatus};
use crate::model::Language;
use std::collections::HashMap;

use super::references::matches_dynamic_pattern;

/// 모든 언어 분석 결과에 공통으로 적용하는 Facts 정규화 단계다.
pub fn normalize_bundle(bundle: &mut FactBundle) {
    normalize_language_units(bundle);
    bundle.units.sort_by(|left, right| left.id.cmp(&right.id));
    bundle.units.dedup_by(|left, right| left.id == right.id);
    bundle
        .references
        .sort_by(|left, right| left.id.cmp(&right.id));
    bundle
        .references
        .dedup_by(|left, right| left.id == right.id);
    bundle
        .entrypoints
        .sort_by(|left, right| left.id.cmp(&right.id));
    bundle
        .entrypoints
        .dedup_by(|left, right| left.id == right.id);
    bundle
        .resources
        .sort_by(|left, right| left.id.cmp(&right.id));
    bundle.resources.dedup_by(|left, right| left.id == right.id);
}

/// 공통 walker가 언어별 receiver·signature 정보를 잃지 않도록, AST를 다시
/// 파싱하지 않고도 확정 가능한 선언 종류만 보정한다. 문법 자체가 필요한
/// 분류는 각 language adapter가 담당하고 여기서는 보수적인 후처리만 한다.
fn normalize_language_units(bundle: &mut FactBundle) {
    let parent_names: HashMap<_, _> = bundle
        .units
        .iter()
        .map(|unit| (unit.id.clone(), unit.name.clone()))
        .collect();
    let property_units: std::collections::HashSet<_> = bundle
        .decorators
        .iter()
        .filter(|decorator| decorator.name.eq_ignore_ascii_case("property"))
        .map(|decorator| decorator.unit_id.clone())
        .collect();

    for unit in &mut bundle.units {
        let signature = unit.signature.as_deref().unwrap_or_default();
        match unit.language {
            Language::Go => {
                if unit.kind == CodeUnitKind::Function
                    && signature.trim_start().starts_with("func (")
                {
                    unit.kind = CodeUnitKind::Method;
                } else if unit.kind == CodeUnitKind::Record {
                    if signature.to_ascii_lowercase().contains("interface") {
                        unit.kind = CodeUnitKind::Interface;
                    } else if signature.to_ascii_lowercase().contains("struct") {
                        unit.kind = CodeUnitKind::Struct;
                    }
                }
            }
            Language::Cpp | Language::Dart => {
                let parent_name = unit
                    .parent_id
                    .as_ref()
                    .and_then(|parent| parent_names.get(parent));
                if matches!(unit.kind, CodeUnitKind::Function | CodeUnitKind::Method)
                    && parent_name.is_some_and(|name| name == &unit.name)
                {
                    unit.kind = CodeUnitKind::Constructor;
                }
            }
            Language::Python if property_units.contains(&unit.id) => {
                if matches!(unit.kind, CodeUnitKind::Function | CodeUnitKind::Method) {
                    unit.kind = CodeUnitKind::Property;
                }
            }
            _ => {}
        }
    }
}

/// 언어별 리플렉션·문자열 디스패치 패턴을 동적 경계로 표시한다.
pub fn mark_dynamic_calls(bundle: &mut FactBundle, patterns: &[String]) {
    let normalized_patterns: Vec<String> = patterns
        .iter()
        .map(|pattern| pattern.to_ascii_lowercase())
        .collect();
    for reference in &mut bundle.references {
        if reference.kind != ReferenceKind::Call {
            continue;
        }
        let target = reference.target_name.to_ascii_lowercase();
        if normalized_patterns
            .iter()
            .any(|pattern| matches_dynamic_pattern(&target, pattern))
        {
            reference.status = ResolutionStatus::Dynamic;
        }
    }
}
