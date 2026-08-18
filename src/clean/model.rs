use crate::facts::{
    CodeUnitKind, EntrypointKind, Evidence, ReferenceKind, ResolutionStatus, ResourceKind,
};
use crate::flow::ExecutionFlowGraph;
use crate::model::Language;
use crate::views::overview::model::DynamicBoundary;
use crate::views::overview::AnalysisCoverage;
use serde::{Deserialize, Serialize};

/// raw Overview에서 중복 그래프와 테스트 전용 사실을 걷어낸 정적 화면 계약이다.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PreparedStaticOverview {
    pub schema_version: String,
    pub domains: Vec<PreparedDomain>,
    pub features: Vec<PreparedFeature>,
    pub relations: Vec<PreparedRelation>,
    pub references: Vec<PreparedReference>,
    pub units: Vec<PreparedUnit>,
    pub entrypoints: Vec<PreparedEntrypoint>,
    pub resources: Vec<PreparedResource>,
    pub execution_flows: ExecutionFlowGraph,
    pub dynamic_boundaries: Vec<DynamicBoundary>,
    pub frameworks: Vec<String>,
    pub unassigned_unit_ids: Vec<String>,
    pub coverage: AnalysisCoverage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedDomain {
    pub id: String,
    pub candidate_key: String,
    pub label: String,
    pub kind: crate::domain::DomainKind,
    pub status: crate::domain::confidence::DomainStatus,
    pub confidence_level: String,
    pub confidence_score: u32,
    pub unit_ids: Vec<String>,
    pub feature_ids: Vec<String>,
    pub entrypoint_ids: Vec<String>,
    pub resource_ids: Vec<String>,
    pub symbols: Vec<String>,
    pub source_paths: Vec<String>,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedFeature {
    pub id: String,
    pub candidate_key: String,
    pub label: String,
    pub kind: crate::views::overview::FeatureKind,
    pub status: crate::views::overview::FeatureStatus,
    pub visibility: crate::views::overview::model::FeatureVisibility,
    pub domain_ids: Vec<String>,
    pub unit_ids: Vec<String>,
    pub reachable_unit_count: usize,
    pub entrypoint_ids: Vec<String>,
    pub flow_ids: Vec<String>,
    pub resource_ids: Vec<String>,
    pub dynamic_boundary_ids: Vec<String>,
    pub symbols: Vec<String>,
    pub source_paths: Vec<String>,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedRelation {
    pub source_domain_id: String,
    pub target_domain_id: String,
    pub kind: String,
    pub status: crate::facts::ResolutionStatus,
    pub weight: u32,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedReference {
    pub id: String,
    pub source_unit_id: String,
    pub target_unit_id: Option<String>,
    pub kind: ReferenceKind,
    pub status: ResolutionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedUnit {
    pub id: String,
    pub kind: CodeUnitKind,
    pub name: String,
    pub qualified_name: String,
    pub language: Language,
    pub path: String,
    pub parent_id: Option<String>,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedEntrypoint {
    pub id: String,
    pub unit_id: String,
    pub kind: EntrypointKind,
    pub name: String,
    pub method: Option<String>,
    pub path: Option<String>,
    pub framework_id: Option<String>,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedResource {
    pub id: String,
    pub unit_id: String,
    pub kind: ResourceKind,
    pub name: String,
    pub mode: crate::facts::AccessMode,
    pub evidence: Vec<Evidence>,
}
