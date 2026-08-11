//! Canonical, provider-neutral Fact Graph rows.

use crate::analysis::ProgrammingLanguage;
use crate::coverage::{
    AnalysisGap, AnalysisIssue, AnalysisUnitReceipt, CapabilityReceipt, FileCoverageRecord,
    SourceScopeCoverageRecord,
};
use crate::evidence::FactEvidence;
use crate::identity::{
    AnalysisUnitId, EvidenceId, FactEdgeId, FactNodeId, SemanticContextId, Sha256Digest,
    SnapshotId, WorkspaceId,
};
use crate::source::SourceFlags;
use crate::validation::{
    ensure_unique, validate_optional_text, validate_text, ContractError, ContractErrorCode,
    Validate,
};
use crate::ContractSchema;
use serde::{Deserialize, Serialize};

/// Broad node family used for bounded queries and presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactNodeFamily {
    Structure,
    Symbol,
    Interface,
    Data,
    Integration,
    Verification,
}

/// Closed node kinds required by the final static data catalog.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactNodeKind {
    Repository,
    Application,
    ServiceBoundary,
    Library,
    Package,
    Module,
    File,
    BuildTarget,
    Entrypoint,
    Job,
    Namespace,
    Type,
    Class,
    Interface,
    Trait,
    Struct,
    Enum,
    TypeAlias,
    Callable,
    Function,
    Method,
    Constructor,
    Constant,
    Field,
    HttpRoute,
    GraphqlEndpoint,
    RpcEndpoint,
    FrontendPage,
    FrontendAction,
    Database,
    Schema,
    Table,
    View,
    MaterializedView,
    Column,
    PrimaryKey,
    ForeignKey,
    UniqueConstraint,
    CheckConstraint,
    Index,
    Routine,
    Trigger,
    Sequence,
    DatabaseType,
    Policy,
    Query,
    /// A database object name written in application source. This is not a
    /// certified database object. A deterministic DB reconciliation step may
    /// connect it to exactly one [`FactNodeKind::Table`] through
    /// [`FactEdgeKind::MapsToTable`].
    TableReference,
    Migration,
    OrmModel,
    Event,
    Queue,
    Topic,
    Stream,
    Channel,
    ExternalService,
    Cache,
    FileResource,
    ConfigBoundary,
    EnvironmentVariable,
    FeatureFlag,
    TestCase,
}

