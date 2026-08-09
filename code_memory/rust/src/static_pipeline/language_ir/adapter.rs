use super::capabilities::{capability_policies, AdapterMeasurement};
use super::definition_inventory::SyntaxDefinition;
use super::imports::{ImportRelation, ImportResolution, ImportSite, ProjectImportIndex};
use super::provider::resolve_provider_descriptor;
use super::source_coordinates::SourceCoordinates;
use super::type_relations::{SyntaxTypeRelationSite, SyntaxTypeUseSite, TypeRelationIntent};
use artifact_writer::{AtomicLanguageIrArtifactWriter, LanguageIrSink, ValidatingDigestSink};
use codebase_fact_model::analysis::{
    AnalysisUnit, ProgrammingLanguage, ProviderDescriptor, ProviderExecutionContext,
    ProviderProtocol,
};
use codebase_fact_model::analysis_plan::AnalysisPlan;
use codebase_fact_model::coverage::{
    AnalysisCapability, AnalysisErrorCode, AnalysisGap, AnalysisIssue, AnalysisScope,
    AnalysisStage, AnalysisUnitCompletion, AnalysisUnitState, CapabilityExecutionState,
    CapabilityReceipt, CoverageDenominator, DeclaredSupport, EvidencePrecision, FileCoverageRecord,
    FileCoverageState, GapCode,
};
use codebase_fact_model::evidence::{
    EvidenceKind, EvidenceLocation, EvidenceProducer, EvidenceProducerKind, FactEvidence,
};
use codebase_fact_model::fact_graph::{
    DispatchKind, FactNodeKind, FactTruth, ResolutionMethod, Visibility,
};
use codebase_fact_model::identity::{EvidenceId, ProviderSymbolId, Sha256Digest, SnapshotId};
use codebase_fact_model::language_ir::{
    IrDefinition, IrEndpoint, IrRelation, LanguageIrHeader, LanguageIrRecord, LanguageRelationKind,
};
use codebase_fact_model::source::{RepositoryPath, SourceFileKind, SourceFlags, SourceSpan};
use codebase_fact_model::source_manifest::{SourceEntryState, SourceManifest, SourceManifestFile};
use codebase_fact_model::validation::Validate;
use codebase_fact_model::ContractSchema;
use serde::Serialize;
use source_inventory::inventory_unit_sources;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::{
    normalize_scip_language, Diagnostic, DiagnosticCode, DocumentOutput, FileCoverageOutput,
    FileRelationOutput, LanguageOutput, OccurrenceOutput, RelationOutput, SymbolOutput, LANGUAGES,
};

mod artifact_writer;
mod source_inventory;

const MIGRATION_RECEIPT_SCHEMA: &str = "codebase-workspace.language-ir-migration-receipt.v7";
const DIAGNOSTIC_RECEIPT_SCHEMA: &str = "codebase-workspace.language-ir-diagnostic-receipt.v1";
const UNAVAILABLE_SAMPLE_LIMIT: usize = 100;
const DEFINITION_AUDIT_SAMPLE_LIMIT: usize = 100;
const DEFINITION_METADATA_AUDIT_SAMPLE_LIMIT: usize = 200;
const IMPORT_AUDIT_SAMPLE_LIMIT: usize = 200;
const TYPE_RELATION_AUDIT_SAMPLE_LIMIT: usize = 200;
const STREAM_ARTIFACT_SCHEMA: &str = "codebase-workspace.language-ir-stream-authority.v2";

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

#[derive(Clone, Debug)]
struct UnitEmissionSummary {
    unit_id: String,
    language: ProgrammingLanguage,
    stream_digest: Sha256Digest,
    semantic_payload_digest: Sha256Digest,
    record_count: u64,
    completion: AnalysisUnitCompletion,
    omitted_definition_count: u64,
    omitted_relation_count: u64,
    definition_audit: DefinitionAudit,
    import_audit: ImportAudit,
    type_relation_audit: TypeRelationAudit,
}

#[derive(Clone, Debug, Default)]
struct DefinitionAudit {
    definition_keys: Vec<String>,
    syntax_definition_count: u64,
    owned_syntax_definition_count: u64,
    matched_definition_count: u64,
    missing_syntax_definition_count: u64,
    extra_provider_definition_count: u64,
    provider_definition_alias_count: u64,
    kind_refinement_count: u64,
    owner_repair_count: u64,
    resolved_owner_count: u64,
    unresolved_owner_count: u64,
    inventory_failed_file_count: u64,
    metadata_keys: Vec<String>,
    metadata_definition_count: u64,
    callable_definition_count: u64,
    callable_signature_count: u64,
    known_visibility_count: u64,
    metadata_entries: Vec<DefinitionMetadataAuditEntry>,
    failures: Vec<DefinitionAuditFailure>,
}

impl DefinitionAudit {
    fn blocking_count(&self) -> u64 {
        self.missing_syntax_definition_count
            + self.extra_provider_definition_count
            + self.unresolved_owner_count
            + self.inventory_failed_file_count
    }

    fn merge(&mut self, other: &Self) {
        self.absorb(other.clone());
        self.canonicalize();
    }

    fn absorb(&mut self, mut other: Self) {
        self.definition_keys.append(&mut other.definition_keys);
        self.syntax_definition_count += other.syntax_definition_count;
        self.owned_syntax_definition_count += other.owned_syntax_definition_count;
        self.matched_definition_count += other.matched_definition_count;
        self.missing_syntax_definition_count += other.missing_syntax_definition_count;
        self.extra_provider_definition_count += other.extra_provider_definition_count;
        self.provider_definition_alias_count += other.provider_definition_alias_count;
        self.kind_refinement_count += other.kind_refinement_count;
        self.owner_repair_count += other.owner_repair_count;
        self.resolved_owner_count += other.resolved_owner_count;
        self.unresolved_owner_count += other.unresolved_owner_count;
        self.inventory_failed_file_count += other.inventory_failed_file_count;
        self.metadata_keys.append(&mut other.metadata_keys);
        self.metadata_definition_count += other.metadata_definition_count;
        self.callable_definition_count += other.callable_definition_count;
        self.callable_signature_count += other.callable_signature_count;
        self.known_visibility_count += other.known_visibility_count;
        self.metadata_entries.append(&mut other.metadata_entries);
        self.failures.append(&mut other.failures);
    }

    fn canonicalize(&mut self) {
        self.definition_keys.sort();
        self.metadata_keys.sort();
        self.metadata_keys.dedup();
        self.metadata_entries.sort();
        self.metadata_entries.dedup();
        self.failures.sort();
        self.failures.dedup();
    }
}

#[derive(Clone, Debug, Default)]
struct ImportCapabilityAudit {
    eligible_count: u64,
    covered_count: u64,
    internal_relation_count: u64,
    known_external_count: u64,
    unresolved_count: u64,
    ambiguous_count: u64,
    invalid_evidence_count: u64,
    inventory_failed_files: BTreeSet<RepositoryPath>,
    metadata_unavailable_files: BTreeSet<RepositoryPath>,
    gap_codes: BTreeSet<GapCode>,
}

impl ImportCapabilityAudit {
    fn truncated_count(&self) -> u64 {
        self.unresolved_count + self.ambiguous_count + self.invalid_evidence_count
    }

    fn denominator_is_known(&self) -> bool {
        self.inventory_failed_files.is_empty() && self.metadata_unavailable_files.is_empty()
    }

    fn merge(&mut self, other: &Self) {
        self.eligible_count += other.eligible_count;
        self.covered_count += other.covered_count;
        self.internal_relation_count += other.internal_relation_count;
        self.known_external_count += other.known_external_count;
        self.unresolved_count += other.unresolved_count;
        self.ambiguous_count += other.ambiguous_count;
        self.invalid_evidence_count += other.invalid_evidence_count;
        self.inventory_failed_files
            .extend(other.inventory_failed_files.iter().cloned());
        self.metadata_unavailable_files
            .extend(other.metadata_unavailable_files.iter().cloned());
        self.gap_codes.extend(other.gap_codes.iter().copied());
    }
}

#[derive(Clone, Debug, Default)]
struct ImportAudit {
    site_keys: Vec<String>,
    capabilities: BTreeMap<AnalysisCapability, ImportCapabilityAudit>,
    entries: Vec<ImportAuditEntry>,
}

impl ImportAudit {
    fn for_language(language: ProgrammingLanguage) -> Self {
        let mut audit = Self::default();
        audit
            .capabilities
            .entry(AnalysisCapability::Imports)
            .or_default();
        if supports_exports(language) {
            audit
                .capabilities
                .entry(AnalysisCapability::Exports)
                .or_default();
        }
        audit
    }

    fn capability(&self, capability: AnalysisCapability) -> Option<&ImportCapabilityAudit> {
        self.capabilities.get(&capability)
    }

    fn capability_mut(&mut self, capability: AnalysisCapability) -> &mut ImportCapabilityAudit {
        self.capabilities.entry(capability).or_default()
    }

    fn blocking_count(&self) -> u64 {
        self.capabilities
            .values()
            .map(|audit| {
                audit.truncated_count()
                    + audit.inventory_failed_files.len() as u64
                    + audit.metadata_unavailable_files.len() as u64
            })
            .sum()
    }

    fn canonicalize(&mut self) {
        self.site_keys.sort();
        self.site_keys.dedup();
        canonicalize_import_entries(&mut self.entries);
    }

    fn merge(&mut self, other: &Self) {
        self.absorb(other.clone());
        self.canonicalize();
    }

    fn absorb(&mut self, mut other: Self) {
        self.site_keys.append(&mut other.site_keys);
        for (capability, audit) in &other.capabilities {
            self.capability_mut(*capability).merge(audit);
        }
        self.entries.append(&mut other.entries);
    }
}

#[derive(Clone, Debug, Default)]
struct TypeRelationAudit {
    explicit_site_keys: BTreeSet<String>,
    matched_explicit_site_keys: BTreeSet<String>,
    inventory_failed_files: BTreeSet<RepositoryPath>,
    relation_keys: Vec<String>,
    entries: Vec<TypeRelationAuditEntry>,
}

impl TypeRelationAudit {
    fn merge(&mut self, other: &Self) {
        self.explicit_site_keys
            .extend(other.explicit_site_keys.iter().cloned());
        self.matched_explicit_site_keys
            .extend(other.matched_explicit_site_keys.iter().cloned());
        self.inventory_failed_files
            .extend(other.inventory_failed_files.iter().cloned());
        self.relation_keys
            .extend(other.relation_keys.iter().cloned());
        self.relation_keys.sort();
        self.relation_keys.dedup();
        self.entries.extend(other.entries.iter().cloned());
        canonicalize_type_relation_entries(&mut self.entries);
    }
}

struct ImportDraft {
    source: RepositoryPath,
    target: IrEndpoint,
    relation: LanguageRelationKind,
    resolution: ResolutionMethod,
    span: SourceSpan,
}

pub(crate) struct LanguageIrEmissionInput<'a> {
    pub(crate) project_root: &'a Path,
    pub(crate) manifest: &'a SourceManifest,
    pub(crate) plan: &'a AnalysisPlan,
    pub(crate) providers_root: Option<&'a Path>,
    pub(crate) languages: &'a [LanguageOutput],
    pub(crate) coverage: &'a [FileCoverageOutput],
    pub(crate) documents: &'a [DocumentOutput],
    pub(crate) relations: &'a [RelationOutput],
    pub(crate) file_relations: &'a [FileRelationOutput],
    pub(crate) project_model_files: &'a [String],
    pub(crate) diagnostics: &'a [Diagnostic],
    pub(crate) execution_contexts: &'a BTreeMap<String, ProviderExecutionContext>,
    pub(crate) static_analyzer_set_digest: Sha256Digest,
}

