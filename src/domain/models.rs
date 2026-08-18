//! 도메인 분석에서 공유하는 데이터 모델.

use crate::domain::confidence::{DomainConfidence, DomainStatus};
use crate::domain::formation::diagnostics::DomainFormationDiagnostics;
use crate::domain::membership::DomainMembership;
use crate::facts::{Evidence, ResolutionStatus};
use crate::graph::StaticRelationGraph;
use serde::{Deserialize, Serialize};

/// 프론트 Overview에서 표시할 도메인 종류다.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DomainKind {
    Business,
    CrossCutting,
    External,
    Unknown,
}

/// 하나의 도메인 그룹이다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainGroup {
    pub id: String,
    pub key: String,
    pub label: String,
    pub kind: DomainKind,
    pub status: DomainStatus,
    pub confidence: DomainConfidence,
    pub primary_unit_ids: Vec<String>,
    pub shared_unit_ids: Vec<String>,
    pub entrypoint_ids: Vec<String>,
    /// 이 도메인에 속한 정적 기능 그룹 ID다.
    #[serde(default)]
    pub feature_ids: Vec<String>,
    pub resource_ids: Vec<String>,
    pub evidence: Vec<Evidence>,
    pub summary: Option<String>,
}

/// 도메인 간 집계 관계다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainRelation {
    pub source_domain_id: String,
    pub target_domain_id: String,
    pub kind: String,
    pub status: ResolutionStatus,
    pub weight: u32,
    pub evidence: Vec<Evidence>,
}

/// 도메인 분석의 내부·외부 공통 결과다.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DomainAnalysisOutput {
    pub static_graph: StaticRelationGraph,
    pub groups: Vec<DomainGroup>,
    pub relations: Vec<DomainRelation>,
    pub memberships: Vec<DomainMembership>,
    pub unassigned_unit_ids: Vec<String>,
    pub dynamic_reference_ids: Vec<String>,
    /// 그룹화에 사용한 내부 신호 개수다. 개별 신호는 후보·멤버십·증거에
    /// 반영된 뒤 보관하지 않아 대형 프로젝트의 메모리 사용량을 억제한다.
    pub signal_count: usize,
    #[serde(default, skip_serializing_if = "DomainFormationDiagnostics::is_empty")]
    pub formation_diagnostics: DomainFormationDiagnostics,
}
