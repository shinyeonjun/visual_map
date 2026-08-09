//! Deterministic Language IR -> canonical Fact Graph normalization.
//!
//! The implementation is deliberately split from language providers. It
//! performs exact identity registration first, relation resolution second,
//! and never uses display-name or path similarity to create a target.

mod framework;
mod linker;
mod store;
mod verification;

#[cfg(test)]
mod tests;

pub(crate) use linker::normalize_language_ir;

use crate::static_pipeline::framework_ir::FrameworkIr;
use crate::static_pipeline::test_ir::TestIr;
use codebase_fact_model::analysis_plan::AnalysisPlan;
use codebase_fact_model::fact_graph::FactBundleManifest;
use codebase_fact_model::identity::{Sha256Digest, SnapshotId};
use codebase_fact_model::source_manifest::SourceManifest;
use serde::Serialize;
use std::path::{Path, PathBuf};

pub(crate) const LINKER_RECEIPT_SCHEMA: &str = "codebase-workspace.canonical-linker-receipt.v2";
pub(crate) const BUNDLE_ARTIFACT_SCHEMA: &str =
    "codebase-workspace.canonical-fact-bundle-artifact.v1";

/// Exact, already validated Language IR artifact inputs consumed by the
/// canonical linker. The artifact itself remains process-local staging.
pub(crate) struct CanonicalLanguageInput<'a> {
    pub(crate) project_root: &'a Path,
    pub(crate) repository_display_name: &'a str,
    pub(crate) manifest: &'a SourceManifest,
    pub(crate) plan: &'a AnalysisPlan,
    pub(crate) ir_path: &'a Path,
    pub(crate) ir_snapshot_id: &'a SnapshotId,
    pub(crate) ir_content_digest: Sha256Digest,
    pub(crate) ir_record_count: u64,
    pub(crate) provider_set_digest: Sha256Digest,
    pub(crate) execution_context_set_digest: Sha256Digest,
    /// Production always supplies this. `None` is reserved for focused
    /// language-linker tests and becomes an explicit empty execution.
    pub(crate) framework_ir: Option<&'a FrameworkIr>,
    /// Production always supplies this. `None` is reserved for focused
    /// language-linker tests and becomes an explicit empty execution.
    pub(crate) test_ir: Option<&'a TestIr>,
    pub(crate) output_root: &'a Path,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CanonicalLinkerReceipt {
    pub(crate) schema: &'static str,
    pub(crate) snapshot_id: SnapshotId,
    pub(crate) language_ir_content_digest: Sha256Digest,
    pub(crate) language_ir_record_count: u64,
    /// Provider-native identities registered during pass 1. Several provider
    /// identities may intentionally converge on one canonical definition.
    pub(crate) provider_definition_identity_count: u64,
    /// Distinct canonical definition nodes before the visualization relevance
    /// gate is applied.
    pub(crate) canonical_definition_node_count: u64,
    pub(crate) retained_definition_node_count: u64,
    pub(crate) pruned_definition_node_count: u64,
    pub(crate) resolved_relation_count: u64,
    pub(crate) unresolved_relation_count: u64,
    pub(crate) framework_route_node_count: u64,
    pub(crate) framework_exposes_edge_count: u64,
    pub(crate) framework_handles_edge_count: u64,
    pub(crate) framework_unresolved_handler_count: u64,
    pub(crate) framework_ir_content_digest: Sha256Digest,
    pub(crate) test_case_node_count: u64,
    pub(crate) tests_edge_count: u64,
    pub(crate) unlinked_test_case_count: u64,
    pub(crate) test_ir_content_digest: Sha256Digest,
    pub(crate) merged_node_count: u64,
    pub(crate) merged_edge_count: u64,
    pub(crate) dangling_endpoint_count: u64,
    pub(crate) confirmed_without_evidence_count: u64,
    pub(crate) duplicate_logical_edge_count: u64,
    pub(crate) semantic_digest: Sha256Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CanonicalFactBundleArtifact {
    pub(crate) schema: &'static str,
    pub(crate) snapshot_id: SnapshotId,
    pub(crate) semantic_digest: Sha256Digest,
    pub(crate) bundle_digest: Sha256Digest,
    pub(crate) bundle_path: PathBuf,
    pub(crate) manifest_path: PathBuf,
}

pub(crate) struct CanonicalLanguageEmission {
    pub(crate) receipt: CanonicalLinkerReceipt,
    pub(crate) manifest: FactBundleManifest,
    pub(crate) artifact: CanonicalFactBundleArtifact,
}
