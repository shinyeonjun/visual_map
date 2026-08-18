//! 함수 내부 이벤트의 위치와 구조적 중첩 관계를 인덱싱한다.

use crate::facts::{ControlFlowFact, ControlFlowKind, SourceSpan};

use super::local::Event;

/// `build_local_edges`가 반복해서 사용하는 이벤트 조회 인덱스다.
///
/// 이벤트 배열은 중첩된 호출을 먼저 두는 평가 순서라서 시작 위치로
/// 이진 검색하지 않는다. span 조회는 포함 관계로 선형 스캔한다.
pub(super) struct FlowEventIndex<'a> {
    pub(super) events: &'a [Event],
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
        let regions = events
            .iter()
            .enumerate()
            .map(|(index, event)| smallest_enclosing_region(events, index, event.span()))
            .collect();
        let enclosing_loop = events
            .iter()
            .enumerate()
            .map(|(index, event)| {
                event
                    .span()
                    .and_then(|span| smallest_enclosing_loop(events, index, span))
            })
            .collect();
        let enclosing_try = events
            .iter()
            .enumerate()
            .map(|(index, event)| {
                event
                    .span()
                    .and_then(|span| smallest_enclosing_try(events, index, span))
            })
            .collect();

        Self {
            events,
            regions,
            enclosing_loop,
            enclosing_try,
        }
    }

    /// 다음 이벤트가 post-test loop의 제어 노드라면 본문 첫 이벤트를
    /// 반환한다. `do { body } while (condition)`은 첫 진입에서 condition을
    /// 평가하지 않고 body부터 실행해야 한다.
    pub(super) fn next_executable(&self, index: usize) -> Option<&Event> {
        let next_index = self
            .events
            .iter()
            .enumerate()
            .skip(index + 1)
            .find(|(candidate, _)| self.regions[*candidate] == self.regions[index])
            .map(|(candidate, _)| candidate)?;
        if let Event::Control { fact, .. } = &self.events[next_index] {
            if fact.post_test {
                return self
                    .first_in_span(next_index, fact.body_span.as_ref())
                    .or_else(|| self.events.get(next_index));
            }
        }
        self.events.get(next_index)
    }

    /// 함수 진입 시 첫 이벤트가 post-test loop라면 loop 본문으로 진입한다.
    pub(super) fn first_executable_in_region(&self, region: Option<EventRegion>) -> Option<&Event> {
        let (index, event) = match region {
            Some(region) => self
                .events
                .iter()
                .enumerate()
                .find(|(index, _)| self.regions[*index] == Some(region))?,
            None => (0, self.events.first()?),
        };
        if let Event::Control { fact, .. } = event {
            if fact.post_test {
                return self
                    .first_in_span(index, fact.body_span.as_ref())
                    .or(Some(event));
            }
        }
        Some(event)
    }

    pub(super) fn post_test_loop_for(&self, index: usize) -> Option<(usize, &ControlFlowFact)> {
        let event_span = self.events.get(index)?.span()?;
        self.events
            .iter()
            .enumerate()
            .find_map(|(loop_index, event)| {
                let Event::Control { fact, .. } = event else {
                    return None;
                };
                (fact.post_test
                    && fact
                        .condition_span
                        .as_ref()
                        .is_some_and(|condition| contains(condition, event_span)))
                .then_some((loop_index, fact.as_ref()))
            })
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
        self.events
            .iter()
            .enumerate()
            .find_map(|(candidate, event)| {
                if candidate == index {
                    return None;
                }
                let nested = event.span()?;
                contains(span, nested).then_some(event)
            })
    }

    pub(super) fn first_after(&self, index: usize, span: &SourceSpan) -> Option<&Event> {
        let end = (span.end_line, span.end_column);
        self.events
            .iter()
            .enumerate()
            .filter(|(candidate, event)| *candidate != index && event.start_position() > end)
            .min_by_key(|(_, event)| event.start_position())
            .map(|(_, event)| event)
    }

    pub(super) fn catch_in_span(&self, span: Option<&SourceSpan>) -> Option<&Event> {
        let span = span?;
        self.events.iter().find(|event| {
            matches!(event, Event::Control { fact, .. } if fact.kind == ControlFlowKind::Catch)
                && event.span().is_some_and(|nested| contains(span, nested))
        })
    }

    /// catch/finally처럼 try의 alternative 영역에 있는 이벤트가 진입해야
    /// 하는 finally 이벤트를 찾는다.
    pub(super) fn finally_after_alternative(&self, index: usize) -> Option<&Event> {
        let event_span = self.events.get(index)?.span()?;
        self.events
            .iter()
            .enumerate()
            .find_map(|(try_index, event)| {
                let Event::Control { fact, .. } = event else {
                    return None;
                };
                if fact.kind != ControlFlowKind::Try
                    || !fact
                        .alternative_span
                        .as_ref()
                        .is_some_and(|alternative| contains(alternative, event_span))
                {
                    return None;
                }
                self.first_in_span(try_index, fact.finally_span.as_ref())
            })
    }

    pub(super) fn events_in_span(
        &'a self,
        index: usize,
        span: Option<&SourceSpan>,
    ) -> Vec<&'a Event> {
        let Some(span) = span else {
            return Vec::new();
        };
        self.events
            .iter()
            .enumerate()
            .filter_map(|(candidate, event)| {
                if candidate == index {
                    return None;
                }
                let nested = event.span()?;
                contains(span, nested).then_some(event)
            })
            .collect()
    }
}

pub(super) fn contains(container: &SourceSpan, nested: &SourceSpan) -> bool {
    container.file_id == nested.file_id
        && (nested.start_line, nested.start_column)
            >= (container.start_line, container.start_column)
        && (nested.end_line, nested.end_column) <= (container.end_line, container.end_column)
}

fn smallest_enclosing_loop(
    events: &[Event],
    event_index: usize,
    event_span: &SourceSpan,
) -> Option<usize> {
    events
        .iter()
        .enumerate()
        .filter_map(|(owner_index, event)| {
            if owner_index == event_index {
                return None;
            }
            let Event::Control { fact, .. } = event else {
                return None;
            };
            if fact.kind != ControlFlowKind::Loop {
                return None;
            }
            let body = fact.body_span.as_ref()?;
            contains(body, event_span).then_some((span_size(body), owner_index))
        })
        .min_by_key(|(size, _)| *size)
        .map(|(_, owner_index)| owner_index)
}

fn smallest_enclosing_try(
    events: &[Event],
    event_index: usize,
    event_span: &SourceSpan,
) -> Option<usize> {
    events
        .iter()
        .enumerate()
        .filter_map(|(owner_index, event)| {
            if owner_index == event_index {
                return None;
            }
            let Event::Control { fact, .. } = event else {
                return None;
            };
            if fact.kind != ControlFlowKind::Try {
                return None;
            }
            let body = fact.body_span.as_ref()?;
            contains(body, event_span).then_some((span_size(body), owner_index))
        })
        .min_by_key(|(size, _)| *size)
        .map(|(_, owner_index)| owner_index)
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
            } else if fact
                .finally_span
                .as_ref()
                .is_some_and(|finally| contains(finally, event_span))
            {
                (fact.finally_span.as_ref()?, true)
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
