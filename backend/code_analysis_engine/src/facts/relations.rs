use crate::facts::evidence::Evidence;
use serde::{Deserialize, Serialize};

/// 코드 단위 사이의 정적 관계 종류다.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ReferenceKind {
    Import,
    Export,
    Include,
    Call,
    Implements,
    Extends,
    Constructs,
    Uses,
    Reads,
    Writes,
    Publishes,
    Consumes,
}

/// 정적 관계가 어느 정도 해석되었는지 나타낸다.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ResolutionStatus {
    Confirmed,
    Candidate,
    Dynamic,
    Unknown,
}

/// 코드 단위 사이의 정적 관계다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    pub id: String,
    pub source_unit_id: String,
    pub target_unit_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidate_unit_ids: Vec<String>,
    pub target_name: String,
    pub kind: ReferenceKind,
    pub status: ResolutionStatus,
    pub evidence: Vec<Evidence>,
}
