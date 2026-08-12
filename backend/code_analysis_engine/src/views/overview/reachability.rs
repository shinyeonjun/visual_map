//! 호출 그래프의 도달 가능 집합을 한 번 계산해 Overview 기능들이 공유한다.
//!
//! 각 entrypoint마다 문자열 ID 기반 BFS를 반복하면 파일 간 결합도가 높은
//! 프로젝트에서 같은 그래프를 계속 순회하게 된다. 먼저 순환 호출을 SCC로
//! 축약한 뒤 DAG의 도달 집합을 뒤에서부터 메모이제이션한다.

use crate::facts::{FactStore, ReferenceKind};
use roaring::RoaringBitmap;
use std::collections::{HashMap, VecDeque};

pub(super) struct ReachabilityIndex {
    unit_ids: Vec<String>,
    unit_indexes: HashMap<String, usize>,
    component_of: Vec<usize>,
    reachable_by_component: Vec<RoaringBitmap>,
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
        let mut reachable_by_component = component_units
            .into_iter()
            .map(|units| {
                let mut reachable = RoaringBitmap::new();
                for unit in units {
                    reachable.insert(
                        u32::try_from(unit)
                            .expect("코드 유닛 인덱스는 RoaringBitmap 범위 안이어야 한다"),
                    );
                }
                reachable
            })
            .collect::<Vec<_>>();
        for &component in topological_order.iter().rev() {
            for &child in &component_outgoing[component] {
                union_component_sets(&mut reachable_by_component, component, child);
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

    pub(super) fn reachable_from(&self, unit_id: &str) -> Vec<usize> {
        let Some(index) = self.unit_index(unit_id) else {
            return Vec::new();
        };
        self.reachable_by_component[self.component_of[index]]
            .iter()
            .map(|index| index as usize)
            .collect()
    }

    pub(super) fn reachable_from_many<'a, I>(&self, unit_ids: I) -> Vec<usize>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut result = RoaringBitmap::new();
        for unit_id in unit_ids {
            let Some(index) = self.unit_index(unit_id) else {
                continue;
            };
            result |= &self.reachable_by_component[self.component_of[index]];
        }
        result.iter().map(|index| index as usize).collect()
    }
}

/// 두 SCC의 도달 집합을 한쪽 복사 없이 합친다.
///
/// `RoaringBitmap`은 압축된 집합 union을 제공하므로 긴 DAG에서 매번
/// 정렬된 `Vec` 전체를 새로 만드는 O(n²) 복사와 할당을 피한다.
fn union_component_sets(
    reachable_by_component: &mut [RoaringBitmap],
    target: usize,
    source: usize,
) {
    debug_assert_ne!(target, source);
    if target < source {
        let (left, right) = reachable_by_component.split_at_mut(source);
        left[target] |= &right[0];
    } else {
        let (left, right) = reachable_by_component.split_at_mut(target);
        right[0] |= &left[source];
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

#[cfg(test)]
mod tests {
    use super::ReachabilityIndex;
    use crate::facts::SourceSpan;
    use crate::facts::{
        CodeUnit, CodeUnitKind, FactStore, Reference, ReferenceKind, ResolutionStatus,
    };
    use crate::model::Language;
    use std::collections::BTreeMap;

    #[test]
    fn 긴_호출체인의_도달집합을_정확히_계산한다() {
        let mut units = BTreeMap::new();
        let mut references = Vec::new();
        for index in 0..64 {
            let id = format!("unit-{index}");
            units.insert(
                id.clone(),
                CodeUnit {
                    id: id.clone(),
                    kind: CodeUnitKind::Function,
                    name: id.clone(),
                    qualified_name: id.clone(),
                    file_id: "file".into(),
                    relative_path: "chain.ts".into(),
                    language: Language::TypeScript,
                    parent_id: None,
                    span: SourceSpan::new("file", "chain.ts", 1, 1, 1, 1),
                    body_span: None,
                    signature: None,
                    parameters: Vec::new(),
                    return_type: None,
                    visibility: Default::default(),
                    modifiers: Vec::new(),
                    exported: false,
                },
            );
            if index + 1 < 64 {
                references.push(Reference {
                    id: format!("reference-{index}"),
                    source_unit_id: id,
                    target_unit_id: Some(format!("unit-{}", index + 1)),
                    candidate_unit_ids: Vec::new(),
                    target_name: format!("unit-{}", index + 1),
                    kind: ReferenceKind::Call,
                    status: ResolutionStatus::Confirmed,
                    evidence: Vec::new(),
                });
            }
        }

        let index = ReachabilityIndex::build(&FactStore {
            units,
            references,
            ..FactStore::default()
        });
        let reachable = index.reachable_from("unit-0");
        assert_eq!(reachable.len(), 64);
        assert_eq!(reachable.first(), Some(&0));
        assert_eq!(reachable.last(), Some(&63));
    }
}
