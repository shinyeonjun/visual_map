use codebase_fact_model::analysis::ProviderExecutionContext;
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::source::is_managed_provider_root;
use crate::{compile_database_dirs, find_tool, is_excluded_source_dir};
use crate::{
    Diagnostic, DiagnosticCode, DocumentOutput, IndexOutput, LanguageSpec, RelationOutput,
    SourceSnapshot,
};

#[derive(Clone)]
struct FileChecksumCacheEntry {
    length: u64,
    modified: Option<SystemTime>,
    checksum: u64,
}

static FILE_CHECKSUM_CACHE: OnceLock<Mutex<HashMap<PathBuf, FileChecksumCacheEntry>>> =
    OnceLock::new();

#[derive(Serialize, Deserialize)]
struct SourceManifest {
    schema: String,
    dependency_context_digest: u64,
    files: HashMap<String, u64>,
    reverse_imports: HashMap<String, Vec<String>>,
}

#[derive(Serialize, Deserialize)]
struct ProviderRunInputManifest {
    schema: String,
    dependency_context_digest: u64,
    files: HashMap<String, u64>,
}

#[derive(Serialize, Deserialize)]
struct CacheGenerationManifest {
    schema: String,
    files: Vec<String>,
}

#[derive(Default)]
pub(crate) struct CacheImpact {
    pub(crate) force_all: bool,
    pub(crate) affected_paths: HashSet<String>,
}

#[derive(Deserialize)]
struct PreviousArchitecture {
    #[serde(default)]
    edges: Vec<PreviousArchitectureEdge>,
}

#[derive(Deserialize)]
struct PreviousArchitectureEdge {
    from: String,
    to: String,
    kind: String,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct CachedLanguageResult {
    pub(crate) schema: String,
    pub(crate) key: String,
    pub(crate) documents: Vec<DocumentOutput>,
    pub(crate) relations: Vec<RelationOutput>,
    pub(crate) execution_context: ProviderExecutionContext,
    #[serde(default)]
    pub(crate) diagnostics: Vec<CachedDiagnostic>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct CachedDiagnostic {
    pub(crate) language: String,
    pub(crate) level: String,
    #[serde(default)]
    pub(crate) code: DiagnosticCode,
    pub(crate) message: String,
    #[serde(default)]
    pub(crate) detail: Option<String>,
    pub(crate) path: Option<String>,
    pub(crate) line: Option<u32>,
}

pub(crate) fn architecture_cache_key(
    root: &Path,
    pack_root: &Path,
    output: &IndexOutput,
    source_snapshot: &SourceSnapshot,
    project_config_digest: u64,
) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    // Bump this whenever architecture projection behavior changes. The
    // Serialized language index can stay identical while the desktop
    // projection changes (for example, preserving an unresolved route node).
    checksum_update(&mut hash, b"code-memory-architecture-cache.v23");
    checksum_update(&mut hash, root.to_string_lossy().as_bytes());
    checksum_update(&mut hash, pack_root.to_string_lossy().as_bytes());
    for document in &output.documents {
        checksum_update(&mut hash, document.path.as_bytes());
        for symbol in &document.symbols {
            checksum_update(&mut hash, symbol.symbol.as_bytes());
            checksum_update(&mut hash, symbol.kind.as_bytes());
            if let Some(enclosing) = &symbol.enclosing_symbol {
                checksum_update(&mut hash, enclosing.as_bytes());
            }
        }
        for occurrence in &document.occurrences {
            checksum_update(&mut hash, occurrence.symbol.as_bytes());
            checksum_range(&mut hash, &occurrence.range);
            checksum_update(&mut hash, &[u8::from(occurrence.definition)]);
            checksum_update(&mut hash, &[u8::from(occurrence.import)]);
        }
    }
    for relation in &output.relations {
        checksum_update(&mut hash, relation.from.as_bytes());
        checksum_update(&mut hash, relation.to.as_bytes());
        checksum_update(&mut hash, relation.kind.as_bytes());
        checksum_update(&mut hash, relation.path.as_bytes());
        checksum_range(&mut hash, &relation.range);
    }
    for relation in &output.file_relations {
        checksum_update(&mut hash, relation.from.as_bytes());
        checksum_update(&mut hash, relation.to.as_bytes());
        checksum_update(&mut hash, relation.kind.as_bytes());
        checksum_update(&mut hash, relation.path.as_bytes());
        checksum_range(&mut hash, &relation.range);
        for (key, value) in &relation.properties {
            checksum_update(&mut hash, key.as_bytes());
            checksum_update(&mut hash, value.as_bytes());
        }
    }
    for path in &output.project_model_files {
        checksum_update(&mut hash, path.as_bytes());
    }
    for framework in &output.frameworks {
        checksum_update(&mut hash, framework.id.as_bytes());
        checksum_update(&mut hash, framework.language.as_bytes());
        for fact in &framework.facts {
            checksum_update(&mut hash, fact.id.as_bytes());
            checksum_update(&mut hash, fact.kind.as_bytes());
            checksum_update(&mut hash, fact.source_file.as_bytes());
            checksum_update(&mut hash, format!("{:?}", fact.source_range).as_bytes());
            if let Some(symbol) = &fact.symbol {
                checksum_update(&mut hash, symbol.as_bytes());
            }
            if let Some(method) = &fact.method {
                checksum_update(&mut hash, method.as_bytes());
            }
            if let Some(path) = &fact.path {
                checksum_update(&mut hash, path.as_bytes());
            }
            for evidence in &fact.evidence {
                checksum_update(&mut hash, evidence.as_bytes());
            }
            for (key, value) in &fact.properties {
                checksum_update(&mut hash, key.as_bytes());
                checksum_update(&mut hash, value.as_bytes());
            }
        }
    }
    for coverage in &output.coverage {
        checksum_update(&mut hash, coverage.language.as_bytes());
        checksum_update(&mut hash, coverage.path.as_bytes());
        checksum_update(&mut hash, coverage.status.as_bytes());
        if let Some(reason) = &coverage.reason {
            checksum_update(&mut hash, reason.as_bytes());
        }
    }
    for diagnostic in &output.diagnostics {
        checksum_update(&mut hash, diagnostic.language.as_bytes());
        checksum_update(&mut hash, diagnostic.level.as_bytes());
        checksum_update(&mut hash, diagnostic.code.as_str().as_bytes());
        checksum_update(&mut hash, diagnostic.message.as_bytes());
        if let Some(path) = &diagnostic.path {
            checksum_update(&mut hash, path.as_bytes());
        }
        if let Some(line) = diagnostic.line {
            checksum_update(&mut hash, line.to_string().as_bytes());
        }
    }
    hash_source_snapshot(source_snapshot, &mut hash);
    checksum_update(&mut hash, b"project-config-digest");
    checksum_update(&mut hash, &project_config_digest.to_le_bytes());
    hash_pack_files(&pack_root.join("packs").join("framework"), &mut hash);
    format!("{hash:016x}")
}
pub(crate) fn framework_cache_key(
    root: &Path,
    pack_root: &Path,
    documents: &[DocumentOutput],
    source_snapshot: &SourceSnapshot,
    project_config_digest: u64,
) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    // v27: JavaScript middleware/route facts are owned by their actual
    // framework syntax; call-expression clients such as
    // `request(server).get(...)` cannot masquerade as server registrations;
    // and anonymous Fastify hooks do not invent callback-parameter targets.
    checksum_update(&mut hash, b"code-memory-framework-cache.v27");
    checksum_update(&mut hash, root.to_string_lossy().as_bytes());
    for document in documents {
        checksum_update(&mut hash, document.path.as_bytes());
        for symbol in &document.symbols {
            checksum_update(&mut hash, symbol.symbol.as_bytes());
            checksum_update(&mut hash, symbol.kind.as_bytes());
            if let Some(enclosing) = &symbol.enclosing_symbol {
                checksum_update(&mut hash, enclosing.as_bytes());
            }
        }
    }
    hash_source_snapshot(source_snapshot, &mut hash);
    checksum_update(&mut hash, b"project-config-digest");
    checksum_update(&mut hash, &project_config_digest.to_le_bytes());
    hash_pack_files(&pack_root.join("packs").join("framework"), &mut hash);
    format!("{hash:016x}")
}
pub(crate) fn load_framework_cache(path: &Path) -> Option<crate::frameworks::Analysis> {
    let (bytes, legacy) = read_compressed_or_legacy(path).ok()?;
    let analysis = serde_json::from_slice(&bytes).ok()?;
    if legacy && write_compressed_json(path, &analysis).is_ok() {
        let _ = fs::remove_file(path.with_extension(""));
    }
    Some(analysis)
}
pub(crate) fn write_framework_cache(
    path: &Path,
    analysis: &crate::frameworks::Analysis,
) -> Result<(), String> {
    write_compressed_json(path, analysis)
}
fn hash_pack_files(root: &Path, hash: &mut u64) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            hash_pack_files(&path, hash);
        } else if path.extension().and_then(|value| value.to_str()) == Some("json") {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            checksum_update(hash, relative.to_string_lossy().as_bytes());
            if let Ok(bytes) = fs::read(&path) {
                checksum_update(hash, &bytes);
            }
        }
    }
}
pub(crate) fn cache_root(_root: &Path) -> PathBuf {
    let base = env::var_os("CODE_MEMORY_CACHE_ROOT")
        .or_else(|| env::var_os("LOCALAPPDATA"))
        .map(PathBuf::from)
        .or_else(|| env::var_os("XDG_CACHE_HOME").map(PathBuf::from))
        .or_else(|| {
            env::var_os("HOME").map(|home| {
                let home = PathBuf::from(home);
                if cfg!(target_os = "macos") {
                    home.join("Library").join("Caches")
                } else {
                    home.join(".cache")
                }
            })
        })
        .unwrap_or_else(env::temp_dir);
    base.join("CodebaseWorkspace")
        .join("cache")
        .join("code-memory")
}
pub(crate) fn project_cache_root(root: &Path) -> PathBuf {
    let mut hash = 0xcbf29ce484222325u64;
    checksum_update(&mut hash, root.to_string_lossy().as_bytes());
    cache_root(root).join(format!("{hash:016x}"))
}

