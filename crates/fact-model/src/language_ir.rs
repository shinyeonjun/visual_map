//! Transient, streaming provider-to-normalizer contract.

use crate::analysis::{
    AnalysisUnit, ProgrammingLanguage, ProviderDescriptor, ProviderExecutionContext,
    ProviderExecutionMode,
};
use crate::coverage::{
    AnalysisGap, AnalysisIssue, AnalysisUnitCompletion, CapabilityReceipt, FileCoverageRecord,
};
use crate::evidence::{EvidenceProducerKind, FactEvidence};
use crate::fact_graph::{
    DispatchKind, ExecutionOccurrence, FactEdgeKind, FactNodeKind, FactTruth, ResolutionMethod,
    Visibility,
};
use crate::identity::{
    AnalysisUnitId, EvidenceId, ProviderSymbolId, SemanticContextId, Sha256Digest, SnapshotId,
};
use crate::source::{RepositoryPath, SourceFlags};
use crate::validation::{
    ensure_unique, validate_optional_text, validate_text, ContractError, ContractErrorCode,
    Validate,
};
use crate::ContractSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Header opening exactly one analysis-unit stream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LanguageIrHeader {
    pub schema: ContractSchema,
    pub snapshot_id: SnapshotId,
    pub source_manifest_digest: Sha256Digest,
    pub unit: AnalysisUnit,
    pub provider: ProviderDescriptor,
    pub execution_context: ProviderExecutionContext,
}

impl Validate for LanguageIrHeader {
    fn validate(&self) -> Result<(), ContractError> {
        if self.schema != ContractSchema::LanguageIrV2 {
            return Err(ContractError::new(
                ContractErrorCode::InvalidSchema,
                "schema",
                "Language IR header requires the language-ir v2 schema",
            ));
        }
        self.unit.validate().map_err(|error| error.under("unit"))?;
        self.provider
            .validate()
            .map_err(|error| error.under("provider"))?;
        self.execution_context
            .validate()
            .map_err(|error| error.under("executionContext"))?;
        if self.execution_context.mode != ProviderExecutionMode::NotExecuted
            && self.execution_context.analysis_root.as_ref() != Some(&self.unit.root)
        {
            return Err(ContractError::new(
                ContractErrorCode::InvalidReceipt,
                "executionContext.analysisRoot",
                "the executed provider root must equal the Analysis Unit root",
            ));
        }
        Ok(())
    }
}

/// One provider definition before canonical ID assignment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IrDefinition {
    pub unit_id: AnalysisUnitId,
    pub symbol_id: ProviderSymbolId,
    pub native_kind: String,
    pub canonical_kind_hint: FactNodeKind,
    pub qualified_name: String,
    pub display_name: String,
    pub signature: Option<String>,
    pub visibility: Visibility,
    pub parent_symbol_id: Option<ProviderSymbolId>,
    pub definition_evidence_id: EvidenceId,
    pub flags: SourceFlags,
}

impl Validate for IrDefinition {
    fn validate(&self) -> Result<(), ContractError> {
        validate_text(&self.native_kind, "nativeKind", 512)?;
        validate_text(&self.qualified_name, "qualifiedName", 4096)?;
        validate_text(&self.display_name, "displayName", 512)?;
        validate_optional_text(self.signature.as_deref(), "signature", 8192)?;
        if self.parent_symbol_id.as_ref() == Some(&self.symbol_id) {
            return Err(ContractError::new(
                ContractErrorCode::NonCanonicalValue,
                "parentSymbolId",
                "a definition may not contain itself",
            ));
        }
        Ok(())
    }
}

/// Provider-resolved endpoint families accepted by the language layer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "endpointType",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum IrEndpoint {
    NativeSymbol {
        symbol_id: ProviderSymbolId,
    },
    File {
        path: RepositoryPath,
    },
    /// Exact project structure that is not equivalent to one arbitrary file.
    /// This is required for package/namespace imports such as Go packages,
    /// Java wildcard imports, and C# namespace imports.
    Structure {
        unit_id: AnalysisUnitId,
        kind: FactNodeKind,
        qualified_name: String,
    },
}

