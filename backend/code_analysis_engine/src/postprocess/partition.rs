//! 도메인 shell을 Codex byte budget 안에서 관계 보존형으로 분할한다.

use super::model::ContextDomain;
use crate::config::PostprocessPolicy;
use crate::facts::ResolutionStatus;
use crate::views::overview::OverviewResponse;
use std::collections::{BTreeSet, HashMap};

pub(crate) fn partition_domains(
    domains: &[ContextDomain],
    overview: &OverviewResponse,
    policy: &PostprocessPolicy,
) -> Vec<Vec<String>> {
    let mut shells = domains
        .iter()
        .map(|domain| {
            (
                domain.domain_id.clone(),
                serde_json::to_vec(domain)
                    .map(|bytes| bytes.len())
                    .unwrap_or(usize::MAX),
            )
        })
        .collect::<HashMap<_, _>>();
    let available = policy
        .target_budget_bytes
        .saturating_sub(policy.global_summary_reserve_bytes);
    let mut remaining = domains
        .iter()
        .map(|domain| domain.domain_id.clone())
        .collect::<BTreeSet<_>>();
    let mut partitions = Vec::new();
    while !remaining.is_empty() {
        let seed = remaining
            .iter()
            .filter_map(|id| domains.iter().find(|domain| &domain.domain_id == id))
            .max_by(|left, right| {
                left.signal
                    .score
                    .cmp(&right.signal.score)
                    .then_with(|| right.domain_id.cmp(&left.domain_id))
            })
            .map(|domain| domain.domain_id.clone())
            .expect("남은 도메인에는 shell이 존재해야 한다");
        remaining.remove(&seed);
        let mut partition = vec![seed.clone()];
        let mut used = shells.remove(&seed).unwrap_or(usize::MAX);

        while let Some(next) = remaining
            .iter()
            .filter_map(|id| domains.iter().find(|domain| &domain.domain_id == id))
            .map(|domain| {
                let connection = connection_weight(&partition, &domain.domain_id, overview, policy);
                let shell_bytes = shells.get(&domain.domain_id).copied().unwrap_or(usize::MAX);
                (domain, connection, shell_bytes)
            })
            .filter(|(_, _, shell_bytes)| {
                used.saturating_add(*shell_bytes) <= available || partition.len() == 1
            })
            .max_by(|left, right| {
                left.1
                    .cmp(&right.1)
                    .then_with(|| left.0.signal.score.cmp(&right.0.signal.score))
                    .then_with(|| right.0.domain_id.cmp(&left.0.domain_id))
            })
        {
            remaining.remove(&next.0.domain_id);
            used = used.saturating_add(next.2);
            partition.push(next.0.domain_id.clone());
        }
        partition.sort();
        partitions.push(partition);
    }
    partitions
}

fn connection_weight(
    partition: &[String],
    candidate: &str,
    overview: &OverviewResponse,
    policy: &PostprocessPolicy,
) -> u64 {
    overview
        .relations
        .iter()
        .filter(|relation| {
            (partition.iter().any(|id| id == &relation.source_domain_id)
                && relation.target_domain_id == candidate)
                || (partition.iter().any(|id| id == &relation.target_domain_id)
                    && relation.source_domain_id == candidate)
        })
        .map(|relation| {
            let status_weight = match relation.status {
                ResolutionStatus::Confirmed => policy.partition_confirmed_weight,
                ResolutionStatus::Candidate => policy.partition_candidate_weight,
                ResolutionStatus::Unknown | ResolutionStatus::Dynamic => {
                    policy.partition_unknown_weight
                }
            };
            u64::from(status_weight) * u64::from(relation.weight.max(1))
        })
        .sum()
}
