//! 함수 내부의 순차·분기·반복 흐름을 CFG 엣지로 변환한다.

use crate::facts::{ControlFlowFact, ControlFlowKind, ResolutionStatus, SourceSpan};

use super::local_index::{contains, FlowEventIndex};
use super::model::{FlowEdge, FlowEdgeKind, FlowNode};

pub(super) fn build_local_edges(
    events: &[Event],
    entry_node_id: &str,
    exit_node_id: &str,
    edges: &mut Vec<FlowEdge>,
) {
    let event_index = FlowEventIndex::build(events);
    if events.is_empty() {
        edges.push(make_edge(
            entry_node_id,
            exit_node_id,
            FlowEdgeKind::Sequential,
            ResolutionStatus::Confirmed,
            None,
            None,
        ));
        return;
    }

    let entry_target = event_index
        .first_in_region(None)
        .map(|event| event.node().id.clone())
        .unwrap_or_else(|| exit_node_id.to_string());
    edges.push(make_edge(
        entry_node_id,
        &entry_target,
        FlowEdgeKind::Sequential,
        ResolutionStatus::Confirmed,
        None,
        None,
    ));

    for (index, event) in events.iter().enumerate() {
        match event {
            Event::Reference { node, reference } => {
                add_reference_successor(&event_index, index, node, reference, exit_node_id, edges)
            }
            Event::Control { node, fact } => match fact.kind {
                ControlFlowKind::Return => {
                    // `return load()`은 바깥 return 노드가 안쪽 호출보다
                    // 먼저 시작한다. 호출을 건너뛰고 곧바로 exit로 보내면
                    // 호출 노드가 entry에서 도달 불가능해지므로 반환식의
                    // 첫 이벤트를 먼저 연결한다.
                    if let Some(value_target) = event_index.first_in_span(index, Some(&fact.span)) {
                        edges.push(make_edge(
                            &node.id,
                            &value_target.node().id,
                            FlowEdgeKind::Sequential,
                            ResolutionStatus::Confirmed,
                            Some("return-value".into()),
                            None,
                        ));
                        edges.push(make_edge(
                            &value_target.node().id,
                            exit_node_id,
                            FlowEdgeKind::Return,
                            ResolutionStatus::Confirmed,
                            None,
                            None,
                        ));
                    } else {
                        edges.push(make_edge(
                            &node.id,
                            exit_node_id,
                            FlowEdgeKind::Return,
                            ResolutionStatus::Confirmed,
                            None,
                            None,
                        ));
                    }
                }
                ControlFlowKind::Throw => {
                    let target = enclosing_try_handler(&event_index, index)
                        .map(|event| event.node().id.clone())
                        .unwrap_or_else(|| exit_node_id.to_string());
                    edges.push(make_edge(
                        &node.id,
                        &target,
                        FlowEdgeKind::Exception,
                        ResolutionStatus::Confirmed,
                        Some("catch".into()),
                        None,
                    ));
                }
                ControlFlowKind::Catch => {
                    let handler_target = event_index
                        .first_in_span(index, fact.body_span.as_ref())
                        .or_else(|| event_index.next_after_region(index))
                        .map(|event| event.node().id.clone())
                        .unwrap_or_else(|| exit_node_id.to_string());
                    edges.push(make_edge(
                        &node.id,
                        &handler_target,
                        FlowEdgeKind::Sequential,
                        ResolutionStatus::Confirmed,
                        Some("catch".into()),
                        None,
                    ));
                }
                ControlFlowKind::Break => {
                    let target = enclosing_loop(&event_index, index)
                        .and_then(|(_, _, loop_fact)| {
                            event_index.first_after(index, &loop_fact.span)
                        })
                        .map(|event| event.node().id.clone())
                        .unwrap_or_else(|| exit_node_id.to_string());
                    edges.push(make_edge(
                        &node.id,
                        &target,
                        FlowEdgeKind::FalseBranch,
                        ResolutionStatus::Confirmed,
                        Some("break".into()),
                        None,
                    ));
                }
                ControlFlowKind::Continue => {
                    let target = enclosing_loop(&event_index, index)
                        .map(|(loop_index, _, _)| events[loop_index].node().id.clone())
                        .unwrap_or_else(|| exit_node_id.to_string());
                    edges.push(make_edge(
                        &node.id,
                        &target,
                        FlowEdgeKind::LoopBack,
                        ResolutionStatus::Confirmed,
                        Some("continue".into()),
                        None,
                    ));
                }
                ControlFlowKind::Condition | ControlFlowKind::Switch => {
                    add_branch_edges(&event_index, index, node, fact, exit_node_id, edges)
                }
                ControlFlowKind::Loop => {
                    add_loop_edges(&event_index, index, node, fact, exit_node_id, edges)
                }
                ControlFlowKind::Try => {
                    let body_target = event_index
                        .first_in_span(index, fact.body_span.as_ref())
                        .or_else(|| event_index.next_after_region(index))
                        .map(|event| event.node().id.clone())
                        .unwrap_or_else(|| exit_node_id.to_string());
                    edges.push(make_edge(
                        &node.id,
                        &body_target,
                        FlowEdgeKind::Sequential,
                        ResolutionStatus::Confirmed,
                        Some("try".into()),
                        None,
                    ));
                    if let Some(catch_target) =
                        event_index.first_in_span(index, fact.alternative_span.as_ref())
                    {
                        edges.push(make_edge(
                            &node.id,
                            &catch_target.node().id,
                            FlowEdgeKind::Exception,
                            ResolutionStatus::Confirmed,
                            Some("catch".into()),
                            None,
                        ));
                    }
                }
            },
        }
    }
}