impl Validate for IrEndpoint {
    fn validate(&self) -> Result<(), ContractError> {
        match self {
            Self::File { path } => {
                if path.is_root() {
                    return Err(ContractError::new(
                        ContractErrorCode::InvalidRepositoryPath,
                        "path",
                        "a relation endpoint must reference a file",
                    ));
                }
            }
            Self::Structure {
                kind,
                qualified_name,
                ..
            } => {
                if !matches!(
                    kind,
                    FactNodeKind::Package | FactNodeKind::Module | FactNodeKind::Namespace
                ) {
                    return Err(ContractError::new(
                        ContractErrorCode::NonCanonicalValue,
                        "kind",
                        "a language structure endpoint must be a package, module, or namespace",
                    ));
                }
                validate_text(qualified_name, "qualifiedName", 4096)?;
            }
            Self::NativeSymbol { .. } => {}
        }
        Ok(())
    }
}

/// Relations produced directly by a language provider or exact project model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LanguageRelationKind {
    Contains,
    Declares,
    BelongsTo,
    Imports,
    Exports,
    Calls,
    Constructs,
    Extends,
    Implements,
    MixesIn,
    Overrides,
    UsesType,
    ExecutesQuery,
    Reads,
    Writes,
    Tests,
}

/// One provider relation before canonical endpoint assignment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IrRelation {
    pub unit_id: AnalysisUnitId,
    pub source: IrEndpoint,
    pub target: IrEndpoint,
    pub kind: LanguageRelationKind,
    pub truth: FactTruth,
    pub resolution: ResolutionMethod,
    pub dispatch: DispatchKind,
    pub semantic_context_id: SemanticContextId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<ExecutionOccurrence>,
    pub evidence_ids: Vec<EvidenceId>,
}

impl Validate for IrRelation {
    fn validate(&self) -> Result<(), ContractError> {
        self.source
            .validate()
            .map_err(|error| error.under("source"))?;
        self.target
            .validate()
            .map_err(|error| error.under("target"))?;
        if !matches!(
            self.resolution,
            ResolutionMethod::Compiler
                | ResolutionMethod::Provider
                | ResolutionMethod::ProjectModel
                | ResolutionMethod::SyntaxExact
        ) {
            return Err(ContractError::new(
                ContractErrorCode::NonCanonicalValue,
                "resolution",
                "language relations require a language-layer resolution method",
            ));
        }
        if self.evidence_ids.is_empty() {
            return Err(ContractError::new(
                ContractErrorCode::MissingEvidence,
                "evidenceIds",
                "language relations require evidence",
            ));
        }
        match (&self.execution, self.kind) {
            (Some(execution), LanguageRelationKind::Calls) => execution
                .validate_for(FactEdgeKind::Calls, &self.evidence_ids)
                .map_err(|error| error.under("execution"))?,
            (Some(execution), LanguageRelationKind::Constructs) => execution
                .validate_for(FactEdgeKind::Constructs, &self.evidence_ids)
                .map_err(|error| error.under("execution"))?,
            (Some(_), _) => {
                return Err(ContractError::new(
                    ContractErrorCode::NonCanonicalValue,
                    "execution",
                    "only calls and constructs may carry execution occurrence data",
                ));
            }
            (None, LanguageRelationKind::Calls | LanguageRelationKind::Constructs) => {
                return Err(ContractError::new(
                    ContractErrorCode::MissingEvidence,
                    "execution",
                    "call and construct relations require one exact execution occurrence",
                ));
            }
            (None, _) => {}
        }
        ensure_unique(self.evidence_ids.iter(), "evidenceIds")?;
        if !self.evidence_ids.windows(2).all(|pair| pair[0] <= pair[1]) {
            return Err(ContractError::new(
                ContractErrorCode::NonCanonicalValue,
                "evidenceIds",
                "evidence IDs must use deterministic sorted order",
            ));
        }
        Ok(())
    }
}

/// One bounded record in the Language IR protocol. A stream starts with one
/// header, ends with one completion receipt, and contains no records afterward.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "recordType",
    content = "record",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum LanguageIrRecord {
    Header(Box<LanguageIrHeader>),
    File(FileCoverageRecord),
    Evidence(FactEvidence),
    Definition(IrDefinition),
    Relation(IrRelation),
    CapabilityReceipt(CapabilityReceipt),
    Gap(AnalysisGap),
    Issue(AnalysisIssue),
    Complete(AnalysisUnitCompletion),
}

