//! 함수 내부의 순차·분기·반복 흐름을 CFG 엣지로 변환한다.

use crate::facts::{ControlFlowFact, ControlFlowKind, ResolutionStatus, SourceSpan};

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

    edges.push(make_edge(
        entry_node_id,
        &events[0].node().id,
        FlowEdgeKind::Sequential,
        ResolutionStatus::Confirmed,
        None,
        None,
    ));

    for (index, event) in events.iter().enumerate() {
        let next = events
            .get(index + 1)
            .map(|event| event.node().id.as_str())
            .unwrap_or(exit_node_id);
        match event {
            Event::Reference { node, reference } => edges.push(make_edge(
                &node.id,
                next,
                if reference.status == ResolutionStatus::Dynamic {
                    FlowEdgeKind::Dynamic
                } else {
                    FlowEdgeKind::Sequential
                },
                reference.status.clone(),
                None,
                Some(reference.id.clone()),
            )),
            Event::Control { node, fact } => match fact.kind {
                ControlFlowKind::Return => edges.push(make_edge(
                    &node.id,
                    exit_node_id,
                    FlowEdgeKind::Return,
                    ResolutionStatus::Confirmed,
                    None,
                    None,
                )),
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
                        .or_else(|| event_index.first_after(index, &fact.span))
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
                        .map(|event| event.node().id.clone())
                        .unwrap_or_else(|| next.to_string());
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

    let last_index = events.len() - 1;
    let last = &events[last_index];
    let terminal = is_abrupt_event(last)
        || matches!(
            last,
            Event::Control {
                fact: ControlFlowFact {
                    kind: ControlFlowKind::Loop,
                    ..
                },
                ..
            }
        );
    let inside_loop = enclosing_loop(&event_index, last_index).is_some();
    if !terminal && !inside_loop {
        edges.push(make_edge(
            &last.node().id,
            exit_node_id,
            FlowEdgeKind::Sequential,
            ResolutionStatus::Confirmed,
            None,
            None,
        ));
    }
}

/// 현재 이벤트를 포함하는 가장 안쪽의 반복문을 찾는다.
///
/// 이벤트를 평탄화한 뒤에도 각 사실의 소스 스팬을 이용하면 중첩 반복문을
/// 구분할 수 있다. 가장 작은 본문 스팬을 고르는 것이 가장 안쪽 반복문이다.
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

fn contains(container: &SourceSpan, nested: &SourceSpan) -> bool {
    container.file_id == nested.file_id
        && (nested.start_line, nested.start_column)
            >= (container.start_line, container.start_column)
        && (nested.end_line, nested.end_column) <= (container.end_line, container.end_column)
}

/// 이벤트 위치와 구조적 중첩 관계를 한 번 계산한 인덱스다.
///
/// 기존 구현은 `first_after`, `first_in_span`, `enclosing_loop`가 호출될
/// 때마다 전체 이벤트 배열을 선형 검색했다. 위치는 이미 시작점 순으로
/// 정렬되어 있으므로 이진 검색과 중첩 스택으로 조회 비용을 줄인다.
struct FlowEventIndex<'a> {
    events: &'a [Event],
    starts: Vec<(u32, u32)>,
    enclosing_loop: Vec<Option<usize>>,
    enclosing_try: Vec<Option<usize>>,
}

impl<'a> FlowEventIndex<'a> {
    fn build(events: &'a [Event]) -> Self {
        let starts = events.iter().map(Event::start_position).collect::<Vec<_>>();
        let mut enclosing_loop = vec![None; events.len()];
        let mut enclosing_try = vec![None; events.len()];
        let mut active_loops = Vec::new();
        let mut active_tries = Vec::new();

        for index in 0..events.len() {
            let current_start = starts[index];
            pop_finished(events, &mut active_loops, current_start, |event| {
                loop_body_span(event)
            });
            pop_finished(events, &mut active_tries, current_start, |event| {
                try_body_span(event)
            });

            if let Some(current_span) = events[index].span() {
                enclosing_loop[index] = active_loops.iter().rev().copied().find(|candidate| {
                    loop_body_span(&events[*candidate])
                        .is_some_and(|body| contains(body, current_span))
                });
                enclosing_try[index] = active_tries.iter().rev().copied().find(|candidate| {
                    try_body_span(&events[*candidate])
                        .is_some_and(|body| contains(body, current_span))
                });
            }

            match &events[index] {
                Event::Control { fact, .. } if fact.kind == ControlFlowKind::Loop => {
                    active_loops.push(index);
                }
                Event::Control { fact, .. } if fact.kind == ControlFlowKind::Try => {
                    active_tries.push(index);
                }
                _ => {}
            }
        }

        Self {
            events,
            starts,
            enclosing_loop,
            enclosing_try,
        }
    }

    fn first_in_span(&self, index: usize, span: Option<&SourceSpan>) -> Option<&Event> {
        let span = span?;
        let mut candidate = self
            .starts
            .partition_point(|position| *position < (span.start_line, span.start_column))
            .max(index + 1);
        let end = self
            .starts
            .partition_point(|position| *position <= (span.end_line, span.end_column));
        while candidate < end {
            if self.events[candidate]
                .span()
                .is_some_and(|nested| contains(span, nested))
            {
                return self.events.get(candidate);
            }
            candidate += 1;
        }
        None
    }

    fn first_after(&self, index: usize, span: &SourceSpan) -> Option<&Event> {
        let candidate = self
            .starts
            .partition_point(|position| *position <= (span.end_line, span.end_column))
            .max(index + 1);
        self.events.get(candidate)
    }

    fn range_in_span(
        &self,
        index: usize,
        span: Option<&SourceSpan>,
    ) -> Option<std::ops::Range<usize>> {
        let span = span?;
        let start = self
            .starts
            .partition_point(|position| *position < (span.start_line, span.start_column))
            .max(index + 1);
        let end = self
            .starts
            .partition_point(|position| *position <= (span.end_line, span.end_column));
        (start < end).then_some(start..end)
    }
}

fn pop_finished<F>(
    events: &[Event],
    active: &mut Vec<usize>,
    current_start: (u32, u32),
    body_span: F,
) where
    F: Fn(&Event) -> Option<&SourceSpan>,
{
    while let Some(&candidate) = active.last() {
        let Some(body) = body_span(&events[candidate]) else {
            active.pop();
            continue;
        };
        if current_start > (body.end_line, body.end_column) {
            active.pop();
        } else {
            break;
        }
    }
}

fn loop_body_span(event: &Event) -> Option<&SourceSpan> {
    match event {
        Event::Control { fact, .. } if fact.kind == ControlFlowKind::Loop => {
            fact.body_span.as_ref()
        }
        _ => None,
    }
}

fn try_body_span(event: &Event) -> Option<&SourceSpan> {
    match event {
        Event::Control { fact, .. } if fact.kind == ControlFlowKind::Try => fact.body_span.as_ref(),
        _ => None,
    }
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
            "edge:{target}:{kind:?}:{}",
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
