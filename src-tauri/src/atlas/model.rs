use serde::{Deserialize, Serialize, Serializer};

pub(crate) const ITEM_SOURCE_CODE: &str = "code";
pub(crate) const ITEM_SOURCE_DB: &str = "db";
pub(crate) const LINK_TRUTH_CONFIRMED: &str = "confirmed";

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
    pub evidence: Option<serde_json::Value>,
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
    pub adapter_version: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_basis: Option<String>,
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

impl InventoryItem {
    pub(crate) fn is_code(&self) -> bool {
        self.source == ITEM_SOURCE_CODE
    }

    pub(crate) fn is_db(&self) -> bool {
        self.source == ITEM_SOURCE_DB
    }

    pub(crate) fn is_project_code_item(&self) -> bool {
        self.is_code()
            && !self
                .path
                .as_deref()
                .is_some_and(|path| path.trim().starts_with('<'))
    }
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

impl SnapshotLink {
    pub(crate) fn is_confirmed(&self) -> bool {
        self.truth_class == LINK_TRUTH_CONFIRMED
    }
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
    /// The evidence-backed organising axis used by the overview projection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overview_axis: Option<OverviewAxis>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_board: Option<ImpactReviewBoard>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_reading: Option<ApiReadingAnswer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub representative_paths: Option<Vec<RepresentativePath>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RepresentativePath {
    pub entry_id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    pub step_count: usize,
    pub new_coverage: usize,
    pub cumulative_share: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OverviewAxis {
    /// "role" or "depth". A depth axis may use the legacy lanes when roots are unsafe.
    pub kind: String,
    pub lanes: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ApiReadingAnswer {
    pub subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    pub steps: Vec<ApiReadingStep>,
    #[serde(default)]
    pub client_requests: Vec<ImpactReviewItem>,
    #[serde(default)]
    pub db_relations: Vec<ImpactReviewItem>,
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
    #[serde(serialize_with = "serialize_diagnostic_kind")]
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

fn serialize_diagnostic_kind<S>(kind: &str, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let code = match kind {
        "handler-gap" => "missing-handler",
        "call-gap" => "unresolved-call",
        "stale" => "stale-index",
        "reindex" | "snapshot-migration" => "snapshot-incompatible",
        "db-inventory-truncated"
        | "db-limit-clamped"
        | "candidate-cap"
        | "candidate-linker-cap"
        | "db-truncated"
        | "truncated" => "display-limit",
        "db-capability" => "unsupported-framework",
        "gap" | "db-inventory-gap" | "excluded-engine-edge" => "unknown",
        other => other,
    };
    serializer.serialize_str(code)
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
    /// Canonical ownership parent for projected group nodes. Ordinary code,
    /// API, and data nodes leave this empty; legacy snapshots remain valid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Structural ownership depth: package = 0, module = 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<usize>,
    /// Evidence basis for the projected group assignment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assigned_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<SourceLocation>,
    /// Structured facts the map can render directly. Legacy snapshots keep using subtitle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics: Option<VisualNodeMetrics>,
    /// Language coverage contributed by this node, when the projection has it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<VisualNodeCoverage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VisualNodeMetrics {
    #[serde(default)]
    pub member_count: usize,
    pub api_count: usize,
    pub code_count: usize,
    pub db_count: usize,
    pub top_api: Vec<String>,
    pub top_code: Vec<String>,
    pub top_db: Vec<String>,
    #[serde(default)]
    pub handler_count: usize,
    #[serde(default)]
    pub service_count: usize,
    #[serde(default)]
    pub repository_count: usize,
    /// Hops from the nearest entry point. None until the overview computes depth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<usize>,
    #[serde(default)]
    pub in_degree: usize,
    #[serde(default)]
    pub out_degree: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VisualNodeCoverage {
    pub languages: Vec<String>,
    pub has_blind_spot: bool,
    #[serde(default)]
    pub has_partial: bool,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::{RepresentativePath, VisualEdge, VisualMap, VisualNode};

    #[test]
    fn legacy_visual_map_deserializes_without_structured_node_fields() {
        let map: VisualMap = serde_json::from_value(serde_json::json!({
            "id": "legacy-map",
            "workspaceId": "workspace-1",
            "mode": "atlas",
            "focus": "all",
            "nodes": [{
                "id": "group:legacy",
                "kind": "group-domain",
                "title": "레거시 영역",
                "subtitle": "API 1 · 코드 2 · DB 0|route|handler|",
                "layer": "mixed",
                "source": "projection",
                "location": null
            }],
            "edges": [],
            "warnings": []
        }))
        .expect("legacy visual map should remain readable");

        assert_eq!(map.nodes[0].metrics, None);
        assert_eq!(map.nodes[0].coverage, None);
        assert_eq!(map.representative_paths, None);
    }

    #[test]
    fn visual_map_contract_serializes_structured_fields() {
        let map = VisualMap {
            id: "map".to_string(),
            workspace_id: "workspace".to_string(),
            mode: "atlas".to_string(),
            focus: "all".to_string(),
            nodes: vec![VisualNode {
                id: "group:api".to_string(),
                kind: "group-domain".to_string(),
                title: "API".to_string(),
                subtitle: None,
                layer: "api".to_string(),
                source: "projection".to_string(),
                parent_id: None,
                depth: None,
                assigned_by: None,
                location: None,
                metrics: None,
                coverage: None,
            }],
            edges: vec![VisualEdge {
                id: "edge".to_string(),
                from: "group:api".to_string(),
                to: "group:code".to_string(),
                kind: "code_call".to_string(),
                confidence: Some("high".to_string()),
                evidence: Vec::new(),
                weight: Some(3),
            }],
            warnings: Vec::new(),
            overview_axis: None,
            review_board: None,
            api_reading: None,
            representative_paths: Some(vec![RepresentativePath {
                entry_id: "route".to_string(),
                title: "GET /orders".to_string(),
                method: Some("GET".to_string()),
                step_count: 3,
                new_coverage: 4,
                cumulative_share: 0.5,
            }]),
        };
        let value = serde_json::to_value(map).expect("visual map contract should serialize");
        assert_eq!(value["edges"][0]["weight"], 3);
        assert_eq!(value["representativePaths"][0]["stepCount"], 3);
    }
}
