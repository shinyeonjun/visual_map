//! Codex 컨텍스트에 포함할 프로젝트 규모와 전역 요약을 계산한다.

use super::domains::DomainPlan;
use super::indexes::PostprocessIndexes;
use super::model::{ContextProjectProfile, GlobalContextSummary};
use crate::facts::ResolutionStatus;
use std::collections::BTreeSet;

pub(super) fn project_profile(indexes: &PostprocessIndexes<'_>) -> ContextProjectProfile {
    let visible_units = indexes.visible_unit_ids.len();
    let entrypoints = indexes.visible_entrypoint_ids.len();
    let resources = indexes.visible_resource_ids.len();
    let references = indexes
        .overview
        .static_graph
        .edges
        .iter()
        .filter(|edge| indexes.visible_unit_ids.contains(&edge.source_unit_id))
        .count();
    let confirmed = indexes
        .overview
        .static_graph
        .edges
        .iter()
        .filter(|edge| {
            indexes.visible_unit_ids.contains(&edge.source_unit_id)
                && edge.status == ResolutionStatus::Confirmed
        })
        .count();
    let max_domain_units = indexes
        .overview
        .domains
        .iter()
        .map(|domain| {
            domain
                .primary_unit_ids
                .iter()
                .chain(&domain.shared_unit_ids)
                .filter(|id| indexes.visible_unit_ids.contains(*id))
                .count()
        })
        .max()
        .unwrap_or(0);
    ContextProjectProfile {
        visible_unit_count: visible_units,
        entrypoint_count: entrypoints,
        resource_count: resources,
        reference_count: references,
        confirmed_reference_count: confirmed,
        entrypoint_density: ratio(entrypoints, visible_units),
        resource_density: ratio(resources, visible_units),
        reference_resolution: ratio(confirmed, references),
        max_domain_unit_ratio: ratio(max_domain_units, visible_units),
    }
}

pub(super) fn global_summary(
    indexes: &PostprocessIndexes<'_>,
    plan: &DomainPlan,
) -> GlobalContextSummary {
    let mut domains = plan
        .clusters
        .iter()
        .filter_map(|cluster| indexes.domains.get(cluster.representative_id.as_str()))
        .collect::<Vec<_>>();
    domains.sort_by(|left, right| left.id.cmp(&right.id));
    let mut languages = indexes
        .overview
        .units
        .iter()
        .filter(|unit| indexes.visible_unit_ids.contains(&unit.id))
        .map(|unit| unit.language.key().to_string())
        .collect::<BTreeSet<_>>();
    GlobalContextSummary {
        domain_ids: domains.iter().map(|domain| domain.id.clone()).collect(),
        domain_labels: domains.iter().map(|domain| domain.label.clone()).collect(),
        represented_domain_count: domains.len(),
        language_keys: std::mem::take(&mut languages).into_iter().collect(),
        total_domains: domains.len() + plan.suppressed.len(),
        total_features: indexes.visible_feature_ids.len(),
        total_flows: indexes.visible_flow_ids.len(),
    }
}

pub(super) fn compact_global_summary(
    mut summary: GlobalContextSummary,
    budget_bytes: usize,
) -> GlobalContextSummary {
    while serialized_bytes(&summary) > budget_bytes
        && !summary.domain_ids.is_empty()
        && summary.domain_ids.len() > 1
    {
        summary.domain_ids.pop();
        summary.domain_labels.pop();
        summary.represented_domain_count = summary.represented_domain_count.saturating_sub(1);
    }
    if serialized_bytes(&summary) > budget_bytes {
        summary.domain_ids.clear();
        summary.domain_labels.clear();
        summary.represented_domain_count = 0;
    }
    summary
}

pub(super) fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

pub(super) fn serialized_bytes<T: serde::Serialize>(value: &T) -> usize {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
}
