use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use crate::{compile_database_dirs, find_tool, is_excluded_source_dir};
use crate::{
    Diagnostic, DocumentOutput, IndexOutput, LanguageSpec, RelationOutput, SourceSnapshot,
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
    files: HashMap<String, u64>,
}

#[derive(Default)]
pub(crate) struct CacheImpact {
    pub(crate) force_all: bool,
    pub(crate) affected_paths: HashSet<String>,
}

#[derive(Deserialize)]
struct PreviousIndex {
    #[serde(default)]
    file_relations: Vec<crate::FileRelationOutput>,
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
    #[serde(default)]
    pub(crate) diagnostics: Vec<CachedDiagnostic>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct CachedDiagnostic {
    pub(crate) language: String,
    pub(crate) level: String,
    pub(crate) message: String,
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
    // serialized language index can stay identical while the Visual Map
    // projection changes (for example, preserving an unresolved route node).
    checksum_update(&mut hash, b"code-memory-architecture-cache.v17");
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
    checksum_update(&mut hash, b"code-memory-framework-cache.v5");
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
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}
pub(crate) fn write_framework_cache(
    path: &Path,
    analysis: &crate::frameworks::Analysis,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    let bytes = serde_json::to_vec(analysis)
        .map_err(|e| format!("cannot serialize framework cache: {e}"))?;
    fs::write(path, bytes)
        .map_err(|e| format!("cannot write framework cache {}: {e}", path.display()))
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
pub(crate) fn cache_root(root: &Path) -> PathBuf {
    let base = env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join(".code_memory"));
    base.join("VisualMap").join("cache").join("code-memory")
}
pub(crate) fn project_cache_root(root: &Path) -> PathBuf {
    let mut hash = 0xcbf29ce484222325u64;
    checksum_update(&mut hash, root.to_string_lossy().as_bytes());
    cache_root(root).join(format!("{hash:016x}"))
}

pub(crate) fn cache_impact(
    root: &Path,
    output: &Path,
    architecture_out: &Path,
    snapshot: &SourceSnapshot,
) -> CacheImpact {
    let manifest_path = project_cache_root(root).join("source-manifest-v1.json");
    let force_all = || CacheImpact {
        force_all: true,
        affected_paths: snapshot.file_hashes.keys().cloned().collect(),
    };
    let Some(manifest) = fs::read(&manifest_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<SourceManifest>(&bytes).ok())
        .filter(|manifest| manifest.schema == "code-memory.source-manifest.v1")
    else {
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

    let mut reverse_imports: HashMap<String, Vec<String>> = HashMap::new();
    let Some(previous) = fs::read(output)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<PreviousIndex>(&bytes).ok())
    else {
        return force_all();
    };
    for relation in previous.file_relations {
        if relation.kind == "IMPORTS" {
            reverse_imports
                .entry(normalize_file_relation_path(&relation.to))
                .or_default()
                .push(normalize_file_relation_path(&relation.from));
        }
    }
    let Some(previous) = fs::read(architecture_out)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<PreviousArchitecture>(&bytes).ok())
    else {
        return force_all();
    };
    for edge in previous.edges {
        if edge.kind != "IMPORTS" {
            continue;
        }
        let Some(from) = architecture_file_path(&edge.from) else {
            continue;
        };
        let Some(to) = architecture_file_path(&edge.to) else {
            continue;
        };
        reverse_imports.entry(to).or_default().push(from);
    }