pub(crate) fn cache_impact(
    root: &Path,
    snapshot: &SourceSnapshot,
    dependency_context_digest: u64,
) -> CacheImpact {
    let manifest_path = project_cache_root(root).join("source-manifest-v2.json");
    let force_all = || CacheImpact {
        force_all: true,
        affected_paths: snapshot.file_hashes.keys().cloned().collect(),
    };
    let manifest = fs::read(&manifest_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<SourceManifest>(&bytes).ok())
        .filter(|manifest| {
            manifest.schema == "code-memory.source-manifest.v2"
                && manifest.dependency_context_digest == dependency_context_digest
        });
    let Some(manifest) = manifest else {
        // A provider may have completed minutes of work before a later
        // canonical validation failed. The provider caches are still valid if
        // and only if the entire source/config input is byte-for-byte the same
        // on retry. This provisional receipt deliberately has no reverse-import
        // graph, so it can authorize only an exact unchanged retry.
        if provider_run_input_matches(root, snapshot, dependency_context_digest) {
            return CacheImpact::default();
        }
        return force_all();
    };
    let mut changed = HashSet::new();
    for (path, hash) in &snapshot.file_hashes {
        if manifest.files.get(path) != Some(hash) {
            changed.insert(path.clone());
        }
    }
    changed.extend(
        manifest
            .files
            .keys()
            .filter(|path| !snapshot.file_hashes.contains_key(*path))
            .cloned(),
    );
    if changed.is_empty() {
        return CacheImpact::default();
    }

    let mut affected_paths = changed.clone();
    let mut pending: Vec<String> = changed.into_iter().collect();
    while let Some(path) = pending.pop() {
        let Some(importers) = manifest.reverse_imports.get(&path) else {
            continue;
        };
        for importer in importers {
            if affected_paths.insert(importer.clone()) {
                pending.push(importer.clone());
            }
        }
    }
    CacheImpact {
        force_all: false,
        affected_paths,
    }
}

