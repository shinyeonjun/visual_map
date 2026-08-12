//! 도메인 후보 점수와 코드 유닛 멤버십을 계산한다.

use crate::config::DomainPolicy;
use crate::domain::candidates::DomainCandidate;
use crate::domain::grouping::stable_domain_id;
use crate::domain::membership::{DomainMembership, MembershipKind};
use crate::domain::signals::DomainSignal;
use crate::facts::FactStore;
use std::collections::{BTreeMap, HashMap};

pub(super) fn score_by_unit(signals: &[DomainSignal]) -> HashMap<String, BTreeMap<String, u32>> {
    let mut scores = HashMap::new();
    for signal in signals {
        *scores
            .entry(signal.unit_id.clone())
            .or_insert_with(BTreeMap::new)
            .entry(signal.domain_key.clone())
            .or_insert(0) += signal.weight;
    }
    scores
}

pub(super) fn assign_units(
    store: &FactStore,
    candidates: &[DomainCandidate],
    scores: &HashMap<String, BTreeMap<String, u32>>,
    policy: &DomainPolicy,
) -> Vec<DomainMembership> {
    let candidate_ids: HashMap<&str, String> = candidates
        .iter()
        .map(|candidate| (candidate.key.as_str(), stable_domain_id(&candidate.key)))
        .collect();
    let mut result = Vec::new();
    for unit in store.units.values() {
        let Some(unit_scores) = scores.get(&unit.id) else {
            result.push(unknown_membership(unit.id.clone()));
            continue;
        };
        let Some((key, score, second_score)) = best_scores(unit_scores) else {
            result.push(unknown_membership(unit.id.clone()));
            continue;
        };
        let domain_id = candidate_ids.get(key.as_str()).cloned();
        let kind = if score == 0 {
            MembershipKind::Unknown
        } else if second_score >= policy.shared_threshold(score) {
            MembershipKind::Shared
        } else {
            MembershipKind::Primary
        };
        let domain_ids = if kind == MembershipKind::Shared {
            let threshold = policy.shared_threshold(score);
            unit_scores
                .iter()
                .filter(|(_, value)| **value >= threshold)
                .filter_map(|(candidate_key, _)| candidate_ids.get(candidate_key.as_str()).cloned())
                .collect()
        } else {
            domain_id.clone().into_iter().collect()
        };
        result.push(DomainMembership {
            unit_id: unit.id.clone(),
            domain_id,
            domain_ids,
            kind,
            score,
        });
    }
    result
}

/// 한 유닛의 최고·차점 도메인 점수를 정렬 없이 계산한다.
///
/// 기존 구현은 유닛마다 모든 후보를 Vec으로 복사하고 정렬했다. 대형 C++
/// 프로젝트에서는 유닛 수가 수십만 개가 되므로 최고점 하나와 차점 점수만
/// 선형 탐색하는 편이 훨씬 싸다. shared 멤버십을 만들 때는 원래 점수 맵을
/// 다시 순회하므로 결과의 tie-break와 멤버십은 변하지 않는다.
fn best_scores(scores: &BTreeMap<String, u32>) -> Option<(&String, u32, u32)> {
    let mut best: Option<(&String, u32)> = None;
    let mut second_score = 0;
    for (key, score) in scores {
        match best {
            None => best = Some((key, *score)),
            Some((_best_key, best_value)) if *score > best_value => {
                second_score = best_value;
                best = Some((key, *score));
            }
            Some((best_key, best_value)) => {
                if *score == best_value && key < best_key {
                    second_score = second_score.max(best_value);
                    best = Some((key, *score));
                } else if *score > second_score {
                    second_score = *score;
                }
            }
        }
    }
    best.map(|(key, score)| (key, score, second_score))
}

fn unknown_membership(unit_id: String) -> DomainMembership {
    DomainMembership {
        unit_id,
        domain_id: None,
        domain_ids: Vec::new(),
        kind: MembershipKind::Unknown,
        score: 0,
    }
}

pub(super) fn index_assignments_by_domain(
    assignments: &[DomainMembership],
) -> HashMap<&str, Vec<&DomainMembership>> {
    let mut result = HashMap::new();
    for assignment in assignments {
        for domain_id in &assignment.domain_ids {
            result
                .entry(domain_id.as_str())
                .or_insert_with(Vec::new)
                .push(assignment);
        }
    }
    result
}

pub(super) fn index_domains_by_unit(assignments: &[DomainMembership]) -> HashMap<&str, Vec<&str>> {
    let mut result = HashMap::new();
    for assignment in assignments {
        let domains = result
            .entry(assignment.unit_id.as_str())
            .or_insert_with(Vec::new);
        for domain_id in &assignment.domain_ids {
            domains.push(domain_id.as_str());
        }
    }
    result
}

pub(super) fn index_entrypoints_by_domain(
    store: &FactStore,
    domains_by_unit: &HashMap<&str, Vec<&str>>,
) -> HashMap<String, Vec<String>> {
    let mut result = HashMap::new();
    for entrypoint in &store.entrypoints {
        let Some(domains) = domains_by_unit.get(entrypoint.unit_id.as_str()) else {
            continue;
        };
        for domain_id in domains {
            result
                .entry((*domain_id).to_owned())
                .or_insert_with(Vec::new)
                .push(entrypoint.id.clone());
        }
    }
    result
}

pub(super) fn index_resources_by_domain(
    store: &FactStore,
    domains_by_unit: &HashMap<&str, Vec<&str>>,
) -> HashMap<String, Vec<String>> {
    let mut result = HashMap::new();
    for resource in &store.resources {
        let Some(domains) = domains_by_unit.get(resource.unit_id.as_str()) else {
            continue;
        };
        for domain_id in domains {
            result
                .entry((*domain_id).to_owned())
                .or_insert_with(Vec::new)
                .push(resource.id.clone());
        }
    }
    result
}
