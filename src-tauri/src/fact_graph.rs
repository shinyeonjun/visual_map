//! Desktop publication boundary for immutable canonical Fact Bundles.
//!
//! The code engine already produces the canonical SQLite payload.  The desktop
//! must not translate that payload into a second, weaker graph model.  This
//! module verifies the manifest, payload digest, every typed row, and all graph
//! references before copying the immutable bundle into the workspace and
//! publishing one small pointer file as the final atomic step.

use crate::workspace;
use codebase_fact_model::{
    coverage::{
        AnalysisGap, AnalysisIssue, AnalysisUnitReceipt, CapabilityReceipt, FileCoverageRecord,
        SourceScopeCoverageRecord,
    },
    evidence::FactEvidence,
    fact_graph::{FactBundleManifest, FactEdge, FactNode},
    identity::{Sha256Digest, SnapshotId},
    validation::Validate,
    ContractSchema,
};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, HashMap},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

const CANONICAL_ARTIFACT_SCHEMA: &str = "codebase-workspace.canonical-fact-bundle-artifact.v1";
const CANONICAL_SQLITE_SCHEMA: &str = "codebase-workspace.canonical-fact-bundle.sqlite.v1";
const PUBLISHED_POINTER_SCHEMA_VERSION: u32 = 1;
const PUBLISHED_POINTER_FILE: &str = "published-fact-v1.json";
const MAX_RESIDENT_SNAPSHOTS: usize = 2;
static SNAPSHOT_CACHE: OnceLock<Mutex<SnapshotCache>> = OnceLock::new();

#[derive(Default)]
struct SnapshotCache {
    clock: u64,
    entries: HashMap<PathBuf, CachedSnapshot>,
}

