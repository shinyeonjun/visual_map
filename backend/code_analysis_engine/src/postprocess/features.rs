//! 도메인 카드에 포함할 기능의 태그와 우선순위를 만든다.

use super::indexes::PostprocessIndexes;
use super::model::{ContextFeature, DomainCluster, FeatureTag, FeatureVisibility};
use crate::config::PostprocessPolicy;
use crate::views::overview::model::FeatureVisibility as OverviewVisibility;
use std::collections::{BTreeSet, HashMap, HashSet};

pub(crate) struct FeatureSelection {
    pub features: Vec<ContextFeature>,
    pub included_ids: HashSet<String>,
    pub mandatory_ids: HashSet<String>,
    pub total_count: usize,
    pub priorities: HashMap<String, u64>,
}

pub(crate) fn select_for_domain(
    cluster: &DomainCluster,
    indexes: &PostprocessIndexes<'_>,
    policy: &PostprocessPolicy,
) -> FeatureSelection {
    let mut candidates = indexes
        .overview
        .features
        .iter()
        .filter(|feature| indexes.visible_feature_ids.contains(&feature.id))
        .filter(|feature| belongs_to_domain(feature, cluster))
        .map(|feature| (feature, priority(feature, policy)))
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| left.0.id.cmp(&right.0.id))
    });

    let total_count = candidates.len();
    let priorities = candidates
        .iter()
        .map(|(feature, priority)| (feature.id.clone(), u64::from(*priority)))
        .collect::<HashMap<_, _>>();
    let features = candidates
        .iter()
        .map(|(feature, _)| {
            to_context_feature(feature, indexes, policy, is_required(feature, indexes))
        })
        .collect::<Vec<_>>();
    let included_ids = features
        .iter()
        .map(|feature| feature.id.clone())
        .collect::<HashSet<_>>();
    let mandatory_ids = features
        .iter()
        .filter(|feature| feature.required)
        .map(|feature| feature.id.clone())
        .collect::<HashSet<_>>();
    FeatureSelection {
        features,
        included_ids,
        mandatory_ids,
        total_count,
        priorities,
    }
}

pub(crate) fn is_required(
    feature: &crate::views::overview::FeatureGroup,
    indexes: &PostprocessIndexes<'_>,
) -> bool {
    feature
        .entrypoint_ids
        .iter()
        .any(|id| indexes.visible_entrypoint_ids.contains(id))
        || feature
            .resource_ids
            .iter()
            .any(|id| indexes.visible_resource_ids.contains(id))
        || !feature.dynamic_boundary_ids.is_empty()
}

fn belongs_to_domain(
    feature: &crate::views::overview::FeatureGroup,
    cluster: &DomainCluster,
) -> bool {
    feature
        .domain_ids
        .iter()
        .any(|domain_id| cluster.source_domain_ids.contains(domain_id))
}

fn to_context_feature(
    feature: &crate::views::overview::FeatureGroup,
    indexes: &PostprocessIndexes<'_>,
    policy: &PostprocessPolicy,
    required: bool,
) -> ContextFeature {
    let mut symbols = feature
        .unit_ids
        .iter()
        .filter_map(|unit_id| indexes.unit(unit_id).map(|unit| unit.name.clone()))
        .filter(|name| !name.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    symbols.truncate(policy.max_symbols_per_feature);

    let mut source_paths = feature
        .unit_ids
        .iter()
        .filter_map(|unit_id| indexes.unit(unit_id).map(|unit| unit.relative_path.clone()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    source_paths.truncate(policy.max_paths_per_feature);

    let tags = feature_tags(feature, indexes);
    let mut entrypoint_ids = feature
        .entrypoint_ids
        .iter()
        .filter(|id| indexes.visible_entrypoint_ids.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    entrypoint_ids.sort();
    let mut resource_ids = feature
        .resource_ids
        .iter()
        .filter(|id| indexes.visible_resource_ids.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    resource_ids.sort();
    let mut flow_ids = feature
        .flow_ids
        .iter()
        .filter(|id| indexes.visible_flow_ids.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    flow_ids.sort();

    ContextFeature {
        id: feature.id.clone(),
        current_label: feature.label.clone(),
        visibility: if matches!(feature.visibility, OverviewVisibility::UserFacing) {
            FeatureVisibility::UserFacing
        } else {
            FeatureVisibility::Internal
        },
        required,
        tags,
        symbols,
        source_paths,
        entrypoint_ids,
        resource_ids,
        flow_ids,
    }
}

fn feature_tags(
    feature: &crate::views::overview::FeatureGroup,
    indexes: &PostprocessIndexes<'_>,
) -> Vec<FeatureTag> {
    let mut tags = Vec::new();
    if !feature.entrypoint_ids.is_empty() {
        tags.push(FeatureTag::Endpoint);
    }
    if !feature.resource_ids.is_empty() {
        tags.push(FeatureTag::ResourceAccess);
        let has_write = feature.resource_ids.iter().any(|resource_id| {
            indexes
                .resource(resource_id)
                .map(|resource| {
                    matches!(
                        resource.mode,
                        crate::facts::AccessMode::Write | crate::facts::AccessMode::ReadWrite
                    )
                })
                .unwrap_or(false)
        });
        if has_write {
            tags.push(FeatureTag::SideEffect);
        }
    }
    if !feature.dynamic_boundary_ids.is_empty() {
        tags.push(FeatureTag::DynamicBoundary);
    }
    if matches!(feature.visibility, OverviewVisibility::Internal) {
        tags.push(FeatureTag::Internal);
    }
    tags
}

fn priority(feature: &crate::views::overview::FeatureGroup, policy: &PostprocessPolicy) -> u32 {
    let mut score = 0;
    if !feature.entrypoint_ids.is_empty() {
        score += policy.feature_entrypoint_weight;
    }
    if !feature.resource_ids.is_empty() {
        score += policy.feature_resource_weight;
    }
    if !feature.dynamic_boundary_ids.is_empty() {
        score += policy.feature_dynamic_weight;
    }
    score.saturating_add(
        (feature
            .reachable_unit_count
            .min(policy.feature_complexity_cap) as u32)
            .saturating_mul(policy.feature_complexity_weight),
    )
}
