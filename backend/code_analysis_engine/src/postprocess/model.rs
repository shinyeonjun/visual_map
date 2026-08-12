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
    pub source_analysis_id: String,
    pub source_schema_version: String,
    pub project_id: String,
    pub analysis_status: AnalysisStatus,
    pub policy_version: &'static str,
    pub domains: Vec<ContextDomain>,
    pub domain_aliases: Vec<DomainAlias>,
    pub suppressed_domains: Vec<SuppressedDomain>,
    pub summary: ContextSummary,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextDomain {
    pub domain_id: String,
    pub source_domain_ids: Vec<String>,
    pub current_label: String,
    pub role: DomainRole,
    pub decision: DomainDecision,
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
    pub feature_id: Option<String>,
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
}

#[derive(Debug, Clone)]
pub(crate) struct SelectedFlow {
    pub flow_id: String,
    pub selection_reason: String,
}

#[derive(Debug, Clone)]
pub(crate) struct DomainCluster {
    pub representative_id: String,
    pub source_domain_ids: Vec<String>,
    pub decision: DomainDecision,
    pub reason: Option<String>,
}
