//! Codex 카드에 포함할 대표 실행 흐름을 선택한다.

use super::indexes::PostprocessIndexes;
use super::model::{ContextFlow, ContextFlowEdge, ContextFlowStep, SelectedFlow};
use crate::config::PostprocessPolicy;
use std::collections::{HashMap, HashSet};

pub(crate) fn select_for_domain(
    feature_ids: &HashSet<String>,
    indexes: &PostprocessIndexes<'_>,
    policy: &PostprocessPolicy,
) -> (Vec<ContextFlow>, usize, HashMap<String, usize>) {
    let feature_by_flow = feature_flow_index(feature_ids, indexes);
    let mut candidates = feature_by_flow
        .iter()
        .filter_map(|(flow_id, feature_id)| {
            let flow = indexes.flow(flow_id)?;
            let feature = indexes.feature(feature_id)?;
            let selected = score_flow(flow, feature, indexes, policy);
            Some((flow, feature_id.clone(), selected))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .2
            .score
            .cmp(&left.2.score)
            .then_with(|| left.0.id.cmp(&right.0.id))
    });

    let total_count = candidates.len();
    let selected = candidates
        .iter()
        .take(policy.max_flows_per_domain)
        .map(|(flow, _, selection)| SelectedFlow {
            flow_id: flow.id.clone(),
            selection_reason: selection.selection_reason.clone(),
        })
        .collect::<Vec<_>>();
    let reasons = candidates.iter().skip(selected.len()).fold(
        HashMap::new(),
        |mut reasons, (_, _, selection)| {
            *reasons
                .entry(selection.selection_reason.clone())
                .or_insert(0) += 1;
            reasons
        },
    );
    let contexts = selected
        .iter()
        .filter_map(|selected| {
            let feature_id = feature_by_flow.get(&selected.flow_id)?;
            let flow = indexes.flow(&selected.flow_id)?;
            Some(to_context_flow(
                flow,
                Some(feature_id.as_str()),
                &selected.selection_reason,
                indexes,
                policy,
            ))
        })
        .collect::<Vec<_>>();

    (contexts, total_count, reasons)
}

fn feature_flow_index(
    feature_ids: &HashSet<String>,
    indexes: &PostprocessIndexes<'_>,
) -> HashMap<String, String> {
    let mut result = HashMap::new();
    for feature_id in feature_ids {
        let Some(feature) = indexes.feature(feature_id) else {
            continue;
        };
        for flow_id in &feature.flow_ids {
            if indexes.visible_flow_ids.contains(flow_id) {
                result
                    .entry(flow_id.clone())
                    .or_insert_with(|| feature_id.clone());
            }
        }
    }
    result
}

fn score_flow(
    flow: &crate::flow::ExecutionFlow,
    feature: &crate::views::overview::FeatureGroup,
    indexes: &PostprocessIndexes<'_>,
    policy: &PostprocessPolicy,
) -> SelectedFlowScore {
    let has_entrypoint = !feature.entrypoint_ids.is_empty();
    let has_resource = !feature.resource_ids.is_empty()
        || indexes
            .overview
            .resources
            .iter()
            .filter(|resource| indexes.visible_resource_ids.contains(&resource.id))
            .any(|resource| resource.unit_id == flow.owner_unit_id);
    let has_dynamic =
        !flow.dynamic_boundary_ids.is_empty() || !feature.dynamic_boundary_ids.is_empty();
    let complexity = flow.nodes.len().saturating_add(flow.edges.len());
    let score = u64::from(has_entrypoint) * u64::from(policy.flow_entrypoint_weight)
        + u64::from(has_resource) * u64::from(policy.flow_resource_weight)
        + u64::from(has_dynamic) * u64::from(policy.flow_dynamic_weight)
        + complexity as u64 * u64::from(policy.flow_complexity_weight);
    let selection_reason = if has_entrypoint {
        "entrypoint_flow"
    } else if has_resource {
        "resource_access_flow"
    } else if has_dynamic {
        "dynamic_boundary_flow"
    } else {
        "complex_internal_flow"
    };
    SelectedFlowScore {
        score,
        selection_reason: selection_reason.into(),
    }
}

fn to_context_flow(
    flow: &crate::flow::ExecutionFlow,
    feature_id: Option<&str>,
    selection_reason: &str,
    indexes: &PostprocessIndexes<'_>,
    policy: &PostprocessPolicy,
) -> ContextFlow {
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
        feature_id: feature_id.map(str::to_owned),
        owner_unit_id: flow.owner_unit_id.clone(),
        owner_name,
        steps,
        edges,
        dynamic_boundary_ids,
        selection_reason: selection_reason.into(),
    }
}

struct SelectedFlowScore {
    score: u64,
    selection_reason: String,
}
