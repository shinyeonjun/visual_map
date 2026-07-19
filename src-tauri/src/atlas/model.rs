use serde::{Deserialize, Serialize};

pub(super) const SNAPSHOT_SCHEMA_VERSION: u32 = 2;

fn legacy_snapshot_schema_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InventorySnapshot {
    #[serde(default = "legacy_snapshot_schema_version")]
    pub schema_version: u32,
    pub workspace_id: String,
    pub saved_at: String,
    #[serde(default)]
    pub metadata: SnapshotMetadata,
    #[serde(default)]
    pub stale_reasons: Vec<String>,
    #[serde(default)]
    pub links: Vec<SnapshotLink>,
    pub items: Vec<InventoryItem>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SnapshotMetadata {
    #[serde(default)]
    pub code: Option<SnapshotSourceMetadata>,
    #[serde(default)]
    pub db: Option<SnapshotSourceMetadata>,
    #[serde(default)]
    pub architecture: Option<serde_json::Value>,
    #[serde(default)]
    pub migration: SnapshotMigration,
    #[serde(default)]
    pub gaps: Vec<SnapshotGap>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SnapshotSourceMetadata {
    pub saved_at: String,
    #[serde(default)]
    pub engine_id: Option<String>,
    pub engine_version: Option<String>,
    #[serde(default)]
    pub engine_checksum: Option<String>,
    #[serde(default)]
    pub contract_version: Option<String>,
    #[serde(default)]
    pub snapshot_key: Option<String>,
    #[serde(default)]
    pub limit_requested: Option<usize>,
    #[serde(default)]
    pub limit_applied: Option<usize>,
    #[serde(default)]
    pub limit_clamped: Option<bool>,
    #[serde(default)]
    pub result_count: Option<usize>,
    #[serde(default)]
    pub total_tables: Option<usize>,
    #[serde(default)]
    pub truncated: Option<bool>,
    #[serde(default)]
    pub source_revision: Option<String>,
    #[serde(default)]
    pub source_revision_label: Option<String>,
    pub source_path: Option<String>,
    pub source_type: String,
    pub profile_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SnapshotMigration {
    #[serde(default)]
    pub source_schema_version: Option<u32>,
    #[serde(default)]
    pub reindex_required: bool,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SnapshotGap {
    pub id: String,
    pub kind: String,
    pub message: String,
    #[serde(default)]
    pub related_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InventoryItem {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub layer: String,
    pub source: String,
    pub parent_id: Option<String>,
    pub path: Option<String>,
    #[serde(default)]
    pub qualified_name: Option<String>,
    #[serde(default)]
    pub engine_label: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    pub location: Option<SourceLocation>,
    #[serde(default)]
    pub is_primary_key: bool,
    #[serde(default)]
    pub is_foreign_key: bool,
    #[serde(default)]
    pub nullable: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SourceLocation {
    pub path: String,
    #[serde(default)]
    pub line: Option<u64>,
    #[serde(default)]
    pub column: Option<u64>,
    #[serde(default)]
    pub end_line: Option<u64>,
    #[serde(default)]
    pub end_column: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SnapshotLink {
    pub id: String,
    pub from: String,
    pub to: String,
    pub kind: String,
    pub label: Option<String>,
    #[serde(default)]
    pub truth_class: String,
    #[serde(default)]
    pub direction: String,
    #[serde(default)]
    pub engine_edge_type: Option<String>,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Evidence {
    pub kind: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct CandidateLink {
    pub id: String,
    pub from: String,
    pub to: String,
    pub confidence: String,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VisualMap {
    pub id: String,
    pub workspace_id: String,
    pub mode: String,
    pub focus: String,
    pub nodes: Vec<VisualNode>,
    pub edges: Vec<VisualEdge>,
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_board: Option<ImpactReviewBoard>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_reading: Option<ApiReadingAnswer>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ApiReadingAnswer {
    pub subject: String,
    pub steps: Vec<ApiReadingStep>,
    #[serde(default)]
    pub db_candidates: Vec<ImpactReviewItem>,
    #[serde(default)]
    pub unknowns: Vec<ImpactReviewItem>,
    #[serde(default)]
    pub recommended_checks: Vec<ImpactReviewItem>,
    pub hidden_branches: usize,
    #[serde(default)]
    pub hidden_branches_is_lower_bound: bool,
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncation_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ApiReadingStep {
    #[serde(flatten)]
    pub item: ImpactReviewItem,
    pub depth: usize,
    pub lane: String,
    pub lane_basis: String,
    #[serde(default)]
    pub incoming_evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImpactReviewBoard {
    pub subject: String,
    pub scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_intent: Option<ChangeIntent>,
    pub lanes: Vec<ImpactReviewLane>,
    pub markdown_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChangeIntent {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImpactReviewLane {
    pub id: String,
    pub order: u8,
    pub title: String,
    pub description: String,
    pub tone: String,
    pub total: usize,
    pub hidden: usize,
    pub empty_message: String,
    pub items: Vec<ImpactReviewItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImpactReviewItem {
    pub id: String,
    pub node_id: Option<String>,
    pub kind: String,
    pub title: String,
    pub detail: String,
    pub truth_class: String,
    pub confidence: Option<String>,
    pub rank: usize,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    pub location: Option<SourceLocation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VisualNode {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub layer: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VisualEdge {
    pub id: String,
    pub from: String,
    pub to: String,
    pub kind: String,
    pub confidence: Option<String>,
    pub evidence: Vec<Evidence>,
}
