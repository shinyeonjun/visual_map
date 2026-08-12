//! 정규화된 제어 흐름 사실과 호출 참조를 함수별 흐름 그래프로 만든다.

use super::index::FlowInputIndex;
use super::local::{build_local_edges, make_edge, Event};
use super::model::{ExecutionFlow, ExecutionFlowGraph, FlowEdgeKind, FlowNode, FlowNodeKind};
use crate::config::AnalysisLimits;
use crate::facts::{
    CodeUnit, CodeUnitKind, ControlFlowFact, ControlFlowKind, FactStore, Reference,
    ResolutionStatus,
};
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

pub(crate) fn build(facts: &FactStore, limits: &AnalysisLimits) -> (ExecutionFlowGraph, bool) {
    let input_index = FlowInputIndex::new(facts);
    let mut units: Vec<_> = facts
        .units
        .values()
        .filter(|unit| is_flow_unit(&unit.kind))
        .collect();
    let mut truncated = units.len() > limits.max_execution_flows;
    units.truncate(limits.max_execution_flows);
    let mut flows: Vec<_> = units
        .par_iter()
        .map(|unit| build_flow(unit, &input_index))
        .collect();

    let entry_nodes: HashMap<_, _> = flows
        .iter()
        .map(|flow| (flow.owner_unit_id.clone(), flow.entry_node_id.clone()))
        .collect();
    let exit_nodes: HashMap<_, _> = flows
        .iter()
        .map(|flow| (flow.owner_unit_id.clone(), flow.exit_node_id.clone()))
        .collect();

    for flow in &mut flows {
        add_call_edges(flow, &input_index, &entry_nodes, &exit_nodes);
        sort_flow(flow);
    }
    flows.sort_by(|left, right| left.owner_unit_id.cmp(&right.owner_unit_id));
    let mut remaining_nodes = limits.max_flow_nodes;
    let mut remaining_edges = limits.max_flow_edges;
    for flow in &mut flows {
        if flow.nodes.len() > remaining_nodes {
            truncated = true;
            flow.nodes.truncate(remaining_nodes);
        }
        let node_ids: HashSet<_> = flow.nodes.iter().map(|node| node.id.as_str()).collect();
        if flow.edges.len() > remaining_edges {
            truncated = true;
            flow.edges.truncate(remaining_edges);
        }
        flow.edges.retain(|edge| {
            node_ids.contains(edge.source_node_id.as_str())
                && node_ids.contains(edge.target_node_id.as_str())
        });
        remaining_nodes = remaining_nodes.saturating_sub(flow.nodes.len());
        remaining_edges = remaining_edges.saturating_sub(flow.edges.len());
    }
    (ExecutionFlowGraph { flows }, truncated)
}

fn build_flow(unit: &CodeUnit, input_index: &FlowInputIndex<'_>) -> ExecutionFlow {
    let entry_node_id = stable_id(&unit.id, "entry");
    let exit_node_id = stable_id(&unit.id, "exit");
    let mut events = Vec::new();

    for fact in input_index.control_for(&unit.id) {
        events.push(Event::Control {
            node: control_node(unit, fact),
            fact: (*fact).clone(),
        });
    }
    for reference in input_index.calls_for(&unit.id) {
        events.push(Event::Reference {
            node: reference_node(unit, reference),
            reference: (*reference).clone(),
        });
    }
    events.sort_by_key(Event::start_position);

    let mut nodes = vec![
        entry_node(unit, &entry_node_id),
        exit_node(unit, &exit_node_id),
    ];
    nodes.extend(events.iter().map(Event::node).cloned());
    let mut edges = Vec::new();
    build_local_edges(&events, &entry_node_id, &exit_node_id, &mut edges);

    let dynamic_boundary_ids = events
        .iter()
        .filter_map(|event| match event {
            Event::Reference { reference, .. } if reference.status == ResolutionStatus::Dynamic => {
                Some(reference.id.clone())
            }
            _ => None,
        })
        .collect();

    ExecutionFlow {
        id: stable_id(&unit.id, "flow"),
        owner_unit_id: unit.id.clone(),
        entry_node_id,
        exit_node_id,
        nodes,
        edges,
        dynamic_boundary_ids,
    }
}