impl FactNodeKind {
    pub const fn family(self) -> FactNodeFamily {
        match self {
            Self::Repository
            | Self::Application
            | Self::ServiceBoundary
            | Self::Library
            | Self::Package
            | Self::Module
            | Self::File
            | Self::BuildTarget
            | Self::Entrypoint
            | Self::Job => FactNodeFamily::Structure,
            Self::Namespace
            | Self::Type
            | Self::Class
            | Self::Interface
            | Self::Trait
            | Self::Struct
            | Self::Enum
            | Self::TypeAlias
            | Self::Callable
            | Self::Function
            | Self::Method
            | Self::Constructor
            | Self::Constant
            | Self::Field => FactNodeFamily::Symbol,
            Self::HttpRoute
            | Self::GraphqlEndpoint
            | Self::RpcEndpoint
            | Self::FrontendPage
            | Self::FrontendAction => FactNodeFamily::Interface,
            Self::Database
            | Self::Schema
            | Self::Table
            | Self::View
            | Self::MaterializedView
            | Self::Column
            | Self::PrimaryKey
            | Self::ForeignKey
            | Self::UniqueConstraint
            | Self::CheckConstraint
            | Self::Index
            | Self::Routine
            | Self::Trigger
            | Self::Sequence
            | Self::DatabaseType
            | Self::Policy
            | Self::Query
            | Self::TableReference
            | Self::Migration
            | Self::OrmModel => FactNodeFamily::Data,
            Self::Event
            | Self::Queue
            | Self::Topic
            | Self::Stream
            | Self::Channel
            | Self::ExternalService
            | Self::Cache
            | Self::FileResource
            | Self::ConfigBoundary
            | Self::EnvironmentVariable
            | Self::FeatureFlag => FactNodeFamily::Integration,
            Self::TestCase => FactNodeFamily::Verification,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Repository => "repository",
            Self::Application => "application",
            Self::ServiceBoundary => "service_boundary",
            Self::Library => "library",
            Self::Package => "package",
            Self::Module => "module",
            Self::File => "file",
            Self::BuildTarget => "build_target",
            Self::Entrypoint => "entrypoint",
            Self::Job => "job",
            Self::Namespace => "namespace",
            Self::Type => "type",
            Self::Class => "class",
            Self::Interface => "interface",
            Self::Trait => "trait",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::TypeAlias => "type_alias",
            Self::Callable => "callable",
            Self::Function => "function",
            Self::Method => "method",
            Self::Constructor => "constructor",
            Self::Constant => "constant",
            Self::Field => "field",
            Self::HttpRoute => "http_route",
            Self::GraphqlEndpoint => "graphql_endpoint",
            Self::RpcEndpoint => "rpc_endpoint",
            Self::FrontendPage => "frontend_page",
            Self::FrontendAction => "frontend_action",
            Self::Database => "database",
            Self::Schema => "schema",
            Self::Table => "table",
            Self::View => "view",
            Self::MaterializedView => "materialized_view",
            Self::Column => "column",
            Self::PrimaryKey => "primary_key",
            Self::ForeignKey => "foreign_key",
            Self::UniqueConstraint => "unique_constraint",
            Self::CheckConstraint => "check_constraint",
            Self::Index => "index",
            Self::Routine => "routine",
            Self::Trigger => "trigger",
            Self::Sequence => "sequence",
            Self::DatabaseType => "database_type",
            Self::Policy => "policy",
            Self::Query => "query",
            Self::TableReference => "table_reference",
            Self::Migration => "migration",
            Self::OrmModel => "orm_model",
            Self::Event => "event",
            Self::Queue => "queue",
            Self::Topic => "topic",
            Self::Stream => "stream",
            Self::Channel => "channel",
            Self::ExternalService => "external_service",
            Self::Cache => "cache",
            Self::FileResource => "file_resource",
            Self::ConfigBoundary => "config_boundary",
            Self::EnvironmentVariable => "environment_variable",
            Self::FeatureFlag => "feature_flag",
            Self::TestCase => "test_case",
        }
    }
}

/// Factual roles attached to an existing symbol rather than represented by
/// duplicate role nodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactRole {
    Handler,
    Middleware,
    Controller,
    Service,
    Repository,
    DataAccess,
    Worker,
    Scheduler,
    Publisher,
    Consumer,
    ApiClient,
    FrontendComponent,
    StateStore,
    OrmEntity,
}

/// A role and the evidence proving it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactRoleAssignment {
    pub role: FactRole,
    pub evidence_ids: Vec<EvidenceId>,
}

impl Validate for FactRoleAssignment {
    fn validate(&self) -> Result<(), ContractError> {
        if self.evidence_ids.is_empty() {
            return Err(ContractError::new(
                ContractErrorCode::MissingEvidence,
                "evidenceIds",
                "a factual role requires evidence",
            ));
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

/// Visibility normalized across supported languages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    Protected,
    Internal,
    Package,
    Private,
    Unknown,
}

/// Closed, kind-specific payloads used by product queries. A generic property
/// bag is intentionally forbidden: every persisted field must be part of the
/// reviewed final visualization contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "detailType",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum FactNodeDetails {
    HttpRoute { method: String, path: String },
}

impl FactNodeDetails {
    fn validate_for(&self, kind: FactNodeKind) -> Result<(), ContractError> {
        match (kind, self) {
            (FactNodeKind::HttpRoute, Self::HttpRoute { method, path }) => {
                validate_http_method(method)?;
                validate_route_path(path)
            }
            _ => Err(ContractError::new(
                ContractErrorCode::NonCanonicalValue,
                "details",
                "node details do not match the canonical node kind",
            )),
        }
    }
}

/// One canonical Fact Graph node.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactNode {
    pub id: FactNodeId,
    pub snapshot_id: SnapshotId,
    pub family: FactNodeFamily,
    pub kind: FactNodeKind,
    pub native_kind: Option<String>,
    pub qualified_name: String,
    pub display_name: String,
    pub signature: Option<String>,
    pub details: Option<FactNodeDetails>,
    pub visibility: Visibility,
    pub language: Option<ProgrammingLanguage>,
    pub analysis_unit_id: Option<AnalysisUnitId>,
    pub parent_id: Option<FactNodeId>,
    pub definition_evidence_id: Option<EvidenceId>,
    pub evidence_ids: Vec<EvidenceId>,
    pub roles: Vec<FactRoleAssignment>,
    pub flags: SourceFlags,
}

