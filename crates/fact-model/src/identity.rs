//! Deterministic identifiers and cryptographic digests.

use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest as _, Sha256};
use std::{fmt, str::FromStr};

const STABLE_ID_DOMAIN: &[u8] = b"codebase-workspace.stable-id.v1\0";
const PROVIDER_SYMBOL_NATIVE_DOMAIN: &[u8] = b"codebase-workspace.provider-symbol-native.v1\0";
const SHA256_HEX_LEN: usize = 64;
const PROVIDER_SYMBOL_MAX_BYTES: usize = 16_384;
const DERIVED_PROVIDER_SYMBOL_PREFIX: &str = "provider-symbol-digest-v1:";

/// Errors returned while constructing or parsing an identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdentityError {
    EmptyComponents,
    EmptyComponent,
    ComponentTooLong,
    InvalidControlCharacter,
    InvalidFormat,
    InvalidDigest,
}

impl fmt::Display for IdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::EmptyComponents => "at least one identity component is required",
            Self::EmptyComponent => "identity components must not be empty",
            Self::ComponentTooLong => "identity component exceeds 65535 UTF-8 bytes",
            Self::InvalidControlCharacter => {
                "identity component contains a forbidden control character"
            }
            Self::InvalidFormat => "identity does not use the canonical prefix and lowercase hex",
            Self::InvalidDigest => "digest must contain exactly 64 lowercase hexadecimal digits",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for IdentityError {}

/// A full SHA-256 digest serialized as lowercase hexadecimal.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sha256Digest([u8; 32]);

impl Sha256Digest {
    /// Hashes the supplied bytes.
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }

    /// Parses canonical lowercase hexadecimal.
    pub fn parse(value: &str) -> Result<Self, IdentityError> {
        if value.len() != SHA256_HEX_LEN
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(IdentityError::InvalidDigest);
        }
        let mut bytes = [0_u8; 32];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            bytes[index] = (hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?;
        }
        Ok(Self(bytes))
    }

    /// Returns the raw digest bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Returns canonical lowercase hexadecimal.
    pub fn to_hex(self) -> String {
        lower_hex(&self.0)
    }
}

impl fmt::Debug for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("Sha256Digest")
            .field(&self.to_hex())
            .finish()
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

impl FromStr for Sha256Digest {
    type Err = IdentityError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl Serialize for Sha256Digest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Sha256Digest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(D::Error::custom)
    }
}