pub(crate) fn write_provider_run_input_manifest(
    root: &Path,
    snapshot: &SourceSnapshot,
    dependency_context_digest: u64,
) -> Result<(), String> {
    let cache_root = project_cache_root(root);
    fs::create_dir_all(&cache_root)
        .map_err(|error| format!("cannot create provider-run input cache: {error}"))?;
    let path = cache_root.join("provider-run-input-v1.json");
    let manifest = ProviderRunInputManifest {
        schema: "code-memory.provider-run-input.v1".to_string(),
        dependency_context_digest,
        files: snapshot.file_hashes.clone(),
    };
    let bytes = serde_json::to_vec(&manifest)
        .map_err(|error| format!("cannot serialize provider-run input: {error}"))?;
    let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
    write_cache_file(&temporary, &bytes)?;
    let _ = fs::remove_file(&path);
    if let Err(error) = fs::rename(&temporary, &path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("cannot promote provider-run input: {error}"));
    }
    Ok(())
}

fn provider_run_input_matches(
    root: &Path,
    snapshot: &SourceSnapshot,
    dependency_context_digest: u64,
) -> bool {
    let path = project_cache_root(root).join("provider-run-input-v1.json");
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<ProviderRunInputManifest>(&bytes).ok())
        .is_some_and(|manifest| {
            manifest.schema == "code-memory.provider-run-input.v1"
                && manifest.dependency_context_digest == dependency_context_digest
                && manifest.files == snapshot.file_hashes
        })
}

pub(crate) fn write_source_manifest(
    root: &Path,
    snapshot: &SourceSnapshot,
    reverse_imports: &HashMap<String, Vec<String>>,
    dependency_context_digest: u64,
) -> Result<(), String> {
    let cache_root = project_cache_root(root);
    let path = cache_root.join("source-manifest-v2.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create source manifest cache: {error}"))?;
    }
    let manifest = SourceManifest {
        schema: "code-memory.source-manifest.v2".to_string(),
        dependency_context_digest,
        files: snapshot.file_hashes.clone(),
        reverse_imports: reverse_imports.clone(),
    };
    let bytes = serde_json::to_vec(&manifest)
        .map_err(|error| format!("cannot serialize source manifest: {error}"))?;
    let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
    write_cache_file(&temporary, &bytes)?;
    let _ = fs::remove_file(&path);
    if let Err(error) = fs::rename(&temporary, &path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("cannot promote source manifest: {error}"));
    }
    let _ = fs::remove_file(cache_root.join("source-manifest-v1.json"));
    Ok(())
}

pub(crate) fn read_architecture_reverse_imports(
    path: &Path,
) -> Result<HashMap<String, Vec<String>>, String> {
    let (bytes, _) = read_compressed_or_legacy(path)?;
    let previous = serde_json::from_slice::<PreviousArchitecture>(&bytes)
        .map_err(|error| format!("cannot parse dependency cache: {error}"))?;
    let mut reverse_imports = HashMap::new();
    for edge in previous.edges {
        if edge.kind == "IMPORTS" {
            record_architecture_reverse_import(&mut reverse_imports, &edge.from, &edge.to);
        }
    }
    canonicalize_reverse_imports(&mut reverse_imports);
    Ok(reverse_imports)
}

pub(crate) fn record_architecture_reverse_import(
    reverse_imports: &mut HashMap<String, Vec<String>>,
    from: &str,
    to: &str,
) {
    let (Some(from), Some(to)) = (architecture_file_path(from), architecture_file_path(to)) else {
        return;
    };
    reverse_imports.entry(to).or_default().push(from);
}

pub(crate) fn record_file_reverse_import(
    reverse_imports: &mut HashMap<String, Vec<String>>,
    from: &str,
    to: &str,
) {
    reverse_imports
        .entry(normalize_file_relation_path(to))
        .or_default()
        .push(normalize_file_relation_path(from));
}

pub(crate) fn canonicalize_reverse_imports(reverse_imports: &mut HashMap<String, Vec<String>>) {
    for importers in reverse_imports.values_mut() {
        importers.sort();
        importers.dedup();
    }
}

pub(crate) fn commit_cache_generation(
    root: &Path,
    active_files: impl IntoIterator<Item = PathBuf>,
) -> Result<(), String> {
    let cache_base = cache_root(root);
    let manifest_dir = project_cache_root(root);
    commit_cache_generation_in(&cache_base, &manifest_dir, active_files)
}

fn commit_cache_generation_in(
    cache_base: &Path,
    manifest_dir: &Path,
    active_files: impl IntoIterator<Item = PathBuf>,
) -> Result<(), String> {
    fs::create_dir_all(manifest_dir)
        .map_err(|error| format!("cannot create cache generation directory: {error}"))?;
    let mut files = active_files
        .into_iter()
        .filter(|path| path.is_file())
        .filter_map(|path| {
            path.strip_prefix(cache_base)
                .ok()
                .map(normalized_cache_path)
        })
        .collect::<Vec<_>>();
    files.sort();
    files.dedup();

    let current_path = manifest_dir.join("cache-generation-current-v1.json");
    let previous_path = manifest_dir.join("cache-generation-previous-v1.json");
    let previous_current = read_cache_generation(&current_path);
    let previous_previous = read_cache_generation(&previous_path);
    let manifest = CacheGenerationManifest {
        schema: "code-memory.cache-generation.v1".to_string(),
        files,
    };
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = manifest_dir.join(format!(
        "cache-generation.{}.{nonce}.tmp",
        std::process::id()
    ));
    let bytes = serde_json::to_vec(&manifest)
        .map_err(|error| format!("cannot serialize cache generation: {error}"))?;
    write_cache_file(&temporary, &bytes)?;

    if previous_current.is_some() {
        let _ = fs::remove_file(&previous_path);
        if let Err(error) = fs::rename(&current_path, &previous_path) {
            let _ = fs::remove_file(&temporary);
            return Err(format!("cannot rotate cache generation: {error}"));
        }
    } else {
        let _ = fs::remove_file(&current_path);
        let _ = fs::remove_file(&previous_path);
    }
    if let Err(error) = fs::rename(&temporary, &current_path) {
        if previous_current.is_some() {
            let _ = fs::rename(&previous_path, &current_path);
        }
        let _ = fs::remove_file(&temporary);
        return Err(format!("cannot promote cache generation: {error}"));
    }

    let mut retained = manifest.files.iter().cloned().collect::<HashSet<_>>();
    if let Some(previous) = previous_current.as_ref() {
        retained.extend(previous.files.iter().cloned());
    }
    let mut known = retained.clone();
    if let Some(previous) = previous_previous {
        known.extend(previous.files);
    }
    prune_managed_cache_files(cache_base, &known, &retained);
    prune_unreferenced_project_cache_dirs(cache_base, manifest_dir, &known, &retained);
    Ok(())
}

