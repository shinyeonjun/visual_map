use super::adapter::{
    emit_language_ir, LanguageIrEmissionInput, LanguageIrMigrationReceipt, LanguageIrStreamArtifact,
};
use codebase_fact_model::analysis::{
    ContextDimension, ContextDimensionKind, ProgrammingLanguage, ProviderConfigArtifact,
    ProviderExecutionContext, ProviderExecutionMode,
};
use codebase_fact_model::analysis_plan::AnalysisPlan;
use codebase_fact_model::identity::Sha256Digest;
use codebase_fact_model::source::RepositoryPath;
use codebase_fact_model::source_manifest::SourceManifest;
use codebase_fact_model::validation::Validate;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::{
    build_file_coverage, generated_context_digest, merge_provider_batches, normalize_scip_language,
    source_scope_digest, Diagnostic, DocumentOutput, FileCoverageOutput, FileRelationOutput,
    LanguageOutput, ProviderUnitBatch, RelationOutput, LANGUAGES,
};

const EXECUTION_CONTEXT_RECEIPT_SCHEMA: &str =
    "codebase-workspace.provider-execution-context-reconciliation.v3";
const EXECUTION_CONTEXT_SAMPLE_LIMIT: usize = 100;

pub(crate) struct DirectLanguageIrInput<'a> {
    pub(crate) project_root: &'a Path,
    pub(crate) manifest: &'a SourceManifest,
    pub(crate) plan: &'a AnalysisPlan,
    pub(crate) providers_root: Option<&'a Path>,
    pub(crate) batches: Vec<ProviderUnitBatch>,
    pub(crate) discovered_files: &'a [(String, PathBuf)],
    pub(crate) file_relations: &'a [FileRelationOutput],
    pub(crate) project_model_files: &'a [String],
    /// Diagnostics produced before provider batches are merged, such as an
    /// unavailable compiler project model. They join the same authoritative
    /// emission so the compatibility projection and Language IR cannot drift.
    pub(crate) coordinator_diagnostics: &'a [Diagnostic],
    /// Deterministic non-language analyzers contributing to this snapshot.
    pub(crate) static_analyzer_set_digest: Sha256Digest,
    pub(crate) artifact_root: &'a Path,
}

pub(crate) struct DirectLanguageIrEmission {
    pub(crate) receipt: LanguageIrMigrationReceipt,
    pub(crate) artifact: LanguageIrStreamArtifact,
    pub(crate) compatibility_projection: DirectProviderProjection,
}

