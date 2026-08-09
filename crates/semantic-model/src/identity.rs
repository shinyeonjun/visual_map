//! Deterministic identities owned by the semantic/read-model boundary.

use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest as _, Sha256};
use std::{fmt, str::FromStr};

const SEMANTIC_ID_DOMAIN: &[u8] = b"codebase-workspace.semantic-id.v1\0";
const SHA256_HEX_LEN: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticIdError {
    EmptyComponents,
    EmptyComponent,
    ComponentTooLong,
    InvalidControlCharacter,
    InvalidFormat,
    InvalidProposalKey,
}

impl fmt::Display for SemanticIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EmptyComponents => "at least one identity component is required",
            Self::EmptyComponent => "identity components must not be empty",
            Self::ComponentTooLong => "identity component exceeds 65535 UTF-8 bytes",
            Self::InvalidControlCharacter => {
                "identity component contains a forbidden control character"
            }
            Self::InvalidFormat => "identity does not use its canonical prefix and lowercase hex",
            Self::InvalidProposalKey => {
                "proposal key must start with a lowercase letter and contain only lowercase ASCII letters, digits, '_' or '-'"
            }
        })
    }
}

impl std::error::Error for SemanticIdError {}

macro_rules! define_semantic_id {
    ($name:ident, $prefix:literal) => {
        #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn from_components(components: &[&str]) -> Result<Self, SemanticIdError> {
                semantic_id($prefix, components).map(Self)
            }

            pub fn parse(value: impl Into<String>) -> Result<Self, SemanticIdError> {
                let value = value.into();
                validate_semantic_id($prefix, &value)?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_tuple(stringify!($name))
                    .field(&self.0)
                    .finish()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = SemanticIdError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::parse(value)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::parse(value).map_err(D::Error::custom)
            }
        }
    };
}

define_semantic_id!(RegionId, "region");
define_semantic_id!(RelationBundleId, "bundle");
define_semantic_id!(TracePathId, "trace");
define_semantic_id!(SemanticAreaId, "area");
define_semantic_id!(SemanticRevisionId, "semantic-revision");

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct ProposalKey(String);

impl ProposalKey {
    pub fn parse(value: impl Into<String>) -> Result<Self, SemanticIdError> {
        let value = value.into();
        let mut bytes = value.bytes();
        let valid = (1..=64).contains(&value.len())
            && bytes.next().is_some_and(|byte| byte.is_ascii_lowercase())
            && bytes.all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
            });
        valid
            .then_some(Self(value))
            .ok_or(SemanticIdError::InvalidProposalKey)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ProposalKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("ProposalKey").field(&self.0).finish()
    }
}

impl fmt::Display for ProposalKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ProposalKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(D::Error::custom)
    }
}

fn semantic_id(prefix: &str, components: &[&str]) -> Result<String, SemanticIdError> {
    if components.is_empty() {
        return Err(SemanticIdError::EmptyComponents);
    }
    let mut hasher = Sha256::new();
    hasher.update(SEMANTIC_ID_DOMAIN);
    hasher.update((prefix.len() as u16).to_be_bytes());
    hasher.update(prefix.as_bytes());
    for component in components {
        validate_component(component)?;
        hasher.update((component.len() as u16).to_be_bytes());
        hasher.update(component.as_bytes());
    }
    Ok(format!("{prefix}-{}", lower_hex(&hasher.finalize())))
}

fn validate_component(component: &str) -> Result<(), SemanticIdError> {
    if component.is_empty() {
        return Err(SemanticIdError::EmptyComponent);
    }
    if component.len() > usize::from(u16::MAX) {
        return Err(SemanticIdError::ComponentTooLong);
    }
    if component.chars().any(char::is_control) {
        return Err(SemanticIdError::InvalidControlCharacter);
    }
    Ok(())
}

fn validate_semantic_id(prefix: &str, value: &str) -> Result<(), SemanticIdError> {
    let expected_prefix = format!("{prefix}-");
    let Some(hex) = value.strip_prefix(&expected_prefix) else {
        return Err(SemanticIdError::InvalidFormat);
    };
    if hex.len() != SHA256_HEX_LEN
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(SemanticIdError::InvalidFormat);
    }
    Ok(())
}

fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_ids_are_deterministic_and_domain_separated() {
        let region = RegionId::from_components(&["workspace", "orders"]).unwrap();
        let region_again = RegionId::from_components(&["workspace", "orders"]).unwrap();
        let trace = TracePathId::from_components(&["workspace", "orders"]).unwrap();

        assert_eq!(region, region_again);
        assert_ne!(region.as_str(), trace.as_str());
        assert!(region.as_str().starts_with("region-"));
    }

    #[test]
    fn proposal_keys_are_small_and_machine_safe() {
        assert!(ProposalKey::parse("orders-create").is_ok());
        assert!(ProposalKey::parse("Orders").is_err());
        assert!(ProposalKey::parse("1-orders").is_err());
        assert!(ProposalKey::parse("orders.create").is_err());
    }
}
