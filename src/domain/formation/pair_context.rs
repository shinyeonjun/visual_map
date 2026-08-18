//! capability pair 분석에 필요한 유사도 행렬과 인덱스를 한 번에 구성한다.

use crate::config::{DomainPolicy, PathPolicy};
use crate::domain::capabilities::{build as build_capabilities, Capability};
use crate::domain::feature_graph::{self, SimilarityMatrix};
use crate::domain::tfidf::{self, FeatureTerms};
use crate::facts::FactStore;
use crate::flow::ExecutionFlowGraph;
use std::collections::HashMap;

use super::capability_data::{extract_capability_data, CapabilityData};

pub(crate) struct CapabilityPairContext {
    pub capabilities: Vec<Capability>,
    pub capability_data: CapabilityData,
    pub terms: Vec<FeatureTerms>,
    pub matrix: SimilarityMatrix,
    pub key_index: HashMap<String, usize>,
    pub merge_threshold: f64,
}

pub(crate) fn build_capability_pair_context(
    store: &FactStore,
    execution_flows: &ExecutionFlowGraph,
    domain_policy: &DomainPolicy,
    path_policy: &PathPolicy,
) -> CapabilityPairContext {
    let capabilities = build_capabilities(store, path_policy);
    let capability_data = extract_capability_data(&capabilities, store, execution_flows);
    let (terms, _) = tfidf::extract_terms(&capability_data.unit_ids, store, domain_policy);
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
    let key_index = capabilities
        .iter()
        .enumerate()
        .map(|(index, capability)| (capability.key.clone(), index))
        .collect();

    CapabilityPairContext {
        capabilities,
        capability_data,
        terms,
        matrix,
        key_index,
        merge_threshold: domain_policy.domain_cluster_merge_threshold,
    }
}
