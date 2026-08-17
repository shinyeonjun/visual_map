//! 단일 능력 도메인을 더 큰 비즈니스 도메인으로 흡수한다.

use crate::config::DomainPolicy;
use crate::domain::capabilities::Capability;
use crate::domain::capability_keys::{
    is_leaf_capability_key, is_operational_capability_key, is_static_page_key, static_page_stem,
};
use crate::domain::clustering;
use crate::domain::contract_path::mounted_path_suffix_match;
use crate::domain::feature_graph;
use crate::domain::grouping::{DomainGroup, DomainKind};
use crate::domain::membership::DomainMembership;
use crate::domain::tfidf;
use crate::facts::FactStore;
use std::collections::{HashMap, HashSet};

use super::cluster_groups::{cluster_domain_id, FormationResult};

pub(super) fn absorb_singleton_domains(
    formation: &mut FormationResult,
    clusters: &[clustering::Cluster],
    capabilities: &[Capability],
    terms: &[tfidf::FeatureTerms],
    matrix: &feature_graph::SimilarityMatrix,
    store: &FactStore,
    domain_policy: &DomainPolicy,
    diagnostics: &mut super::diagnostics::DomainFormationDiagnostics,
) {
    if formation.groups.is_empty() || clusters.is_empty() {
        return;
    }

    let mut capability_domain_ids = HashMap::new();
    let mut domain_capability_counts = HashMap::new();
    for cluster in clusters {
        let domain_id = cluster_domain_id(cluster, capabilities, terms, domain_policy);
        *domain_capability_counts
            .entry(domain_id.clone())
            .or_default() += cluster.members.len();
        for &member in &cluster.members {
            capability_domain_ids.insert(member, domain_id.clone());
        }
    }

    let domain_index: HashMap<String, usize> = formation
        .groups
        .iter()
        .enumerate()
        .map(|(index, group)| (group.id.clone(), index))
        .collect();

    let mut absorbed_domain_ids = HashSet::new();
    let mut merges: Vec<(String, String, &'static str)> = Vec::new();

    for cluster in clusters {
        if cluster.members.len() != 1 {
            continue;
        }
        let member = cluster.members[0];
        let capability = &capabilities[member];
        let child_domain_id = match capability_domain_ids.get(&member) {
            Some(domain_id) => domain_id.clone(),
            None => continue,
        };
        if absorbed_domain_ids.contains(&child_domain_id) {
            continue;
        }

        if is_operational_capability_key(&capability.key) {
            mark_cross_cutting(&mut formation.groups, &domain_index, &child_domain_id);
            continue;
        }

        let Some((parent_domain_id, reason)) = find_absorption_parent(
            member,
            capability,
            &child_domain_id,
            &formation.groups,
            capabilities,
            clusters,
            terms,
            matrix,
            store,
            domain_policy,
            &domain_capability_counts,
            &absorbed_domain_ids,
        ) else {
            continue;
        };

        if parent_domain_id == child_domain_id {
            continue;
        }
        merges.push((child_domain_id.clone(), parent_domain_id, reason));
        absorbed_domain_ids.insert(child_domain_id);
    }

    for (child_id, parent_id, reason) in merges {
        diagnostics.record_absorption_reason(reason);
        merge_domains(
            &mut formation.groups,
            &mut formation.memberships,
            &child_id,
            &parent_id,
        );
    }

    formation
        .groups
        .retain(|group| !absorbed_domain_ids.contains(&group.id));
}

fn mark_cross_cutting(
    groups: &mut [DomainGroup],
    domain_index: &HashMap<String, usize>,
    domain_id: &str,
) {
    let Some(&index) = domain_index.get(domain_id) else {
        return;
    };
    groups[index].kind = DomainKind::CrossCutting;
}

fn find_absorption_parent(
    child_member: usize,
    child: &Capability,
    child_domain_id: &str,
    groups: &[DomainGroup],
    capabilities: &[Capability],
    clusters: &[clustering::Cluster],
    terms: &[tfidf::FeatureTerms],
    matrix: &feature_graph::SimilarityMatrix,
    store: &FactStore,
    domain_policy: &DomainPolicy,
    domain_capability_counts: &HashMap<String, usize>,
    absorbed: &HashSet<String>,
) -> Option<(String, &'static str)> {
    if let Some(domain_id) = related_noun_parent(child, groups, absorbed) {
        return Some((domain_id, "related-noun"));
    }

    if let Some(domain_id) = static_page_parent(
        child,
        child_domain_id,
        capabilities,
        clusters,
        terms,
        domain_policy,
        absorbed,
    ) {
        return Some((domain_id, "static-page"));
    }

    if let Some(domain_id) =
        same_source_file_parent(child, capabilities, clusters, terms, store, domain_policy, absorbed)
    {
        return Some((domain_id, "same-source-file"));
    }

    if is_leaf_capability_key(&child.key) || domain_policy.is_generic(&child.key) {
        if let Some(domain_id) = contract_path_parent(
            child,
            capabilities,
            clusters,
            terms,
            domain_policy,
            absorbed,
        ) {
            return Some((domain_id, "contract-path"));
        }
    }

    similarity_parent(
        child_member,
        capabilities,
        clusters,
        terms,
        matrix,
        domain_policy,
        domain_capability_counts,
        absorbed,
    )
    .map(|domain_id| (domain_id, "similarity"))
}

fn related_noun_parent(
    child: &Capability,
    groups: &[DomainGroup],
    absorbed: &HashSet<String>,
) -> Option<String> {
    let parent_key = related_domain_key(&child.key)?;
    groups
        .iter()
        .find(|group| {
            !absorbed.contains(&group.id)
                && group.kind == DomainKind::Business
                && group.key == parent_key
        })
        .map(|group| group.id.clone())
}

fn related_domain_key(key: &str) -> Option<&'static str> {
    if key.contains("schedule") && key != "schedules" {
        return Some("schedules");
    }
    None
}

