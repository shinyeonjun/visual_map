//! 함수 내부 이벤트의 위치와 구조적 중첩 관계를 인덱싱한다.

use crate::facts::{ControlFlowKind, SourceSpan};

use super::local::Event;

/// `build_local_edges`가 반복해서 사용하는 이벤트 조회 인덱스다.
///
/// 이벤트는 시작 위치 순으로 정렬되어 있으므로 위치 조회는 이진 검색으로
/// 수행하고, 반복문·try의 활성 영역은 한 번만 스캔해서 계산한다.
pub(super) struct FlowEventIndex<'a> {
    pub(super) events: &'a [Event],
    starts: Vec<(u32, u32)>,
    regions: Vec<Option<EventRegion>>,
    pub(super) enclosing_loop: Vec<Option<usize>>,
    pub(super) enclosing_try: Vec<Option<usize>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct EventRegion {
    owner_index: usize,
    #[allow(dead_code)]
    alternative: bool,
}

impl<'a> FlowEventIndex<'a> {
    pub(super) fn build(events: &'a [Event]) -> Self {
        let starts = events.iter().map(Event::start_position).collect::<Vec<_>>();
        let regions = events
            .iter()
            .enumerate()
            .map(|(index, event)| smallest_enclosing_region(events, index, event.span()))
            .collect();
        let mut enclosing_loop = vec![None; events.len()];
        let mut enclosing_try = vec![None; events.len()];
        let mut active_loops = Vec::new();
        let mut active_tries = Vec::new();

        for index in 0..events.len() {
            let current_start = starts[index];
            pop_finished(events, &mut active_loops, current_start, loop_body_span);
            pop_finished(events, &mut active_tries, current_start, try_body_span);

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
            regions,
            enclosing_loop,
            enclosing_try,
        }
    }

    pub(super) fn first_in_region(&self, region: Option<EventRegion>) -> Option<&Event> {
        self.events
            .iter()
            .enumerate()
            .find(|(index, _)| self.regions[*index] == region)
            .map(|(_, event)| event)
    }

    pub(super) fn next_in_region(&self, index: usize) -> Option<&Event> {
        let region = *self.regions.get(index)?;
        self.events
            .iter()
            .enumerate()
            .skip(index + 1)
            .find(|(candidate, _)| self.regions[*candidate] == region)
            .map(|(_, event)| event)
    }

    /// 현재 body/alternative의 끝에서 제어문 다음 join으로 이동한다.
    pub(super) fn next_after_region(&self, index: usize) -> Option<&Event> {
        let region = self.regions.get(index).copied().flatten()?;
        let container = self.events.get(region.owner_index)?;
        let parent_region = self.regions.get(region.owner_index).copied().flatten();
        let end = container.span()?.end_line;
        let end_column = container.span()?.end_column;
        self.events
            .iter()
            .enumerate()
            .skip(index + 1)
            .find(|(candidate, event)| {
                self.regions[*candidate] == parent_region
                    && event.start_position() > (end, end_column)
            })
            .map(|(_, event)| event)
    }

    pub(super) fn first_in_span(&self, index: usize, span: Option<&SourceSpan>) -> Option<&Event> {
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

    pub(super) fn first_after(&self, index: usize, span: &SourceSpan) -> Option<&Event> {
        let candidate = self
            .starts
            .partition_point(|position| *position <= (span.end_line, span.end_column))
            .max(index + 1);
        self.events.get(candidate)
    }

    pub(super) fn range_in_span(
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

pub(super) fn contains(container: &SourceSpan, nested: &SourceSpan) -> bool {
    container.file_id == nested.file_id
        && (nested.start_line, nested.start_column)
            >= (container.start_line, container.start_column)
        && (nested.end_line, nested.end_column) <= (container.end_line, container.end_column)
}

fn smallest_enclosing_region(
    events: &[Event],
    event_index: usize,
    event_span: Option<&SourceSpan>,
) -> Option<EventRegion> {
    let event_span = event_span?;
    events
        .iter()
        .enumerate()
        .filter_map(|(owner_index, event)| {
            let Event::Control { fact, .. } = event else {
                return None;
            };
            if owner_index == event_index {
                return None;
            }
            let (span, alternative) = if fact
                .body_span
                .as_ref()
                .is_some_and(|body| contains(body, event_span))
            {
                (fact.body_span.as_ref()?, false)
            } else if fact
                .alternative_span
                .as_ref()
                .is_some_and(|alternative| contains(alternative, event_span))
            {
                (fact.alternative_span.as_ref()?, true)
            } else {
                return None;
            };
            Some((
                span_size(span),
                EventRegion {
                    owner_index,
                    alternative,
                },
            ))
        })
        .min_by_key(|(size, _)| *size)
        .map(|(_, region)| region)
}

fn span_size(span: &SourceSpan) -> (u32, u32) {
    (
        span.end_line.saturating_sub(span.start_line),
        span.end_column.saturating_sub(span.start_column),
    )
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
