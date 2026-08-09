//! Evidence records supporting canonical facts.

use crate::identity::{EvidenceId, Sha256Digest};
use crate::source::{RepositoryPath, SourceSpan};
use crate::validation::{
    validate_optional_text, validate_text, ContractError, ContractErrorCode, Validate,
};
use serde::{Deserialize, Serialize};

/// Why a piece of evidence is relevant to a fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    SourceDefinition,
    SourceReference,
    CallSite,
    ImportSite,
    TypeRelation,
    FrameworkRegistration,
    ManifestDeclaration,
    ContractDeclaration,
    QueryLiteral,
    DatabaseMetadata,
    ConfigurationDeclaration,
    DerivedStructural,
}

/// The class of deterministic producer that extracted the evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceProducerKind {
    Scip,
    LanguageServer,
    CompilerApi,
    SyntaxParser,
    FrameworkAdapter,
    AssetAdapter,
    DatabaseAdapter,
    StaticNormalizer,
}

/// Bounded producer provenance. Strategy is descriptive provenance only and
/// may never be parsed to infer truth or resolution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvidenceProducer {
    pub kind: EvidenceProducerKind,
    pub name: String,
    pub version: Option<String>,
    pub strategy: Option<String>,
}

impl Validate for EvidenceProducer {
    fn validate(&self) -> Result<(), ContractError> {
        validate_text(&self.name, "name", 256)?;
        validate_optional_text(self.version.as_deref(), "version", 256)?;
        validate_optional_text(self.strategy.as_deref(), "strategy", 256)?;
        Ok(())
    }
}

/// A location inside an executable repository artifact such as a manifest,
/// migration, OpenAPI file, protobuf file, or GraphQL schema.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactLocation {
    pub path: RepositoryPath,
    pub content_digest: Sha256Digest,
    pub pointer: Option<String>,
}

impl Validate for ArtifactLocation {
    fn validate(&self) -> Result<(), ContractError> {
        if self.path.is_root() {
            return Err(ContractError::new(
                ContractErrorCode::InvalidRepositoryPath,
                "path",
                "repository artifact evidence must point to a file",
            ));
        }
        validate_optional_text(self.pointer.as_deref(), "pointer", 2048)
    }
}

/// A metadata-only database catalog location. It intentionally contains no
/// credential or application row data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DatabaseCatalogLocation {
    pub source_kind: String,
    pub connection_alias: String,
    pub object_key: String,
    pub scope_digest: Sha256Digest,
}

impl Validate for DatabaseCatalogLocation {
    fn validate(&self) -> Result<(), ContractError> {
        validate_text(&self.source_kind, "sourceKind", 64)?;
        validate_text(&self.connection_alias, "connectionAlias", 256)?;
        validate_text(&self.object_key, "objectKey", 4096)?;
        Ok(())
    }
}

/// The closed evidence location families accepted by canonical facts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "locationType",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum EvidenceLocation {
    Source { span: SourceSpan },
    RepositoryArtifact { artifact: ArtifactLocation },
    DatabaseCatalog { catalog: DatabaseCatalogLocation },
}

impl Validate for EvidenceLocation {
    fn validate(&self) -> Result<(), ContractError> {
        match self {
            Self::Source { span } => span.validate().map_err(|error| error.under("span")),
            Self::RepositoryArtifact { artifact } => {
                artifact.validate().map_err(|error| error.under("artifact"))
            }
            Self::DatabaseCatalog { catalog } => {
                catalog.validate().map_err(|error| error.under("catalog"))
            }
        }
    }
}

/// Evidence row referenced by Language IR and canonical facts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactEvidence {
    pub id: EvidenceId,
    pub kind: EvidenceKind,
    pub producer: EvidenceProducer,
    pub location: EvidenceLocation,
    pub summary: Option<String>,
}

impl Validate for FactEvidence {
    fn validate(&self) -> Result<(), ContractError> {
        self.producer
            .validate()
            .map_err(|error| error.under("producer"))?;
        self.location
            .validate()
            .map_err(|error| error.under("location"))?;
        validate_optional_text(self.summary.as_deref(), "summary", 1024)?;
        let expected = Self::stable_id(self.kind, &self.producer, &self.location)?;
        if self.id != expected {
            return Err(ContractError::new(
                crate::validation::ContractErrorCode::InvalidIdentifier,
                "id",
                "evidence ID does not match producer and location",
            ));
        }
        Ok(())
    }
}

