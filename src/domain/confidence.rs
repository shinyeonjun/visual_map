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
    #[serde(default)]
    pub cohesion: f64,
    #[serde(default)]
    pub separation: f64,
    #[serde(default)]
    pub evidence_diversity: f64,
    #[serde(default)]
    pub overall: f64,
}

/// Feature-first 클러스터링에서 도메인 확신도를 계산한다.
pub fn calculate_from_cluster(
    member_count: usize,
    cohesion: f64,
    separation: f64,
    evidence_diversity: f64,
) -> (DomainStatus, DomainConfidence) {
    let overall = 0.4 * cohesion + 0.3 * separation + 0.3 * evidence_diversity;
    let (status, level) =
        if member_count >= 2 && overall >= 0.6 && evidence_diversity >= 0.4 && separation >= 0.3 {
            (DomainStatus::Confirmed, "high")
        } else if overall >= 0.3 {
            (DomainStatus::Candidate, "medium")
        } else if overall > 0.0 {
            (DomainStatus::Ambiguous, "low")
        } else {
            (DomainStatus::Unknown, "none")
        };
    let score = (overall * 100.0).round() as u32;
    (
        status,
        DomainConfidence {
            level: level.to_string(),
            score,
            signal_families: BTreeSet::new(),
            cohesion,
            separation,
            evidence_diversity,
            overall,
        },
    )
}