pub(super) fn emit_language_ir(
    input: LanguageIrEmissionInput<'_>,
    artifact_root: &Path,
) -> Result<LanguageIrEmission, String> {
    let LanguageIrEmissionInput {
        project_root,
        manifest,
        plan,
        providers_root,
        languages,
        coverage,
        documents,
        relations,
        file_relations,
        project_model_files,
        diagnostics,
        execution_contexts,
        static_analyzer_set_digest,
    } = input;
    manifest
        .validate()
        .map_err(|error| format!("invalid source manifest before Language IR: {error}"))?;
    plan.validate_against(manifest)
        .map_err(|error| format!("invalid analysis plan before Language IR: {error}"))?;
    let import_index = ProjectImportIndex::build(
        project_root,
        manifest,
        plan,
        file_relations,
        project_model_files,
    )?;

    let language_outputs = languages
        .iter()
        .filter_map(|output| language_from_id(&output.id).map(|language| (language, output)))
        .collect::<BTreeMap<_, _>>();
    let mut descriptors = BTreeMap::<ProgrammingLanguage, Option<ProviderDescriptor>>::new();
    for unit in &plan.units {
        if descriptors.contains_key(&unit.language) {
            continue;
        }
        let descriptor = match language_outputs.get(&unit.language) {
            Some(output) => {
                resolve_provider_descriptor(unit.language, output.provider, providers_root)?
            }
            None => None,
        };
        descriptors.insert(unit.language, descriptor);
    }
    let provider_set_digest = provider_set_digest(&descriptors, static_analyzer_set_digest);
    let execution_context_set_digest = execution_context_set_digest(execution_contexts);
    let snapshot_id = SnapshotId::from_execution_inputs(
        &manifest.workspace_id,
        manifest.manifest_digest,
        plan.plan_digest,
        provider_set_digest,
        execution_context_set_digest,
    )
    .map_err(|error| format!("cannot build Language IR snapshot ID: {error}"))?;
    let mut artifact_writer =
        AtomicLanguageIrArtifactWriter::create(project_root, artifact_root, snapshot_id.as_str())?;

    let mut files_by_unit = BTreeMap::<String, Vec<RepositoryPath>>::new();
    for assignment in &plan.assignments {
        for unit_id in &assignment.unit_ids {
            files_by_unit
                .entry(unit_id.as_str().to_string())
                .or_default()
                .push(assignment.path.clone());
        }
    }
    for files in files_by_unit.values_mut() {
        files.sort();
        files.dedup();
    }
    let mut gaps_by_unit = BTreeMap::<String, Vec<AnalysisGap>>::new();
    for gap in &plan.gaps {
        if let Some(unit_id) = gap.scope.unit_id() {
            gaps_by_unit
                .entry(unit_id.as_str().to_string())
                .or_default()
                .push(gap.clone());
        }
    }

    let manifest_files = manifest
        .files
        .iter()
        .map(|file| (file.path.clone(), file))
        .collect::<BTreeMap<_, _>>();
    let mut summaries = Vec::new();
    let mut unavailable = Vec::new();
    for unit in &plan.units {
        let Some(language_output) = language_outputs.get(&unit.language).copied() else {
            unavailable.push(UnavailableUnitReceipt {
                unit_id: unit.id.as_str().to_string(),
                language: unit.language,
                root: unit.root.clone(),
                gap_code: GapCode::ProviderExecutionIncomplete,
            });
            continue;
        };
        let Some(provider) = descriptors
            .get(&unit.language)
            .and_then(|descriptor| descriptor.as_ref())
        else {
            unavailable.push(UnavailableUnitReceipt {
                unit_id: unit.id.as_str().to_string(),
                language: unit.language,
                root: unit.root.clone(),
                gap_code: GapCode::ProviderUnavailable,
            });
            continue;
        };
        let assigned_files = files_by_unit
            .get(unit.id.as_str())
            .ok_or_else(|| format!("analysis unit has no assigned files: {}", unit.id))?;
        let unit_gaps = gaps_by_unit
            .get(unit.id.as_str())
            .map(Vec::as_slice)
            .unwrap_or_default();
        let execution_context = execution_contexts.get(unit.id.as_str()).ok_or_else(|| {
            format!(
                "analysis unit has no executed provider context: {}",
                unit.id
            )
        })?;
        let mut sink = ValidatingDigestSink::new(&mut artifact_writer);
        let adapter_summary = emit_unit(
            UnitAdapterInput {
                project_root,
                manifest_digest: manifest.manifest_digest,
                snapshot_id: snapshot_id.clone(),
                unit,
                provider,
                execution_context,
                language_output,
                assigned_files,
                manifest_files: &manifest_files,
                coverage,
                documents,
                relations,
                import_index: &import_index,
                diagnostics,
                plan_gaps: unit_gaps,
            },
            &mut sink,
        )?;
        let (stream_digest, semantic_payload_digest, record_count) = sink.finish()?;
        summaries.push(UnitEmissionSummary {
            unit_id: unit.id.as_str().to_string(),
            language: unit.language,
            stream_digest,
            semantic_payload_digest,
            record_count,
            completion: adapter_summary.completion,
            omitted_definition_count: adapter_summary.omitted_definition_count,
            omitted_relation_count: adapter_summary.omitted_relation_count,
            definition_audit: adapter_summary.definition_audit,
            import_audit: adapter_summary.import_audit,
            type_relation_audit: adapter_summary.type_relation_audit,
        });
    }
    summaries.sort_by(|left, right| left.unit_id.cmp(&right.unit_id));
    unavailable.sort();
    let stream_set_digest = stream_set_digest(&summaries);
    let semantic_payload_set_digest = semantic_payload_set_digest(&summaries);
    let mut definition_audit = DefinitionAudit::default();
    let mut definition_audit_by_language = BTreeMap::<ProgrammingLanguage, DefinitionAudit>::new();
    for summary in &summaries {
        definition_audit.merge(&summary.definition_audit);
        definition_audit_by_language
            .entry(summary.language)
            .or_default()
            .merge(&summary.definition_audit);
    }
    let definition_language_summaries = definition_audit_by_language
        .into_iter()
        .map(|(language, audit)| DefinitionLanguageSummary {
            language,
            definition_set_digest: definition_set_digest(&audit.definition_keys),
            syntax_definition_count: audit.syntax_definition_count,
            owned_syntax_definition_count: audit.owned_syntax_definition_count,
            matched_definition_count: audit.matched_definition_count,
            missing_syntax_definition_count: audit.missing_syntax_definition_count,
            extra_provider_definition_count: audit.extra_provider_definition_count,
            provider_definition_alias_count: audit.provider_definition_alias_count,
            kind_refinement_count: audit.kind_refinement_count,
            owner_repair_count: audit.owner_repair_count,
            resolved_owner_count: audit.resolved_owner_count,
            unresolved_owner_count: audit.unresolved_owner_count,
            inventory_failed_file_count: audit.inventory_failed_file_count,
            metadata_set_digest: definition_set_digest(&audit.metadata_keys),
            metadata_definition_count: audit.metadata_definition_count,
            callable_definition_count: audit.callable_definition_count,
            callable_signature_count: audit.callable_signature_count,
            known_visibility_count: audit.known_visibility_count,
        })
        .collect::<Vec<_>>();
    let mut import_audit = ImportAudit::default();
    let mut import_audit_by_language = BTreeMap::<ProgrammingLanguage, ImportAudit>::new();
    for summary in &summaries {
        import_audit.merge(&summary.import_audit);
        import_audit_by_language
            .entry(summary.language)
            .or_default()
            .merge(&summary.import_audit);
    }
    let import_language_summaries = import_audit_by_language
        .into_iter()
        .map(|(language, audit)| import_language_summary(language, &audit))
        .collect::<Vec<_>>();
    let mut type_relation_audit = TypeRelationAudit::default();
    let mut type_relation_audit_by_language =
        BTreeMap::<ProgrammingLanguage, TypeRelationAudit>::new();
    for summary in &summaries {
        type_relation_audit.merge(&summary.type_relation_audit);
        type_relation_audit_by_language
            .entry(summary.language)
            .or_default()
            .merge(&summary.type_relation_audit);
    }
    let type_relation_language_summaries = type_relation_audit_by_language
        .into_iter()
        .map(|(language, audit)| type_relation_language_summary(language, &audit))
        .collect::<Vec<_>>();

    let receipt = LanguageIrMigrationReceipt {
        schema: MIGRATION_RECEIPT_SCHEMA,
        snapshot_id: snapshot_id.clone(),
        source_manifest_digest: manifest.manifest_digest,
        analysis_plan_digest: plan.plan_digest,
        provider_set_digest,
        execution_context_set_digest,
        stream_set_digest,
        semantic_payload_set_digest,
        emitted_unit_count: summaries.len() as u64,
        unavailable_unit_count: unavailable.len() as u64,
        record_count: summaries.iter().map(|item| item.record_count).sum(),
        file_record_count: summaries
            .iter()
            .map(|item| item.completion.file_record_count)
            .sum(),
        definition_count: summaries
            .iter()
            .map(|item| item.completion.definition_count)
            .sum(),
        relation_count: summaries
            .iter()
            .map(|item| item.completion.relation_count)
            .sum(),
        evidence_count: summaries
            .iter()
            .map(|item| item.completion.evidence_count)
            .sum(),
        capability_receipt_count: summaries
            .iter()
            .map(|item| item.completion.capability_receipt_count)
            .sum(),
        gap_count: summaries.iter().map(|item| item.completion.gap_count).sum(),
        issue_count: summaries
            .iter()
            .map(|item| item.completion.issue_count)
            .sum(),
        omitted_definition_count: summaries
            .iter()
            .map(|item| item.omitted_definition_count)
            .sum(),
        omitted_relation_count: summaries
            .iter()
            .map(|item| item.omitted_relation_count)
            .sum(),
        syntax_definition_count: definition_audit.syntax_definition_count,
        matched_definition_count: definition_audit.matched_definition_count,
        missing_syntax_definition_count: definition_audit.missing_syntax_definition_count,
        extra_provider_definition_count: definition_audit.extra_provider_definition_count,
        provider_definition_alias_count: definition_audit.provider_definition_alias_count,
        kind_refinement_count: definition_audit.kind_refinement_count,
        owner_repair_count: definition_audit.owner_repair_count,
        unresolved_owner_count: definition_audit.unresolved_owner_count,
        definition_inventory_failed_file_count: definition_audit.inventory_failed_file_count,
    };
    let diagnostics = LanguageIrDiagnosticReceipt {
        schema: DIAGNOSTIC_RECEIPT_SCHEMA,
        snapshot_id,
        definition_language_summaries,
        definition_audit_sample: definition_audit
            .failures
            .iter()
            .take(DEFINITION_AUDIT_SAMPLE_LIMIT)
            .cloned()
            .collect(),
        definition_metadata_audit_sample: definition_audit
            .metadata_entries
            .iter()
            .take(DEFINITION_METADATA_AUDIT_SAMPLE_LIMIT)
            .cloned()
            .collect(),
        import_language_summaries,
        import_audit_sample: import_audit
            .entries
            .iter()
            .take(IMPORT_AUDIT_SAMPLE_LIMIT)
            .cloned()
            .collect(),
        type_relation_language_summaries,
        type_relation_audit_sample: type_relation_audit
            .entries
            .iter()
            .take(TYPE_RELATION_AUDIT_SAMPLE_LIMIT)
            .cloned()
            .collect(),
        unavailable_unit_sample: unavailable
            .iter()
            .take(UNAVAILABLE_SAMPLE_LIMIT)
            .cloned()
            .collect(),
        details_truncated: unavailable.len() > UNAVAILABLE_SAMPLE_LIMIT
            || definition_audit.failures.len() > DEFINITION_AUDIT_SAMPLE_LIMIT
            || definition_audit.metadata_entries.len() > DEFINITION_METADATA_AUDIT_SAMPLE_LIMIT
            || import_audit.entries.len() > IMPORT_AUDIT_SAMPLE_LIMIT
            || type_relation_audit.entries.len() > TYPE_RELATION_AUDIT_SAMPLE_LIMIT,
    };
    let artifact_file = artifact_writer.finish()?;
    if artifact_file.record_count != receipt.record_count {
        return Err(format!(
            "Language IR artifact record count does not match receipt: artifact={} receipt={}",
            artifact_file.record_count, receipt.record_count
        ));
    }
    Ok(LanguageIrEmission {
        artifact: LanguageIrStreamArtifact {
            schema: STREAM_ARTIFACT_SCHEMA,
            snapshot_id: receipt.snapshot_id.clone(),
            stream_set_digest: receipt.stream_set_digest,
            content_digest: artifact_file.content_digest,
            record_count: artifact_file.record_count,
            byte_count: artifact_file.byte_count,
            complete: true,
            path: artifact_file.path,
        },
        receipt,
        diagnostics,
    })
}

struct UnitAdapterInput<'a> {
    project_root: &'a Path,
    manifest_digest: Sha256Digest,
    snapshot_id: SnapshotId,
    unit: &'a AnalysisUnit,
    provider: &'a ProviderDescriptor,
    execution_context: &'a ProviderExecutionContext,
    language_output: &'a LanguageOutput,
    assigned_files: &'a [RepositoryPath],
    manifest_files: &'a BTreeMap<RepositoryPath, &'a SourceManifestFile>,
    coverage: &'a [FileCoverageOutput],
    documents: &'a [DocumentOutput],
    relations: &'a [RelationOutput],
    import_index: &'a ProjectImportIndex,
    diagnostics: &'a [Diagnostic],
    plan_gaps: &'a [AnalysisGap],
}

struct AdapterSummary {
    completion: AnalysisUnitCompletion,
    omitted_definition_count: u64,
    omitted_relation_count: u64,
    definition_audit: DefinitionAudit,
    import_audit: ImportAudit,
    type_relation_audit: TypeRelationAudit,
}

fn emit_unit(
    input: UnitAdapterInput<'_>,
    sink: &mut dyn LanguageIrSink,
) -> Result<AdapterSummary, String> {
    let timing_enabled = std::env::var_os("CODE_MEMORY_LANGUAGE_IR_TIMING").is_some();
    let unit_started = Instant::now();
    let mut phase_started = Instant::now();
    let (assigned, file_records) = emit_unit_header_and_coverage(&input, sink)?;
    emit_unit_timing(
        timing_enabled,
        input.unit,
        "coverage",
        phase_started,
        unit_started,
    );
    phase_started = Instant::now();

    let UnitSourceInventory {
        definition_audit,
        syntax_definitions,
        syntax_type_relations,
        syntax_type_uses,
        type_relation_inventory_failed_files,
        import_audit,
        import_drafts,
    } = inventory_unit_sources(&input, &assigned, timing_enabled)?;
    emit_unit_timing(
        timing_enabled,
        input.unit,
        "source_inventory",
        phase_started,
        unit_started,
    );
    let mut type_relation_audit = TypeRelationAudit {
        inventory_failed_files: type_relation_inventory_failed_files.clone(),
        ..TypeRelationAudit::default()
    };
    for (path, sites) in &syntax_type_relations {
        for site in sites {
            type_relation_audit
                .explicit_site_keys
                .insert(type_relation_site_key(path, site));
        }
    }

    let definitions = reconcile_unit_definitions(
        &input,
        &assigned,
        &syntax_definitions,
        definition_audit,
        timing_enabled,
        unit_started,
    )?;
    let ClassifiedUnitFacts {
        evidence,
        definition_records,
        relation_records,
        emitted_relations,
        omitted_relations,
        omitted_definition_count,
        definition_audit,
        mut type_relation_audit,
    } = classify_unit_relations(
        RelationClassificationStage {
            input: &input,
            syntax_definitions: &syntax_definitions,
            syntax_type_relations: &syntax_type_relations,
            syntax_type_uses: &syntax_type_uses,
            import_drafts,
            type_relation_audit,
            definitions,
        },
        timing_enabled,
        unit_started,
    )?;
    phase_started = Instant::now();

    for fact_evidence in evidence.values() {
        sink.push(LanguageIrRecord::Evidence(fact_evidence.clone()))?;
    }
    for definition in &definition_records {
        sink.push(LanguageIrRecord::Definition(definition.clone()))?;
    }
    for relation in &relation_records {
        sink.push(LanguageIrRecord::Relation(relation.clone()))?;
    }
    emit_unit_timing(
        timing_enabled,
        input.unit,
        "stream_emission",
        phase_started,
        unit_started,
    );

    let mut gaps = input.plan_gaps.to_vec();
    let mut issues = Vec::new();
    for diagnostic in input.diagnostics.iter().filter(|diagnostic| {
        diagnostic.language == input.unit.language.as_str()
            && diagnostic
                .path
                .as_deref()
                .is_none_or(|path| assigned.iter().any(|candidate| candidate.as_str() == path))
    }) {
        if let Some(gap) = diagnostic_gap(input.unit, diagnostic) {
            gaps.push(gap);
        }
        if let Some(issue) = diagnostic_issue(input.unit, diagnostic) {
            issues.push(issue);
        }
    }
    append_import_gaps(input.unit, &import_audit, &mut gaps);
    let base_state = unit_state(input.language_output.status);
    let state = if base_state == AnalysisUnitState::Complete
        && (omitted_definition_count > 0
            || omitted_relations.values().copied().sum::<u64>() > 0
            || !input.plan_gaps.is_empty()
            || definition_audit.blocking_count() > 0
            || import_audit.blocking_count() > 0
            || !type_relation_inventory_failed_files.is_empty())
    {
        AnalysisUnitState::Partial
    } else {
        base_state
    };
    if base_state != AnalysisUnitState::Complete {
        gaps.push(AnalysisGap {
            code: GapCode::ProviderExecutionIncomplete,
            scope: AnalysisScope::AnalysisUnit {
                unit_id: input.unit.id.clone(),
            },
            capability: None,
            evidence_ids: Vec::new(),
            message: "The provider-to-Language-IR unit is incomplete; zero facts must not be interpreted as absence".to_string(),
        });
    }
    if omitted_definition_count > 0 || omitted_relations.values().copied().sum::<u64>() > 0 {
        gaps.push(AnalysisGap {
            code: GapCode::UnresolvedTarget,
            scope: AnalysisScope::AnalysisUnit {
                unit_id: input.unit.id.clone(),
            },
            capability: None,
            evidence_ids: Vec::new(),
            message: "Provider records without an exact source range or typed endpoint were not promoted into Language IR".to_string(),
        });
    }
    if definition_audit.blocking_count() > 0 {
        gaps.push(AnalysisGap {
            code: GapCode::UnresolvedTarget,
            scope: AnalysisScope::AnalysisUnit {
                unit_id: input.unit.id.clone(),
            },
            capability: Some(AnalysisCapability::Definitions),
            evidence_ids: Vec::new(),
            message: "The provider definition set did not exactly reconcile with the independent source-definition inventory".to_string(),
        });
    }
    if !type_relation_inventory_failed_files.is_empty() {
        gaps.push(AnalysisGap {
            code: GapCode::ProviderExecutionIncomplete,
            scope: AnalysisScope::AnalysisUnit {
                unit_id: input.unit.id.clone(),
            },
            capability: Some(AnalysisCapability::TypeRelations),
            evidence_ids: Vec::new(),
            message: "The explicit type-relation syntax inventory was incomplete; no guessed hierarchy relation was emitted".to_string(),
        });
    }
    canonicalize_gaps(&mut gaps);
    canonicalize_issues(&mut issues);

    let capability_receipts = build_capability_receipts(
        input.unit,
        input.provider.protocol,
        state,
        file_records.len() as u64,
        definition_records.len() as u64,
        definition_audit.syntax_definition_count,
        definition_audit.matched_definition_count,
        omitted_definition_count,
        &emitted_relations,
        &omitted_relations,
        &import_audit,
        &gaps,
    )?;
    for receipt in &capability_receipts {
        sink.push(LanguageIrRecord::CapabilityReceipt(receipt.clone()))?;
    }
    for gap in &gaps {
        sink.push(LanguageIrRecord::Gap(gap.clone()))?;
    }
    for issue in &issues {
        sink.push(LanguageIrRecord::Issue(issue.clone()))?;
    }

    let completion = AnalysisUnitCompletion {
        unit_id: input.unit.id.clone(),
        state,
        file_record_count: file_records.len() as u64,
        definition_count: definition_records.len() as u64,
        relation_count: relation_records.len() as u64,
        evidence_count: evidence.len() as u64,
        capability_receipt_count: capability_receipts.len() as u64,
        gap_count: gaps.len() as u64,
        issue_count: issues.len() as u64,
    };
    sink.push(LanguageIrRecord::Complete(completion.clone()))?;
    type_relation_audit.relation_keys.sort();
    type_relation_audit.relation_keys.dedup();
    canonicalize_type_relation_entries(&mut type_relation_audit.entries);
    Ok(AdapterSummary {
        completion,
        omitted_definition_count,
        omitted_relation_count: omitted_relations.values().copied().sum(),
        definition_audit,
        import_audit,
        type_relation_audit,
    })
}

