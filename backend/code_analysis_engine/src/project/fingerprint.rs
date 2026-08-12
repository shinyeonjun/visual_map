//! 파일 인벤토리 요약과 안정적인 식별자 계산을 담당한다.

use crate::config::LanguageRegistry;
use crate::model::{FileEntry, ProjectSummary};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn summarize(
    files: &[FileEntry],
    language_registry: &LanguageRegistry,
) -> ProjectSummary {
    let mut languages = BTreeMap::new();
    let mut total_bytes = 0_u64;
    let mut total_lines = 0_u64;

    for file in files {
        *languages
            .entry(language_registry.key(file.language))
            .or_insert(0) += 1;
        total_bytes += file.size_bytes;
        total_lines += file.line_count;
    }

    ProjectSummary {
        total_files: files.len(),
        total_bytes,
        total_lines,
        source_files: files.len(),
        languages,
    }
}

pub(crate) fn fingerprint_snapshot(files: &[FileEntry]) -> String {
    let mut hasher = Sha256::new();
    for file in files {
        hasher.update(file.relative_path.as_bytes());
        hasher.update([0]);
        if let Some(content_hash) = &file.content_hash {
            hasher.update(content_hash.as_bytes());
        } else {
            hasher.update(file.size_bytes.to_le_bytes());
            hasher.update(file.modified_unix_ms.unwrap_or_default().to_le_bytes());
        }
        hasher.update([0]);
    }
    format!("snapshot_{}", hex_digest(hasher.finalize()))
}

pub(crate) fn stable_id(prefix: &str, value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{prefix}_{}", &hex_digest(hasher.finalize())[..24])
}

pub(crate) fn runtime_id(prefix: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    stable_id(
        prefix,
        &format!("{}:{}", now.as_nanos(), std::process::id()),
    )
}

pub(crate) fn hex_digest(digest: impl AsRef<[u8]>) -> String {
    digest
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(crate) fn modified_unix_ms(modified: &Option<SystemTime>) -> Option<u64> {
    modified
        .as_ref()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
}

pub(crate) fn elapsed_millis(started: SystemTime) -> u64 {
    started
        .elapsed()
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

pub(crate) fn normalized_path(path: &std::path::Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    normalized
        .strip_prefix("//?/")
        .unwrap_or(&normalized)
        .to_string()
}
