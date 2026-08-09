//! Writable, manifest-backed provider execution workspaces.
//!
//! Some semantic providers invoke build-system discovery and legitimately
//! create `obj`, `bin`, `target`, or IDE metadata. They must never receive the
//! selected repository as their writable working tree. This module copies the
//! files admitted for analysis plus regular, non-sensitive support files from
//! the sealed [`SourceManifest`] into a process-scoped workspace and preserves
//! repository-relative paths for provider evidence.
//!
//! Support files do not become source facts merely because they are copied.
//! They are present so build systems see the same checked-in inputs as the
//! selected repository. Gradle projects, for example, commonly load arbitrary
//! Checkstyle XML, wrapper JARs, allowlists, or resource files while building
//! their project model.

use codebase_fact_model::identity::Sha256Digest;
use codebase_fact_model::source::RepositoryPath;
use codebase_fact_model::source_manifest::{SourceEntryState, SourceLinkState, SourceManifest};
use codebase_fact_model::validation::Validate;
use sha2::{Digest as _, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

const COPY_BUFFER_BYTES: usize = 128 * 1024;

#[derive(Clone)]
struct WorkspaceInput {
    path: RepositoryPath,
    byte_size: u64,
    content_digest: Sha256Digest,
}

#[derive(Default)]
struct WorkspaceState {
    next_turn: u64,
    active: bool,
    materialized: bool,
}

/// One shared writable copy for a language in the current analysis process.
/// Turns serialize same-language provider units so build outputs cannot race.
pub(crate) struct ProviderWorkspace {
    source_root: PathBuf,
    execution_root: PathBuf,
    inputs: Vec<WorkspaceInput>,
    state: Mutex<WorkspaceState>,
    ready: Condvar,
}

/// A cloneable job binding. The ordinal is assigned from the deterministic
/// provider schedule after cache hits have been removed.
#[derive(Clone)]
pub(crate) struct ProviderWorkspaceBinding {
    workspace: Arc<ProviderWorkspace>,
    ordinal: u64,
}

impl ProviderWorkspaceBinding {
    pub(crate) fn new(workspace: Arc<ProviderWorkspace>, ordinal: u64) -> Self {
        Self { workspace, ordinal }
    }

    pub(crate) fn begin(&self) -> Result<ProviderWorkspaceTurn<'_>, String> {
        self.workspace.begin_turn(self.ordinal)
    }
}

/// Exclusive execution token. Dropping it advances the deterministic turn
/// even when a provider fails or unwinds.
pub(crate) struct ProviderWorkspaceTurn<'a> {
    workspace: &'a ProviderWorkspace,
    ordinal: u64,
    released: bool,
}