struct UnitSourceInventory {
    definition_audit: DefinitionAudit,
    syntax_definitions: BTreeMap<RepositoryPath, Vec<SyntaxDefinition>>,
    syntax_type_relations: BTreeMap<RepositoryPath, Vec<SyntaxTypeRelationSite>>,
    syntax_type_uses: BTreeMap<RepositoryPath, Vec<SyntaxTypeUseSite>>,
    type_relation_inventory_failed_files: BTreeSet<RepositoryPath>,
    import_audit: ImportAudit,
    import_drafts: Vec<ImportDraft>,
}

struct ReconciledUnitDefinitions<'a> {
    unit_documents: Vec<&'a DocumentOutput>,
    document_paths: BTreeSet<&'a str>,
    evidence: BTreeMap<EvidenceId, FactEvidence>,
    definitions: BTreeMap<ProviderSymbolId, DefinitionDraft>,
    definition_spans: BTreeMap<ProviderSymbolId, SourceSpan>,
    provider_definition_aliases: BTreeMap<ProviderSymbolId, ProviderSymbolId>,
    discarded_definition_ids: BTreeSet<ProviderSymbolId>,
    omitted_definition_count: u64,
    definition_records: Vec<IrDefinition>,
    definition_audit: DefinitionAudit,
}

struct RelationClassificationStage<'a> {
    input: &'a UnitAdapterInput<'a>,
    syntax_definitions: &'a BTreeMap<RepositoryPath, Vec<SyntaxDefinition>>,
    syntax_type_relations: &'a BTreeMap<RepositoryPath, Vec<SyntaxTypeRelationSite>>,
    syntax_type_uses: &'a BTreeMap<RepositoryPath, Vec<SyntaxTypeUseSite>>,
    import_drafts: Vec<ImportDraft>,
    type_relation_audit: TypeRelationAudit,
    definitions: ReconciledUnitDefinitions<'a>,
}

struct ClassifiedUnitFacts {
    evidence: BTreeMap<EvidenceId, FactEvidence>,
    definition_records: Vec<IrDefinition>,
    relation_records: Vec<IrRelation>,
    emitted_relations: BTreeMap<AnalysisCapability, u64>,
    omitted_relations: BTreeMap<AnalysisCapability, u64>,
    omitted_definition_count: u64,
    definition_audit: DefinitionAudit,
    type_relation_audit: TypeRelationAudit,
}

fn classify_unit_relations(
    stage: RelationClassificationStage<'_>,
    timing_enabled: bool,
    unit_started: Instant,
) -> Result<ClassifiedUnitFacts, String> {
    let RelationClassificationStage {
        input,
        syntax_definitions,
        syntax_type_relations,
        syntax_type_uses,
        import_drafts,
        mut type_relation_audit,
        definitions,
    } = stage;
    let ReconciledUnitDefinitions {
        unit_documents,
        document_paths,
        mut evidence,
        definitions,
        definition_spans,
        provider_definition_aliases,
        discarded_definition_ids,
        omitted_definition_count,
        definition_records,
        definition_audit,
    } = definitions;
    let mut phase_started = Instant::now();
    let mut unit_relations = input
        .relations
        .iter()
        .filter(|relation| document_paths.contains(relation.path.as_str()))
        .collect::<Vec<_>>();
    unit_relations.sort_by(|left, right| {
        (&left.path, &left.range, &left.from, &left.to, &left.kind).cmp(&(
            &right.path,
            &right.range,
            &right.from,
            &right.to,
            &right.kind,
        ))
    });
    let hierarchy_endpoint_ids = unit_relations
        .iter()
        .filter(|relation| {
            provider_relation_capability(&relation.kind) == Some(AnalysisCapability::TypeRelations)
        })
        .flat_map(|relation| [&relation.from, &relation.to])
        .filter_map(|raw| ProviderSymbolId::parse(raw).ok())
        .map(|id| provider_definition_aliases.get(&id).cloned().unwrap_or(id))
        .collect::<BTreeSet<_>>();
    let mut hierarchy_occurrence_ranges =
        BTreeMap::<String, BTreeMap<ProviderSymbolId, Vec<Vec<i32>>>>::new();
    for document in &unit_documents {
        let mut by_symbol = BTreeMap::<ProviderSymbolId, Vec<Vec<i32>>>::new();
        for occurrence in &document.occurrences {
            let Ok(id) = ProviderSymbolId::parse(&occurrence.symbol) else {
                continue;
            };
            let id = provider_definition_aliases.get(&id).cloned().unwrap_or(id);
            if hierarchy_endpoint_ids.contains(&id) {
                by_symbol
                    .entry(id)
                    .or_default()
                    .push(occurrence.range.clone());
            }
        }
        if !by_symbol.is_empty() {
            hierarchy_occurrence_ranges.insert(document.path.clone(), by_symbol);
        }
    }
    emit_unit_timing(
        timing_enabled,
        input.unit,
        "relation_indexes",
        phase_started,
        unit_started,
    );
    phase_started = Instant::now();

    let mut relation_records = Vec::new();
    let mut emitted_relations = BTreeMap::<AnalysisCapability, u64>::new();
    let mut omitted_relations = BTreeMap::<AnalysisCapability, u64>::new();
    let mut current_path = None::<String>;
    let mut current_coordinates = None::<SourceCoordinates>;
    let relation_classification = ProviderRelationClassificationContext {
        language: input.unit.language,
        protocol: input.provider.protocol,
        definitions: &definitions,
        syntax_definitions,
        syntax_type_relations,
        syntax_type_uses,
        hierarchy_occurrence_ranges: &hierarchy_occurrence_ranges,
    };
    for relation in unit_relations {
        let Some(capability) = provider_relation_capability(&relation.kind) else {
            continue;
        };
        let source = match endpoint(&relation.from) {
            Ok(endpoint) => endpoint,
            Err(_) => {
                *omitted_relations.entry(capability).or_default() += 1;
                continue;
            }
        };
        let target = match endpoint(&relation.to) {
            Ok(endpoint) => endpoint,
            Err(_) => {
                *omitted_relations.entry(capability).or_default() += 1;
                continue;
            }
        };
        let source = remap_provider_alias(source, &provider_definition_aliases);
        let target = remap_provider_alias(target, &provider_definition_aliases);
        let Some((mut source, mut target)) = retain_relation_endpoints(
            relation,
            capability,
            source,
            target,
            &discarded_definition_ids,
        ) else {
            *omitted_relations.entry(capability).or_default() += 1;
            continue;
        };
        let Some(mapping) =
            classify_provider_relation(relation, &source, &target, &relation_classification)
        else {
            *omitted_relations.entry(capability).or_default() += 1;
            continue;
        };
        if mapping.reverse_endpoints {
            std::mem::swap(&mut source, &mut target);
        }
        if source == target {
            continue;
        }
        let evidence_range = mapping.evidence_range.as_deref().unwrap_or(&relation.range);
        let span = if evidence_range.is_empty() {
            match &source {
                IrEndpoint::NativeSymbol { symbol_id } => definition_spans.get(symbol_id).cloned(),
                IrEndpoint::File { .. } | IrEndpoint::Structure { .. } => None,
            }
        } else {
            if current_path.as_deref() != Some(&relation.path) {
                let path = RepositoryPath::parse(&relation.path)
                    .map_err(|error| format!("provider relation has invalid path: {error}"))?;
                let manifest_file = input.manifest_files.get(&path).copied().ok_or_else(|| {
                    format!("provider relation path is absent from manifest: {path}")
                })?;
                current_coordinates =
                    Some(SourceCoordinates::load(input.project_root, manifest_file)?);
                current_path = Some(relation.path.clone());
            }
            current_coordinates.as_ref().and_then(|coordinates| {
                coordinates
                    .span(evidence_range, input.provider.protocol)
                    .ok()
            })
        };
        let Some(span) = span else {
            *omitted_relations.entry(mapping.capability).or_default() += 1;
            continue;
        };
        if mapping.capability == AnalysisCapability::TypeRelations {
            if let Some(site_key) = &mapping.matched_explicit_site_key {
                type_relation_audit
                    .matched_explicit_site_keys
                    .insert(site_key.clone());
            }
            let (source_symbol, source_name) = type_relation_endpoint(&source, &definitions);
            let (target_symbol, target_name) = type_relation_endpoint(&target, &definitions);
            let entry = TypeRelationAuditEntry {
                unit_id: input.unit.id.as_str().to_string(),
                language: input.unit.language,
                path: span.path.clone(),
                native_relation: relation.kind.clone(),
                kind: mapping.kind,
                source_symbol,
                source_name,
                target_symbol,
                target_name,
                start_line: span.start.line,
                start_utf8_column: span.start.utf8_column,
                end_line: span.end.line,
                end_utf8_column: span.end.utf8_column,
            };
            type_relation_audit
                .relation_keys
                .push(type_relation_audit_key(&entry));
            type_relation_audit.entries.push(entry);
        }
        let fact_evidence = FactEvidence::new(
            mapping.evidence_kind,
            evidence_producer(input.provider, "provider-relation"),
            EvidenceLocation::Source { span },
            None,
        )
        .map_err(|error| format!("cannot build relation evidence: {error}"))?;
        let evidence_id = fact_evidence.id.clone();
        evidence.insert(evidence_id.clone(), fact_evidence);
        relation_records.push(IrRelation {
            unit_id: input.unit.id.clone(),
            source,
            target,
            kind: mapping.kind,
            truth: FactTruth::Confirmed,
            resolution: ResolutionMethod::Provider,
            dispatch: match mapping.kind {
                LanguageRelationKind::Calls => DispatchKind::Unknown,
                LanguageRelationKind::Constructs => DispatchKind::Direct,
                _ => DispatchKind::NotApplicable,
            },
            semantic_context_id: input.unit.context.id.clone(),
            evidence_ids: vec![evidence_id],
        });
        *emitted_relations.entry(mapping.capability).or_default() += 1;
    }

    for draft in import_drafts {
        let capability = match draft.relation {
            LanguageRelationKind::Imports => AnalysisCapability::Imports,
            LanguageRelationKind::Exports => AnalysisCapability::Exports,
            _ => unreachable!("import draft must be imports or exports"),
        };
        let fact_evidence = FactEvidence::new(
            EvidenceKind::ImportSite,
            syntax_import_evidence_producer(),
            EvidenceLocation::Source { span: draft.span },
            None,
        )
        .map_err(|error| format!("cannot build import evidence: {error}"))?;
        let evidence_id = fact_evidence.id.clone();
        evidence.insert(evidence_id.clone(), fact_evidence);
        relation_records.push(IrRelation {
            unit_id: input.unit.id.clone(),
            source: IrEndpoint::File { path: draft.source },
            target: draft.target,
            kind: draft.relation,
            truth: FactTruth::Confirmed,
            resolution: draft.resolution,
            dispatch: DispatchKind::NotApplicable,
            semantic_context_id: input.unit.context.id.clone(),
            evidence_ids: vec![evidence_id],
        });
        *emitted_relations.entry(capability).or_default() += 1;
    }
    relation_records.sort_by_key(relation_sort_key);
    emit_unit_timing(
        timing_enabled,
        input.unit,
        "relation_classification",
        phase_started,
        unit_started,
    );

    Ok(ClassifiedUnitFacts {
        evidence,
        definition_records,
        relation_records,
        emitted_relations,
        omitted_relations,
        omitted_definition_count,
        definition_audit,
        type_relation_audit,
    })
}

