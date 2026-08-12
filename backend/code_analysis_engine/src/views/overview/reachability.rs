//! 호출 그래프의 도달 가능 집합을 한 번 계산해 Overview 기능들이 공유한다.
//!
//! 각 entrypoint마다 문자열 ID 기반 BFS를 반복하면 파일 간 결합도가 높은
//! 프로젝트에서 같은 그래프를 계속 순회하게 된다. 먼저 순환 호출을 SCC로
//! 축약한 뒤 DAG의 도달 집합을 뒤에서부터 메모이제이션한다.

use crate::facts::{FactStore, ReferenceKind};
use std::collections::{HashMap, VecDeque};

pub(super) struct ReachabilityIndex {
    unit_ids: Vec<String>,
    unit_indexes: HashMap<String, usize>,
    component_of: Vec<usize>,
    reachable_by_component: Vec<Vec<usize>>,
}

impl ReachabilityIndex {
    pub(super) fn build(facts: &FactStore) -> Self {
        let unit_ids: Vec<String> = facts.units.keys().cloned().collect();
        let unit_indexes = unit_ids
            .iter()
            .enumerate()
            .map(|(index, id)| (id.clone(), index))
            .collect::<HashMap<_, _>>();
        let mut outgoing = vec![Vec::new(); unit_ids.len()];
        let mut incoming = vec![Vec::new(); unit_ids.len()];

        for reference in &facts.references {
            if !matches!(
                reference.kind,
                ReferenceKind::Call | ReferenceKind::Constructs
            ) {
                continue;
            }
            let Some(&source) = unit_indexes.get(&reference.source_unit_id) else {
                continue;
            };
            let Some(target) = reference
                .target_unit_id
                .as_deref()
                .and_then(|id| unit_indexes.get(id).copied())
            else {
                continue;
            };
            outgoing[source].push(target);
            incoming[target].push(source);
        }

        for neighbors in &mut outgoing {
            neighbors.sort_unstable();
            neighbors.dedup();
        }
        for neighbors in &mut incoming {
            neighbors.sort_unstable();
            neighbors.dedup();
        }

        let finish_order = finish_order(&outgoing);
        let (component_of, component_units) =
            strongly_connected_components(&incoming, &finish_order);
        let component_count = component_units.len();
        let mut component_outgoing = vec![Vec::new(); component_count];
        for (source, targets) in outgoing.iter().enumerate() {
            for &target in targets {
                let source_component = component_of[source];
                let target_component = component_of[target];
                if source_component != target_component {
                    component_outgoing[source_component].push(target_component);
                }
            }
        }
        for neighbors in &mut component_outgoing {
            neighbors.sort_unstable();
            neighbors.dedup();
        }

        let topological_order = topological_order(&component_outgoing);
        let mut reachable_by_component = component_units;
        for &component in topological_order.iter().rev() {
            let children = component_outgoing[component].clone();
            for child in children {
                let child_reachable = reachable_by_component[child].clone();
                merge_sorted(&mut reachable_by_component[component], &child_reachable);
            }
        }

        Self {
            unit_ids,
            unit_indexes,
            component_of,
            reachable_by_component,
        }
    }

    pub(super) fn unit_index(&self, unit_id: &str) -> Option<usize> {
        self.unit_indexes.get(unit_id).copied()
    }

    pub(super) fn unit_id(&self, index: usize) -> &str {
        &self.unit_ids[index]
    }

    pub(super) fn reachable_from(&self, unit_id: &str) -> &[usize] {
        self.unit_index(unit_id)
            .map(|index| self.reachable_by_component[self.component_of[index]].as_slice())
            .unwrap_or(&[])
    }

    pub(super) fn reachable_from_many<'a, I>(&self, unit_ids: I) -> Vec<usize>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut result = Vec::new();
        for unit_id in unit_ids {
            merge_sorted(&mut result, self.reachable_from(unit_id));
        }
        result
    }
}

fn finish_order(outgoing: &[Vec<usize>]) -> Vec<usize> {
    let mut visited = vec![false; outgoing.len()];
    let mut order = Vec::with_capacity(outgoing.len());
    for start in 0..outgoing.len() {
        if visited[start] {
            continue;
        }
        let mut pending = vec![(start, false)];
        while let Some((node, expanded)) = pending.pop() {
            if expanded {
                order.push(node);
                continue;
            }
            if visited[node] {
                continue;
            }
            visited[node] = true;
            pending.push((node, true));
            for &child in outgoing[node].iter().rev() {
                if !visited[child] {
                    pending.push((child, false));
                }
            }
        }
    }
    order
}

fn strongly_connected_components(
    incoming: &[Vec<usize>],
    finish_order: &[usize],
) -> (Vec<usize>, Vec<Vec<usize>>) {
    let mut component_of = vec![usize::MAX; incoming.len()];
    let mut components = Vec::new();
    for &start in finish_order.iter().rev() {
        if component_of[start] != usize::MAX {
            continue;
        }
        let component = components.len();
        let mut units = Vec::new();
        let mut pending = vec![start];
        component_of[start] = component;
        while let Some(node) = pending.pop() {
            units.push(node);
            for &parent in &incoming[node] {
                if component_of[parent] == usize::MAX {
                    component_of[parent] = component;
                    pending.push(parent);
                }
            }
        }
        units.sort_unstable();
        components.push(units);
    }
    (component_of, components)
}

fn topological_order(outgoing: &[Vec<usize>]) -> Vec<usize> {
    let mut indegree = vec![0; outgoing.len()];
    for neighbors in outgoing {
        for &target in neighbors {
            indegree[target] += 1;
        }
    }
    let mut pending = VecDeque::new();
    for (component, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            pending.push_back(component);
        }
    }
    let mut order = Vec::with_capacity(outgoing.len());
    while let Some(component) = pending.pop_front() {
        order.push(component);
        for &target in &outgoing[component] {
            indegree[target] -= 1;
            if indegree[target] == 0 {
                pending.push_back(target);
            }
        }
    }
    order
}

fn merge_sorted(target: &mut Vec<usize>, source: &[usize]) {
    if source.is_empty() {
        return;
    }
    if target.is_empty() {
        target.extend_from_slice(source);
        return;
    }
    let mut merged = Vec::with_capacity(target.len() + source.len());
    let mut left = 0;
    let mut right = 0;
    while left < target.len() || right < source.len() {
        let next = match (target.get(left), source.get(right)) {
            (Some(&left_value), Some(&right_value)) => {
                if left_value <= right_value {
                    left += 1;
                    left_value
                } else {
                    right += 1;
                    right_value
                }
            }
            (Some(&left_value), None) => {
                left += 1;
                left_value
            }
            (None, Some(&right_value)) => {
                right += 1;
                right_value
            }
            (None, None) => break,
        };
        if merged.last().copied() != Some(next) {
            merged.push(next);
        }
    }
    *target = merged;
}

#[cfg(test)]
mod tests {
    use super::merge_sorted;

    #[test]
    fn 정렬된_도달집합을_중복없이_병합한다() {
        let mut target = vec![1, 3, 7];
        merge_sorted(&mut target, &[2, 3, 4, 7, 9]);
        assert_eq!(target, [1, 2, 3, 4, 7, 9]);
    }
}