impl Validate for FactNode {
    fn validate(&self) -> Result<(), ContractError> {
        if self.family != self.kind.family() {
            return Err(ContractError::new(
                ContractErrorCode::NonCanonicalValue,
                "family",
                "node family does not match canonical node kind",
            ));
        }
        validate_optional_text(self.native_kind.as_deref(), "nativeKind", 512)?;
        validate_text(&self.qualified_name, "qualifiedName", 4096)?;
        validate_text(&self.display_name, "displayName", 512)?;
        validate_optional_text(self.signature.as_deref(), "signature", 8192)?;
        match (&self.kind, &self.details) {
            (FactNodeKind::HttpRoute, Some(FactNodeDetails::HttpRoute { method, path })) => {
                self.details
                    .as_ref()
                    .expect("matched route details")
                    .validate_for(self.kind)?;
                if self.qualified_name != format!("{method} {path}") {
                    return Err(ContractError::new(
                        ContractErrorCode::NonCanonicalValue,
                        "qualifiedName",
                        "HTTP route identity must be the canonical method and path",
                    ));
                }
            }
            (FactNodeKind::HttpRoute, None) => {
                return Err(ContractError::new(
                    ContractErrorCode::NonCanonicalValue,
                    "details",
                    "an HTTP route requires typed method and path details",
                ));
            }
            (_, Some(details)) => details.validate_for(self.kind)?,
            (_, None) => {}
        }
        let expected = Self::stable_id(
            self.kind,
            self.language,
            self.analysis_unit_id.as_ref(),
            &self.qualified_name,
            self.signature.as_deref(),
        )?;
        if self.id != expected {
            return Err(ContractError::new(
                ContractErrorCode::InvalidIdentifier,
                "id",
                "node ID does not match kind, language, analysis unit, qualified name, and signature",
            ));
        }
        if self.evidence_ids.is_empty() {
            return Err(ContractError::new(
                ContractErrorCode::MissingEvidence,
                "evidenceIds",
                "every canonical node requires evidence",
            ));
        }
        ensure_unique(self.evidence_ids.iter(), "evidenceIds")?;
        if !self.evidence_ids.windows(2).all(|pair| pair[0] <= pair[1]) {
            return Err(ContractError::new(
                ContractErrorCode::NonCanonicalValue,
                "evidenceIds",
                "evidence IDs must use deterministic sorted order",
            ));
        }
        if self
            .definition_evidence_id
            .as_ref()
            .is_some_and(|id| !self.evidence_ids.contains(id))
        {
            return Err(ContractError::new(
                ContractErrorCode::MissingEvidence,
                "definitionEvidenceId",
                "definition evidence must also appear in evidenceIds",
            ));
        }
        ensure_unique(self.roles.iter().map(|assignment| assignment.role), "roles")?;
        if !self
            .roles
            .windows(2)
            .all(|pair| pair[0].role <= pair[1].role)
        {
            return Err(ContractError::new(
                ContractErrorCode::NonCanonicalValue,
                "roles",
                "roles must use deterministic sorted order",
            ));
        }
        for (index, role) in self.roles.iter().enumerate() {
            role.validate()
                .map_err(|error| error.under(&format!("roles[{index}]")))?;
        }
        Ok(())
    }
}