fn reconcile_unit_definitions<'a>(
    input: &'a UnitAdapterInput<'a>,
    assigned: &BTreeSet<RepositoryPath>,
    syntax_definitions: &BTreeMap<RepositoryPath, Vec<SyntaxDefinition>>,
    mut definition_audit: DefinitionAudit,
    timing_enabled: bool,
    unit_started: Instant,
) -> Result<ReconciledUnitDefinitions<'a>, String> {
    let mut phase_started = Instant::now();
    let mut unit_documents = input
        .documents
        .iter()
        .filter(|document| {
            normalize_scip_language(&document.language, input.unit.language.as_str())
                == input.unit.language.as_str()
                && RepositoryPath::parse(&document.path).is_ok_and(|path| assigned.contains(&path))
        })
        .collect::<Vec<_>>();
    unit_documents.sort_by(|left, right| left.path.cmp(&right.path));
    let document_paths = unit_documents
        .iter()
        .map(|document| document.path.as_str())
        .collect::<BTreeSet<_>>();
    let mut evidence = BTreeMap::<EvidenceId, FactEvidence>::new();
    let mut definitions = BTreeMap::<ProviderSymbolId, DefinitionDraft>::new();
    let mut definition_spans = BTreeMap::<ProviderSymbolId, SourceSpan>::new();
    let mut ignored_provider_definition_ids = BTreeSet::<ProviderSymbolId>::new();
    let mut omitted_definition_count = 0_u64;

    for document in &unit_documents {
        let path = RepositoryPath::parse(&document.path)
            .map_err(|error| format!("provider returned invalid document path: {error}"))?;
        let manifest_file = input
            .manifest_files
            .get(&path)
            .copied()
            .ok_or_else(|| format!("provider document is absent from manifest: {path}"))?;
        let coordinates = SourceCoordinates::load(input.project_root, manifest_file)?;
        let definition_occurrences_by_symbol = document
            .occurrences
            .iter()
            .filter(|occurrence| occurrence.definition)
            .fold(
                HashMap::<&str, &OccurrenceOutput>::new(),
                |mut index, occurrence| {
                    index
                        .entry(occurrence.symbol.as_str())
                        .and_modify(|current| {
                            if occurrence.range < current.range {
                                *current = occurrence;
                            }
                        })
                        .or_insert(occurrence);
                    index
                },
            );
        let mut symbols = document.symbols.iter().collect::<Vec<_>>();
        symbols.sort_by(|left, right| left.symbol.cmp(&right.symbol));
        for symbol in symbols {
            let kind = canonical_definition_kind(&symbol.kind);
            let field_candidate = kind.is_none() && is_variable_definition_kind(&symbol.kind);
            let Some(kind) = kind.or(field_candidate.then_some(FactNodeKind::Field)) else {
                continue;
            };
            let Some(occurrence) = definition_occurrences_by_symbol
                .get(symbol.symbol.as_str())
                .copied()
            else {
                omitted_definition_count += 1;
                continue;
            };
            let symbol_id = ProviderSymbolId::parse(&symbol.symbol)
                .map_err(|error| format!("provider emitted invalid symbol ID: {error}"))?;
            let span = match coordinates.span(&occurrence.range, input.provider.protocol) {
                Ok(span) => span,
                Err(_) => {
                    omitted_definition_count += 1;
                    continue;
                }
            };
            // Zero-width document sentinels are not source definitions and
            // cannot carry verifiable definition evidence.
            if span.start.byte_offset == span.end.byte_offset {
                omitted_definition_count += 1;
                ignored_provider_definition_ids.insert(symbol_id);
                continue;
            }
            let fact_evidence = FactEvidence::new(
                EvidenceKind::SourceDefinition,
                evidence_producer(input.provider, "provider-definition"),
                EvidenceLocation::Source { span: span.clone() },
                None,
            )
            .map_err(|error| format!("cannot build definition evidence: {error}"))?;
            let evidence_id = fact_evidence.id.clone();
            evidence.insert(evidence_id.clone(), fact_evidence);
            let parent = symbol
                .enclosing_symbol
                .as_deref()
                .map(ProviderSymbolId::parse)
                .transpose()
                .map_err(|error| format!("provider emitted invalid parent symbol ID: {error}"))?;
            definitions
                .entry(symbol_id.clone())
                .or_insert_with(|| DefinitionDraft {
                    symbol_id: symbol_id.clone(),
                    native_kind: symbol.kind.clone(),
                    canonical_kind_hint: kind,
                    qualified_name: symbol.symbol.clone(),
                    display_name: definition_display_name(symbol),
                    signature: normalized_optional_text(symbol.signature.as_deref()),
                    visibility: Visibility::Unknown,
                    parent_symbol_id: parent,
                    definition_evidence_id: evidence_id,
                    flags: source_flags(manifest_file.file_kind),
                    field_candidate,
                    path: path.clone(),
                    provider_range: occurrence.range.clone(),
                    syntax_match: None,
                });
            definition_spans.entry(symbol_id).or_insert(span);
        }
    }
    emit_unit_timing(
        timing_enabled,
        input.unit,
        "provider_definitions",
        phase_started,
        unit_started,
    );
    phase_started = Instant::now();

    let provider_definition_ids = definitions.keys().cloned().collect::<BTreeSet<_>>();
    let provider_definition_aliases = reconcile_definition_inventory(
        input.unit,
        input.provider.protocol,
        syntax_definitions,
        &mut definitions,
        &mut definition_audit,
    );
    reconcile_definition_drafts(input.unit.language, &mut definitions);
    record_definition_metadata(input.unit, &definitions, &mut definition_audit);
    omitted_definition_count += definition_audit.blocking_count();
    let definition_ids = definitions.keys().cloned().collect::<BTreeSet<_>>();
    let mut discarded_definition_ids = provider_definition_ids
        .difference(&definition_ids)
        .cloned()
        .collect::<BTreeSet<_>>();
    discarded_definition_ids.extend(ignored_provider_definition_ids);
    let retained_definition_evidence = definitions
        .values()
        .map(|draft| draft.definition_evidence_id.clone())
        .collect::<BTreeSet<_>>();
    evidence.retain(|id, _| retained_definition_evidence.contains(id));
    let mut definition_records = definitions
        .values()
        .cloned()
        .map(|draft| IrDefinition {
            unit_id: input.unit.id.clone(),
            symbol_id: draft.symbol_id,
            native_kind: draft.native_kind,
            canonical_kind_hint: draft.canonical_kind_hint,
            qualified_name: draft.qualified_name,
            display_name: draft.display_name,
            signature: draft.signature,
            visibility: draft.visibility,
            parent_symbol_id: draft
                .parent_symbol_id
                .filter(|parent| definition_ids.contains(parent)),
            definition_evidence_id: draft.definition_evidence_id,
            flags: draft.flags,
        })
        .collect::<Vec<_>>();
    definition_records.sort_by(|left, right| left.symbol_id.cmp(&right.symbol_id));
    emit_unit_timing(
        timing_enabled,
        input.unit,
        "definition_reconciliation",
        phase_started,
        unit_started,
    );

    Ok(ReconciledUnitDefinitions {
        unit_documents,
        document_paths,
        evidence,
        definitions,
        definition_spans,
        provider_definition_aliases,
        discarded_definition_ids,
        omitted_definition_count,
        definition_records,
        definition_audit,
    })
}

fn emit_unit_header_and_coverage(
    input: &UnitAdapterInput<'_>,
    sink: &mut dyn LanguageIrSink,
) -> Result<(BTreeSet<RepositoryPath>, Vec<FileCoverageRecord>), String> {
    sink.push(LanguageIrRecord::Header(Box::new(LanguageIrHeader {
        schema: ContractSchema::LanguageIrV2,
        snapshot_id: input.snapshot_id.clone(),
        source_manifest_digest: input.manifest_digest,
        unit: input.unit.clone(),
        provider: input.provider.clone(),
        execution_context: input.execution_context.clone(),
    })))?;

    let assigned = input
        .assigned_files
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let provider_resource_gap = provider_resource_gap(input.diagnostics);
    let mut file_records = Vec::with_capacity(assigned.len());
    for path in &assigned {
        let manifest_file =
            input.manifest_files.get(path).copied().ok_or_else(|| {
                format!("analysis unit file missing from source manifest: {path}")
            })?;
        if manifest_file.state != SourceEntryState::Included
            || !manifest_file.languages.contains(&input.unit.language)
        {
            return Err(format!(
                "analysis unit owns an ineligible source file: {}",
                path.as_str()
            ));
        }
        let coverage = provider_file_coverage(input.coverage, input.unit.language, path);
        let (state, gap_codes) = file_coverage_state(
            coverage,
            input.language_output.status,
            provider_resource_gap,
        );
        let record = FileCoverageRecord {
            unit_id: Some(input.unit.id.clone()),
            path: path.clone(),
            language: Some(input.unit.language),
            file_kind: manifest_file.file_kind,
            state,
            byte_size: manifest_file.byte_size,
            line_count: manifest_file.line_count,
            non_blank_line_count: manifest_file.non_blank_line_count,
            content_digest: manifest_file.content_digest,
            gap_codes,
        };
        record
            .validate()
            .map_err(|error| format!("invalid Language IR file receipt for {path}: {error}"))?;
        file_records.push(record);
    }
    file_records.sort_by(|left, right| left.path.cmp(&right.path));
    for record in &file_records {
        sink.push(LanguageIrRecord::File(record.clone()))?;
    }
    Ok((assigned, file_records))
}

fn emit_unit_timing(
    enabled: bool,
    unit: &AnalysisUnit,
    phase: &str,
    phase_started: Instant,
    unit_started: Instant,
) {
    if enabled {
        eprintln!(
            "timing stage=language_ir_unit phase={phase} language={} unit={} elapsed_ms={} total_ms={}",
            unit.language.as_str(),
            unit.id.as_str(),
            phase_started.elapsed().as_millis(),
            unit_started.elapsed().as_millis()
        );
    }
}

fn record_definition_metadata(
    unit: &AnalysisUnit,
    definitions: &BTreeMap<ProviderSymbolId, DefinitionDraft>,
    audit: &mut DefinitionAudit,
) {
    let owners = definitions
        .iter()
        .map(|(id, draft)| (id, draft.display_name.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut entries = definitions
        .values()
        .map(|draft| DefinitionMetadataAuditEntry {
            unit_id: unit.id.as_str().to_string(),
            language: unit.language,
            path: draft.path.clone(),
            kind: draft.canonical_kind_hint,
            name: draft.display_name.clone(),
            owner: draft
                .parent_symbol_id
                .as_ref()
                .and_then(|parent| owners.get(parent).copied())
                .map(str::to_string),
            signature: draft.signature.clone(),
            visibility: draft.visibility,
        })
        .collect::<Vec<_>>();
    entries.sort();
    for entry in &entries {
        let callable = matches!(
            entry.kind,
            FactNodeKind::Function | FactNodeKind::Method | FactNodeKind::Constructor
        );
        audit.metadata_definition_count += 1;
        audit.callable_definition_count += u64::from(callable);
        audit.callable_signature_count += u64::from(callable && entry.signature.is_some());
        audit.known_visibility_count += u64::from(entry.visibility != Visibility::Unknown);
        audit.metadata_keys.push(definition_metadata_key(entry));
    }
    audit.metadata_keys.sort();
    audit.metadata_keys.dedup();
    audit.metadata_entries.extend(entries);
    audit.metadata_entries.sort();
    audit.metadata_entries.dedup();
}

fn definition_metadata_key(entry: &DefinitionMetadataAuditEntry) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}",
        entry.path.as_str(),
        entry.kind.as_str(),
        entry.name,
        entry.owner.as_deref().unwrap_or("-"),
        visibility_name(entry.visibility),
        entry.signature.as_deref().unwrap_or("-")
    )
}

const fn visibility_name(visibility: Visibility) -> &'static str {
    match visibility {
        Visibility::Public => "public",
        Visibility::Protected => "protected",
        Visibility::Internal => "internal",
        Visibility::Package => "package",
        Visibility::Private => "private",
        Visibility::Unknown => "unknown",
    }
}

fn record_definition_inventory_failure(
    unit: &AnalysisUnit,
    path: &RepositoryPath,
    audit: &mut DefinitionAudit,
) {
    audit.inventory_failed_file_count += 1;
    audit.failures.push(DefinitionAuditFailure {
        unit_id: unit.id.as_str().to_string(),
        language: unit.language,
        path: path.clone(),
        name: "<file>".to_string(),
        reason: "syntax-inventory-failed",
        expected_kind: None,
        provider_kind: None,
    });
}

fn collect_import_drafts(
    unit: &AnalysisUnit,
    index: &ProjectImportIndex,
    path: &RepositoryPath,
    coordinates: &SourceCoordinates,
    sites: Vec<ImportSite>,
    audit: &mut ImportAudit,
    drafts: &mut Vec<ImportDraft>,
) {
    for site in sites {
        let capability = import_capability(site.relation);
        audit.capability_mut(capability).eligible_count += 1;
        let mut entry = ImportAuditEntry {
            unit_id: unit.id.as_str().to_string(),
            language: unit.language,
            path: path.clone(),
            capability,
            specifier: Some(site.specifier.clone()),
            utf8_range: site.utf8_range.clone(),
            utf16_range: site.utf16_range.clone(),
            outcome: ImportAuditOutcome::Unresolved,
            target: None,
            resolution_method: None,
            gap_code: None,
            candidate_count: None,
        };
        match index.resolve(path, &site) {
            ImportResolution::Internal { target, method } => {
                match coordinates.span(&site.utf8_range, ProviderProtocol::CompilerApi) {
                    Ok(span) => {
                        let measured = audit.capability_mut(capability);
                        measured.covered_count += 1;
                        measured.internal_relation_count += 1;
                        entry.outcome = ImportAuditOutcome::Internal;
                        entry.target = Some(endpoint_key(&target));
                        entry.resolution_method = Some(method);
                        drafts.push(ImportDraft {
                            source: path.clone(),
                            target,
                            relation: import_relation_kind(site.relation),
                            resolution: method,
                            span,
                        });
                    }
                    Err(_) => {
                        let measured = audit.capability_mut(capability);
                        measured.invalid_evidence_count += 1;
                        measured.gap_codes.insert(GapCode::UnresolvedTarget);
                        entry.outcome = ImportAuditOutcome::InvalidEvidence;
                        entry.gap_code = Some(GapCode::UnresolvedTarget);
                    }
                }
            }
            ImportResolution::KnownExternal => {
                let measured = audit.capability_mut(capability);
                measured.covered_count += 1;
                measured.known_external_count += 1;
                entry.outcome = ImportAuditOutcome::KnownExternal;
            }
            ImportResolution::Unresolved { gap } => {
                let measured = audit.capability_mut(capability);
                measured.unresolved_count += 1;
                measured.gap_codes.insert(gap);
                entry.outcome = ImportAuditOutcome::Unresolved;
                entry.gap_code = Some(gap);
            }
            ImportResolution::Ambiguous { candidate_count } => {
                let measured = audit.capability_mut(capability);
                measured.ambiguous_count += 1;
                measured.gap_codes.insert(GapCode::UnresolvedTarget);
                entry.outcome = ImportAuditOutcome::Ambiguous;
                entry.gap_code = Some(GapCode::UnresolvedTarget);
                entry.candidate_count = Some(candidate_count);
            }
        }
        audit.site_keys.push(import_audit_key(&entry));
        audit.entries.push(entry);
    }
}

fn record_import_file_failure(
    unit: &AnalysisUnit,
    path: &RepositoryPath,
    outcome: ImportAuditOutcome,
    gap: GapCode,
    audit: &mut ImportAudit,
) {
    let capabilities = audit.capabilities.keys().copied().collect::<Vec<_>>();
    for capability in capabilities {
        let measured = audit.capability_mut(capability);
        match outcome {
            ImportAuditOutcome::MetadataUnavailable => {
                measured.metadata_unavailable_files.insert(path.clone());
            }
            _ => {
                measured.inventory_failed_files.insert(path.clone());
            }
        }
        measured.gap_codes.insert(gap);
        let entry = ImportAuditEntry {
            unit_id: unit.id.as_str().to_string(),
            language: unit.language,
            path: path.clone(),
            capability,
            specifier: None,
            utf8_range: Vec::new(),
            utf16_range: Vec::new(),
            outcome,
            target: None,
            resolution_method: None,
            gap_code: Some(gap),
            candidate_count: None,
        };
        audit.site_keys.push(import_audit_key(&entry));
        audit.entries.push(entry);
    }
}

fn append_import_gaps(unit: &AnalysisUnit, audit: &ImportAudit, gaps: &mut Vec<AnalysisGap>) {
    for (capability, measured) in &audit.capabilities {
        for code in &measured.gap_codes {
            gaps.push(AnalysisGap {
                code: *code,
                scope: AnalysisScope::AnalysisUnit {
                    unit_id: unit.id.clone(),
                },
                capability: Some(*capability),
                evidence_ids: Vec::new(),
                message: format!(
                    "Independent {} site resolution was incomplete; no guessed internal relation was emitted",
                    capability.as_str()
                ),
            });
        }
    }
}

fn import_language_summary(
    language: ProgrammingLanguage,
    audit: &ImportAudit,
) -> ImportLanguageSummary {
    let imports = audit
        .capability(AnalysisCapability::Imports)
        .cloned()
        .unwrap_or_default();
    let exports = audit
        .capability(AnalysisCapability::Exports)
        .cloned()
        .unwrap_or_default();
    ImportLanguageSummary {
        language,
        site_set_digest: import_site_set_digest(&audit.site_keys),
        eligible_site_count: imports.eligible_count + exports.eligible_count,
        import_site_count: imports.eligible_count,
        export_site_count: exports.eligible_count,
        internal_relation_count: imports.internal_relation_count + exports.internal_relation_count,
        known_external_count: imports.known_external_count + exports.known_external_count,
        unresolved_count: imports.unresolved_count + exports.unresolved_count,
        ambiguous_count: imports.ambiguous_count + exports.ambiguous_count,
        invalid_evidence_count: imports.invalid_evidence_count + exports.invalid_evidence_count,
        inventory_failed_file_count: imports
            .inventory_failed_files
            .union(&exports.inventory_failed_files)
            .count() as u64,
        metadata_unavailable_file_count: imports
            .metadata_unavailable_files
            .union(&exports.metadata_unavailable_files)
            .count() as u64,
    }
}

fn import_site_set_digest(keys: &[String]) -> Sha256Digest {
    let mut bytes = b"codebase-workspace.import-site-set.v1\0".to_vec();
    let mut keys = keys.to_vec();
    keys.sort();
    keys.dedup();
    for key in keys {
        append_digest_component(&mut bytes, key.as_bytes());
    }
    Sha256Digest::of_bytes(&bytes)
}