struct CachedSnapshot {
    snapshot: Arc<CanonicalFactSnapshot>,
    bundle_file: String,
    bundle_path: PathBuf,
    file_len: u64,
    modified: Option<SystemTime>,
    last_used: u64,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CanonicalFactBundleArtifact {
    pub schema: String,
    pub snapshot_id: SnapshotId,
    pub semantic_digest: Sha256Digest,
    pub bundle_digest: Sha256Digest,
    pub bundle_path: PathBuf,
    pub manifest_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PublishedFactPointer {
    schema_version: u32,
    manifest: FactBundleManifest,
    bundle_file: String,
    published_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PublishedFactBundle {
    pub manifest: FactBundleManifest,
    pub bundle_path: PathBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct CanonicalFactSnapshot {
    pub manifest: FactBundleManifest,
    pub nodes: Vec<FactNode>,
    pub edges: Vec<FactEdge>,
    pub evidence: Vec<FactEvidence>,
    pub file_coverage: Vec<FileCoverageRecord>,
    pub capability_receipts: Vec<CapabilityReceipt>,
    pub gaps: Vec<AnalysisGap>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FactGraphStatus {
    pub schema_version: u32,
    pub snapshot_id: Option<String>,
    pub source_revision: Option<String>,
    pub node_count: usize,
    pub edge_count: usize,
    pub evidence_count: usize,
    pub coverage_count: usize,
}

pub(crate) fn status_for_workspace(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
) -> Result<FactGraphStatus, String> {
    let Some(published) = published_bundle_for_workspace(app_data_dir, workspace_id)? else {
        return Ok(empty_status());
    };
    Ok(status_from_manifest(&published.manifest))
}

pub(crate) fn checkpoint_published_pointer(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
) -> Result<Option<Vec<u8>>, String> {
    // Validate the referenced bundle before accepting it as a rollback point.
    let current = published_bundle_for_workspace(&app_data_dir, workspace_id)?;
    if current.is_none() {
        return Ok(None);
    }
    let path =
        workspace::workspace_data_dir(app_data_dir, workspace_id)?.join(PUBLISHED_POINTER_FILE);
    fs::read(path)
        .map(Some)
        .map_err(|error| format!("Fact snapshot rollback point를 읽지 못했습니다: {error}"))
}

pub(crate) fn restore_published_pointer(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
    checkpoint: Option<&[u8]>,
) -> Result<(), String> {
    let workspace_dir = workspace::workspace_data_dir(app_data_dir, workspace_id)?;
    let path = workspace_dir.join(PUBLISHED_POINTER_FILE);
    match checkpoint {
        Some(bytes) => {
            let pointer: PublishedFactPointer = serde_json::from_slice(bytes)
                .map_err(|error| format!("Fact rollback pointer 형식 오류: {error}"))?;
            if pointer.manifest.workspace_id.as_str() != workspace_id {
                return Err("Fact rollback pointer의 workspace identity가 다릅니다".to_string());
            }
            replace_pointer_bytes(&path, bytes)
        }
        None => {
            if path.is_file() {
                fs::remove_file(path).map_err(|error| {
                    format!("실패한 Fact snapshot pointer rollback 실패: {error}")
                })?;
            }
            Ok(())
        }
    }
}

pub(crate) fn import_and_publish(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
    artifact: &CanonicalFactBundleArtifact,
) -> Result<FactGraphStatus, String> {
    if artifact.schema != CANONICAL_ARTIFACT_SCHEMA {
        return Err(format!(
            "지원하지 않는 canonical artifact schema입니다: {}",
            artifact.schema
        ));
    }
    let manifest_path = canonical_file(&artifact.manifest_path, "canonical manifest")?;
    let bundle_path = canonical_file(&artifact.bundle_path, "canonical bundle")?;
    let manifest_bytes = fs::read(&manifest_path)
        .map_err(|error| format!("canonical manifest를 읽지 못했습니다: {error}"))?;
    let manifest: FactBundleManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("canonical manifest 형식이 올바르지 않습니다: {error}"))?;
    manifest
        .validate()
        .map_err(|error| format!("canonical manifest 검증 실패: {error}"))?;

    if manifest.schema != ContractSchema::CanonicalFactV1
        || manifest.workspace_id.as_str() != workspace_id
        || manifest.snapshot_id != artifact.snapshot_id
        || manifest.semantic_digest != artifact.semantic_digest
        || manifest.bundle_digest != artifact.bundle_digest
    {
        return Err("canonical artifact와 manifest의 identity가 일치하지 않습니다".to_string());
    }
    let actual_digest = sha256_file(&bundle_path)?;
    if actual_digest != manifest.bundle_digest {
        return Err("canonical bundle SHA-256이 manifest와 일치하지 않습니다".to_string());
    }
    validate_bundle(&bundle_path, &manifest)?;

    let workspace_dir = workspace::workspace_data_dir(app_data_dir, workspace_id)?;
    let snapshot_dir = workspace_dir.join("fact-snapshots");
    fs::create_dir_all(&snapshot_dir)
        .map_err(|error| format!("Fact snapshot 폴더를 만들지 못했습니다: {error}"))?;
    let bundle_file = format!("canonical-{}.sqlite", manifest.bundle_digest);
    let target = snapshot_dir.join(&bundle_file);
    publish_immutable_bundle(&bundle_path, &target, manifest.bundle_digest)?;

    let pointer = PublishedFactPointer {
        schema_version: PUBLISHED_POINTER_SCHEMA_VERSION,
        manifest: manifest.clone(),
        bundle_file,
        published_at_unix_ms: unix_millis(),
    };
    publish_pointer(&workspace_dir.join(PUBLISHED_POINTER_FILE), &pointer)?;
    Ok(status_from_manifest(&manifest))
}

pub(crate) fn published_bundle_for_workspace(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
) -> Result<Option<PublishedFactBundle>, String> {
    let workspace_dir = workspace::workspace_data_dir(app_data_dir, workspace_id)?;
    let pointer_path = workspace_dir.join(PUBLISHED_POINTER_FILE);
    if !pointer_path.is_file() {
        return Ok(None);
    }
    let pointer_bytes = fs::read(&pointer_path)
        .map_err(|error| format!("게시된 Fact snapshot을 읽지 못했습니다: {error}"))?;
    let pointer: PublishedFactPointer = serde_json::from_slice(&pointer_bytes)
        .map_err(|error| format!("게시된 Fact snapshot 형식이 올바르지 않습니다: {error}"))?;
    if pointer.schema_version != PUBLISHED_POINTER_SCHEMA_VERSION {
        return Err("게시된 Fact snapshot schema migration이 필요합니다".to_string());
    }
    pointer
        .manifest
        .validate()
        .map_err(|error| format!("게시된 Fact manifest 검증 실패: {error}"))?;
    if pointer.manifest.workspace_id.as_str() != workspace_id {
        return Err("게시된 Fact snapshot의 workspace identity가 다릅니다".to_string());
    }
    validate_leaf_file_name(&pointer.bundle_file)?;
    let bundle_path = workspace_dir
        .join("fact-snapshots")
        .join(&pointer.bundle_file);
    let bundle_path = canonical_file(&bundle_path, "게시된 canonical bundle")?;
    let snapshot_root = workspace_dir
        .join("fact-snapshots")
        .canonicalize()
        .map_err(|error| {
            format!("Fact snapshot 폴더의 실제 경로를 확인하지 못했습니다: {error}")
        })?;
    if bundle_path.parent() != Some(snapshot_root.as_path()) {
        return Err("게시된 canonical bundle 경로가 workspace를 벗어났습니다".to_string());
    }
    if sha256_file(&bundle_path)? != pointer.manifest.bundle_digest {
        return Err("게시된 canonical bundle이 저장 후 변경되었습니다".to_string());
    }
    Ok(Some(PublishedFactBundle {
        manifest: pointer.manifest,
        bundle_path,
    }))
}

pub(crate) fn load_published_snapshot(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
) -> Result<Option<Arc<CanonicalFactSnapshot>>, String> {
    let cache_key = workspace::workspace_data_dir(&app_data_dir, workspace_id)?;
    let pointer = lightweight_pointer(&app_data_dir, workspace_id)?;
    let Some(pointer) = pointer else {
        if let Ok(mut cache) = snapshot_cache().lock() {
            cache.entries.remove(&cache_key);
        }
        return Ok(None);
    };
    if let Some(snapshot) = cached_snapshot(&cache_key, &pointer)? {
        return Ok(Some(snapshot));
    }
    let Some(published) = published_bundle_for_workspace(app_data_dir, workspace_id)? else {
        return Ok(None);
    };
    let connection = open_read_only(&published.bundle_path)?;
    let snapshot = Arc::new(CanonicalFactSnapshot {
        manifest: published.manifest,
        nodes: validated_rows(&connection, "nodes")?,
        edges: validated_rows(&connection, "edges")?,
        evidence: validated_rows(&connection, "evidence")?,
        file_coverage: validated_rows(&connection, "file_coverage")?,
        capability_receipts: validated_rows(&connection, "capability_receipts")?,
        gaps: validated_rows(&connection, "gaps")?,
    });
    cache_snapshot(
        cache_key,
        pointer.bundle_file,
        published.bundle_path,
        Arc::clone(&snapshot),
    )?;
    Ok(Some(snapshot))
}

fn snapshot_cache() -> &'static Mutex<SnapshotCache> {
    SNAPSHOT_CACHE.get_or_init(|| Mutex::new(SnapshotCache::default()))
}

fn lightweight_pointer(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
) -> Result<Option<PublishedFactPointer>, String> {
    let workspace_dir = workspace::workspace_data_dir(app_data_dir, workspace_id)?;
    let pointer_path = workspace_dir.join(PUBLISHED_POINTER_FILE);
    if !pointer_path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(pointer_path)
        .map_err(|error| format!("게시된 Fact snapshot을 읽지 못했습니다: {error}"))?;
    let pointer: PublishedFactPointer = serde_json::from_slice(&bytes)
        .map_err(|error| format!("게시된 Fact snapshot 형식이 올바르지 않습니다: {error}"))?;
    if pointer.schema_version != PUBLISHED_POINTER_SCHEMA_VERSION {
        return Err("게시된 Fact snapshot schema migration이 필요합니다".to_string());
    }
    pointer
        .manifest
        .validate()
        .map_err(|error| format!("게시된 Fact manifest 검증 실패: {error}"))?;
    if pointer.manifest.workspace_id.as_str() != workspace_id {
        return Err("게시된 Fact snapshot의 workspace identity가 다릅니다".to_string());
    }
    validate_leaf_file_name(&pointer.bundle_file)?;
    Ok(Some(pointer))
}

fn cached_snapshot(
    cache_key: &Path,
    pointer: &PublishedFactPointer,
) -> Result<Option<Arc<CanonicalFactSnapshot>>, String> {
    let mut cache = snapshot_cache()
        .lock()
        .map_err(|_| "Fact snapshot cache lock이 손상되었습니다".to_string())?;
    let Some(entry) = cache.entries.get(cache_key) else {
        return Ok(None);
    };
    if entry.bundle_file != pointer.bundle_file
        || entry.snapshot.manifest.bundle_digest != pointer.manifest.bundle_digest
        || entry.snapshot.manifest.snapshot_id != pointer.manifest.snapshot_id
    {
        return Ok(None);
    }
    let metadata = match fs::metadata(&entry.bundle_path) {
        Ok(metadata) => metadata,
        Err(_) => return Ok(None),
    };
    if metadata.len() != entry.file_len || metadata.modified().ok() != entry.modified {
        return Ok(None);
    }
    let snapshot = Arc::clone(&entry.snapshot);
    cache.clock = cache.clock.wrapping_add(1);
    let clock = cache.clock;
    if let Some(entry) = cache.entries.get_mut(cache_key) {
        entry.last_used = clock;
    }
    Ok(Some(snapshot))
}

fn cache_snapshot(
    cache_key: PathBuf,
    bundle_file: String,
    bundle_path: PathBuf,
    snapshot: Arc<CanonicalFactSnapshot>,
) -> Result<(), String> {
    let metadata = fs::metadata(&bundle_path)
        .map_err(|error| format!("게시된 canonical bundle metadata를 읽지 못했습니다: {error}"))?;
    let mut cache = snapshot_cache()
        .lock()
        .map_err(|_| "Fact snapshot cache lock이 손상되었습니다".to_string())?;
    cache.clock = cache.clock.wrapping_add(1);
    if !cache.entries.contains_key(&cache_key) && cache.entries.len() >= MAX_RESIDENT_SNAPSHOTS {
        if let Some(oldest) = cache
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(cache_key, _)| cache_key.clone())
        {
            cache.entries.remove(&oldest);
        }
    }
    let last_used = cache.clock;
    cache.entries.insert(
        cache_key,
        CachedSnapshot {
            snapshot,
            bundle_file,
            bundle_path,
            file_len: metadata.len(),
            modified: metadata.modified().ok(),
            last_used,
        },
    );
    Ok(())
}

pub(crate) fn open_read_only(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| format!("canonical bundle을 열지 못했습니다: {error}"))?;
    connection
        .execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF;")
        .map_err(|error| format!("canonical bundle 안전 모드를 적용하지 못했습니다: {error}"))?;
    Ok(connection)
}

fn validate_bundle(path: &Path, manifest: &FactBundleManifest) -> Result<(), String> {
    let connection = open_read_only(path)?;
    let user_version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| format!("canonical bundle schema version을 읽지 못했습니다: {error}"))?;
    if user_version != 1 {
        return Err(format!(
            "지원하지 않는 canonical SQLite schema입니다: {user_version}"
        ));
    }
    let schema: Option<String> = connection
        .query_row(
            "SELECT value FROM bundle_metadata WHERE key = 'schema'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("canonical bundle metadata를 읽지 못했습니다: {error}"))?;
    let snapshot_id: Option<String> = connection
        .query_row(
            "SELECT value FROM bundle_metadata WHERE key = 'snapshot_id'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("canonical snapshot metadata를 읽지 못했습니다: {error}"))?;
    if schema.as_deref() != Some(CANONICAL_SQLITE_SCHEMA)
        || snapshot_id.as_deref() != Some(manifest.snapshot_id.as_str())
    {
        return Err("canonical SQLite metadata가 manifest와 일치하지 않습니다".to_string());
    }

    for (table, expected) in [
        (
            "analysis_unit_receipts",
            manifest.analysis_unit_receipt_count,
        ),
        ("nodes", manifest.node_count),
        ("edges", manifest.edge_count),
        ("evidence", manifest.evidence_count),
        ("file_coverage", manifest.file_coverage_count),
        (
            "source_scope_coverage",
            manifest.source_scope_coverage_count,
        ),
        ("capability_receipts", manifest.capability_receipt_count),
        ("gaps", manifest.gap_count),
        ("issues", manifest.issue_count),
    ] {
        let actual = table_count(&connection, table)?;
        if actual != expected {
            return Err(format!(
                "canonical bundle {table} count가 manifest와 다릅니다: {actual}/{expected}"
            ));
        }
    }

    let evidence = validated_rows::<FactEvidence>(&connection, "evidence")?;
    let evidence_ids = evidence
        .iter()
        .map(|item| item.id.as_str())
        .collect::<BTreeSet<_>>();
    let nodes = validated_rows::<FactNode>(&connection, "nodes")?;
    let node_ids = nodes
        .iter()
        .map(|item| item.id.as_str())
        .collect::<BTreeSet<_>>();
    for node in &nodes {
        if node.snapshot_id != manifest.snapshot_id {
            return Err(format!("node가 다른 snapshot을 참조합니다: {}", node.id));
        }
        if node
            .parent_id
            .as_ref()
            .is_some_and(|parent| !node_ids.contains(parent.as_str()))
        {
            return Err(format!("node parent가 존재하지 않습니다: {}", node.id));
        }
        for evidence_id in node.evidence_ids.iter().chain(
            node.roles
                .iter()
                .flat_map(|assignment| assignment.evidence_ids.iter()),
        ) {
            if !evidence_ids.contains(evidence_id.as_str()) {
                return Err(format!("node evidence가 존재하지 않습니다: {evidence_id}"));
            }
        }
    }
    let edges = validated_rows::<FactEdge>(&connection, "edges")?;
    for edge in &edges {
        if edge.snapshot_id != manifest.snapshot_id {
            return Err(format!("edge가 다른 snapshot을 참조합니다: {}", edge.id));
        }
        if !node_ids.contains(edge.source_id.as_str())
            || !node_ids.contains(edge.target_id.as_str())
        {
            return Err(format!("edge endpoint가 존재하지 않습니다: {}", edge.id));
        }
        if edge
            .evidence_ids
            .iter()
            .any(|id| !evidence_ids.contains(id.as_str()))
        {
            return Err(format!("edge evidence가 존재하지 않습니다: {}", edge.id));
        }
    }

    validated_rows::<AnalysisUnitReceipt>(&connection, "analysis_unit_receipts")?;
    validated_rows::<FileCoverageRecord>(&connection, "file_coverage")?;
    validated_rows::<SourceScopeCoverageRecord>(&connection, "source_scope_coverage")?;
    validated_rows::<CapabilityReceipt>(&connection, "capability_receipts")?;
    validated_rows::<AnalysisGap>(&connection, "gaps")?;
    validated_rows::<AnalysisIssue>(&connection, "issues")?;
    Ok(())
}

fn validated_rows<T: DeserializeOwned + Validate>(
    connection: &Connection,
    table: &str,
) -> Result<Vec<T>, String> {
    validate_table_name(table)?;
    let sql = format!("SELECT payload_json FROM {table} ORDER BY 1");
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| format!("canonical {table} query를 준비하지 못했습니다: {error}"))?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| format!("canonical {table} rows를 읽지 못했습니다: {error}"))?;
    let mut result = Vec::new();
    for (index, row) in rows.enumerate() {
        let payload =
            row.map_err(|error| format!("canonical {table}[{index}]를 읽지 못했습니다: {error}"))?;
        let value: T = serde_json::from_str(&payload).map_err(|error| {
            format!("canonical {table}[{index}] payload가 올바르지 않습니다: {error}")
        })?;
        value
            .validate()
            .map_err(|error| format!("canonical {table}[{index}] 검증 실패: {error}"))?;
        result.push(value);
    }
    Ok(result)
}

