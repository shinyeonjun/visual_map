//! 코드 관계를 도메인 관계로 집계한다.

use crate::domain::membership::DomainMembership;
use crate::domain::models::{DomainGroup, DomainRelation};
use crate::facts::FactStore;
use crate::graph::aggregation::aggregate as aggregate_graph_edges;
use crate::graph::StaticRelationGraph;
use std::collections::{HashMap, HashSet};

pub(super) fn aggregate_relations(
    _store: &FactStore,
    graph: &StaticRelationGraph,
    memberships: &[DomainMembership],
    groups: &[DomainGroup],
) -> Vec<DomainRelation> {
    let mut unit_domains: HashMap<String, HashSet<String>> = HashMap::new();
    for membership in memberships {
        let domain_ids = membership_domain_ids(membership);
        if domain_ids.is_empty() {
            continue;
        }
        unit_domains
            .entry(membership.unit_id.clone())
            .or_default()
            .extend(domain_ids);
    }
    let group_ids: HashSet<&str> = groups.iter().map(|group| group.id.as_str()).collect();
    let mut aggregated: HashMap<(String, String, String), DomainRelation> = HashMap::new();
    for reference in aggregate_graph_edges(&graph.edges) {
        let Some(source_domains) = unit_domains.get(&reference.source_unit_id) else {
            continue;
        };
        let Some(target_unit_id) = reference.target_unit_id.as_deref() else {
            continue;
        };
        let Some(target_domains) = unit_domains.get(target_unit_id) else {
            continue;
        };
        for source_domain in source_domains {
            for target_domain in target_domains {
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
        }
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

fn membership_domain_ids(membership: &DomainMembership) -> Vec<String> {
    if !membership.domain_ids.is_empty() {
        return membership.domain_ids.clone();
    }
    membership
        .domain_id
        .clone()
        .map(|domain_id| vec![domain_id])
        .unwrap_or_default()
}

fn relation_kind(kind: &crate::facts::ReferenceKind) -> String {
    format!("{kind:?}").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::aggregate_relations;
    use crate::domain::confidence::{DomainConfidence, DomainStatus};
    use crate::domain::membership::{DomainMembership, MembershipKind};
    use crate::domain::models::{DomainGroup, DomainKind};
    use crate::facts::{FactStore, Reference, ReferenceKind, ResolutionStatus};
    use crate::graph::StaticRelationGraph;
    use std::collections::BTreeSet;

    fn group(id: &str) -> DomainGroup {
        DomainGroup {
            id: id.to_string(),
            key: id.to_string(),
            label: id.to_string(),
            kind: DomainKind::Business,
            status: DomainStatus::Candidate,
            confidence: DomainConfidence {
                level: "medium".to_string(),
                score: 1,
                signal_families: BTreeSet::new(),
                cohesion: 0.0,
                separation: 0.0,
                evidence_diversity: 0.0,
                overall: 0.0,
            },
            primary_unit_ids: Vec::new(),
            shared_unit_ids: Vec::new(),
            entrypoint_ids: Vec::new(),
            feature_ids: Vec::new(),
            resource_ids: Vec::new(),
            evidence: Vec::new(),
            summary: None,
        }
    }

    fn membership(unit_id: &str, domain_id: &str) -> DomainMembership {
        DomainMembership {
            unit_id: unit_id.to_string(),
            domain_id: Some(domain_id.to_string()),
            domain_ids: vec![domain_id.to_string()],
            kind: MembershipKind::Primary,
            score: 1,
        }
    }

    #[test]
    fn unresolved_reference는_이름_suffix로_도메인_관계를_만들지_않는다() {
        let graph = StaticRelationGraph {
            node_ids: vec!["source".to_string()],
            edges: vec![Reference {
                id: "reference:unresolved".to_string(),
                source_unit_id: "source".to_string(),
                target_unit_id: None,
                candidate_unit_ids: Vec::new(),
                target_name: "Service.handle".to_string(),
                kind: ReferenceKind::Call,
                status: ResolutionStatus::Unknown,
                evidence: Vec::new(),
            }],
            dynamic_edge_ids: Vec::new(),
            unresolved_edge_ids: vec!["reference:unresolved".to_string()],
        };

        let relations = aggregate_relations(
            &FactStore::default(),
            &graph,
            &[
                membership("source", "domain-a"),
                membership("handle", "domain-b"),
            ],
            &[group("domain-a"), group("domain-b")],
        );

        assert!(relations.is_empty());
    }

    #[test]
    fn confirmed_reference만_확정된_도메인_관계가_된다() {
        let graph = StaticRelationGraph {
            node_ids: vec!["source".to_string(), "target".to_string()],
            edges: vec![Reference {
                id: "reference:confirmed".to_string(),
                source_unit_id: "source".to_string(),
                target_unit_id: Some("target".to_string()),
                candidate_unit_ids: Vec::new(),
                target_name: "Service.handle".to_string(),
                kind: ReferenceKind::Call,
                status: ResolutionStatus::Confirmed,
                evidence: Vec::new(),
            }],
            dynamic_edge_ids: Vec::new(),
            unresolved_edge_ids: Vec::new(),
        };

        let relations = aggregate_relations(
            &FactStore::default(),
            &graph,
            &[
                membership("source", "domain-a"),
                membership("target", "domain-b"),
            ],
            &[group("domain-a"), group("domain-b")],
        );

        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0].source_domain_id, "domain-a");
        assert_eq!(relations[0].target_domain_id, "domain-b");
    }
}