fn type_relation_language_summary(
    language: ProgrammingLanguage,
    audit: &TypeRelationAudit,
) -> TypeRelationLanguageSummary {
    let count = |kind| {
        audit
            .entries
            .iter()
            .filter(|entry| entry.kind == kind)
            .count() as u64
    };
    TypeRelationLanguageSummary {
        language,
        relation_set_digest: type_relation_set_digest(&audit.relation_keys),
        relation_count: audit.entries.len() as u64,
        extends_count: count(LanguageRelationKind::Extends),
        implements_count: count(LanguageRelationKind::Implements),
        mixes_in_count: count(LanguageRelationKind::MixesIn),
        overrides_count: count(LanguageRelationKind::Overrides),
        uses_type_count: count(LanguageRelationKind::UsesType),
        explicit_hierarchy_site_count: audit.explicit_site_keys.len() as u64,
        matched_explicit_hierarchy_site_count: audit.matched_explicit_site_keys.len() as u64,
        unmatched_explicit_hierarchy_site_count: audit
            .explicit_site_keys
            .difference(&audit.matched_explicit_site_keys)
            .count() as u64,
        inventory_failed_file_count: audit.inventory_failed_files.len() as u64,
    }
}

fn type_relation_set_digest(keys: &[String]) -> Sha256Digest {
    let mut bytes = b"codebase-workspace.type-relation-set.v1\0".to_vec();
    let mut keys = keys.to_vec();
    keys.sort();
    keys.dedup();
    for key in keys {
        append_digest_component(&mut bytes, key.as_bytes());
    }
    Sha256Digest::of_bytes(&bytes)
}

fn type_relation_site_key(path: &RepositoryPath, site: &SyntaxTypeRelationSite) -> String {
    let intent = match site.intent {
        TypeRelationIntent::Exact(kind) => relation_kind_rank(kind),
        TypeRelationIntent::CSharpBase => u8::MAX,
    };
    format!(
        "{}\t{}\t{:?}\t{}\t{:?}\t{}",
        path.as_str(),
        site.source_name,
        site.source_utf8_range,
        site.target_name,
        site.target_utf8_range,
        intent,
    )
}

fn type_relation_endpoint(
    endpoint: &IrEndpoint,
    definitions: &BTreeMap<ProviderSymbolId, DefinitionDraft>,
) -> (String, String) {
    match endpoint {
        IrEndpoint::NativeSymbol { symbol_id } => (
            symbol_id.as_str().to_string(),
            definitions
                .get(symbol_id)
                .map(|definition| definition.display_name.clone())
                .unwrap_or_else(|| short_symbol_name(symbol_id.as_str()).to_string()),
        ),
        IrEndpoint::File { path } => (endpoint_key(endpoint), path.as_str().to_string()),
        IrEndpoint::Structure { qualified_name, .. } => {
            (endpoint_key(endpoint), qualified_name.clone())
        }
    }
}

fn type_relation_audit_key(entry: &TypeRelationAuditEntry) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}:{}-{}:{}",
        entry.unit_id,
        entry.language.as_str(),
        entry.path.as_str(),
        relation_kind_rank(entry.kind),
        entry.source_symbol,
        entry.target_symbol,
        entry.native_relation,
        entry.start_line,
        entry.start_utf8_column,
        entry.end_line,
        entry.end_utf8_column,
    )
}

fn canonicalize_type_relation_entries(entries: &mut Vec<TypeRelationAuditEntry>) {
    entries.sort_by(|left, right| {
        (
            &left.unit_id,
            left.language,
            &left.path,
            relation_kind_rank(left.kind),
            &left.source_symbol,
            &left.target_symbol,
            &left.native_relation,
            left.start_line,
            left.start_utf8_column,
            left.end_line,
            left.end_utf8_column,
        )
            .cmp(&(
                &right.unit_id,
                right.language,
                &right.path,
                relation_kind_rank(right.kind),
                &right.source_symbol,
                &right.target_symbol,
                &right.native_relation,
                right.start_line,
                right.start_utf8_column,
                right.end_line,
                right.end_utf8_column,
            ))
    });
    entries.dedup();
}

fn canonicalize_import_entries(entries: &mut Vec<ImportAuditEntry>) {
    entries.sort_by(|left, right| {
        (
            &left.unit_id,
            left.language,
            &left.path,
            left.capability,
            &left.utf8_range,
            left.specifier.as_deref(),
            left.outcome,
            left.target.as_deref(),
            resolution_method_rank(left.resolution_method),
            left.gap_code,
            left.candidate_count,
        )
            .cmp(&(
                &right.unit_id,
                right.language,
                &right.path,
                right.capability,
                &right.utf8_range,
                right.specifier.as_deref(),
                right.outcome,
                right.target.as_deref(),
                resolution_method_rank(right.resolution_method),
                right.gap_code,
                right.candidate_count,
            ))
    });
    entries.dedup();
}

fn import_audit_key(entry: &ImportAuditEntry) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{:?}\t{}\t{}\t{}\t{}\t{}\t{}",
        entry.unit_id,
        entry.language.as_str(),
        entry.path.as_str(),
        entry.capability.as_str(),
        entry.utf8_range,
        entry.specifier.as_deref().unwrap_or("-"),
        import_outcome_rank(entry.outcome),
        entry.target.as_deref().unwrap_or("-"),
        resolution_method_rank(entry.resolution_method),
        entry.gap_code.map(GapCode::as_str).unwrap_or("-"),
        entry.candidate_count.unwrap_or(0),
    )
}

const fn resolution_method_rank(method: Option<ResolutionMethod>) -> u8 {
    match method {
        Some(ResolutionMethod::Compiler) => 0,
        Some(ResolutionMethod::Provider) => 1,
        Some(ResolutionMethod::ProjectModel) => 2,
        Some(ResolutionMethod::SyntaxExact) => 3,
        Some(ResolutionMethod::FrameworkAdapter) => 4,
        Some(ResolutionMethod::DatabaseReconciliation) => 5,
        Some(ResolutionMethod::Manifest) => 6,
        None => 7,
    }
}

const fn import_outcome_rank(outcome: ImportAuditOutcome) -> u8 {
    match outcome {
        ImportAuditOutcome::Internal => 0,
        ImportAuditOutcome::KnownExternal => 1,
        ImportAuditOutcome::Unresolved => 2,
        ImportAuditOutcome::Ambiguous => 3,
        ImportAuditOutcome::InvalidEvidence => 4,
        ImportAuditOutcome::InventoryFailed => 5,
        ImportAuditOutcome::MetadataUnavailable => 6,
    }
}

const fn import_capability(relation: ImportRelation) -> AnalysisCapability {
    match relation {
        ImportRelation::Imports => AnalysisCapability::Imports,
        ImportRelation::Exports => AnalysisCapability::Exports,
    }
}

const fn import_relation_kind(relation: ImportRelation) -> LanguageRelationKind {
    match relation {
        ImportRelation::Imports => LanguageRelationKind::Imports,
        ImportRelation::Exports => LanguageRelationKind::Exports,
    }
}

const fn supports_exports(language: ProgrammingLanguage) -> bool {
    matches!(
        language,
        ProgrammingLanguage::TypeScript
            | ProgrammingLanguage::JavaScript
            | ProgrammingLanguage::Dart
    )
}

#[derive(Clone)]
struct DefinitionDraft {
    symbol_id: ProviderSymbolId,
    native_kind: String,
    canonical_kind_hint: FactNodeKind,
    qualified_name: String,
    display_name: String,
    signature: Option<String>,
    visibility: Visibility,
    parent_symbol_id: Option<ProviderSymbolId>,
    definition_evidence_id: EvidenceId,
    flags: SourceFlags,
    field_candidate: bool,
    path: RepositoryPath,
    provider_range: Vec<i32>,
    syntax_match: Option<usize>,
}

fn reconcile_definition_inventory(
    unit: &AnalysisUnit,
    protocol: ProviderProtocol,
    syntax_by_path: &BTreeMap<RepositoryPath, Vec<SyntaxDefinition>>,
    definitions: &mut BTreeMap<ProviderSymbolId, DefinitionDraft>,
    audit: &mut DefinitionAudit,
) -> BTreeMap<ProviderSymbolId, ProviderSymbolId> {
    let mut matched_syntax = BTreeMap::<(RepositoryPath, usize), ProviderSymbolId>::new();
    let mut provider_aliases = BTreeMap::<ProviderSymbolId, ProviderSymbolId>::new();

    // The source denominator is keyed by file, so index provider definitions by
    // file once as well. Scanning the complete provider definition map for each
    // source file made reconciliation O(files * definitions): a 9k-file Java
    // workspace repeated tens of millions of ordered-map visits before doing
    // any real matching. Keep the former deterministic length/id order inside
    // each bucket so this is a performance-only change.
    let mut provider_ids_by_path = BTreeMap::<RepositoryPath, Vec<ProviderSymbolId>>::new();
    for (id, draft) in definitions.iter() {
        provider_ids_by_path
            .entry(draft.path.clone())
            .or_default()
            .push(id.clone());
    }
    for provider_ids in provider_ids_by_path.values_mut() {
        provider_ids.sort_by(|left, right| {
            left.as_str()
                .len()
                .cmp(&right.as_str().len())
                .then_with(|| left.cmp(right))
        });
    }

    for (path, syntax) in syntax_by_path {
        let provider_ids = provider_ids_by_path.get(path).cloned().unwrap_or_default();
        let mut used = BTreeSet::<usize>::new();

        for id in provider_ids {
            let Some(draft) = definitions.get(&id) else {
                continue;
            };
            if draft.canonical_kind_hint == FactNodeKind::Namespace {
                continue;
            }
            let selected = select_syntax_definition(draft, syntax, &used, protocol);
            let Some(index) = selected else {
                if let Some(primary) = select_provider_definition_alias(
                    draft,
                    syntax,
                    &used,
                    path,
                    &matched_syntax,
                    definitions,
                    protocol,
                ) {
                    provider_aliases.insert(id.clone(), primary);
                    audit.provider_definition_alias_count += 1;
                    continue;
                }
                if !draft.field_candidate {
                    audit.extra_provider_definition_count += 1;
                    audit.failures.push(DefinitionAuditFailure {
                        unit_id: unit.id.as_str().to_string(),
                        language: unit.language,
                        path: path.clone(),
                        name: draft.display_name.clone(),
                        reason: "provider-definition-without-source-declaration",
                        expected_kind: None,
                        provider_kind: Some(draft.canonical_kind_hint),
                    });
                }
                continue;
            };
            used.insert(index);
            matched_syntax.insert((path.clone(), index), id.clone());
            audit.matched_definition_count += 1;
            if let Some(draft) = definitions.get_mut(&id) {
                draft.syntax_match = Some(index);
                // The source token is the authoritative human-readable name.
                // SCIP parameter and constructor descriptors may be encoded as
                // `(value)` or `<constructor>`, while LSPs may use `.ctor`.
                // Keep the provider symbol as identity, but never leak those
                // protocol spellings (or an empty name) into the product IR.
                draft.display_name = syntax[index].name.clone();
                // A source declaration is the uniform authority for callable
                // headers and language-defined accessibility. Provider
                // signatures remain a fallback for non-callable symbols.
                if syntax[index].signature.is_some() {
                    draft.signature = syntax[index].signature.clone();
                }
                draft.visibility = syntax[index].visibility;
                // Syntax reconciliation has now decided whether a provider
                // Variable is a real field (or another explicit declaration).
                // The old provider-only field heuristic must not delete the
                // reconciled definition later.
                draft.field_candidate = false;
                if draft.canonical_kind_hint != syntax[index].kind {
                    draft.canonical_kind_hint = syntax[index].kind;
                    audit.kind_refinement_count += 1;
                }
            }
        }

        for (index, definition) in syntax.iter().enumerate() {
            if used.contains(&index) {
                continue;
            }
            audit.missing_syntax_definition_count += 1;
            audit.failures.push(DefinitionAuditFailure {
                unit_id: unit.id.as_str().to_string(),
                language: unit.language,
                path: path.clone(),
                name: definition.name.clone(),
                reason: "source-declaration-missing-from-provider",
                expected_kind: Some(definition.kind),
                provider_kind: None,
            });
        }
    }

    let provider_kinds = definitions
        .iter()
        .map(|(id, draft)| (id.clone(), draft.canonical_kind_hint))
        .collect::<BTreeMap<_, _>>();
    let matched = definitions
        .iter()
        .filter_map(|(id, draft)| {
            draft
                .syntax_match
                .map(|index| (id.clone(), draft.path.clone(), index))
        })
        .collect::<Vec<_>>();
    let mut unresolved_definition_ids = BTreeSet::<ProviderSymbolId>::new();
    for (id, path, index) in matched {
        let Some(candidate) = syntax_by_path
            .get(&path)
            .and_then(|definitions| definitions.get(index))
        else {
            continue;
        };
        let expected_parent = candidate
            .parent_name_range(protocol)
            .and_then(|parent_range| {
                syntax_by_path.get(&path).and_then(|definitions| {
                    definitions
                        .iter()
                        .position(|definition| definition.name_range(protocol) == parent_range)
                })
            });
        let expected_parent =
            expected_parent.and_then(|parent| matched_syntax.get(&(path.clone(), parent)).cloned());
        let current_parent = definitions
            .get(&id)
            .and_then(|draft| draft.parent_symbol_id.clone());
        if candidate.parent_name_range(protocol).is_some() && expected_parent.is_none() {
            audit.unresolved_owner_count += 1;
            audit.matched_definition_count = audit.matched_definition_count.saturating_sub(1);
            unresolved_definition_ids.insert(id.clone());
            audit.failures.push(DefinitionAuditFailure {
                unit_id: unit.id.as_str().to_string(),
                language: unit.language,
                path: path.clone(),
                name: candidate.name.clone(),
                reason: "source-owner-missing-from-provider",
                expected_kind: Some(candidate.kind),
                provider_kind: definitions.get(&id).map(|draft| draft.canonical_kind_hint),
            });
            continue;
        }
        if candidate.parent_name_range(protocol).is_some() {
            audit.resolved_owner_count += 1;
        }
        let normalized_parent = match expected_parent {
            Some(parent) => Some(parent),
            None => current_parent
                .clone()
                .filter(|parent| provider_kinds.get(parent) != Some(&FactNodeKind::Namespace)),
        };
        if normalized_parent != current_parent {
            audit.owner_repair_count += 1;
            if let Some(draft) = definitions.get_mut(&id) {
                draft.parent_symbol_id = normalized_parent;
            }
        }
    }

    definitions.retain(|id, draft| {
        !unresolved_definition_ids.contains(id)
            && (draft.syntax_match.is_some() || !syntax_by_path.contains_key(&draft.path))
    });
    audit.failures.sort();
    audit.failures.dedup();
    provider_aliases
}

fn select_provider_definition_alias(
    draft: &DefinitionDraft,
    syntax: &[SyntaxDefinition],
    used: &BTreeSet<usize>,
    path: &RepositoryPath,
    matched_syntax: &BTreeMap<(RepositoryPath, usize), ProviderSymbolId>,
    definitions: &BTreeMap<ProviderSymbolId, DefinitionDraft>,
    protocol: ProviderProtocol,
) -> Option<ProviderSymbolId> {
    let aliases = used
        .iter()
        .copied()
        .filter(|index| ranges_equal(&draft.provider_range, syntax[*index].name_range(protocol)))
        .filter_map(|index| {
            let primary = matched_syntax.get(&(path.clone(), index))?;
            let primary_kind = definitions.get(primary)?.canonical_kind_hint;
            definition_kinds_can_alias(primary_kind, draft.canonical_kind_hint, syntax[index].kind)
                .then(|| primary.clone())
        })
        .collect::<Vec<_>>();
    (aliases.len() == 1).then(|| aliases[0].clone())
}

fn definition_kinds_can_alias(
    primary: FactNodeKind,
    duplicate: FactNodeKind,
    syntax: FactNodeKind,
) -> bool {
    definition_kind_matches_source(primary, syntax)
        && definition_kind_matches_source(duplicate, syntax)
}

fn definition_kind_matches_source(provider: FactNodeKind, syntax: FactNodeKind) -> bool {
    provider == syntax
        || provider == FactNodeKind::Type && is_type_owner_kind(syntax)
        || provider == FactNodeKind::Method && syntax == FactNodeKind::Constructor
}