impl ProviderWorkspace {
    pub(crate) fn from_manifest(
        source_root: &Path,
        execution_root: PathBuf,
        manifest: &SourceManifest,
    ) -> Result<Self, String> {
        manifest.validate().map_err(|error| {
            format!("cannot prepare provider workspace from an invalid SourceManifest: {error}")
        })?;
        // Use the exact canonical representation owned by the indexing
        // boundary. On Windows, calling `std::fs::canonicalize` here would
        // reintroduce a `\\?\` prefix that `canonical_project_root` has
        // deliberately removed. Two paths to the same directory would then
        // fail `strip_prefix` and the isolated Java/C# providers would be
        // reported as outside the selected repository.
        let source_root = crate::source::canonical_existing_path(source_root).map_err(|error| {
            format!(
                "cannot resolve provider workspace source root {}: {error}",
                source_root.display()
            )
        })?;
        let parent = execution_root.parent().ok_or_else(|| {
            format!(
                "provider workspace has no parent directory: {}",
                execution_root.display()
            )
        })?;
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "cannot create provider workspace parent {}: {error}",
                parent.display()
            )
        })?;
        let parent = crate::source::canonical_existing_path(parent).map_err(|error| {
            format!(
                "cannot resolve provider workspace parent {}: {error}",
                parent.display()
            )
        })?;
        let name = execution_root.file_name().ok_or_else(|| {
            format!(
                "provider workspace has no final component: {}",
                execution_root.display()
            )
        })?;
        let execution_root = parent.join(name);
        if execution_root.starts_with(&source_root) || source_root.starts_with(&execution_root) {
            return Err(format!(
                "provider writable workspace must be disjoint from the selected repository: {}",
                execution_root.display()
            ));
        }
        if execution_root.exists() {
            return Err(format!(
                "provider writable workspace already exists before materialization: {}",
                execution_root.display()
            ));
        }

        let mut inputs = Vec::new();
        for file in manifest.files.iter().filter(|file| {
            matches!(
                file.state,
                SourceEntryState::Included | SourceEntryState::Unsupported
            )
        }) {
            if file.link_state != SourceLinkState::Regular {
                // Unsupported symlinks are intentionally not followed. An
                // included semantic input may never reach this branch because
                // SourceManifest validation rejects it, but keep the invariant
                // explicit at the provider boundary.
                if file.state == SourceEntryState::Included {
                    return Err(format!(
                        "included provider workspace input is not a regular file: {}",
                        file.path
                    ));
                }
                continue;
            }
            let source = join_repository_path(&source_root, &file.path);
            let content_digest = match file.content_digest {
                Some(content_digest) => content_digest,
                None => digest_workspace_support_file(&source, file.byte_size)?,
            };
            inputs.push(WorkspaceInput {
                path: file.path.clone(),
                byte_size: file.byte_size,
                content_digest,
            });
        }
        inputs.sort_by(|left, right| left.path.cmp(&right.path));
        if inputs.is_empty() {
            return Err("provider writable workspace has no admitted inputs".to_string());
        }

        Ok(Self {
            source_root,
            execution_root,
            inputs,
            state: Mutex::new(WorkspaceState::default()),
            ready: Condvar::new(),
        })
    }

    fn begin_turn(&self, ordinal: u64) -> Result<ProviderWorkspaceTurn<'_>, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "provider workspace turn state is poisoned".to_string())?;
        loop {
            if ordinal < state.next_turn {
                return Err(format!(
                    "provider workspace turn {ordinal} has already passed turn {}",
                    state.next_turn
                ));
            }
            if ordinal == state.next_turn && !state.active {
                state.active = true;
                break;
            }
            state = self
                .ready
                .wait(state)
                .map_err(|_| "provider workspace turn wait is poisoned".to_string())?;
        }
        drop(state);
        Ok(ProviderWorkspaceTurn {
            workspace: self,
            ordinal,
            released: false,
        })
    }

    fn materialize(&self) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "provider workspace materialization state is poisoned".to_string())?;
        if state.materialized {
            return Ok(());
        }
        if self.execution_root.exists() {
            return Err(format!(
                "provider workspace appeared before its manifest copy was sealed: {}",
                self.execution_root.display()
            ));
        }
        fs::create_dir(&self.execution_root).map_err(|error| {
            format!(
                "cannot create provider workspace {}: {error}",
                self.execution_root.display()
            )
        })?;
        let canonical_execution_root = crate::source::canonical_existing_path(&self.execution_root)
            .map_err(|error| {
                format!(
                    "cannot resolve provider workspace {}: {error}",
                    self.execution_root.display()
                )
            })?;
        if canonical_execution_root != self.execution_root {
            return Err(format!(
                "provider workspace resolved through an unexpected filesystem alias: {}",
                self.execution_root.display()
            ));
        }

        for input in &self.inputs {
            self.copy_verified_input(input)?;
        }
        state.materialized = true;
        Ok(())
    }

    fn copy_verified_input(&self, input: &WorkspaceInput) -> Result<(), String> {
        let source = join_repository_path(&self.source_root, &input.path);
        let metadata = fs::symlink_metadata(&source).map_err(|error| {
            format!(
                "cannot inspect provider workspace input {}: {error}",
                source.display()
            )
        })?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(format!(
                "provider workspace input changed into a non-regular file: {}",
                source.display()
            ));
        }
        if metadata.len() != input.byte_size {
            return Err(format!(
                "provider workspace input size changed after Source Census: {}",
                source.display()
            ));
        }
        let canonical_source =
            crate::source::canonical_existing_path(&source).map_err(|error| {
                format!(
                    "cannot resolve provider workspace input {}: {error}",
                    source.display()
                )
            })?;
        if !canonical_source.starts_with(&self.source_root) {
            return Err(format!(
                "provider workspace input escaped the selected repository: {}",
                source.display()
            ));
        }

        let destination = join_repository_path(&self.execution_root, &input.path);
        if !destination.starts_with(&self.execution_root) {
            return Err(format!(
                "provider workspace destination escaped its root: {}",
                destination.display()
            ));
        }
        let parent = destination.parent().ok_or_else(|| {
            format!(
                "provider workspace destination has no parent: {}",
                destination.display()
            )
        })?;
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "cannot create provider workspace directory {}: {error}",
                parent.display()
            )
        })?;

        let source_file = File::open(&source).map_err(|error| {
            format!(
                "cannot open provider workspace input {}: {error}",
                source.display()
            )
        })?;
        let destination_file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&destination)
            .map_err(|error| {
                format!(
                    "cannot create provider workspace input {}: {error}",
                    destination.display()
                )
            })?;
        let mut reader = BufReader::with_capacity(COPY_BUFFER_BYTES, source_file);
        let mut writer = BufWriter::with_capacity(COPY_BUFFER_BYTES, destination_file);
        let mut hasher = Sha256::new();
        let mut copied = 0_u64;
        let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
        loop {
            let count = reader.read(&mut buffer).map_err(|error| {
                format!(
                    "cannot read provider workspace input {}: {error}",
                    source.display()
                )
            })?;
            if count == 0 {
                break;
            }
            writer.write_all(&buffer[..count]).map_err(|error| {
                format!(
                    "cannot write provider workspace input {}: {error}",
                    destination.display()
                )
            })?;
            hasher.update(&buffer[..count]);
            copied += count as u64;
        }
        writer.flush().map_err(|error| {
            format!(
                "cannot flush provider workspace input {}: {error}",
                destination.display()
            )
        })?;
        let actual_digest = hasher.finalize();
        if copied != input.byte_size || actual_digest.as_slice() != input.content_digest.as_bytes()
        {
            return Err(format!(
                "provider workspace input content changed after Source Census: {}",
                source.display()
            ));
        }
        Ok(())
    }
}