fn read_cache_generation(path: &Path) -> Option<CacheGenerationManifest> {
    serde_json::from_slice::<CacheGenerationManifest>(&fs::read(path).ok()?)
        .ok()
        .filter(|manifest| manifest.schema == "code-memory.cache-generation.v1")
}

fn write_cache_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let file = fs::File::create(path)
        .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    let mut writer = BufWriter::new(file);
    writer
        .write_all(bytes)
        .and_then(|_| writer.flush())
        .and_then(|_| writer.get_ref().sync_all())
        .map_err(|error| format!("cannot write {}: {error}", path.display()))
}

pub(crate) fn read_compressed_cache(path: &Path) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path)
        .map_err(|error| format!("cannot open compressed cache {}: {error}", path.display()))?;
    let mut decoder = GzDecoder::new(file);
    let mut bytes = Vec::new();
    decoder
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read compressed cache {}: {error}", path.display()))?;
    Ok(bytes)
}

pub(crate) fn write_compressed_json<T: Serialize + ?Sized>(
    path: &Path,
    value: &T,
) -> Result<(), String> {
    write_compressed(path, |encoder| {
        serde_json::to_writer(encoder, value)
            .map_err(|error| format!("cannot serialize compressed cache: {error}"))
    })
}

pub(crate) fn write_compressed_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    write_compressed(path, |encoder| {
        encoder
            .write_all(bytes)
            .map_err(|error| format!("cannot write compressed cache: {error}"))
    })
}

fn write_compressed(
    path: &Path,
    write: impl FnOnce(&mut GzEncoder<BufWriter<fs::File>>) -> Result<(), String>,
) -> Result<(), String> {
    if path.is_file() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = path.with_extension(format!("gz.{}.{nonce}.tmp", std::process::id()));
    let file = fs::File::create(&temporary)
        .map_err(|error| format!("cannot create {}: {error}", temporary.display()))?;
    let mut encoder = GzEncoder::new(BufWriter::new(file), Compression::fast());
    if let Err(error) = write(&mut encoder) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    let mut writer = match encoder.finish() {
        Ok(writer) => writer,
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            return Err(format!("cannot finish compressed cache: {error}"));
        }
    };
    if let Err(error) = writer.flush().and_then(|_| writer.get_ref().sync_all()) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("cannot flush compressed cache: {error}"));
    }
    if path.is_file() {
        let _ = fs::remove_file(&temporary);
        return Ok(());
    }
    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!(
            "cannot promote compressed cache {}: {error}",
            path.display()
        )
    })
}

pub(crate) fn read_compressed_or_legacy(path: &Path) -> Result<(Vec<u8>, bool), String> {
    match read_compressed_cache(path) {
        Ok(bytes) => Ok((bytes, false)),
        Err(compressed_error) => {
            let legacy = path.with_extension("");
            fs::read(&legacy)
                .map(|bytes| (bytes, true))
                .map_err(|legacy_error| {
                    format!(
                        "cannot read cache {} ({compressed_error}); legacy {}: {legacy_error}",
                        path.display(),
                        legacy.display()
                    )
                })
        }
    }
}

fn normalized_cache_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn prune_managed_cache_files(
    cache_base: &Path,
    known_files: &HashSet<String>,
    retained_files: &HashSet<String>,
) {
    let directories = known_files
        .iter()
        .filter_map(|relative| Path::new(relative).parent())
        .map(|relative| cache_base.join(relative))
        .collect::<HashSet<_>>();
    for directory in directories {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() || !is_content_addressed_cache_file(&path) {
                continue;
            }
            let Some(relative) = path
                .strip_prefix(cache_base)
                .ok()
                .map(normalized_cache_path)
            else {
                continue;
            };
            if !retained_files.contains(&relative) {
                let _ = fs::remove_file(path);
            }
        }
    }
}

fn prune_unreferenced_project_cache_dirs(
    cache_base: &Path,
    manifest_dir: &Path,
    known_files: &HashSet<String>,
    retained_files: &HashSet<String>,
) {
    let retained = retained_files
        .iter()
        .filter_map(|path| cache_project_directory(path))
        .collect::<HashSet<_>>();
    let candidates = known_files
        .iter()
        .filter_map(|path| cache_project_directory(path))
        .collect::<HashSet<_>>();

    for name in candidates.difference(&retained) {
        let directory = cache_base.join(name);
        if directory != manifest_dir && directory.parent() == Some(cache_base) {
            let _ = fs::remove_dir_all(directory);
        }
    }
}

fn cache_project_directory(path: &str) -> Option<String> {
    let name = Path::new(path).components().next()?.as_os_str().to_str()?;
    (name.len() == 16 && name.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| name.to_string())
}

fn is_content_addressed_cache_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    let Some(stem) = name
        .strip_suffix(".json.gz")
        .or_else(|| name.strip_suffix(".json"))
    else {
        return false;
    };
    let Some((_, digest)) = stem.rsplit_once('-') else {
        return false;
    };
    digest.len() == 16 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn architecture_file_path(id: &str) -> Option<String> {
    id.strip_prefix("file:").map(|path| path.replace('\\', "/"))
}

