//! Feature-first 클러스터링으로 도메인을 형성하는 파이프라인.
//!
//! 기존 단어 기반 도메인 형성(grouping.rs)과 달리 Feature를 먼저 만들고
//! Multi-view 유사도 그래프를 구축한 뒤 응집형 클러스터링으로 도메인을 형성한다.

use crate::config::DomainPolicy;
use crate::domain::confidence::calculate_from_cluster;
use crate::domain::grouping::{stable_domain_id, DomainAnalysisOutput, DomainGroup, DomainKind};
use crate::domain::membership::{DomainMembership, MembershipKind};
use crate::domain::naming::label;
use crate::facts::{FactStore, ResolutionStatus};
use crate::flow::ExecutionFlowGraph;
use crate::graph::StaticRelationGraph;
use crate::views::overview::model::{FeatureGroup, FeatureKind};
use std::collections::{HashMap, HashSet};

use super::aggregation::aggregate_relations;
use super::{clustering, feature_graph, tfidf};

/// Feature-first 클러스터링의 결과다.
pub struct FeatureFirstResult {
    pub analysis: DomainAnalysisOutput,
    pub features: Vec<FeatureGroup>,
}

/// Feature를 먼저 만들고 Multi-view 유사도로 클러스터링해 도메인을 형성한다.
pub(super) fn analyze_feature_first(
    store: &FactStore,
    graph: &StaticRelationGraph,
    execution_flows: &ExecutionFlowGraph,
    domain_policy: &DomainPolicy,
    path_policy: &crate::config::PathPolicy,
) -> FeatureFirstResult {
    let empty_analysis = DomainAnalysisOutput::default();
    let mut features =
        crate::views::overview::features::build(&empty_analysis, store, execution_flows);

    if features.is_empty() {
        return FeatureFirstResult {
            analysis: DomainAnalysisOutput {
                static_graph: graph.clone(),
                ..Default::default()
            },
            features,
        };
    }

    let feature_data = extract_feature_data(&features, store);
    let (terms, _term_index) =
        tfidf::extract_terms(&feature_data.unit_ids, store, domain_policy);

    let matrix = feature_graph::compute(
        &feature_data.unit_ids,
        &feature_data.resource_ids,
        &feature_data.flow_ids,
        &feature_data.paths,
        &terms,
        store,
        execution_flows,
        domain_policy,
    );

    let feature_paths = extract_feature_data(&features, store).paths;
    let constraints = build_constraints(&features, store, path_policy);
    let endpoint_count = features
        .iter()
        .filter(|feature| matches!(feature.kind, FeatureKind::Endpoint))
        .count();
    let target_clusters = clustering::target_cluster_count(
        endpoint_count.max(1),
        domain_policy.domain_cluster_min,
        domain_policy.domain_cluster_max,
    );
    let clusters = clustering::cluster(
        &matrix,
        &constraints,
        clustering::ClusterOptions {
            merge_threshold: domain_policy.domain_cluster_merge_threshold,
            target_count: target_clusters,
            min_count: domain_policy.domain_cluster_min,
            max_count: domain_policy.domain_cluster_max,
        },
    );

    let formation = form_domains_from_clusters(
        &clusters,
        &features,
        &feature_paths,
        &terms,
        &matrix,
        domain_policy,
        store,
    );

    apply_domain_ids_to_features(&mut features, &formation.cluster_domain_ids);

    eprintln!(
        "[domain] formation features={} endpoints={} clusters={} groups={} target_clusters={}",
        features.len(),
        endpoint_count,
        clusters.len(),
        formation.groups.len(),
        target_clusters,
    );

    let unassigned_unit_ids = collect_unassigned_units(store, &formation.assigned_units, path_policy);
    let mut memberships = formation.memberships;
    for unit_id in &unassigned_unit_ids {
        memberships.push(DomainMembership {
            unit_id: unit_id.clone(),
            domain_id: None,
            domain_ids: Vec::new(),
            kind: MembershipKind::Unknown,
            score: 0,
        });
    }

    let mut groups = formation.groups;
    let relations = aggregate_relations(store, graph, &memberships, &groups);
    groups.sort_by(|a, b| a.key.cmp(&b.key));

    let dynamic_reference_ids = graph
        .edges
        .iter()
        .filter(|reference| reference.status == ResolutionStatus::Dynamic)
        .map(|reference| reference.id.clone())
        .collect();

    FeatureFirstResult {
        analysis: DomainAnalysisOutput {
            static_graph: graph.clone(),
            groups,
            relations,
            memberships,
            unassigned_unit_ids,
            dynamic_reference_ids,
            signal_count: 0,
        },
        features,
    }
}

