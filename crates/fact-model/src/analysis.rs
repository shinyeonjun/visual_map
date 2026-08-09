//! Provider, language, analysis-unit, and semantic-context contracts.

use crate::identity::{AnalysisUnitId, SemanticContextId, Sha256Digest, WorkspaceId};
use crate::source::RepositoryPath;
use crate::validation::{
    ensure_unique, validate_optional_text, validate_text, ContractError, ContractErrorCode,
    Validate,
};
use serde::{Deserialize, Serialize};

/// The ten source languages in the final support contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ProgrammingLanguage {
    #[serde(rename = "typescript")]
    TypeScript,
    #[serde(rename = "javascript")]
    JavaScript,
    #[serde(rename = "python")]
    Python,
    #[serde(rename = "java")]
    Java,
    #[serde(rename = "csharp")]
    CSharp,
    #[serde(rename = "c")]
    C,
    #[serde(rename = "cpp")]
    Cpp,
    #[serde(rename = "go")]
    Go,
    #[serde(rename = "rust")]
    Rust,
    #[serde(rename = "dart")]
    Dart,
}

impl ProgrammingLanguage {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TypeScript => "typescript",
            Self::JavaScript => "javascript",
            Self::Python => "python",
            Self::Java => "java",
            Self::CSharp => "csharp",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::Go => "go",
            Self::Rust => "rust",
            Self::Dart => "dart",
        }
    }
}

/// How a language provider communicates semantic results.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderProtocol {
    #[serde(rename = "scip")]
    Scip,
    #[serde(rename = "lsp")]
    LanguageServerProtocol,
    #[serde(rename = "compiler_api")]
    CompilerApi,
}

/// Where the provider executable was obtained.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderOrigin {
    ManagedBundle,
    Embedded,
    SystemPath,
    UserOverride,
}

/// Reproducible provider provenance. Paths and command lines are deliberately
/// excluded; the executed provider artifact is identified by content digest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderDescriptor {
    pub name: String,
    pub version: Option<String>,
    pub protocol: ProviderProtocol,
    pub origin: ProviderOrigin,
    pub artifact_digest: Sha256Digest,
}

impl Validate for ProviderDescriptor {
    fn validate(&self) -> Result<(), ContractError> {
        validate_text(&self.name, "name", 256)?;
        validate_optional_text(self.version.as_deref(), "version", 256)?;
        Ok(())
    }
}

/// The authority that gives a set of files one consistent semantic meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticContextKind {
    CompilerProject,
    Package,
    TranslationUnit,
    ExecutionEnvironment,
}

impl SemanticContextKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CompilerProject => "compiler_project",
            Self::Package => "package",
            Self::TranslationUnit => "translation_unit",
            Self::ExecutionEnvironment => "execution_environment",
        }
    }
}

/// Closed axes that can change static semantics without retaining raw config.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextDimensionKind {
    LanguageVersion,
    Platform,
    Architecture,
    Target,
    TargetFramework,
    Profile,
    Feature,
    BuildTag,
    ModuleMode,
    SourceSet,
}

impl ContextDimensionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LanguageVersion => "language_version",
            Self::Platform => "platform",
            Self::Architecture => "architecture",
            Self::Target => "target",
            Self::TargetFramework => "target_framework",
            Self::Profile => "profile",
            Self::Feature => "feature",
            Self::BuildTag => "build_tag",
            Self::ModuleMode => "module_mode",
            Self::SourceSet => "source_set",
        }
    }
}

/// One canonical, non-secret semantic context dimension.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextDimension {
    pub kind: ContextDimensionKind,
    pub value: String,
}

impl Validate for ContextDimension {
    fn validate(&self) -> Result<(), ContractError> {
        validate_text(&self.value, "value", 512)
    }
}

/// How the provider obtained the semantic project it actually executed.
///
/// This is deliberately separate from [`SemanticContext`]: that type records
/// what the planner intended, while this enum records the path the provider
/// really took (including honest fallback execution).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderExecutionMode {
    Project,
    InferredWorkspace,
    GeneratedProject,
    SourceOnlyFallback,
    /// One canonical analysis unit was covered by several explicit provider
    /// executions (for example deterministic large-workspace shards). The
    /// context contains the union of their source/config receipts and its
    /// generated digest commits to the sorted child fingerprints.
    Composite,
    NotExecuted,
}

impl ProviderExecutionMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::InferredWorkspace => "inferred_workspace",
            Self::GeneratedProject => "generated_project",
            Self::SourceOnlyFallback => "source_only_fallback",
            Self::Composite => "composite",
            Self::NotExecuted => "not_executed",
        }
    }
}