fn normalize_file_relation_path(path: &str) -> String {
    path.strip_prefix("file:")
        .unwrap_or(path)
        .replace('\\', "/")
}
pub(crate) fn cleanup_stale_provider_work(root: &Path, max_age: Duration) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_dir() {
            continue;
        }
        let stale = metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > max_age);
        if stale {
            let _ = fs::remove_dir_all(path);
        }
    }
}
pub(crate) fn project_config_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_project_config_files(root, &mut files);
    files.sort();
    files
}

pub(crate) fn project_config_digest(root: &Path) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    hash_project_config_files(root, &mut hash);
    for name in [
        "CODE_MEMORY_OFFLINE",
        "CODE_MEMORY_ALLOW_NETWORK",
        "CODE_MEMORY_RUST_SEMANTIC_MAX_FILES",
        "CODE_MEMORY_LSP_TIMEOUT_MS",
        "CODE_MEMORY_LSP_MAX_REQUESTS",
        "CODE_MEMORY_LSP_MAX_SECONDS",
        "CODE_MEMORY_PROVIDER_TIMEOUT_SECONDS",
        "CODE_MEMORY_LSP_REFERENCES",
        // Provider-inherited semantic axes. Only their bounded hash reaches
        // the cache key; raw values are not published in product output.
        "GOOS",
        "GOARCH",
        "GOFLAGS",
        "GOWORK",
        "CGO_ENABLED",
        "CARGO_BUILD_TARGET",
        "RUSTFLAGS",
        "RUSTUP_TOOLCHAIN",
        "JAVA_HOME",
        "CODE_MEMORY_JAVA_TOOLCHAIN_PATHS",
        "JAVA_TOOL_OPTIONS",
        "GRADLE_OPTS",
        "MAVEN_OPTS",
        "DOTNET_ROOT",
        "VIRTUAL_ENV",
        "PYTHONPATH",
        "PUB_CACHE",
        "FLUTTER_ROOT",
    ] {
        checksum_update(&mut hash, name.as_bytes());
        match env::var_os(name) {
            Some(value) => checksum_update(&mut hash, value.to_string_lossy().as_bytes()),
            None => checksum_update(&mut hash, b"<unset>"),
        }
    }
    hash
}

pub(crate) fn source_dependency_context_digest(
    providers_root: Option<&Path>,
    project_config_digest: u64,
) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    // Source census/planning compatibility is an explicit contract. Hashing
    // the whole executable here made any unrelated engine or UI-adjacent Rust
    // rebuild mark every source file as affected, which in turn bypassed the
    // otherwise stable per-language caches. Bump this marker whenever census,
    // dependency closure, or AnalysisPlan semantics change.
    checksum_update(&mut hash, b"code-memory.source-dependency-context.v2");
    checksum_update(&mut hash, &project_config_digest.to_le_bytes());
    if let Some(providers_root) = providers_root {
        if let Some(manifest_hash) = cached_file_checksum(&providers_root.join("manifest.json")) {
            checksum_update(&mut hash, b"provider-manifest");
            checksum_update(&mut hash, &manifest_hash.to_le_bytes());
        }
    }
    hash
}
fn collect_project_config_files(dir: &Path, files: &mut Vec<PathBuf>) {
    if is_managed_provider_root(dir) {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if !is_excluded_source_dir(&entry.file_name().to_string_lossy()) {
                collect_project_config_files(&path, files);
            }
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if file_type.is_file() && is_project_config_name(&name) {
            files.push(path);
        }
    }
}
fn is_project_config_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if ((lower.starts_with("tsconfig.") || lower.starts_with("jsconfig."))
        && lower.ends_with(".json"))
        || matches!(lower.as_str(), "tsconfig.json" | "jsconfig.json")
    {
        return true;
    }
    matches!(
        name,
        "package.json"
            | "package-lock.json"
            | "pnpm-lock.yaml"
            | "yarn.lock"
            | "pyproject.toml"
            | "pyrightconfig.json"
            | "requirements.txt"
            | "requirements-dev.txt"
            | "poetry.lock"
            | "uv.lock"
            | "Pipfile"
            | "Pipfile.lock"
            | "setup.py"
            | "setup.cfg"
            | "global.json"
            | "NuGet.config"
            | "nuget.config"
            | "Cargo.toml"
            | "Cargo.lock"
            | "go.mod"
            | "go.sum"
            | "go.work"
            | "go.work.sum"
            | "pom.xml"
            | "build.gradle"
            | "build.gradle.kts"
            | "settings.gradle"
            | "settings.gradle.kts"
            | "gradle.properties"
            | "libs.versions.toml"
            | "pubspec.yaml"
            | "pubspec.lock"
            | "pubspec_overrides.yaml"
            | "analysis_options.yaml"
            | "compile_commands.json"
            | "compile_flags.txt"
            | ".clangd"
            | "CMakeLists.txt"
            | "CMakePresets.json"
            | "meson.build"
            | "conf/routes"
    ) || lower.ends_with(".csproj")
        || lower.ends_with(".sln")
        || lower.ends_with(".slnx")
        || lower.ends_with(".vcxproj")
        || lower.ends_with(".vcxproj.filters")
        || lower.ends_with(".props")
        || lower.ends_with(".targets")
        || lower.ends_with(".ruleset")
}
fn hash_project_config_files(root: &Path, hash: &mut u64) {
    for path in project_config_files(root) {
        let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy();
        checksum_update(hash, relative.as_bytes());
        if let Ok(bytes) = fs::read(path) {
            checksum_update(hash, &bytes);
        }
    }
    // Build directories are excluded from source discovery, but a generated
    // compile database is semantic input for C/C++ and must invalidate caches.
    for database in compile_database_dirs(root) {
        let path = database.join("compile_commands.json");
        checksum_update(hash, b"resolved-compile-database");
        checksum_update(hash, path.to_string_lossy().as_bytes());
        if let Ok(bytes) = fs::read(path) {
            checksum_update(hash, &bytes);
        }
    }
}
pub(crate) fn hash_source_snapshot(snapshot: &SourceSnapshot, hash: &mut u64) {
    let mut files: Vec<_> = snapshot.file_hashes.iter().collect();
    files.sort_by(|left, right| left.0.cmp(right.0));
    for (path, source_hash) in files {
        checksum_update(hash, path.as_bytes());
        checksum_update(hash, &source_hash.to_le_bytes());
    }
}