struct FeatureData {
    unit_ids: Vec<Vec<String>>,
    resource_ids: Vec<Vec<String>>,
    flow_ids: Vec<Vec<String>>,
    paths: Vec<HashSet<String>>,
}

fn extract_feature_data(features: &[FeatureGroup], store: &FactStore) -> FeatureData {
    FeatureData {
        unit_ids: features.iter().map(|f| f.unit_ids.clone()).collect(),
        resource_ids: features.iter().map(|f| f.resource_ids.clone()).collect(),
        flow_ids: features.iter().map(|f| f.flow_ids.clone()).collect(),
        paths: features
            .iter()
            .map(|f| {
                f.unit_ids
                    .iter()
                    .filter_map(|uid| {
                        store
                            .unit(uid)
                            .map(|u| u.relative_path.replace('\\', "/"))
                    })
                    .collect()
            })
            .collect(),
    }
}

struct FormationResult {
    groups: Vec<DomainGroup>,
    memberships: Vec<DomainMembership>,
    assigned_units: HashSet<String>,
    cluster_domain_ids: Vec<(Vec<usize>, String)>,
}

fn form_domains_from_clusters(
    clusters: &[clustering::Cluster],
    features: &[FeatureGroup],
    feature_paths: &[HashSet<String>],
    terms: &[tfidf::FeatureTerms],
    matrix: &feature_graph::SimilarityMatrix,
    domain_policy: &DomainPolicy,
    store: &FactStore,
) -> FormationResult {
    let mut groups: Vec<DomainGroup> = Vec::new();
    let mut memberships = Vec::new();
    let mut assigned_units = HashSet::new();
    let mut cluster_domain_ids = Vec::new();
    let mut unit_membership_idx: HashMap<String, usize> = HashMap::new();
    let mut group_index_by_id: HashMap<String, usize> = HashMap::new();

    for cluster in clusters {
        let cluster_key = pick_domain_key(cluster, terms, feature_paths, domain_policy);
        let cohesion = compute_cohesion(cluster, matrix);
        let separation = compute_separation(cluster, clusters, matrix);
        let evidence_diversity = compute_evidence_diversity(cluster, matrix);
        let (status, confidence) = calculate_from_cluster(
            cluster.members.len(),
            cohesion,
            separation,
            evidence_diversity,
        );
        cluster_domain_ids.push((
            cluster.members.clone(),
            stable_domain_id(&cluster_key),
        ));
        let domain_id = cluster_domain_ids
            .last()
            .map(|(_, id)| id.clone())
            .expect("클러스터 도메인 ID가 있어야 한다");
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
            let feature = &features[member_idx];
            if let Some(&group_idx) = group_index_by_id.get(&domain_id) {
                let group = &mut groups[group_idx];
                if !group.feature_ids.contains(&feature.id) {
                    group.feature_ids.push(feature.id.clone());
                }
                for entrypoint_id in &feature.entrypoint_ids {
                    if !group.entrypoint_ids.contains(entrypoint_id) {
                        group.entrypoint_ids.push(entrypoint_id.clone());
                    }
                }
                for resource_id in &feature.resource_ids {
                    if !group.resource_ids.contains(resource_id) {
                        group.resource_ids.push(resource_id.clone());
                    }
                }
            }

            for unit_id in &feature.unit_ids {
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
        group.feature_ids.sort();
        group.feature_ids.dedup();
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
        cluster_domain_ids,
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
    if policy.cross_cutting_keys.contains(key) {
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

fn dominant_directory_key(
    paths: &HashSet<String>,
    domain_policy: &DomainPolicy,
) -> Option<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for path in paths {
        let normalized = path.replace('\\', "/");
        let parent = normalized.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
        for segment in parent.split('/') {
            let segment = segment.to_ascii_lowercase();
            if segment.len() >= domain_policy.minimum_token_length
                && !domain_policy.is_generic(&segment)
            {
                *counts.entry(segment).or_default() += 1;
            }
        }
    }
    counts
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)))
        .map(|(key, _)| key)
}

