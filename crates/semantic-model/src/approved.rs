//! Verified semantic revision generated after provider output validation.

use crate::{
    AreaCategory, LabelSource, ProjectSemanticProposal, RegionId, SemanticAreaId,
    SemanticFallbackReason, SemanticRevisionId, TracePathId,
};
use codebase_fact_model::identity::{EvidenceId, FactNodeId, Sha256Digest, SnapshotId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApprovedSemanticRevision {
    pub schema_version: u16,
    pub revision_id: SemanticRevisionId,
    pub snapshot_id: SnapshotId,
    pub semantic_input_digest: Sha256Digest,
    pub provider: SemanticRevisionProvider,
    pub prompt_policy_version: String,
    pub project: ProjectSemanticProposal,
    pub areas: Vec<ApprovedSemanticArea>,
    pub assignments: Vec<ApprovedRegionAssignment>,
    pub unassigned_regions: Vec<crate::UnassignedRegion>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticRevisionProvider {
    pub kind: crate::AiProviderKind,
    pub model: String,
    pub effort: crate::ReasoningEffort,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApprovedSemanticArea {
    pub area_id: SemanticAreaId,
    pub parent_area_id: Option<SemanticAreaId>,
    pub level: u8,
    pub label: String,
    pub summary: String,
    pub category: AreaCategory,
    pub direct_member_region_ids: Vec<RegionId>,
    pub effective_member_region_ids: Vec<RegionId>,
    pub representative_fact_ids: Vec<FactNodeId>,
    pub representative_trace_path_ids: Vec<TracePathId>,
    pub evidence_ids: Vec<EvidenceId>,
    pub aliases: Vec<String>,
    pub label_source: LabelSource,
    pub fallback_reason: Option<SemanticFallbackReason>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApprovedRegionAssignment {
    pub region_id: RegionId,
    pub area_id: SemanticAreaId,
}