fn add_call_edges(
    flow: &mut ExecutionFlow,
    input_index: &FlowInputIndex<'_>,
    entry_nodes: &HashMap<String, String>,
    exit_nodes: &HashMap<String, String>,
) {
    let node_by_reference: HashMap<&str, &str> = flow
        .nodes
        .iter()
        .filter_map(|node| {
            node.reference_id
                .as_deref()
                .map(|reference_id| (reference_id, node.id.as_str()))
        })
        .collect();
    let sequential_successors: HashMap<String, String> = flow
        .edges
        .iter()
        .filter(|edge| edge.kind == FlowEdgeKind::Sequential)
        .map(|edge| (edge.source_node_id.clone(), edge.target_node_id.clone()))
        .collect();

    for reference in input_index.calls_for(&flow.owner_unit_id) {
        let Some(target_unit_id) = reference.target_unit_id.as_deref() else {
            continue;
        };
        let Some(call_node_id) = node_by_reference.get(reference.id.as_str()) else {
            continue;
        };
        let Some(target_entry) = entry_nodes.get(target_unit_id) else {
            continue;
        };
        flow.edges.push(make_edge(
            call_node_id,
            target_entry,
            FlowEdgeKind::Call,
            reference.status.clone(),
            None,
            Some(reference.id.clone()),
        ));

        if let Some(target_exit) = exit_nodes.get(target_unit_id) {
            let return_target = sequential_successors
                .get(*call_node_id)
                .cloned()
                .unwrap_or_else(|| flow.exit_node_id.clone());
            flow.edges.push(make_edge(
                target_exit,
                &return_target,
                FlowEdgeKind::Return,
                reference.status.clone(),
                None,
                Some(reference.id.clone()),
            ));
        }
    }
}

fn control_node(unit: &CodeUnit, fact: &ControlFlowFact) -> FlowNode {
    let (kind, fallback_label) = match fact.kind {
        ControlFlowKind::Condition => (FlowNodeKind::Condition, "condition"),
        ControlFlowKind::Switch => (FlowNodeKind::Switch, "switch"),
        ControlFlowKind::Loop => (FlowNodeKind::Loop, "loop"),
        ControlFlowKind::Return => (FlowNodeKind::Return, "return"),
        ControlFlowKind::Throw => (FlowNodeKind::Throw, "throw"),
        ControlFlowKind::Break => (FlowNodeKind::Break, "break"),
        ControlFlowKind::Continue => (FlowNodeKind::Continue, "continue"),
        ControlFlowKind::Try => (FlowNodeKind::Condition, "try"),
        ControlFlowKind::Catch => (FlowNodeKind::Catch, "catch"),
    };
    FlowNode {
        id: stable_id(&unit.id, &format!("fact:{}", fact.id)),
        owner_unit_id: unit.id.clone(),
        kind,
        span: Some(fact.span.clone()),
        label: fact
            .condition
            .clone()
            .unwrap_or_else(|| fallback_label.into()),
        reference_id: None,
        target_unit_id: None,
    }
}

fn reference_node(unit: &CodeUnit, reference: &Reference) -> FlowNode {
    FlowNode {
        id: stable_id(&unit.id, &format!("reference:{}", reference.id)),
        owner_unit_id: unit.id.clone(),
        kind: if reference.status == ResolutionStatus::Dynamic {
            FlowNodeKind::DynamicBoundary
        } else {
            FlowNodeKind::Call
        },
        span: reference
            .evidence
            .first()
            .map(|evidence| evidence.span.clone()),
        label: reference.target_name.clone(),
        reference_id: Some(reference.id.clone()),
        target_unit_id: reference.target_unit_id.clone(),
    }
}

fn entry_node(unit: &CodeUnit, id: &str) -> FlowNode {
    FlowNode {
        id: id.into(),
        owner_unit_id: unit.id.clone(),
        kind: FlowNodeKind::Entry,
        span: None,
        label: "entry".into(),
        reference_id: None,
        target_unit_id: None,
    }
}

fn exit_node(unit: &CodeUnit, id: &str) -> FlowNode {
    FlowNode {
        id: id.into(),
        owner_unit_id: unit.id.clone(),
        kind: FlowNodeKind::Exit,
        span: None,
        label: "exit".into(),
        reference_id: None,
        target_unit_id: None,
    }
}

fn sort_flow(flow: &mut ExecutionFlow) {
    flow.nodes.sort_by(|left, right| left.id.cmp(&right.id));
    flow.edges.sort_by(|left, right| left.id.cmp(&right.id));
    flow.dynamic_boundary_ids.sort();
}

fn is_flow_unit(kind: &CodeUnitKind) -> bool {
    matches!(
        kind,
        CodeUnitKind::Function
            | CodeUnitKind::Method
            | CodeUnitKind::Constructor
            | CodeUnitKind::Lambda
    )
}

pub(super) fn stable_id(owner: &str, suffix: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(owner.as_bytes());
    hasher.update([0]);
    hasher.update(suffix.as_bytes());
    let hex = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("flow_{}", &hex[..24])
}