fn validate_http_method(method: &str) -> Result<(), ContractError> {
    validate_text(method, "details.method", 32)?;
    if !method.bytes().all(|byte| {
        byte.is_ascii_uppercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
    }) {
        return Err(ContractError::new(
            ContractErrorCode::NonCanonicalValue,
            "details.method",
            "HTTP method must be a normalized uppercase token",
        ));
    }
    Ok(())
}

fn validate_route_path(path: &str) -> Result<(), ContractError> {
    validate_text(path, "details.path", 2048)?;
    if !path.starts_with('/') || path.chars().any(char::is_control) {
        return Err(ContractError::new(
            ContractErrorCode::NonCanonicalValue,
            "details.path",
            "HTTP route path must be an absolute, single-line route pattern",
        ));
    }
    Ok(())
}

impl FactNode {
    /// Computes a stable semantic node identity. Display name, source range,
    /// parent, evidence, roles, flags, and snapshot are intentionally excluded.
    pub fn stable_id(
        kind: FactNodeKind,
        language: Option<ProgrammingLanguage>,
        analysis_unit_id: Option<&AnalysisUnitId>,
        qualified_name: &str,
        signature: Option<&str>,
    ) -> Result<FactNodeId, ContractError> {
        validate_text(qualified_name, "qualifiedName", 4096)?;
        validate_optional_text(signature, "signature", 8192)?;
        let mut components = vec![format!("kind:{}", kind.as_str()), "language".to_string()];
        match language {
            Some(language) => {
                components.push("present".to_string());
                components.push(language.as_str().to_string());
            }
            None => components.push("absent".to_string()),
        }
        components.push("analysis_unit".to_string());
        match analysis_unit_id {
            Some(id) => {
                components.push("present".to_string());
                components.push(id.as_str().to_string());
            }
            None => components.push("absent".to_string()),
        }
        components.push(format!("qualified_name:{qualified_name}"));
        components.push("signature".to_string());
        match signature {
            Some(signature) => {
                components.push("present".to_string());
                components.push(signature.to_string());
            }
            None => components.push("absent".to_string()),
        }
        let component_refs = components.iter().map(String::as_str).collect::<Vec<_>>();
        FactNodeId::from_components(&component_refs).map_err(|error| {
            ContractError::new(
                ContractErrorCode::InvalidIdentifier,
                "id",
                error.to_string(),
            )
        })
    }
}

/// Broad edge family used for relation filtering and aggregation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactEdgeFamily {
    Structure,
    Code,
    Interface,
    Data,
    Integration,
    Verification,
}

/// Closed canonical relation vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactEdgeKind {
    Contains,
    Declares,
    BelongsTo,
    Imports,
    Exports,
    DependsOn,
    BuildOwns,
    Deploys,
    ServiceDependsOn,
    Calls,
    Constructs,
    Extends,
    Implements,
    MixesIn,
    Overrides,
    UsesType,
    Exposes,
    Handles,
    RoutesTo,
    MiddlewareBefore,
    FrontendActionCallsApi,
    Reads,
    Writes,
    ExecutesQuery,
    MapsToTable,
    MapsToColumn,
    MigrationChanges,
    ForeignKeyTo,
    DerivedFrom,
    Publishes,
    Consumes,
    Dispatches,
    CallsExternal,
    UsesCache,
    UsesFile,
    UsesConfig,
    Tests,
    AssertsContract,
}