fn table_count(connection: &Connection, table: &str) -> Result<u64, String> {
    validate_table_name(table)?;
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get::<_, i64>(0)
        })
        .map(|count| count.max(0) as u64)
        .map_err(|error| format!("canonical {table} count를 읽지 못했습니다: {error}"))
}

fn validate_table_name(table: &str) -> Result<(), String> {
    if matches!(
        table,
        "analysis_unit_receipts"
            | "nodes"
            | "edges"
            | "evidence"
            | "file_coverage"
            | "source_scope_coverage"
            | "capability_receipts"
            | "gaps"
            | "issues"
    ) {
        Ok(())
    } else {
        Err("허용되지 않은 canonical table입니다".to_string())
    }
}

fn publish_immutable_bundle(
    source: &Path,
    target: &Path,
    expected_digest: Sha256Digest,
) -> Result<(), String> {
    if target.is_file() {
        return (sha256_file(target)? == expected_digest)
            .then_some(())
            .ok_or_else(|| "같은 이름의 Fact snapshot payload가 손상되었습니다".to_string());
    }
    let parent = target
        .parent()
        .ok_or_else(|| "Fact snapshot 저장 경로가 올바르지 않습니다".to_string())?;
    let temporary = parent.join(format!(
        ".canonical-{}-{}.sqlite.next",
        std::process::id(),
        unix_millis()
    ));
    fs::copy(source, &temporary)
        .map_err(|error| format!("canonical bundle을 staging하지 못했습니다: {error}"))?;
    sync_file(&temporary)?;
    if sha256_file(&temporary)? != expected_digest {
        let _ = fs::remove_file(&temporary);
        return Err("staging canonical bundle SHA-256이 변경되었습니다".to_string());
    }
    if let Err(error) = fs::rename(&temporary, target) {
        let _ = fs::remove_file(&temporary);
        if target.is_file() && sha256_file(target)? == expected_digest {
            return Ok(());
        }
        return Err(format!("canonical bundle을 게시하지 못했습니다: {error}"));
    }
    Ok(())
}