fn select_syntax_definition(
    draft: &DefinitionDraft,
    syntax: &[SyntaxDefinition],
    used: &BTreeSet<usize>,
    protocol: ProviderProtocol,
) -> Option<usize> {
    let positional = syntax
        .iter()
        .enumerate()
        .filter(|(index, candidate)| {
            !used.contains(index)
                && candidate.matches_provider_range(&draft.provider_range, protocol)
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let exact_range = positional
        .iter()
        .copied()
        .filter(|index| ranges_equal(&draft.provider_range, syntax[*index].name_range(protocol)))
        .collect::<Vec<_>>();
    if exact_range.len() == 1 {
        return exact_range.first().copied();
    }
    if let Some(point) = provider_symbol_source_point(&draft.symbol_id) {
        let exact = positional
            .iter()
            .copied()
            .filter(|index| range_start(syntax[*index].name_range(protocol)) == Some(point))
            .collect::<Vec<_>>();
        if exact.len() == 1 {
            return exact.first().copied();
        }
    }
    let provider_name = definition_base_name(&draft.display_name);
    let same_name = positional
        .iter()
        .copied()
        .filter(|index| syntax[*index].name == provider_name)
        .collect::<Vec<_>>();
    (same_name.len() == 1).then(|| same_name[0])
}

fn provider_symbol_source_point(symbol: &ProviderSymbolId) -> Option<(i32, i32)> {
    let value = symbol.as_str();
    if !value.starts_with("lsp ") {
        return None;
    }
    let (_, location) = value.rsplit_once('@')?;
    let (line, column) = location.split_once(':')?;
    Some((line.parse().ok()?, column.parse().ok()?))
}

fn range_start(range: &[i32]) -> Option<(i32, i32)> {
    match range {
        [line, start, ..] => Some((*line, *start)),
        _ => None,
    }
}

fn ranges_equal(left: &[i32], right: &[i32]) -> bool {
    canonical_range_bounds(left) == canonical_range_bounds(right)
}

fn canonical_range_bounds(range: &[i32]) -> Option<((i32, i32), (i32, i32))> {
    match range {
        [line, start, end] => Some(((*line, *start), (*line, *end))),
        [start_line, start_column, end_line, end_column, ..] => {
            Some(((*start_line, *start_column), (*end_line, *end_column)))
        }
        _ => None,
    }
}

fn canonical_definition_kind(native: &str) -> Option<FactNodeKind> {
    let compact = native
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    match compact.as_str() {
        "namespace" | "package" | "module" => Some(FactNodeKind::Namespace),
        "type" => Some(FactNodeKind::Type),
        "class" => Some(FactNodeKind::Class),
        "interface" => Some(FactNodeKind::Interface),
        "trait" | "mixin" => Some(FactNodeKind::Trait),
        "struct" | "union" => Some(FactNodeKind::Struct),
        "enum" => Some(FactNodeKind::Enum),
        "typealias" | "typedef" => Some(FactNodeKind::TypeAlias),
        "function" | "operator" => Some(FactNodeKind::Function),
        "method" => Some(FactNodeKind::Method),
        "constructor" => Some(FactNodeKind::Constructor),
        "constant" => Some(FactNodeKind::Constant),
        "field" | "property" => Some(FactNodeKind::Field),
        _ => None,
    }
}

fn is_variable_definition_kind(native: &str) -> bool {
    native
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .eq("variable".chars())
}

fn reconcile_definition_drafts(
    language: ProgrammingLanguage,
    definitions: &mut BTreeMap<ProviderSymbolId, DefinitionDraft>,
) {
    // A provider can occasionally report a declaration as its own enclosing
    // symbol (clangd does this for some macro-heavy C declarations). Such an
    // ownership edge is structurally impossible. Preserve the declaration and
    // its evidence, but abstain from emitting the invalid containment claim.
    for (id, draft) in definitions.iter_mut() {
        if draft.parent_symbol_id.as_ref() == Some(id) {
            draft.parent_symbol_id = None;
        }
    }

    let owners = definitions
        .iter()
        .map(|(id, draft)| {
            (
                id.clone(),
                (draft.canonical_kind_hint, draft.display_name.clone()),
            )
        })
        .collect::<BTreeMap<_, _>>();

    definitions.retain(|_, draft| {
        if !draft.field_candidate {
            return true;
        }
        draft
            .parent_symbol_id
            .as_ref()
            .and_then(|parent| owners.get(parent))
            .is_some_and(|(kind, _)| is_type_owner_kind(*kind))
    });

    for draft in definitions.values_mut() {
        let parent = draft
            .parent_symbol_id
            .as_ref()
            .and_then(|parent| owners.get(parent));
        if draft.field_candidate {
            draft.canonical_kind_hint = FactNodeKind::Field;
            continue;
        }
        if draft.canonical_kind_hint != FactNodeKind::Method {
            continue;
        }
        if parent.is_some_and(|(kind, _)| *kind == FactNodeKind::Namespace) {
            draft.canonical_kind_hint = FactNodeKind::Function;
            continue;
        }
        if definition_is_constructor(language, draft, parent) {
            draft.canonical_kind_hint = FactNodeKind::Constructor;
        }
    }
}

fn is_type_owner_kind(kind: FactNodeKind) -> bool {
    matches!(
        kind,
        FactNodeKind::Type
            | FactNodeKind::Class
            | FactNodeKind::Interface
            | FactNodeKind::Trait
            | FactNodeKind::Struct
            | FactNodeKind::Enum
    )
}

fn definition_is_constructor(
    language: ProgrammingLanguage,
    draft: &DefinitionDraft,
    parent: Option<&(FactNodeKind, String)>,
) -> bool {
    let child = definition_base_name(&draft.display_name);
    let Some((parent_kind, parent_name)) = parent else {
        return false;
    };
    if !is_type_owner_kind(*parent_kind) {
        return false;
    }
    if matches!(
        child.as_str(),
        "constructor" | "__init__" | ".ctor" | "<init>"
    ) || draft.qualified_name.contains("<constructor>")
        || draft.qualified_name.contains("`.ctor`")
        || draft.qualified_name.contains("<init>")
    {
        return true;
    }
    if child != definition_base_name(parent_name) {
        return false;
    }
    match language {
        ProgrammingLanguage::Dart => true,
        ProgrammingLanguage::Java => draft.signature.is_none(),
        _ => false,
    }
}

fn definition_base_name(value: &str) -> String {
    let value = value
        .split('(')
        .next()
        .unwrap_or(value)
        .split('<')
        .next()
        .unwrap_or(value)
        .trim();
    value
        .rsplit(['#', '.', ':', '/', ' '])
        .next()
        .unwrap_or(value)
        .trim_matches('`')
        .to_string()
}

fn definition_display_name(symbol: &SymbolOutput) -> String {
    symbol
        .display_name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| short_symbol_name(&symbol.symbol).to_string())
}

fn normalized_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn short_symbol_name(symbol: &str) -> &str {
    // Only LSP fallback identities use a trailing `@line:column` suffix.
    // SCIP package identities may legitimately contain `@` (notably scoped
    // npm packages such as `@nestjs/core`), so splitting at the first `@`
    // corrupts otherwise exact provider definitions into the name `npm`.
    let symbol = strip_lsp_location_suffix(symbol);
    let property_descriptor = symbol.ends_with(':');
    let symbol = symbol.trim_end_matches(['.', ':', '/', '#']);
    let symbol = symbol.rsplit(['#', '.', ':', '/']).next().unwrap_or(symbol);
    // clangd gives unnamed declarations stable labels such as
    // `(anonymous enum)` and `(anonymous namespace)`. These are displayable
    // structural identities, not callable signatures. Splitting every value
    // at the first `(` turned them into an empty display name and rejected the
    // whole otherwise-valid Language IR stream.
    if symbol.starts_with('(') && symbol.ends_with(')') {
        return symbol;
    }
    let symbol = symbol.split('(').next().unwrap_or(symbol);
    let symbol = symbol.split_whitespace().last().unwrap_or(symbol);
    if property_descriptor {
        symbol.trim_end_matches(char::is_numeric)
    } else {
        symbol
    }
}

fn strip_lsp_location_suffix(symbol: &str) -> &str {
    let Some((identity, suffix)) = symbol.rsplit_once('@') else {
        return symbol;
    };
    let Some((line, column)) = suffix.split_once(':') else {
        return symbol;
    };
    if !line.is_empty()
        && !column.is_empty()
        && line.bytes().all(|value| value.is_ascii_digit())
        && column.bytes().all(|value| value.is_ascii_digit())
    {
        identity
    } else {
        symbol
    }
}

fn source_flags(kind: SourceFileKind) -> SourceFlags {
    SourceFlags {
        test: kind == SourceFileKind::Test,
        generated: kind == SourceFileKind::Generated,
        vendor: kind == SourceFileKind::Vendor,
        external: false,
    }
}

#[derive(Clone, Debug)]
struct ProviderRelationMapping {
    kind: LanguageRelationKind,
    capability: AnalysisCapability,
    evidence_kind: EvidenceKind,
    reverse_endpoints: bool,
    evidence_range: Option<Vec<i32>>,
    matched_explicit_site_key: Option<String>,
}

struct ProviderRelationClassificationContext<'a> {
    language: ProgrammingLanguage,
    protocol: ProviderProtocol,
    definitions: &'a BTreeMap<ProviderSymbolId, DefinitionDraft>,
    syntax_definitions: &'a BTreeMap<RepositoryPath, Vec<SyntaxDefinition>>,
    syntax_type_relations: &'a BTreeMap<RepositoryPath, Vec<SyntaxTypeRelationSite>>,
    syntax_type_uses: &'a BTreeMap<RepositoryPath, Vec<SyntaxTypeUseSite>>,
    hierarchy_occurrence_ranges: &'a BTreeMap<String, BTreeMap<ProviderSymbolId, Vec<Vec<i32>>>>,
}

fn provider_relation_capability(native: &str) -> Option<AnalysisCapability> {
    match native {
        // Imports/exports use the independent syntax-site denominator and
        // exact project resolver. Provider relations are deliberately not an
        // authority here because they commonly omit top-level import sites.
        "IMPORTS" => None,
        // Generic provider references are an internal resolution input, not a
        // product relation. Persist only a typed, visualization-relevant
        // relation such as calls, uses_type, tests, reads, or writes.
        "REFERENCES" | "SYMBOL_REFERENCE" => None,
        "CALLS" | "CONSTRUCTS" => Some(AnalysisCapability::DirectCalls),
        "IMPLEMENTATION"
        | "DEFINITION_OVERRIDE"
        | "DEFINITION"
        | "TYPE_DEFINITION"
        | "USES_TYPE" => Some(AnalysisCapability::TypeRelations),
        _ => None,
    }
}

fn classify_provider_relation(
    relation: &RelationOutput,
    source: &IrEndpoint,
    target: &IrEndpoint,
    context: &ProviderRelationClassificationContext<'_>,
) -> Option<ProviderRelationMapping> {
    if let Some((kind, capability, evidence_kind)) = basic_relation_mapping(&relation.kind) {
        return Some(ProviderRelationMapping {
            kind,
            capability,
            evidence_kind,
            reverse_endpoints: false,
            evidence_range: None,
            matched_explicit_site_key: None,
        });
    }

    let (source_id, target_id) = match (source, target) {
        (
            IrEndpoint::NativeSymbol {
                symbol_id: source_id,
            },
            IrEndpoint::NativeSymbol {
                symbol_id: target_id,
            },
        ) => (source_id, target_id),
        _ => return None,
    };
    let source_definition = context.definitions.get(source_id)?;
    let target_definition = context.definitions.get(target_id)?;

    if matches!(relation.kind.as_str(), "TYPE_DEFINITION" | "USES_TYPE") {
        if !is_type_reference_target_kind(target_definition.canonical_kind_hint) {
            return None;
        }
        let site =
            explicit_type_use_site(context, relation, source_id, target_id, source_definition)?;
        return Some(type_relation_mapping(
            LanguageRelationKind::UsesType,
            Some(site.target_range(context.protocol).to_vec()),
            false,
            None,
        ));
    }

    if is_type_owner_kind(source_definition.canonical_kind_hint)
        && is_type_owner_kind(target_definition.canonical_kind_hint)
    {
        if relation.kind != "IMPLEMENTATION" {
            return None;
        }
        if context.language == ProgrammingLanguage::Go {
            return Some(type_relation_mapping(
                LanguageRelationKind::Implements,
                None,
                false,
                None,
            ));
        }
        let site = explicit_hierarchy_site(
            context,
            source_id,
            target_id,
            source_definition,
            target_definition,
        )?;
        let kind = hierarchy_kind(site.intent, source_definition, target_definition)?;
        return Some(type_relation_mapping(
            kind,
            Some(site.target_range(context.protocol).to_vec()),
            false,
            Some(type_relation_site_key(&source_definition.path, site)),
        ));
    }

    if is_override_pair(source_definition, target_definition, context.definitions) {
        return match relation.kind.as_str() {
            "IMPLEMENTATION" | "DEFINITION_OVERRIDE" => Some(type_relation_mapping(
                LanguageRelationKind::Overrides,
                None,
                false,
                None,
            )),
            // clangd reports a header declaration -> implementation pair as
            // `is_definition`. Only a cross-owner C++ method pair is an
            // override here; top-level prototype/definition pairs are not.
            "DEFINITION" if context.language == ProgrammingLanguage::Cpp => Some(
                type_relation_mapping(LanguageRelationKind::Overrides, None, true, None),
            ),
            _ => None,
        };
    }

    None
}

fn is_type_reference_target_kind(kind: FactNodeKind) -> bool {
    is_type_owner_kind(kind) || kind == FactNodeKind::TypeAlias
}

fn basic_relation_mapping(
    native: &str,
) -> Option<(LanguageRelationKind, AnalysisCapability, EvidenceKind)> {
    match native {
        "IMPORTS" => None,
        "REFERENCES" | "SYMBOL_REFERENCE" => None,
        "CALLS" => Some((
            LanguageRelationKind::Calls,
            AnalysisCapability::DirectCalls,
            EvidenceKind::CallSite,
        )),
        "CONSTRUCTS" => Some((
            LanguageRelationKind::Constructs,
            AnalysisCapability::DirectCalls,
            EvidenceKind::CallSite,
        )),
        // Hierarchy and override relationships need endpoint kinds plus the
        // independent syntax inventory, so they are classified separately.
        "IMPLEMENTATION" | "DEFINITION_OVERRIDE" | "DEFINITION" => None,
        "TYPE_DEFINITION" | "USES_TYPE" => None,
        _ => None,
    }
}

fn explicit_type_use_site<'a>(
    context: &'a ProviderRelationClassificationContext<'a>,
    relation: &RelationOutput,
    source_id: &ProviderSymbolId,
    target_id: &ProviderSymbolId,
    source: &DefinitionDraft,
) -> Option<&'a SyntaxTypeUseSite> {
    if source_id == target_id || source.path.as_str() != relation.path {
        return None;
    }
    let source_syntax = source
        .syntax_match
        .and_then(|index| context.syntax_definitions.get(&source.path)?.get(index))?;
    context
        .syntax_type_uses
        .get(&source.path)?
        .iter()
        .find(|site| {
            ranges_equal(
                site.source_name_range(context.protocol),
                source_syntax.name_range(context.protocol),
            ) && site.matches_target_range(&relation.range, context.protocol)
        })
}

fn type_relation_mapping(
    kind: LanguageRelationKind,
    evidence_range: Option<Vec<i32>>,
    reverse_endpoints: bool,
    matched_explicit_site_key: Option<String>,
) -> ProviderRelationMapping {
    ProviderRelationMapping {
        kind,
        capability: AnalysisCapability::TypeRelations,
        evidence_kind: EvidenceKind::TypeRelation,
        reverse_endpoints,
        evidence_range,
        matched_explicit_site_key,
    }
}

