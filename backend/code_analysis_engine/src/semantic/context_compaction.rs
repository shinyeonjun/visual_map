use crate::config::SemanticPolicy;
use crate::domain::{DomainGroup, DomainRelation};
use crate::frameworks::registry::detector::FrameworkDetection;
use serde::Serialize;
use std::collections::BTreeSet;

use super::context_model::SemanticContext;

pub(super) fn compact_domain(domain: &DomainGroup, policy: &SemanticPolicy) -> DomainGroup {
    let mut compact = domain.clone();
    compact.primary_unit_ids.truncate(policy.domain_unit_limit);
    compact.shared_unit_ids.truncate(policy.shared_unit_limit);
    compact.entrypoint_ids.truncate(policy.entrypoint_limit);
    compact.resource_ids.truncate(policy.resource_limit);
    compact.evidence.truncate(policy.domain_evidence_limit);
    compact
}

pub(super) fn build_context(
    domains: Vec<DomainGroup>,
    all_relations: Vec<DomainRelation>,
    frameworks: &[FrameworkDetection],
    policy: &SemanticPolicy,
) -> SemanticContext {
    let domain_ids: BTreeSet<_> = domains.iter().map(|domain| domain.id.as_str()).collect();
    let mut relations: Vec<_> = all_relations
        .into_iter()
        .filter(|relation| {
            domain_ids.contains(relation.source_domain_id.as_str())
                || domain_ids.contains(relation.target_domain_id.as_str())
        })
        .collect();
    relations.truncate(policy.relation_limit);
    for relation in &mut relations {
        relation.evidence.truncate(policy.relation_evidence_limit);
    }

    let mut frameworks = frameworks.to_vec();
    for framework in &mut frameworks {
        framework.evidence.truncate(policy.framework_evidence_limit);
    }

    SemanticContext {
        domains,
        relations,
        frameworks,
    }
}

pub(super) fn compact_relation(
    relation: &DomainRelation,
    policy: &SemanticPolicy,
) -> DomainRelation {
    let mut compact = relation.clone();
    compact.evidence.truncate(policy.relation_evidence_limit);
    compact
}

pub(super) fn compact_framework(
    framework: &FrameworkDetection,
    policy: &SemanticPolicy,
) -> FrameworkDetection {
    let mut compact = framework.clone();
    compact.evidence.truncate(policy.framework_evidence_limit);
    compact
}

pub(super) fn json_size<T: Serialize>(value: &T) -> usize {
    serde_json::to_vec(value)
        .map(|json| json.len())
        .unwrap_or(usize::MAX)
}

pub(super) fn json_array_size<I>(item_sizes: I, item_count: usize) -> usize
where
    I: IntoIterator<Item = usize>,
{
    2usize
        .saturating_add(item_sizes.into_iter().fold(0, usize::saturating_add))
        .saturating_add(item_count.saturating_sub(1))
}