impl FactEdgeKind {
    pub const fn family(self) -> FactEdgeFamily {
        match self {
            Self::Contains
            | Self::Declares
            | Self::BelongsTo
            | Self::Imports
            | Self::Exports
            | Self::DependsOn
            | Self::BuildOwns
            | Self::Deploys
            | Self::ServiceDependsOn => FactEdgeFamily::Structure,
            Self::Calls
            | Self::Constructs
            | Self::Extends
            | Self::Implements
            | Self::MixesIn
            | Self::Overrides
            | Self::UsesType => FactEdgeFamily::Code,
            Self::Exposes
            | Self::Handles
            | Self::RoutesTo
            | Self::MiddlewareBefore
            | Self::FrontendActionCallsApi => FactEdgeFamily::Interface,
            Self::Reads
            | Self::Writes
            | Self::ExecutesQuery
            | Self::MapsToTable
            | Self::MapsToColumn
            | Self::MigrationChanges
            | Self::ForeignKeyTo
            | Self::DerivedFrom => FactEdgeFamily::Data,
            Self::Publishes
            | Self::Consumes
            | Self::Dispatches
            | Self::CallsExternal
            | Self::UsesCache
            | Self::UsesFile
            | Self::UsesConfig => FactEdgeFamily::Integration,
            Self::Tests | Self::AssertsContract => FactEdgeFamily::Verification,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::Declares => "declares",
            Self::BelongsTo => "belongs_to",
            Self::Imports => "imports",
            Self::Exports => "exports",
            Self::DependsOn => "depends_on",
            Self::BuildOwns => "build_owns",
            Self::Deploys => "deploys",
            Self::ServiceDependsOn => "service_depends_on",
            Self::Calls => "calls",
            Self::Constructs => "constructs",
            Self::Extends => "extends",
            Self::Implements => "implements",
            Self::MixesIn => "mixes_in",
            Self::Overrides => "overrides",
            Self::UsesType => "uses_type",
            Self::Exposes => "exposes",
            Self::Handles => "handles",
            Self::RoutesTo => "routes_to",
            Self::MiddlewareBefore => "middleware_before",
            Self::FrontendActionCallsApi => "frontend_action_calls_api",
            Self::Reads => "reads",
            Self::Writes => "writes",
            Self::ExecutesQuery => "executes_query",
            Self::MapsToTable => "maps_to_table",
            Self::MapsToColumn => "maps_to_column",
            Self::MigrationChanges => "migration_changes",
            Self::ForeignKeyTo => "foreign_key_to",
            Self::DerivedFrom => "derived_from",
            Self::Publishes => "publishes",
            Self::Consumes => "consumes",
            Self::Dispatches => "dispatches",
            Self::CallsExternal => "calls_external",
            Self::UsesCache => "uses_cache",
            Self::UsesFile => "uses_file",
            Self::UsesConfig => "uses_config",
            Self::Tests => "tests",
            Self::AssertsContract => "asserts_contract",
        }
    }
}

/// Static truth classes. Unknown and AI candidates are intentionally absent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactTruth {
    Confirmed,
    Structural,
    StaticCandidate,
}

/// Deterministic authority that resolved a relation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionMethod {
    Compiler,
    Provider,
    ProjectModel,
    SyntaxExact,
    FrameworkAdapter,
    DatabaseReconciliation,
    Manifest,
}

/// Dispatch precision retained without expanding possible implementations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchKind {
    Direct,
    Virtual,
    Interface,
    Dynamic,
    /// The provider resolved the relation target but did not preserve enough
    /// dispatch metadata to classify the call without guessing.
    Unknown,
    NotApplicable,
}

/// Source-backed control context attached to one written call occurrence.
///
/// These flags describe lexical source structure, not observed runtime
/// behavior. In particular, `lexical_ordinal` below is never presented as a
/// guarantee that the call executes before another branch at runtime.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionControlContext {
    /// The call is inside a branch, short-circuit expression, handler, or
    /// another construct whose body is not unconditionally entered.
    pub guarded: bool,
    /// The call is lexically inside a loop and may execute more than once.
    pub repeated: bool,
    /// The call is inside an anonymous callback/closure owned by the enclosing
    /// named fact and is not an immediate synchronous hop from that owner.
    pub deferred: bool,
    /// The source explicitly awaits this call.
    pub awaited: bool,
}

/// One exact written call occurrence retained on a canonical code edge.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionOccurrence {
    pub call_site_evidence_id: EvidenceId,
    /// Zero-based lexical order among calls owned by the same named callable.
    pub lexical_ordinal: u32,
    pub control: ExecutionControlContext,
}

