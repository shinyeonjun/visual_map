use crate::models::{MapFlow, MapFlowEdge, MapFlowNode};
use std::collections::HashMap;

use super::clean::{FeatureJson, FlowJson, UnitJson};
use super::status::{edge_status, is_boundary_flow_kind, node_status};

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
            let (nodes, edges) = project_flow_graph(flow);
            MapFlow {
                id: flow.id.clone(),
                owner,
                status: flow_status(flow, &nodes),
                entry_node_id: flow.entry_node_id.clone(),
                nodes,
                edges,
            }
        })
        .collect()
}

fn project_flow_graph(flow: &FlowJson) -> (Vec<MapFlowNode>, Vec<MapFlowEdge>) {
    let visible_node_ids: std::collections::HashSet<&str> = flow
        .nodes
        .iter()
        .filter(|node| !is_boundary_flow_kind(&node.kind))
        .map(|node| node.id.as_str())
        .collect();

    let nodes = flow
        .nodes
        .iter()
        .filter(|node| visible_node_ids.contains(node.id.as_str()))
        .map(|node| MapFlowNode {
            id: node.id.clone(),
            label: node.label.clone(),
            kind: node.kind.clone(),
            status: node_status(flow, node),
        })
        .collect();

    let edges = flow
        .edges
        .iter()
        .filter(|edge| {
            let source_visible = visible_node_ids.contains(edge.source_node_id.as_str());
            let target_visible = visible_node_ids.contains(edge.target_node_id.as_str());
            let from_entry = edge.source_node_id == flow.entry_node_id;
            (source_visible && target_visible) || (from_entry && target_visible)
        })
        .map(|edge| MapFlowEdge {
            source_node_id: edge.source_node_id.clone(),
            target_node_id: edge.target_node_id.clone(),
            kind: edge
                .kind
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("sequential")
                .to_string(),
            status: edge_status(edge.status.as_deref()),
            label: edge.label.clone(),
        })
        .collect();

    (nodes, edges)
}

fn flow_status(flow: &FlowJson, nodes: &[MapFlowNode]) -> String {
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
    if nodes.iter().any(|node| node.status == "candidate") {
        return "candidate".to_string();
    }
    "verified".to_string()
}
