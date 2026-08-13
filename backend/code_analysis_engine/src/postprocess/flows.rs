//! Codex 카드에 포함할 실행 흐름 후보를 만든다.

use super::indexes::PostprocessIndexes;
use super::model::{ContextFlow, ContextFlowEdge, ContextFlowStep};
use crate::config::PostprocessPolicy;
use crate::views::overview::FeatureGroup;
use std::collections::{BTreeSet, HashMap, HashSet};

pub(crate) struct FlowSelection {
    pub candidates: Vec<FlowCandidate>,
    pub total_count: usize,
}

pub(crate) struct FlowCandidate {
    pub context: ContextFlow,
    pub score: u64,
}

pub(crate) fn candidates_for_features(
    feature_ids: &HashSet<String>,
    indexes: &PostprocessIndexes<'_>,
    policy: &PostprocessPolicy,
) -> FlowSelection {
    let feature_by_flow = feature_flow_index(feature_ids, indexes);
    let mut candidates = feature_by_flow
        .into_iter()
        .filter_map(|(flow_id, feature_ids)| {
            let flow = indexes.flow(&flow_id)?;
            let features = feature_ids
                .iter()
                .filter_map(|id| indexes.feature(id))
                .collect::<Vec<_>>();
            let score = score_flow(flow, &features, indexes, policy);
            let selection_reason = selection_reason(flow, &features, indexes);
            Some(FlowCandidate {
                context: to_context_flow(flow, feature_ids, &selection_reason, indexes, policy),
                score,
            })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.context.id.cmp(&right.context.id))
    });
    let total_count = candidates.len();
    FlowSelection {
        candidates,
        total_count,
    }
}

fn feature_flow_index(
    feature_ids: &HashSet<String>,
    indexes: &PostprocessIndexes<'_>,
) -> HashMap<String, Vec<String>> {
    let mut result: HashMap<String, BTreeSet<String>> = HashMap::new();
    for feature_id in feature_ids {
        let Some(feature) = indexes.feature(feature_id) else {
            continue;
        };
        for flow_id in &feature.flow_ids {
            if indexes.visible_flow_ids.contains(flow_id) {
                result
                    .entry(flow_id.clone())
                    .or_default()
                    .insert(feature_id.clone());
            }
        }
    }
    result
        .into_iter()
        .map(|(flow_id, feature_ids)| (flow_id, feature_ids.into_iter().collect()))
        .collect()
}

fn score_flow(
    flow: &crate::flow::ExecutionFlow,
    features: &[&FeatureGroup],
    indexes: &PostprocessIndexes<'_>,
    policy: &PostprocessPolicy,
) -> u64 {
    let has_entrypoint = features
        .iter()
        .any(|feature| !feature.entrypoint_ids.is_empty());
    let has_resource = features
        .iter()
        .any(|feature| !feature.resource_ids.is_empty())
        || indexes
            .overview
            .resources
            .iter()
            .filter(|resource| indexes.visible_resource_ids.contains(&resource.id))
            .any(|resource| resource.unit_id == flow.owner_unit_id);
    let has_dynamic = !flow.dynamic_boundary_ids.is_empty()
        || features
            .iter()
            .any(|feature| !feature.dynamic_boundary_ids.is_empty());
    let complexity = flow.nodes.len().saturating_add(flow.edges.len());
    u64::from(has_entrypoint) * u64::from(policy.flow_entrypoint_weight)
        + u64::from(has_resource) * u64::from(policy.flow_resource_weight)
        + u64::from(has_dynamic) * u64::from(policy.flow_dynamic_weight)
        + complexity as u64 * u64::from(policy.flow_complexity_weight)
}

fn selection_reason(
    flow: &crate::flow::ExecutionFlow,
    features: &[&FeatureGroup],
    indexes: &PostprocessIndexes<'_>,
) -> String {
    let has_entrypoint = features
        .iter()
        .any(|feature| !feature.entrypoint_ids.is_empty());
    let has_resource = features
        .iter()
        .any(|feature| !feature.resource_ids.is_empty())
        || indexes
            .overview
            .resources
            .iter()
            .filter(|resource| indexes.visible_resource_ids.contains(&resource.id))
            .any(|resource| resource.unit_id == flow.owner_unit_id);
    let has_dynamic = !flow.dynamic_boundary_ids.is_empty()
        || features
            .iter()
            .any(|feature| !feature.dynamic_boundary_ids.is_empty());
    if has_entrypoint {
        "entrypoint_flow"
    } else if has_resource {
        "resource_access_flow"
    } else if has_dynamic {
        "dynamic_boundary_flow"
    } else {
        "complex_internal_flow"
    }
    .into()
}

fn to_context_flow(
    flow: &crate::flow::ExecutionFlow,
    mut feature_ids: Vec<String>,
    selection_reason: &str,
    indexes: &PostprocessIndexes<'_>,
    policy: &PostprocessPolicy,
) -> ContextFlow {
    feature_ids.sort();
    feature_ids.dedup();
    let owner_name = indexes
        .unit(&flow.owner_unit_id)
        .map(|unit| unit.name.clone())
        .unwrap_or_else(|| flow.owner_unit_id.clone());
    let steps = flow
        .nodes
        .iter()
        .take(policy.max_flow_nodes)
        .map(|node| ContextFlowStep {
            id: node.id.clone(),
            kind: node.kind.clone(),
            label: node.label.clone(),
            target_unit_id: node.target_unit_id.clone(),
        })
        .collect();
    let edges = flow
        .edges
        .iter()
        .take(policy.max_flow_edges)
        .map(|edge| ContextFlowEdge {
            source_node_id: edge.source_node_id.clone(),
            target_node_id: edge.target_node_id.clone(),
            kind: edge.kind.clone(),
            status: edge.status.clone(),
        })
        .collect();
    let mut dynamic_boundary_ids = flow.dynamic_boundary_ids.clone();
    dynamic_boundary_ids.sort();
    dynamic_boundary_ids.dedup();

    ContextFlow {
        id: flow.id.clone(),
        feature_ids,
        owner_unit_id: flow.owner_unit_id.clone(),
        owner_name,
        steps,
        edges,
        dynamic_boundary_ids,
        selection_reason: selection_reason.into(),
    }
}