macro_rules! define_stable_id {
    ($name:ident, $prefix:literal) => {
        #[doc = concat!("A deterministic ", $prefix, " identity.")]
        #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            /// Builds a domain-separated SHA-256 identity from length-prefixed
            /// canonical components.
            pub fn from_components(components: &[&str]) -> Result<Self, IdentityError> {
                stable_id($prefix, components).map(Self)
            }

            /// Parses an already-generated canonical identity.
            pub fn parse(value: impl Into<String>) -> Result<Self, IdentityError> {
                let value = value.into();
                validate_stable_id($prefix, &value)?;
                Ok(Self(value))
            }

            /// Returns the canonical serialized identity.
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
            type Err = IdentityError;

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

define_stable_id!(AnalysisUnitId, "unit");
define_stable_id!(SemanticContextId, "context");
define_stable_id!(FactNodeId, "node");
define_stable_id!(FactEdgeId, "edge");
define_stable_id!(EvidenceId, "evidence");
define_stable_id!(SnapshotId, "snapshot");

impl SnapshotId {
    /// Computes the semantic snapshot identity. Operational timestamps and
    /// output order are intentionally excluded.
    pub fn from_analysis_inputs(
        workspace_id: &WorkspaceId,
        source_manifest_digest: Sha256Digest,
        config_digest: Sha256Digest,
        provider_set_digest: Sha256Digest,
    ) -> Result<Self, IdentityError> {
        let source = source_manifest_digest.to_hex();
        let config = config_digest.to_hex();
        let providers = provider_set_digest.to_hex();
        Self::from_components(&[
            "workspace",
            workspace_id.as_str(),
            "source_manifest",
            &source,
            "config",
            &config,
            "provider_set",
            &providers,
        ])
    }

    /// Computes the snapshot identity used by the executed static pipeline.
    ///
    /// The analysis-plan digest seals source ownership and intended semantic
    /// contexts, while the execution-context-set digest seals the contexts the
    /// providers actually used. Keeping both prevents a planned build target
    /// from being published as though it were the target that really ran.
    pub fn from_execution_inputs(
        workspace_id: &WorkspaceId,
        source_manifest_digest: Sha256Digest,
        analysis_plan_digest: Sha256Digest,
        provider_set_digest: Sha256Digest,
        execution_context_set_digest: Sha256Digest,
    ) -> Result<Self, IdentityError> {
        let source = source_manifest_digest.to_hex();
        let plan = analysis_plan_digest.to_hex();
        let providers = provider_set_digest.to_hex();
        let execution_contexts = execution_context_set_digest.to_hex();
        Self::from_components(&[
            "workspace",
            workspace_id.as_str(),
            "source_manifest",
            &source,
            "analysis_plan",
            &plan,
            "provider_set",
            &providers,
            "execution_context_set",
            &execution_contexts,
        ])
    }
}

/// The existing desktop workspace identity. It remains shorter than fact IDs
/// because it is a local directory key, not a cross-table content identity.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorkspaceId(String);

impl WorkspaceId {
    pub fn parse(value: impl Into<String>) -> Result<Self, IdentityError> {
        let value = value.into();
        let Some(hex) = value.strip_prefix("ws-") else {
            return Err(IdentityError::InvalidFormat);
        };
        if hex.len() != 16
            || !hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(IdentityError::InvalidFormat);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for WorkspaceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("WorkspaceId").field(&self.0).finish()
    }
}

impl fmt::Display for WorkspaceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for WorkspaceId {
    type Err = IdentityError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl Serialize for WorkspaceId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for WorkspaceId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(D::Error::custom)
    }
}

/// Provider-native symbol identity. It is evidence used during normalization,
/// not a canonical FactNode ID.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProviderSymbolId(String);

impl ProviderSymbolId {
    pub fn parse(value: impl Into<String>) -> Result<Self, IdentityError> {
        let value = value.into();
        if value.is_empty() {
            return Err(IdentityError::EmptyComponent);
        }
        if value.len() > PROVIDER_SYMBOL_MAX_BYTES {
            return Err(IdentityError::ComponentTooLong);
        }
        if value.chars().any(char::is_control) {
            return Err(IdentityError::InvalidControlCharacter);
        }
        Ok(Self(value))
    }

    /// Converts an opaque provider-native identity into the persisted contract
    /// representation used by Language IR.
    ///
    /// Provider protocols are allowed to choose their own symbol grammar. Some
    /// real providers include multi-line source fragments in otherwise stable
    /// identities, while the persisted Fact contract deliberately forbids
    /// control characters. Ordinary contract-safe identities are retained for
    /// backward compatibility. Unsafe, oversized, or reserved-prefix values
    /// are mapped to a domain-labelled SHA-256 identity. Applying this at every
    /// provider boundary keeps definitions, parents, occurrences, and relation
    /// endpoints joinable without deleting or trimming identity bytes.
    pub fn from_provider_native(value: impl AsRef<str>) -> Result<Self, IdentityError> {
        let value = value.as_ref();
        if value.is_empty() {
            return Err(IdentityError::EmptyComponent);
        }
        if value.len() <= PROVIDER_SYMBOL_MAX_BYTES
            && !value.chars().any(char::is_control)
            && !value.starts_with(DERIVED_PROVIDER_SYMBOL_PREFIX)
        {
            return Ok(Self(value.to_string()));
        }

        let mut hasher = Sha256::new();
        hasher.update(PROVIDER_SYMBOL_NATIVE_DOMAIN);
        hasher.update((value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
        Ok(Self(format!(
            "{DERIVED_PROVIDER_SYMBOL_PREFIX}{}",
            lower_hex(&hasher.finalize())
        )))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ProviderSymbolId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ProviderSymbolId")
            .field(&self.0)
            .finish()
    }
}

impl fmt::Display for ProviderSymbolId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for ProviderSymbolId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ProviderSymbolId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(D::Error::custom)
    }
}

fn stable_id(prefix: &str, components: &[&str]) -> Result<String, IdentityError> {
    if components.is_empty() {
        return Err(IdentityError::EmptyComponents);
    }
    let mut hasher = Sha256::new();
    hasher.update(STABLE_ID_DOMAIN);
    write_hash_component(&mut hasher, prefix)?;
    for component in components {
        write_hash_component(&mut hasher, component)?;
    }
    Ok(format!("{prefix}-{}", lower_hex(&hasher.finalize())))
}

fn write_hash_component(hasher: &mut Sha256, component: &str) -> Result<(), IdentityError> {
    if component.is_empty() {
        return Err(IdentityError::EmptyComponent);
    }
    if component.len() > u16::MAX as usize {
        return Err(IdentityError::ComponentTooLong);
    }
    if component.chars().any(char::is_control) {
        return Err(IdentityError::InvalidControlCharacter);
    }
    hasher.update((component.len() as u16).to_be_bytes());
    hasher.update(component.as_bytes());
    Ok(())
}

fn validate_stable_id(prefix: &str, value: &str) -> Result<(), IdentityError> {
    let Some(hex) = value.strip_prefix(&format!("{prefix}-")) else {
        return Err(IdentityError::InvalidFormat);
    };
    if hex.len() != SHA256_HEX_LEN
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(IdentityError::InvalidFormat);
    }
    Ok(())
}

fn hex_nibble(byte: u8) -> Result<u8, IdentityError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(IdentityError::InvalidDigest),
    }
}

fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}