fn add_reference_successor(
    event_index: &FlowEventIndex<'_>,
    index: usize,
    node: &FlowNode,
    reference: &crate::facts::Reference,
    exit_node_id: &str,
    edges: &mut Vec<FlowEdge>,
) {
    let target = event_index
        .next_in_region(index)
        .or_else(|| event_index.next_after_region(index))
        .map(|event| event.node().id.clone())
        .unwrap_or_else(|| exit_node_id.to_string());
    edges.push(make_edge(
        &node.id,
        &target,
        if reference.status == ResolutionStatus::Dynamic {
            FlowEdgeKind::Dynamic
        } else {
            FlowEdgeKind::Sequential
        },
        reference.status.clone(),
        None,
        Some(reference.id.clone()),
    ));
}

/// 현재 이벤트를 포함하는 가장 안쪽의 반복문을 찾는다.
fn enclosing_loop<'a>(
    event_index: &'a FlowEventIndex<'a>,
    index: usize,
) -> Option<(usize, &'a FlowNode, &'a ControlFlowFact)> {
    let candidate_index = event_index.enclosing_loop.get(index).copied().flatten()?;
    match event_index.events.get(candidate_index)? {
        Event::Control { node, fact } => Some((candidate_index, node, fact)),
        Event::Reference { .. } => None,
    }
}

/// 현재 throw를 받을 가장 안쪽 try의 catch 노드를 찾는다.
fn enclosing_try_handler<'a>(
    event_index: &'a FlowEventIndex<'a>,
    index: usize,
) -> Option<&'a Event> {
    let try_index = event_index.enclosing_try.get(index).copied().flatten()?;
    let alternative = match event_index.events.get(try_index)? {
        Event::Control { fact, .. } => fact.alternative_span.as_ref(),
        Event::Reference { .. } => None,
    }?;
    event_index.first_in_span(try_index, Some(alternative))
}

