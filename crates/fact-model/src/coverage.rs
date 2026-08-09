//! Capability coverage, expected gaps, and operational analysis failures.

use crate::analysis::{AnalysisUnit, ProgrammingLanguage, ProviderDescriptor};
use crate::identity::{AnalysisUnitId, EvidenceId, ProviderSymbolId, Sha256Digest};
use crate::source::{RepositoryPath, SourceFileKind};
use crate::validation::{
    ensure_unique, validate_message, validate_optional_message, ContractError, ContractErrorCode,
    Validate,
};
use serde::{Deserialize, Serialize};

/// Product capabilities measured independently for every analysis unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisCapability {
    ProjectStructure,
    Definitions,
    Imports,
    Exports,
    DirectCalls,
    TypeRelations,
    Overrides,
    TestRelations,
    FrameworkBindings,
    OrmQuery,
    EventExternal,
}

impl AnalysisCapability {
    /// Returns the stable serialized name used by deterministic receipts.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProjectStructure => "project_structure",
            Self::Definitions => "definitions",
            Self::Imports => "imports",
            Self::Exports => "exports",
            Self::DirectCalls => "direct_calls",
            Self::TypeRelations => "type_relations",
            Self::Overrides => "overrides",
            Self::TestRelations => "test_relations",
            Self::FrameworkBindings => "framework_bindings",
            Self::OrmQuery => "orm_query",
            Self::EventExternal => "event_external",
        }
    }
}

/// Whether a certified provider strategy promises a capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeclaredSupport {
    Required,
    Conditional,
    Unsupported,
}

/// What actually happened for one capability in one analysis unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityExecutionState {
    Complete,
    Partial,
    Failed,
    NotRun,
    NotApplicable,
}

/// Finest evidence precision reached by a capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidencePrecision {
    ExactRange,
    Symbol,
    File,
    Manifest,
    None,
}

/// The eligible population for a coverage measurement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "denominatorType",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CoverageDenominator {
    Known { eligible_count: u64 },
    Unknown,
}

/// Expected, evidence-honest reasons why static analysis is incomplete.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GapCode {
    MissingProjectMetadata,
    MissingDependencyMetadata,
    MissingCompileContext,
    MissingTypeMetadata,
    CapabilityUnsupported,
    DynamicDispatch,
    Reflection,
    RuntimeRegistration,
    Metaprogramming,
    GeneratedSourceMappingUnavailable,
    UnresolvedTarget,
    QueryBudgetExceeded,
    WorkspaceBudgetExceeded,
    ProviderUnavailable,
    ProviderExecutionIncomplete,
    ExcludedByRule,
    VcsIgnored,
    ProductIgnored,
    UnsupportedFileType,
    BinarySource,
    SymlinkNotFollowed,
    SymlinkEscapesRoot,
    SensitiveFile,
    DependencyScopeNotEnumerated,
    UnsupportedEncoding,
    UnreadableFile,
}

impl GapCode {
    /// Returns the stable serialized name used by manifests and plans.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissingProjectMetadata => "missing_project_metadata",
            Self::MissingDependencyMetadata => "missing_dependency_metadata",
            Self::MissingCompileContext => "missing_compile_context",
            Self::MissingTypeMetadata => "missing_type_metadata",
            Self::CapabilityUnsupported => "capability_unsupported",
            Self::DynamicDispatch => "dynamic_dispatch",
            Self::Reflection => "reflection",
            Self::RuntimeRegistration => "runtime_registration",
            Self::Metaprogramming => "metaprogramming",
            Self::GeneratedSourceMappingUnavailable => "generated_source_mapping_unavailable",
            Self::UnresolvedTarget => "unresolved_target",
            Self::QueryBudgetExceeded => "query_budget_exceeded",
            Self::WorkspaceBudgetExceeded => "workspace_budget_exceeded",
            Self::ProviderUnavailable => "provider_unavailable",
            Self::ProviderExecutionIncomplete => "provider_execution_incomplete",
            Self::ExcludedByRule => "excluded_by_rule",
            Self::VcsIgnored => "vcs_ignored",
            Self::ProductIgnored => "product_ignored",
            Self::UnsupportedFileType => "unsupported_file_type",
            Self::BinarySource => "binary_source",
            Self::SymlinkNotFollowed => "symlink_not_followed",
            Self::SymlinkEscapesRoot => "symlink_escapes_root",
            Self::SensitiveFile => "sensitive_file",
            Self::DependencyScopeNotEnumerated => "dependency_scope_not_enumerated",
            Self::UnsupportedEncoding => "unsupported_encoding",
            Self::UnreadableFile => "unreadable_file",
        }
    }
}