/// Why one repository configuration artifact belongs to an execution receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderConfigUse {
    ExplicitArgument,
    WorkspaceDiscovery,
    GeneratedLineage,
}

impl ProviderConfigUse {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExplicitArgument => "explicit_argument",
            Self::WorkspaceDiscovery => "workspace_discovery",
            Self::GeneratedLineage => "generated_lineage",
        }
    }
}

/// One non-secret repository file that constrained a provider execution.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderConfigArtifact {
    pub path: RepositoryPath,
    pub content_digest: Sha256Digest,
    pub usage: ProviderConfigUse,
}

impl Validate for ProviderConfigArtifact {
    fn validate(&self) -> Result<(), ContractError> {
        if self.path.is_root() {
            return Err(ContractError::new(
                ContractErrorCode::InvalidRepositoryPath,
                "path",
                "a provider configuration artifact must point to a file",
            ));
        }
        Ok(())
    }
}

/// Canonical receipt for the semantic context a provider actually executed.
///
/// Raw command lines, absolute paths, environment dumps, and secret-bearing
/// configuration are intentionally excluded. Unknown semantic dimensions are
/// represented by `missing_dimensions`; absence is never promoted to a value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderExecutionContext {
    pub mode: ProviderExecutionMode,
    pub analysis_root: Option<RepositoryPath>,
    pub source_scope_digest: Option<Sha256Digest>,
    pub source_file_count: u64,
    pub config_artifacts: Vec<ProviderConfigArtifact>,
    pub generated_context_digest: Option<Sha256Digest>,
    pub dimensions: Vec<ContextDimension>,
    pub missing_dimensions: Vec<ContextDimensionKind>,
    pub fingerprint: Sha256Digest,
}

impl ProviderExecutionContext {
    #[allow(clippy::too_many_arguments)]
    pub fn executed(
        mode: ProviderExecutionMode,
        analysis_root: RepositoryPath,
        source_scope_digest: Sha256Digest,
        source_file_count: u64,
        mut config_artifacts: Vec<ProviderConfigArtifact>,
        generated_context_digest: Option<Sha256Digest>,
        mut dimensions: Vec<ContextDimension>,
        mut missing_dimensions: Vec<ContextDimensionKind>,
    ) -> Result<Self, ContractError> {
        if mode == ProviderExecutionMode::NotExecuted {
            return Err(ContractError::new(
                ContractErrorCode::NonCanonicalValue,
                "mode",
                "an executed provider context may not use not_executed mode",
            ));
        }
        config_artifacts.sort();
        config_artifacts.dedup();
        dimensions.sort();
        dimensions.dedup();
        missing_dimensions.sort();
        missing_dimensions.dedup();
        let mut context = Self {
            mode,
            analysis_root: Some(analysis_root),
            source_scope_digest: Some(source_scope_digest),
            source_file_count,
            config_artifacts,
            generated_context_digest,
            dimensions,
            missing_dimensions,
            fingerprint: Sha256Digest::of_bytes(b"uninitialized provider execution context"),
        };
        context.fingerprint = provider_execution_context_fingerprint(&context);
        context.validate()?;
        Ok(context)
    }

    pub fn not_executed(
        mut missing_dimensions: Vec<ContextDimensionKind>,
    ) -> Result<Self, ContractError> {
        missing_dimensions.sort();
        missing_dimensions.dedup();
        let mut context = Self {
            mode: ProviderExecutionMode::NotExecuted,
            analysis_root: None,
            source_scope_digest: None,
            source_file_count: 0,
            config_artifacts: Vec::new(),
            generated_context_digest: None,
            dimensions: Vec::new(),
            missing_dimensions,
            fingerprint: Sha256Digest::of_bytes(b"uninitialized provider execution context"),
        };
        context.fingerprint = provider_execution_context_fingerprint(&context);
        context.validate()?;
        Ok(context)
    }
}