fn digest_workspace_support_file(path: &Path, expected_size: u64) -> Result<Sha256Digest, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "cannot inspect provider workspace support file {}: {error}",
            path.display()
        )
    })?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(format!(
            "provider workspace support input is not a regular file: {}",
            path.display()
        ));
    }
    if metadata.len() != expected_size {
        return Err(format!(
            "provider workspace support input size changed after Source Census: {}",
            path.display()
        ));
    }

    let file = File::open(path).map_err(|error| {
        format!(
            "cannot read provider workspace support file {}: {error}",
            path.display()
        )
    })?;
    let mut reader = BufReader::with_capacity(COPY_BUFFER_BYTES, file);
    let mut hasher = Sha256::new();
    let mut copied = 0_u64;
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
    loop {
        let count = reader.read(&mut buffer).map_err(|error| {
            format!(
                "cannot read provider workspace support file {}: {error}",
                path.display()
            )
        })?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        copied += count as u64;
    }
    if copied != expected_size {
        return Err(format!(
            "provider workspace support input changed after Source Census: {}",
            path.display()
        ));
    }
    Sha256Digest::parse(&format!("{:x}", hasher.finalize())).map_err(|error| {
        format!(
            "cannot seal provider workspace support file {}: {error}",
            path.display()
        )
    })
}

impl ProviderWorkspaceTurn<'_> {
    pub(crate) fn ensure_materialized(&self) -> Result<(), String> {
        self.workspace.materialize()
    }

    pub(crate) fn execution_root(&self) -> &Path {
        &self.workspace.execution_root
    }

    pub(crate) fn map_path(&self, source_path: &Path) -> Result<PathBuf, String> {
        let relative = source_path
            .strip_prefix(&self.workspace.source_root)
            .map_err(|_| {
                format!(
                    "provider path is outside the selected repository: {}",
                    source_path.display()
                )
            })?;
        if relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        {
            if relative.as_os_str().is_empty() {
                return Ok(self.workspace.execution_root.clone());
            }
            return Err(format!(
                "provider path is not a canonical repository path: {}",
                source_path.display()
            ));
        }
        Ok(self.workspace.execution_root.join(relative))
    }
}

impl Drop for ProviderWorkspaceTurn<'_> {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        let mut state = match self.workspace.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        if state.active && state.next_turn == self.ordinal {
            state.active = false;
            state.next_turn += 1;
            self.released = true;
            self.workspace.ready.notify_all();
        }
    }
}

fn join_repository_path(root: &Path, path: &RepositoryPath) -> PathBuf {
    path.as_str()
        .split('/')
        .fold(root.to_path_buf(), |current, segment| current.join(segment))
}

#[cfg(test)]
mod tests {
    use super::*;
    use codebase_fact_model::identity::WorkspaceId;
    use codebase_fact_model::source::SourceFileKind;
    use codebase_fact_model::source_manifest::{SourceEncoding, SourceManifestFile};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

