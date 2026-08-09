//! Canonical repository paths and source locations.

use crate::identity::Sha256Digest;
use crate::validation::{ContractError, ContractErrorCode, Validate};
use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use std::{fmt, str::FromStr};

/// A normalized repository-relative path using forward slashes. `.` denotes
/// the repository root and is not valid for a source span.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepositoryPath(String);

impl RepositoryPath {
    pub fn parse(value: impl Into<String>) -> Result<Self, ContractError> {
        let value = value.into();
        validate_repository_path(&value)?;
        Ok(Self(value))
    }

    pub fn root() -> Self {
        Self(".".to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_root(&self) -> bool {
        self.0 == "."
    }
}

impl fmt::Debug for RepositoryPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("RepositoryPath")
            .field(&self.0)
            .finish()
    }
}

impl fmt::Display for RepositoryPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for RepositoryPath {
    type Err = ContractError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl Serialize for RepositoryPath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for RepositoryPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(D::Error::custom)
    }
}

/// Canonical zero-based UTF-8 position. Provider-specific coordinates are
/// converted before entering the canonical contract.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourcePosition {
    pub line: u32,
    pub utf8_column: u32,
    pub byte_offset: u64,
}

/// A zero-based half-open source range tied to the exact file content digest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceSpan {
    pub path: RepositoryPath,
    pub content_digest: Sha256Digest,
    pub start: SourcePosition,
    pub end: SourcePosition,
}

impl Validate for SourceSpan {
    fn validate(&self) -> Result<(), ContractError> {
        if self.path.is_root() {
            return Err(ContractError::new(
                ContractErrorCode::InvalidRepositoryPath,
                "path",
                "a source span must point to a file, not the repository root",
            ));
        }
        if self.end.byte_offset < self.start.byte_offset
            || (self.end.line, self.end.utf8_column) < (self.start.line, self.start.utf8_column)
        {
            return Err(ContractError::new(
                ContractErrorCode::InvalidSourceRange,
                "end",
                "half-open range end must not precede start",
            ));
        }
        Ok(())
    }
}

/// Source classification used by census and relevance filtering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFileKind {
    Source,
    Test,
    Generated,
    Vendor,
    Config,
    Build,
    Deployment,
    Migration,
    Sql,
    Documentation,
    Other,
}

impl SourceFileKind {
    /// Returns the stable serialized name used in manifest digests.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Test => "test",
            Self::Generated => "generated",
            Self::Vendor => "vendor",
            Self::Config => "config",
            Self::Build => "build",
            Self::Deployment => "deployment",
            Self::Migration => "migration",
            Self::Sql => "sql",
            Self::Documentation => "documentation",
            Self::Other => "other",
        }
    }
}

/// Orthogonal source flags retained on canonical facts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceFlags {
    pub test: bool,
    pub generated: bool,
    pub vendor: bool,
    pub external: bool,
}

fn validate_repository_path(value: &str) -> Result<(), ContractError> {
    if value.is_empty() || value.len() > 4096 {
        return Err(ContractError::new(
            if value.is_empty() {
                ContractErrorCode::EmptyValue
            } else {
                ContractErrorCode::ValueTooLong
            },
            "repositoryPath",
            "repository path must contain 1 to 4096 UTF-8 bytes",
        ));
    }
    if value == "." {
        return Ok(());
    }
    if value.trim() != value
        || value.starts_with('/')
        || value.ends_with('/')
        || value.contains('\\')
        || value.contains(':')
        || value.contains("//")
        || value.chars().any(char::is_control)
        || value
            .split('/')
            .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
    {
        return Err(ContractError::new(
            ContractErrorCode::InvalidRepositoryPath,
            "repositoryPath",
            "path must be canonical, relative, slash-separated, and traversal-free",
        ));
    }
    Ok(())
}
