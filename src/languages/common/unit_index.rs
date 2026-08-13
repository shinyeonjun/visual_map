//! 파일 내 코드 유닛 위치를 구간 트리로 조회하는 인덱스다.

use crate::facts::{CodeUnit, CodeUnitKind};

#[derive(Debug, Clone)]
pub(crate) struct UnitSpanIndex {
    spans: Vec<UnitSpan>,
    max_end_tree: Vec<u32>,
    tree_size: usize,
    fallback_id: String,
}

#[derive(Debug, Clone)]
struct UnitSpan {
    start_line: u32,
    end_line: u32,
    unit_id: String,
}

impl UnitSpanIndex {
    pub(crate) fn build(units: &[CodeUnit]) -> Self {
        let mut spans = units
            .iter()
            .filter(|unit| unit.kind != CodeUnitKind::File)
            .map(|unit| UnitSpan {
                start_line: unit.span.start_line,
                end_line: unit.span.end_line,
                unit_id: unit.id.clone(),
            })
            .collect::<Vec<_>>();
        spans.sort_by_key(|span| (span.start_line, span.end_line));

        let tree_size = spans.len().next_power_of_two().max(1);
        let mut max_end_tree = vec![0; tree_size * 2];
        for (index, span) in spans.iter().enumerate() {
            max_end_tree[tree_size + index] = span.end_line;
        }
        for index in (1..tree_size).rev() {
            max_end_tree[index] = max_end_tree[index * 2].max(max_end_tree[index * 2 + 1]);
        }

        let fallback_id = units
            .iter()
            .find(|unit| unit.kind == CodeUnitKind::File)
            .or_else(|| units.first())
            .map(|unit| unit.id.clone())
            .unwrap_or_default();
        Self {
            spans,
            max_end_tree,
            tree_size,
            fallback_id,
        }
    }

    pub(crate) fn unit_for_line(&self, line: u32) -> String {
        let upper_bound = self.spans.partition_point(|span| span.start_line <= line);
        let index = self.find_rightmost(1, 0, self.tree_size, upper_bound, line);
        if let Some(index) = index {
            return self.spans[index].unit_id.clone();
        }
        // Java annotation, C# attribute, Rust attribute처럼 선언 바로
        // 앞에 놓이는 문법은 선언 유닛의 span 밖에 있다. 가까운 다음
        // 선언에 귀속시켜 route·진입점이 파일 유닛으로 뭉개지지 않게 한다.
        let next = self
            .spans
            .partition_point(|span| span.start_line < line)
            .min(self.spans.len());
        if next < self.spans.len() && self.spans[next].start_line.saturating_sub(line) <= 8 {
            return self.spans[next].unit_id.clone();
        }
        self.fallback_id.clone()
    }

    pub(crate) fn unit_for_annotation_line(&self, line: u32) -> String {
        let upper_bound = self.spans.partition_point(|span| span.start_line <= line);
        // 여러 annotation이 하나의 선언 앞에 연속해서 놓이면 AST 선언 span은
        // 첫 annotation부터 시작한다. 두 번째 annotation을 다음 메서드로
        // 넘기지 말고, 현재 줄을 실제로 포함하는 가장 안쪽 선언을 우선한다.
        if let Some(index) = self.find_rightmost(1, 0, self.tree_size, upper_bound, line) {
            return self.spans[index].unit_id.clone();
        }
        let next = self
            .spans
            .partition_point(|span| span.start_line <= line)
            .min(self.spans.len());
        self.spans
            .get(next)
            .map(|span| span.unit_id.clone())
            .unwrap_or_else(|| self.unit_for_line(line))
    }

    fn find_rightmost(
        &self,
        node: usize,
        left: usize,
        right: usize,
        upper_bound: usize,
        line: u32,
    ) -> Option<usize> {
        if left >= upper_bound || self.max_end_tree[node] < line {
            return None;
        }
        if right - left == 1 {
            return (left < self.spans.len()).then_some(left);
        }
        let middle = (left + right) / 2;
        self.find_rightmost(node * 2 + 1, middle, right, upper_bound, line)
            .or_else(|| self.find_rightmost(node * 2, left, middle, upper_bound, line))
    }
}
