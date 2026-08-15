//! Capability-first 클러스터링으로 도메인을 형성하는 파이프라인.

mod capability_data;
mod cluster_groups;
mod constraints;
mod feature_assignment;
mod singleton_absorption;

use crate::config::DomainPolicy;
use crate::domain::capabilities::build as build_capabilities;
use crate::domain::grouping::DomainAnalysisOutput;
use crate::domain::membership::{DomainMembership, MembershipKind};
use crate::facts::{FactStore, ResolutionStatus};
use crate::flow::ExecutionFlowGraph;
use crate::graph::StaticRelationGraph;
use crate::views::overview::model::FeatureGroup;

use super::aggregation::aggregate_relations;
use super::{clustering, feature_graph, tfidf};

use capability_data::extract_capability_data;
use cluster_groups::form_domains_from_clusters;
use constraints::{build_constraints, collect_unassigned_units};
use feature_assignment::assign_features_to_domains;
use singleton_absorption::absorb_singleton_domains;

/// Capability-first 클러스터링의 결과다.
pub struct FeatureFirstResult {
    pub analysis: DomainAnalysisOutput,
    pub features: Vec<FeatureGroup>,
}

/// Capability를 먼저 만들고 Multi-view 유사도로 클러스터링해 도메인을 형성한다.
pub(super) fn analyze_feature_first(
    store: &FactStore,
    graph: &StaticRelationGraph,
    execution_flows: &ExecutionFlowGraph,
    domain_policy: &DomainPolicy,
    path_policy: &crate::config::PathPolicy,
) -> FeatureFirstResult {
    let empty_analysis = DomainAnalysisOutput::default();
    let capabilities = build_capabilities(store, path_policy);
    let mut features = crate::views::overview::features::build(
        &empty_analysis,
        store,
        execution_flows,
        path_policy,
    );

    if capabilities.is_empty() {
        return FeatureFirstResult {
            analysis: DomainAnalysisOutput {
                static_graph: graph.clone(),
                ..Default::default()
            },
            features,
        };
    }

    let capability_data = extract_capability_data(&capabilities, store, execution_flows);
    let (terms, _term_index) =
        tfidf::extract_terms(&capability_data.unit_ids, store, domain_policy);

    let matrix = feature_graph::compute(
        &capability_data.unit_ids,
        &capability_data.resource_ids,
        &capability_data.flow_ids,
        &capability_data.paths,
        &capability_data.keys,
        &capability_data.contract_paths,
        &terms,
        store,
        execution_flows,
        domain_policy,
    );

    let constraints = build_constraints(&capabilities, store, path_policy);
    let target_clusters = clustering::target_cluster_count(
        capabilities.len().max(1),
        domain_policy.domain_cluster_min,
        domain_policy.domain_cluster_max,
    );
    let clusters = clustering::cluster(
        &matrix,
        &constraints,
        clustering::ClusterOptions {
            merge_threshold: domain_policy.domain_cluster_merge_threshold,
            target_count: target_clusters,
            max_count: domain_policy.domain_cluster_max,
        },
    );

    let mut formation = form_domains_from_clusters(
        &clusters,
        &capabilities,
        &terms,
        &matrix,
        domain_policy,
        store,
    );

    absorb_singleton_domains(
        &mut formation,
        &clusters,
        &capabilities,
        &terms,
        &matrix,
        store,
        domain_policy,
    );

    assign_features_to_domains(&mut features, &formation.groups);

    eprintln!(
        "[domain] formation capabilities={} features={} clusters={} groups={} target_clusters={}",
        capabilities.len(),
        features.len(),
        clusters.len(),
        formation.groups.len(),
        target_clusters,
    );

    let unassigned_unit_ids =
        collect_unassigned_units(store, &formation.assigned_units, path_policy);
    let mut memberships = formation.memberships;
    for unit_id in &unassigned_unit_ids {
        memberships.push(DomainMembership {
            unit_id: unit_id.clone(),
            domain_id: None,
            domain_ids: Vec::new(),
            kind: MembershipKind::Unknown,
            score: 0,
        });
    }

    let mut groups = formation.groups;
    let relations = aggregate_relations(store, graph, &memberships, &groups);
    groups.sort_by(|a, b| a.key.cmp(&b.key));

    let dynamic_reference_ids = graph
        .edges
        .iter()
        .filter(|reference| reference.status == ResolutionStatus::Dynamic)
        .map(|reference| reference.id.clone())
        .collect();

    FeatureFirstResult {
        analysis: DomainAnalysisOutput {
            static_graph: graph.clone(),
            groups,
            relations,
            memberships,
            unassigned_unit_ids,
            dynamic_reference_ids,
            signal_count: 0,
        },
        features,
    }
}
