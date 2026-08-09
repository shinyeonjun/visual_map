//! Strict provider response for base semantic compilation.

use crate::{ProposalKey, RegionId, TracePathId};
use codebase_fact_model::identity::{EvidenceId, FactNodeId, Sha256Digest, SnapshotId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AreaCategory {
    Domain,
    Shared,
    Infrastructure,
    Integration,
    Structural,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabelSource {
    Semantic,
    Structural,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticFallbackReason {
    InsufficientSemanticSignal,
    MixedResponsibility,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnassignedReason {
    InsufficientInput,
    MixedResponsibility,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticRevisionProposal {
    pub schema_version: u16,
    pub snapshot_id: SnapshotId,
    pub semantic_input_digest: Sha256Digest,
    pub project: ProjectSemanticProposal,
    pub areas: Vec<AreaProposal>,
    pub assignments: Vec<RegionAssignment>,
    pub unassigned_regions: Vec<UnassignedRegion>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectSemanticProposal {
    pub summary: String,
    pub aliases: Vec<String>,
    pub representative_fact_ids: Vec<FactNodeId>,
    pub evidence_ids: Vec<EvidenceId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AreaProposal {
    pub proposal_key: ProposalKey,
    pub parent_proposal_key: Option<ProposalKey>,
    pub level: u8,
    pub label: String,
    pub summary: String,
    pub category: AreaCategory,
    pub representative_fact_ids: Vec<FactNodeId>,
    pub representative_trace_path_ids: Vec<TracePathId>,
    pub evidence_ids: Vec<EvidenceId>,
    pub aliases: Vec<String>,
    pub label_source: LabelSource,
    pub fallback_reason: Option<SemanticFallbackReason>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegionAssignment {
    pub region_id: RegionId,
    pub area_proposal_key: ProposalKey,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnassignedRegion {
    pub region_id: RegionId,
    pub reason: UnassignedReason,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_and_confidence_fields_fail_closed() {
        let value = serde_json::json!({
            "schemaVersion": 1,
            "snapshotId": format!("snapshot-{}", "0".repeat(64)),
            "semanticInputDigest": "0".repeat(64),
            "project": {
                "summary": "요약",
                "aliases": [],
                "representativeFactIds": [],
                "evidenceIds": []
            },
            "areas": [],
            "assignments": [],
            "unassignedRegions": [],
            "warnings": [],
            "confidence": 0.99
        });

        assert!(serde_json::from_value::<SemanticRevisionProposal>(value).is_err());
    }
}
