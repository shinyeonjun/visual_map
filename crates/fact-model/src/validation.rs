//! Fail-closed validation shared by all contract records.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Stable categories for malformed contract data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractErrorCode {
    EmptyValue,
    ValueTooLong,
    InvalidControlCharacter,
    NonCanonicalValue,
    InvalidIdentifier,
    InvalidDigest,
    InvalidRepositoryPath,
    InvalidSourceRange,
    InvalidSchema,
    InvalidReceipt,
    MissingEvidence,
    DuplicateValue,
    StreamOrder,
}

/// A deterministic validation failure. Machine decisions use `code`; the
/// message exists for diagnostics and may be refined without changing meaning.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContractError {
    pub code: ContractErrorCode,
    pub path: String,
    pub message: String,
}

impl ContractError {
    pub(crate) fn new(
        code: ContractErrorCode,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            path: path.into(),
            message: message.into(),
        }
    }

    /// Prefixes the field path while retaining the stable error category.
    pub fn under(mut self, prefix: &str) -> Self {
        self.path = if self.path.is_empty() {
            prefix.to_string()
        } else {
            format!("{prefix}.{}", self.path)
        };
        self
    }
}

impl fmt::Display for ContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:?} at {}: {}",
            self.code, self.path, self.message
        )
    }
}

impl std::error::Error for ContractError {}

/// Implemented by records that have cross-field contract invariants.
pub trait Validate {
    fn validate(&self) -> Result<(), ContractError>;
}

pub(crate) fn validate_text(
    value: &str,
    path: &str,
    max_bytes: usize,
) -> Result<(), ContractError> {
    if value.is_empty() {
        return Err(ContractError::new(
            ContractErrorCode::EmptyValue,
            path,
            "value must not be empty",
        ));
    }
    if value.len() > max_bytes {
        return Err(ContractError::new(
            ContractErrorCode::ValueTooLong,
            path,
            format!("value exceeds {max_bytes} UTF-8 bytes"),
        ));
    }
    if value.trim() != value {
        return Err(ContractError::new(
            ContractErrorCode::NonCanonicalValue,
            path,
            "leading or trailing whitespace is not canonical",
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(ContractError::new(
            ContractErrorCode::InvalidControlCharacter,
            path,
            "value contains a forbidden control character",
        ));
    }
    Ok(())
}

pub(crate) fn validate_message(
    value: &str,
    path: &str,
    max_bytes: usize,
) -> Result<(), ContractError> {
    if value.is_empty() {
        return Err(ContractError::new(
            ContractErrorCode::EmptyValue,
            path,
            "message must not be empty",
        ));
    }
    if value.len() > max_bytes {
        return Err(ContractError::new(
            ContractErrorCode::ValueTooLong,
            path,
            format!("message exceeds {max_bytes} UTF-8 bytes"),
        ));
    }
    if value.trim() != value {
        return Err(ContractError::new(
            ContractErrorCode::NonCanonicalValue,
            path,
            "leading or trailing whitespace is not canonical",
        ));
    }
    if value
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(ContractError::new(
            ContractErrorCode::InvalidControlCharacter,
            path,
            "message contains a forbidden control character",
        ));
    }
    Ok(())
}

pub(crate) fn validate_optional_message(
    value: Option<&str>,
    path: &str,
    max_bytes: usize,
) -> Result<(), ContractError> {
    if let Some(value) = value {
        validate_message(value, path, max_bytes)?;
    }
    Ok(())
}

pub(crate) fn validate_optional_text(
    value: Option<&str>,
    path: &str,
    max_bytes: usize,
) -> Result<(), ContractError> {
    if let Some(value) = value {
        validate_text(value, path, max_bytes)?;
    }
    Ok(())
}

pub(crate) fn ensure_unique<T>(
    values: impl IntoIterator<Item = T>,
    path: &str,
) -> Result<(), ContractError>
where
    T: Ord,
{
    let mut seen = std::collections::BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(ContractError::new(
                ContractErrorCode::DuplicateValue,
                path,
                "collection contains a duplicate value",
            ));
        }
    }
    Ok(())
}