fn hierarchy_kind(
    intent: TypeRelationIntent,
    source: &DefinitionDraft,
    target: &DefinitionDraft,
) -> Option<LanguageRelationKind> {
    match intent {
        TypeRelationIntent::Exact(kind) => Some(kind),
        TypeRelationIntent::CSharpBase => {
            if source.canonical_kind_hint == FactNodeKind::Interface {
                return (target.canonical_kind_hint == FactNodeKind::Interface)
                    .then_some(LanguageRelationKind::Extends);
            }
            match target.canonical_kind_hint {
                FactNodeKind::Interface | FactNodeKind::Trait => {
                    Some(LanguageRelationKind::Implements)
                }
                FactNodeKind::Class | FactNodeKind::Struct => Some(LanguageRelationKind::Extends),
                _ => None,
            }
        }
    }
}

fn explicit_hierarchy_site<'a>(
    context: &'a ProviderRelationClassificationContext<'a>,
    source_id: &ProviderSymbolId,
    target_id: &ProviderSymbolId,
    source: &DefinitionDraft,
    target: &DefinitionDraft,
) -> Option<&'a SyntaxTypeRelationSite> {
    let source_syntax = source
        .syntax_match
        .and_then(|index| context.syntax_definitions.get(&source.path)?.get(index))?;
    let sites = context.syntax_type_relations.get(&source.path)?;
    let ranges_for = |symbol_id: &ProviderSymbolId| {
        context
            .hierarchy_occurrence_ranges
            .get(source.path.as_str())
            .and_then(|ranges| ranges.get(symbol_id))
            .map(|ranges| ranges.iter().map(Vec::as_slice).collect::<Vec<_>>())
            .unwrap_or_default()
    };
    let exact_source_ranges = ranges_for(source_id);
    let exact_target_ranges = ranges_for(target_id);
    let target_name = definition_base_name(&target.display_name);

    sites.iter().find(|site| {
        (ranges_equal(
            site.source_range(context.protocol),
            source_syntax.name_range(context.protocol),
        ) || exact_source_ranges
            .iter()
            .any(|range| ranges_equal(range, site.source_range(context.protocol))))
            && (exact_target_ranges
                .iter()
                .any(|range| ranges_equal(range, site.target_range(context.protocol)))
                || site.target_name == target_name)
    })
}

fn is_override_pair(
    source: &DefinitionDraft,
    target: &DefinitionDraft,
    definitions: &BTreeMap<ProviderSymbolId, DefinitionDraft>,
) -> bool {
    if source.canonical_kind_hint != FactNodeKind::Method
        || target.canonical_kind_hint != FactNodeKind::Method
    {
        return false;
    }
    let (Some(source_owner), Some(target_owner)) = (
        source.parent_symbol_id.as_ref(),
        target.parent_symbol_id.as_ref(),
    ) else {
        return false;
    };
    source_owner != target_owner
        && definitions
            .get(source_owner)
            .is_some_and(|owner| is_type_owner_kind(owner.canonical_kind_hint))
        && definitions
            .get(target_owner)
            .is_some_and(|owner| is_type_owner_kind(owner.canonical_kind_hint))
}

fn endpoint(raw: &str) -> Result<IrEndpoint, String> {
    if let Some(path) = raw.strip_prefix("file:") {
        return RepositoryPath::parse(path)
            .map(|path| IrEndpoint::File { path })
            .map_err(|error| format!("invalid provider file endpoint: {error}"));
    }
    ProviderSymbolId::parse(raw)
        .map(|symbol_id| IrEndpoint::NativeSymbol { symbol_id })
        .map_err(|error| format!("invalid provider symbol endpoint: {error}"))
}

fn remap_provider_alias(
    endpoint: IrEndpoint,
    aliases: &BTreeMap<ProviderSymbolId, ProviderSymbolId>,
) -> IrEndpoint {
    match endpoint {
        IrEndpoint::NativeSymbol { symbol_id } => IrEndpoint::NativeSymbol {
            symbol_id: aliases.get(&symbol_id).cloned().unwrap_or(symbol_id),
        },
        endpoint => endpoint,
    }
}

/// A provider may use its zero-width file sentinel as the caller for a
/// top-level callback (notably test callbacks). The sentinel is deliberately
/// not a canonical definition, but the provider's exact call-site range still
/// proves a file-scoped call. Re-anchor only that source endpoint to the exact
/// manifest file. A discarded target, or any non-call relation involving a
/// discarded endpoint, remains unresolved and is not emitted.
fn retain_relation_endpoints(
    relation: &RelationOutput,
    capability: AnalysisCapability,
    source: IrEndpoint,
    target: IrEndpoint,
    discarded_definition_ids: &BTreeSet<ProviderSymbolId>,
) -> Option<(IrEndpoint, IrEndpoint)> {
    let target_discarded = matches!(
        &target,
        IrEndpoint::NativeSymbol { symbol_id }
            if discarded_definition_ids.contains(symbol_id)
    );
    if target_discarded {
        return None;
    }
    let source_discarded = matches!(
        &source,
        IrEndpoint::NativeSymbol { symbol_id }
            if discarded_definition_ids.contains(symbol_id)
    );
    if !source_discarded {
        return Some((source, target));
    }
    if capability != AnalysisCapability::DirectCalls {
        return None;
    }
    let path = RepositoryPath::parse(&relation.path).ok()?;
    Some((IrEndpoint::File { path }, target))
}

fn relation_sort_key(relation: &IrRelation) -> (String, String, u8, String) {
    (
        endpoint_key(&relation.source),
        endpoint_key(&relation.target),
        relation_kind_rank(relation.kind),
        relation
            .evidence_ids
            .first()
            .map(ToString::to_string)
            .unwrap_or_default(),
    )
}

fn endpoint_key(endpoint: &IrEndpoint) -> String {
    match endpoint {
        IrEndpoint::NativeSymbol { symbol_id } => format!("symbol:{}", symbol_id.as_str()),
        IrEndpoint::File { path } => format!("file:{}", path.as_str()),
        IrEndpoint::Structure {
            unit_id,
            kind,
            qualified_name,
        } => format!(
            "structure:{}:{}:{}",
            unit_id.as_str(),
            kind.as_str(),
            qualified_name
        ),
    }
}

fn relation_kind_rank(kind: LanguageRelationKind) -> u8 {
    use LanguageRelationKind::*;
    // These are stable wire-order IDs, not compact enum ordinals. Rank 5 was
    // the retired generic `references` relation. Never renumber surviving
    // kinds when a relation is removed: capability-specific set digests must
    // not change because an unrelated vocabulary entry was hard-cut.
    match kind {
        Contains => 0,
        Declares => 1,
        BelongsTo => 2,
        Imports => 3,
        Exports => 4,
        Calls => 6,
        Constructs => 7,
        Extends => 8,
        Implements => 9,
        MixesIn => 10,
        Overrides => 11,
        UsesType => 12,
        Tests => 13,
    }
}

fn evidence_producer(provider: &ProviderDescriptor, strategy: &str) -> EvidenceProducer {
    EvidenceProducer {
        kind: match provider.protocol {
            ProviderProtocol::Scip => EvidenceProducerKind::Scip,
            ProviderProtocol::LanguageServerProtocol => EvidenceProducerKind::LanguageServer,
            ProviderProtocol::CompilerApi => EvidenceProducerKind::CompilerApi,
        },
        name: provider.name.clone(),
        version: provider.version.clone(),
        strategy: Some(strategy.to_string()),
    }
}

fn syntax_import_evidence_producer() -> EvidenceProducer {
    EvidenceProducer {
        kind: EvidenceProducerKind::SyntaxParser,
        name: "tree-sitter-project-import-resolver".to_string(),
        version: None,
        strategy: Some("exact-project-import".to_string()),
    }
}

fn provider_file_coverage<'a>(
    coverage: &'a [FileCoverageOutput],
    language: ProgrammingLanguage,
    path: &RepositoryPath,
) -> Option<&'a FileCoverageOutput> {
    coverage
        .iter()
        .find(|entry| entry.language == language.as_str() && entry.path == path.as_str())
}

fn file_coverage_state(
    coverage: Option<&FileCoverageOutput>,
    language_status: &str,
    provider_resource_gap: Option<GapCode>,
) -> (FileCoverageState, Vec<GapCode>) {
    let Some(coverage) = coverage else {
        return (
            FileCoverageState::Partial,
            vec![provider_resource_gap.unwrap_or(GapCode::ProviderExecutionIncomplete)],
        );
    };
    match coverage.status {
        "indexed" => (FileCoverageState::Indexed, Vec::new()),
        "excluded"
            if coverage.reason.as_deref() == Some("provider-excluded")
                && provider_resource_gap.is_some() =>
        {
            (
                FileCoverageState::Partial,
                vec![provider_resource_gap.expect("checked provider resource gap")],
            )
        }
        "excluded" => (
            FileCoverageState::Excluded,
            vec![coverage_gap(coverage.reason.as_deref())],
        ),
        _ if matches!(
            language_status,
            "missing-tool" | "indexer-failed" | "invalid-output"
        ) =>
        {
            (
                FileCoverageState::Failed,
                vec![coverage_gap(coverage.reason.as_deref())],
            )
        }
        _ => (
            FileCoverageState::Partial,
            vec![provider_resource_gap.unwrap_or_else(|| coverage_gap(coverage.reason.as_deref()))],
        ),
    }
}

fn provider_resource_gap(diagnostics: &[Diagnostic]) -> Option<GapCode> {
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == DiagnosticCode::LargeWorkspacePartial)
    {
        return Some(GapCode::WorkspaceBudgetExceeded);
    }
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == DiagnosticCode::ProviderTimeout)
        .then_some(GapCode::QueryBudgetExceeded)
}