fn add_branch_edges(
    event_index: &FlowEventIndex<'_>,
    index: usize,
    node: &FlowNode,
    fact: &ControlFlowFact,
    exit_node_id: &str,
    edges: &mut Vec<FlowEdge>,
) {
    let body_target = event_index
        .first_in_span(index, fact.body_span.as_ref())
        .or_else(|| event_index.first_after(index, &fact.span))
        .map(|event| event.node().id.clone())
        .unwrap_or_else(|| exit_node_id.to_string());
    let alternative_target = event_index
        .first_in_span(index, fact.alternative_span.as_ref())
        .or_else(|| event_index.first_after(index, &fact.span))
        .map(|event| event.node().id.clone())
        .unwrap_or_else(|| exit_node_id.to_string());

    edges.push(make_edge(
        &node.id,
        &body_target,
        FlowEdgeKind::TrueBranch,
        ResolutionStatus::Confirmed,
        Some("true".into()),
        None,
    ));
    edges.push(make_edge(
        &node.id,
        &alternative_target,
        FlowEdgeKind::FalseBranch,
        ResolutionStatus::Confirmed,
        Some("false".into()),
        None,
    ));
}

fn add_loop_edges(
    event_index: &FlowEventIndex<'_>,
    index: usize,
    node: &FlowNode,
    fact: &ControlFlowFact,
    exit_node_id: &str,
    edges: &mut Vec<FlowEdge>,
) {
    let body_range = event_index.range_in_span(index, fact.body_span.as_ref());
    let body_events = body_range
        .as_ref()
        .map(|range| &event_index.events[range.clone()])
        .unwrap_or(&[]);
    let body_target = event_index
        .first_in_span(index, fact.body_span.as_ref())
        .map(|event| event.node().id.clone())
        .unwrap_or_else(|| exit_node_id.to_string());
    let exit_target = event_index
        .first_after(index, &fact.span)
        .map(|event| event.node().id.clone())
        .unwrap_or_else(|| exit_node_id.to_string());

    edges.push(make_edge(
        &node.id,
        &body_target,
        FlowEdgeKind::LoopBody,
        ResolutionStatus::Confirmed,
        Some("repeat".into()),
        None,
    ));
    edges.push(make_edge(
        &node.id,
        &exit_target,
        FlowEdgeKind::FalseBranch,
        ResolutionStatus::Confirmed,
        Some("exit".into()),
        None,
    ));
    if let Some(last) = body_events
        .iter()
        .rev()
        .filter(|event| {
            fact.body_span
                .as_ref()
                .is_some_and(|span| event.span().is_some_and(|nested| contains(span, nested)))
        })
        .find(|event| !is_abrupt_event(event))
    {
        edges.push(make_edge(
            &last.node().id,
            &node.id,
            FlowEdgeKind::LoopBack,
            ResolutionStatus::Confirmed,
            Some("repeat".into()),
            None,
        ));
    }
}

fn is_abrupt_event(event: &Event) -> bool {
    matches!(
        event,
        Event::Control {
            fact: ControlFlowFact {
                kind: ControlFlowKind::Return
                    | ControlFlowKind::Throw
                    | ControlFlowKind::Break
                    | ControlFlowKind::Continue,
                ..
            },
            ..
        }
    )
}

pub(super) fn make_edge(
    source: &str,
    target: &str,
    kind: FlowEdgeKind,
    status: ResolutionStatus,
    label: Option<String>,
    reference_id: Option<String>,
) -> FlowEdge {
    let id = super::builder::stable_id(
        source,
        &format!(
            "edge:{target}:{kind:?}:{}:{}",
            label.as_deref().unwrap_or_default(),
            reference_id.as_deref().unwrap_or_default()
        ),
    );
    FlowEdge {
        id,
        source_node_id: source.into(),
        target_node_id: target.into(),
        kind,
        status,
        label,
        reference_id,
    }
}

pub(super) enum Event {
    Control {
        node: FlowNode,
        fact: ControlFlowFact,
    },
    Reference {
        node: FlowNode,
        reference: crate::facts::Reference,
    },
}

impl Event {
    pub(super) fn node(&self) -> &FlowNode {
        match self {
            Self::Control { node, .. } | Self::Reference { node, .. } => node,
        }
    }

    pub(super) fn span(&self) -> Option<&SourceSpan> {
        self.node().span.as_ref()
    }

    pub(super) fn start_position(&self) -> (u32, u32) {
        self.span()
            .map(|span| (span.start_line, span.start_column))
            .unwrap_or((u32::MAX, u32::MAX))
    }
}
