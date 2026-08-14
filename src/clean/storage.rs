use super::{CleanBundleError, CLEAN_POLICY_VERSION, CLEAN_SCHEMA_VERSION};
use crate::model::AnalysisResult;
use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn bundle_id(
    result: &AnalysisResult,
    policy_hash: &str,
    prepared_bytes: &[u8],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(CLEAN_SCHEMA_VERSION.as_bytes());
    hasher.update([0]);
    hasher.update(CLEAN_POLICY_VERSION.as_bytes());
    hasher.update([0]);
    hasher.update(env!("CARGO_PKG_VERSION").as_bytes());
    hasher.update([0]);
    hasher.update(result.project.project_id.as_bytes());
    hasher.update([0]);
    hasher.update(result.project.snapshot_id.as_bytes());
    hasher.update([0]);
    hasher.update(policy_hash.as_bytes());
    hasher.update([0]);
    hasher.update(prepared_bytes);
    format!("clean_{}", &sha256_hex(&hasher.finalize())[..32])
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(super) fn staging_directory(output: &Path) -> Result<PathBuf, CleanBundleError> {
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| io_error(parent, source))?;
    let name = output
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("clean");
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    Ok(parent.join(format!(".{name}.tmp-{}-{timestamp}", std::process::id())))
}

pub(super) fn replace_directory(
    staging: &Path,
    destination: &Path,
) -> Result<(), CleanBundleError> {
    if !destination.exists() {
        fs::rename(staging, destination).map_err(|source| io_error(destination, source))?;
        return Ok(());
    }

    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let backup = parent.join(format!(
        ".{}.backup-{}-{timestamp}",
        destination
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("clean"),
        std::process::id()
    ));
    fs::rename(destination, &backup).map_err(|source| io_error(destination, source))?;
    match fs::rename(staging, destination) {
        Ok(()) => {
            let _ = fs::remove_dir_all(backup);
            Ok(())
        }
        Err(source) => {
            let _ = fs::rename(&backup, destination);
            Err(io_error(destination, source))
        }
    }
}

pub(super) fn io_error(path: &Path, source: io::Error) -> CleanBundleError {
    CleanBundleError::Io {
        path: path.to_path_buf(),
        source,
    }
}
