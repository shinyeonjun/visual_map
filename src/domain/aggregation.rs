//! 코드 관계를 도메인 관계로 집계한다.

use crate::domain::contract_path::{capability_key_from_path, paths_match};
use crate::domain::membership::DomainMembership;
use crate::domain::models::{DomainGroup, DomainRelation};
use crate::facts::{CodeUnitKind, FactStore, ResolutionStatus, ResourceKind};
use crate::graph::aggregation::aggregate as aggregate_graph_edges;
use crate::graph::StaticRelationGraph;
use std::collections::{HashMap, HashSet};

pub(super) fn aggregate_relations(
    store: &FactStore,
    graph: &StaticRelationGraph,
    memberships: &[DomainMembership],
    groups: &[DomainGroup],
) -> Vec<DomainRelation> {
    let unit_domains = unit_domain_index(memberships);
    let group_ids: HashSet<&str> = groups.iter().map(|group| group.id.as_str()).collect();
    let key_to_domain: HashMap<&str, &str> = groups
        .iter()
        .map(|group| (group.key.as_str(), group.id.as_str()))
        .collect();

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
                insert_relation(
                    &mut aggregated,
                    source_domain,
                    target_domain,
                    kind,
                    reference.status.clone(),
                    reference.weight,
                    reference.evidence.clone(),
                );
            }
        }
    }

    aggregate_http_relations(
        store,
        groups,
        &unit_domains,
        &key_to_domain,
        &mut aggregated,
    );

    let mut relations: Vec<_> = aggregated.into_values().collect();
    relations.sort_by(|left, right| {
        left.source_domain_id
            .cmp(&right.source_domain_id)
            .then(left.target_domain_id.cmp(&right.target_domain_id))
            .then(left.kind.cmp(&right.kind))
    });
    relations
}

fn unit_domain_index(memberships: &[DomainMembership]) -> HashMap<String, HashSet<String>> {
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
    unit_domains
}

fn aggregate_http_relations(
    store: &FactStore,
    groups: &[DomainGroup],
    unit_domains: &HashMap<String, HashSet<String>>,
    key_to_domain: &HashMap<&str, &str>,
    aggregated: &mut HashMap<(String, String, String), DomainRelation>,
) {
    let entrypoints_by_id: HashMap<&str, &crate::facts::Entrypoint> = store
        .entrypoints
        .iter()
        .map(|entrypoint| (entrypoint.id.as_str(), entrypoint))
        .collect();
    for resource in &store.resources {
        if !matches!(
            resource.kind,
            ResourceKind::ExternalApi | ResourceKind::WebSocket
        ) {
            continue;
        }
        let target_domains =
            target_domains_for_resource(resource, groups, key_to_domain, &entrypoints_by_id);
        if target_domains.is_empty() {
            continue;
        }
        let Some(source_domains) = source_domains_for_unit(&resource.unit_id, store, unit_domains)
        else {
            continue;
        };
        let kind = if resource.kind == ResourceKind::WebSocket {
            "websocket".to_string()
        } else {
            "http".to_string()
        };
        for source_domain in &source_domains {
            for target_domain in &target_domains {
                if source_domain == target_domain {
                    continue;
                }
                insert_relation(
                    aggregated,
                    source_domain,
                    target_domain,
                    kind.clone(),
                    ResolutionStatus::Confirmed,
                    1,
                    resource.evidence.clone(),
                );
            }
        }
    }
}

fn source_domains_for_unit(
    unit_id: &str,
    store: &FactStore,
    unit_domains: &HashMap<String, HashSet<String>>,
) -> Option<HashSet<String>> {
    let mut current = Some(unit_id.to_string());
    while let Some(id) = current {
        if let Some(domains) = unit_domains.get(&id) {
            if !domains.is_empty() {
                return Some(domains.clone());
            }
        }
        current = store.unit(&id).and_then(|unit| unit.parent_id.clone());
    }
    let file_id = store.unit(unit_id)?.file_id.clone();
    store.units.values().find_map(|unit| {
        (unit.kind == CodeUnitKind::File && unit.file_id == file_id)
            .then(|| unit_domains.get(&unit.id))
            .flatten()
            .filter(|domains| !domains.is_empty())
            .cloned()
    })
}

fn target_domains_for_resource(
    resource: &crate::facts::ResourceAccess,
    groups: &[DomainGroup],
    key_to_domain: &HashMap<&str, &str>,
    entrypoints_by_id: &HashMap<&str, &crate::facts::Entrypoint>,
) -> HashSet<String> {
    let mut targets = HashSet::new();
    if let Some(key) = capability_key_from_path(&resource.name) {
        if let Some(domain_id) = key_to_domain.get(key.as_str()) {
            targets.insert((*domain_id).to_string());
        }
    }
    for group in groups {
        for entrypoint_id in &group.entrypoint_ids {
            let Some(entrypoint) = entrypoints_by_id.get(entrypoint_id.as_str()) else {
                continue;
            };
            let raw = entrypoint.path.as_deref().unwrap_or(&entrypoint.name);
            if paths_match(&resource.name, raw) {
                targets.insert(group.id.clone());
            }
        }
    }
    targets
}

