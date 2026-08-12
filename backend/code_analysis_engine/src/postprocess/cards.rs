//! 인덱스·도메인 계획·기능·흐름을 하나의 Codex 카드로 조립한다.

use super::domains::{build_plan, domain_context};
use super::features::select_for_domain as select_features;
use super::flows::select_for_domain as select_flows;
use super::indexes::PostprocessIndexes;
use super::model::{
    CodexSemanticContext, ContextEntrypoint, ContextResource, ContextSummary, DomainOmission,
};
use super::PostprocessError;
use crate::config::AnalysisConfig;
use crate::model::AnalysisResult;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn build(
    result: &AnalysisResult,
    config: &AnalysisConfig,
) -> Result<CodexSemanticContext, PostprocessError> {
    let overview = result
        .overview
        .as_ref()
        .ok_or(PostprocessError::MissingOverview)?;
    let indexes = PostprocessIndexes::build(overview, &result.files);
    let plan = build_plan(&indexes, &config.domains, &config.postprocess);
    let mut domains = Vec::new();
    let mut total_features = 0;
    let mut included_features = 0;
    let mut total_flows = 0;
    let mut included_flows = 0;

    for cluster in &plan.clusters {
        let Some(mut domain) = domain_context(cluster, &indexes, &config.domains) else {
            continue;
        };
        domain
            .source_paths
            .truncate(config.postprocess.max_files_per_domain);
        domain.entrypoints = entrypoints_for_domain(cluster, &indexes, &config.postprocess);
        domain.resources = resources_for_domain(cluster, &indexes, &config.postprocess);

        let feature_selection = select_features(cluster, &indexes, &config.postprocess);
        let (flows, flow_count, flow_reasons) = select_flows(
            &feature_selection.included_ids,
            &indexes,
            &config.postprocess,
        );
        domain.features = feature_selection.features;
        domain.flows = flows;
        let included_flow_ids = domain
            .flows
            .iter()
            .map(|flow| flow.id.as_str())
            .collect::<std::collections::HashSet<_>>();
        for feature in &mut domain.features {
            feature
                .flow_ids
                .retain(|flow_id| included_flow_ids.contains(flow_id.as_str()));
        }
        domain.omission = DomainOmission {
            total_features: feature_selection.total_count,
            included_features: domain.features.len(),
            total_flows: flow_count,
            included_flows: domain.flows.len(),
            reasons: flow_reasons.into_iter().collect::<BTreeMap<_, _>>(),
        };
        total_features += domain.omission.total_features;
        included_features += domain.omission.included_features;
        total_flows += domain.omission.total_flows;
        included_flows += domain.omission.included_flows;
        domains.push(domain);
    }

    domains.sort_by(|left, right| left.domain_id.cmp(&right.domain_id));
    let summary = ContextSummary {
        total_source_domains: plan
            .clusters
            .iter()
            .map(|cluster| cluster.source_domain_ids.len())
            .sum::<usize>()
            + plan.suppressed.len(),
        included_domains: domains.len(),
        suppressed_domains: plan.suppressed.len(),
        total_features,
        included_features,
        total_flows,
        included_flows,
    };

    Ok(CodexSemanticContext {
        schema_version: "codex-semantic-context.v1",
        source_analysis_id: result.analysis_id.clone(),
        source_schema_version: result.schema_version.clone(),
        project_id: result.project.project_id.clone(),
        analysis_status: result.status.clone(),
        policy_version: "codex-semantic-context-policy.v1",
        domains,
        domain_aliases: plan.aliases,
        suppressed_domains: plan.suppressed,
        summary,
    })
}

fn entrypoints_for_domain(
    cluster: &super::model::DomainCluster,
    indexes: &PostprocessIndexes<'_>,
    policy: &crate::config::PostprocessPolicy,
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
    ids.into_iter()
        .take(policy.max_entrypoints_per_domain)
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
    cluster: &super::model::DomainCluster,
    indexes: &PostprocessIndexes<'_>,
    policy: &crate::config::PostprocessPolicy,
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
    ids.into_iter()
        .take(policy.max_resources_per_domain)
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
