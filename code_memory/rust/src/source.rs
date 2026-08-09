use std::borrow::Cow;
use std::collections::HashSet;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use codebase_fact_model::identity::Sha256Digest;
use codebase_fact_model::source_manifest::SourceEncoding;
use sha2::{Digest as _, Sha256};

use crate::SourceSnapshot;
use crate::LANGUAGES;

pub(crate) const DEFAULT_SOURCE_READ_BUFFER_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StreamedSourceMeasurement {
    pub(crate) byte_size: u64,
    pub(crate) line_count: Option<u64>,
    pub(crate) non_blank_line_count: Option<u64>,
    pub(crate) content_digest: Sha256Digest,
    pub(crate) cache_hash: u64,
    pub(crate) encoding: SourceEncoding,
}

/// Reads a source through one bounded buffer while calculating the full
/// content digest and UTF-8 line metrics. File size never changes whether the
/// file is measured; binary and invalid UTF-8 inputs are classified after the
/// complete byte stream has been observed.
pub(crate) fn measure_source_file(
    path: &Path,
    buffer_bytes: usize,
) -> io::Result<StreamedSourceMeasurement> {
    if buffer_bytes < 4 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "source read buffer must contain at least four bytes",
        ));
    }
    let file = fs::File::open(path)?;
    measure_source_reader(file, buffer_bytes)
}

fn measure_source_reader(
    mut reader: impl Read,
    buffer_bytes: usize,
) -> io::Result<StreamedSourceMeasurement> {
    let mut buffer = vec![0_u8; buffer_bytes];
    let mut sha256 = Sha256::new();
    let mut cache_hash = 0xcbf29ce484222325_u64;
    let mut byte_size = 0_u64;
    let mut has_nul = false;
    let mut prefix = Vec::with_capacity(3);
    let mut prefix_decided = false;
    let mut has_utf8_bom = false;
    let mut text = StreamingTextMetrics::default();

    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let bytes = &buffer[..read];
        byte_size = byte_size.checked_add(read as u64).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "source byte count overflow")
        })?;
        sha256.update(bytes);
        for byte in bytes {
            cache_hash ^= u64::from(*byte);
            cache_hash = cache_hash.wrapping_mul(0x100000001b3);
        }
        has_nul |= bytes.contains(&0);

        let mut offset = 0;
        if !prefix_decided {
            let needed = 3 - prefix.len();
            let take = needed.min(bytes.len());
            prefix.extend_from_slice(&bytes[..take]);
            offset = take;
            if prefix.len() == 3 {
                prefix_decided = true;
                has_utf8_bom = prefix == [0xef, 0xbb, 0xbf];
                if !has_utf8_bom {
                    text.push(&prefix);
                }
                prefix.clear();
            }
        }
        if prefix_decided && offset < bytes.len() {
            text.push(&bytes[offset..]);
        }
    }

    if !prefix_decided && !prefix.is_empty() {
        text.push(&prefix);
    }
    let text_metrics = text.finish();
    let encoding = if has_nul {
        SourceEncoding::Binary
    } else if text_metrics.is_none() {
        SourceEncoding::InvalidUtf8
    } else if has_utf8_bom {
        SourceEncoding::Utf8Bom
    } else {
        SourceEncoding::Utf8
    };
    let (line_count, non_blank_line_count) =
        if matches!(encoding, SourceEncoding::Utf8 | SourceEncoding::Utf8Bom) {
            let (line_count, non_blank_line_count) = text_metrics.expect("checked UTF-8 metrics");
            (Some(line_count), Some(non_blank_line_count))
        } else {
            (None, None)
        };
    let content_digest = Sha256Digest::parse(&format!("{:x}", sha256.finalize()))
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(StreamedSourceMeasurement {
        byte_size,
        line_count,
        non_blank_line_count,
        content_digest,
        cache_hash,
        encoding,
    })
}