fn publish_pointer(path: &Path, pointer: &PublishedFactPointer) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(pointer)
        .map_err(|error| format!("Fact snapshot pointer를 직렬화하지 못했습니다: {error}"))?;
    replace_pointer_bytes(path, &bytes)
}

fn replace_pointer_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Fact snapshot pointer 경로가 올바르지 않습니다".to_string())?;
    let temporary = parent.join(format!("{PUBLISHED_POINTER_FILE}.next"));
    let previous = parent.join(format!("{PUBLISHED_POINTER_FILE}.previous"));
    {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| format!("Fact snapshot pointer staging 실패: {error}"))?;
        file.write_all(bytes)
            .map_err(|error| format!("Fact snapshot pointer 쓰기 실패: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("Fact snapshot pointer fsync 실패: {error}"))?;
    }
    if path.is_file() {
        if previous.is_file() {
            fs::remove_file(&previous)
                .map_err(|error| format!("이전 Fact snapshot pointer 정리 실패: {error}"))?;
        }
        fs::rename(path, &previous)
            .map_err(|error| format!("Fact snapshot pointer backup 실패: {error}"))?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        if previous.is_file() {
            let _ = fs::rename(&previous, path);
        }
        return Err(format!("Fact snapshot pointer 게시 실패: {error}"));
    }
    if previous.is_file() {
        let _ = fs::remove_file(&previous);
    }
    Ok(())
}

