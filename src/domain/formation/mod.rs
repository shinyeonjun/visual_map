//! Capability-first 클러스터링으로 도메인을 형성하는 파이프라인.

mod capability_data;
mod cluster_groups;
mod constraints;
pub mod diagnostics;
mod domain_cleanup;
mod feature_assignment;
mod singleton_absorption;

use crate::config::DomainPolicy;
use crate::domain::capabilities::build as build_capabilities;
use crate::domain::formation::diagnostics::DomainFormationDiagnostics;
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
use domain_cleanup::prune_resource_only_domains;
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
                formation_diagnostics: DomainFormationDiagnostics::new(
                    domain_policy.domain_clustering_mode,
                ),
                ..Default::default()
            },
            features,
        };
    }

    let mut formation_diagnostics =
        DomainFormationDiagnostics::new(domain_policy.domain_clustering_mode);
    let distinct_keys = capabilities
        .iter()
        .map(|capability| capability.key.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();

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

    let constraints = build_constraints(
        &capabilities,
        store,
        path_policy,
        domain_policy.domain_clustering_mode,
    );
    formation_diagnostics.record_constraint_stats(
        capabilities.len(),
        distinct_keys,
        constraints.forbidden_pairs.len(),
    );
    let target_clusters = clustering::target_cluster_count(
        capabilities.len().max(1),
        domain_policy.domain_cluster_min,
        domain_policy.domain_cluster_max,
    );
    let (clusters, merge_events) = clustering::cluster(
        &matrix,
        &constraints,
        clustering::ClusterOptions {
            merge_threshold: domain_policy.domain_cluster_merge_threshold,
            target_count: target_clusters,
            max_count: domain_policy.domain_cluster_max,
            mode: domain_policy.domain_clustering_mode,
        },
    );
    for event in merge_events {
        formation_diagnostics.record_merge(&event.reason);
    }

    let mut formation = form_domains_from_clusters(
        &clusters,
        &capabilities,
        &terms,
        &matrix,
        domain_policy,
        store,
    );
    formation_diagnostics.record_clustering(clusters.len(), formation.groups.len());

    absorb_singleton_domains(
        &mut formation,
        &clusters,
        &capabilities,
        &terms,
        &matrix,
        store,
        domain_policy,
        &mut formation_diagnostics,
    );

    prune_resource_only_domains(&mut formation);
    formation_diagnostics.record_absorption(formation.groups.len());

    assign_features_to_domains(&mut features, &formation.groups);

    eprintln!(
        "[domain] formation mode={} capabilities={} features={} clusters={} groups={} target_clusters={} forbidden={}/{} merges={} absorbed={}",
        formation_diagnostics.clustering_mode,
        capabilities.len(),
        features.len(),
        formation_diagnostics.clusters_before_absorption,
        formation.groups.len(),
        target_clusters,
        formation_diagnostics.forbidden_pairs,
        formation_diagnostics.total_pairs,
        formation_diagnostics.clustering_merges,
        formation_diagnostics.absorbed_domains,
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
            formation_diagnostics,
        },
        features,
    }
}
