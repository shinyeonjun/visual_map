//! 클러스터 결과를 도메인 그룹과 멤버십으로 변환한다.

use crate::config::DomainPolicy;
use crate::domain::capabilities::Capability;
use crate::domain::capability_keys::is_operational_capability_key;
use crate::domain::confidence::calculate_from_cluster;
use crate::domain::grouping::{stable_domain_id, DomainGroup, DomainKind};
use crate::domain::membership::{DomainMembership, MembershipKind};
use crate::domain::naming::label;
use crate::facts::FactStore;
use std::collections::{HashMap, HashSet};

use crate::domain::clustering;
use crate::domain::feature_graph;
use crate::domain::tfidf;

pub(super) struct FormationResult {
    pub groups: Vec<DomainGroup>,
    pub memberships: Vec<DomainMembership>,
    pub assigned_units: HashSet<String>,
}

pub(super) fn cluster_domain_id(
    cluster: &clustering::Cluster,
    capabilities: &[Capability],
    terms: &[tfidf::FeatureTerms],
    domain_policy: &DomainPolicy,
) -> String {
    stable_domain_id(&pick_capability_cluster_key(
        cluster,
        capabilities,
        terms,
        domain_policy,
    ))
}

pub(super) fn form_domains_from_clusters(
    clusters: &[clustering::Cluster],
    capabilities: &[Capability],
    terms: &[tfidf::FeatureTerms],
    matrix: &feature_graph::SimilarityMatrix,
    domain_policy: &DomainPolicy,
    store: &FactStore,
) -> FormationResult {
    let mut groups: Vec<DomainGroup> = Vec::new();
    let mut memberships = Vec::new();
    let mut assigned_units = HashSet::new();
    let mut unit_membership_idx: HashMap<String, usize> = HashMap::new();
    let mut group_index_by_id: HashMap<String, usize> = HashMap::new();

    for cluster in clusters {
        let cluster_key = pick_capability_cluster_key(cluster, capabilities, terms, domain_policy);
        let cohesion = compute_cohesion(cluster, matrix);
        let separation = compute_separation(cluster, clusters, matrix);
        let evidence_diversity = compute_evidence_diversity(cluster, matrix);
        let (status, confidence) = calculate_from_cluster(
            cluster.members.len(),
            cohesion,
            separation,
            evidence_diversity,
        );
        let domain_id = stable_domain_id(&cluster_key);
        ensure_group(
            &cluster_key,
            &domain_id,
            status,
            &confidence,
            domain_policy,
            &mut groups,
            &mut group_index_by_id,
        );

        for &member_idx in &cluster.members {
            let capability = &capabilities[member_idx];
            if let Some(&group_idx) = group_index_by_id.get(&domain_id) {
                let group = &mut groups[group_idx];
                for entrypoint_id in &capability.entrypoint_ids {
                    if !group.entrypoint_ids.contains(entrypoint_id) {
                        group.entrypoint_ids.push(entrypoint_id.clone());
                    }
                }
                for resource_id in &capability.resource_ids {
                    if !group.resource_ids.contains(resource_id) {
                        group.resource_ids.push(resource_id.clone());
                    }
                }
            }

            for unit_id in &capability.unit_ids {
                assign_unit_membership(
                    unit_id,
                    &domain_id,
                    confidence.score,
                    &mut memberships,
                    &mut unit_membership_idx,
                    &mut groups,
                    &group_index_by_id,
                    &mut assigned_units,
                );
            }
        }
    }

    for group in &mut groups {
        group.primary_unit_ids.sort();
        group.primary_unit_ids.dedup();
        group.shared_unit_ids.sort();
        group.shared_unit_ids.dedup();
        group.entrypoint_ids.sort();
        group.entrypoint_ids.dedup();
        group.resource_ids.sort();
        group.resource_ids.dedup();
        for unit_id in group
            .primary_unit_ids
            .iter()
            .chain(group.shared_unit_ids.iter())
        {
            if let Some(unit) = store.unit(unit_id) {
                group.evidence.push(crate::facts::Evidence::new(
                    "unit",
                    unit.qualified_name.clone(),
                    unit.span.clone(),
                ));
            }
        }
        group.evidence.sort_by(|a, b| a.id.cmp(&b.id));
        group.evidence.dedup_by(|a, b| a.id == b.id);
        group.evidence.truncate(24);
    }

    FormationResult {
        groups,
        memberships,
        assigned_units,
    }
}

