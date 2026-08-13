use crate::config::DomainPolicy;
use crate::domain::signals::DomainSignalKind;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// 도메인 후보의 판단 상태다.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DomainStatus {
    Confirmed,
    Candidate,
    Ambiguous,
    Unknown,
}

/// 신호 종류와 점수로 표현한 도메인 확신도다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainConfidence {
    pub level: String,
    pub score: u32,
    pub signal_families: BTreeSet<DomainSignalKind>,
}

pub fn calculate(
    score: u32,
    families: &BTreeSet<DomainSignalKind>,
    ambiguous: bool,
    policy: &DomainPolicy,
) -> (DomainStatus, DomainConfidence) {
    let status = if ambiguous {
        DomainStatus::Ambiguous
    } else if families.len() >= policy.confirmed_minimum_signal_families
        && score >= policy.confirmed_minimum_score
    {
        DomainStatus::Confirmed
    } else if score > 0 {
        DomainStatus::Candidate
    } else {
        DomainStatus::Unknown
    };
    let level = match status {
        DomainStatus::Confirmed => "high",
        DomainStatus::Candidate => "medium",
        DomainStatus::Ambiguous => "low",
        DomainStatus::Unknown => "none",
    };
    (
        status,
        DomainConfidence {
            level: level.to_string(),
            score,
            signal_families: families.clone(),
        },
    )
}
