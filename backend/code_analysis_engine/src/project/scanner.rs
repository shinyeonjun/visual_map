//! 프로젝트 파일을 순회하고 언어별 파일 인벤토리를 만든다.

use super::fingerprint::{
    elapsed_millis, fingerprint_snapshot, hex_digest, modified_unix_ms, normalized_path,
    runtime_id, stable_id, summarize,
};
use super::model::{FileReadIssue, ScanOutput};
use crate::diagnostics::Diagnostic;
use crate::model::{AnalysisOptions, AnalysisStatus, FileEntry, Language, ProjectContext};
use crate::EngineError;
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// 프로젝트 경로를 읽고 이후 분석기가 사용할 파일 인벤토리를 만드는 컴포넌트다.
pub struct ProjectScanner {
    options: AnalysisOptions,
}

impl ProjectScanner {
    pub fn new(options: AnalysisOptions) -> Self {
        Self { options }
    }

    pub fn scan(&self, requested_root: &Path) -> Result<ScanOutput, EngineError> {
        validate_root(requested_root)?;
        let root = requested_root
            .canonicalize()
            .map_err(|source| EngineError::Canonicalize {
                path: requested_root.to_path_buf(),
                source,
            })?;

        let started = SystemTime::now();
        let mut files = Vec::new();
        let mut candidates: Vec<(PathBuf, Language)> = Vec::new();
        let mut diagnostics = Vec::new();
        let mut pending = vec![root.clone()];

        while let Some(directory) = pending.pop() {
            let entries = match fs::read_dir(&directory) {
                Ok(entries) => entries,
                Err(error) => {
                    diagnostics.push(Diagnostic::warning(
                        "DIRECTORY_READ_FAILED",
                        format!("디렉터리를 읽지 못해 건너뜁니다: {error}"),
                        &relative_path(&root, &directory),
                    ));
                    continue;
                }
            };

            for entry in entries {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        diagnostics.push(Diagnostic::info(
                            "DIRECTORY_ENTRY_UNAVAILABLE",
                            format!("디렉터리 항목을 읽지 못했습니다: {error}"),
                        ));
                        continue;
                    }
                };

                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();

                if !self.options.config.scan.include_hidden && name.starts_with('.') {
                    continue;
                }

                let file_type = match entry.file_type() {
                    Ok(file_type) => file_type,
                    Err(error) => {
                        diagnostics.push(Diagnostic::warning(
                            "FILE_TYPE_UNAVAILABLE",
                            format!("파일 종류를 확인하지 못해 건너뜁니다: {error}"),
                            &relative_path(&root, &path),
                        ));
                        continue;
                    }
                };

                if file_type.is_symlink() {
                    // 기본값에서는 심볼릭 링크를 따라가지 않는다. 외부 디렉터리나
                    // 순환 링크가 분석 범위를 바꾸는 일을 막기 위해서다.
                    continue;
                }

                if file_type.is_dir() {
                    if !self
                        .options
                        .config
                        .paths
                        .ignored_directories
                        .iter()
                        .any(|ignored| ignored == &name)
                    {
                        pending.push(path);
                    }
                    continue;
                }

                if !file_type.is_file() {
                    continue;
                }

                let language = path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .and_then(|extension| self.options.config.languages.from_extension(extension))
                    .or_else(|| {
                        self.options
                            .config
                            .scan
                            .framework_config_file_names
                            .iter()
                            .any(|candidate| candidate.eq_ignore_ascii_case(&name))
                            .then_some(Language::Unknown)
                    });
                let Some(language) = language else { continue };

                candidates.push((path, language));
            }
        }

        // 디렉터리 발견은 순차적으로 수행하되, 파일 메타데이터·라인 수·해시는
        // 서로 독립적이므로 병렬 처리한다. collect 결과는 입력 순서를 보존해
        // 진단과 최종 경로 정렬의 결정성을 유지한다.
        candidates.sort_by(|left, right| left.0.cmp(&right.0));
        let limits = &self.options.config.limits;
        let mut limited = false;
        let mut selected = Vec::with_capacity(candidates.len().min(limits.max_files));
        let mut selected_bytes = 0_u64;
        for candidate in candidates {
            if selected.len() >= limits.max_files {
                limited = true;
                break;
            }
            let size = fs::metadata(&candidate.0)
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            if selected_bytes.saturating_add(size) > limits.max_total_bytes {
                // 정렬상 첫 파일이 크다고 뒤의 작은 파일까지 버리지 않는다.
                // 한도에 맞는 파일을 계속 찾아 분석 범위를 최대화한다.
                limited = true;
                continue;
            }
            selected_bytes = selected_bytes.saturating_add(size);
            selected.push(candidate);
        }
        candidates = selected;
        if limited {
            diagnostics.push(Diagnostic::warning(
                "ANALYSIS_LIMIT_REACHED",
                format!(
                    "프로젝트 스캔 한도에 도달해 일부 파일을 분석하지 않습니다: files={} bytes={}",
                    limits.max_files, limits.max_total_bytes
                ),
                Path::new("."),
            ));
        }
        let read_results: Vec<_> = candidates
            .par_iter()
            .map(|(path, language)| (path, self.read_file(&root, path, *language)))
            .collect();
        for (path, result) in read_results {
            match result {
                Ok(file) => files.push(file),
                Err(FileReadIssue::TooLarge { size, limit }) => {
                    diagnostics.push(Diagnostic::warning(
                        "FILE_TOO_LARGE",
                        format!("파일 크기 {size}바이트가 제한 {limit}바이트를 초과해 건너뜁니다."),
                        &relative_path(&root, path),
                    ))
                }
                Err(FileReadIssue::Io(error)) => diagnostics.push(Diagnostic::warning(
                    "FILE_READ_FAILED",
                    format!("파일을 읽지 못해 건너뜁니다: {error}"),
                    &relative_path(&root, path),
                )),
            }
        }

        files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        let summary = summarize(&files, &self.options.config.languages);
        let snapshot_id = fingerprint_snapshot(&files);
        let normalized_root = normalized_path(&root);
        let project_id = stable_id("project", &normalized_root);
        let analysis_id = runtime_id("analysis");
        let status = if diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == crate::diagnostics::DiagnosticSeverity::Error)
        {
            AnalysisStatus::Failed
        } else if diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == crate::diagnostics::DiagnosticSeverity::Warning
        }) {
            AnalysisStatus::Partial
        } else {
            AnalysisStatus::Ready
        };

        Ok(ScanOutput {
            analysis_id,
            status,
            context: ProjectContext {
                project_id,
                root_path: normalized_root,
                snapshot_id,
            },
            files,
            summary,
            diagnostics,
            elapsed_ms: elapsed_millis(started),
        })
    }

    fn read_file(
        &self,
        root: &Path,
        path: &Path,
        language: Language,
    ) -> Result<FileEntry, FileReadIssue> {
        let metadata = fs::metadata(path).map_err(FileReadIssue::Io)?;
        if metadata.len() > self.options.config.scan.max_file_size_bytes {
            return Err(FileReadIssue::TooLarge {
                size: metadata.len(),
                limit: self.options.config.scan.max_file_size_bytes,
            });
        }

        let mut reader = BufReader::new(File::open(path).map_err(FileReadIssue::Io)?);
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 16 * 1024];
        let mut line_count = 0_u64;
        let mut byte_count = 0_u64;
        let mut last_byte = None;
        let probe_shared_header = language == Language::C
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("h"));
        let mut shared_header_probe = Vec::new();

        loop {
            let read = reader.read(&mut buffer).map_err(FileReadIssue::Io)?;
            if read == 0 {
                break;
            }

            if self.options.config.scan.compute_hashes {
                hasher.update(&buffer[..read]);
            }
            if probe_shared_header && shared_header_probe.len() < SHARED_HEADER_PROBE_BYTES {
                let remaining = SHARED_HEADER_PROBE_BYTES - shared_header_probe.len();
                shared_header_probe.extend_from_slice(&buffer[..read.min(remaining)]);
            }
            byte_count += read as u64;
            line_count += buffer[..read].iter().filter(|byte| **byte == b'\n').count() as u64;
            last_byte = buffer.get(read - 1).copied();
        }

        if byte_count > 0 && last_byte != Some(b'\n') {
            line_count += 1;
        }

        let relative = relative_path(root, path);
        let relative_string = relative.to_string_lossy().replace('\\', "/");
        let content_hash = self
            .options
            .config
            .scan
            .compute_hashes
            .then(|| hex_digest(hasher.finalize()));
        let language = infer_shared_header_language(language, &shared_header_probe);

        Ok(FileEntry {
            file_id: stable_id("file", &relative_string),
            relative_path: relative_string.clone(),
            language,
            size_bytes: byte_count,
            line_count,
            modified_unix_ms: modified_unix_ms(&metadata.modified().ok()),
            content_hash,
            is_test: self.options.config.paths.is_test_path(&relative_string),
            parse_status: crate::model::ParseStatus::NotAnalyzed,
        })
    }
}

