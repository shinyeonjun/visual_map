//! Codex에 전달할 정적 분석 후보정 결과의 출력 계약.

use crate::facts::{AccessMode, EntrypointKind, ResolutionStatus, ResourceKind};
use crate::flow::{FlowEdgeKind, FlowNodeKind};
use crate::model::AnalysisStatus;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexSemanticContext {
    pub schema_version: &'static str,
    pub chunk_id: String,
    pub source_analysis_id: String,
    pub source_schema_version: String,
    pub project_id: String,
    pub analysis_status: AnalysisStatus,
    pub policy_version: &'static str,
    pub project_profile: ContextProjectProfile,
    pub global_summary: GlobalContextSummary,
    pub adjacent_domains: Vec<AdjacentDomain>,
    pub domains: Vec<ContextDomain>,
    pub domain_aliases: Vec<DomainAlias>,
    pub suppressed_domains: Vec<SuppressedDomain>,
    pub summary: ContextSummary,
    pub warnings: Vec<ContextWarning>,
}

#[derive(Debug, Clone)]
pub struct CodexContextBundle {
    pub manifest: CodexContextManifest,
    pub chunks: Vec<CodexSemanticContext>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ContextProjectProfile {
    pub visible_unit_count: usize,
    pub entrypoint_count: usize,
    pub resource_count: usize,
    pub reference_count: usize,
    pub confirmed_reference_count: usize,
    pub entrypoint_density: f64,
    pub resource_density: f64,
    pub reference_resolution: f64,
    pub max_domain_unit_ratio: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdjacentDomain {
    pub domain_id: String,
    pub label: String,
    pub relation_kinds: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextWarning {
    pub code: String,
    pub message: String,
    pub related_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexContextManifest {
    pub schema_version: &'static str,
    pub source_analysis_id: String,
    pub source_schema_version: String,
    pub project_id: String,
    pub project_profile: ContextProjectProfile,
    pub global_summary: GlobalContextSummary,
    pub chunks: Vec<ContextChunkDescriptor>,
    pub domain_coverage: Vec<DomainCoverage>,
    pub warnings: Vec<ContextWarning>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalContextSummary {
    pub domain_ids: Vec<String>,
    pub domain_labels: Vec<String>,
    pub represented_domain_count: usize,
    pub language_keys: Vec<String>,
    pub total_domains: usize,
    pub total_features: usize,
    pub total_flows: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextChunkDescriptor {
    pub chunk_id: String,
    pub file_name: String,
    pub domain_ids: Vec<String>,
    pub used_bytes: usize,
    pub budget_bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainCoverage {
    pub domain_id: String,
    pub representation: String,
    pub chunk_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextDomain {
    pub domain_id: String,
    pub source_domain_ids: Vec<String>,
    pub current_label: String,
    pub role: DomainRole,
    pub decision: DomainDecision,
    pub signal: DomainSignal,
    pub source_paths: Vec<String>,
    pub entrypoints: Vec<ContextEntrypoint>,
    pub resources: Vec<ContextResource>,
    pub features: Vec<ContextFeature>,
    pub flows: Vec<ContextFlow>,
    pub evidence_ids: Vec<String>,
    pub omission: DomainOmission,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainAlias {
    pub from_domain_id: String,
    pub to_domain_id: String,
    pub reason: String,
    pub source_domain_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuppressedDomain {
    pub domain_id: String,
    pub key: String,
    pub reason: String,
    pub unit_count: usize,
    pub signal: Option<DomainSignal>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DomainSignal {
    pub score: u32,
    pub anchor: f64,
    pub density: f64,
    pub specificity: f64,
    pub confidence: f64,
    pub has_business_anchor: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DomainRole {
    Business,
    CrossCutting,
    External,
    Noise,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DomainDecision {
    Original,
    AliasMerged,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextEntrypoint {
    pub id: String,
    pub unit_id: String,
    pub kind: EntrypointKind,
    pub name: String,
    pub method: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextResource {
    pub id: String,
    pub kind: ResourceKind,
    pub name: String,
    pub mode: AccessMode,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextFeature {
    pub id: String,
    pub current_label: String,
    pub visibility: FeatureVisibility,
    pub tags: Vec<FeatureTag>,
    pub symbols: Vec<String>,
    pub source_paths: Vec<String>,
    pub entrypoint_ids: Vec<String>,
    pub resource_ids: Vec<String>,
    pub flow_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FeatureVisibility {
    UserFacing,
    Internal,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FeatureTag {
    Endpoint,
    ResourceAccess,
    SideEffect,
    DynamicBoundary,
    Internal,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextFlow {
    pub id: String,
    pub feature_ids: Vec<String>,
    pub owner_unit_id: String,
    pub owner_name: String,
    pub steps: Vec<ContextFlowStep>,
    pub edges: Vec<ContextFlowEdge>,
    pub dynamic_boundary_ids: Vec<String>,
    pub selection_reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextFlowStep {
    pub id: String,
    pub kind: FlowNodeKind,
    pub label: String,
    pub target_unit_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextFlowEdge {
    pub source_node_id: String,
    pub target_node_id: String,
    pub kind: FlowEdgeKind,
    pub status: ResolutionStatus,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DomainOmission {
    pub total_features: usize,
    pub included_features: usize,
    pub total_flows: usize,
    pub included_flows: usize,
    pub reasons: BTreeMap<String, usize>,
    pub budget_bytes: usize,
    pub used_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ContextSummary {
    pub total_source_domains: usize,
    pub included_domains: usize,
    pub suppressed_domains: usize,
    pub total_features: usize,
    pub included_features: usize,
    pub total_flows: usize,
    pub included_flows: usize,
    pub budget_bytes: usize,
    pub used_bytes: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct DomainCluster {
    pub representative_id: String,
    pub source_domain_ids: Vec<String>,
    pub decision: DomainDecision,
    pub reason: Option<String>,
    pub signal: DomainSignal,
}
