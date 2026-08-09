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
use definitions::{
    canonical_definition_kind, definition_base_name, definition_display_name, is_type_owner_kind,
    is_variable_definition_kind, normalized_optional_text, ranges_equal,
    reconcile_definition_drafts, reconcile_definition_inventory, short_symbol_name, source_flags,
    DefinitionDraft,
};
pub(crate) use receipts::{
    DefinitionAuditFailure, DefinitionLanguageSummary, DefinitionMetadataAuditEntry,
    ImportAuditEntry, ImportAuditOutcome, ImportLanguageSummary, LanguageIrDiagnosticReceipt,
    LanguageIrEmission, LanguageIrMigrationReceipt, LanguageIrStreamArtifact,
    TypeRelationAuditEntry, TypeRelationLanguageSummary, UnavailableUnitReceipt,
};
use relations::{
    classify_provider_relation, endpoint, endpoint_key, provider_relation_capability,
    relation_kind_rank, relation_sort_key, remap_provider_alias, retain_relation_endpoints,
    ProviderRelationClassificationContext,
};
use source_inventory::inventory_unit_sources;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::time::Instant;

use crate::{
    normalize_scip_language, Diagnostic, DiagnosticCode, DocumentOutput, FileCoverageOutput,
    FileRelationOutput, LanguageOutput, OccurrenceOutput, RelationOutput, SymbolOutput, LANGUAGES,
};

mod artifact_writer;
mod definitions;
mod receipts;
mod relations;
mod source_inventory;

const MIGRATION_RECEIPT_SCHEMA: &str = "codebase-workspace.language-ir-migration-receipt.v7";
const DIAGNOSTIC_RECEIPT_SCHEMA: &str = "codebase-workspace.language-ir-diagnostic-receipt.v1";
const UNAVAILABLE_SAMPLE_LIMIT: usize = 100;
const DEFINITION_AUDIT_SAMPLE_LIMIT: usize = 100;
const DEFINITION_METADATA_AUDIT_SAMPLE_LIMIT: usize = 200;
const IMPORT_AUDIT_SAMPLE_LIMIT: usize = 200;
const TYPE_RELATION_AUDIT_SAMPLE_LIMIT: usize = 200;
const STREAM_ARTIFACT_SCHEMA: &str = "codebase-workspace.language-ir-stream-authority.v2";

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
