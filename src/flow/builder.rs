//! 정규화된 제어 흐름 사실과 호출 참조를 함수별 흐름 그래프로 만든다.

use super::index::FlowInputIndex;
use super::local::{build_local_edges, Event};
use super::model::{
    ExecutionFlow, ExecutionFlowGraph, FlowEdgeKind, FlowLink, FlowNode, FlowNodeKind,
};
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
    let flow_ids: HashMap<_, _> = flows
        .iter()
        .map(|flow| (flow.owner_unit_id.clone(), flow.id.clone()))
        .collect();

    let mut links = Vec::new();
    for flow in &mut flows {
        add_call_links(
            flow,
            &input_index,
            &flow_ids,
            &entry_nodes,
            &exit_nodes,
            &mut links,
        );
        sort_flow(flow);
    }
    flows.sort_by(|left, right| left.owner_unit_id.cmp(&right.owner_unit_id));
    let mut remaining_nodes = limits.max_flow_nodes;
    let mut remaining_edges = limits.max_flow_edges;
    for flow in &mut flows {
        truncated |= limit_flow(flow, &mut remaining_nodes, &mut remaining_edges);
    }
    // Entry/exit 노드조차 예산에 들어가지 않는 흐름은 부분 그래프로
    // 내보내면 안 된다. 존재하지 않는 entryNodeId를 프론트가 따라가게
    // 하는 대신 해당 flow를 생략하고 한도 도달 상태만 보고한다.
    flows.retain(|flow| !flow.nodes.is_empty());
    links.retain(|link| {
        flows.iter().any(|flow| {
            flow.id == link.source_flow_id
                && flow.nodes.iter().any(|node| node.id == link.source_node_id)
        }) && flows.iter().any(|flow| {
            flow.id == link.target_flow_id
                && flow
                    .nodes
                    .iter()
                    .any(|node| node.id == link.target_entry_node_id)
                && flow
                    .nodes
                    .iter()
                    .any(|node| node.id == link.target_exit_node_id)
        })
    });
    for link in &mut links {
        let return_node_exists = link.return_node_id.as_ref().is_some_and(|return_node_id| {
            flows.iter().any(|flow| {
                flow.id == link.source_flow_id
                    && flow.nodes.iter().any(|node| node.id == *return_node_id)
            })
        });
        if !return_node_exists {
            link.return_node_id = None;
        }
    }
    links.sort_by(|left, right| left.id.cmp(&right.id));
    (ExecutionFlowGraph { flows, links }, truncated)
}

fn build_flow(unit: &CodeUnit, input_index: &FlowInputIndex<'_>) -> ExecutionFlow {
    let entry_node_id = stable_id(&unit.id, "entry");
    let exit_node_id = stable_id(&unit.id, "exit");
    let mut events = Vec::new();

    for fact in input_index.control_for(&unit.id) {
        events.push(Event::Control {
            node: control_node(unit, fact),
            fact: Box::new((**fact).clone()),
        });
    }
    for reference in input_index.calls_for(&unit.id) {
        events.push(Event::Reference {
            node: reference_node(unit, reference),
            reference: (*reference).clone(),
        });
    }
    // 호출식의 span은 인자 전체를 포함한다. 단순한 시작 위치 정렬을
    // 사용하면 `combine(first(), second())`가 `combine → first → second`로
    // 보이므로, 중첩된 호출은 내부 인자를 먼저 평가하는 순서로 정렬한다.
    events.sort_by(compare_events);

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

fn compare_events(left: &Event, right: &Event) -> std::cmp::Ordering {
    if spans_contain(left.span(), right.span()) {
        return std::cmp::Ordering::Greater;
    }
    if spans_contain(right.span(), left.span()) {
        return std::cmp::Ordering::Less;
    }
    left.start_position()
        .cmp(&right.start_position())
        .then_with(|| left.end_position().cmp(&right.end_position()))
        .then_with(|| left.node().id.cmp(&right.node().id))
}

fn spans_contain(
    container: Option<&crate::facts::SourceSpan>,
    nested: Option<&crate::facts::SourceSpan>,
) -> bool {
    let (Some(container), Some(nested)) = (container, nested) else {
        return false;
    };
    container.file_id == nested.file_id
        && (nested.start_line, nested.start_column)
            >= (container.start_line, container.start_column)
        && (nested.end_line, nested.end_column) <= (container.end_line, container.end_column)
        && container != nested
}

fn add_call_links(
    flow: &mut ExecutionFlow,
    input_index: &FlowInputIndex<'_>,
    flow_ids: &HashMap<String, String>,
    entry_nodes: &HashMap<String, String>,
    exit_nodes: &HashMap<String, String>,
    links: &mut Vec<FlowLink>,
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
        let Some(target_exit) = exit_nodes.get(target_unit_id) else {
            continue;
        };
        let Some(target_flow_id) = flow_ids.get(target_unit_id) else {
            continue;
        };
        let return_target = sequential_successors
            .get(*call_node_id)
            .cloned()
            .unwrap_or_else(|| flow.exit_node_id.clone());
        links.push(FlowLink {
            id: stable_id(&flow.id, &format!("link:{}", reference.id)),
            reference_id: reference.id.clone(),
            source_flow_id: flow.id.clone(),
            source_node_id: (*call_node_id).to_string(),
            target_flow_id: target_flow_id.clone(),
            target_entry_node_id: target_entry.clone(),
            target_exit_node_id: target_exit.clone(),
            return_node_id: Some(return_target),
            status: reference.status.clone(),
        });
    }
}

