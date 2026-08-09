//! Stable Language IR publication and diagnostic receipt contracts.
//!
//! These are data-only wire shapes. Runtime audit accumulators and extraction
//! logic stay outside this module so changing diagnostics cannot silently
//! change analysis behavior.

use codebase_fact_model::analysis::ProgrammingLanguage;
use codebase_fact_model::coverage::{AnalysisCapability, GapCode};
use codebase_fact_model::fact_graph::{FactNodeKind, ResolutionMethod, Visibility};
use codebase_fact_model::identity::{Sha256Digest, SnapshotId};
use codebase_fact_model::language_ir::LanguageRelationKind;
use codebase_fact_model::source::RepositoryPath;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LanguageIrMigrationReceipt {
    pub(crate) schema: &'static str,
    pub(crate) snapshot_id: SnapshotId,
    pub(crate) source_manifest_digest: Sha256Digest,
    pub(crate) analysis_plan_digest: Sha256Digest,
    pub(crate) provider_set_digest: Sha256Digest,
    pub(crate) execution_context_set_digest: Sha256Digest,
    pub(crate) stream_set_digest: Sha256Digest,
    /// Digest of evidence/definition/relation records only. This remains a
    /// stable semantic comparison key while coverage records evolve.
    pub(crate) semantic_payload_set_digest: Sha256Digest,
    pub(crate) emitted_unit_count: u64,
    pub(crate) unavailable_unit_count: u64,
    pub(crate) record_count: u64,
    pub(crate) file_record_count: u64,
    pub(crate) definition_count: u64,
    pub(crate) relation_count: u64,
    pub(crate) evidence_count: u64,
    pub(crate) capability_receipt_count: u64,
    pub(crate) gap_count: u64,
    pub(crate) issue_count: u64,
    pub(crate) omitted_definition_count: u64,
    pub(crate) omitted_relation_count: u64,
    pub(crate) syntax_definition_count: u64,
    pub(crate) matched_definition_count: u64,
    pub(crate) missing_syntax_definition_count: u64,
    pub(crate) extra_provider_definition_count: u64,
    pub(crate) provider_definition_alias_count: u64,
    pub(crate) kind_refinement_count: u64,
    pub(crate) owner_repair_count: u64,
    pub(crate) unresolved_owner_count: u64,
    pub(crate) definition_inventory_failed_file_count: u64,
}

/// Potentially large language-level audit detail. It is produced alongside
/// the bounded migration receipt for tests and opt-in diagnostics, but it is
/// never written to the normal product progress stream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LanguageIrDiagnosticReceipt {
    pub(crate) schema: &'static str,
    pub(crate) snapshot_id: SnapshotId,
    pub(crate) definition_language_summaries: Vec<DefinitionLanguageSummary>,
    pub(crate) definition_audit_sample: Vec<DefinitionAuditFailure>,
    pub(crate) definition_metadata_audit_sample: Vec<DefinitionMetadataAuditEntry>,
    pub(crate) import_language_summaries: Vec<ImportLanguageSummary>,
    pub(crate) import_audit_sample: Vec<ImportAuditEntry>,
    pub(crate) type_relation_language_summaries: Vec<TypeRelationLanguageSummary>,
    pub(crate) type_relation_audit_sample: Vec<TypeRelationAuditEntry>,
    pub(crate) unavailable_unit_sample: Vec<UnavailableUnitReceipt>,
    pub(crate) details_truncated: bool,
}

/// A complete, validated, deterministic Language IR stream set. The path is
/// process-local staging state and is deliberately omitted from the receipt;
/// the canonical linker consumes it before the provider-work guard removes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LanguageIrStreamArtifact {
    pub(crate) schema: &'static str,
    pub(crate) snapshot_id: SnapshotId,
    pub(crate) stream_set_digest: Sha256Digest,
    pub(crate) content_digest: Sha256Digest,
    pub(crate) record_count: u64,
    pub(crate) byte_count: u64,
    pub(crate) complete: bool,
    #[serde(skip)]
    pub(crate) path: PathBuf,
}

