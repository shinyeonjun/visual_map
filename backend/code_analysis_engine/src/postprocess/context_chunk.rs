//! 도메인별 feature·flow bundle을 실제 byte budget에 맞춰 조립한다.

use super::domains::DomainPlan;
use super::features::select_for_domain as select_features;
use super::flows::candidates_for_features;
use super::indexes::PostprocessIndexes;
use super::model::{
    AdjacentDomain, CodexSemanticContext, ContextDomain, ContextProjectProfile, ContextSummary,
    ContextWarning, GlobalContextSummary,
};
use super::selection::{fill_domain, promote_required_relationships, required_domain_bytes};
use crate::config::AnalysisConfig;
use crate::model::AnalysisResult;
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct ChunkBuildInput<'a> {
    pub(super) chunk_id: &'a str,
    pub(super) partition: &'a [String],
    pub(super) shells: &'a [ContextDomain],
    pub(super) plan: &'a DomainPlan,
    pub(super) indexes: &'a PostprocessIndexes<'a>,
    pub(super) overview: &'a crate::views::overview::OverviewResponse,
    pub(super) result: &'a AnalysisResult,
    pub(super) config: &'a AnalysisConfig,
    pub(super) profile: ContextProjectProfile,
    pub(super) global_summary: GlobalContextSummary,
}

pub(super) fn required_domain_size(
    domain: &ContextDomain,
    plan: &DomainPlan,
    indexes: &PostprocessIndexes<'_>,
    config: &AnalysisConfig,
) -> usize {
    let cluster = plan
        .clusters
        .iter()
        .find(|cluster| cluster.representative_id == domain.domain_id)
        .expect("domain shell에 대응하는 cluster가 있어야 한다");
    let mut features = select_features(cluster, indexes, &config.postprocess);
    let flows = candidates_for_features(&features.included_ids, indexes, &config.postprocess);
    promote_required_relationships(&mut features, &flows);
    required_domain_bytes(domain, &features, &flows)
}

pub(super) fn build_chunk(input: ChunkBuildInput<'_>) -> CodexSemanticContext {
    let ChunkBuildInput {
        chunk_id,
        partition,
        shells,
        plan,
        indexes,
        overview,
        result,
        config,
        profile,
        global_summary,
    } = input;
    let mut domains = partition
        .iter()
        .filter_map(|domain_id| shells.iter().find(|domain| &domain.domain_id == domain_id))
        .cloned()
        .collect::<Vec<_>>();
    let base_bytes = serialized_bytes(&ChunkShell {
        chunk_id,
        source_analysis_id: &result.analysis_id,
        project_id: &result.project.project_id,
        project_profile: &profile,
        global_summary: &global_summary,
        adjacent_domains: &[],
        domains: &domains,
    });
    let total_signal = domains
        .iter()
        .map(|domain| u64::from(domain.signal.score.max(1)))
        .sum::<u64>();
    let remaining = config
        .postprocess
        .target_budget_bytes
        .saturating_sub(base_bytes);
    let mut feature_membership_total = 0;
    let mut flow_membership_total = 0;
    let mut total_feature_ids = BTreeSet::new();
    let mut total_flow_ids = BTreeSet::new();
    let mut required_extra_total = 0usize;
    let mut required_extras = BTreeMap::new();
    let mut selections = Vec::new();
    for domain in &mut domains {
        let shell_bytes = serialized_bytes(domain);
        let feature_selection = select_features(
            plan.clusters
                .iter()
                .find(|cluster| cluster.representative_id == domain.domain_id)
                .expect("domain shell에 대응하는 cluster가 있어야 한다"),
            indexes,
            &config.postprocess,
        );
        let flow_selection = candidates_for_features(
            &feature_selection.included_ids,
            indexes,
            &config.postprocess,
        );
        let mut feature_selection = feature_selection;
        promote_required_relationships(&mut feature_selection, &flow_selection);
        let required_bytes = required_domain_bytes(domain, &feature_selection, &flow_selection);
        let required_extra = required_bytes.saturating_sub(shell_bytes);
        required_extra_total = required_extra_total.saturating_add(required_extra);
        required_extras.insert(domain.domain_id.clone(), required_extra);
        feature_membership_total += feature_selection.total_count;
        flow_membership_total += flow_selection.total_count;
        total_feature_ids.extend(feature_selection.included_ids.iter().cloned());
        total_flow_ids.extend(
            flow_selection
                .candidates
                .iter()
                .map(|candidate| candidate.context.id.clone()),
        );
        selections.push((domain.domain_id.clone(), feature_selection, flow_selection));
    }
    let extra_remaining = remaining.saturating_sub(required_extra_total);
    for domain in &mut domains {
        let shell_bytes = serialized_bytes(domain);
        let signal = domain.signal.score.max(1) as usize;
        let allocation = if total_signal == 0 {
            0
        } else {
            extra_remaining.saturating_mul(signal) / total_signal as usize
        };
        let required_extra = required_extras.get(&domain.domain_id).copied().unwrap_or(0);
        let Some((_, feature_selection, flow_selection)) = selections
            .iter_mut()
            .find(|(domain_id, _, _)| domain_id == &domain.domain_id)
        else {
            continue;
        };
        *domain = fill_domain(
            domain.clone(),
            std::mem::replace(
                feature_selection,
                super::features::FeatureSelection {
                    features: Vec::new(),
                    included_ids: std::collections::HashSet::new(),
                    mandatory_ids: std::collections::HashSet::new(),
                    total_count: 0,
                    priorities: std::collections::HashMap::new(),
                },
            ),
            std::mem::replace(
                flow_selection,
                super::flows::FlowSelection {
                    candidates: Vec::new(),
                    total_count: 0,
                },
            ),
            shell_bytes
                .saturating_add(required_extra)
                .saturating_add(allocation),
        );
    }
    let adjacent_domains = adjacent_domains(partition, overview, shells, config);
    let mut context = CodexSemanticContext {
        schema_version: "codex-semantic-context.v1",
        chunk_id: chunk_id.into(),
        source_analysis_id: result.analysis_id.clone(),
        source_schema_version: result.schema_version.clone(),
        project_id: result.project.project_id.clone(),
        analysis_status: result.status.clone(),
        policy_version: "codex-semantic-context-policy.v2",
        project_profile: profile,
        global_summary,
        adjacent_domains,
        domains,
        domain_aliases: plan.aliases.clone(),
        suppressed_domains: plan.suppressed.clone(),
        summary: ContextSummary {
            total_source_domains: partition.len(),
            included_domains: partition.len(),
            suppressed_domains: plan.suppressed.len(),
            total_features: feature_membership_total,
            total_unique_features: total_feature_ids.len(),
            total_feature_memberships: feature_membership_total,
            included_features: 0,
            included_unique_features: 0,
            included_feature_memberships: 0,
            omitted_unique_features: 0,
            omitted_feature_memberships: 0,
            total_flows: flow_membership_total,
            total_unique_flows: total_flow_ids.len(),
            total_flow_memberships: flow_membership_total,
            included_flows: 0,
            included_unique_flows: 0,
            included_flow_memberships: 0,
            omitted_unique_flows: 0,
            omitted_flow_memberships: 0,
            budget_bytes: config.postprocess.target_budget_bytes,
            used_bytes: 0,
        },
        warnings: Vec::new(),
    };
    update_summary(&mut context);
    context
}