fn static_page_parent(
    child: &Capability,
    child_domain_id: &str,
    capabilities: &[Capability],
    clusters: &[clustering::Cluster],
    terms: &[tfidf::FeatureTerms],
    domain_policy: &DomainPolicy,
    absorbed: &HashSet<String>,
) -> Option<String> {
    if !is_static_page_key(&child.key) {
        return None;
    }
    let stem = static_page_stem(&child.key)?;

    let mut best_score = 0usize;
    let mut best_domain_id: Option<String> = None;
    for cluster in clusters {
        let domain_id = cluster_domain_id(cluster, capabilities, terms, domain_policy);
        if absorbed.contains(&domain_id) || domain_id == child_domain_id {
            continue;
        }
        let score = cluster
            .members
            .iter()
            .map(|&member| page_parent_score(stem, &capabilities[member]))
            .max()
            .unwrap_or(0);
        if score > best_score {
            best_score = score;
            best_domain_id = Some(domain_id);
        }
    }

    if best_score >= 2 {
        return best_domain_id;
    }

    None
}

fn page_parent_score(stem: &str, capability: &Capability) -> usize {
    let mut score = 0;
    if capability.key == stem {
        score += 3;
    }
    if capability.key.contains(stem) || stem.contains(&capability.key) {
        score += 2;
    }
    for path in &capability.contract_paths {
        if path.contains(&format!("/{stem}")) || path.contains(&format!("/{stem}/")) {
            score += 2;
        }
    }
    score
}

fn same_source_file_parent(
    child: &Capability,
    capabilities: &[Capability],
    clusters: &[clustering::Cluster],
    terms: &[tfidf::FeatureTerms],
    store: &FactStore,
    domain_policy: &DomainPolicy,
    absorbed: &HashSet<String>,
) -> Option<String> {
    let child_files = capability_source_files(child, store);
    if child_files.is_empty() {
        return None;
    }

    let mut best_overlap = 0usize;
    let mut best_size = 0usize;
    let mut best_domain_id: Option<String> = None;
    for cluster in clusters {
        if cluster.members.len() <= 1 {
            continue;
        }
        let domain_id = cluster_domain_id(cluster, capabilities, terms, domain_policy);
        if absorbed.contains(&domain_id) {
            continue;
        }
        let overlap = cluster
            .members
            .iter()
            .map(|&member| shared_source_files(&child_files, &capabilities[member], store))
            .sum::<usize>();
        if overlap == 0 {
            continue;
        }
        let size = cluster.members.len();
        if overlap > best_overlap || (overlap == best_overlap && size > best_size) {
            best_overlap = overlap;
            best_size = size;
            best_domain_id = Some(domain_id);
        }
    }
    best_domain_id
}