pub(crate) struct LanguageIrEmission {
    pub(crate) receipt: LanguageIrMigrationReceipt,
    pub(crate) diagnostics: LanguageIrDiagnosticReceipt,
    pub(crate) artifact: LanguageIrStreamArtifact,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DefinitionAuditFailure {
    pub(crate) unit_id: String,
    pub(crate) language: ProgrammingLanguage,
    pub(crate) path: RepositoryPath,
    pub(crate) name: String,
    pub(crate) reason: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) expected_kind: Option<FactNodeKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) provider_kind: Option<FactNodeKind>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DefinitionLanguageSummary {
    pub(crate) language: ProgrammingLanguage,
    pub(crate) definition_set_digest: Sha256Digest,
    pub(crate) syntax_definition_count: u64,
    pub(crate) owned_syntax_definition_count: u64,
    pub(crate) matched_definition_count: u64,
    pub(crate) missing_syntax_definition_count: u64,
    pub(crate) extra_provider_definition_count: u64,
    pub(crate) provider_definition_alias_count: u64,
    pub(crate) kind_refinement_count: u64,
    pub(crate) owner_repair_count: u64,
    pub(crate) resolved_owner_count: u64,
    pub(crate) unresolved_owner_count: u64,
    pub(crate) inventory_failed_file_count: u64,
    pub(crate) metadata_set_digest: Sha256Digest,
    pub(crate) metadata_definition_count: u64,
    pub(crate) callable_definition_count: u64,
    pub(crate) callable_signature_count: u64,
    pub(crate) known_visibility_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DefinitionMetadataAuditEntry {
    pub(crate) unit_id: String,
    pub(crate) language: ProgrammingLanguage,
    pub(crate) path: RepositoryPath,
    pub(crate) kind: FactNodeKind,
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) signature: Option<String>,
    pub(crate) visibility: Visibility,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ImportAuditOutcome {
    Internal,
    KnownExternal,
    Unresolved,
    Ambiguous,
    InvalidEvidence,
    InventoryFailed,
    MetadataUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportAuditEntry {
    pub(crate) unit_id: String,
    pub(crate) language: ProgrammingLanguage,
    pub(crate) path: RepositoryPath,
    pub(crate) capability: AnalysisCapability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) specifier: Option<String>,
    pub(crate) utf8_range: Vec<i32>,
    pub(crate) utf16_range: Vec<i32>,
    pub(crate) outcome: ImportAuditOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) resolution_method: Option<ResolutionMethod>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) gap_code: Option<GapCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) candidate_count: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportLanguageSummary {
    pub(crate) language: ProgrammingLanguage,
    pub(crate) site_set_digest: Sha256Digest,
    pub(crate) eligible_site_count: u64,
    pub(crate) import_site_count: u64,
    pub(crate) export_site_count: u64,
    pub(crate) internal_relation_count: u64,
    pub(crate) known_external_count: u64,
    pub(crate) unresolved_count: u64,
    pub(crate) ambiguous_count: u64,
    pub(crate) invalid_evidence_count: u64,
    pub(crate) inventory_failed_file_count: u64,
    pub(crate) metadata_unavailable_file_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TypeRelationAuditEntry {
    pub(crate) unit_id: String,
    pub(crate) language: ProgrammingLanguage,
    pub(crate) path: RepositoryPath,
    pub(crate) native_relation: String,
    pub(crate) kind: LanguageRelationKind,
    pub(crate) source_symbol: String,
    pub(crate) source_name: String,
    pub(crate) target_symbol: String,
    pub(crate) target_name: String,
    pub(crate) start_line: u32,
    pub(crate) start_utf8_column: u32,
    pub(crate) end_line: u32,
    pub(crate) end_utf8_column: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TypeRelationLanguageSummary {
    pub(crate) language: ProgrammingLanguage,
    pub(crate) relation_set_digest: Sha256Digest,
    pub(crate) relation_count: u64,
    pub(crate) extends_count: u64,
    pub(crate) implements_count: u64,
    pub(crate) mixes_in_count: u64,
    pub(crate) overrides_count: u64,
    pub(crate) uses_type_count: u64,
    pub(crate) explicit_hierarchy_site_count: u64,
    pub(crate) matched_explicit_hierarchy_site_count: u64,
    pub(crate) unmatched_explicit_hierarchy_site_count: u64,
    pub(crate) inventory_failed_file_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UnavailableUnitReceipt {
    pub(crate) unit_id: String,
    pub(crate) language: ProgrammingLanguage,
    pub(crate) root: RepositoryPath,
    pub(crate) gap_code: GapCode,
}
