//! Deterministic repository census contract.

use crate::analysis::ProgrammingLanguage;
use crate::coverage::{GapCode, SourceScopeCoverageRecord};
use crate::identity::{Sha256Digest, WorkspaceId};
use crate::source::{RepositoryPath, SourceFileKind};
use crate::validation::{ensure_unique, ContractError, ContractErrorCode, Validate};
use crate::ContractSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

const SOURCE_MANIFEST_DIGEST_DOMAIN: &[u8] = b"codebase-workspace.source-manifest.digest.v1\0";

/// Whether a census file is eligible for later static analysis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceEntryState {
    Included,
    Excluded,
    Unsupported,
    Failed,
}

impl SourceEntryState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Included => "included",
            Self::Excluded => "excluded",
            Self::Unsupported => "unsupported",
            Self::Failed => "failed",
        }
    }
}

/// Encoding state observed without inventing text for unreadable files.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceEncoding {
    Utf8,
    Utf8Bom,
    Binary,
    InvalidUtf8,
    NotRead,
}

impl SourceEncoding {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Utf8 => "utf8",
            Self::Utf8Bom => "utf8_bom",
            Self::Binary => "binary",
            Self::InvalidUtf8 => "invalid_utf8",
            Self::NotRead => "not_read",
        }
    }
}

/// Filesystem link state. Census never follows a link as analysis input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceLinkState {
    Regular,
    SymlinkWithinRoot,
    SymlinkEscapesRoot,
    BrokenSymlink,
}

impl SourceLinkState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Regular => "regular",
            Self::SymlinkWithinRoot => "symlink_within_root",
            Self::SymlinkEscapesRoot => "symlink_escapes_root",
            Self::BrokenSymlink => "broken_symlink",
        }
    }
}

/// One explicitly enumerated repository file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceManifestFile {
    pub path: RepositoryPath,
    pub languages: Vec<ProgrammingLanguage>,
    pub file_kind: SourceFileKind,
    pub state: SourceEntryState,
    pub byte_size: u64,
    pub line_count: Option<u64>,
    pub non_blank_line_count: Option<u64>,
    pub content_digest: Option<Sha256Digest>,
    pub encoding: SourceEncoding,
    pub link_state: SourceLinkState,
    pub gap_codes: Vec<GapCode>,
}

impl Validate for SourceManifestFile {
    fn validate(&self) -> Result<(), ContractError> {
        if self.path.is_root() {
            return Err(ContractError::new(
                ContractErrorCode::InvalidRepositoryPath,
                "path",
                "a source-manifest file must not be the repository root",
            ));
        }
        ensure_unique(self.languages.iter(), "languages")?;
        ensure_unique(self.gap_codes.iter(), "gapCodes")?;
        if !self.languages.windows(2).all(|pair| pair[0] <= pair[1])
            || !self.gap_codes.windows(2).all(|pair| pair[0] <= pair[1])
        {
            return Err(ContractError::new(
                ContractErrorCode::NonCanonicalValue,
                "sourceManifestFile",
                "languages and gap codes must use deterministic sorted order",
            ));
        }
        if matches!(
            self.file_kind,
            SourceFileKind::Source | SourceFileKind::Test | SourceFileKind::Generated
        ) && self.languages.is_empty()
        {
            return Err(ContractError::new(
                ContractErrorCode::InvalidReceipt,
                "languages",
                "source, test, and generated files require a recognized language",
            ));
        }
        match (self.line_count, self.non_blank_line_count) {
            (Some(lines), Some(non_blank)) if non_blank <= lines => {}
            (None, None) => {}
            _ => {
                return Err(ContractError::new(
                    ContractErrorCode::InvalidReceipt,
                    "lineCount",
                    "line counts must be present together and non-blank lines may not exceed all lines",
                ));
            }
        }
        if matches!(
            self.encoding,
            SourceEncoding::Binary | SourceEncoding::NotRead
        ) && self.line_count.is_some()
        {
            return Err(ContractError::new(
                ContractErrorCode::InvalidReceipt,
                "lineCount",
                "binary or unread source must not claim text line counts",
            ));
        }
        if self.state == SourceEntryState::Included {
            if self.link_state != SourceLinkState::Regular
                || self.content_digest.is_none()
                || self.line_count.is_none()
                || !matches!(
                    self.encoding,
                    SourceEncoding::Utf8 | SourceEncoding::Utf8Bom
                )
                || !self.gap_codes.is_empty()
            {
                return Err(ContractError::new(
                    ContractErrorCode::InvalidReceipt,
                    "sourceManifestFile",
                    "included files must be regular, readable UTF-8 with a digest, line counts, and no gap",
                ));
            }
        } else if self.gap_codes.is_empty() {
            return Err(ContractError::new(
                ContractErrorCode::InvalidReceipt,
                "gapCodes",
                "excluded, unsupported, or failed files require a stable gap code",
            ));
        }
        Ok(())
    }
}

/// Immutable census input for analysis-unit planning and snapshot identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceManifest {
    pub schema: ContractSchema,
    pub workspace_id: WorkspaceId,
    pub files: Vec<SourceManifestFile>,
    pub scopes: Vec<SourceScopeCoverageRecord>,
    pub manifest_digest: Sha256Digest,
}