impl ExecutionOccurrence {
    pub(crate) fn validate_for(
        &self,
        kind: FactEdgeKind,
        evidence_ids: &[EvidenceId],
    ) -> Result<(), ContractError> {
        if !matches!(kind, FactEdgeKind::Calls | FactEdgeKind::Constructs) {
            return Err(ContractError::new(
                ContractErrorCode::NonCanonicalValue,
                "execution",
                "execution occurrence is valid only for calls and constructs",
            ));
        }
        if !evidence_ids.contains(&self.call_site_evidence_id) {
            return Err(ContractError::new(
                ContractErrorCode::MissingEvidence,
                "execution.callSiteEvidenceId",
                "execution occurrence must reference one of the edge evidence IDs",
            ));
        }
        Ok(())
    }
}

/// One canonical directed relation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactEdge {
    pub id: FactEdgeId,
    pub snapshot_id: SnapshotId,
    pub source_id: FactNodeId,
    pub target_id: FactNodeId,
    pub family: FactEdgeFamily,
    pub kind: FactEdgeKind,
    pub truth: FactTruth,
    pub resolution: ResolutionMethod,
    pub dispatch: DispatchKind,
    pub semantic_context_id: Option<SemanticContextId>,
    pub qualifier: Option<String>,
    /// Present on new canonical call/construct edges. `serde(default)` keeps
    /// historical snapshots readable, but TracePath treats a missing value as
    /// an unproven legacy execution hop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<ExecutionOccurrence>,
    pub evidence_ids: Vec<EvidenceId>,
}