    let mut affected_paths = changed.clone();
    let mut pending: Vec<String> = changed.into_iter().collect();
    while let Some(path) = pending.pop() {
        let Some(importers) = reverse_imports.get(&path) else {
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

pub(crate) fn write_source_manifest(root: &Path, snapshot: &SourceSnapshot) -> Result<(), String> {
    let path = project_cache_root(root).join("source-manifest-v1.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create source manifest cache: {error}"))?;
    }
    let manifest = SourceManifest {
        schema: "code-memory.source-manifest.v1".to_string(),
        files: snapshot.file_hashes.clone(),
    };
    let bytes = serde_json::to_vec(&manifest)
        .map_err(|error| format!("cannot serialize source manifest: {error}"))?;
    fs::write(path, bytes).map_err(|error| format!("cannot write source manifest: {error}"))
}

fn normalize_file_relation_path(path: &str) -> String {
    path.strip_prefix("file:")
        .unwrap_or(path)
        .replace('\\', "/")
}

fn architecture_file_path(id: &str) -> Option<String> {
    id.strip_prefix("file:").map(|path| path.replace('\\', "/"))
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
    hash
}
fn collect_project_config_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if !is_excluded_source_dir(&entry.file_name().to_string_lossy()) {
                collect_project_config_files(&path, files);
            }
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if is_project_config_name(&name) {
            files.push(path);
        }
    }
}
fn is_project_config_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    matches!(
        name,
        "package.json"
            | "package-lock.json"
            | "pnpm-lock.yaml"
            | "yarn.lock"
            | "tsconfig.json"
            | "jsconfig.json"
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
            | "composer.json"
            | "composer.lock"
            | "Gemfile"
            | "Gemfile.lock"
            | ".ruby-version"
            | ".ruby-gemset"
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

pub(crate) fn javascript_workspace(root: &Path, language: &str) -> PathBuf {
    project_cache_root(root).join("workspaces").join(language)
}
pub(crate) fn language_cache_key(
    root: &Path,
    lang: &LanguageSpec,
    files: &[PathBuf],
    providers_root: Option<&Path>,
    config_digest: u64,
    source_snapshot: &SourceSnapshot,
) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    // Provider-to-VisualMap normalization changes must not reuse a previous
    // provider result with the same source checksum.
    checksum_update(&mut hash, b"code-memory-language-cache.v131");
    checksum_update(&mut hash, root.to_string_lossy().as_bytes());
    checksum_update(&mut hash, lang.id.as_bytes());
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
    project_cache_root(root).join(format!("{}-{key}.json", lang.id))
}
pub(crate) fn load_language_cache(
    root: &Path,
    lang: &LanguageSpec,
    key: &str,
) -> Option<CachedLanguageResult> {
    let value = fs::read(language_cache_path(root, lang, key)).ok()?;
    let cached: CachedLanguageResult = serde_json::from_slice(&value).ok()?;
    (cached.schema == "code-memory.language-cache.v2"
        && cached.key == key
        && !cached.documents.is_empty())
    .then_some(cached)
}
pub(crate) fn write_language_cache(
    root: &Path,
    lang: LanguageSpec,
    key: &str,
    documents: &[DocumentOutput],
    relations: &[RelationOutput],
    diagnostics: &[Diagnostic],
) {
    let directory = project_cache_root(root);
    if fs::create_dir_all(&directory).is_err() {
        return;
    }
    let cached = CachedLanguageResult {
        schema: "code-memory.language-cache.v2".to_string(),
        key: key.to_string(),
        documents: documents.to_vec(),
        relations: relations.to_vec(),
        diagnostics: diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.path.is_some())
            .map(|diagnostic| CachedDiagnostic {
                language: diagnostic.language.clone(),
                level: diagnostic.level.to_string(),
                message: diagnostic.message.clone(),
                path: diagnostic.path.clone(),
                line: diagnostic.line,
            })
            .collect(),
    };
    let path = language_cache_path(root, &lang, key);
    let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
    let Ok(file) = fs::File::create(&temporary) else {
        return;
    };
    let mut writer = BufWriter::new(file);
    if serde_json::to_writer(&mut writer, &cached).is_ok() && writer.flush().is_ok() {
        let _ = fs::rename(temporary, path);
    } else {
        let _ = fs::remove_file(temporary);
    }
}

pub(crate) struct ProviderWorkGuard(pub(crate) PathBuf);

impl Drop for ProviderWorkGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