impl Validate for ProviderExecutionContext {
    fn validate(&self) -> Result<(), ContractError> {
        let not_executed = self.mode == ProviderExecutionMode::NotExecuted;
        if not_executed
            != (self.analysis_root.is_none()
                && self.source_scope_digest.is_none()
                && self.source_file_count == 0
                && self.config_artifacts.is_empty()
                && self.generated_context_digest.is_none()
                && self.dimensions.is_empty())
        {
            return Err(ContractError::new(
                ContractErrorCode::InvalidReceipt,
                "providerExecutionContext",
                "not_executed is the only mode without an executed root and source scope",
            ));
        }
        if !not_executed
            && (self.analysis_root.is_none()
                || self.source_scope_digest.is_none()
                || self.source_file_count == 0)
        {
            return Err(ContractError::new(
                ContractErrorCode::InvalidReceipt,
                "providerExecutionContext",
                "an executed context requires a root and a non-empty exact source scope",
            ));
        }
        ensure_unique(self.config_artifacts.iter(), "configArtifacts")?;
        ensure_unique(self.dimensions.iter(), "dimensions")?;
        ensure_unique(self.missing_dimensions.iter(), "missingDimensions")?;
        if !self
            .config_artifacts
            .windows(2)
            .all(|pair| pair[0] <= pair[1])
            || !self.dimensions.windows(2).all(|pair| pair[0] <= pair[1])
            || !self
                .missing_dimensions
                .windows(2)
                .all(|pair| pair[0] <= pair[1])
        {
            return Err(ContractError::new(
                ContractErrorCode::NonCanonicalValue,
                "providerExecutionContext",
                "configuration, dimensions, and missing dimensions must be sorted",
            ));
        }
        for (index, artifact) in self.config_artifacts.iter().enumerate() {
            artifact
                .validate()
                .map_err(|error| error.under(&format!("configArtifacts[{index}]")))?;
        }
        for (index, dimension) in self.dimensions.iter().enumerate() {
            dimension
                .validate()
                .map_err(|error| error.under(&format!("dimensions[{index}]")))?;
            if self.mode != ProviderExecutionMode::Composite
                && self.missing_dimensions.contains(&dimension.kind)
            {
                return Err(ContractError::new(
                    ContractErrorCode::NonCanonicalValue,
                    format!("dimensions[{index}].kind"),
                    "one context dimension cannot be both known and missing",
                ));
            }
        }
        let expected = provider_execution_context_fingerprint(self);
        if self.fingerprint != expected {
            return Err(ContractError::new(
                ContractErrorCode::InvalidIdentifier,
                "fingerprint",
                "provider execution fingerprint does not match its canonical fields",
            ));
        }
        Ok(())
    }
}

/// Canonical configuration fingerprint used by one analysis unit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticContext {
    pub id: SemanticContextId,
    pub kind: SemanticContextKind,
    pub fingerprint: Sha256Digest,
    pub config_files: Vec<RepositoryPath>,
    pub dimensions: Vec<ContextDimension>,
}

impl SemanticContext {
    pub fn new(
        kind: SemanticContextKind,
        fingerprint: Sha256Digest,
        mut config_files: Vec<RepositoryPath>,
        mut dimensions: Vec<ContextDimension>,
    ) -> Result<Self, ContractError> {
        config_files.sort();
        config_files.dedup();
        dimensions.sort();
        dimensions.dedup();
        let id = semantic_context_id(kind, fingerprint, &config_files, &dimensions)?;
        let context = Self {
            id,
            kind,
            fingerprint,
            config_files,
            dimensions,
        };
        context.validate()?;
        Ok(context)
    }
}

impl Validate for SemanticContext {
    fn validate(&self) -> Result<(), ContractError> {
        ensure_unique(self.config_files.iter(), "configFiles")?;
        ensure_unique(self.dimensions.iter(), "dimensions")?;
        if !self.config_files.windows(2).all(|pair| pair[0] <= pair[1])
            || !self.dimensions.windows(2).all(|pair| pair[0] <= pair[1])
        {
            return Err(ContractError::new(
                ContractErrorCode::NonCanonicalValue,
                "semanticContext",
                "config files and dimensions must use deterministic sorted order",
            ));
        }
        if let Some((index, _)) = self
            .config_files
            .iter()
            .enumerate()
            .find(|(_, path)| path.is_root())
        {
            return Err(ContractError::new(
                ContractErrorCode::InvalidRepositoryPath,
                format!("configFiles[{index}]"),
                "a configuration file must not be the repository root",
            ));
        }
        for (index, dimension) in self.dimensions.iter().enumerate() {
            dimension
                .validate()
                .map_err(|error| error.under(&format!("dimensions[{index}]")))?;
        }
        let expected = semantic_context_id(
            self.kind,
            self.fingerprint,
            &self.config_files,
            &self.dimensions,
        )?;
        if self.id != expected {
            return Err(ContractError::new(
                ContractErrorCode::InvalidIdentifier,
                "id",
                "semantic context ID does not match its canonical fields",
            ));
        }
        Ok(())
    }
}

/// One restartable provider unit with one semantic context.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisUnit {
    pub id: AnalysisUnitId,
    pub workspace_id: WorkspaceId,
    pub language: ProgrammingLanguage,
    pub root: RepositoryPath,
    pub context: SemanticContext,
    pub eligible_file_count: u64,
}