const SHARED_HEADER_PROBE_BYTES: usize = 256 * 1024;

/// `.h`는 C와 C++ 양쪽에서 사용되므로 확장자만으로는 parser를 선택할 수
/// 없다. 전체 소스를 다시 읽지 않도록 scanner가 확보한 앞부분에서 C++ 문법
/// 근거를 확인한다. 근거가 없으면 기존의 보수적인 C 판정을 유지한다.
fn infer_shared_header_language(language: Language, probe: &[u8]) -> Language {
    if language != Language::C || probe.is_empty() {
        return language;
    }
    let Ok(source) = std::str::from_utf8(probe) else {
        return language;
    };
    let has_cpp_marker = source.lines().map(str::trim).any(|line| {
        line.starts_with("namespace ")
            || line.starts_with("class ")
            || line.starts_with("template<")
            || line.starts_with("using namespace ")
            || line.starts_with("public:")
            || line.starts_with("private:")
            || line.starts_with("protected:")
            || line.contains("#include <drogon/")
            || line.contains("#include <crow")
            || line.contains("#include <boost/")
            || line.contains("#include <grpc")
            || line.contains("#include <Qt")
    });
    if has_cpp_marker {
        Language::Cpp
    } else {
        language
    }
}

fn validate_root(path: &Path) -> Result<(), EngineError> {
    if !path.exists() {
        return Err(EngineError::ProjectNotFound(path.to_path_buf()));
    }
    if !path.is_dir() {
        return Err(EngineError::ProjectIsNotDirectory(path.to_path_buf()));
    }
    Ok(())
}

fn relative_path(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}