pub(super) fn fit_context_budget(context: &mut CodexSemanticContext, budget: usize) {
    while serialized_bytes(context) > budget {
        let Some(domain_index) = context
            .domains
            .iter()
            .enumerate()
            .filter(|(_, domain)| {
                domain.features.iter().any(|feature| !feature.required)
                    || domain.flows.iter().any(|flow| !flow.required)
            })
            .min_by(|(_, left), (_, right)| {
                optional_domain_score(left)
                    .cmp(&optional_domain_score(right))
                    .then_with(|| right.domain_id.cmp(&left.domain_id))
            })
            .map(|(index, _)| index)
        else {
            context.warnings.push(ContextWarning {
                code: "required_content_exceeds_budget".into(),
                message: "필수 기능·실행 흐름을 보존하면 byte budget을 초과합니다.".into(),
                related_ids: context
                    .domains
                    .iter()
                    .map(|domain| domain.domain_id.clone())
                    .collect(),
            });
            break;
        };
        let domain = &mut context.domains[domain_index];
        let optional_feature_index = domain
            .features
            .iter()
            .enumerate()
            .filter(|(_, feature)| !feature.required)
            .min_by(|(_, left), (_, right)| {
                feature_optional_score(left)
                    .cmp(&feature_optional_score(right))
                    .then_with(|| right.id.cmp(&left.id))
            })
            .map(|(index, _)| index);
        if let Some(feature_index) = optional_feature_index {
            let feature = domain.features.remove(feature_index);
            let feature_id = feature.id;
            for flow in &mut domain.flows {
                if !flow.required {
                    flow.feature_ids.retain(|id| id != &feature_id);
                }
            }
            domain
                .flows
                .retain(|flow| flow.required || !flow.feature_ids.is_empty());
            *domain
                .omission
                .reasons
                .entry("global_budget_exceeded".into())
                .or_insert(0) += 1;
        } else {
            if let Some(flow_index) = domain
                .flows
                .iter()
                .enumerate()
                .filter(|(_, flow)| !flow.required)
                .min_by(|(_, left), (_, right)| {
                    flow_optional_score(left)
                        .cmp(&flow_optional_score(right))
                        .then_with(|| right.id.cmp(&left.id))
                })
                .map(|(index, _)| index)
            {
                domain.flows.remove(flow_index);
            }
            *domain
                .omission
                .reasons
                .entry("global_budget_exceeded".into())
                .or_insert(0) += 1;
        }
        update_summary(context);
    }
    context.summary.used_bytes = serialized_bytes(context);
    for domain in &mut context.domains {
        domain.omission.used_bytes = serialized_bytes(domain);
    }
}

