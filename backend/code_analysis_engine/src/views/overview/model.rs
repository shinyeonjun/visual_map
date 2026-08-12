use crate::domain::{DomainGroup, DomainRelation};
use crate::facts::{CodeUnit, Entrypoint, Evidence, ReferenceKind, ResourceAccess};
use crate::flow::ExecutionFlowGraph;
use crate::frameworks::registry::detector::FrameworkDetection;
use crate::graph::StaticRelationGraph;
use crate::semantic::SemanticStatus;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 언어 하나의 분석 커버리지다.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LanguageCoverage {
    pub detected_files: usize,
    pub parsed_files: usize,
    pub partial_files: usize,
    pub failed_files: usize,
    pub extracted_units: usize,
    pub extracted_references: usize,
}

/// 1단계 전체 분석 커버리지다.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisCoverage {
    pub total_files: usize,
    pub parsed_files: usize,
    pub partial_files: usize,
    pub failed_files: usize,
    pub languages: BTreeMap<String, LanguageCoverage>,
    pub total_units: usize,
    pub total_references: usize,
    pub total_entrypoints: usize,
    pub total_resources: usize,
    pub total_features: usize,
    pub total_execution_flows: usize,
    pub total_confirmed_references: usize,
    pub total_candidate_references: usize,
    pub total_unknown_references: usize,
    pub total_dynamic_references: usize,
    pub total_dynamic_boundaries: usize,
    /// target unit이 없는 모든 참조의 수다. 동적 참조도 하위 집합으로
    /// 포함되므로 상세 분류가 필요하면 위 상태별 필드를 사용한다.
    pub total_unresolved_references: usize,
}

/// 정적 분석으로 목적지를 확정할 수 없는 동적 경계다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DynamicBoundary {
    pub id: String,
    pub source_unit_id: String,
    pub target_name: String,
    pub kind: ReferenceKind,
    pub evidence: Vec<Evidence>,
}

/// 정적 분석으로 묶은 2단계 기능의 종류다.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FeatureKind {
    Endpoint,
    Operation,
}

/// 기능 그룹 자체와 내부 호출 경로의 확정 상태다.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FeatureStatus {
    Confirmed,
    Candidate,
    Unknown,
}

/// 기능의 정적 근거 품질을 수치가 아니라 구성 요소별로 보존한다.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FeatureConfidence {
    pub level: String,
    pub resolved_edge_count: usize,
    pub unresolved_edge_count: usize,
    pub dynamic_edge_count: usize,
    pub evidence_count: usize,
}

/// 도메인 안에서 프론트엔드가 2단계로 보여줄 정적 기능 그룹이다.
///
/// 기능은 entrypoint에서 호출 그래프를 따라 수집한 코드 유닛 집합이다.
/// 동적·미해결 호출은 `dynamicBoundaryIds`와 confidence에 남기고 삭제하지 않는다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureGroup {
    pub id: String,
    pub key: String,
    pub label: String,
    pub kind: FeatureKind,
    pub status: FeatureStatus,
    pub confidence: FeatureConfidence,
    pub domain_ids: Vec<String>,
    /// 응답에 직접 materialize한 유닛이다. 대형 operation은 전체 전이
    /// 폐쇄를 중복 직렬화하지 않고 소유 유닛만 담는다.
    pub unit_ids: Vec<String>,
    /// 정적으로 도달 가능한 전체 유닛 수다. 상세 유닛은 staticGraph와
    /// unitIds를 이용해 별도 조회할 수 있다.
    #[serde(default)]
    pub reachable_unit_count: usize,
    pub entrypoint_ids: Vec<String>,
    pub flow_ids: Vec<String>,
    pub resource_ids: Vec<String>,
    pub dynamic_boundary_ids: Vec<String>,
    pub evidence: Vec<Evidence>,
    pub summary: Option<String>,
}

/// Codex 의미 분석을 청크 단위로 실행한 결과 요약이다.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SemanticAnalysisSummary {
    pub provider: String,
    pub chunk_count: usize,
    pub completed_chunks: usize,
    pub failed_chunks: usize,
    pub max_input_bytes: usize,
}

/// 프론트엔드가 1단계 Overview 화면에서 바로 사용하는 응답이다.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OverviewResponse {
    pub schema_version: String,
    pub domains: Vec<DomainGroup>,
    pub features: Vec<FeatureGroup>,
    pub relations: Vec<DomainRelation>,
    pub static_graph: StaticRelationGraph,
    pub execution_flows: ExecutionFlowGraph,
    pub units: Vec<CodeUnit>,
    pub entrypoints: Vec<Entrypoint>,
    pub resources: Vec<ResourceAccess>,
    pub unassigned_unit_ids: Vec<String>,
    pub dynamic_boundary_ids: Vec<String>,
    pub dynamic_boundaries: Vec<DynamicBoundary>,
    pub detected_frameworks: Vec<FrameworkDetection>,
    pub semantic_status: SemanticStatus,
    pub semantic_analysis: SemanticAnalysisSummary,
    pub coverage: AnalysisCoverage,
}