fn ensure_group(
    domain_key: &str,
    domain_id: &str,
    status: crate::domain::confidence::DomainStatus,
    confidence: &crate::domain::confidence::DomainConfidence,
    domain_policy: &DomainPolicy,
    groups: &mut Vec<DomainGroup>,
    group_index_by_id: &mut HashMap<String, usize>,
) {
    if group_index_by_id.contains_key(domain_id) {
        return;
    }
    let idx = groups.len();
    group_index_by_id.insert(domain_id.to_string(), idx);
    groups.push(DomainGroup {
        id: domain_id.to_string(),
        key: domain_key.to_string(),
        label: label(domain_key),
        kind: domain_kind(domain_key, domain_policy),
        status,
        confidence: confidence.clone(),
        primary_unit_ids: Vec::new(),
        shared_unit_ids: Vec::new(),
        entrypoint_ids: Vec::new(),
        feature_ids: Vec::new(),
        resource_ids: Vec::new(),
        evidence: Vec::new(),
        summary: None,
    });
}

fn domain_kind(key: &str, policy: &DomainPolicy) -> DomainKind {
    if policy.cross_cutting_keys.contains(key) || is_operational_capability_key(key) {
        DomainKind::CrossCutting
    } else {
        DomainKind::Business
    }
}

fn assign_unit_membership(
    unit_id: &str,
    domain_id: &str,
    score: u32,
    memberships: &mut Vec<DomainMembership>,
    unit_membership_idx: &mut HashMap<String, usize>,
    groups: &mut [DomainGroup],
    group_index_by_id: &HashMap<String, usize>,
    assigned_units: &mut HashSet<String>,
) {
    assigned_units.insert(unit_id.to_string());

    let Some(&existing_idx) = unit_membership_idx.get(unit_id) else {
        let membership = DomainMembership {
            unit_id: unit_id.to_string(),
            domain_id: Some(domain_id.to_string()),
            domain_ids: vec![domain_id.to_string()],
            kind: MembershipKind::Primary,
            score,
        };
        unit_membership_idx.insert(unit_id.to_string(), memberships.len());
        memberships.push(membership);
        if let Some(&group_idx) = group_index_by_id.get(domain_id) {
            let group = &mut groups[group_idx];
            if !group.primary_unit_ids.contains(&unit_id.to_string()) {
                group.primary_unit_ids.push(unit_id.to_string());
            }
        }
        return;
    };

    let existing = &memberships[existing_idx];
    let primary_domain_id = existing
        .domain_id
        .clone()
        .unwrap_or_else(|| domain_id.to_string());
    if primary_domain_id == domain_id {
        return;
    }

    let mut domain_ids = existing.domain_ids.clone();
    if !domain_ids.iter().any(|id| id == domain_id) {
        domain_ids.push(domain_id.to_string());
    }
    domain_ids.sort();
    domain_ids.dedup();

    memberships[existing_idx] = DomainMembership {
        unit_id: unit_id.to_string(),
        domain_id: Some(primary_domain_id.clone()),
        domain_ids: domain_ids.clone(),
        kind: MembershipKind::Shared,
        score: existing.score.max(score),
    };

    if let Some(&primary_idx) = group_index_by_id.get(&primary_domain_id) {
        let primary_group = &mut groups[primary_idx];
        primary_group
            .primary_unit_ids
            .retain(|candidate| candidate != unit_id);
        if !primary_group.shared_unit_ids.contains(&unit_id.to_string()) {
            primary_group.shared_unit_ids.push(unit_id.to_string());
        }
    }
    if let Some(&shared_idx) = group_index_by_id.get(domain_id) {
        let shared_group = &mut groups[shared_idx];
        if !shared_group.shared_unit_ids.contains(&unit_id.to_string())
            && !shared_group.primary_unit_ids.contains(&unit_id.to_string())
        {
            shared_group.shared_unit_ids.push(unit_id.to_string());
        }
    }
}