fn update_summary(context: &mut CodexSemanticContext) {
    let feature_memberships = context
        .domains
        .iter()
        .map(|domain| domain.features.len())
        .sum::<usize>();
    let flow_memberships = context
        .domains
        .iter()
        .map(|domain| domain.flows.len())
        .sum::<usize>();
    let feature_ids = context
        .domains
        .iter()
        .flat_map(|domain| domain.features.iter().map(|feature| feature.id.clone()))
        .collect::<BTreeSet<_>>();
    let flow_ids = context
        .domains
        .iter()
        .flat_map(|domain| domain.flows.iter().map(|flow| flow.id.clone()))
        .collect::<BTreeSet<_>>();
    context.summary.included_features = feature_memberships;
    context.summary.included_unique_features = feature_ids.len();
    context.summary.included_feature_memberships = feature_memberships;
    context.summary.omitted_unique_features = context
        .summary
        .total_unique_features
        .saturating_sub(feature_ids.len());
    context.summary.omitted_feature_memberships = context
        .summary
        .total_feature_memberships
        .saturating_sub(feature_memberships);
    context.summary.included_flows = flow_memberships;
    context.summary.included_unique_flows = flow_ids.len();
    context.summary.included_flow_memberships = flow_memberships;
    context.summary.omitted_unique_flows = context
        .summary
        .total_unique_flows
        .saturating_sub(flow_ids.len());
    context.summary.omitted_flow_memberships = context
        .summary
        .total_flow_memberships
        .saturating_sub(flow_memberships);
    context.summary.used_bytes = serialized_bytes(context);
    for domain in &mut context.domains {
        domain.omission.included_features = domain.features.len();
        domain.omission.included_flows = domain.flows.len();
        domain.omission.used_bytes = serialized_bytes(domain);
    }
}

fn optional_domain_score(domain: &ContextDomain) -> usize {
    domain
        .features
        .iter()
        .filter(|feature| !feature.required)
        .map(feature_optional_score)
        .chain(
            domain
                .flows
                .iter()
                .filter(|flow| !flow.required)
                .map(flow_optional_score),
        )
        .min()
        .unwrap_or(usize::MAX)
}

fn feature_optional_score(feature: &super::model::ContextFeature) -> usize {
    feature.entrypoint_ids.len()
        + feature.resource_ids.len()
        + feature.flow_ids.len()
        + feature.symbols.len()
}

fn flow_optional_score(flow: &super::model::ContextFlow) -> usize {
    flow.feature_ids.len() + flow.steps.len() + flow.edges.len()
}

fn adjacent_domains(
    partition: &[String],
    overview: &crate::views::overview::OverviewResponse,
    shells: &[ContextDomain],
    config: &AnalysisConfig,
) -> Vec<AdjacentDomain> {
    let mut relation_kinds: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for relation in &overview.relations {
        let inside_source = partition.contains(&relation.source_domain_id);
        let inside_target = partition.contains(&relation.target_domain_id);
        if inside_source == inside_target {
            continue;
        }
        let outside = if inside_source {
            &relation.target_domain_id
        } else {
            &relation.source_domain_id
        };
        relation_kinds
            .entry(outside.clone())
            .or_default()
            .insert(relation.kind.clone());
    }
    relation_kinds
        .into_iter()
        .filter_map(|(domain_id, kinds)| {
            let domain = shells.iter().find(|domain| domain.domain_id == domain_id)?;
            Some(AdjacentDomain {
                domain_id,
                label: domain.current_label.clone(),
                relation_kinds: kinds.into_iter().collect(),
            })
        })
        .take(config.postprocess.max_adjacent_domains)
        .collect()
}

pub(super) fn serialized_bytes<T: serde::Serialize>(value: &T) -> usize {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
}

#[derive(serde::Serialize)]
struct ChunkShell<'a> {
    chunk_id: &'a str,
    source_analysis_id: &'a str,
    project_id: &'a str,
    project_profile: &'a ContextProjectProfile,
    global_summary: &'a GlobalContextSummary,
    adjacent_domains: &'a [AdjacentDomain],
    domains: &'a [ContextDomain],
}