pub(crate) struct DirectProviderProjection {
    pub(crate) languages: Vec<LanguageOutput>,
    pub(crate) coverage: Vec<FileCoverageOutput>,
    pub(crate) documents: Vec<DocumentOutput>,
    pub(crate) relations: Vec<RelationOutput>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderExecutionContextReceipt {
    schema: &'static str,
    analysis_plan_digest: String,
    context_set_digest: String,
    execution_count: u64,
    exact_execution_count: u64,
    partial_execution_count: u64,
    not_executed_count: u64,
    generated_or_fallback_count: u64,
    source_file_count: u64,
    missing_dimension_counts: BTreeMap<&'static str, u64>,
    execution_sample: Vec<ExecutionContextAuditEntry>,
    partial_sample: Vec<PartialExecutionContextReceipt>,
    details_truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
struct PartialExecutionContextReceipt {
    unit_id: String,
    language: ProgrammingLanguage,
    mode: ProviderExecutionMode,
    missing_dimensions: Vec<ContextDimensionKind>,
    planned_only_config_files: Vec<RepositoryPath>,
    actual_only_dimensions: Vec<ContextDimension>,
    planned_only_dimensions: Vec<ContextDimension>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExecutionContextAuditEntry {
    unit_id: String,
    language: ProgrammingLanguage,
    mode: ProviderExecutionMode,
    fingerprint: Sha256Digest,
    dimensions: Vec<ContextDimension>,
    missing_dimensions: Vec<ContextDimensionKind>,
    config_artifacts: Vec<ProviderConfigArtifact>,
}

pub(crate) fn reconcile_provider_execution_contexts(
    batches: &[ProviderUnitBatch],
    plan: &AnalysisPlan,
) -> Result<ProviderExecutionContextReceipt, String> {
    plan.validate().map_err(|error| {
        format!("invalid AnalysisPlan before execution reconciliation: {error}")
    })?;
    let owners = plan_file_owners(plan)?;
    let units = plan
        .units
        .iter()
        .map(|unit| (unit.id.as_str(), unit))
        .collect::<BTreeMap<_, _>>();
    let mut context_keys = Vec::new();
    let mut exact_execution_count = 0_u64;
    let mut partial_execution_count = 0_u64;
    let mut not_executed_count = 0_u64;
    let mut generated_or_fallback_count = 0_u64;
    let mut source_file_count = 0_u64;
    let mut missing_dimension_counts = BTreeMap::new();
    let mut execution_audit = Vec::new();
    let mut partial = Vec::new();

    for batch in batches {
        batch.execution_context.validate().map_err(|error| {
            format!(
                "{} provider emitted an invalid execution context: {error}",
                batch.language.id
            )
        })?;
        let (language, unit_id) = batch_owner(batch, &owners)?;
        let unit = units.get(unit_id.as_str()).ok_or_else(|| {
            format!("provider execution references an unknown AnalysisPlan unit: {unit_id}")
        })?;
        let context = &batch.execution_context;
        context_keys.push((unit_id.clone(), context.fingerprint));
        execution_audit.push(ExecutionContextAuditEntry {
            unit_id: unit_id.clone(),
            language,
            mode: context.mode,
            fingerprint: context.fingerprint,
            dimensions: context.dimensions.clone(),
            missing_dimensions: context.missing_dimensions.clone(),
            config_artifacts: context.config_artifacts.clone(),
        });
        if context.mode == ProviderExecutionMode::NotExecuted {
            if matches!(
                batch.language.status,
                "indexed" | "indexed-partial" | "empty-semantic"
            ) {
                return Err(format!(
                    "{} batch claims {} without an executed provider context",
                    language.as_str(),
                    batch.language.status
                ));
            }
            not_executed_count += 1;
            for kind in &context.missing_dimensions {
                *missing_dimension_counts.entry(kind.as_str()).or_default() += 1;
            }
            partial.push(PartialExecutionContextReceipt {
                unit_id: unit_id.clone(),
                language,
                mode: context.mode,
                missing_dimensions: context.missing_dimensions.clone(),
                planned_only_config_files: unit.context.config_files.clone(),
                actual_only_dimensions: Vec::new(),
                planned_only_dimensions: unit.context.dimensions.clone(),
            });
            continue;
        }
        let actual_root = context.analysis_root.as_ref().ok_or_else(|| {
            format!(
                "{} executed provider context omitted its analysis root",
                language.as_str()
            )
        })?;
        if actual_root != &unit.root {
            return Err(format!(
                "{} provider executed root {} but AnalysisPlan unit {} requires {}",
                language.as_str(),
                actual_root,
                unit.id,
                unit.root
            ));
        }
        let planned_configs = unit
            .context
            .config_files
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let actual_configs = context
            .config_artifacts
            .iter()
            .map(|artifact| artifact.path.clone())
            .collect::<BTreeSet<_>>();
        let unplanned = actual_configs
            .difference(&planned_configs)
            .cloned()
            .collect::<Vec<_>>();
        if !unplanned.is_empty() {
            return Err(format!(
                "{} provider executed with configuration outside AnalysisPlan unit {}: {}",
                language.as_str(),
                unit.id,
                unplanned
                    .iter()
                    .map(RepositoryPath::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        let planned_only_config_files = planned_configs
            .difference(&actual_configs)
            .cloned()
            .collect::<Vec<_>>();
        let planned_dimensions = unit
            .context
            .dimensions
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let actual_dimensions = context.dimensions.iter().cloned().collect::<BTreeSet<_>>();
        let actual_only_dimensions = actual_dimensions
            .difference(&planned_dimensions)
            .cloned()
            .collect::<Vec<_>>();
        if context.mode == ProviderExecutionMode::Project && !actual_only_dimensions.is_empty() {
            return Err(format!(
                "{} provider executed dimensions outside AnalysisPlan unit {}: {}",
                language.as_str(),
                unit.id,
                actual_only_dimensions
                    .iter()
                    .map(|dimension| format!("{}={}", dimension.kind.as_str(), dimension.value))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        let planned_only_dimensions = planned_dimensions
            .difference(&actual_dimensions)
            .cloned()
            .collect::<Vec<_>>();
        let exact = context.mode == ProviderExecutionMode::Project
            && context.missing_dimensions.is_empty()
            && planned_only_config_files.is_empty()
            && actual_only_dimensions.is_empty()
            && planned_only_dimensions.is_empty();
        if exact {
            exact_execution_count += 1;
        } else {
            partial_execution_count += 1;
            partial.push(PartialExecutionContextReceipt {
                unit_id: unit_id.clone(),
                language,
                mode: context.mode,
                missing_dimensions: context.missing_dimensions.clone(),
                planned_only_config_files,
                actual_only_dimensions,
                planned_only_dimensions,
            });
        }
        if matches!(
            context.mode,
            ProviderExecutionMode::GeneratedProject
                | ProviderExecutionMode::SourceOnlyFallback
                | ProviderExecutionMode::Composite
        ) {
            generated_or_fallback_count += 1;
        }
        source_file_count += context.source_file_count;
        for kind in &context.missing_dimensions {
            *missing_dimension_counts.entry(kind.as_str()).or_default() += 1;
        }
    }
    context_keys.sort();
    execution_audit.sort();
    let context_set_digest = super::execution_context_set_digest(&context_keys);
    partial.sort();
    Ok(ProviderExecutionContextReceipt {
        schema: EXECUTION_CONTEXT_RECEIPT_SCHEMA,
        analysis_plan_digest: plan.plan_digest.to_hex(),
        context_set_digest: context_set_digest.to_hex(),
        execution_count: batches.len() as u64,
        exact_execution_count,
        partial_execution_count,
        not_executed_count,
        generated_or_fallback_count,
        source_file_count,
        missing_dimension_counts,
        execution_sample: execution_audit
            .iter()
            .take(EXECUTION_CONTEXT_SAMPLE_LIMIT)
            .cloned()
            .collect(),
        partial_sample: partial
            .iter()
            .take(EXECUTION_CONTEXT_SAMPLE_LIMIT)
            .cloned()
            .collect(),
        details_truncated: partial.len() > EXECUTION_CONTEXT_SAMPLE_LIMIT
            || execution_audit.len() > EXECUTION_CONTEXT_SAMPLE_LIMIT,
    })
}

pub(crate) fn emit_direct_language_ir(
    input: DirectLanguageIrInput<'_>,
) -> Result<DirectLanguageIrEmission, String> {
    let timing_enabled = std::env::var_os("CODE_MEMORY_LANGUAGE_IR_TIMING").is_some();
    let direct_started = Instant::now();
    let mut phase_started = Instant::now();
    let DirectLanguageIrInput {
        project_root,
        manifest,
        plan,
        providers_root,
        batches,
        discovered_files,
        file_relations,
        project_model_files,
        coordinator_diagnostics,
        static_analyzer_set_digest,
        artifact_root,
    } = input;
    validate_batch_unit_ownership(&batches, plan)?;
    emit_direct_timing(
        timing_enabled,
        "batch_ownership",
        phase_started.elapsed(),
        direct_started.elapsed(),
    );
    phase_started = Instant::now();
    for batch in &batches {
        let language = contract_language(&batch.language.id).ok_or_else(|| {
            format!(
                "provider emitted a language outside the ten-language contract: {}",
                batch.language.id
            )
        })?;
        ensure_closed_status(batch.language.status)?;
        let source_files = canonical_source_scope(batch)?;
        validate_batch_payload(batch, language, &source_files)?;
    }
    emit_direct_timing(
        timing_enabled,
        "batch_validation",
        phase_started.elapsed(),
        direct_started.elapsed(),
    );
    phase_started = Instant::now();
    let execution_contexts = execution_contexts_by_unit(&batches, plan)?;
    emit_direct_timing(
        timing_enabled,
        "execution_contexts",
        phase_started.elapsed(),
        direct_started.elapsed(),
    );
    phase_started = Instant::now();
    let (languages, documents, relations, provider_diagnostics) = merge_provider_batches(batches);
    emit_direct_timing(
        timing_enabled,
        "merge_provider_batches",
        phase_started.elapsed(),
        direct_started.elapsed(),
    );
    phase_started = Instant::now();
    let mut diagnostics = coordinator_diagnostics.to_vec();
    diagnostics.extend(provider_diagnostics.iter().cloned());
    emit_direct_timing(
        timing_enabled,
        "merge_diagnostics",
        phase_started.elapsed(),
        direct_started.elapsed(),
    );
    phase_started = Instant::now();
    let coverage = build_file_coverage(
        project_root,
        discovered_files,
        &documents,
        &languages,
        project_model_files,
    );
    emit_direct_timing(
        timing_enabled,
        "file_coverage",
        phase_started.elapsed(),
        direct_started.elapsed(),
    );
    phase_started = Instant::now();
    let emission = emit_language_ir(
        LanguageIrEmissionInput {
            project_root,
            manifest,
            plan,
            providers_root,
            languages: &languages,
            coverage: &coverage,
            documents: &documents,
            relations: &relations,
            file_relations,
            project_model_files,
            diagnostics: &diagnostics,
            execution_contexts: &execution_contexts,
            static_analyzer_set_digest,
        },
        artifact_root,
    )?;
    emit_direct_timing(
        timing_enabled,
        "emit_language_ir",
        phase_started.elapsed(),
        direct_started.elapsed(),
    );
    Ok(DirectLanguageIrEmission {
        receipt: emission.receipt,
        artifact: emission.artifact,
        compatibility_projection: DirectProviderProjection {
            languages,
            coverage,
            documents,
            relations,
            diagnostics: provider_diagnostics,
        },
    })
}

fn emit_direct_timing(enabled: bool, phase: &str, elapsed: Duration, total: Duration) {
    if enabled {
        eprintln!(
            "timing stage=direct_language_ir phase={phase} elapsed_ms={} total_ms={}",
            elapsed.as_millis(),
            total.as_millis()
        );
    }
}

fn canonical_source_scope(batch: &ProviderUnitBatch) -> Result<BTreeSet<RepositoryPath>, String> {
    if batch.language.files_found > 0 && batch.source_files.is_empty() {
        return Err(format!(
            "{} provider batch has files but no scheduler-owned source scope",
            batch.language.id
        ));
    }
    batch
        .source_files
        .iter()
        .map(|path| {
            RepositoryPath::parse(path).map_err(|error| {
                format!(
                    "{} provider batch contains an invalid source path {path}: {error}",
                    batch.language.id
                )
            })
        })
        .collect()
}

fn validate_batch_payload(
    batch: &ProviderUnitBatch,
    language: ProgrammingLanguage,
    source_files: &BTreeSet<RepositoryPath>,
) -> Result<(), String> {
    for document in &batch.documents {
        let document_language = normalize_scip_language(&document.language, language.as_str());
        if document_language != language.as_str() {
            return Err(format!(
                "provider document language {} does not match batch language {}",
                document.language,
                language.as_str()
            ));
        }
        let path = RepositoryPath::parse(&document.path)
            .map_err(|error| format!("provider returned invalid document path: {error}"))?;
        if !source_files.contains(&path) {
            return Err(format!(
                "{} provider returned an out-of-scope document: {}",
                language.as_str(),
                path
            ));
        }
    }
    for relation in &batch.relations {
        let path = RepositoryPath::parse(&relation.path)
            .map_err(|error| format!("provider returned invalid relation path: {error}"))?;
        if !source_files.contains(&path) {
            return Err(format!(
                "{} provider returned an out-of-scope relation: {}",
                language.as_str(),
                path
            ));
        }
    }
    Ok(())
}

fn validate_batch_unit_ownership(
    batches: &[ProviderUnitBatch],
    plan: &AnalysisPlan,
) -> Result<(), String> {
    let owners = plan_file_owners(plan)?;
    for batch in batches {
        batch_owner(batch, &owners)?;
    }
    Ok(())
}

pub(super) fn execution_contexts_by_unit(
    batches: &[ProviderUnitBatch],
    plan: &AnalysisPlan,
) -> Result<BTreeMap<String, codebase_fact_model::analysis::ProviderExecutionContext>, String> {
    let owners = plan_file_owners(plan)?;
    let mut grouped = BTreeMap::<String, Vec<&ProviderUnitBatch>>::new();
    for batch in batches {
        let (_, unit_id) = batch_owner(batch, &owners)?;
        grouped.entry(unit_id).or_default().push(batch);
    }
    let mut contexts = BTreeMap::new();
    for (unit_id, batches) in grouped {
        let context = if batches.len() == 1 {
            batches[0].execution_context.clone()
        } else {
            composite_execution_context(&unit_id, &batches)?
        };
        contexts.insert(unit_id, context);
    }
    Ok(contexts)
}

fn composite_execution_context(
    unit_id: &str,
    batches: &[&ProviderUnitBatch],
) -> Result<ProviderExecutionContext, String> {
    let mut roots = BTreeSet::new();
    let mut source_paths = BTreeSet::new();
    let mut config_artifacts = BTreeSet::new();
    let mut dimensions = BTreeSet::new();
    let mut missing_dimensions = BTreeSet::new();
    let mut child_fingerprints = BTreeSet::new();
    let mut executed_count = 0usize;

    for batch in batches {
        for path in &batch.source_files {
            source_paths.insert(
                RepositoryPath::parse(path).map_err(|error| {
                    format!("cannot merge provider shard source {path}: {error}")
                })?,
            );
        }
        let context = &batch.execution_context;
        child_fingerprints.insert(context.fingerprint);
        missing_dimensions.extend(context.missing_dimensions.iter().copied());
        if context.mode == ProviderExecutionMode::NotExecuted {
            continue;
        }
        executed_count += 1;
        roots.extend(context.analysis_root.iter().cloned());
        config_artifacts.extend(context.config_artifacts.iter().cloned());
        dimensions.extend(context.dimensions.iter().cloned());
    }

    if executed_count == 0 {
        return ProviderExecutionContext::not_executed(missing_dimensions.into_iter().collect())
            .map_err(|error| {
                format!("cannot merge unexecuted provider shards for {unit_id}: {error}")
            });
    }
    if roots.len() != 1 {
        return Err(format!(
            "provider shards for Analysis Unit {unit_id} executed {} different roots",
            roots.len()
        ));
    }
    if source_paths.is_empty() {
        return Err(format!(
            "provider shards for Analysis Unit {unit_id} have no canonical source scope"
        ));
    }
    let root = roots.into_iter().next().expect("checked one root");
    let source_paths = source_paths.into_iter().collect::<Vec<_>>();
    let child_parts = child_fingerprints
        .into_iter()
        .map(|fingerprint| fingerprint.as_bytes().to_vec())
        .collect::<Vec<_>>();
    ProviderExecutionContext::executed(
        ProviderExecutionMode::Composite,
        root,
        source_scope_digest(&source_paths),
        source_paths.len() as u64,
        config_artifacts.into_iter().collect(),
        Some(generated_context_digest(&child_parts)),
        dimensions.into_iter().collect(),
        missing_dimensions.into_iter().collect(),
    )
    .map_err(|error| format!("cannot merge provider shards for {unit_id}: {error}"))
}

fn plan_file_owners(
    plan: &AnalysisPlan,
) -> Result<BTreeMap<(ProgrammingLanguage, RepositoryPath), String>, String> {
    let mut owners = BTreeMap::<(ProgrammingLanguage, RepositoryPath), String>::new();
    for assignment in &plan.assignments {
        if assignment.unit_ids.len() != 1 {
            return Err(format!(
                "direct provider adapter requires one owner per file/language: {} has {}",
                assignment.path,
                assignment.unit_ids.len()
            ));
        }
        owners.insert(
            (assignment.language, assignment.path.clone()),
            assignment.unit_ids[0].as_str().to_string(),
        );
    }
    Ok(owners)
}

fn batch_owner(
    batch: &ProviderUnitBatch,
    owners: &BTreeMap<(ProgrammingLanguage, RepositoryPath), String>,
) -> Result<(ProgrammingLanguage, String), String> {
    let language = contract_language(&batch.language.id).ok_or_else(|| {
        format!(
            "provider emitted a language outside the ten-language contract: {}",
            batch.language.id
        )
    })?;
    let mut unit_ids = BTreeSet::new();
    for source_file in &batch.source_files {
        let path = RepositoryPath::parse(source_file).map_err(|error| {
            format!("provider batch contains an invalid source path {source_file}: {error}")
        })?;
        let unit_id = owners.get(&(language, path.clone())).ok_or_else(|| {
            format!(
                "{} provider source is not owned by the AnalysisPlan: {}",
                language.as_str(),
                path
            )
        })?;
        unit_ids.insert(unit_id.clone());
    }
    if unit_ids.len() != 1 {
        return Err(format!(
            "{} provider batch maps to {} AnalysisPlan semantic contexts; exactly one is required",
            language.as_str(),
            unit_ids.len()
        ));
    }
    Ok((
        language,
        unit_ids.into_iter().next().expect("checked one owner"),
    ))
}

fn contract_language(id: &str) -> Option<ProgrammingLanguage> {
    LANGUAGES
        .iter()
        .find(|language| language.id == id)
        .map(|language| language.contract_language)
}

fn ensure_closed_status(status: &str) -> Result<(), String> {
    if matches!(
        status,
        "indexed"
            | "indexed-partial"
            | "excluded"
            | "excluded-by-project-config"
            | "empty-semantic"
            | "missing-tool"
            | "indexer-failed"
            | "invalid-output"
    ) {
        Ok(())
    } else {
        Err(format!(
            "provider emitted an unknown execution status: {status}"
        ))
    }
}
