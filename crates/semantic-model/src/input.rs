//! Bounded input packet for base semantic compilation.

use crate::{
    RegionId, RelationBundleId, SemanticAreaId, SemanticIdError, SemanticRevisionId, TracePathId,
};
use codebase_fact_model::{
    analysis::ProgrammingLanguage,
    fact_graph::{DispatchKind, FactEdgeFamily, FactNodeKind, FactRole, FactTruth},
    identity::{EvidenceId, FactEdgeId, FactNodeId, Sha256Digest, SnapshotId, WorkspaceId},
    source::RepositoryPath,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiProviderKind {
    Codex,
    Claude,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
    Ultra,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiProviderDescriptor {
    pub kind: AiProviderKind,
    pub model: String,
    pub effort: ReasoningEffort,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputLanguage {
    Korean,
    English,
}

impl OutputLanguage {
    pub const fn prompt_name(self) -> &'static str {
        match self {
            Self::Korean => "Korean",
            Self::English => "English",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SemanticTask {
    Base,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScopeReceipt {
    pub included: u64,
    pub total: u64,
    pub truncated: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BaseSemanticPacket {
    pub schema_version: u16,
    pub task: SemanticTask,
    pub workspace_id: WorkspaceId,
    pub snapshot_id: SnapshotId,
    pub semantic_input_digest: Sha256Digest,
    pub provider: AiProviderDescriptor,
    pub output_language: OutputLanguage,
    pub packet_compiler_version: String,
    pub prompt_policy_version: String,
    pub scope_receipt: ScopeReceipt,
    pub input: BaseSemanticInput,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BaseSemanticInput {
    pub repository: ProjectSemanticContext,
    pub regions: Vec<StaticRegionSummary>,
    pub anchors: Vec<AnchorFactSummary>,
    pub boundary_relations: Vec<BoundaryRelationSummary>,
    pub representative_traces: Vec<TracePathSummary>,
    pub excerpts: Vec<EvidenceExcerpt>,
    pub previous_revision: Option<PreviousSemanticRevisionSummary>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectSemanticContext {
    pub fact_id: FactNodeId,
    pub name: String,
    pub languages: Vec<ProgrammingLanguage>,
    pub framework_fact_ids: Vec<FactNodeId>,
    pub root_region_ids: Vec<RegionId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StaticRegionKind {
    BuildTarget,
    Package,
    Module,
    FileRegion,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StaticRegionSummary {
    pub region_id: RegionId,
    pub parent_region_id: Option<RegionId>,
    pub structural_label: String,
    pub structural_kind: StaticRegionKind,
    pub path_roots: Vec<RepositoryPath>,
    pub languages: Vec<ProgrammingLanguage>,
    pub file_count: u64,
    pub effective_loc: u64,
    pub anchor_fact_ids: Vec<FactNodeId>,
    pub representative_trace_path_ids: Vec<TracePathId>,
    pub inbound_bundle_ids: Vec<RelationBundleId>,
    pub outbound_bundle_ids: Vec<RelationBundleId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnchorFactSummary {
    pub fact_id: FactNodeId,
    pub owner_region_id: RegionId,
    pub kind: FactNodeKind,
    pub name: String,
    pub qualified_name: Option<String>,
    pub signature: Option<String>,
    pub static_roles: Vec<FactRole>,
    pub evidence_ids: Vec<EvidenceId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoundaryRelationSummary {
    pub bundle_id: RelationBundleId,
    pub source_region_id: RegionId,
    pub target_region_id: RegionId,
    pub families: Vec<BoundaryRelationCount>,
    pub representative_edge_ids: Vec<FactEdgeId>,
    pub evidence_ids: Vec<EvidenceId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoundaryRelationCount {
    pub family: FactEdgeFamily,
    pub truth: FactTruth,
    /// Dispatch is retained separately from truth. A resolved dynamic or
    /// virtual target is still a real relation, but it is not an exact static
    /// execution hop. Historical semantic packets deserialize as `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatch: Option<DispatchKind>,
    pub relation_count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TracePathState {
    Complete,
    Partial,
    Gap,
    Cycle,
    DepthLimited,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TracePathSummary {
    pub trace_path_id: TracePathId,
    pub entry_fact_id: FactNodeId,
    pub ordered_fact_ids: Vec<FactNodeId>,
    pub ordered_edge_ids: Vec<FactEdgeId>,
    pub state: TracePathState,
    pub evidence_ids: Vec<EvidenceId>,
}

impl TracePathSummary {
    /// Stable identity of one ordered static path. State and evidence are
    /// provenance that may improve without changing the path itself.
    pub fn stable_id(
        entry_fact_id: &FactNodeId,
        ordered_edge_ids: &[FactEdgeId],
    ) -> Result<TracePathId, SemanticIdError> {
        let mut components = vec![
            "trace-path-v1".to_string(),
            format!("entry:{entry_fact_id}"),
        ];
        if ordered_edge_ids.is_empty() {
            components.push("edges:empty".to_string());
        } else {
            components.extend(
                ordered_edge_ids
                    .iter()
                    .enumerate()
                    .map(|(index, edge_id)| format!("edge[{index}]:{edge_id}")),
            );
        }
        let refs = components.iter().map(String::as_str).collect::<Vec<_>>();
        TracePathId::from_components(&refs)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvidenceExcerpt {
    pub evidence_id: EvidenceId,
    pub owner_region_id: RegionId,
    pub file_fact_id: FactNodeId,
    pub relative_path: RepositoryPath,
    pub start_line: u32,
    pub end_line: u32,
    pub content_hash: Sha256Digest,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviousSemanticRevisionSummary {
    pub revision_id: SemanticRevisionId,
    pub areas: Vec<PreviousAreaSummary>,
    pub assignments: Vec<PreviousRegionAssignment>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviousAreaSummary {
    pub area_id: SemanticAreaId,
    pub parent_area_id: Option<SemanticAreaId>,
    pub level: u8,
    pub label: String,
    pub summary: String,
    pub member_region_ids: Vec<RegionId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviousRegionAssignment {
    pub region_id: RegionId,
    pub area_id: SemanticAreaId,
}