fn coverage_gap(reason: Option<&str>) -> GapCode {
    match reason {
        Some("provider-missing") => GapCode::ProviderUnavailable,
        Some("provider-failed" | "not-returned-by-provider" | "provider-excluded") | None => {
            GapCode::ProviderExecutionIncomplete
        }
        Some("no-compile-context" | "header-not-reachable" | "not-in-active-build") => {
            GapCode::MissingCompileContext
        }
        Some("project-config") => GapCode::MissingProjectMetadata,
        Some("generated") => GapCode::GeneratedSourceMappingUnavailable,
        Some(_) => GapCode::ProviderExecutionIncomplete,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_capability_receipts(
    unit: &AnalysisUnit,
    protocol: ProviderProtocol,
    unit_state: AnalysisUnitState,
    file_count: u64,
    definition_count: u64,
    definition_eligible_count: u64,
    definition_covered_count: u64,
    omitted_definition_count: u64,
    emitted_relations: &BTreeMap<AnalysisCapability, u64>,
    omitted_relations: &BTreeMap<AnalysisCapability, u64>,
    import_audit: &ImportAudit,
    gaps: &[AnalysisGap],
) -> Result<Vec<CapabilityReceipt>, String> {
    let mut receipts = Vec::new();
    for policy in capability_policies(unit.language, protocol) {
        if matches!(
            policy.capability,
            AnalysisCapability::Imports | AnalysisCapability::Exports
        ) && policy.declared_support != DeclaredSupport::Unsupported
        {
            let measured = import_audit.capability(policy.capability).ok_or_else(|| {
                format!(
                    "missing independent {} audit for {}",
                    policy.capability.as_str(),
                    unit.id
                )
            })?;
            let emitted_relation_count = emitted_relations
                .get(&policy.capability)
                .copied()
                .unwrap_or(0);
            if emitted_relation_count != measured.internal_relation_count {
                return Err(format!(
                    "{} emitted relation count does not match its import-site audit: emitted={} audited={}",
                    policy.capability.as_str(),
                    emitted_relation_count,
                    measured.internal_relation_count
                ));
            }
            let incomplete = !measured.denominator_is_known() || measured.truncated_count() > 0;
            let execution_state = if incomplete {
                CapabilityExecutionState::Partial
            } else {
                CapabilityExecutionState::Complete
            };
            let denominator = if measured.denominator_is_known() {
                CoverageDenominator::Known {
                    eligible_count: measured.eligible_count,
                }
            } else {
                CoverageDenominator::Unknown
            };
            let mut gap_codes = measured.gap_codes.iter().copied().collect::<Vec<_>>();
            if incomplete && gap_codes.is_empty() {
                gap_codes.push(GapCode::ProviderExecutionIncomplete);
            }
            gap_codes.sort();
            gap_codes.dedup();
            let receipt = CapabilityReceipt {
                unit_id: unit.id.clone(),
                capability: policy.capability,
                declared_support: policy.declared_support,
                execution_state,
                precision: EvidencePrecision::ExactRange,
                denominator,
                covered_count: measured.covered_count,
                emitted_fact_count: 0,
                emitted_relation_count,
                truncated_count: measured.truncated_count(),
                gap_codes,
            };
            receipt
                .validate()
                .map_err(|error| format!("invalid import capability receipt: {error}"))?;
            receipts.push(receipt);
            continue;
        }
        let emitted_fact_count = if policy.capability == AnalysisCapability::Definitions {
            definition_count
        } else {
            0
        };
        let emitted_relation_count = emitted_relations
            .get(&policy.capability)
            .copied()
            .unwrap_or(0);
        let truncated_count = if policy.capability == AnalysisCapability::Definitions {
            omitted_definition_count
        } else {
            omitted_relations
                .get(&policy.capability)
                .copied()
                .unwrap_or(0)
        };
        let emitted_total = emitted_fact_count + emitted_relation_count;
        let mut gap_codes = gaps
            .iter()
            .filter(|gap| gap.capability.is_none() || gap.capability == Some(policy.capability))
            .map(|gap| gap.code)
            .collect::<Vec<_>>();
        let mut execution_state = match policy.measurement {
            AdapterMeasurement::NotApplicable => CapabilityExecutionState::NotApplicable,
            AdapterMeasurement::Partial(code) => {
                gap_codes.push(code);
                CapabilityExecutionState::Partial
            }
            AdapterMeasurement::Full => CapabilityExecutionState::Complete,
        };
        if policy.declared_support != DeclaredSupport::Unsupported {
            if unit_state == AnalysisUnitState::Failed {
                execution_state = if emitted_total > 0 {
                    CapabilityExecutionState::Partial
                } else {
                    CapabilityExecutionState::Failed
                };
                gap_codes.push(GapCode::ProviderExecutionIncomplete);
            } else if unit_state == AnalysisUnitState::Partial
                && execution_state == CapabilityExecutionState::Complete
            {
                execution_state = CapabilityExecutionState::Partial;
                gap_codes.push(GapCode::ProviderExecutionIncomplete);
            }
            if truncated_count > 0 && execution_state == CapabilityExecutionState::Complete {
                execution_state = CapabilityExecutionState::Partial;
                gap_codes.push(GapCode::UnresolvedTarget);
            }
        }
        if matches!(
            execution_state,
            CapabilityExecutionState::Complete | CapabilityExecutionState::NotApplicable
        ) {
            gap_codes.clear();
        }
        gap_codes.sort();
        gap_codes.dedup();
        let precision = if matches!(
            execution_state,
            CapabilityExecutionState::Complete | CapabilityExecutionState::Partial
        ) {
            policy.precision
        } else {
            EvidencePrecision::None
        };
        let denominator = if policy.capability == AnalysisCapability::Definitions {
            CoverageDenominator::Known {
                eligible_count: definition_eligible_count,
            }
        } else if policy.file_denominator {
            CoverageDenominator::Known {
                eligible_count: file_count,
            }
        } else {
            CoverageDenominator::Unknown
        };
        let covered_count = match policy.capability {
            AnalysisCapability::ProjectStructure => file_count,
            AnalysisCapability::Definitions => definition_covered_count,
            _ => emitted_relation_count,
        };
        let (covered_count, emitted_fact_count, emitted_relation_count, truncated_count) = if matches!(
            execution_state,
            CapabilityExecutionState::Failed
                | CapabilityExecutionState::NotRun
                | CapabilityExecutionState::NotApplicable
        ) {
            (0, 0, 0, 0)
        } else {
            (
                covered_count,
                emitted_fact_count,
                emitted_relation_count,
                truncated_count,
            )
        };
        let receipt = CapabilityReceipt {
            unit_id: unit.id.clone(),
            capability: policy.capability,
            declared_support: policy.declared_support,
            execution_state,
            precision,
            denominator,
            covered_count,
            emitted_fact_count,
            emitted_relation_count,
            truncated_count,
            gap_codes,
        };
        receipt
            .validate()
            .map_err(|error| format!("invalid capability receipt: {error}"))?;
        receipts.push(receipt);
    }
    receipts.sort_by_key(|receipt| receipt.capability);
    Ok(receipts)
}

fn diagnostic_gap(unit: &AnalysisUnit, diagnostic: &Diagnostic) -> Option<AnalysisGap> {
    let (code, capability) = match diagnostic.code {
        DiagnosticCode::MissingDependencyMetadata | DiagnosticCode::DependencyMetadataGap => {
            // Missing dependency metadata can affect imports, calls, and type
            // resolution at once. Do not mislabel the unit-wide gap as one
            // generic reference capability.
            (GapCode::MissingDependencyMetadata, None)
        }
        DiagnosticCode::MissingCompileContext | DiagnosticCode::MissingLegacySdk => (
            GapCode::MissingCompileContext,
            Some(AnalysisCapability::ProjectStructure),
        ),
        DiagnosticCode::LargeWorkspacePartial => (
            GapCode::WorkspaceBudgetExceeded,
            Some(AnalysisCapability::DirectCalls),
        ),
        DiagnosticCode::GeneratedCode => (
            GapCode::GeneratedSourceMappingUnavailable,
            Some(AnalysisCapability::Definitions),
        ),
        DiagnosticCode::DynamicRegistration => (
            GapCode::RuntimeRegistration,
            Some(AnalysisCapability::FrameworkBindings),
        ),
        DiagnosticCode::ProviderMissing => (GapCode::ProviderUnavailable, None),
        DiagnosticCode::ProviderTimeout => (GapCode::QueryBudgetExceeded, None),
        DiagnosticCode::ProviderStopped => (GapCode::ProviderExecutionIncomplete, None),
        _ => return None,
    };
    Some(AnalysisGap {
        code,
        scope: AnalysisScope::AnalysisUnit {
            unit_id: unit.id.clone(),
        },
        capability,
        evidence_ids: Vec::new(),
        message: bounded_message(&diagnostic.message),
    })
}

fn diagnostic_issue(unit: &AnalysisUnit, diagnostic: &Diagnostic) -> Option<AnalysisIssue> {
    let (code, stage, retryable) = match diagnostic.code {
        DiagnosticCode::ProviderMissing => (
            AnalysisErrorCode::ProviderMissing,
            AnalysisStage::ProviderExecution,
            true,
        ),
        DiagnosticCode::ProviderFailed | DiagnosticCode::IndexerFailed => (
            AnalysisErrorCode::ProviderStartFailed,
            AnalysisStage::ProviderExecution,
            true,
        ),
        DiagnosticCode::InvalidOutput => (
            AnalysisErrorCode::ProviderMalformedOutput,
            AnalysisStage::ProviderDecoding,
            false,
        ),
        DiagnosticCode::ProviderTimeout => (
            AnalysisErrorCode::ProviderTimeout,
            AnalysisStage::ProviderExecution,
            true,
        ),
        DiagnosticCode::ProviderStopped => (
            AnalysisErrorCode::ProviderStopped,
            AnalysisStage::ProviderExecution,
            true,
        ),
        _ => return None,
    };
    let scope = diagnostic
        .path
        .as_deref()
        .and_then(|path| RepositoryPath::parse(path).ok())
        .map(|path| AnalysisScope::File {
            unit_id: Some(unit.id.clone()),
            path,
        })
        .unwrap_or_else(|| AnalysisScope::AnalysisUnit {
            unit_id: unit.id.clone(),
        });
    Some(AnalysisIssue {
        code,
        stage,
        scope,
        retryable,
        message: bounded_message(&diagnostic.message),
        remediation: None,
    })
}

fn bounded_message(message: &str) -> String {
    if message.len() <= 2048 {
        return message.to_string();
    }
    let mut end = 2048;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    message[..end].to_string()
}

fn canonicalize_gaps(gaps: &mut Vec<AnalysisGap>) {
    gaps.sort_by(|left, right| {
        (left.code, left.capability, &left.message).cmp(&(
            right.code,
            right.capability,
            &right.message,
        ))
    });
    gaps.dedup_by(|left, right| {
        left.code == right.code && left.capability == right.capability && left.scope == right.scope
    });
}

fn canonicalize_issues(issues: &mut Vec<AnalysisIssue>) {
    issues.sort_by(|left, right| {
        (left.code, &left.message, left.retryable).cmp(&(
            right.code,
            &right.message,
            right.retryable,
        ))
    });
    issues.dedup_by(|left, right| {
        left.code == right.code && left.scope == right.scope && left.message == right.message
    });
}

fn unit_state(status: &str) -> AnalysisUnitState {
    match status {
        "indexed" => AnalysisUnitState::Complete,
        "indexed-partial" | "excluded" | "excluded-by-project-config" | "empty-semantic" => {
            AnalysisUnitState::Partial
        }
        _ => AnalysisUnitState::Failed,
    }
}

fn language_from_id(id: &str) -> Option<ProgrammingLanguage> {
    LANGUAGES
        .iter()
        .find(|language| language.id == id)
        .map(|language| language.contract_language)
}

fn provider_set_digest(
    descriptors: &BTreeMap<ProgrammingLanguage, Option<ProviderDescriptor>>,
    static_analyzer_set_digest: Sha256Digest,
) -> Sha256Digest {
    let mut bytes = Vec::new();
    append_digest_component(&mut bytes, b"codebase-workspace.static-analyzer-set.v1");
    for (language, descriptor) in descriptors {
        append_digest_component(&mut bytes, language.as_str().as_bytes());
        match descriptor {
            Some(descriptor) => {
                append_digest_component(&mut bytes, descriptor.name.as_bytes());
                append_digest_component(
                    &mut bytes,
                    descriptor.version.as_deref().unwrap_or("<none>").as_bytes(),
                );
                append_digest_component(&mut bytes, descriptor.artifact_digest.to_hex().as_bytes());
            }
            None => append_digest_component(&mut bytes, b"<unavailable>"),
        }
    }
    append_digest_component(&mut bytes, static_analyzer_set_digest.to_hex().as_bytes());
    Sha256Digest::of_bytes(&bytes)
}

fn execution_context_set_digest(
    contexts: &BTreeMap<String, ProviderExecutionContext>,
) -> Sha256Digest {
    let contexts = contexts
        .iter()
        .map(|(unit_id, context)| (unit_id.clone(), context.fingerprint))
        .collect::<Vec<_>>();
    super::execution_context_set_digest(&contexts)
}

fn stream_set_digest(summaries: &[UnitEmissionSummary]) -> Sha256Digest {
    let mut bytes = Vec::new();
    for summary in summaries {
        append_digest_component(&mut bytes, summary.unit_id.as_bytes());
        append_digest_component(&mut bytes, summary.stream_digest.to_hex().as_bytes());
    }
    Sha256Digest::of_bytes(&bytes)
}

fn semantic_payload_set_digest(summaries: &[UnitEmissionSummary]) -> Sha256Digest {
    let mut bytes = Vec::new();
    for summary in summaries {
        append_digest_component(&mut bytes, summary.unit_id.as_bytes());
        append_digest_component(
            &mut bytes,
            summary.semantic_payload_digest.to_hex().as_bytes(),
        );
    }
    Sha256Digest::of_bytes(&bytes)
}

fn definition_set_digest(keys: &[String]) -> Sha256Digest {
    let mut keys = keys.to_vec();
    keys.sort();
    Sha256Digest::of_bytes(keys.join("\n").as_bytes())
}

fn append_digest_component(bytes: &mut Vec<u8>, value: &[u8]) {
    bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
    bytes.extend_from_slice(value);
}

#[cfg(test)]
mod definition_reconciliation_tests {
    use super::{reconcile_definition_drafts, short_symbol_name, DefinitionDraft};
    use codebase_fact_model::analysis::ProgrammingLanguage;
    use codebase_fact_model::fact_graph::{FactNodeKind, Visibility};
    use codebase_fact_model::identity::{EvidenceId, ProviderSymbolId};
    use codebase_fact_model::source::SourceFlags;
    use std::collections::BTreeMap;

    fn symbol(value: &str) -> ProviderSymbolId {
        ProviderSymbolId::parse(value).unwrap()
    }

    fn draft(
        id: &str,
        kind: FactNodeKind,
        display_name: &str,
        parent: Option<&str>,
        signature: Option<&str>,
        field_candidate: bool,
    ) -> DefinitionDraft {
        DefinitionDraft {
            symbol_id: symbol(id),
            native_kind: if field_candidate {
                "Variable"
            } else {
                "fixture"
            }
            .to_string(),
            canonical_kind_hint: kind,
            qualified_name: id.to_string(),
            display_name: display_name.to_string(),
            signature: signature.map(str::to_string),
            visibility: Visibility::Unknown,
            parent_symbol_id: parent.map(symbol),
            definition_evidence_id: EvidenceId::from_components(&[id]).unwrap(),
            flags: SourceFlags::default(),
            field_candidate,
            path: codebase_fact_model::source::RepositoryPath::parse("fixture.rs").unwrap(),
            provider_range: vec![0, 0, 1],
            syntax_match: None,
        }
    }

    #[test]
    fn variables_are_fields_only_when_a_type_directly_owns_them() {
        let mut definitions = BTreeMap::from([
            (
                symbol("Box"),
                draft("Box", FactNodeKind::Class, "Box", None, None, false),
            ),
            (
                symbol("Box.value"),
                draft(
                    "Box.value",
                    FactNodeKind::Field,
                    "value",
                    Some("Box"),
                    None,
                    true,
                ),
            ),
            (
                symbol("local"),
                draft("local", FactNodeKind::Field, "local", None, None, true),
            ),
        ]);

        reconcile_definition_drafts(ProgrammingLanguage::Go, &mut definitions);

        assert_eq!(
            definitions[&symbol("Box.value")].canonical_kind_hint,
            FactNodeKind::Field
        );
        assert!(!definitions.contains_key(&symbol("local")));
    }

    #[test]
    fn constructor_and_file_function_kinds_use_structural_context() {
        let mut dart = BTreeMap::from([
            (
                symbol("file"),
                draft(
                    "file",
                    FactNodeKind::Namespace,
                    "types.dart",
                    None,
                    None,
                    false,
                ),
            ),
            (
                symbol("file.run"),
                draft(
                    "file.run",
                    FactNodeKind::Method,
                    "run",
                    Some("file"),
                    Some("()"),
                    false,
                ),
            ),
            (
                symbol("Box"),
                draft("Box", FactNodeKind::Class, "Box", None, None, false),
            ),
            (
                symbol("Box.ctor"),
                draft(
                    "Box.ctor",
                    FactNodeKind::Method,
                    "Box",
                    Some("Box"),
                    Some("(this.value)"),
                    false,
                ),
            ),
        ]);

        reconcile_definition_drafts(ProgrammingLanguage::Dart, &mut dart);

        assert_eq!(
            dart[&symbol("file.run")].canonical_kind_hint,
            FactNodeKind::Function
        );
        assert_eq!(
            dart[&symbol("Box.ctor")].canonical_kind_hint,
            FactNodeKind::Constructor
        );
    }

    #[test]
    fn java_same_named_void_method_is_not_a_constructor() {
        let mut java = BTreeMap::from([
            (
                symbol("Box"),
                draft("Box", FactNodeKind::Class, "Box", None, None, false),
            ),
            (
                symbol("Box.constructor"),
                draft(
                    "Box.constructor",
                    FactNodeKind::Method,
                    "Box(T)",
                    Some("Box"),
                    None,
                    false,
                ),
            ),
            (
                symbol("Box.void_method"),
                draft(
                    "Box.void_method",
                    FactNodeKind::Method,
                    "Box()",
                    Some("Box"),
                    Some(": void"),
                    false,
                ),
            ),
        ]);

        reconcile_definition_drafts(ProgrammingLanguage::Java, &mut java);

        assert_eq!(
            java[&symbol("Box.constructor")].canonical_kind_hint,
            FactNodeKind::Constructor
        );
        assert_eq!(
            java[&symbol("Box.void_method")].canonical_kind_hint,
            FactNodeKind::Method
        );
    }

    #[test]
    fn clangd_anonymous_declarations_keep_a_non_empty_display_name() {
        assert_eq!(
            short_symbol_name("lsp . . . include.fmt.base.h#(anonymous enum)@447:0"),
            "(anonymous enum)"
        );
        assert_eq!(
            short_symbol_name("lsp . . . src.os.cc#(anonymous namespace)@62:10"),
            "(anonymous namespace)"
        );
    }

    #[test]
    fn scoped_npm_symbols_keep_their_real_definition_name() {
        assert_eq!(
            short_symbol_name(
                "scip-typescript npm @nestjs/core 11.1.26 src/cats/`cats.controller.ts`/CatsController#create()."
            ),
            "create"
        );
        assert_eq!(
            short_symbol_name("scip-typescript npm @scope/pkg 1.0.0 src/service.ts/ScopedService#"),
            "ScopedService"
        );
        assert_eq!(
            short_symbol_name("lsp . . . src/service.ts#ScopedService@17:4"),
            "ScopedService"
        );
    }

    #[test]
    fn impossible_self_parent_is_removed_without_dropping_the_definition() {
        let mut definitions = BTreeMap::from([(
            symbol("uv_loop_init"),
            draft(
                "uv_loop_init",
                FactNodeKind::Function,
                "uv_loop_init",
                Some("uv_loop_init"),
                Some("(uv_loop_t*)"),
                false,
            ),
        )]);

        reconcile_definition_drafts(ProgrammingLanguage::C, &mut definitions);

        let definition = &definitions[&symbol("uv_loop_init")];
        assert!(definition.parent_symbol_id.is_none());
        assert_eq!(definition.display_name, "uv_loop_init");
    }
}

#[cfg(test)]
mod file_coverage_tests {
    use super::{file_coverage_state, provider_resource_gap};
    use crate::{Diagnostic, DiagnosticCode, FileCoverageOutput};
    use codebase_fact_model::coverage::{FileCoverageState, GapCode};

    #[test]
    fn provider_timeout_is_a_partial_resource_gap_not_a_source_exclusion() {
        let diagnostics = vec![Diagnostic {
            language: "typescript".to_string(),
            level: "warning",
            code: DiagnosticCode::ProviderTimeout,
            message: "provider time budget exhausted".to_string(),
            detail: None,
            path: None,
            line: None,
        }];
        let resource_gap = provider_resource_gap(&diagnostics);
        let coverage = FileCoverageOutput {
            language: "typescript".to_string(),
            path: "src/large.ts".to_string(),
            status: "excluded",
            reason: Some("provider-excluded".to_string()),
        };

        assert_eq!(resource_gap, Some(GapCode::QueryBudgetExceeded));
        assert_eq!(
            file_coverage_state(Some(&coverage), "excluded", resource_gap),
            (
                FileCoverageState::Partial,
                vec![GapCode::QueryBudgetExceeded]
            )
        );
    }

    #[test]
    fn a_large_workspace_budget_has_a_distinct_stable_gap() {
        let diagnostics = vec![Diagnostic {
            language: "rust".to_string(),
            level: "warning",
            code: DiagnosticCode::LargeWorkspacePartial,
            message: "workspace budget exhausted".to_string(),
            detail: None,
            path: None,
            line: None,
        }];

        assert_eq!(
            provider_resource_gap(&diagnostics),
            Some(GapCode::WorkspaceBudgetExceeded)
        );
    }
}
