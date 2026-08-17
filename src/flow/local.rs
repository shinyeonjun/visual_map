//! 함수 내부의 순차·분기·반복 흐름을 CFG 엣지로 변환한다.

use crate::facts::{ControlFlowFact, ControlFlowKind, ResolutionStatus, SourceSpan};

use super::local_index::FlowEventIndex;
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
        .first_executable_in_region(None)
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
            Event::Reference {
                node,
                reference_id,
                status,
            } => {
                if let Some((loop_index, loop_fact)) = event_index.post_test_loop_for(index) {
                    add_post_test_condition_edges(
                        &event_index,
                        loop_index,
                        loop_fact,
                        node,
                        reference_id,
                        *status,
                        exit_node_id,
                        edges,
                    );
                } else {
                    add_reference_successor(
                        &event_index,
                        index,
                        node,
                        reference_id,
                        *status,
                        exit_node_id,
                        edges,
                    );
                }
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
                    if let Some(finally_target) = event_index.finally_after_alternative(index) {
                        edges.push(make_edge(
                            &node.id,
                            &finally_target.node().id,
                            FlowEdgeKind::Sequential,
                            ResolutionStatus::Confirmed,
                            Some("finally".into()),
                            None,
                        ));
                    }
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
                        event_index.catch_in_span(fact.alternative_span.as_ref())
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
                    if let Some(finally_target) =
                        event_index.first_in_span(index, fact.finally_span.as_ref())
                    {
                        edges.push(make_edge(
                            &node.id,
                            &finally_target.node().id,
                            FlowEdgeKind::Sequential,
                            ResolutionStatus::Confirmed,
                            Some("finally".into()),
                            None,
                        ));
                        if let Some(last_body) = event_index
                            .events_in_span(index, fact.body_span.as_ref())
                            .iter()
                            .rev()
                            .copied()
                            .find(|event| !is_abrupt_event(event))
                        {
                            edges.push(make_edge(
                                &last_body.node().id,
                                &finally_target.node().id,
                                FlowEdgeKind::Sequential,
                                ResolutionStatus::Confirmed,
                                Some("finally".into()),
                                None,
                            ));
                        }
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
    reference_id: &str,
    status: ResolutionStatus,
    exit_node_id: &str,
    edges: &mut Vec<FlowEdge>,
) {
    let target = event_index
        .next_executable(index)
        .or_else(|| event_index.next_after_region(index))
        .map(|event| event.node().id.clone())
        .unwrap_or_else(|| exit_node_id.to_string());
    edges.push(make_edge(
        &node.id,
        &target,
        if status == ResolutionStatus::Dynamic {
            FlowEdgeKind::Dynamic
        } else {
            FlowEdgeKind::Sequential
        },
        status,
        None,
        Some(reference_id.to_string()),
    ));
}

/// 현재 이벤트를 포함하는 가장 안쪽의 반복문을 찾는다.
fn enclosing_loop<'a>(
    event_index: &'a FlowEventIndex<'a>,
    index: usize,
) -> Option<(usize, &'a FlowNode, &'a ControlFlowFact)> {
    let candidate_index = event_index.enclosing_loop.get(index).copied().flatten()?;
    match event_index.events.get(candidate_index)? {
        Event::Control { node, fact } => Some((candidate_index, node, fact.as_ref())),
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
    event_index.catch_in_span(Some(alternative))
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

    let (true_target, false_target) = match fact.condition_operator.as_deref() {
        // `&&`: 우변은 좌변이 참일 때만 평가된다.
        Some("&&") => (body_target, alternative_target),
        // `||`: 우변은 좌변이 거짓일 때만 평가된다.
        Some("||") => (alternative_target, body_target),
        _ => (body_target, alternative_target),
    };

    edges.push(make_edge(
        &node.id,
        &true_target,
        FlowEdgeKind::TrueBranch,
        ResolutionStatus::Confirmed,
        Some("true".into()),
        None,
    ));
    edges.push(make_edge(
        &node.id,
        &false_target,
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
    let body_events = event_index.events_in_span(index, fact.body_span.as_ref());
    let body_target = event_index
        .first_in_span(index, fact.body_span.as_ref())
        .map(|event| event.node().id.clone())
        .unwrap_or_else(|| exit_node_id.to_string());
    let exit_target = event_index
        .first_after(index, &fact.span)
        .map(|event| event.node().id.clone())
        .unwrap_or_else(|| exit_node_id.to_string());

    if fact.post_test {
        if let Some(condition_target) = event_index
            .first_in_span(index, fact.condition_span.as_ref())
            .map(|event| event.node().id.clone())
        {
            // 본문을 한 번 실행한 뒤 조건식 이벤트를 거쳐 다음 반복 또는
            // loop 이후로 이동한다. 조건 호출 자체의 두 분기 엣지는
            // `add_post_test_condition_edges`가 생성한다.
            edges.push(make_edge(
                &node.id,
                &condition_target,
                FlowEdgeKind::Sequential,
                ResolutionStatus::Confirmed,
                Some("condition".into()),
                None,
            ));
        } else {
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
        }
    } else {
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
    }
    if let Some(last) = body_events
        .iter()
        .rev()
        .copied()
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
    matches!(event, Event::Control { fact, .. } if matches!(
        fact.kind,
        ControlFlowKind::Return
            | ControlFlowKind::Throw
            | ControlFlowKind::Break
            | ControlFlowKind::Continue
    ))
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
        fact: Box<ControlFlowFact>,
    },
    Reference {
        node: FlowNode,
        reference_id: String,
        status: ResolutionStatus,
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

    pub(super) fn end_position(&self) -> (u32, u32) {
        self.span()
            .map(|span| (span.end_line, span.end_column))
            .unwrap_or((u32::MAX, u32::MAX))
    }
}

fn add_post_test_condition_edges(
    event_index: &FlowEventIndex<'_>,
    loop_index: usize,
    loop_fact: &ControlFlowFact,
    node: &FlowNode,
    reference_id: &str,
    status: ResolutionStatus,
    exit_node_id: &str,
    edges: &mut Vec<FlowEdge>,
) {
    let body_target = event_index
        .first_in_span(loop_index, loop_fact.body_span.as_ref())
        .map(|event| event.node().id.clone())
        .unwrap_or_else(|| exit_node_id.to_string());
    let exit_target = event_index
        .first_after(loop_index, &loop_fact.span)
        .map(|event| event.node().id.clone())
        .unwrap_or_else(|| exit_node_id.to_string());
    edges.push(make_edge(
        &node.id,
        &body_target,
        FlowEdgeKind::LoopBody,
        status,
        Some("repeat".into()),
        Some(reference_id.to_string()),
    ));
    edges.push(make_edge(
        &node.id,
        &exit_target,
        if status == ResolutionStatus::Dynamic {
            FlowEdgeKind::Dynamic
        } else {
            FlowEdgeKind::FalseBranch
        },
        status,
        Some("exit".into()),
        Some(reference_id.to_string()),
    ));
}