fn canonical_file(path: &Path, label: &str) -> Result<PathBuf, String> {
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("{label} 경로를 확인하지 못했습니다: {error}"))?;
    if !canonical.is_file() {
        return Err(format!("{label}가 파일이 아닙니다"));
    }
    Ok(canonical)
}

fn validate_leaf_file_name(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.is_empty()
        || value.len() > 160
        || value.chars().any(char::is_control)
        || path.file_name().and_then(|name| name.to_str()) != Some(value)
    {
        return Err("게시된 Fact bundle 파일 이름이 올바르지 않습니다".to_string());
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<Sha256Digest, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("SHA-256 대상 파일을 열지 못했습니다: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("SHA-256 대상 파일을 읽지 못했습니다: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Sha256Digest::parse(&format!("{:x}", hasher.finalize()))
        .map_err(|error| format!("SHA-256 결과를 해석하지 못했습니다: {error}"))
}

fn sync_file(path: &Path) -> Result<(), String> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("파일을 디스크에 반영하지 못했습니다: {error}"))
}

fn empty_status() -> FactGraphStatus {
    FactGraphStatus {
        schema_version: PUBLISHED_POINTER_SCHEMA_VERSION,
        snapshot_id: None,
        source_revision: None,
        node_count: 0,
        edge_count: 0,
        evidence_count: 0,
        coverage_count: 0,
    }
}