fn insert_relation(
    aggregated: &mut HashMap<(String, String, String), DomainRelation>,
    source_domain: &str,
    target_domain: &str,
    kind: String,
    status: ResolutionStatus,
    weight: u32,
    evidence: Vec<crate::facts::Evidence>,
) {
    let key = (
        source_domain.to_string(),
        target_domain.to_string(),
        kind.clone(),
    );
    let relation = aggregated.entry(key).or_insert_with(|| DomainRelation {
        source_domain_id: source_domain.to_string(),
        target_domain_id: target_domain.to_string(),
        kind,
        status: status.clone(),
        weight: 0,
        evidence: Vec::new(),
    });
    relation.weight += weight;
    relation.evidence.extend(evidence);
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

    use std::sync::Arc;

    #[test]
    fn unresolved_reference는_이름_suffix로_도메인_관계를_만들지_않는다() {
        let graph = StaticRelationGraph {
            node_ids: vec!["source".to_string()],
            edges: Arc::from([Reference {
                id: "reference:unresolved".to_string(),
                source_unit_id: "source".to_string(),
                target_unit_id: None,
                candidate_unit_ids: Vec::new(),
                target_name: "Service.handle".to_string(),
                kind: ReferenceKind::Call,
                status: ResolutionStatus::Unknown,
                evidence: Vec::new(),
            }]),
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
            edges: Arc::from([Reference {
                id: "reference:confirmed".to_string(),
                source_unit_id: "source".to_string(),
                target_unit_id: Some("target".to_string()),
                candidate_unit_ids: Vec::new(),
                target_name: "Service.handle".to_string(),
                kind: ReferenceKind::Call,
                status: ResolutionStatus::Confirmed,
                evidence: Vec::new(),
            }]),
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

    #[test]
    fn fetch_리소스와_서버_계약이_다른_도메인_사이_http_관계를_만든다() {
        use crate::facts::{ResourceAccess, ResourceKind};

        let graph = StaticRelationGraph::default();
        let mut store = FactStore::default();
        store.resources.push(ResourceAccess {
            id: "resource:fetch".to_string(),
            unit_id: "client".to_string(),
            kind: ResourceKind::ExternalApi,
            name: "/billing/invoices".to_string(),
            mode: crate::facts::AccessMode::Read,
            evidence: Vec::new(),
        });
        let groups = vec![
            group_with_key("domain-web", "web"),
            group_with_key("domain-billing", "billing"),
        ];
        let memberships = vec![
            membership("client", "domain-web"),
            membership("server", "domain-billing"),
        ];

        let relations = aggregate_relations(&store, &graph, &memberships, &groups);
        let http = relations
            .iter()
            .find(|relation| relation.kind == "http")
            .expect("fetch와 billing 계약 사이 http 관계가 있어야 한다");

        assert_eq!(http.source_domain_id, "domain-web");
        assert_eq!(http.target_domain_id, "domain-billing");
        assert!(http.weight >= 1);
    }

    #[test]
    fn fetch와_엔트리포인트_경로가_같으면_능력_키가_달라도_http_관계를_만든다() {
        use crate::facts::{Entrypoint, ResourceAccess, ResourceKind};

        let graph = StaticRelationGraph::default();
        let mut store = FactStore::default();
        store.resources.push(ResourceAccess {
            id: "resource:fetch".to_string(),
            unit_id: "client".to_string(),
            kind: ResourceKind::ExternalApi,
            name: "https://host/api/v1/billing/invoices".to_string(),
            mode: crate::facts::AccessMode::Read,
            evidence: Vec::new(),
        });
        store.entrypoints.push(Entrypoint {
            id: "entrypoint:billing".to_string(),
            unit_id: "server".to_string(),
            name: "GET /billing/invoices".to_string(),
            method: Some("GET".to_string()),
            path: Some("/billing/invoices".to_string()),
            kind: crate::facts::EntrypointKind::Http,
            framework_id: None,
            evidence: Vec::new(),
        });
        let mut billing = group_with_key("domain-billing", "billing");
        billing
            .entrypoint_ids
            .push("entrypoint:billing".to_string());
        let groups = vec![group_with_key("domain-web", "web"), billing];
        let memberships = vec![
            membership("client", "domain-web"),
            membership("server", "domain-billing"),
        ];

        let relations = aggregate_relations(&store, &graph, &memberships, &groups);
        let http = relations
            .iter()
            .find(|relation| relation.kind == "http")
            .expect("fetch와 billing 엔트리포인트 사이 http 관계가 있어야 한다");

        assert_eq!(http.source_domain_id, "domain-web");
        assert_eq!(http.target_domain_id, "domain-billing");
    }

    #[test]
    fn 홈_도메인에서_다른_계약으로의_fetch만_http_관계가_된다() {
        use crate::facts::{Entrypoint, ResourceAccess, ResourceKind};

        let graph = StaticRelationGraph::default();
        let mut store = FactStore::default();
        store.resources.push(ResourceAccess {
            id: "resource:auth".to_string(),
            unit_id: "client".to_string(),
            kind: ResourceKind::ExternalApi,
            name: "/auth/me".to_string(),
            mode: crate::facts::AccessMode::Read,
            evidence: Vec::new(),
        });
        store.resources.push(ResourceAccess {
            id: "resource:home".to_string(),
            unit_id: "client".to_string(),
            kind: ResourceKind::ExternalApi,
            name: "/billing/invoices".to_string(),
            mode: crate::facts::AccessMode::Read,
            evidence: Vec::new(),
        });
        store.entrypoints.push(Entrypoint {
            id: "entrypoint:auth".to_string(),
            unit_id: "server-auth".to_string(),
            name: "GET /auth/me".to_string(),
            method: Some("GET".to_string()),
            path: Some("/auth/me".to_string()),
            kind: crate::facts::EntrypointKind::Http,
            framework_id: None,
            evidence: Vec::new(),
        });
        let mut auth = group_with_key("domain-auth", "auth");
        auth.entrypoint_ids.push("entrypoint:auth".to_string());
        let groups = vec![group_with_key("domain-billing", "billing"), auth];
        let memberships = vec![
            membership("client", "domain-billing"),
            membership("server-auth", "domain-auth"),
        ];

        let relations = aggregate_relations(&store, &graph, &memberships, &groups);
        let http: Vec<_> = relations
            .iter()
            .filter(|relation| relation.kind == "http")
            .collect();
        assert_eq!(http.len(), 1);
        assert_eq!(http[0].source_domain_id, "domain-billing");
        assert_eq!(http[0].target_domain_id, "domain-auth");
    }

    #[test]
    fn fetch_유닛_멤버십이_없으면_파일_유닛으로_source를_찾는다() {
        use crate::facts::{CodeUnit, CodeUnitKind, ResourceAccess, ResourceKind, SourceSpan};
        use crate::model::Language;

        let graph = StaticRelationGraph::default();
        let mut store = FactStore::default();
        store.units.insert(
            "client:file".into(),
            CodeUnit {
                id: "client:file".into(),
                kind: CodeUnitKind::File,
                name: "client.ts".into(),
                qualified_name: "client.ts".into(),
                file_id: "file:client".into(),
                relative_path: "web/client.ts".into(),
                language: Language::TypeScript,
                parent_id: None,
                span: SourceSpan::new("file:client", "web/client.ts", 1, 1, 1, 1),
                body_span: None,
                signature: None,
                parameters: Vec::new(),
                return_type: None,
                visibility: Default::default(),
                modifiers: Vec::new(),
                exported: true,
            },
        );
        store.units.insert(
            "client:fn".into(),
            CodeUnit {
                id: "client:fn".into(),
                kind: CodeUnitKind::Function,
                name: "loadMe".into(),
                qualified_name: "loadMe".into(),
                file_id: "file:client".into(),
                relative_path: "web/client.ts".into(),
                language: Language::TypeScript,
                parent_id: Some("client:file".into()),
                span: SourceSpan::new("file:client", "web/client.ts", 2, 1, 4, 1),
                body_span: None,
                signature: None,
                parameters: Vec::new(),
                return_type: None,
                visibility: Default::default(),
                modifiers: Vec::new(),
                exported: true,
            },
        );
        store.resources.push(ResourceAccess {
            id: "resource:auth".to_string(),
            unit_id: "client:fn".to_string(),
            kind: ResourceKind::ExternalApi,
            name: "/auth/me".to_string(),
            mode: crate::facts::AccessMode::Read,
            evidence: Vec::new(),
        });
        let groups = vec![
            group_with_key("domain-billing", "billing"),
            group_with_key("domain-auth", "auth"),
        ];
        let memberships = vec![
            membership("client:file", "domain-billing"),
            membership("server-auth", "domain-auth"),
        ];

        let relations = aggregate_relations(&store, &graph, &memberships, &groups);
        let http = relations
            .iter()
            .find(|relation| relation.kind == "http")
            .expect("파일 유닛 홈에서 auth로 http 관계가 있어야 한다");
        assert_eq!(http.source_domain_id, "domain-billing");
        assert_eq!(http.target_domain_id, "domain-auth");
    }

    fn group_with_key(id: &str, key: &str) -> DomainGroup {
        DomainGroup {
            id: id.to_string(),
            key: key.to_string(),
            label: key.to_string(),
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
}