pub(crate) fn typescript_project_model_cache_key(
    root: &Path,
    files: &[PathBuf],
    providers_root: Option<&Path>,
    config_digest: u64,
    source_snapshot: &SourceSnapshot,
) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    checksum_update(&mut hash, b"code-memory-tsjs-project-model.v1");
    checksum_update(&mut hash, root.to_string_lossy().as_bytes());
    checksum_update(&mut hash, &config_digest.to_le_bytes());
    if let Some(providers_root) = providers_root {
        checksum_update(&mut hash, providers_root.to_string_lossy().as_bytes());
        if let Some(file_hash) = cached_file_checksum(&providers_root.join("manifest.json")) {
            checksum_update(&mut hash, &file_hash.to_le_bytes());
        }
        if let Some(file_hash) =
            cached_file_checksum(&providers_root.join("node").join("project-model.cjs"))
        {
            checksum_update(&mut hash, &file_hash.to_le_bytes());
        }
    }
    if let Some(node) = find_tool("node", providers_root) {
        checksum_update(&mut hash, node.to_string_lossy().as_bytes());
        if let Some(file_hash) = cached_file_checksum(&node) {
            checksum_update(&mut hash, &file_hash.to_le_bytes());
        }
    }
    let mut files: Vec<_> = files.iter().collect();
    files.sort();
    for file in files {
        let relative = file
            .strip_prefix(root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        checksum_update(&mut hash, relative.as_bytes());
        if let Some(file_hash) = source_snapshot.file_hashes.get(&relative) {
            checksum_update(&mut hash, &file_hash.to_le_bytes());
        } else {
            checksum_update(&mut hash, b"unreadable-source");
        }
    }
    format!("{hash:016x}")
}

pub(crate) struct LanguageCacheKeyInput<'a> {
    pub(crate) root: &'a Path,
    pub(crate) lang: &'a LanguageSpec,
    pub(crate) files: &'a [PathBuf],
    pub(crate) providers_root: Option<&'a Path>,
    pub(crate) config_digest: u64,
    pub(crate) source_snapshot: &'a SourceSnapshot,
    pub(crate) execution_scope_id: &'a str,
    pub(crate) provider_config: Option<&'a Path>,
}