impl Validate for LanguageIrRecord {
    fn validate(&self) -> Result<(), ContractError> {
        match self {
            Self::Header(record) => record.validate(),
            Self::File(record) => record.validate(),
            Self::Evidence(record) => record.validate(),
            Self::Definition(record) => record.validate(),
            Self::Relation(record) => record.validate(),
            Self::CapabilityReceipt(record) => record.validate(),
            Self::Gap(record) => record.validate(),
            Self::Issue(record) => record.validate(),
            Self::Complete(_) => Ok(()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct StreamCounts {
    files: u64,
    definitions: u64,
    relations: u64,
    evidence: u64,
    capabilities: u64,
    gaps: u64,
    issues: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum StreamState {
    New,
    Open {
        unit_id: AnalysisUnitId,
        language: ProgrammingLanguage,
        semantic_context_id: SemanticContextId,
        execution_mode: ProviderExecutionMode,
        counts: StreamCounts,
        capabilities: BTreeSet<crate::coverage::AnalysisCapability>,
    },
    Complete,
}

/// Stateful, constant-memory validator for Language IR record order and
/// unit-level count receipts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageIrStreamValidator {
    state: StreamState,
}

impl Default for LanguageIrStreamValidator {
    fn default() -> Self {
        Self {
            state: StreamState::New,
        }
    }
}

impl LanguageIrStreamValidator {
    pub fn push(&mut self, record: &LanguageIrRecord) -> Result<(), ContractError> {
        record.validate()?;
        match (&mut self.state, record) {
            (StreamState::New, LanguageIrRecord::Header(header)) => {
                self.state = StreamState::Open {
                    unit_id: header.unit.id.clone(),
                    language: header.unit.language,
                    semantic_context_id: header.unit.context.id.clone(),
                    execution_mode: header.execution_context.mode,
                    counts: StreamCounts::default(),
                    capabilities: BTreeSet::new(),
                };
                Ok(())
            }
            (StreamState::New, _) => Err(stream_error(
                "record",
                "Language IR stream must start with exactly one header",
            )),
            (StreamState::Complete, _) => Err(stream_error(
                "record",
                "Language IR stream may not contain records after completion",
            )),
            (StreamState::Open { .. }, LanguageIrRecord::Header(_)) => Err(stream_error(
                "record",
                "Language IR stream may contain only one header",
            )),
            (
                StreamState::Open {
                    unit_id, counts, ..
                },
                LanguageIrRecord::Complete(completion),
            ) => {
                require_unit(unit_id, &completion.unit_id)?;
                validate_completion(*counts, completion)?;
                self.state = StreamState::Complete;
                Ok(())
            }
            (
                StreamState::Open {
                    unit_id,
                    language,
                    semantic_context_id,
                    execution_mode,
                    counts,
                    capabilities,
                },
                record,
            ) => {
                require_unit_scoped_record(record)?;
                if let Some(record_unit_id) = record_unit_id(record) {
                    require_unit(unit_id, record_unit_id)?;
                }
                require_header_context(*language, semantic_context_id, record)?;
                require_execution_provenance(*execution_mode, record)?;
                if let LanguageIrRecord::CapabilityReceipt(receipt) = record {
                    if !capabilities.insert(receipt.capability) {
                        return Err(stream_error(
                            "capability",
                            "a Language IR unit may contain only one receipt per capability",
                        ));
                    }
                }
                increment_counts(counts, record);
                Ok(())
            }
        }
    }

    /// Fails unless a valid completion receipt has closed the stream.
    pub fn finish(self) -> Result<(), ContractError> {
        match self.state {
            StreamState::Complete => Ok(()),
            StreamState::New => Err(stream_error("stream", "Language IR stream is empty")),
            StreamState::Open { language, .. } => Err(stream_error(
                "stream",
                format!(
                    "{} Language IR stream ended without a completion receipt",
                    language.as_str()
                ),
            )),
        }
    }
}

fn require_execution_provenance(
    mode: ProviderExecutionMode,
    record: &LanguageIrRecord,
) -> Result<(), ContractError> {
    if mode != ProviderExecutionMode::NotExecuted {
        return Ok(());
    }
    match record {
        LanguageIrRecord::Evidence(evidence)
            if matches!(
                evidence.producer.kind,
                EvidenceProducerKind::Scip
                    | EvidenceProducerKind::LanguageServer
                    | EvidenceProducerKind::CompilerApi
            ) =>
        {
            Err(stream_error(
                "producer.kind",
                "a not-executed provider unit may not claim SCIP, LSP, or compiler evidence",
            ))
        }
        LanguageIrRecord::Relation(relation)
            if matches!(
                relation.resolution,
                ResolutionMethod::Provider | ResolutionMethod::Compiler
            ) =>
        {
            Err(stream_error(
                "resolution",
                "a not-executed provider unit may not claim provider or compiler resolution",
            ))
        }
        _ => Ok(()),
    }
}

fn require_header_context(
    language: ProgrammingLanguage,
    semantic_context_id: &SemanticContextId,
    record: &LanguageIrRecord,
) -> Result<(), ContractError> {
    match record {
        LanguageIrRecord::File(file) if file.language != Some(language) => Err(stream_error(
            "language",
            "file language does not match the Language IR stream header",
        )),
        LanguageIrRecord::Relation(relation)
            if &relation.semantic_context_id != semantic_context_id =>
        {
            Err(stream_error(
                "semanticContextId",
                "relation semantic context does not match the Language IR stream header",
            ))
        }
        _ => Ok(()),
    }
}

fn require_unit_scoped_record(record: &LanguageIrRecord) -> Result<(), ContractError> {
    let missing_unit = match record {
        LanguageIrRecord::File(FileCoverageRecord { unit_id, .. }) => unit_id.is_none(),
        LanguageIrRecord::Gap(record) => record.scope.unit_id().is_none(),
        LanguageIrRecord::Issue(record) => record.scope.unit_id().is_none(),
        _ => false,
    };
    if missing_unit {
        return Err(stream_error(
            "unitId",
            "a record inside a Language IR unit stream requires analysis-unit scope",
        ));
    }
    Ok(())
}

fn record_unit_id(record: &LanguageIrRecord) -> Option<&AnalysisUnitId> {
    match record {
        LanguageIrRecord::File(record) => record.unit_id.as_ref(),
        LanguageIrRecord::Definition(record) => Some(&record.unit_id),
        LanguageIrRecord::Relation(record) => Some(&record.unit_id),
        LanguageIrRecord::CapabilityReceipt(record) => Some(&record.unit_id),
        LanguageIrRecord::Gap(record) => record.scope.unit_id(),
        LanguageIrRecord::Issue(record) => record.scope.unit_id(),
        LanguageIrRecord::Header(_)
        | LanguageIrRecord::Evidence(_)
        | LanguageIrRecord::Complete(_) => None,
    }
}

fn require_unit(expected: &AnalysisUnitId, actual: &AnalysisUnitId) -> Result<(), ContractError> {
    if expected != actual {
        return Err(stream_error(
            "unitId",
            "record unit ID does not match the stream header",
        ));
    }
    Ok(())
}

fn increment_counts(counts: &mut StreamCounts, record: &LanguageIrRecord) {
    match record {
        LanguageIrRecord::File(_) => counts.files += 1,
        LanguageIrRecord::Evidence(_) => counts.evidence += 1,
        LanguageIrRecord::Definition(_) => counts.definitions += 1,
        LanguageIrRecord::Relation(_) => counts.relations += 1,
        LanguageIrRecord::CapabilityReceipt(_) => counts.capabilities += 1,
        LanguageIrRecord::Gap(_) => counts.gaps += 1,
        LanguageIrRecord::Issue(_) => counts.issues += 1,
        LanguageIrRecord::Header(_) | LanguageIrRecord::Complete(_) => {}
    }
}

fn validate_completion(
    counts: StreamCounts,
    completion: &AnalysisUnitCompletion,
) -> Result<(), ContractError> {
    let expected = [
        (
            "fileRecordCount",
            counts.files,
            completion.file_record_count,
        ),
        (
            "definitionCount",
            counts.definitions,
            completion.definition_count,
        ),
        ("relationCount", counts.relations, completion.relation_count),
        ("evidenceCount", counts.evidence, completion.evidence_count),
        (
            "capabilityReceiptCount",
            counts.capabilities,
            completion.capability_receipt_count,
        ),
        ("gapCount", counts.gaps, completion.gap_count),
        ("issueCount", counts.issues, completion.issue_count),
    ];
    if let Some((field, actual, declared)) = expected
        .into_iter()
        .find(|(_, actual, declared)| actual != declared)
    {
        return Err(stream_error(
            field,
            format!("completion declared {declared} records but stream contained {actual}"),
        ));
    }
    Ok(())
}

fn stream_error(path: &str, message: impl Into<String>) -> ContractError {
    ContractError::new(ContractErrorCode::StreamOrder, path, message)
}