fn limit_flow(
    flow: &mut ExecutionFlow,
    remaining_nodes: &mut usize,
    remaining_edges: &mut usize,
) -> bool {
    let mut truncated = false;
    if *remaining_nodes < 2 {
        let had_nodes = !flow.nodes.is_empty();
        flow.nodes.clear();
        flow.edges.clear();
        flow.dynamic_boundary_ids.clear();
        return had_nodes;
    }
    let node_budget = (*remaining_nodes).min(flow.nodes.len());
    if flow.nodes.len() > node_budget {
        truncated = true;
        let mut selected = HashSet::new();
        selected.insert(flow.entry_node_id.clone());
        selected.insert(flow.exit_node_id.clone());

        let mut candidates = flow
            .nodes
            .iter()
            .filter(|node| node.id != flow.entry_node_id && node.id != flow.exit_node_id)
            .collect::<Vec<_>>();
        candidates.sort_by_key(|node| (node_priority(&node.kind), node.id.clone()));
        let additional = node_budget.saturating_sub(selected.len());
        selected.extend(
            candidates
                .into_iter()
                .take(additional)
                .map(|node| node.id.clone()),
        );
        flow.nodes.retain(|node| selected.contains(&node.id));
        flow.dynamic_boundary_ids.retain(|reference_id| {
            flow.nodes
                .iter()
                .any(|node| node.reference_id.as_deref() == Some(reference_id.as_str()))
        });
    }

    let node_ids: HashSet<_> = flow.nodes.iter().map(|node| node.id.as_str()).collect();
    flow.edges.retain(|edge| {
        node_ids.contains(edge.source_node_id.as_str())
            && node_ids.contains(edge.target_node_id.as_str())
    });
    if flow.edges.len() > *remaining_edges {
        truncated = true;
        flow.edges.truncate(*remaining_edges);
    }
    *remaining_nodes = remaining_nodes.saturating_sub(flow.nodes.len());
    *remaining_edges = remaining_edges.saturating_sub(flow.edges.len());
    truncated
}

fn node_priority(kind: &FlowNodeKind) -> u8 {
    match kind {
        FlowNodeKind::Condition | FlowNodeKind::Switch => 0,
        FlowNodeKind::Loop => 1,
        FlowNodeKind::Call | FlowNodeKind::DynamicBoundary => 2,
        FlowNodeKind::Return | FlowNodeKind::Throw => 3,
        FlowNodeKind::Break | FlowNodeKind::Continue | FlowNodeKind::Catch => 4,
        FlowNodeKind::Entry | FlowNodeKind::Exit => 5,
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
    // 노드 배열 자체가 실행 순서다. ID로 재정렬하면 안정적인 JSON은 만들 수
    // 있지만 `first() -> second() -> combine()` 같은 흐름의 의미를 잃는다.
    // 이벤트는 build_flow에서 소스 위치와 중첩 관계로 이미 결정적으로
    // 정렬했으므로 그 순서를 유지한다.
    flow.edges.sort_by(|left, right| left.id.cmp(&right.id));
    flow.edges.dedup_by(|left, right| left.id == right.id);
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