impl Validate for FactEdge {
    fn validate(&self) -> Result<(), ContractError> {
        if self.family != self.kind.family() {
            return Err(ContractError::new(
                ContractErrorCode::NonCanonicalValue,
                "family",
                "edge family does not match canonical edge kind",
            ));
        }
        validate_optional_text(self.qualifier.as_deref(), "qualifier", 1024)?;
        if let Some(execution) = &self.execution {
            execution.validate_for(self.kind, &self.evidence_ids)?;
        }
        let expected = Self::stable_id(
            &self.source_id,
            &self.target_id,
            self.kind,
            self.semantic_context_id.as_ref(),
            self.qualifier.as_deref(),
            self.execution.as_ref(),
        )?;
        if self.id != expected {
            return Err(ContractError::new(
                ContractErrorCode::InvalidIdentifier,
                "id",
                "edge ID does not match its logical relation key",
            ));
        }
        if self.evidence_ids.is_empty() {
            return Err(ContractError::new(
                ContractErrorCode::MissingEvidence,
                "evidenceIds",
                "canonical edges require evidence for every truth class",
            ));
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

impl FactEdge {
    /// Computes the logical edge identity. Truth, resolution, dispatch, and
    /// general provenance evidence remain mergeable. An exact execution
    /// occurrence is identity input because two written calls between the same
    /// symbols must not collapse into one edge.
    pub fn stable_id(
        source_id: &FactNodeId,
        target_id: &FactNodeId,
        kind: FactEdgeKind,
        semantic_context_id: Option<&SemanticContextId>,
        qualifier: Option<&str>,
        execution: Option<&ExecutionOccurrence>,
    ) -> Result<FactEdgeId, ContractError> {
        validate_optional_text(qualifier, "qualifier", 1024)?;
        let mut components = vec![
            format!("source:{}", source_id.as_str()),
            format!("target:{}", target_id.as_str()),
            format!("kind:{}", kind.as_str()),
            "semantic_context".to_string(),
        ];
        match semantic_context_id {
            Some(id) => {
                components.push("present".to_string());
                components.push(id.as_str().to_string());
            }
            None => components.push("absent".to_string()),
        }
        components.push("qualifier".to_string());
        match qualifier {
            Some(qualifier) => {
                components.push("present".to_string());
                components.push(qualifier.to_string());
            }
            None => components.push("absent".to_string()),
        }
        if let Some(execution) = execution {
            // This suffix is deliberately absent for historical relations.
            // Therefore snapshots written before execution occurrences were
            // introduced keep their original IDs and remain verifiable.
            components.extend([
                "execution".to_string(),
                "present".to_string(),
                format!(
                    "call_site_evidence:{}",
                    execution.call_site_evidence_id.as_str()
                ),
                format!("lexical_ordinal:{}", execution.lexical_ordinal),
                format!("guarded:{}", execution.control.guarded),
                format!("repeated:{}", execution.control.repeated),
                format!("deferred:{}", execution.control.deferred),
                format!("awaited:{}", execution.control.awaited),
            ]);
        }
        let component_refs = components.iter().map(String::as_str).collect::<Vec<_>>();
        FactEdgeId::from_components(&component_refs).map_err(|error| {
            ContractError::new(
                ContractErrorCode::InvalidIdentifier,
                "id",
                error.to_string(),
            )
        })
    }
}

/// Immutable import-bundle manifest. `completed_at_unix_ms` is operational
/// metadata and is excluded when the producer computes `semantic_digest`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactBundleManifest {
    pub schema: ContractSchema,
    pub snapshot_id: SnapshotId,
    pub workspace_id: WorkspaceId,
    pub source_manifest_digest: Sha256Digest,
    pub config_digest: Sha256Digest,
    pub analysis_plan_digest: Sha256Digest,
    pub provider_set_digest: Sha256Digest,
    pub execution_context_set_digest: Sha256Digest,
    pub semantic_digest: Sha256Digest,
    pub bundle_digest: Sha256Digest,
    pub analysis_unit_receipt_count: u64,
    pub node_count: u64,
    pub edge_count: u64,
    pub evidence_count: u64,
    pub file_coverage_count: u64,
    pub source_scope_coverage_count: u64,
    pub capability_receipt_count: u64,
    pub gap_count: u64,
    pub issue_count: u64,
    pub completed_at_unix_ms: u64,
}

impl Validate for FactBundleManifest {
    fn validate(&self) -> Result<(), ContractError> {
        if self.schema != ContractSchema::CanonicalFactV1 {
            return Err(ContractError::new(
                ContractErrorCode::InvalidSchema,
                "schema",
                "fact bundle manifest requires the canonical-fact v1 schema",
            ));
        }
        let expected = SnapshotId::from_execution_inputs(
            &self.workspace_id,
            self.source_manifest_digest,
            self.analysis_plan_digest,
            self.provider_set_digest,
            self.execution_context_set_digest,
        )
        .map_err(|error| {
            ContractError::new(
                ContractErrorCode::InvalidIdentifier,
                "snapshotId",
                error.to_string(),
            )
        })?;
        if self.snapshot_id != expected {
            return Err(ContractError::new(
                ContractErrorCode::InvalidIdentifier,
                "snapshotId",
                "snapshot ID does not match workspace and semantic input digests",
            ));
        }
        Ok(())
    }
}

/// Typed rows that may appear in a canonical import bundle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "recordType",
    content = "record",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum CanonicalFactRecord {
    AnalysisUnitReceipt(AnalysisUnitReceipt),
    Evidence(FactEvidence),
    Node(FactNode),
    Edge(FactEdge),
    FileCoverage(FileCoverageRecord),
    SourceScopeCoverage(SourceScopeCoverageRecord),
    CapabilityReceipt(CapabilityReceipt),
    Gap(AnalysisGap),
    Issue(AnalysisIssue),
}

impl Validate for CanonicalFactRecord {
    fn validate(&self) -> Result<(), ContractError> {
        match self {
            Self::AnalysisUnitReceipt(record) => record.validate(),
            Self::Evidence(record) => record.validate(),
            Self::Node(record) => record.validate(),
            Self::Edge(record) => record.validate(),
            Self::FileCoverage(record) => record.validate(),
            Self::SourceScopeCoverage(record) => record.validate(),
            Self::CapabilityReceipt(record) => record.validate(),
            Self::Gap(record) => record.validate(),
            Self::Issue(record) => record.validate(),
        }
    }
}