impl AnalysisUnit {
    pub fn new(
        workspace_id: WorkspaceId,
        language: ProgrammingLanguage,
        root: RepositoryPath,
        context: SemanticContext,
        eligible_file_count: u64,
    ) -> Result<Self, ContractError> {
        let id = analysis_unit_id(&workspace_id, language, &root, &context.id)?;
        let unit = Self {
            id,
            workspace_id,
            language,
            root,
            context,
            eligible_file_count,
        };
        unit.validate()?;
        Ok(unit)
    }
}

impl Validate for AnalysisUnit {
    fn validate(&self) -> Result<(), ContractError> {
        self.context
            .validate()
            .map_err(|error| error.under("context"))?;
        let expected = analysis_unit_id(
            &self.workspace_id,
            self.language,
            &self.root,
            &self.context.id,
        )?;
        if self.id != expected {
            return Err(ContractError::new(
                ContractErrorCode::InvalidIdentifier,
                "id",
                "analysis unit ID does not match workspace, language, root, and context",
            ));
        }
        Ok(())
    }
}

fn semantic_context_id(
    kind: SemanticContextKind,
    fingerprint: Sha256Digest,
    config_files: &[RepositoryPath],
    dimensions: &[ContextDimension],
) -> Result<SemanticContextId, ContractError> {
    let kind = kind.as_str().to_string();
    let digest = fingerprint.to_hex();
    let mut components = vec![kind, digest];
    components.extend(
        config_files
            .iter()
            .map(|path| format!("config:{}", path.as_str())),
    );
    components.extend(
        dimensions
            .iter()
            .map(|item| format!("dimension:{}:{}", item.kind.as_str(), item.value)),
    );
    let component_refs = components.iter().map(String::as_str).collect::<Vec<_>>();
    SemanticContextId::from_components(&component_refs).map_err(|error| {
        ContractError::new(
            ContractErrorCode::InvalidIdentifier,
            "semanticContext.id",
            error.to_string(),
        )
    })
}

fn provider_execution_context_fingerprint(context: &ProviderExecutionContext) -> Sha256Digest {
    const DOMAIN: &[u8] = b"codebase-workspace.provider-execution-context.v1\0";
    let mut bytes = Vec::new();
    append_digest_component(&mut bytes, DOMAIN);
    append_digest_component(&mut bytes, context.mode.as_str().as_bytes());
    append_digest_component(
        &mut bytes,
        context
            .analysis_root
            .as_ref()
            .map(RepositoryPath::as_str)
            .unwrap_or("-")
            .as_bytes(),
    );
    append_digest_component(
        &mut bytes,
        context
            .source_scope_digest
            .map(Sha256Digest::to_hex)
            .unwrap_or_else(|| "-".to_string())
            .as_bytes(),
    );
    append_digest_component(&mut bytes, &context.source_file_count.to_be_bytes());
    for artifact in &context.config_artifacts {
        append_digest_component(&mut bytes, b"config");
        append_digest_component(&mut bytes, artifact.path.as_str().as_bytes());
        append_digest_component(&mut bytes, artifact.usage.as_str().as_bytes());
        append_digest_component(&mut bytes, artifact.content_digest.to_hex().as_bytes());
    }
    append_digest_component(
        &mut bytes,
        context
            .generated_context_digest
            .map(Sha256Digest::to_hex)
            .unwrap_or_else(|| "-".to_string())
            .as_bytes(),
    );
    for dimension in &context.dimensions {
        append_digest_component(&mut bytes, b"dimension");
        append_digest_component(&mut bytes, dimension.kind.as_str().as_bytes());
        append_digest_component(&mut bytes, dimension.value.as_bytes());
    }
    for kind in &context.missing_dimensions {
        append_digest_component(&mut bytes, b"missing-dimension");
        append_digest_component(&mut bytes, kind.as_str().as_bytes());
    }
    Sha256Digest::of_bytes(&bytes)
}

fn append_digest_component(output: &mut Vec<u8>, value: &[u8]) {
    output.extend_from_slice(&(value.len() as u64).to_be_bytes());
    output.extend_from_slice(value);
}

fn analysis_unit_id(
    workspace_id: &WorkspaceId,
    language: ProgrammingLanguage,
    root: &RepositoryPath,
    context_id: &SemanticContextId,
) -> Result<AnalysisUnitId, ContractError> {
    AnalysisUnitId::from_components(&[
        workspace_id.as_str(),
        language.as_str(),
        root.as_str(),
        context_id.as_str(),
    ])
    .map_err(|error| {
        ContractError::new(
            ContractErrorCode::InvalidIdentifier,
            "analysisUnit.id",
            error.to_string(),
        )
    })
}