fn pick_capability_cluster_key(
    cluster: &clustering::Cluster,
    capabilities: &[Capability],
    terms: &[tfidf::FeatureTerms],
    domain_policy: &DomainPolicy,
) -> String {
    let mut key_votes: HashMap<String, usize> = HashMap::new();
    for &member_idx in &cluster.members {
        let key = &capabilities[member_idx].key;
        *key_votes.entry(key.clone()).or_default() += 1;
    }
    if let Some((key, _)) = key_votes
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)))
    {
        return key;
    }

    let mut all_terms: HashMap<String, f64> = HashMap::new();
    let mut non_generic_terms: HashMap<String, f64> = HashMap::new();
    for &member_idx in &cluster.members {
        for (term, weight) in &terms[member_idx].term_frequencies {
            *all_terms.entry(term.clone()).or_insert(0.0) += weight;
            if !domain_policy.is_generic(term) {
                *non_generic_terms.entry(term.clone()).or_insert(0.0) += weight;
            }
        }
    }
    let candidates = if non_generic_terms.is_empty() {
        &all_terms
    } else {
        &non_generic_terms
    };
    candidates
        .iter()
        .max_by(|(ta, wa), (tb, wb)| {
            wa.partial_cmp(wb)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| ta.cmp(tb))
        })
        .map(|(term, _)| term.clone())
        .unwrap_or_else(|| format!("cluster_{}", cluster.id))
}

fn compute_cohesion(
    cluster: &clustering::Cluster,
    matrix: &feature_graph::SimilarityMatrix,
) -> f64 {
    if cluster.members.len() <= 1 {
        return 1.0;
    }
    let mut total = 0.0;
    let mut count = 0;
    for (idx_a, &a) in cluster.members.iter().enumerate() {
        for &b in cluster.members.iter().skip(idx_a + 1) {
            total += matrix.get(a, b).combined;
            count += 1;
        }
    }
    if count == 0 {
        return 0.0;
    }
    total / count as f64
}

fn compute_separation(
    cluster: &clustering::Cluster,
    all_clusters: &[clustering::Cluster],
    matrix: &feature_graph::SimilarityMatrix,
) -> f64 {
    let mut max_external_sim = 0.0_f64;
    for other in all_clusters {
        if other.id == cluster.id {
            continue;
        }
        for &a in &cluster.members {
            for &b in &other.members {
                let sim = matrix.get(a, b).combined;
                max_external_sim = max_external_sim.max(sim);
            }
        }
    }
    (1.0 - max_external_sim).clamp(0.0, 1.0)
}

fn compute_evidence_diversity(
    cluster: &clustering::Cluster,
    matrix: &feature_graph::SimilarityMatrix,
) -> f64 {
    if cluster.members.len() <= 1 {
        return 0.0;
    }
    let mut has_http = false;
    let mut has_call = false;
    let mut has_flow = false;
    let mut has_resource = false;
    let mut has_lexical = false;
    for (idx_a, &a) in cluster.members.iter().enumerate() {
        for &b in cluster.members.iter().skip(idx_a + 1) {
            let sim = matrix.get(a, b);
            if sim.http_match > 0.0 {
                has_http = true;
            }
            if sim.call > 0.0 {
                has_call = true;
            }
            if sim.flow > 0.0 {
                has_flow = true;
            }
            if sim.resource > 0.0 {
                has_resource = true;
            }
            if sim.lexical > 0.0 {
                has_lexical = true;
            }
        }
    }
    let count = [has_http, has_call, has_flow, has_resource, has_lexical]
        .iter()
        .filter(|&&v| v)
        .count();
    count as f64 / 5.0
}