fn pick_domain_key(
    cluster: &clustering::Cluster,
    terms: &[tfidf::FeatureTerms],
    feature_paths: &[HashSet<String>],
    domain_policy: &DomainPolicy,
) -> String {
    let mut directory_votes: HashMap<String, usize> = HashMap::new();
    for &member_idx in &cluster.members {
        if let Some(key) = dominant_directory_key(&feature_paths[member_idx], domain_policy) {
            *directory_votes.entry(key).or_default() += 1;
        }
    }
    if let Some((directory_key, _)) = directory_votes
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)))
    {
        return directory_key;
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

fn apply_domain_ids_to_features(
    features: &mut [FeatureGroup],
    cluster_domain_ids: &[(Vec<usize>, String)],
) {
    for (member_indices, domain_id) in cluster_domain_ids {
        for &member_idx in member_indices {
            features[member_idx].domain_ids.push(domain_id.clone());
            features[member_idx].domain_ids.sort();
            features[member_idx].domain_ids.dedup();
        }
    }
}

fn collect_unassigned_units(
    store: &FactStore,
    assigned_units: &HashSet<String>,
    path_policy: &crate::config::PathPolicy,
) -> Vec<String> {
    store
        .units
        .keys()
        .filter(|uid| !assigned_units.contains(*uid))
        .filter(|uid| {
            store
                .unit(uid)
                .map(|u| !path_policy.is_test_path(&u.relative_path))
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

fn build_constraints(
    features: &[FeatureGroup],
    store: &FactStore,
    path_policy: &crate::config::PathPolicy,
) -> clustering::MergeConstraints {
    let is_test_feature = |feature: &FeatureGroup| -> bool {
        feature.unit_ids.iter().any(|unit_id| {
            store
                .unit(unit_id)
                .map(|unit| path_policy.is_test_path(&unit.relative_path))
                .unwrap_or(false)
        })
    };
    let mut forbidden_pairs = Vec::new();
    for (i, feature_a) in features.iter().enumerate() {
        for (j, feature_b) in features.iter().enumerate().skip(i + 1) {
            if is_test_feature(feature_a) != is_test_feature(feature_b) {
                forbidden_pairs.push((i, j));
            }
        }
    }
    clustering::MergeConstraints { forbidden_pairs }
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
    let mut has_call = false;
    let mut has_flow = false;
    let mut has_resource = false;
    let mut has_path = false;
    let mut has_lexical = false;
    for (idx_a, &a) in cluster.members.iter().enumerate() {
        for &b in cluster.members.iter().skip(idx_a + 1) {
            let sim = matrix.get(a, b);
            if sim.call > 0.0 {
                has_call = true;
            }
            if sim.flow > 0.0 {
                has_flow = true;
            }
            if sim.resource > 0.0 {
                has_resource = true;
            }
            if sim.path > 0.0 {
                has_path = true;
            }
            if sim.lexical > 0.0 {
                has_lexical = true;
            }
        }
    }
    let count = [has_call, has_flow, has_resource, has_path, has_lexical]
        .iter()
        .filter(|&&v| v)
        .count();
    count as f64 / 5.0
}
