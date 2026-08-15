use crate::models::{MapFlow, MapFlowStep};
use std::collections::{HashMap, HashSet, VecDeque};

use super::clean::{FeatureJson, FlowJson, UnitJson};
use super::status::{is_boundary_flow_kind, step_status};

pub(crate) fn build_feature_flows(
    feature: &FeatureJson,
    flows_by_id: &HashMap<&str, &FlowJson>,
    units_by_id: &HashMap<&str, &UnitJson>,
) -> Vec<MapFlow> {
    feature
        .flow_ids
        .iter()
        .filter_map(|flow_id| flows_by_id.get(flow_id.as_str()).copied())
        .map(|flow| {
            let owner = units_by_id
                .get(flow.owner_unit_id.as_str())
                .map(|unit| unit.qualified_name.clone())
                .filter(|value| !value.trim().is_empty())
                .or_else(|| {
                    units_by_id
                        .get(flow.owner_unit_id.as_str())
                        .map(|unit| unit.name.clone())
                })
                .unwrap_or_else(|| flow.owner_unit_id.clone());
            let steps = ordered_flow_steps(flow);
            MapFlow {
                id: flow.id.clone(),
                owner,
                status: flow_status(flow, &steps),
                steps,
            }
        })
        .collect()
}

fn ordered_flow_steps(flow: &FlowJson) -> Vec<MapFlowStep> {
    let nodes_by_id: HashMap<&str, &super::clean::FlowNodeJson> = flow
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let mut next: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
    for edge in &flow.edges {
        let status = edge.status.as_deref().unwrap_or("confirmed");
        next.entry(edge.source_node_id.as_str())
            .or_default()
            .push((edge.target_node_id.as_str(), status));
    }

    let mut visited = HashSet::new();
    let mut queue = VecDeque::from([(flow.entry_node_id.as_str(), None)]);
    let mut steps = Vec::new();
    while let Some((node_id, edge_status)) = queue.pop_front() {
        if !visited.insert(node_id) {
            continue;
        }
        let Some(node) = nodes_by_id.get(node_id) else {
            continue;
        };
        if !is_boundary_flow_kind(&node.kind) {
            steps.push(MapFlowStep {
                label: node.label.clone(),
                kind: node.kind.clone(),
                status: step_status(&node.kind, edge_status),
            });
        }
        if let Some(children) = next.get(node_id) {
            for (child, status) in children {
                if !visited.contains(child) {
                    queue.push_back((child, Some(*status)));
                }
            }
        }
    }
    steps
}

fn flow_status(flow: &FlowJson, steps: &[MapFlowStep]) -> String {
    if !flow.dynamic_boundary_ids.is_empty() {
        return "candidate".to_string();
    }
    if flow.edges.iter().any(|edge| {
        edge.status
            .as_deref()
            .is_some_and(|status| status != "confirmed")
    }) {
        return "candidate".to_string();
    }
    if steps.iter().any(|step| step.status == "candidate") {
        return "candidate".to_string();
    }
    "verified".to_string()
}