/// Stable operational failures. These are not semantic gaps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisErrorCode {
    InvalidRequest,
    RepositoryUnavailable,
    RootEscapeRejected,
    ProviderMissing,
    ProviderStartFailed,
    ProviderTimeout,
    ProviderStopped,
    ProviderMalformedOutput,
    ProviderProtocolError,
    SourceDigestMismatch,
    InvalidSourceRange,
    ContractViolation,
    BundleWriteFailed,
    BundleValidationFailed,
    ImportFailed,
    PublishFailed,
    Cancelled,
    Internal,
}

/// Pipeline stage associated with an operational issue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisStage {
    Request,
    SourceCensus,
    UnitPlanning,
    ProviderExecution,
    ProviderDecoding,
    Normalization,
    Reconciliation,
    BundleWrite,
    BundleValidation,
    Import,
    Publish,
}

/// Scope affected by a gap or issue.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "scopeType",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum AnalysisScope {
    Workspace,
    AnalysisUnit {
        unit_id: AnalysisUnitId,
    },
    File {
        unit_id: Option<AnalysisUnitId>,
        path: RepositoryPath,
    },
    RepositoryScope {
        path: RepositoryPath,
    },
    NativeSymbol {
        unit_id: AnalysisUnitId,
        symbol_id: ProviderSymbolId,
    },
}

impl AnalysisScope {
    /// Returns the containing unit when this is not a workspace-wide scope.
    pub fn unit_id(&self) -> Option<&AnalysisUnitId> {
        match self {
            Self::Workspace => None,
            Self::AnalysisUnit { unit_id } | Self::NativeSymbol { unit_id, .. } => Some(unit_id),
            Self::File { unit_id, .. } => unit_id.as_ref(),
            Self::RepositoryScope { .. } => None,
        }
    }
}

/// A per-capability measurement receipt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityReceipt {
    pub unit_id: AnalysisUnitId,
    pub capability: AnalysisCapability,
    pub declared_support: DeclaredSupport,
    pub execution_state: CapabilityExecutionState,
    pub precision: EvidencePrecision,
    pub denominator: CoverageDenominator,
    pub covered_count: u64,
    pub emitted_fact_count: u64,
    pub emitted_relation_count: u64,
    pub truncated_count: u64,
    pub gap_codes: Vec<GapCode>,
}

