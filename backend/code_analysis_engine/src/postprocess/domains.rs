//! Codex 컨텍스트에 사용할 도메인 lineage와 보수적 병합 계획.

use super::indexes::PostprocessIndexes;
use super::model::{
    ContextDomain, DomainAlias, DomainCluster, DomainDecision, DomainRole, SuppressedDomain,
};
use crate::config::{DomainPolicy, PostprocessPolicy};
use crate::domain::confidence::DomainStatus;
use crate::domain::DomainKind;
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
            status: domain.status,
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.id.cmp(&right.id));

    let mut suppressed = Vec::new();
    candidates.retain(|candidate| {
        let should_suppress = matches!(candidate.role, DomainRole::Noise)
            && candidate.unit_ids.len() <= policy.generic_domain_max_units
            && matches!(
                candidate.status,
                DomainStatus::Unknown | DomainStatus::Candidate
            );
        if should_suppress {
            suppressed.push(SuppressedDomain {
                domain_id: candidate.id.clone(),
                key: candidate.key.clone(),
                reason: "generic_low_signal_domain".into(),
                unit_count: candidate.unit_ids.len(),
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
            normalized_key(&candidate.key) == normalized_key(&representative.key)
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
        source_paths,
        entrypoints: Vec::new(),
        resources: Vec::new(),
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

fn normalized_key(value: &str) -> String {
    tokenize(value)
        .into_iter()
        .map(|token| singularize(&token))
        .collect::<Vec<_>>()
        .join("::")
}

fn singularize(token: &str) -> String {
    if token.len() > 4 && token.ends_with("ies") {
        return format!("{}y", &token[..token.len() - 3]);
    }
    if token.len() > 3 && token.ends_with('s') && !token.ends_with("ss") {
        return token[..token.len() - 1].to_string();
    }
    token.to_string()
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
    status: DomainStatus,
}
