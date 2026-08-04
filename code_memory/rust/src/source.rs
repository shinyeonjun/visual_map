use std::borrow::Cow;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::SourceSnapshot;
use crate::LANGUAGES;

pub(crate) fn collect_files(root: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files_recursive(root, extensions, &mut files);
    files.sort();
    files
}

pub(crate) fn canonical_project_root(root: &Path) -> Result<PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("invalid project root {}: {error}", root.display()))?;
    Ok(root
        .to_string_lossy()
        .strip_prefix("\\\\?\\")
        .map(PathBuf::from)
        .unwrap_or(root))
}

pub(crate) fn load_source_snapshot(root: &Path) -> SourceSnapshot {
    let mut extensions = HashSet::new();
    for language in LANGUAGES {
        extensions.extend(language.extensions.iter().copied());
    }
    extensions.insert("vue");
    let files = collect_files(root, &extensions.into_iter().collect::<Vec<_>>());
    load_source_snapshot_from_files(root, &files)
}

pub(crate) fn load_source_snapshot_from_files(root: &Path, files: &[PathBuf]) -> SourceSnapshot {
    let mut snapshot = load_source_snapshot_metadata_from_files(root, files);
    load_source_contents(root, &mut snapshot);
    snapshot
}

pub(crate) fn load_source_snapshot_metadata_from_files(
    root: &Path,
    files: &[PathBuf],
) -> SourceSnapshot {
    const MAX_SNAPSHOT_SOURCE_BYTES: u64 = 1_000_000;
    let sorted_files = if files.windows(2).all(|pair| pair[0] <= pair[1]) {
        Cow::Borrowed(files)
    } else {
        let mut sorted = files.to_vec();
        sorted.sort();
        Cow::Owned(sorted)
    };
    let mut file_hashes = std::collections::HashMap::new();
    let mut source_paths = Vec::new();
    for path in sorted_files.iter() {
        let Ok(metadata) = fs::metadata(path) else {
            continue;
        };
        let relative = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if metadata.len() > MAX_SNAPSHOT_SOURCE_BYTES {
            // ponytail: providers already exclude files above this limit;
            // preserve the tree while avoiding a second full-file read.
            file_hashes.insert(relative, metadata_fingerprint(&metadata));
            source_paths.push(path.clone());
            continue;
        }
        let Ok(bytes) = fs::read(path) else {
            continue;
        };
        file_hashes.insert(relative, source_hash(&bytes));
        source_paths.push(path.clone());
    }
    SourceSnapshot {
        files: Vec::new(),
        file_hashes,
        source_paths,
    }
}

pub(crate) fn load_source_contents(root: &Path, snapshot: &mut SourceSnapshot) {
    const MAX_SNAPSHOT_SOURCE_BYTES: u64 = 1_000_000;
    if snapshot.source_paths.is_empty() || !snapshot.files.is_empty() {
        return;
    }
    let paths = std::mem::take(&mut snapshot.source_paths);
    snapshot.files = paths
        .into_iter()
        .filter_map(|path| {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if fs::metadata(&path)
                .ok()
                .is_some_and(|metadata| metadata.len() > MAX_SNAPSHOT_SOURCE_BYTES)
            {
                // Keep the source boundary visible without loading a provider-
                // excluded generated/large file into memory.
                return Some((relative, String::new()));
            }
            Some((relative, fs::read_to_string(path).ok()?))
        })
        .collect();
}

fn metadata_fingerprint(metadata: &fs::Metadata) -> u64 {
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_nanos().to_le_bytes())
        .unwrap_or([0; 16]);
    let mut bytes = metadata.len().to_le_bytes().to_vec();
    bytes.extend_from_slice(&modified);
    source_hash(&bytes)
}

fn source_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub(crate) fn collect_files_recursive(dir: &Path, extensions: &[&str], files: &mut Vec<PathBuf>) {
    if is_managed_provider_root(dir) {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if !is_excluded_source_dir(&name) {
                collect_files_recursive(&path, extensions, files);
            }
        } else if file_type.is_file()
            && path
                .extension()
                .and_then(|value| value.to_str())
                .map(|ext| ext.to_ascii_lowercase())
                .is_some_and(|ext| extensions.iter().any(|candidate| *candidate == ext))
        {
            files.push(path);
        }
    }
}

pub(crate) fn is_managed_provider_root(path: &Path) -> bool {
    let manifest = path.join("manifest.json");
    let Ok(metadata) = fs::metadata(&manifest) else {
        return false;
    };
    metadata.len() <= 64 * 1024
        && fs::read_to_string(manifest)
            .ok()
            .is_some_and(|source| source.contains("\"code-memory.provider-manifest.v1\""))
}

pub(crate) fn is_excluded_source_dir(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    matches!(
        name.as_str(),
        ".git"
            | ".github"
            | ".dart_tool"
            | ".gradle"
            | ".idea"
            | ".pytest_cache"
            | ".ruby-lsp"
            | ".venv"
            | ".vscode"
            | ".storybook"
            | ".cache"
            | ".code_memory"
            | "__pycache__"
            | "build"
            | "coverage"
            | "dist"
            | "docs"
            | "generated"
            | "gen"
            | "node_modules"
            | "obj"
            | "out"
            | "target"
            | "tmp"
            | "vendor"
            | "venv"
    )
}