impl Validate for CapabilityReceipt {
    fn validate(&self) -> Result<(), ContractError> {
        ensure_unique(self.gap_codes.iter(), "gapCodes")?;
        if !self.gap_codes.windows(2).all(|pair| pair[0] <= pair[1]) {
            return Err(ContractError::new(
                ContractErrorCode::NonCanonicalValue,
                "gapCodes",
                "gap codes must use deterministic sorted order",
            ));
        }
        if let CoverageDenominator::Known { eligible_count } = self.denominator {
            if self.covered_count > eligible_count {
                return Err(ContractError::new(
                    ContractErrorCode::InvalidReceipt,
                    "coveredCount",
                    "covered count exceeds the known eligible population",
                ));
            }
        }
        let emitted = self
            .emitted_fact_count
            .checked_add(self.emitted_relation_count)
            .ok_or_else(|| {
                ContractError::new(
                    ContractErrorCode::InvalidReceipt,
                    "emittedCount",
                    "emitted fact and relation counts overflow u64",
                )
            })?;
        if self.precision == EvidencePrecision::None && emitted > 0 {
            return Err(ContractError::new(
                ContractErrorCode::InvalidReceipt,
                "precision",
                "emitted facts require a measured precision",
            ));
        }
        if matches!(
            self.execution_state,
            CapabilityExecutionState::Complete | CapabilityExecutionState::Partial
        ) && self.precision == EvidencePrecision::None
        {
            return Err(ContractError::new(
                ContractErrorCode::InvalidReceipt,
                "precision",
                "complete or partial execution requires a measured precision",
            ));
        }
        if matches!(
            self.execution_state,
            CapabilityExecutionState::Failed
                | CapabilityExecutionState::NotRun
                | CapabilityExecutionState::NotApplicable
        ) && self.precision != EvidencePrecision::None
        {
            return Err(ContractError::new(
                ContractErrorCode::InvalidReceipt,
                "precision",
                "failed, not-run, and not-applicable execution must not claim precision",
            ));
        }
        if self.execution_state == CapabilityExecutionState::Complete && self.truncated_count > 0 {
            return Err(ContractError::new(
                ContractErrorCode::InvalidReceipt,
                "truncatedCount",
                "a complete capability may not report truncated results",
            ));
        }
        if matches!(
            self.execution_state,
            CapabilityExecutionState::Failed
                | CapabilityExecutionState::NotRun
                | CapabilityExecutionState::NotApplicable
        ) && (self.covered_count > 0 || emitted > 0 || self.truncated_count > 0)
        {
            return Err(ContractError::new(
                ContractErrorCode::InvalidReceipt,
                "executionState",
                "failed, not-run, and not-applicable capabilities may not emit partial facts",
            ));
        }
        if matches!(
            self.execution_state,
            CapabilityExecutionState::Partial
                | CapabilityExecutionState::Failed
                | CapabilityExecutionState::NotRun
        ) && self.gap_codes.is_empty()
        {
            return Err(ContractError::new(
                ContractErrorCode::InvalidReceipt,
                "gapCodes",
                "incomplete execution requires at least one stable gap code",
            ));
        }
        if self.declared_support == DeclaredSupport::Unsupported
            && matches!(
                self.execution_state,
                CapabilityExecutionState::Complete | CapabilityExecutionState::Partial
            )
        {
            return Err(ContractError::new(
                ContractErrorCode::InvalidReceipt,
                "declaredSupport",
                "an unsupported capability cannot execute as complete or partial",
            ));
        }
        if self.declared_support == DeclaredSupport::Required
            && self.execution_state == CapabilityExecutionState::NotApplicable
        {
            return Err(ContractError::new(
                ContractErrorCode::InvalidReceipt,
                "executionState",
                "a required capability cannot be marked not applicable",
            ));
        }
        Ok(())
    }
}

/// File-level census and indexing state. This is intentionally distinct from
/// capability coverage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileCoverageState {
    Indexed,
    Partial,
    Excluded,
    Unsupported,
    Failed,
}

/// One file's source-census and indexing receipt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileCoverageRecord {
    pub unit_id: Option<AnalysisUnitId>,
    pub path: RepositoryPath,
    pub language: Option<ProgrammingLanguage>,
    pub file_kind: SourceFileKind,
    pub state: FileCoverageState,
    pub byte_size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub non_blank_line_count: Option<u64>,
    pub content_digest: Option<Sha256Digest>,
    pub gap_codes: Vec<GapCode>,
}

impl Validate for FileCoverageRecord {
    fn validate(&self) -> Result<(), ContractError> {
        if self.path.is_root() {
            return Err(ContractError::new(
                ContractErrorCode::InvalidRepositoryPath,
                "path",
                "file coverage must point to a file",
            ));
        }
        ensure_unique(self.gap_codes.iter(), "gapCodes")?;
        match (self.line_count, self.non_blank_line_count) {
            (Some(lines), Some(non_blank)) if non_blank <= lines => {}
            (None, None) => {}
            _ => {
                return Err(ContractError::new(
                    ContractErrorCode::InvalidReceipt,
                    "lineCount",
                    "line and non-blank line counts must be present together and non-blank cannot exceed total",
                ));
            }
        }
        if !self.gap_codes.windows(2).all(|pair| pair[0] <= pair[1]) {
            return Err(ContractError::new(
                ContractErrorCode::NonCanonicalValue,
                "gapCodes",
                "gap codes must use deterministic sorted order",
            ));
        }
        if matches!(
            self.state,
            FileCoverageState::Indexed | FileCoverageState::Partial
        ) && (self.content_digest.is_none() || self.unit_id.is_none())
        {
            return Err(ContractError::new(
                ContractErrorCode::InvalidReceipt,
                "fileCoverage",
                "indexed or partial files require a content digest and analysis unit",
            ));
        }
        if self.state == FileCoverageState::Indexed && !self.gap_codes.is_empty() {
            return Err(ContractError::new(
                ContractErrorCode::InvalidReceipt,
                "gapCodes",
                "fully indexed files may not carry a file-level gap",
            ));
        }
        if self.state != FileCoverageState::Indexed && self.gap_codes.is_empty() {
            return Err(ContractError::new(
                ContractErrorCode::InvalidReceipt,
                "gapCodes",
                "non-indexed file state requires a stable gap code",
            ));
        }
        Ok(())
    }
}