impl FactEvidence {
    /// Constructs evidence with its canonical stable ID.
    pub fn new(
        kind: EvidenceKind,
        producer: EvidenceProducer,
        location: EvidenceLocation,
        summary: Option<String>,
    ) -> Result<Self, ContractError> {
        let id = Self::stable_id(kind, &producer, &location)?;
        let evidence = Self {
            id,
            kind,
            producer,
            location,
            summary,
        };
        evidence.validate()?;
        Ok(evidence)
    }

    /// Computes the canonical evidence identity. Human summary text is
    /// intentionally excluded.
    pub fn stable_id(
        kind: EvidenceKind,
        producer: &EvidenceProducer,
        location: &EvidenceLocation,
    ) -> Result<EvidenceId, ContractError> {
        producer
            .validate()
            .map_err(|error| error.under("producer"))?;
        location
            .validate()
            .map_err(|error| error.under("location"))?;
        let mut components = vec![
            format!("kind:{}", kind.as_str()),
            format!("producer_kind:{}", producer.kind.as_str()),
            format!("producer_name:{}", producer.name),
            "producer_version".to_string(),
        ];
        match producer.version.as_deref() {
            Some(version) => {
                components.push("present".to_string());
                components.push(version.to_string());
            }
            None => components.push("absent".to_string()),
        }
        components.push("producer_strategy".to_string());
        match producer.strategy.as_deref() {
            Some(strategy) => {
                components.push("present".to_string());
                components.push(strategy.to_string());
            }
            None => components.push("absent".to_string()),
        }
        match location {
            EvidenceLocation::Source { span } => {
                components.extend([
                    "location:source".to_string(),
                    format!("path:{}", span.path.as_str()),
                    format!("digest:{}", span.content_digest),
                    format!(
                        "start:{}:{}:{}",
                        span.start.line, span.start.utf8_column, span.start.byte_offset
                    ),
                    format!(
                        "end:{}:{}:{}",
                        span.end.line, span.end.utf8_column, span.end.byte_offset
                    ),
                ]);
            }
            EvidenceLocation::RepositoryArtifact { artifact } => {
                components.extend([
                    "location:repository_artifact".to_string(),
                    format!("path:{}", artifact.path.as_str()),
                    format!("digest:{}", artifact.content_digest),
                    "pointer".to_string(),
                ]);
                match artifact.pointer.as_deref() {
                    Some(pointer) => {
                        components.push("present".to_string());
                        components.push(pointer.to_string());
                    }
                    None => components.push("absent".to_string()),
                }
            }
            EvidenceLocation::DatabaseCatalog { catalog } => {
                components.extend([
                    "location:database_catalog".to_string(),
                    format!("source_kind:{}", catalog.source_kind),
                    format!("connection_alias:{}", catalog.connection_alias),
                    format!("object_key:{}", catalog.object_key),
                    format!("scope_digest:{}", catalog.scope_digest),
                ]);
            }
        }
        let component_refs = components.iter().map(String::as_str).collect::<Vec<_>>();
        EvidenceId::from_components(&component_refs).map_err(|error| {
            ContractError::new(
                crate::validation::ContractErrorCode::InvalidIdentifier,
                "id",
                error.to_string(),
            )
        })
    }
}

impl EvidenceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourceDefinition => "source_definition",
            Self::SourceReference => "source_reference",
            Self::CallSite => "call_site",
            Self::ImportSite => "import_site",
            Self::TypeRelation => "type_relation",
            Self::FrameworkRegistration => "framework_registration",
            Self::ManifestDeclaration => "manifest_declaration",
            Self::ContractDeclaration => "contract_declaration",
            Self::QueryLiteral => "query_literal",
            Self::DatabaseMetadata => "database_metadata",
            Self::ConfigurationDeclaration => "configuration_declaration",
            Self::DerivedStructural => "derived_structural",
        }
    }
}

impl EvidenceProducerKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scip => "scip",
            Self::LanguageServer => "language_server",
            Self::CompilerApi => "compiler_api",
            Self::SyntaxParser => "syntax_parser",
            Self::FrameworkAdapter => "framework_adapter",
            Self::AssetAdapter => "asset_adapter",
            Self::DatabaseAdapter => "database_adapter",
            Self::StaticNormalizer => "static_normalizer",
        }
    }
}