fn status_from_manifest(manifest: &FactBundleManifest) -> FactGraphStatus {
    FactGraphStatus {
        schema_version: PUBLISHED_POINTER_SCHEMA_VERSION,
        snapshot_id: Some(manifest.snapshot_id.to_string()),
        source_revision: Some(manifest.source_manifest_digest.to_string()),
        node_count: usize::try_from(manifest.node_count).unwrap_or(usize::MAX),
        edge_count: usize::try_from(manifest.edge_count).unwrap_or(usize::MAX),
        evidence_count: usize::try_from(manifest.evidence_count).unwrap_or(usize::MAX),
        coverage_count: usize::try_from(manifest.file_coverage_count).unwrap_or(usize::MAX),
    }
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use codebase_fact_model::identity::WorkspaceId;

    #[test]
    fn no_pointer_is_an_honest_empty_status() {
        let root = test_root("empty");
        let workspace_dir = root
            .join("workspaces")
            .join("v2")
            .join("ws-0123456789abcdef");
        fs::create_dir_all(&workspace_dir).unwrap();
        let status = status_for_workspace(&root, "ws-0123456789abcdef").unwrap();
        assert_eq!(status, empty_status());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bundle_digest_tampering_is_rejected_before_publish() {
        let root = test_root("tamper");
        let workspace_id = "ws-0123456789abcdef";
        fs::create_dir_all(root.join("workspaces").join("v2").join(workspace_id)).unwrap();
        let fixture = empty_bundle_fixture(&root, workspace_id);
        OpenOptions::new()
            .append(true)
            .open(&fixture.bundle_path)
            .unwrap()
            .write_all(b"tampered")
            .unwrap();
        let error = import_and_publish(&root, workspace_id, &fixture).unwrap_err();
        assert!(error.contains("SHA-256"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_same_verified_snapshot_can_be_republished_on_windows() {
        let root = test_root("republish");
        let workspace_id = "ws-0123456789abcdef";
        fs::create_dir_all(root.join("workspaces").join("v2").join(workspace_id)).unwrap();
        let fixture = empty_bundle_fixture(&root, workspace_id);

        let first = import_and_publish(&root, workspace_id, &fixture).unwrap();
        let second = import_and_publish(&root, workspace_id, &fixture).unwrap();
        let reopened = status_for_workspace(&root, workspace_id).unwrap();

        assert_eq!(first, second);
        assert_eq!(second, reopened);
        assert!(!root
            .join("workspaces")
            .join("v2")
            .join(workspace_id)
            .join(format!("{PUBLISHED_POINTER_FILE}.previous"))
            .exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn repeated_reads_reuse_the_verified_immutable_snapshot() {
        let root = test_root("snapshot-cache");
        let workspace_id = "ws-0123456789abcdef";
        fs::create_dir_all(root.join("workspaces").join("v2").join(workspace_id)).unwrap();
        let fixture = empty_bundle_fixture(&root, workspace_id);
        import_and_publish(&root, workspace_id, &fixture).unwrap();

        let first = load_published_snapshot(&root, workspace_id)
            .unwrap()
            .unwrap();
        let second = load_published_snapshot(&root, workspace_id)
            .unwrap()
            .unwrap();

        assert!(Arc::ptr_eq(&first, &second));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_semantic_refresh_restores_the_previous_fact_pointer() {
        let root = test_root("rollback");
        let workspace_id = "ws-0123456789abcdef";
        let workspace_dir = root.join("workspaces").join("v2").join(workspace_id);
        fs::create_dir_all(&workspace_dir).unwrap();
        let fixture = empty_bundle_fixture(&root, workspace_id);
        let expected = import_and_publish(&root, workspace_id, &fixture).unwrap();
        let checkpoint = checkpoint_published_pointer(&root, workspace_id)
            .unwrap()
            .unwrap();

        fs::remove_file(workspace_dir.join(PUBLISHED_POINTER_FILE)).unwrap();
        restore_published_pointer(&root, workspace_id, Some(&checkpoint)).unwrap();
        assert_eq!(status_for_workspace(&root, workspace_id).unwrap(), expected);

        restore_published_pointer(&root, workspace_id, None).unwrap();
        assert_eq!(
            status_for_workspace(&root, workspace_id).unwrap(),
            empty_status()
        );
        assert!(workspace_dir.join("fact-snapshots").is_dir());
        fs::remove_dir_all(root).unwrap();
    }

    fn empty_bundle_fixture(root: &Path, workspace_id: &str) -> CanonicalFactBundleArtifact {
        let bundle_path = root.join("fixture.sqlite");
        let connection = Connection::open(&bundle_path).unwrap();
        connection
            .execute_batch(
                "PRAGMA user_version=1;
                 CREATE TABLE bundle_metadata(key TEXT PRIMARY KEY, value TEXT NOT NULL) STRICT;
                 CREATE TABLE analysis_unit_receipts(payload_json TEXT NOT NULL) STRICT;
                 CREATE TABLE evidence(payload_json TEXT NOT NULL) STRICT;
                 CREATE TABLE nodes(payload_json TEXT NOT NULL) STRICT;
                 CREATE TABLE edges(payload_json TEXT NOT NULL) STRICT;
                 CREATE TABLE file_coverage(payload_json TEXT NOT NULL) STRICT;
                 CREATE TABLE source_scope_coverage(payload_json TEXT NOT NULL) STRICT;
                 CREATE TABLE capability_receipts(payload_json TEXT NOT NULL) STRICT;
                 CREATE TABLE gaps(payload_json TEXT NOT NULL) STRICT;
                 CREATE TABLE issues(payload_json TEXT NOT NULL) STRICT;",
            )
            .unwrap();
        let workspace = WorkspaceId::parse(workspace_id).unwrap();
        let source = Sha256Digest::of_bytes(b"source");
        let plan = Sha256Digest::of_bytes(b"plan");
        let providers = Sha256Digest::of_bytes(b"providers");
        let contexts = Sha256Digest::of_bytes(b"contexts");
        let snapshot =
            SnapshotId::from_execution_inputs(&workspace, source, plan, providers, contexts)
                .unwrap();
        connection
            .execute(
                "INSERT INTO bundle_metadata(key,value) VALUES('schema',?1)",
                [CANONICAL_SQLITE_SCHEMA],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO bundle_metadata(key,value) VALUES('snapshot_id',?1)",
                [snapshot.as_str()],
            )
            .unwrap();
        drop(connection);
        let bundle_digest = sha256_file(&bundle_path).unwrap();
        let manifest = FactBundleManifest {
            schema: ContractSchema::CanonicalFactV1,
            snapshot_id: snapshot.clone(),
            workspace_id: workspace,
            source_manifest_digest: source,
            config_digest: Sha256Digest::of_bytes(b"config"),
            analysis_plan_digest: plan,
            provider_set_digest: providers,
            execution_context_set_digest: contexts,
            semantic_digest: Sha256Digest::of_bytes(b"semantic"),
            bundle_digest,
            analysis_unit_receipt_count: 0,
            node_count: 0,
            edge_count: 0,
            evidence_count: 0,
            file_coverage_count: 0,
            source_scope_coverage_count: 0,
            capability_receipt_count: 0,
            gap_count: 0,
            issue_count: 0,
            completed_at_unix_ms: 1,
        };
        let manifest_path = root.join("fixture.manifest.json");
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        CanonicalFactBundleArtifact {
            schema: CANONICAL_ARTIFACT_SCHEMA.to_string(),
            snapshot_id: snapshot,
            semantic_digest: manifest.semantic_digest,
            bundle_digest,
            bundle_path,
            manifest_path,
        }
    }

    fn test_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "codebase-workspace-canonical-store-{label}-{}-{}",
            std::process::id(),
            unix_millis()
        ))
    }
}