/// State of an excluded or unreadable repository subtree. Such a subtree is
/// recorded without recursively enumerating every descendant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceScopeState {
    Excluded,
    Unsupported,
    Failed,
}

impl SourceScopeState {
    /// Returns the stable serialized name used in source-manifest digests.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Excluded => "excluded",
            Self::Unsupported => "unsupported",
            Self::Failed => "failed",
        }
    }
}

/// Census receipt for a repository subtree, including whether descendants
/// were explicitly enumerated.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceScopeCoverageRecord {
    pub path: RepositoryPath,
    pub state: SourceScopeState,
    pub descendants_enumerated: bool,
    pub gap_codes: Vec<GapCode>,
}

impl Validate for SourceScopeCoverageRecord {
    fn validate(&self) -> Result<(), ContractError> {
        if self.gap_codes.is_empty() {
            return Err(ContractError::new(
                ContractErrorCode::InvalidReceipt,
                "gapCodes",
                "excluded, unsupported, or failed scope requires a stable gap code",
            ));
        }
        ensure_unique(self.gap_codes.iter(), "gapCodes")?;
        if !self.gap_codes.windows(2).all(|pair| pair[0] <= pair[1]) {
            return Err(ContractError::new(
                ContractErrorCode::NonCanonicalValue,
                "gapCodes",
                "gap codes must use deterministic sorted order",
            ));
        }
        Ok(())
    }
}

/// Expected static-analysis limitation attached to an explicit scope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisGap {
    pub code: GapCode,
    pub scope: AnalysisScope,
    pub capability: Option<AnalysisCapability>,
    pub evidence_ids: Vec<EvidenceId>,
    pub message: String,
}

impl Validate for AnalysisGap {
    fn validate(&self) -> Result<(), ContractError> {
        validate_message(&self.message, "message", 2048)?;
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

/// Operational failure or warning emitted by a deterministic pipeline stage.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisIssue {
    pub code: AnalysisErrorCode,
    pub stage: AnalysisStage,
    pub scope: AnalysisScope,
    pub retryable: bool,
    pub message: String,
    pub remediation: Option<String>,
}

impl Validate for AnalysisIssue {
    fn validate(&self) -> Result<(), ContractError> {
        validate_message(&self.message, "message", 2048)?;
        validate_optional_message(self.remediation.as_deref(), "remediation", 2048)
    }
}

/// Final state of one provider unit stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisUnitState {
    Complete,
    Partial,
    Failed,
    Cancelled,
}

/// Unit-level completion counts used to close a Language IR stream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisUnitCompletion {
    pub unit_id: AnalysisUnitId,
    pub state: AnalysisUnitState,
    pub file_record_count: u64,
    pub definition_count: u64,
    pub relation_count: u64,
    pub evidence_count: u64,
    pub capability_receipt_count: u64,
    pub gap_count: u64,
    pub issue_count: u64,
}

/// Canonical provenance for one completed provider unit. The nested stream
/// counts make provider-to-normalizer parity auditable after import.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisUnitReceipt {
    pub unit: AnalysisUnit,
    pub provider: ProviderDescriptor,
    pub completion: AnalysisUnitCompletion,
}

impl Validate for AnalysisUnitReceipt {
    fn validate(&self) -> Result<(), ContractError> {
        self.unit.validate().map_err(|error| error.under("unit"))?;
        self.provider
            .validate()
            .map_err(|error| error.under("provider"))?;
        if self.completion.unit_id != self.unit.id {
            return Err(ContractError::new(
                ContractErrorCode::InvalidReceipt,
                "completion.unitId",
                "analysis-unit completion must reference the enclosed unit",
            ));
        }
        Ok(())
    }
}