fn capability_source_files(capability: &Capability, store: &FactStore) -> HashSet<String> {
    capability
        .unit_ids
        .iter()
        .filter_map(|unit_id| store.unit(unit_id))
        .map(|unit| unit.relative_path.clone())
        .collect()
}

fn shared_source_files(
    child_files: &HashSet<String>,
    capability: &Capability,
    store: &FactStore,
) -> usize {
    capability
        .unit_ids
        .iter()
        .filter_map(|unit_id| store.unit(unit_id))
        .filter(|unit| child_files.contains(&unit.relative_path))
        .count()
}

fn contract_path_parent(
    child: &Capability,
    capabilities: &[Capability],
    clusters: &[clustering::Cluster],
    terms: &[tfidf::FeatureTerms],
    domain_policy: &DomainPolicy,
    absorbed: &HashSet<String>,
) -> Option<String> {
    let child_paths: Vec<_> = child.contract_paths.iter().cloned().collect();
    if child_paths.is_empty() {
        return None;
    }

    let mut best_score = 0usize;
    let mut best_domain_id: Option<String> = None;
    for cluster in clusters {
        if cluster.members.len() <= 1 {
            continue;
        }
        let domain_id = cluster_domain_id(cluster, capabilities, terms, domain_policy);
        if absorbed.contains(&domain_id) {
            continue;
        }
        let score = cluster
            .members
            .iter()
            .map(|&member| path_affinity_score(&child_paths, &capabilities[member]))
            .sum::<usize>();
        if score > best_score {
            best_score = score;
            best_domain_id = Some(domain_id);
        }
    }
    best_domain_id
}

fn path_affinity_score(child_paths: &[String], parent: &Capability) -> usize {
    let mut score = 0;
    for child_path in child_paths {
        for parent_path in &parent.contract_paths {
            if child_path == parent_path {
                score += 3;
                continue;
            }
            if mounted_path_suffix_match(child_path, parent_path) {
                score += 2;
            }
        }
    }
    score
}

fn similarity_parent(
    child_member: usize,
    capabilities: &[Capability],
    clusters: &[clustering::Cluster],
    terms: &[tfidf::FeatureTerms],
    matrix: &feature_graph::SimilarityMatrix,
    domain_policy: &DomainPolicy,
    domain_capability_counts: &HashMap<String, usize>,
    absorbed: &HashSet<String>,
) -> Option<String> {
    let mut best_sim = 0.0_f64;
    let mut best_size = 0usize;
    let mut best_domain_id: Option<String> = None;
    for cluster in clusters {
        if cluster.members.len() <= 1 {
            continue;
        }
        let domain_id = cluster_domain_id(cluster, capabilities, terms, domain_policy);
        if absorbed.contains(&domain_id) {
            continue;
        }
        let mut max_sim = 0.0_f64;
        for &member in &cluster.members {
            let sim = matrix.get(child_member, member).combined;
            if sim > max_sim {
                max_sim = sim;
            }
        }
        if max_sim < 0.12 {
            continue;
        }
        let size = domain_capability_counts
            .get(&domain_id)
            .copied()
            .unwrap_or(cluster.members.len());
        if max_sim > best_sim || (max_sim == best_sim && size > best_size) {
            best_sim = max_sim;
            best_size = size;
            best_domain_id = Some(domain_id);
        }
    }
    best_domain_id
}

