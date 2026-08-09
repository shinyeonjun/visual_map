use codebase_fact_model::analysis::ProgrammingLanguage;
use codebase_fact_model::coverage::{GapCode, SourceScopeCoverageRecord, SourceScopeState};
use codebase_fact_model::identity::{Sha256Digest, WorkspaceId};
use codebase_fact_model::source::{RepositoryPath, SourceFileKind};
use codebase_fact_model::source_manifest::{
    SourceEncoding, SourceEntryState, SourceLinkState, SourceManifest, SourceManifestFile,
};
use codebase_fact_model::validation::Validate;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::Match;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc,
};
use std::thread;

use crate::source::{
    is_managed_provider_root, measure_source_file, DEFAULT_SOURCE_READ_BUFFER_BYTES,
};
use crate::LANGUAGES;

const DEFAULT_MAX_ENTRIES: usize = 1_000_000;

#[derive(Clone, Copy, Debug)]
pub(crate) struct SourceCensusOptions {
    pub(crate) read_buffer_bytes: usize,
    pub(crate) max_entries: usize,
    pub(crate) measurement_workers: usize,
}

impl Default for SourceCensusOptions {
    fn default() -> Self {
        Self {
            read_buffer_bytes: DEFAULT_SOURCE_READ_BUFFER_BYTES,
            max_entries: DEFAULT_MAX_ENTRIES,
            measurement_workers: default_measurement_workers(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SourceCensus {
    pub(crate) root: PathBuf,
    pub(crate) manifest: SourceManifest,
}

impl SourceCensus {
    pub(crate) fn scan(root: &Path) -> Result<Self, String> {
        Self::scan_with_options(root, SourceCensusOptions::default())
    }

    pub(crate) fn scan_with_options(
        root: &Path,
        options: SourceCensusOptions,
    ) -> Result<Self, String> {
        if options.read_buffer_bytes < 4
            || options.max_entries == 0
            || options.measurement_workers == 0
        {
            return Err(
                "source census read buffer must be at least four bytes and entry/worker limits must be positive"
                    .to_string(),
            );
        }
        let security_root = root.canonicalize().map_err(|error| {
            format!(
                "cannot canonicalize project root {}: {error}",
                root.display()
            )
        })?;
        if !security_root.is_dir() {
            return Err(format!(
                "project root is not a directory: {}",
                root.display()
            ));
        }
        let workspace_id = workspace_id(&security_root)?;
        let mut scanner = Scanner {
            root: root.to_path_buf(),
            security_root,
            options,
            files: Vec::new(),
            pending_measurements: Vec::new(),
            scopes: Vec::new(),
            entry_count: 0,
        };
        scanner.walk_directory(root, &mut Vec::new(), true)?;
        scanner.files.extend(measure_pending_files(
            &scanner.pending_measurements,
            options.read_buffer_bytes,
            options.measurement_workers,
        )?);
        let manifest = SourceManifest::new(workspace_id, scanner.files, scanner.scopes)
            .map_err(|error| format!("invalid source manifest: {error}"))?;
        manifest
            .validate()
            .map_err(|error| format!("invalid source manifest: {error}"))?;
        Ok(Self {
            root: root.to_path_buf(),
            manifest,
        })
    }

    /// Reuses a manifest produced by the immediately preceding provider-pack
    /// selection pass. The manifest is fully schema/identity validated here;
    /// `index_project` still performs a fresh census before publication and
    /// refuses to publish if any source byte changed while providers ran.
    pub(crate) fn load_verified_manifest(
        root: &Path,
        path: &Path,
        expected_digest: &Sha256Digest,
    ) -> Result<Self, String> {
        let security_root = root.canonicalize().map_err(|error| {
            format!(
                "cannot canonicalize project root {}: {error}",
                root.display()
            )
        })?;
        let manifest: SourceManifest =
            serde_json::from_slice(&fs::read(path).map_err(|error| {
                format!("cannot read source manifest {}: {error}", path.display())
            })?)
            .map_err(|error| format!("invalid source manifest JSON: {error}"))?;
        manifest
            .validate()
            .map_err(|error| format!("invalid source manifest: {error}"))?;
        if manifest.workspace_id != workspace_id(&security_root)? {
            return Err("preflight source manifest belongs to another repository".to_string());
        }
        if &manifest.manifest_digest != expected_digest {
            return Err(format!(
                "preflight source manifest digest mismatch: expected={expected_digest} actual={}",
                manifest.manifest_digest
            ));
        }
        Ok(Self {
            root: root.to_path_buf(),
            manifest,
        })
    }

    /// Absolute, sorted provider inputs. Config and unsupported files remain
    /// in the manifest but are not presented as source-language documents.
    pub(crate) fn included_language_files(&self) -> Vec<PathBuf> {
        self.manifest
            .files
            .iter()
            .filter(|file| file.state == SourceEntryState::Included && !file.languages.is_empty())
            .map(|file| self.root.join(repository_path_to_native(&file.path)))
            .collect()
    }

    /// Compatibility view for the current cache layer. The first 64 bits of
    /// the already-computed SHA-256 are only a cache accelerator; snapshot
    /// identity continues to use the full manifest digest.
    pub(crate) fn source_snapshot_metadata(&self) -> crate::SourceSnapshot {
        let mut file_hashes = HashMap::new();
        let mut source_paths = Vec::new();
        for file in
            self.manifest.files.iter().filter(|file| {
                file.state == SourceEntryState::Included && !file.languages.is_empty()
            })
        {
            let Some(digest) = file.content_digest else {
                continue;
            };
            let mut prefix = [0_u8; 8];
            prefix.copy_from_slice(&digest.as_bytes()[..8]);
            file_hashes.insert(file.path.as_str().to_string(), u64::from_be_bytes(prefix));
            source_paths.push(self.root.join(repository_path_to_native(&file.path)));
        }
        crate::SourceSnapshot {
            files: Vec::new(),
            file_hashes,
            source_paths,
        }
    }
}

struct Scanner {
    root: PathBuf,
    security_root: PathBuf,
    options: SourceCensusOptions,
    files: Vec<SourceManifestFile>,
    pending_measurements: Vec<PendingMeasurement>,
    scopes: Vec<SourceScopeCoverageRecord>,
    entry_count: usize,
}

impl Scanner {
    fn walk_directory(
        &mut self,
        directory: &Path,
        matchers: &mut Vec<Gitignore>,
        is_root: bool,
    ) -> Result<(), String> {
        let local_matcher = build_ignore_matcher(directory, is_root)?;
        matchers.push(local_matcher);
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("cannot enumerate {}: {error}", directory.display())),
            Err(error) => Err(format!("cannot enumerate {}: {error}", directory.display())),
        };
        let mut entries = match entries {
            Ok(entries) => entries,
            Err(_error) if !is_root => {
                self.push_scope(
                    relative_path(&self.root, directory)?,
                    SourceScopeState::Failed,
                    vec![GapCode::UnreadableFile],
                )?;
                matchers.pop();
                return Ok(());
            }
            Err(error) => {
                matchers.pop();
                return Err(error);
            }
        };
        entries.sort_by_key(|entry| entry.file_name());

        for entry in entries {
            let path = entry.path();
            let relative = relative_path(&self.root, &path)?;
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
            let file_type = metadata.file_type();
            let is_directory = file_type.is_dir();
            self.reserve_entry()?;

            if is_link_like(&metadata) {
                self.record_symlink(&path, relative, &metadata)?;
                continue;
            }
            if is_ignored(matchers, &path, is_directory) {
                if is_directory {
                    self.push_scope(
                        relative,
                        SourceScopeState::Excluded,
                        vec![GapCode::ExcludedByRule, GapCode::VcsIgnored],
                    )?;
                } else if file_type.is_file() {
                    self.files.push(unread_file(
                        relative,
                        metadata.len(),
                        SourceEntryState::Excluded,
                        vec![GapCode::ExcludedByRule, GapCode::VcsIgnored],
                    ));
                }
                continue;
            }
            if is_directory {
                let name = entry.file_name().into_string().map_err(|_| {
                    format!("repository path is not valid UTF-8: {}", path.display())
                })?;
                if let Some(dependency_scope) = product_ignored_directory(&name)
                    .or_else(|| is_managed_provider_root(&path).then_some(true))
                {
                    let mut gaps = vec![GapCode::ExcludedByRule, GapCode::ProductIgnored];
                    if dependency_scope {
                        gaps.push(GapCode::DependencyScopeNotEnumerated);
                    }
                    self.push_scope(relative, SourceScopeState::Excluded, gaps)?;
                } else {
                    self.walk_directory(&path, matchers, false)?;
                }
            } else if file_type.is_file() {
                self.record_file(&path, relative, &metadata)?;
            }
        }
        matchers.pop();
        Ok(())
    }

    fn reserve_entry(&mut self) -> Result<(), String> {
        self.entry_count += 1;
        if self.entry_count > self.options.max_entries {
            return Err(format!(
                "source census exceeded its {}-entry safety budget",
                self.options.max_entries
            ));
        }
        Ok(())
    }

    fn push_scope(
        &mut self,
        path: RepositoryPath,
        state: SourceScopeState,
        mut gap_codes: Vec<GapCode>,
    ) -> Result<(), String> {
        gap_codes.sort();
        let scope = SourceScopeCoverageRecord {
            path,
            state,
            descendants_enumerated: false,
            gap_codes,
        };
        scope
            .validate()
            .map_err(|error| format!("invalid source scope receipt: {error}"))?;
        self.scopes.push(scope);
        Ok(())
    }

    fn record_symlink(
        &mut self,
        path: &Path,
        relative: RepositoryPath,
        metadata: &fs::Metadata,
    ) -> Result<(), String> {
        let (link_state, target_is_directory) = match path.canonicalize() {
            Ok(target) => (
                if target.starts_with(&self.security_root) {
                    SourceLinkState::SymlinkWithinRoot
                } else {
                    SourceLinkState::SymlinkEscapesRoot
                },
                target.is_dir(),
            ),
            Err(_) => (SourceLinkState::BrokenSymlink, false),
        };
        let mut gaps = vec![GapCode::ExcludedByRule, GapCode::SymlinkNotFollowed];
        match link_state {
            SourceLinkState::SymlinkEscapesRoot => gaps.push(GapCode::SymlinkEscapesRoot),
            SourceLinkState::BrokenSymlink => gaps.push(GapCode::UnreadableFile),
            SourceLinkState::Regular | SourceLinkState::SymlinkWithinRoot => {}
        }
        gaps.sort();
        if target_is_directory
            || (link_state == SourceLinkState::BrokenSymlink && path.extension().is_none())
        {
            self.push_scope(relative, SourceScopeState::Excluded, gaps)
        } else {
            let mut file = unread_file(relative, metadata.len(), SourceEntryState::Excluded, gaps);
            file.link_state = link_state;
            self.files.push(file);
            Ok(())
        }
    }

    fn record_file(
        &mut self,
        path: &Path,
        relative: RepositoryPath,
        metadata: &fs::Metadata,
    ) -> Result<(), String> {
        let classification = classify(&relative);
        if is_sensitive_path(&relative) {
            self.files.push(SourceManifestFile {
                path: relative,
                languages: classification.languages,
                file_kind: classification.kind,
                state: SourceEntryState::Excluded,
                byte_size: metadata.len(),
                line_count: None,
                non_blank_line_count: None,
                content_digest: None,
                encoding: SourceEncoding::NotRead,
                link_state: SourceLinkState::Regular,
                gap_codes: sorted_gaps(vec![GapCode::ExcludedByRule, GapCode::SensitiveFile]),
            });
            return Ok(());
        }
        match classification.policy {
            FilePolicy::Exclude => {
                self.files.push(SourceManifestFile {
                    path: relative,
                    languages: classification.languages,
                    file_kind: classification.kind,
                    state: SourceEntryState::Excluded,
                    byte_size: metadata.len(),
                    line_count: None,
                    non_blank_line_count: None,
                    content_digest: None,
                    encoding: SourceEncoding::NotRead,
                    link_state: SourceLinkState::Regular,
                    gap_codes: sorted_gaps(vec![GapCode::ExcludedByRule, GapCode::ProductIgnored]),
                });
                return Ok(());
            }
            FilePolicy::Unsupported => {
                self.files.push(SourceManifestFile {
                    path: relative,
                    languages: classification.languages,
                    file_kind: classification.kind,
                    state: SourceEntryState::Unsupported,
                    byte_size: metadata.len(),
                    line_count: None,
                    non_blank_line_count: None,
                    content_digest: None,
                    encoding: SourceEncoding::NotRead,
                    link_state: SourceLinkState::Regular,
                    gap_codes: vec![GapCode::UnsupportedFileType],
                });
                return Ok(());
            }
            FilePolicy::Measure => {}
        }
        self.pending_measurements.push(PendingMeasurement {
            path: path.to_path_buf(),
            relative,
            metadata_len: metadata.len(),
            languages: classification.languages,
            file_kind: classification.kind,
        });
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct PendingMeasurement {
    path: PathBuf,
    relative: RepositoryPath,
    metadata_len: u64,
    languages: Vec<ProgrammingLanguage>,
    file_kind: SourceFileKind,
}

fn default_measurement_workers() -> usize {
    thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .min(16)
        .max(1)
}

fn measure_pending_files(
    pending: &[PendingMeasurement],
    read_buffer_bytes: usize,
    requested_workers: usize,
) -> Result<Vec<SourceManifestFile>, String> {
    if pending.is_empty() {
        return Ok(Vec::new());
    }
    let worker_count = requested_workers.min(pending.len()).max(1);
    let next = AtomicUsize::new(0);
    let (sender, receiver) = mpsc::channel();
    thread::scope(|scope| -> Result<(), String> {
        let mut workers = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let sender = sender.clone();
            let next = &next;
            workers.push(scope.spawn(move || loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(file) = pending.get(index) else {
                    break;
                };
                if sender
                    .send((index, measure_pending_file(file, read_buffer_bytes)))
                    .is_err()
                {
                    break;
                }
            }));
        }
        drop(sender);
        for worker in workers {
            worker
                .join()
                .map_err(|_| "source census measurement worker panicked".to_string())?;
        }
        Ok(())
    })?;

    let mut ordered = std::iter::repeat_with(|| None)
        .take(pending.len())
        .collect::<Vec<_>>();
    for (index, file) in receiver {
        ordered[index] = Some(file);
    }
    ordered
        .into_iter()
        .enumerate()
        .map(|(index, file)| {
            file.ok_or_else(|| format!("source census measurement {index} produced no result"))
        })
        .collect()
}

fn measure_pending_file(
    pending: &PendingMeasurement,
    read_buffer_bytes: usize,
) -> SourceManifestFile {
    let measurement = match measure_source_file(&pending.path, read_buffer_bytes) {
        Ok(measurement) => measurement,
        Err(_) => {
            return SourceManifestFile {
                path: pending.relative.clone(),
                languages: pending.languages.clone(),
                file_kind: pending.file_kind,
                state: SourceEntryState::Failed,
                byte_size: pending.metadata_len,
                line_count: None,
                non_blank_line_count: None,
                content_digest: None,
                encoding: SourceEncoding::NotRead,
                link_state: SourceLinkState::Regular,
                gap_codes: vec![GapCode::UnreadableFile],
            };
        }
    };
    if measurement.encoding == SourceEncoding::Binary {
        return SourceManifestFile {
            path: pending.relative.clone(),
            languages: pending.languages.clone(),
            file_kind: pending.file_kind,
            state: SourceEntryState::Unsupported,
            byte_size: measurement.byte_size,
            line_count: None,
            non_blank_line_count: None,
            content_digest: None,
            encoding: SourceEncoding::Binary,
            link_state: SourceLinkState::Regular,
            gap_codes: vec![GapCode::BinarySource],
        };
    }
    if measurement.encoding == SourceEncoding::InvalidUtf8 {
        return SourceManifestFile {
            path: pending.relative.clone(),
            languages: pending.languages.clone(),
            file_kind: pending.file_kind,
            state: SourceEntryState::Unsupported,
            byte_size: measurement.byte_size,
            line_count: None,
            non_blank_line_count: None,
            content_digest: None,
            encoding: SourceEncoding::InvalidUtf8,
            link_state: SourceLinkState::Regular,
            gap_codes: vec![GapCode::UnsupportedEncoding],
        };
    }
    SourceManifestFile {
        path: pending.relative.clone(),
        languages: pending.languages.clone(),
        file_kind: pending.file_kind,
        state: SourceEntryState::Included,
        byte_size: measurement.byte_size,
        line_count: measurement.line_count,
        non_blank_line_count: measurement.non_blank_line_count,
        content_digest: Some(measurement.content_digest),
        encoding: measurement.encoding,
        link_state: SourceLinkState::Regular,
        gap_codes: vec![],
    }
}

#[derive(Clone, Copy)]
enum FilePolicy {
    Measure,
    Exclude,
    Unsupported,
}

struct FileClassification {
    languages: Vec<ProgrammingLanguage>,
    kind: SourceFileKind,
    policy: FilePolicy,
}

fn classify(path: &RepositoryPath) -> FileClassification {
    let lower = path.as_str().to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(lower.as_str());
    let extension = name.rsplit_once('.').map(|(_, extension)| extension);
    let mut languages = languages_for_extension(extension);
    if extension == Some("vue") {
        languages.push(ProgrammingLanguage::TypeScript);
    }
    languages.sort();
    languages.dedup();
    let segments = lower.split('/').collect::<Vec<_>>();
    let in_test_scope = segments.iter().any(|segment| {
        matches!(
            *segment,
            "test" | "tests" | "spec" | "specs" | "e2e" | "__tests__"
        )
    });
    let in_migration_scope = segments
        .iter()
        .any(|segment| matches!(*segment, "migration" | "migrations" | "migrate"));
    let generated = is_generated_name(name);

    if !languages.is_empty() {
        let kind = if generated {
            SourceFileKind::Generated
        } else if in_test_scope || is_test_name(name) {
            SourceFileKind::Test
        } else if in_migration_scope {
            SourceFileKind::Migration
        } else {
            SourceFileKind::Source
        };
        return FileClassification {
            languages,
            kind,
            policy: FilePolicy::Measure,
        };
    }
    if extension == Some("sql") {
        return FileClassification {
            languages,
            kind: if in_migration_scope {
                SourceFileKind::Migration
            } else {
                SourceFileKind::Sql
            },
            policy: FilePolicy::Measure,
        };
    }
    if is_deployment_file(&segments, name, extension) {
        return FileClassification {
            languages,
            kind: SourceFileKind::Deployment,
            policy: FilePolicy::Measure,
        };
    }
    if is_build_file(name) {
        return FileClassification {
            languages,
            kind: SourceFileKind::Build,
            policy: FilePolicy::Measure,
        };
    }
    if is_semantic_config_file(name) {
        return FileClassification {
            languages,
            kind: SourceFileKind::Config,
            policy: FilePolicy::Measure,
        };
    }
    if is_documentation(name, extension) {
        return FileClassification {
            languages,
            kind: SourceFileKind::Documentation,
            policy: FilePolicy::Exclude,
        };
    }
    FileClassification {
        languages,
        kind: SourceFileKind::Other,
        policy: FilePolicy::Unsupported,
    }
}

fn languages_for_extension(extension: Option<&str>) -> Vec<ProgrammingLanguage> {
    let Some(extension) = extension else {
        return Vec::new();
    };
    LANGUAGES
        .iter()
        .filter(|language| language.extensions.contains(&extension))
        .map(|language| language.contract_language)
        .collect()
}

fn is_generated_name(name: &str) -> bool {
    name.contains(".generated.")
        || name.ends_with(".g.dart")
        || name.ends_with(".freezed.dart")
        || name.ends_with(".designer.cs")
        || name.ends_with(".pb.go")
        || name.ends_with(".pb.cc")
        || name.ends_with(".pb.h")
}

fn is_test_name(name: &str) -> bool {
    [".test.", ".spec.", "_test.", "test_"]
        .iter()
        .any(|marker| name.contains(marker))
}

fn is_documentation(name: &str, extension: Option<&str>) -> bool {
    matches!(extension, Some("md" | "mdx" | "rst" | "adoc" | "txt"))
        || matches!(
            name,
            "readme" | "license" | "notice" | "authors" | "changelog"
        )
}

fn is_deployment_file(segments: &[&str], name: &str, extension: Option<&str>) -> bool {
    name == "dockerfile"
        || name.starts_with("dockerfile.")
        || name == "docker-compose.yml"
        || name == "docker-compose.yaml"
        || matches!(extension, Some("tf" | "tfvars"))
        || segments.iter().any(|segment| {
            matches!(
                *segment,
                ".github" | "k8s" | "kubernetes" | "deploy" | "deployment"
            )
        }) && matches!(extension, Some("yml" | "yaml" | "json"))
}

fn is_build_file(name: &str) -> bool {
    matches!(
        name,
        "makefile"
            | "gnumakefile"
            | "justfile"
            | "cmakelists.txt"
            | "meson.build"
            | "meson_options.txt"
            | "build"
            | "build.bazel"
            | "workspace"
            | "workspace.bazel"
            | "compile_commands.json"
            | "compile_flags.txt"
            | ".clangd"
    ) || name.ends_with(".gradle")
        || name.ends_with(".gradle.kts")
        || name.ends_with(".csproj")
        || name.ends_with(".sln")
        || name.ends_with(".slnx")
        || name.ends_with(".vcxproj")
        || name.ends_with(".props")
        || name.ends_with(".targets")
}

fn is_semantic_config_file(name: &str) -> bool {
    matches!(
        name,
        ".gitignore"
            | ".ignore"
            | "package.json"
            | "package-lock.json"
            | "pnpm-lock.yaml"
            | "yarn.lock"
            | "pyproject.toml"
            | "pyrightconfig.json"
            | "setup.cfg"
            | "requirements.txt"
            | "pipfile"
            | "pipfile.lock"
            | "poetry.lock"
            | "global.json"
            | "nuget.config"
            | ".editorconfig"
            | "pom.xml"
            | "gradle.properties"
            | "go.mod"
            | "go.sum"
            | "go.work"
            | "go.work.sum"
            | "cargo.toml"
            | "cargo.lock"
            | "pubspec.yaml"
            | "pubspec.lock"
            | "analysis_options.yaml"
            | "prisma.schema"
    ) || name.starts_with("tsconfig") && name.ends_with(".json")
        || name.starts_with("jsconfig") && name.ends_with(".json")
        || name.starts_with("requirements") && name.ends_with(".txt")
        || name.ends_with(".prisma")
        || name.ends_with(".ruleset")
}

fn is_sensitive_path(path: &RepositoryPath) -> bool {
    let name = path
        .as_str()
        .rsplit('/')
        .next()
        .unwrap_or(path.as_str())
        .to_ascii_lowercase();
    name == ".env"
        || name.starts_with(".env.")
        || matches!(
            name.as_str(),
            ".npmrc" | ".pypirc" | ".netrc" | "credentials"
        )
}

fn product_ignored_directory(name: &str) -> Option<bool> {
    let name = name.to_ascii_lowercase();
    if matches!(
        name.as_str(),
        "node_modules" | "vendor" | "venv" | ".venv" | ".dart_tool" | ".gradle"
    ) {
        return Some(true);
    }
    matches!(
        name.as_str(),
        ".git"
            | ".idea"
            | ".vscode"
            | ".pytest_cache"
            | ".ruby-lsp"
            | ".storybook"
            | ".cache"
            | ".code_memory"
            | "__pycache__"
            | "coverage"
            | "dist"
            | "docs"
            | "obj"
            | "out"
            | "target"
            | "tmp"
    )
    .then_some(false)
}

fn build_ignore_matcher(directory: &Path, is_root: bool) -> Result<Gitignore, String> {
    let mut builder = GitignoreBuilder::new(directory);
    for name in [".gitignore", ".ignore"] {
        let path = directory.join(name);
        if path.is_file() {
            if let Some(error) = builder.add(&path) {
                return Err(format!("cannot parse {}: {error}", path.display()));
            }
        }
    }
    if is_root {
        let info_exclude = directory.join(".git").join("info").join("exclude");
        if info_exclude.is_file() {
            if let Some(error) = builder.add(&info_exclude) {
                return Err(format!("cannot parse {}: {error}", info_exclude.display()));
            }
        }
    }
    builder.build().map_err(|error| {
        format!(
            "cannot build ignore rules for {}: {error}",
            directory.display()
        )
    })
}

fn is_ignored(matchers: &[Gitignore], path: &Path, is_directory: bool) -> bool {
    for matcher in matchers.iter().rev() {
        match matcher.matched(path, is_directory) {
            Match::Ignore(_) => return true,
            Match::Whitelist(_) => return false,
            Match::None => {}
        }
    }
    false
}

fn relative_path(root: &Path, path: &Path) -> Result<RepositoryPath, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| format!("path escaped project root: {}", path.display()))?;
    if relative.as_os_str().is_empty() {
        return Ok(RepositoryPath::root());
    }
    let segments = relative
        .components()
        .map(|component| {
            component
                .as_os_str()
                .to_str()
                .ok_or_else(|| format!("repository path is not valid UTF-8: {}", path.display()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    RepositoryPath::parse(segments.join("/"))
        .map_err(|error| format!("invalid repository path {}: {error}", path.display()))
}

fn repository_path_to_native(path: &RepositoryPath) -> PathBuf {
    path.as_str().split('/').collect()
}

fn workspace_id(security_root: &Path) -> Result<WorkspaceId, String> {
    let normalized = security_root
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase();
    let digest = Sha256Digest::of_bytes(normalized.as_bytes()).to_hex();
    WorkspaceId::parse(format!("ws-{}", &digest[..16]))
        .map_err(|error| format!("cannot derive workspace identity: {error}"))
}

fn unread_file(
    path: RepositoryPath,
    byte_size: u64,
    state: SourceEntryState,
    gap_codes: Vec<GapCode>,
) -> SourceManifestFile {
    let classification = classify(&path);
    SourceManifestFile {
        path,
        languages: classification.languages,
        file_kind: classification.kind,
        state,
        byte_size,
        line_count: None,
        non_blank_line_count: None,
        content_digest: None,
        encoding: SourceEncoding::NotRead,
        link_state: SourceLinkState::Regular,
        gap_codes: sorted_gaps(gap_codes),
    }
}

fn sorted_gaps(mut gaps: Vec<GapCode>) -> Vec<GapCode> {
    gaps.sort();
    gaps.dedup();
    gaps
}

#[cfg(not(windows))]
fn is_link_like(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(windows)]
fn is_link_like(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}