    fn fixture() -> (PathBuf, PathBuf, SourceManifest) {
        let nonce = format!(
            "{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        );
        let source = std::env::temp_dir().join(format!("provider-workspace-source-{nonce}"));
        let work = std::env::temp_dir().join(format!("provider-workspace-cache-{nonce}"));
        let _ = fs::remove_dir_all(&source);
        let _ = fs::remove_dir_all(&work);
        fs::create_dir_all(source.join("src")).unwrap();
        fs::create_dir_all(&work).unwrap();
        fs::write(source.join("src/Main.cs"), b"class Main {}\n").unwrap();
        fs::write(source.join("project.csproj"), b"<Project />\n").unwrap();
        fs::create_dir_all(source.join("buildSrc/config/checkstyle")).unwrap();
        fs::write(
            source.join("buildSrc/config/checkstyle/checkstyle.xml"),
            b"<module name=\"Checker\" />\n",
        )
        .unwrap();
        fs::write(source.join("secret.txt"), b"not admitted\n").unwrap();
        let file = |path: &str, bytes: &[u8]| SourceManifestFile {
            path: RepositoryPath::parse(path).unwrap(),
            languages: Vec::new(),
            file_kind: SourceFileKind::Config,
            state: SourceEntryState::Included,
            byte_size: bytes.len() as u64,
            line_count: Some(1),
            non_blank_line_count: Some(1),
            content_digest: Some(Sha256Digest::of_bytes(bytes)),
            encoding: SourceEncoding::Utf8,
            link_state: SourceLinkState::Regular,
            gap_codes: Vec::new(),
        };
        let unsupported_support = SourceManifestFile {
            path: RepositoryPath::parse("buildSrc/config/checkstyle/checkstyle.xml").unwrap(),
            languages: Vec::new(),
            file_kind: SourceFileKind::Other,
            state: SourceEntryState::Unsupported,
            byte_size: b"<module name=\"Checker\" />\n".len() as u64,
            line_count: None,
            non_blank_line_count: None,
            content_digest: None,
            encoding: SourceEncoding::NotRead,
            link_state: SourceLinkState::Regular,
            gap_codes: vec![codebase_fact_model::coverage::GapCode::UnsupportedFileType],
        };
        let manifest = SourceManifest::new(
            WorkspaceId::parse("ws-0123456789abcdef").unwrap(),
            vec![
                file("project.csproj", b"<Project />\n"),
                file("src/Main.cs", b"class Main {}\n"),
                unsupported_support,
            ],
            Vec::new(),
        )
        .unwrap();
        (source, work, manifest)
    }

    #[test]
    fn manifest_copy_is_disjoint_and_preserves_repository_paths() {
        let (source, work, manifest) = fixture();
        let selected_root = crate::source::canonical_project_root(&source).unwrap();
        let workspace = Arc::new(
            ProviderWorkspace::from_manifest(&selected_root, work.join("csharp"), &manifest)
                .unwrap(),
        );
        let binding = ProviderWorkspaceBinding::new(workspace, 0);
        {
            let turn = binding.begin().unwrap();
            turn.ensure_materialized().unwrap();
            assert_eq!(
                fs::read(turn.execution_root().join("src/Main.cs")).unwrap(),
                b"class Main {}\n"
            );
            assert_eq!(
                fs::read(
                    turn.execution_root()
                        .join("buildSrc/config/checkstyle/checkstyle.xml")
                )
                .unwrap(),
                b"<module name=\"Checker\" />\n"
            );
            assert!(!turn.execution_root().join("secret.txt").exists());
            assert_eq!(
                turn.map_path(&selected_root).unwrap(),
                turn.execution_root()
            );
            fs::create_dir_all(turn.execution_root().join("obj")).unwrap();
            fs::write(
                turn.execution_root().join("obj/generated.props"),
                b"generated",
            )
            .unwrap();
        }
        assert!(!source.join("obj").exists());
        assert_eq!(
            fs::read(source.join("src/Main.cs")).unwrap(),
            b"class Main {}\n"
        );
        let _ = fs::remove_dir_all(source);
        let _ = fs::remove_dir_all(work);
    }

    #[test]
    fn changed_input_fails_before_any_provider_can_run() {
        let (source, work, manifest) = fixture();
        let workspace = Arc::new(
            ProviderWorkspace::from_manifest(&source, work.join("java"), &manifest).unwrap(),
        );
        fs::write(source.join("src/Main.cs"), b"changed but same?\n").unwrap();
        let binding = ProviderWorkspaceBinding::new(workspace, 0);
        let turn = binding.begin().unwrap();
        let error = turn.ensure_materialized().unwrap_err();
        assert!(error.contains("changed after Source Census"));
        let _ = fs::remove_dir_all(source);
        let _ = fs::remove_dir_all(work);
    }

    #[test]
    fn deterministic_turns_advance_without_recopying_the_workspace() {
        let (source, work, manifest) = fixture();
        let workspace = Arc::new(
            ProviderWorkspace::from_manifest(&source, work.join("java"), &manifest).unwrap(),
        );
        {
            let first = ProviderWorkspaceBinding::new(workspace.clone(), 0);
            let turn = first.begin().unwrap();
            turn.ensure_materialized().unwrap();
            fs::write(
                turn.execution_root().join("provider-output"),
                b"kept in sandbox",
            )
            .unwrap();
        }
        {
            let second = ProviderWorkspaceBinding::new(workspace, 1);
            let turn = second.begin().unwrap();
            turn.ensure_materialized().unwrap();
            assert!(turn.execution_root().join("provider-output").is_file());
        }
        let _ = fs::remove_dir_all(source);
        let _ = fs::remove_dir_all(work);
    }
}
