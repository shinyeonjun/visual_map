//! 코드 관계를 도메인 관계로 집계한다.

use crate::domain::membership::DomainMembership;
use crate::domain::models::{DomainGroup, DomainRelation};
use crate::facts::FactStore;
use crate::graph::aggregation::{aggregate as aggregate_graph_edges, AggregatedReference};
use crate::graph::StaticRelationGraph;
use std::collections::{HashMap, HashSet};

pub(super) fn aggregate_relations(
    store: &FactStore,
    graph: &StaticRelationGraph,
    memberships: &[DomainMembership],
    groups: &[DomainGroup],
) -> Vec<DomainRelation> {
    let mut unit_domains = HashMap::new();
    for membership in memberships {
        if let Some(domain_id) = &membership.domain_id {
            unit_domains.insert(membership.unit_id.clone(), domain_id.clone());
        }
    }
    let group_ids: HashSet<&str> = groups.iter().map(|group| group.id.as_str()).collect();
    let unit_name_index = UnitNameIndex::new(store);
    let mut aggregated: HashMap<(String, String, String), DomainRelation> = HashMap::new();
    for reference in aggregate_graph_edges(&graph.edges) {
        let source_domain = unit_domains.get(&reference.source_unit_id);
        let target_unit = resolve_target(&reference, &unit_name_index);
        let target_domain = target_unit
            .as_deref()
            .and_then(|unit_id| unit_domains.get(unit_id));
        let (Some(source_domain), Some(target_domain)) = (source_domain, target_domain) else {
            continue;
        };
        if source_domain == target_domain {
            continue;
        }
        if !group_ids.contains(source_domain.as_str())
            || !group_ids.contains(target_domain.as_str())
        {
            continue;
        }
        let kind = relation_kind(&reference.kind);
        let key = (source_domain.clone(), target_domain.clone(), kind.clone());
        let relation = aggregated.entry(key).or_insert_with(|| DomainRelation {
            source_domain_id: source_domain.clone(),
            target_domain_id: target_domain.clone(),
            kind,
            status: reference.status.clone(),
            weight: 0,
            evidence: Vec::new(),
        });
        relation.weight += reference.weight;
        relation.evidence.extend(reference.evidence.clone());
    }
    let mut relations: Vec<_> = aggregated.into_values().collect();
    relations.sort_by(|left, right| {
        left.source_domain_id
            .cmp(&right.source_domain_id)
            .then(left.target_domain_id.cmp(&right.target_domain_id))
            .then(left.kind.cmp(&right.kind))
    });
    relations
}

struct UnitNameIndex {
    by_name: HashMap<String, Vec<String>>,
}

impl UnitNameIndex {
    fn new(store: &FactStore) -> Self {
        let mut by_name: HashMap<String, Vec<String>> = HashMap::new();
        for (id, unit) in &store.units {
            by_name
                .entry(unit.name.to_ascii_lowercase())
                .or_default()
                .push(id.clone());
        }
        for ids in by_name.values_mut() {
            ids.sort();
        }
        Self { by_name }
    }
}

fn resolve_target(reference: &AggregatedReference, index: &UnitNameIndex) -> Option<String> {
    if let Some(target_id) = &reference.target_unit_id {
        return Some(target_id.clone());
    }
    let target = reference.target_name.to_ascii_lowercase();
    let mut best_id: Option<&String> = None;
    for offset in target.char_indices().map(|(offset, _)| offset) {
        let suffix = &target[offset..];
        if let Some(ids) = index.by_name.get(suffix) {
            for id in ids {
                if best_id.is_none_or(|best| id < best) {
                    best_id = Some(id);
                }
            }
        }
    }
    best_id.cloned()
}

fn relation_kind(kind: &crate::facts::ReferenceKind) -> String {
    format!("{kind:?}").to_ascii_lowercase()
}
