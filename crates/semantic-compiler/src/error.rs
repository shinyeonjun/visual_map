use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticCompileErrorCode {
    InvalidSchema,
    InvalidPacket,
    InvalidText,
    DuplicateIdentifier,
    MissingReference,
    InvalidHierarchy,
    IncompleteAssignment,
    UnexpectedReference,
    EvidenceMismatch,
    ContradictoryFallback,
    DigestMismatch,
    InvalidProviderOutput,
    NonCanonicalValue,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticCompileError {
    pub code: SemanticCompileErrorCode,
    pub path: String,
    pub message: String,
}

impl SemanticCompileError {
    pub(crate) fn new(
        code: SemanticCompileErrorCode,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            path: path.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for SemanticCompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:?} at {}: {}",
            self.code, self.path, self.message
        )
    }
}

impl std::error::Error for SemanticCompileError {}
