//! Codex 컨텍스트에 사용할 도메인 lineage와 보수적 병합 계획.

use super::indexes::PostprocessIndexes;
use super::model::{
    ContextDomain, DomainAlias, DomainCluster, DomainDecision, DomainRole, SuppressedDomain,
};
use crate::config::{DomainPolicy, PostprocessPolicy};
use crate::domain::DomainKind;
use crate::facts::ResolutionStatus;
use std::collections::{BTreeSet, HashSet};

pub(crate) struct DomainPlan {
    pub clusters: Vec<DomainCluster>,
    pub aliases: Vec<DomainAlias>,
    pub suppressed: Vec<SuppressedDomain>,
}

pub(crate) fn build_plan(
    indexes: &PostprocessIndexes<'_>,
    domain_policy: &DomainPolicy,
    policy: &PostprocessPolicy,
) -> DomainPlan {
    let mut candidates = indexes
        .overview
        .domains
        .iter()
        .filter(|domain| has_visible_unit(domain, indexes))
        .map(|domain| DomainCandidate {
            id: domain.id.clone(),
            key: domain.key.clone(),
            role: domain_role(domain, domain_policy),
            unit_ids: visible_unit_ids(domain, indexes),
            has_business_anchor: has_business_anchor(domain),
            signal: domain_signal(domain, indexes, domain_policy, policy),
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.id.cmp(&right.id));

    let mut suppressed = Vec::new();
    candidates.retain(|candidate| {
        let (should_suppress, reason) =
            if matches!(candidate.role, DomainRole::Noise) && !candidate.has_business_anchor {
                (true, "generic_without_business_anchor")
            } else if matches!(candidate.role, DomainRole::CrossCutting)
                && !policy.include_cross_cutting_domains
            {
                (true, "cross_cutting_context_excluded")
            } else {
                (false, "")
            };
        if should_suppress {
            suppressed.push(SuppressedDomain {
                domain_id: candidate.id.clone(),
                key: candidate.key.clone(),
                reason: reason.into(),
                unit_count: candidate.unit_ids.len(),
                signal: Some(candidate.signal.clone()),
            });
        }
        !should_suppress
    });

    let mut clusters = Vec::<DomainCluster>::new();
    for candidate in candidates {
        let matching_cluster = clusters.iter().position(|cluster| {
            let Some(representative) = indexes.domains.get(cluster.representative_id.as_str())
            else {
                return false;
            };
            if matches!(candidate.role, DomainRole::CrossCutting)
                || matches!(
                    domain_role(representative, domain_policy),
                    DomainRole::CrossCutting
                )
            {
                return false;
            }
            keys_equivalent(&candidate.key, &representative.key)
                && overlap_percent(
                    &candidate.unit_ids,
                    &visible_unit_ids(representative, indexes),
                ) >= policy.domain_overlap_percent
        });

        if let Some(cluster_index) = matching_cluster {
            clusters[cluster_index]
                .source_domain_ids
                .push(candidate.id.clone());
            clusters[cluster_index].source_domain_ids.sort_unstable();
            clusters[cluster_index].decision = DomainDecision::AliasMerged;
            clusters[cluster_index].reason = Some("normalized_key_and_unit_overlap".into());
        } else {
            let representative_id = candidate.id.clone();
            clusters.push(DomainCluster {
                representative_id,
                source_domain_ids: vec![candidate.id],
                decision: DomainDecision::Original,
                reason: None,
                signal: candidate.signal.clone(),
            });
        }
    }

    let mut aliases = Vec::new();
    for cluster in &clusters {
        for source_id in &cluster.source_domain_ids {
            if source_id != &cluster.representative_id {
                aliases.push(DomainAlias {
                    from_domain_id: source_id.clone(),
                    to_domain_id: cluster.representative_id.clone(),
                    reason: cluster
                        .reason
                        .clone()
                        .unwrap_or_else(|| "domain_alias".into()),
                    source_domain_ids: cluster.source_domain_ids.clone(),
                });
            }
        }
    }

    DomainPlan {
        clusters,
        aliases,
        suppressed,
    }
}

pub(crate) fn domain_context(
    cluster: &DomainCluster,
    indexes: &PostprocessIndexes<'_>,
    domain_policy: &DomainPolicy,
) -> Option<ContextDomain> {
    let domain = indexes.domains.get(cluster.representative_id.as_str())?;
    let source_paths = cluster
        .source_domain_ids
        .iter()
        .flat_map(|domain_id| {
            indexes
                .domains
                .get(domain_id.as_str())
                .into_iter()
                .flat_map(|domain| {
                    domain
                        .primary_unit_ids
                        .iter()
                        .chain(&domain.shared_unit_ids)
                })
                .filter_map(|unit_id| indexes.unit(unit_id).map(|unit| unit.relative_path.clone()))
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let evidence_ids = cluster
        .source_domain_ids
        .iter()
        .flat_map(|domain_id| {
            indexes
                .domains
                .get(domain_id.as_str())
                .into_iter()
                .flat_map(|domain| domain.evidence.iter().map(|evidence| evidence.id.clone()))
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    Some(ContextDomain {
        domain_id: cluster.representative_id.clone(),
        source_domain_ids: cluster.source_domain_ids.clone(),
        current_label: domain.label.clone(),
        role: domain_role(domain, domain_policy),
        decision: cluster.decision.clone(),
        signal: cluster.signal.clone(),
        source_paths,
        entrypoints: Vec::new(),
        resources: Vec::new(),
        feature_ids: Vec::new(),
        flow_ids: Vec::new(),
        features: Vec::new(),
        flows: Vec::new(),
        evidence_ids,
        omission: Default::default(),
    })
}

fn has_visible_unit(domain: &crate::domain::DomainGroup, indexes: &PostprocessIndexes<'_>) -> bool {
    domain
        .primary_unit_ids
        .iter()
        .chain(&domain.shared_unit_ids)
        .any(|unit_id| indexes.visible_unit_ids.contains(unit_id))
}

fn has_business_anchor(domain: &crate::domain::DomainGroup) -> bool {
    !domain.entrypoint_ids.is_empty()
        || !domain.resource_ids.is_empty()
        || domain.confidence.signal_families.iter().any(|family| {
            matches!(
                family,
                crate::domain::signals::DomainSignalKind::Entrypoint
                    | crate::domain::signals::DomainSignalKind::Resource
            )
        })
}

fn domain_signal(
    domain: &crate::domain::DomainGroup,
    indexes: &PostprocessIndexes<'_>,
    domain_policy: &DomainPolicy,
    policy: &PostprocessPolicy,
) -> super::model::DomainSignal {
    let unit_ids = visible_unit_ids(domain, indexes);
    let unit_set = unit_ids.iter().map(String::as_str).collect::<HashSet<_>>();
    let has_entrypoint = !domain.entrypoint_ids.is_empty();
    let has_resource = !domain.resource_ids.is_empty();
    let anchor = match (has_entrypoint, has_resource) {
        (true, true) => 1.0,
        (true, false) | (false, true) => 0.6,
        (false, false) => 0.0,
    };
    let touching = indexes
        .overview
        .static_graph
        .edges
        .iter()
        .filter(|edge| {
            unit_set.contains(edge.source_unit_id.as_str())
                || edge
                    .target_unit_id
                    .as_deref()
                    .map(|id| unit_set.contains(id))
                    .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    let confirmed = touching
        .iter()
        .filter(|edge| edge.status == ResolutionStatus::Confirmed)
        .count();
    let density = if touching.is_empty() {
        0.0
    } else {
        confirmed as f64 / touching.len() as f64
    };
    let tokens = tokenize(&domain.key);
    let specificity = if tokens.is_empty() {
        0.0
    } else {
        tokens
            .iter()
            .filter(|token| !domain_policy.is_generic(token))
            .count() as f64
            / tokens.len() as f64
    };
    let confidence = match domain.status {
        crate::domain::confidence::DomainStatus::Confirmed => 1.0,
        crate::domain::confidence::DomainStatus::Candidate => 0.5,
        crate::domain::confidence::DomainStatus::Ambiguous => 0.2,
        crate::domain::confidence::DomainStatus::Unknown => 0.0,
    };
    let weights = [
        policy.signal_anchor_weight,
        policy.signal_density_weight,
        policy.signal_specificity_weight,
        policy.signal_confidence_weight,
    ];
    let weight_total = weights.iter().map(|weight| u64::from(*weight)).sum::<u64>();
    let weighted = if weight_total == 0 {
        0.0
    } else {
        (anchor * f64::from(policy.signal_anchor_weight)
            + density * f64::from(policy.signal_density_weight)
            + specificity * f64::from(policy.signal_specificity_weight)
            + confidence * f64::from(policy.signal_confidence_weight))
            / weight_total as f64
    };
    super::model::DomainSignal {
        score: (weighted * 1000.0).round() as u32,
        anchor,
        density,
        specificity,
        confidence,
        has_business_anchor: has_business_anchor(domain),
    }
}

fn visible_unit_ids(
    domain: &crate::domain::DomainGroup,
    indexes: &PostprocessIndexes<'_>,
) -> Vec<String> {
    domain
        .primary_unit_ids
        .iter()
        .chain(&domain.shared_unit_ids)
        .filter(|unit_id| indexes.visible_unit_ids.contains(*unit_id))
        .cloned()
        .collect()
}

fn domain_role(domain: &crate::domain::DomainGroup, policy: &DomainPolicy) -> DomainRole {
    let tokens = tokenize(&domain.key);
    if policy.cross_cutting_keys.contains(&domain.key)
        || tokens
            .iter()
            .any(|token| policy.cross_cutting_keys.contains(token))
    {
        return DomainRole::CrossCutting;
    }
    match domain.kind {
        DomainKind::Business => {
            if tokens.iter().all(|token| policy.is_generic(token)) {
                DomainRole::Noise
            } else {
                DomainRole::Business
            }
        }
        DomainKind::CrossCutting => DomainRole::CrossCutting,
        DomainKind::External => DomainRole::External,
        DomainKind::Unknown => DomainRole::Unknown,
    }
}

fn tokenize(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

fn keys_equivalent(left: &str, right: &str) -> bool {
    let left_variants = key_variants(left);
    let right_variants = key_variants(right);
    left_variants.intersection(&right_variants).next().is_some()
}

fn key_variants(value: &str) -> BTreeSet<String> {
    let tokens = tokenize(value);
    let mut variants = BTreeSet::new();
    variants.insert(tokens.join("::"));
    let singular_tokens = tokens
        .iter()
        .map(|token| safe_singular_variants(token))
        .collect::<Vec<_>>();
    let mut combinations = vec![Vec::<String>::new()];
    for token_variants in singular_tokens {
        let mut next = Vec::new();
        for combination in &combinations {
            for variant in &token_variants {
                let mut extended = combination.clone();
                extended.push(variant.clone());
                next.push(extended);
            }
        }
        combinations = next;
    }
    variants.extend(combinations.into_iter().map(|tokens| tokens.join("::")));
    variants
}

fn safe_singular_variants(token: &str) -> Vec<String> {
    let mut variants = vec![token.to_string()];
    if token.len() > 4 && token.ends_with("ies") {
        variants.push(format!("{}y", &token[..token.len() - 3]));
    }
    if token.len() > 3
        && token.ends_with('s')
        && !["ss", "us", "is", "as", "os"]
            .iter()
            .any(|suffix| token.ends_with(suffix))
    {
        variants.push(token[..token.len() - 1].to_string());
    }
    if token.len() > 4 && token.ends_with("es") {
        variants.push(token[..token.len() - 2].to_string());
    }
    variants
}

fn overlap_percent(left: &[String], right: &[String]) -> u32 {
    let left_set = left.iter().collect::<HashSet<_>>();
    let right_set = right.iter().collect::<HashSet<_>>();
    let smaller = left_set.len().min(right_set.len());
    if smaller == 0 {
        return 0;
    }
    let intersection = left_set.intersection(&right_set).count();
    ((intersection * 100) / smaller) as u32
}

struct DomainCandidate {
    id: String,
    key: String,
    role: DomainRole,
    unit_ids: Vec<String>,
    has_business_anchor: bool,
    signal: super::model::DomainSignal,
}