#[derive(Default)]
struct StreamingTextMetrics {
    pending_utf8: Vec<u8>,
    invalid_utf8: bool,
    line_count: u64,
    non_blank_line_count: u64,
    current_line_has_content: bool,
    current_line_has_non_whitespace: bool,
}

impl StreamingTextMetrics {
    fn push(&mut self, bytes: &[u8]) {
        if self.invalid_utf8 || bytes.is_empty() {
            return;
        }
        if self.pending_utf8.is_empty() {
            self.push_contiguous(bytes);
            return;
        }
        let mut combined = Vec::with_capacity(self.pending_utf8.len() + bytes.len());
        combined.extend_from_slice(&self.pending_utf8);
        combined.extend_from_slice(bytes);
        self.pending_utf8.clear();
        self.push_contiguous(&combined);
    }

    fn push_contiguous(&mut self, bytes: &[u8]) {
        match std::str::from_utf8(bytes) {
            Ok(value) => self.push_text(value),
            Err(error) => {
                let valid = error.valid_up_to();
                if valid > 0 {
                    // SAFETY: `valid_up_to` is guaranteed to end on a UTF-8
                    // boundary for the prefix that preceded the error.
                    self.push_text(unsafe { std::str::from_utf8_unchecked(&bytes[..valid]) });
                }
                if error.error_len().is_some() {
                    self.invalid_utf8 = true;
                    self.pending_utf8.clear();
                } else {
                    self.pending_utf8.extend_from_slice(&bytes[valid..]);
                }
            }
        }
    }

    fn push_text(&mut self, value: &str) {
        for character in value.chars() {
            if character == '\n' {
                self.line_count += 1;
                self.non_blank_line_count += u64::from(self.current_line_has_non_whitespace);
                self.current_line_has_content = false;
                self.current_line_has_non_whitespace = false;
            } else {
                self.current_line_has_content = true;
                self.current_line_has_non_whitespace |= !character.is_whitespace();
            }
        }
    }

    fn finish(mut self) -> Option<(u64, u64)> {
        if !self.pending_utf8.is_empty() {
            self.invalid_utf8 = true;
        }
        if self.invalid_utf8 {
            return None;
        }
        if self.current_line_has_content {
            self.line_count += 1;
            self.non_blank_line_count += u64::from(self.current_line_has_non_whitespace);
        }
        Some((self.line_count, self.non_blank_line_count))
    }
}

pub(crate) fn collect_files(root: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files_recursive(root, extensions, &mut files);
    files.sort();
    files
}

pub(crate) fn canonical_project_root(root: &Path) -> Result<PathBuf, String> {
    canonical_existing_path(root)
        .map_err(|error| format!("invalid project root {}: {error}", root.display()))
}

/// Resolves one existing path into the representation used by repository
/// ownership checks. Windows extended-length prefixes are an OS transport
/// detail, not a second identity for the same file or directory.
pub(crate) fn canonical_existing_path(path: &Path) -> io::Result<PathBuf> {
    let canonical = path.canonicalize()?;
    Ok(canonical
        .to_string_lossy()
        .strip_prefix("\\\\?\\")
        .map(PathBuf::from)
        .unwrap_or(canonical))
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
        let Ok(measurement) = measure_source_file(path, DEFAULT_SOURCE_READ_BUFFER_BYTES) else {
            continue;
        };
        let relative = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        file_hashes.insert(relative, measurement.cache_hash);
        source_paths.push(path.clone());
    }
    SourceSnapshot {
        files: Vec::new(),
        file_hashes,
        source_paths,
    }
}

pub(crate) fn load_source_contents(root: &Path, snapshot: &mut SourceSnapshot) {
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
            Some((relative, fs::read_to_string(path).ok()?))
        })
        .collect();
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
            | "coverage"
            | "dist"
            | "docs"
            | "node_modules"
            | "obj"
            | "out"
            | "target"
            | "tmp"
            | "vendor"
            | "venv"
    )
}