pub(crate) fn language_cache_key(input: LanguageCacheKeyInput<'_>) -> String {
    let LanguageCacheKeyInput {
        root,
        lang,
        files,
        providers_root,
        config_digest,
        source_snapshot,
        execution_scope_id,
        provider_config,
    } = input;
    let mut hash = 0xcbf29ce484222325u64;
    // This explicit contract version owns provider-output normalization.
    // Bump it whenever SCIP/LSP decoding or provider-side normalization can
    // change the cached documents/relations/context. Hashing the whole engine
    // executable here would invalidate minutes of provider work for unrelated
    // UI, canonical-linker, storage, or validation changes.
    checksum_update(&mut hash, b"code-memory-language-cache.v154");
    checksum_update(&mut hash, root.to_string_lossy().as_bytes());
    checksum_update(&mut hash, lang.id.as_bytes());
    if matches!(lang.id, "typescript" | "javascript") {
        // Configless shards use an isolated generated project.  This marker
        // invalidates only TS/JS caches created by the old --infer-tsconfig
        // path, which could write tsconfig.json into the selected repository.
        checksum_update(&mut hash, b"tsjs-isolated-source-only.v1");
    }
    if lang.id == "rust" {
        // Large Rust workspaces used to omit public impl methods from call
        // enrichment. Keep old provider results from surviving the corrected
        // visibility boundary without invalidating unrelated languages.
        checksum_update(&mut hash, b"rust-public-impl-boundary.v1");
    }
    if lang.id == "csharp" {
        // scip-dotnet commonly omits occurrence enclosing ranges. Calls in an
        // expression-bodied method were therefore attached to the class by
        // the old brace fallback, destroying executable flow paths. Invalidate
        // only C# provider-normalization caches after exact syntax-owner repair.
        checksum_update(&mut hash, b"csharp-exact-call-owner.v2");
    }
    if lang.id == "java" {
        // Source-only JDTLS must open every scheduled file because there is no
        // imported build project to index unopened documents. Invalidate the
        // former 256-document fallback shards, which reported file coverage
        // while omitting most Java definitions.
        checksum_update(&mut hash, b"java-source-only-all-documents.v2");
        // Java/C# providers run in a writable manifest-backed copy. The old
        // copy omitted unsupported-but-required build support files such as
        // Gradle wrapper JARs and Checkstyle XML, so a valid build project
        // could silently fall back to source-only semantics. Invalidate those
        // provider results after the execution-fidelity repair.
        checksum_update(&mut hash, b"java-provider-support-files.v1");
        // Large JDTLS sessions now use workload/memory-aware heap sizing and
        // suppress editor-only source diagnostics. Do not retain an empty or
        // partial result produced by the former fixed 1 GiB process.
        checksum_update(&mut hash, b"java-jdtls-large-workspace.v8");
        // Large Java call flow now resolves exact syntax call sites directly
        // and schedules them fairly across source modules. The former
        // call-hierarchy prefix could fill the entire budget with
        // alphabetically early modules and omit valid web/API flows.
        checksum_update(&mut hash, b"java-direct-call-sites.v1");
        // JDTLS display labels can include complete parameter lists or report
        // a malformed 0:0 selection. Definition evidence now uses the exact
        // source name token and repairs an invalid selection only from a
        // unique match inside the provider declaration range.
        checksum_update(&mut hash, b"java-definition-name-evidence.v2");
    }
    if matches!(
        lang.id,
        "c" | "cpp" | "dart" | "go" | "java" | "python" | "rust"
    ) {
        // Large LSP workspaces now complete call-hierarchy chunks end-to-end
        // and reserve request budget across hierarchy, call, and type-use
        // capabilities. Old shards may contain prepare-only starvation.
        checksum_update(&mut hash, b"lsp-capability-budget-scheduler.v2");
    }
    // The same files under the same repository-wide config digest can be
    // interpreted by a different project config after ownership planning
    // changes.  NestJS exposed the collision: packages/platform-ws cached a
    // result executed with integration/websockets/tsconfig.json and reused it
    // for packages/platform-ws/tsconfig.build.json.  Bind provider output to
    // the stable planned scope plus the exact generated/explicit config bytes.
    checksum_update(&mut hash, b"execution-scope");
    checksum_update(&mut hash, execution_scope_id.as_bytes());
    if let Some(provider_config) = provider_config {
        checksum_update(&mut hash, b"provider-config");
        if let Ok(bytes) = fs::read(provider_config) {
            checksum_update(&mut hash, &bytes);
        } else {
            checksum_update(&mut hash, b"unreadable-provider-config");
        }
    }
    let provider_program = find_tool(lang.tool, providers_root).or_else(|| {
        matches!(lang.id, "c" | "cpp")
            .then(|| find_tool("clangd", providers_root))
            .flatten()
    });
    if let Some(provider_program) = provider_program {
        checksum_update(&mut hash, b"provider-executable");
        checksum_update(&mut hash, provider_program.to_string_lossy().as_bytes());
        if let Some(file_hash) = cached_file_checksum(&provider_program) {
            checksum_update(&mut hash, &file_hash.to_le_bytes());
        }
    }
    if let Some(providers_root) = providers_root {
        if let Some(file_hash) = cached_file_checksum(&providers_root.join("manifest.json")) {
            checksum_update(&mut hash, b"provider-manifest");
            checksum_update(&mut hash, &file_hash.to_le_bytes());
        }
    }
    checksum_update(&mut hash, b"project-config-digest");
    checksum_update(&mut hash, &config_digest.to_le_bytes());
    for file in files {
        let relative = file
            .strip_prefix(root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        checksum_update(&mut hash, relative.as_bytes());
        if let Some(file_hash) = source_snapshot.file_hashes.get(&relative) {
            checksum_update(&mut hash, &file_hash.to_le_bytes());
        } else if let Ok(bytes) = fs::read(file) {
            // Files outside the collected source snapshot still participate in
            // invalidation instead of silently producing a reusable cache key.
            checksum_update(&mut hash, &bytes);
        }
    }
    format!("{hash:016x}")
}

fn cached_file_checksum(path: &Path) -> Option<u64> {
    let metadata = fs::metadata(path).ok()?;
    let length = metadata.len();
    let modified = metadata.modified().ok();
    let cache = FILE_CHECKSUM_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(cache) = cache.lock() {
        if let Some(entry) = cache.get(path) {
            if entry.length == length && entry.modified == modified {
                return Some(entry.checksum);
            }
        }
    }
    let bytes = fs::read(path).ok()?;
    let mut checksum = 0xcbf29ce484222325u64;
    checksum_update(&mut checksum, &bytes);
    if let Ok(mut cache) = cache.lock() {
        cache.insert(
            path.to_path_buf(),
            FileChecksumCacheEntry {
                length,
                modified,
                checksum,
            },
        );
    }
    Some(checksum)
}

fn checksum_update(state: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *state ^= u64::from(*byte);
        *state = state.wrapping_mul(0x100000001b3);
    }
}

fn checksum_range(state: &mut u64, range: &[i32]) {
    for value in range {
        checksum_update(state, &value.to_le_bytes());
    }
}
pub(crate) fn language_cache_path(root: &Path, lang: &LanguageSpec, key: &str) -> PathBuf {
    project_cache_root(root).join(format!("{}-{key}.json.gz", lang.id))
}
pub(crate) struct LanguageCacheRead {
    pub(crate) value: Option<CachedLanguageResult>,
    pub(crate) io_ms: u128,
    pub(crate) deserialize_ms: u128,
}

pub(crate) fn load_language_cache(
    root: &Path,
    lang: &LanguageSpec,
    key: &str,
) -> LanguageCacheRead {
    let io_started = Instant::now();
    let path = language_cache_path(root, lang, key);
    let Ok((value, legacy)) = read_compressed_or_legacy(&path) else {
        return LanguageCacheRead {
            value: None,
            io_ms: io_started.elapsed().as_millis(),
            deserialize_ms: 0,
        };
    };
    let io_ms = io_started.elapsed().as_millis();
    let deserialize_started = Instant::now();
    let cached = serde_json::from_slice::<CachedLanguageResult>(&value).ok();
    let deserialize_ms = deserialize_started.elapsed().as_millis();
    let value = cached.filter(|cached| {
        cached.schema == "code-memory.language-cache.v4"
            && cached.key == key
            // A provider can successfully prove that a scoped source file has
            // no semantic symbols. `write_language_cache` records that honest
            // result with EmptySemantic; rejecting it here made the same Dart
            // fixture shards launch a language server on every warm run.
            // Do not cache generic empty failures/timeouts: only the explicit
            // completed-empty receipt is reusable.
            && (!cached.documents.is_empty()
                || cached
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == DiagnosticCode::EmptySemantic))
    });
    if legacy {
        if let Some(cached) = value.as_ref() {
            if write_compressed_json(&path, cached).is_ok() {
                let _ = fs::remove_file(path.with_extension(""));
            }
        }
    }
    LanguageCacheRead {
        value,
        io_ms,
        deserialize_ms,
    }
}
pub(crate) fn write_language_cache(
    root: &Path,
    lang: LanguageSpec,
    key: &str,
    documents: &[DocumentOutput],
    relations: &[RelationOutput],
    diagnostics: &[Diagnostic],
    execution_context: &ProviderExecutionContext,
) {
    let directory = project_cache_root(root);
    if fs::create_dir_all(&directory).is_err() {
        return;
    }
    let cached = CachedLanguageResult {
        schema: "code-memory.language-cache.v4".to_string(),
        key: key.to_string(),
        documents: documents.to_vec(),
        relations: relations.to_vec(),
        execution_context: execution_context.clone(),
        diagnostics: diagnostics
            .iter()
            .map(|diagnostic| CachedDiagnostic {
                language: diagnostic.language.clone(),
                level: diagnostic.level.to_string(),
                code: diagnostic.code,
                message: diagnostic.message.clone(),
                detail: diagnostic.detail.clone(),
                path: diagnostic.path.clone(),
                line: diagnostic.line,
            })
            .collect(),
    };
    let _ = write_compressed_json(&language_cache_path(root, &lang, key), &cached);
}