impl SourceManifest {
    /// Canonicalizes ordering and computes the cryptographic semantic digest.
    pub fn new(
        workspace_id: WorkspaceId,
        mut files: Vec<SourceManifestFile>,
        mut scopes: Vec<SourceScopeCoverageRecord>,
    ) -> Result<Self, ContractError> {
        for file in &mut files {
            file.languages.sort();
            file.gap_codes.sort();
        }
        for scope in &mut scopes {
            scope.gap_codes.sort();
        }
        files.sort_by(|left, right| left.path.cmp(&right.path));
        scopes.sort_by(|left, right| left.path.cmp(&right.path));
        let mut manifest = Self {
            schema: ContractSchema::SourceManifestV1,
            workspace_id,
            files,
            scopes,
            manifest_digest: Sha256Digest::of_bytes(b"uninitialized source manifest"),
        };
        manifest.manifest_digest = manifest.expected_digest();
        manifest.validate()?;
        Ok(manifest)
    }

    /// Recomputes the digest from canonical semantic fields.
    pub fn expected_digest(&self) -> Sha256Digest {
        let mut hasher = Sha256::new();
        hasher.update(SOURCE_MANIFEST_DIGEST_DOMAIN);
        hash_text(&mut hasher, self.schema.as_str());
        hash_text(&mut hasher, self.workspace_id.as_str());
        for file in &self.files {
            hash_text(&mut hasher, "file");
            hash_text(&mut hasher, file.path.as_str());
            for language in &file.languages {
                hash_text(&mut hasher, language.as_str());
            }
            hash_text(&mut hasher, file.file_kind.as_str());
            hash_text(&mut hasher, file.state.as_str());
            hash_u64(&mut hasher, file.byte_size);
            hash_optional_u64(&mut hasher, file.line_count);
            hash_optional_u64(&mut hasher, file.non_blank_line_count);
            hash_text(
                &mut hasher,
                file.content_digest
                    .map(Sha256Digest::to_hex)
                    .as_deref()
                    .unwrap_or("-"),
            );
            hash_text(&mut hasher, file.encoding.as_str());
            hash_text(&mut hasher, file.link_state.as_str());
            for gap in &file.gap_codes {
                hash_text(&mut hasher, gap.as_str());
            }
        }
        for scope in &self.scopes {
            hash_text(&mut hasher, "scope");
            hash_text(&mut hasher, scope.path.as_str());
            hash_text(&mut hasher, scope.state.as_str());
            hasher.update([u8::from(scope.descendants_enumerated)]);
            for gap in &scope.gap_codes {
                hash_text(&mut hasher, gap.as_str());
            }
        }
        Sha256Digest::of_bytes(&hasher.finalize())
    }
}

impl Validate for SourceManifest {
    fn validate(&self) -> Result<(), ContractError> {
        if self.schema != ContractSchema::SourceManifestV1 {
            return Err(ContractError::new(
                ContractErrorCode::InvalidSchema,
                "schema",
                "source manifest requires the source-manifest v1 schema",
            ));
        }
        ensure_unique(self.files.iter().map(|file| &file.path), "files.path")?;
        ensure_unique(self.scopes.iter().map(|scope| &scope.path), "scopes.path")?;
        if !self
            .files
            .windows(2)
            .all(|pair| pair[0].path <= pair[1].path)
            || !self
                .scopes
                .windows(2)
                .all(|pair| pair[0].path <= pair[1].path)
        {
            return Err(ContractError::new(
                ContractErrorCode::NonCanonicalValue,
                "sourceManifest",
                "files and scopes must use deterministic path order",
            ));
        }
        for (index, file) in self.files.iter().enumerate() {
            file.validate()
                .map_err(|error| error.under(&format!("files[{index}]")))?;
            if self.scopes.iter().any(|scope| {
                !scope.descendants_enumerated && path_is_within(&file.path, &scope.path)
            }) {
                return Err(ContractError::new(
                    ContractErrorCode::InvalidReceipt,
                    format!("files[{index}].path"),
                    "a file cannot be enumerated inside a scope declared non-enumerated",
                ));
            }
        }
        for (index, scope) in self.scopes.iter().enumerate() {
            scope
                .validate()
                .map_err(|error| error.under(&format!("scopes[{index}]")))?;
        }
        if self.manifest_digest != self.expected_digest() {
            return Err(ContractError::new(
                ContractErrorCode::InvalidDigest,
                "manifestDigest",
                "source manifest digest does not match its canonical fields",
            ));
        }
        Ok(())
    }
}

fn path_is_within(path: &RepositoryPath, scope: &RepositoryPath) -> bool {
    if scope.is_root() {
        return true;
    }
    path.as_str() == scope.as_str()
        || path
            .as_str()
            .strip_prefix(scope.as_str())
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn hash_text(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value.as_bytes());
}

fn hash_u64(hasher: &mut Sha256, value: u64) {
    hasher.update(value.to_be_bytes());
}

fn hash_optional_u64(hasher: &mut Sha256, value: Option<u64>) {
    match value {
        Some(value) => {
            hasher.update([1]);
            hash_u64(hasher, value);
        }
        None => hasher.update([0]),
    }
}