pub(super) fn merge_domains(
    groups: &mut [DomainGroup],
    memberships: &mut [DomainMembership],
    child_id: &str,
    parent_id: &str,
) {
    let Some(child_idx) = groups.iter().position(|group| group.id == child_id) else {
        return;
    };
    let Some(parent_idx) = groups.iter().position(|group| group.id == parent_id) else {
        return;
    };

    let child = groups[child_idx].clone();
    let parent = &mut groups[parent_idx];

    for entrypoint_id in child.entrypoint_ids {
        if !parent.entrypoint_ids.contains(&entrypoint_id) {
            parent.entrypoint_ids.push(entrypoint_id);
        }
    }
    for resource_id in child.resource_ids {
        if !parent.resource_ids.contains(&resource_id) {
            parent.resource_ids.push(resource_id);
        }
    }
    for unit_id in child.primary_unit_ids {
        if !parent.primary_unit_ids.contains(&unit_id)
            && !parent.shared_unit_ids.contains(&unit_id)
        {
            parent.primary_unit_ids.push(unit_id);
        }
    }
    for unit_id in child.shared_unit_ids {
        if !parent.primary_unit_ids.contains(&unit_id)
            && !parent.shared_unit_ids.contains(&unit_id)
        {
            parent.shared_unit_ids.push(unit_id);
        }
    }
    for evidence in child.evidence {
        if !parent.evidence.iter().any(|item| item.id == evidence.id) {
            parent.evidence.push(evidence);
        }
    }

    parent.primary_unit_ids.sort();
    parent.primary_unit_ids.dedup();
    parent.shared_unit_ids.sort();
    parent.shared_unit_ids.dedup();
    parent.entrypoint_ids.sort();
    parent.entrypoint_ids.dedup();
    parent.resource_ids.sort();
    parent.resource_ids.dedup();
    parent.evidence.sort_by(|left, right| left.id.cmp(&right.id));
    parent.evidence.dedup_by(|left, right| left.id == right.id);
    parent.evidence.truncate(24);

    for membership in memberships.iter_mut() {
        if membership.domain_id.as_deref() == Some(child_id) {
            membership.domain_id = Some(parent_id.to_string());
        }
        membership.domain_ids = membership
            .domain_ids
            .iter()
            .map(|domain_id| {
                if domain_id == child_id {
                    parent_id.to_string()
                } else {
                    domain_id.clone()
                }
            })
            .collect();
        membership.domain_ids.sort();
        membership.domain_ids.dedup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DomainClusteringMode;
    use crate::domain::confidence::DomainStatus;
    use crate::domain::formation::diagnostics::DomainFormationDiagnostics;
    use crate::domain::grouping::stable_domain_id;
    use crate::facts::{CodeUnit, CodeUnitKind, SourceSpan};
    use crate::model::Language;
    use std::collections::HashMap;

    fn empty_terms() -> tfidf::FeatureTerms {
        tfidf::FeatureTerms {
            term_frequencies: HashMap::new(),
        }
    }

    fn capability(key: &str, unit_id: &str, paths: &[&str]) -> Capability {
        Capability {
            key: key.to_string(),
            entrypoint_ids: Vec::new(),
            resource_ids: Vec::new(),
            unit_ids: vec![unit_id.to_string()],
            contract_paths: paths.iter().map(|path| (*path).to_string()).collect(),
        }
    }

    fn file_unit(id: &str, path: &str) -> CodeUnit {
        CodeUnit {
            id: id.into(),
            kind: CodeUnitKind::File,
            name: id.into(),
            qualified_name: id.into(),
            file_id: format!("file:{id}"),
            relative_path: path.into(),
            language: Language::Python,
            parent_id: None,
            span: SourceSpan::new(format!("file:{id}"), path, 1, 1, 1, 1),
            body_span: None,
            signature: None,
            parameters: Vec::new(),
            return_type: None,
            visibility: Default::default(),
            modifiers: Vec::new(),
            exported: true,
        }
    }

    fn group(id: &str, key: &str) -> DomainGroup {
        DomainGroup {
            id: id.into(),
            key: key.into(),
            label: key.into(),
            kind: DomainKind::Business,
            status: DomainStatus::Candidate,
            confidence: crate::domain::confidence::DomainConfidence {
                level: "candidate".into(),
                score: 0,
                signal_families: Default::default(),
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

    #[test]
    fn 같은_소스_파일의_단일_능력은_부모_도메인으로_흡수된다() {
        let mut store = FactStore::default();
        store.units.insert(
            "unit:analysis".into(),
            file_unit("unit:analysis", "backend/routers/analysis.py"),
        );

        let capabilities = vec![
            capability("analyze", "unit:analysis", &["/analyze/text"]),
            capability("file", "unit:analysis", &["/analyze/file"]),
            capability("save", "unit:analysis", &["/save"]),
        ];
        let clusters = vec![
            clustering::Cluster {
                id: 0,
                members: vec![1],
            },
            clustering::Cluster {
                id: 1,
                members: vec![0, 2],
            },
        ];
        let terms = vec![empty_terms(), empty_terms(), empty_terms()];
        let matrix = feature_graph::SimilarityMatrix::uniform(3, 0.2);
        let mut formation = FormationResult {
            groups: vec![
                group(&stable_domain_id("file"), "file"),
                group(&stable_domain_id("analyze"), "analyze"),
            ],
            memberships: Vec::new(),
            assigned_units: HashSet::new(),
        };

        let mut diagnostics =
            DomainFormationDiagnostics::new(DomainClusteringMode::LegacyStrictKey);
        absorb_singleton_domains(
            &mut formation,
            &clusters,
            &capabilities,
            &terms,
            &matrix,
            &store,
            &DomainPolicy::default(),
            &mut diagnostics,
        );

        assert_eq!(formation.groups.len(), 1);
        assert_eq!(formation.groups[0].key, "analyze");
    }

    #[test]
    fn 정적_html_페이지는_auth_도메인으로_흡수된다() {
        let capabilities = vec![
            capability("login.html", "unit:page", &["/login.html"]),
            capability("auth", "unit:auth", &["/auth/login", "/auth/status"]),
        ];
        let clusters = vec![
            clustering::Cluster {
                id: 0,
                members: vec![0],
            },
            clustering::Cluster {
                id: 1,
                members: vec![1],
            },
        ];
        let terms = vec![empty_terms(), empty_terms()];
        let matrix = feature_graph::SimilarityMatrix::uniform(2, 0.05);
        let mut formation = FormationResult {
            groups: vec![
                group(&stable_domain_id("login.html"), "login.html"),
                group(&stable_domain_id("auth"), "auth"),
            ],
            memberships: Vec::new(),
            assigned_units: HashSet::new(),
        };

        let mut diagnostics =
            DomainFormationDiagnostics::new(DomainClusteringMode::LegacyStrictKey);
        absorb_singleton_domains(
            &mut formation,
            &clusters,
            &capabilities,
            &terms,
            &matrix,
            &FactStore::default(),
            &DomainPolicy::default(),
            &mut diagnostics,
        );

        assert_eq!(formation.groups.len(), 1);
        assert_eq!(formation.groups[0].key, "auth");
    }

    #[test]
    fn health_단일_도메인은_cross_cutting으로_남는다() {
        let capabilities = vec![capability("health", "unit:health", &["/health"])];
        let clusters = vec![clustering::Cluster {
            id: 0,
            members: vec![0],
        }];
        let terms = vec![empty_terms()];
        let matrix = feature_graph::SimilarityMatrix::uniform(1, 0.0);
        let mut formation = FormationResult {
            groups: vec![group(&stable_domain_id("health"), "health")],
            memberships: Vec::new(),
            assigned_units: HashSet::new(),
        };

        let mut diagnostics =
            DomainFormationDiagnostics::new(DomainClusteringMode::LegacyStrictKey);
        absorb_singleton_domains(
            &mut formation,
            &clusters,
            &capabilities,
            &terms,
            &matrix,
            &FactStore::default(),
            &DomainPolicy::default(),
            &mut diagnostics,
        );

        assert_eq!(formation.groups.len(), 1);
        assert_eq!(formation.groups[0].kind, DomainKind::CrossCutting);
    }

    #[test]
    fn schedule_계열_단일_키는_schedules_도메인으로_흡수된다() {
        let capabilities = vec![
            capability("send-schedule", "unit:mail", &["/send-schedule"]),
            capability("schedules", "unit:sched", &["/schedules", "/schedules/{id}"]),
        ];
        let clusters = vec![
            clustering::Cluster {
                id: 0,
                members: vec![0],
            },
            clustering::Cluster {
                id: 1,
                members: vec![1],
            },
        ];
        let terms = vec![empty_terms(), empty_terms()];
        let matrix = feature_graph::SimilarityMatrix::uniform(2, 0.05);
        let mut formation = FormationResult {
            groups: vec![
                group(&stable_domain_id("send-schedule"), "send-schedule"),
                group(&stable_domain_id("schedules"), "schedules"),
            ],
            memberships: Vec::new(),
            assigned_units: HashSet::new(),
        };

        let mut diagnostics =
            DomainFormationDiagnostics::new(DomainClusteringMode::LegacyStrictKey);
        absorb_singleton_domains(
            &mut formation,
            &clusters,
            &capabilities,
            &terms,
            &matrix,
            &FactStore::default(),
            &DomainPolicy::default(),
            &mut diagnostics,
        );

        assert_eq!(formation.groups.len(), 1);
        assert_eq!(formation.groups[0].key, "schedules");
    }
}
