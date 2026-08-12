use crate::facts::{FactBundle, ReferenceKind, ResolutionStatus};

use super::references::matches_dynamic_pattern;

/// 모든 언어 분석 결과에 공통으로 적용하는 Facts 정규화 단계다.
pub fn normalize_bundle(bundle: &mut FactBundle) {
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