pub(crate) struct ProviderWorkGuard(pub(crate) PathBuf);

impl Drop for ProviderWorkGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod generation_tests {
    use super::*;

    #[test]
    fn language_cache_identity_includes_planned_scope_and_provider_config() {
        let root = std::env::temp_dir().join(format!(
            "code-memory-language-cache-context-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        let source = root.join("src/main.ts");
        let first_config = root.join("tsconfig.first.json");
        let second_config = root.join("tsconfig.second.json");
        fs::write(&source, b"export const value = 1;\n").unwrap();
        fs::write(&first_config, br#"{"compilerOptions":{"strict":true}}"#).unwrap();
        fs::write(&second_config, br#"{"compilerOptions":{"strict":false}}"#).unwrap();
        let snapshot = SourceSnapshot {
            files: vec![(
                "src/main.ts".to_string(),
                "export const value = 1;\n".to_string(),
            )],
            file_hashes: HashMap::from([("src/main.ts".to_string(), 7)]),
            source_paths: vec![source.clone()],
        };
        let language = crate::LANGUAGES
            .iter()
            .find(|language| language.id == "typescript")
            .unwrap();
        let first = language_cache_key(LanguageCacheKeyInput {
            root: &root,
            lang: language,
            files: std::slice::from_ref(&source),
            providers_root: None,
            config_digest: 11,
            source_snapshot: &snapshot,
            execution_scope_id: "tsjs:first",
            provider_config: Some(&first_config),
        });
        let different_scope = language_cache_key(LanguageCacheKeyInput {
            root: &root,
            lang: language,
            files: std::slice::from_ref(&source),
            providers_root: None,
            config_digest: 11,
            source_snapshot: &snapshot,
            execution_scope_id: "tsjs:second",
            provider_config: Some(&first_config),
        });
        let different_config = language_cache_key(LanguageCacheKeyInput {
            root: &root,
            lang: language,
            files: std::slice::from_ref(&source),
            providers_root: None,
            config_digest: 11,
            source_snapshot: &snapshot,
            execution_scope_id: "tsjs:first",
            provider_config: Some(&second_config),
        });
        assert_ne!(first, different_scope);
        assert_ne!(first, different_config);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cache_gc_keeps_current_and_previous_complete_generations() {
        let root = std::env::temp_dir().join(format!(
            "code-memory-cache-generation-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let cache_base = root.join("cache");
        let project = cache_base.join("0123456789abcdef");
        fs::create_dir_all(&project).unwrap();
        let first = project.join("rust-1111111111111111.json");
        let second = project.join("rust-2222222222222222.json");
        let third = project.join("rust-3333333333333333.json");
        let stale = project.join("rust-0000000000000000.json");
        let source_manifest = project.join("source-manifest-v2.json");
        fs::write(&first, b"first").unwrap();
        fs::write(&stale, b"stale").unwrap();
        fs::write(&source_manifest, b"source").unwrap();

        commit_cache_generation_in(&cache_base, &project, [first.clone()]).unwrap();
        assert!(first.is_file());
        assert!(!stale.exists());
        assert!(source_manifest.is_file());

        fs::write(&second, b"second").unwrap();
        commit_cache_generation_in(&cache_base, &project, [second.clone()]).unwrap();
        assert!(first.is_file());
        assert!(second.is_file());

        fs::write(&third, b"third").unwrap();
        commit_cache_generation_in(&cache_base, &project, [third.clone()]).unwrap();
        assert!(!first.exists());
        assert!(second.is_file());
        assert!(third.is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cache_gc_removes_lsp_workspace_after_its_generation_expires() {
        let root =
            std::env::temp_dir().join(format!("code-memory-lsp-generation-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let cache_base = root.join("cache");
        let project = cache_base.join("aaaaaaaaaaaaaaaa");
        let first_project = cache_base.join("1111111111111111");
        let second_project = cache_base.join("2222222222222222");
        let third_project = cache_base.join("3333333333333333");
        fs::create_dir_all(first_project.join("lsp-workspaces/java-v2")).unwrap();
        fs::write(first_project.join("rust-1111111111111111.json"), b"first").unwrap();
        fs::write(
            first_project.join("lsp-workspaces/java-v2/index.bin"),
            b"lsp",
        )
        .unwrap();

        let first = first_project.join("rust-1111111111111111.json");
        commit_cache_generation_in(&cache_base, &project, [first]).unwrap();
        fs::create_dir_all(&second_project).unwrap();
        let second = second_project.join("rust-2222222222222222.json");
        fs::write(&second, b"second").unwrap();
        commit_cache_generation_in(&cache_base, &project, [second]).unwrap();
        assert!(first_project.is_dir());

        fs::create_dir_all(&third_project).unwrap();
        let third = third_project.join("rust-3333333333333333.json");
        fs::write(&third, b"third").unwrap();
        commit_cache_generation_in(&cache_base, &project, [third]).unwrap();

        assert!(!first_project.exists());
        assert!(second_project.is_dir());
        assert!(third_project.is_dir());
        fs::remove_dir_all(root).unwrap();
    }
}
