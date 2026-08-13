//! 도메인 shell에 파일·진입점·자원 메타데이터를 채운다.

use super::domains::{domain_context, DomainPlan};
use super::features::is_required as feature_is_required;
use super::indexes::PostprocessIndexes;
use super::model::{ContextDomain, ContextEntrypoint, ContextResource, DomainCluster};
use crate::config::{AnalysisConfig, PostprocessPolicy};
use std::collections::BTreeSet;

pub(super) fn build_shells(
    plan: &DomainPlan,
    indexes: &PostprocessIndexes<'_>,
    config: &AnalysisConfig,
) -> Vec<ContextDomain> {
    let mut shells = plan
        .clusters
        .iter()
        .filter_map(|cluster| {
            let mut domain = domain_context(cluster, indexes, &config.domains)?;
            domain
                .source_paths
                .truncate(config.postprocess.max_files_per_domain);
            domain
                .evidence_ids
                .truncate(config.postprocess.max_evidence_ids_per_domain);
            domain.entrypoints = entrypoints_for_domain(cluster, indexes, &config.postprocess);
            domain.resources = resources_for_domain(cluster, indexes, &config.postprocess);
            Some(domain)
        })
        .collect::<Vec<_>>();
    shells.sort_by(|left, right| left.domain_id.cmp(&right.domain_id));
    shells
}

fn entrypoints_for_domain(
    cluster: &DomainCluster,
    indexes: &PostprocessIndexes<'_>,
    policy: &PostprocessPolicy,
) -> Vec<ContextEntrypoint> {
    let mut ids = cluster
        .source_domain_ids
        .iter()
        .filter_map(|domain_id| indexes.domains.get(domain_id.as_str()))
        .flat_map(|domain| domain.entrypoint_ids.iter())
        .filter(|id| indexes.visible_entrypoint_ids.contains(*id))
        .cloned()
        .collect::<BTreeSet<_>>();
    ids.retain(|id| indexes.entrypoint(id).is_some());
    let required = required_entrypoint_ids(cluster, indexes);
    let mut ordered_ids = required
        .iter()
        .filter(|id| ids.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    ordered_ids.extend(
        ids.iter()
            .filter(|id| !required.contains(*id))
            .take(policy.max_entrypoints_per_domain)
            .cloned(),
    );
    ordered_ids
        .into_iter()
        .filter_map(|id| {
            let entrypoint = indexes.entrypoint(&id)?;
            Some(ContextEntrypoint {
                id: entrypoint.id.clone(),
                unit_id: entrypoint.unit_id.clone(),
                kind: entrypoint.kind.clone(),
                name: entrypoint.name.clone(),
                method: entrypoint.method.clone(),
                path: entrypoint.path.clone(),
            })
        })
        .collect()
}

fn resources_for_domain(
    cluster: &DomainCluster,
    indexes: &PostprocessIndexes<'_>,
    policy: &PostprocessPolicy,
) -> Vec<ContextResource> {
    let mut ids = cluster
        .source_domain_ids
        .iter()
        .filter_map(|domain_id| indexes.domains.get(domain_id.as_str()))
        .flat_map(|domain| domain.resource_ids.iter())
        .filter(|id| indexes.visible_resource_ids.contains(*id))
        .cloned()
        .collect::<BTreeSet<_>>();
    ids.retain(|id| indexes.resource(id).is_some());
    let required = required_resource_ids(cluster, indexes);
    let mut ordered_ids = required
        .iter()
        .filter(|id| ids.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    ordered_ids.extend(
        ids.iter()
            .filter(|id| !required.contains(*id))
            .take(policy.max_resources_per_domain)
            .cloned(),
    );
    ordered_ids
        .into_iter()
        .filter_map(|id| {
            let resource = indexes.resource(&id)?;
            Some(ContextResource {
                id: resource.id.clone(),
                kind: resource.kind.clone(),
                name: resource.name.clone(),
                mode: resource.mode.clone(),
            })
        })
        .collect()
}

fn required_entrypoint_ids(
    cluster: &DomainCluster,
    indexes: &PostprocessIndexes<'_>,
) -> BTreeSet<String> {
    indexes
        .overview
        .features
        .iter()
        .filter(|feature| {
            indexes.visible_feature_ids.contains(&feature.id)
                && belongs_to_domain(feature, cluster)
                && feature_is_required(feature, indexes)
        })
        .flat_map(|feature| feature.entrypoint_ids.iter())
        .filter(|id| indexes.visible_entrypoint_ids.contains(*id))
        .cloned()
        .collect()
}

fn required_resource_ids(
    cluster: &DomainCluster,
    indexes: &PostprocessIndexes<'_>,
) -> BTreeSet<String> {
    indexes
        .overview
        .features
        .iter()
        .filter(|feature| {
            indexes.visible_feature_ids.contains(&feature.id)
                && belongs_to_domain(feature, cluster)
                && feature_is_required(feature, indexes)
        })
        .flat_map(|feature| feature.resource_ids.iter())
        .filter(|id| indexes.visible_resource_ids.contains(*id))
        .cloned()
        .collect()
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
