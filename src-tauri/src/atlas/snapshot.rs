use crate::{
    engine::{self, EngineRegistry},
    paths::base_paths,
    workspace::{
        validate_workspace_id, CodeCall, CodeInventory, CodeInventoryItem, DbConstraint,
        DbDependentObject, DbForeignKey, DbIndex, DbInventory, DbProfile, DbSource, Workspace,
    },
};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{btree_map::Entry, BTreeMap, BTreeSet, HashMap, HashSet},
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use super::model::{
    Evidence, InventoryItem, InventorySnapshot, SnapshotGap, SnapshotLink, SnapshotMetadata,
    SnapshotMigration, SnapshotSourceMetadata, SourceLocation, SNAPSHOT_SCHEMA_VERSION,
};

const REINDEX_REASON: &str = "스냅샷 형식이 호환되지 않아 다시 읽어야 합니다";
const V1_MIGRATION_NOTE: &str = "Snapshot V1의 안전한 필드를 V2로 이전했습니다.";
const V1_CODE_REINDEX_NOTE: &str =
    "Snapshot V1 코드 항목은 이전 BM25 bucket 분류를 신뢰할 수 없어 다시 읽어야 합니다.";
const UNSCORED_CODE_CALL_REINDEX_NOTE: &str =
    "기존 CALLS에 엔진 신뢰도 정보가 없어 코드를 다시 읽어야 합니다.";
const BACKUP_REINDEX_NOTE: &str =
    "주 스냅샷 대신 이전 백업을 복구했습니다. 다시 읽어 최신 상태를 확인하세요.";
const BACKUP_CODE_REINDEX_NOTE: &str = "백업에서 복구한 코드 목록은 다시 읽어야 합니다.";
const BACKUP_DB_REINDEX_NOTE: &str = "백업에서 복구한 DB 구조는 다시 읽어야 합니다.";
const CONFIRMED_CODE_CALL_CONFIDENCE: u8 = 85;
const CANDIDATE_CODE_CALL_CONFIDENCE: u8 = 70;
static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(0);
static SNAPSHOT_CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedSnapshot>>> = OnceLock::new();
static FRESHNESS_CACHE: OnceLock<Mutex<HashMap<String, CachedFreshness>>> = OnceLock::new();
// Keep at most two idle inventories. Entries held by active commands may temporarily exceed
// the limit so concurrent workspace reads do not evict data still in use.
const SNAPSHOT_CACHE_LIMIT: usize = 2;
const FRESHNESS_CACHE_TTL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
struct SnapshotFileState {
    modified: Option<SystemTime>,
    length: Option<u64>,
}

#[derive(Debug, Clone)]
struct CachedSnapshot {
    primary: SnapshotFileState,
    backup: SnapshotFileState,
    snapshot: Arc<InventorySnapshot>,
}

#[derive(Debug, Clone)]
struct CachedFreshness {
    snapshot_saved_at: String,
    snapshot_schema_version: u32,
    snapshot_stale_reasons: Vec<String>,
    workspace: Workspace,
    registry: EngineRegistry,
    checked_at: Instant,
    reasons: Vec<String>,
}

pub(crate) fn build_inventory_snapshot(
    workspace_id: String,
    code: Option<&CodeInventory>,
    db: Option<&DbInventory>,
) -> InventorySnapshot {
    let mut snapshot = InventorySnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        workspace_id,
        saved_at: timestamp(),
        metadata: SnapshotMetadata::default(),
        stale_reasons: Vec::new(),
        links: Vec::new(),
        items: Vec::new(),
    };

    if let Some(code) = code {
        snapshot.metadata.code = Some(SnapshotSourceMetadata {
            saved_at: snapshot.saved_at.clone(),
            engine_id: Some("codebase-memory".to_string()),
            engine_version: None,
            engine_checksum: None,
            contract_version: None,
            snapshot_key: None,
            limit_requested: None,
            limit_applied: None,
            limit_clamped: None,
            result_count: None,
            total_tables: None,
            truncated: None,
            source_revision: None,
            source_revision_label: None,
            source_path: None,
            source_type: "unknown".to_string(),
            profile_id: None,
        });
        snapshot.metadata.architecture = code.architecture.clone();
        snapshot.items.extend(
            code.routes
                .iter()
                .map(|entry| code_item(entry, "api", "api", &code.project)),
        );
        snapshot.items.extend(
            code.services
                .iter()
                .map(|entry| code_item(entry, "service", "code", &code.project)),
        );
        snapshot.items.extend(
            code.handlers
                .iter()
                .map(|entry| code_item(entry, "handler", "code", &code.project)),
        );
        snapshot.items.extend(
            code.repositories
                .iter()
                .map(|entry| code_item(entry, "repository", "code", &code.project)),
        );
        snapshot.items.extend(code.functions.iter().map(|entry| {
            let kind = match entry.engine_label.as_str() {
                "Method" => "method",
                _ => "function",
            };
            code_item(entry, kind, "code", &code.project)
        }));
        snapshot.items.extend(
            code.classes
                .iter()
                .map(|entry| code_item(entry, "class", "code", &code.project)),
        );
        snapshot.items.extend(
            code.modules
                .iter()
                .map(|entry| code_item(entry, "module", "code", &code.project)),
        );
        snapshot.items.extend(code.unknown.iter().map(|entry| {
            let engine_kind = entry.engine_label.to_ascii_lowercase();
            let kind = if entry.kind.eq_ignore_ascii_case("unknown") || engine_kind.is_empty() {
                "code"
            } else {
                &engine_kind
            };
            code_item(entry, kind, "code", &code.project)
        }));
        snapshot.items.extend(
            code.files
                .iter()
                .map(|entry| code_item(entry, "file", "code", &code.project)),
        );
        snapshot.links.extend(code.calls.iter().map(code_call_link));
        snapshot.links.extend(code.handles.iter().map(|handle| {
            let mut link = confirmed_link(
                format!("code-handle:{}->{}", handle.route, handle.handler),
                format!("code:{}", handle.route),
                format!("code:{}", handle.handler),
                "code_handle",
                "HANDLES",
                "codebase-memory HANDLES: upstream handler→route was normalized to product route→handler",
            );
            link.direction = "outbound".to_string();
            link
        }));
    }

    if let Some(db) = db {
        snapshot.metadata.db = Some(SnapshotSourceMetadata {
            saved_at: snapshot.saved_at.clone(),
            engine_id: Some("database-memory".to_string()),
            engine_version: None,
            engine_checksum: None,
            contract_version: db.contract_version.clone(),
            snapshot_key: db.snapshot_key.clone(),
            limit_requested: db.limit_requested,
            limit_applied: db.limit_applied,
            limit_clamped: db.limit_clamped,
            result_count: db.result_count,
            total_tables: db.total_tables,
            truncated: db.truncated,
            source_revision: None,
            source_revision_label: None,
            source_path: None,
            source_type: "unknown".to_string(),
            profile_id: Some(db.profile_id.clone()),
        });
        for (index, warning) in db.capability_warnings.iter().enumerate() {
            let message = localized_db_capability_warning(warning);
            snapshot.metadata.gaps.push(gap(
                format!("gap:db-capability:{index}"),
                "db-capability",
                &message,
                Vec::new(),
            ));
        }
        snapshot.metadata.gaps.extend(db.gaps.iter().map(|entry| {
            gap(
                format!("gap:{}", entry.id),
                &entry.kind,
                &entry.message,
                entry
                    .table_key
                    .as_deref()
                    .map(|table_key| vec![format!("db:table:{table_key}")])
                    .unwrap_or_default(),
            )
        }));
        let table_keys = db
            .tables
            .iter()
            .map(|table| db_table_key(table.schema.as_deref(), &table.name))
            .collect::<BTreeSet<_>>();
        let stable_table_keys = db
            .tables
            .iter()
            .filter_map(|table| {
                table.key.as_ref().map(|key| {
                    (
                        key.clone(),
                        db_table_key(table.schema.as_deref(), &table.name),
                    )
                })
            })
            .collect::<BTreeMap<_, _>>();
        let mut named_table_keys = BTreeMap::<String, Vec<String>>::new();
        for table in &db.tables {
            named_table_keys
                .entry(table.name.clone())
                .or_default()
                .push(db_table_key(table.schema.as_deref(), &table.name));
        }
        let column_ids = db
            .tables
            .iter()
            .flat_map(|table| {
                let table_key = db_table_key(table.schema.as_deref(), &table.name);
                table
                    .columns
                    .iter()
                    .map(move |column| db_column_id(&table_key, &column.name))
            })
            .collect::<BTreeSet<_>>();
        let stable_column_ids = db
            .tables
            .iter()
            .flat_map(|table| {
                let table_key = db_table_key(table.schema.as_deref(), &table.name);
                table.columns.iter().filter_map(move |column| {
                    column
                        .key
                        .as_ref()
                        .map(|key| (key.clone(), db_column_id(&table_key, &column.name)))
                })
            })
            .collect::<BTreeMap<_, _>>();

        for table in &db.tables {
            let table_key = db_table_key(table.schema.as_deref(), &table.name);
            let table_id = format!("db:table:{table_key}");
            let mut table_item = item(
                &table_id,
                "table",
                &table.name,
                "data",
                "db",
                None,
                table.schema.as_deref(),
            );
            let display_table_name = db_qualified_table_name(table.schema.as_deref(), &table.name);
            table_item.qualified_name = table
                .key
                .clone()
                .or_else(|| Some(display_table_name.clone()));
            table_item.engine_label = Some("Table".to_string());
            table_item.project_id = Some(db.profile_id.clone());
            table_item.group_id = table.schema.clone();
            snapshot.items.push(table_item);

            snapshot.items.extend(table.columns.iter().map(|column| {
                let mut column_item = InventoryItem {
                    id: db_column_id(&table_key, &column.name),
                    kind: "column".to_string(),
                    name: column.name.clone(),
                    layer: "data".to_string(),
                    source: "db".to_string(),
                    parent_id: Some(table_id.clone()),
                    path: column.data_type.clone(),
                    qualified_name: column
                        .key
                        .clone()
                        .or_else(|| Some(format!("{display_table_name}.{}", column.name))),
                    engine_label: Some("Column".to_string()),
                    project_id: Some(db.profile_id.clone()),
                    group_id: table.schema.clone(),
                    location: None,
                    is_primary_key: column.is_primary_key,
                    is_foreign_key: column.is_foreign_key,
                    nullable: column.nullable,
                };
                if column_item.qualified_name.as_deref() == Some("") {
                    column_item.qualified_name = None;
                }
                column_item
            }));

            for constraint in &table.constraints {
                append_db_constraint(
                    &mut snapshot,
                    &table_key,
                    &db.profile_id,
                    constraint,
                    &column_ids,
                    &stable_column_ids,
                );
            }
            for index in &table.indexes {
                append_db_index(
                    &mut snapshot,
                    &table_key,
                    &db.profile_id,
                    index,
                    &column_ids,
                    &stable_column_ids,
                );
            }
            for dependent in &table.dependents {
                append_db_dependent(
                    &mut snapshot,
                    &table_key,
                    table.schema.as_deref(),
                    &db.profile_id,
                    dependent,
                    &stable_column_ids,
                );
            }
        }

        let mut foreign_key_observations = db
            .tables
            .iter()
            .flat_map(|table| {
                table
                    .foreign_keys
                    .iter()
                    .map(move |foreign_key| (table, foreign_key, "outbound"))
                    .chain(
                        table
                            .inbound_foreign_keys
                            .iter()
                            .map(move |foreign_key| (table, foreign_key, "inbound")),
                    )
            })
            .collect::<Vec<_>>();
        foreign_key_observations.sort_by(|left, right| {
            (left.2 != "outbound")
                .cmp(&(right.2 != "outbound"))
                .then_with(|| left.1.key.cmp(&right.1.key))
                .then_with(|| left.1.name.cmp(&right.1.name))
        });
        for (table, foreign_key, direction) in foreign_key_observations {
            let current_table_key = db_table_key(table.schema.as_deref(), &table.name);
            let source_key = resolve_db_table_key(
                foreign_key.table_key.as_deref(),
                foreign_key.table_schema.as_deref(),
                foreign_key.table.as_deref(),
                &table_keys,
                &stable_table_keys,
                &named_table_keys,
            )
            .or_else(|| (direction == "outbound").then(|| current_table_key.clone()));
            let referenced_key = resolve_db_table_key(
                foreign_key.referenced_table_key.as_deref(),
                foreign_key.referenced_schema.as_deref(),
                Some(&foreign_key.referenced_table),
                &table_keys,
                &stable_table_keys,
                &named_table_keys,
            );
            let (Some(source_key), Some(referenced_key)) = (source_key, referenced_key) else {
                snapshot.metadata.gaps.push(gap(
                        format!(
                            "gap:db-fk-table:{}:{}",
                            current_table_key,
                            foreign_key.name.as_deref().unwrap_or("unnamed")
                        ),
                        "db-fk-unresolved-table",
                        "FK의 source 또는 target table을 유일하게 확인할 수 없어 관계를 만들지 않았습니다.",
                        vec![format!("db:table:{current_table_key}")],
                    ));
                continue;
            };

            let constraint = constraint_from_foreign_key(foreign_key, direction);
            append_db_constraint(
                &mut snapshot,
                &source_key,
                &db.profile_id,
                &constraint,
                &column_ids,
                &stable_column_ids,
            );
            let source_column_count = foreign_key.columns.len().max(foreign_key.column_keys.len());
            let referenced_column_count = foreign_key
                .referenced_columns
                .len()
                .max(foreign_key.referenced_column_keys.len());
            if source_column_count != referenced_column_count {
                snapshot.metadata.gaps.push(gap(
                    format!(
                        "gap:db-fk-columns:{}:{}",
                        source_key,
                        foreign_key.name.as_deref().unwrap_or("unnamed")
                    ),
                    "db-fk-column-mismatch",
                    "FK source/target column 수가 달라 확인 가능한 열 관계만 보존했습니다.",
                    vec![
                        format!("db:table:{source_key}"),
                        format!("db:table:{referenced_key}"),
                    ],
                ));
            }
            for ordinal in 0..source_column_count.min(referenced_column_count) {
                let source_column = foreign_key.columns.get(ordinal).map(String::as_str);
                let source_stable_key = foreign_key.column_keys.get(ordinal).map(String::as_str);
                let referenced_column = foreign_key
                    .referenced_columns
                    .get(ordinal)
                    .map(String::as_str);
                let referenced_stable_key = foreign_key
                    .referenced_column_keys
                    .get(ordinal)
                    .map(String::as_str);
                let from = resolve_db_column_id(
                    &source_key,
                    source_column,
                    source_stable_key,
                    &column_ids,
                    &stable_column_ids,
                );
                let to = resolve_db_column_id(
                    &referenced_key,
                    referenced_column,
                    referenced_stable_key,
                    &column_ids,
                    &stable_column_ids,
                );
                let (Some(from), Some(to)) = (from, to) else {
                    snapshot.metadata.gaps.push(gap(
                        format!(
                            "gap:db-fk-endpoint:{}:{}:{ordinal}",
                            source_key,
                            foreign_key.name.as_deref().unwrap_or("unnamed")
                        ),
                        "db-fk-missing-column",
                        "FK column endpoint가 inventory에 없어 관계를 만들지 않았습니다.",
                        vec![
                            format!("db:table:{source_key}"),
                            format!("db:table:{referenced_key}"),
                        ],
                    ));
                    continue;
                };
                let mut link = confirmed_link(
                    format!("db-fk:{from}->{to}"),
                    from,
                    to,
                    "db_fk",
                    "FOREIGN_KEY",
                    "database-memory foreign key metadata",
                );
                link.label = foreign_key.name.clone();
                if let Some(key) = foreign_key.key.as_deref() {
                    link.evidence.push(Evidence {
                        kind: "db-object-key".to_string(),
                        text: key.to_string(),
                    });
                }
                push_evidence(&mut link.evidence, "db-column-key", source_stable_key);
                push_evidence(
                    &mut link.evidence,
                    "db-referenced-column-key",
                    referenced_stable_key,
                );
                link.evidence.push(Evidence {
                    kind: "db-fk-direction".to_string(),
                    text: direction.to_string(),
                });
                snapshot.links.push(link);
            }
        }
    }

    canonicalize_snapshot(snapshot)
}

#[cfg(test)]
pub(crate) fn normalize_inventory(
    workspace_id: String,
    code: Option<&CodeInventory>,
    db: Option<&DbInventory>,
) -> InventorySnapshot {
    build_inventory_snapshot(workspace_id, code, db)
}

pub(crate) fn snapshot_with_metadata(
    mut snapshot: InventorySnapshot,
    workspace: &Workspace,
    registry: &EngineRegistry,
) -> InventorySnapshot {
    let saved_at = timestamp();
    let has_code = snapshot.metadata.code.is_some()
        || snapshot.items.iter().any(|entry| entry.source == "code");
    let has_db =
        snapshot.metadata.db.is_some() || snapshot.items.iter().any(|entry| entry.source == "db");
    let code_revision = has_code.then(|| code_source_revision(workspace)).flatten();
    let profile = workspace.active_db_profile_id.as_deref().and_then(|id| {
        workspace
            .db_profiles
            .iter()
            .find(|profile| profile.id == id)
    });
    let db_revision = profile.and_then(db_source_revision);
    snapshot.saved_at = saved_at.clone();
    snapshot.metadata.code = has_code.then(|| SnapshotSourceMetadata {
        saved_at: saved_at.clone(),
        engine_id: Some("codebase-memory".to_string()),
        engine_version: engine_version(registry, "codebase-memory"),
        engine_checksum: engine_checksum(registry, "codebase-memory"),
        contract_version: engine_contract_version(registry, "codebase-memory"),
        snapshot_key: None,
        limit_requested: None,
        limit_applied: None,
        limit_clamped: None,
        result_count: None,
        total_tables: None,
        truncated: None,
        source_revision: code_revision.as_ref().map(|(revision, _)| revision.clone()),
        source_revision_label: code_revision.map(|(_, label)| label),
        source_path: Some(workspace.repo_path.clone()),
        source_type: code_source_type(workspace),
        profile_id: None,
    });
    let previous_db_metadata = snapshot.metadata.db.clone();
    let db_contract_version = previous_db_metadata
        .as_ref()
        .and_then(|metadata| metadata.contract_version.clone());
    let db_snapshot_key = previous_db_metadata
        .as_ref()
        .and_then(|metadata| metadata.snapshot_key.clone());
    snapshot.metadata.db = has_db.then(|| SnapshotSourceMetadata {
        saved_at: saved_at.clone(),
        engine_id: Some("database-memory".to_string()),
        engine_version: engine_version(registry, "database-memory"),
        engine_checksum: engine_checksum(registry, "database-memory"),
        contract_version: db_contract_version
            .or_else(|| engine_contract_version(registry, "database-memory")),
        snapshot_key: db_snapshot_key,
        limit_requested: previous_db_metadata
            .as_ref()
            .and_then(|metadata| metadata.limit_requested),
        limit_applied: previous_db_metadata
            .as_ref()
            .and_then(|metadata| metadata.limit_applied),
        limit_clamped: previous_db_metadata
            .as_ref()
            .and_then(|metadata| metadata.limit_clamped),
        result_count: previous_db_metadata
            .as_ref()
            .and_then(|metadata| metadata.result_count),
        total_tables: previous_db_metadata
            .as_ref()
            .and_then(|metadata| metadata.total_tables),
        truncated: previous_db_metadata
            .as_ref()
            .and_then(|metadata| metadata.truncated),
        source_revision: db_revision.as_ref().map(|(revision, _)| revision.clone()),
        source_revision_label: db_revision
            .map(|(_, label)| label)
            .or_else(|| profile.map(|_| "외부 DB · 마지막 읽기 기준".to_string())),
        source_path: profile.and_then(|profile| profile.path.clone()),
        source_type: profile
            .map(|profile| db_source_key(&profile.source))
            .unwrap_or_else(|| "unknown".to_string()),
        profile_id: profile.map(|profile| profile.id.clone()),
    });
    snapshot.stale_reasons.clear();
    canonicalize_snapshot(snapshot)
}

#[cfg(test)]
pub(crate) fn mark_snapshot_staleness(
    mut snapshot: InventorySnapshot,
    workspace: &Workspace,
    registry: &EngineRegistry,
) -> InventorySnapshot {
    snapshot.stale_reasons = snapshot_staleness_reasons(&snapshot, workspace, registry);
    snapshot
}

pub(crate) fn snapshot_staleness_reasons_cached(
    snapshot: &InventorySnapshot,
    workspace: &Workspace,
    registry: &EngineRegistry,
) -> Vec<String> {
    let cache = FRESHNESS_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(cache) = cache.lock() {
        if let Some(entry) = cache.get(&workspace.id) {
            if entry.checked_at.elapsed() < FRESHNESS_CACHE_TTL
                && entry.snapshot_saved_at == snapshot.saved_at
                && entry.snapshot_schema_version == snapshot.schema_version
                && entry.snapshot_stale_reasons == snapshot.stale_reasons
                && entry.workspace == *workspace
                && entry.registry == *registry
            {
                return entry.reasons.clone();
            }
        }
    }

    let reasons = snapshot_staleness_reasons(snapshot, workspace, registry);
    if let Ok(mut cache) = cache.lock() {
        cache.insert(
            workspace.id.clone(),
            CachedFreshness {
                snapshot_saved_at: snapshot.saved_at.clone(),
                snapshot_schema_version: snapshot.schema_version,
                snapshot_stale_reasons: snapshot.stale_reasons.clone(),
                workspace: workspace.clone(),
                registry: registry.clone(),
                checked_at: Instant::now(),
                reasons: reasons.clone(),
            },
        );
    }
    reasons
}

pub(crate) fn invalidate_snapshot_freshness(workspace_id: &str) {
    if let Some(cache) = FRESHNESS_CACHE.get() {
        if let Ok(mut cache) = cache.lock() {
            cache.remove(workspace_id);
        }
    }
}

pub(crate) fn snapshot_staleness_reasons(
    snapshot: &InventorySnapshot,
    workspace: &Workspace,
    registry: &EngineRegistry,
) -> Vec<String> {
    let mut reasons = snapshot.stale_reasons.clone();
    let has_code = snapshot.metadata.code.is_some()
        || snapshot.items.iter().any(|entry| entry.source == "code");
    let has_db =
        snapshot.metadata.db.is_some() || snapshot.items.iter().any(|entry| entry.source == "db");

    if has_code {
        match &snapshot.metadata.code {
            Some(code) => {
                let same_path = code.source_path.as_deref() == Some(workspace.repo_path.as_str());
                if !same_path {
                    push_unique(&mut reasons, "코드 프로젝트 경로가 바뀌었습니다");
                } else {
                    let current_revision = code_source_revision(workspace);
                    mark_source_revision_staleness(
                        code.source_revision.as_deref(),
                        current_revision.as_ref(),
                        Path::new(&workspace.repo_path).is_dir(),
                        "코드 소스 지문이 없어 다시 읽어야 합니다",
                        "코드 파일이 마지막 읽기 이후 바뀌었습니다",
                        "코드 변경 상태를 확인할 수 없습니다",
                        &mut reasons,
                    );
                }
                mark_engine_staleness(code, registry, "codebase-memory", "코드", &mut reasons);
            }
            None => push_unique(&mut reasons, "읽은 코드 구조가 없습니다"),
        }
    }

    if has_db {
        match &snapshot.metadata.db {
            Some(db) => {
                let profile = db.profile_id.as_deref().and_then(|id| {
                    workspace
                        .db_profiles
                        .iter()
                        .find(|profile| profile.id == id)
                });
                match profile {
                    Some(profile) => {
                        if workspace.active_db_profile_id.as_deref() != Some(profile.id.as_str()) {
                            push_unique(&mut reasons, "활성 DB 연결이 바뀌었습니다");
                        }
                        if db.source_type != db_source_key(&profile.source) {
                            push_unique(&mut reasons, "DB 연결 유형이 바뀌었습니다");
                        }
                        if db.source_path.as_deref() != profile.path.as_deref() {
                            push_unique(&mut reasons, "DB 연결 경로가 바뀌었습니다");
                        } else if profile.path.is_some() {
                            let current_revision = db_source_revision(profile);
                            let source_exists = profile
                                .path
                                .as_deref()
                                .is_some_and(|path| Path::new(path).exists());
                            mark_source_revision_staleness(
                                db.source_revision.as_deref(),
                                current_revision.as_ref(),
                                source_exists,
                                "DB 소스 지문이 없어 다시 읽어야 합니다",
                                "DB 파일이 마지막 읽기 이후 바뀌었습니다",
                                "DB 파일 변경 상태를 확인할 수 없습니다",
                                &mut reasons,
                            );
                        }
                    }
                    None => push_unique(&mut reasons, "DB 연결을 찾을 수 없습니다"),
                }
                mark_engine_staleness(db, registry, "database-memory", "DB", &mut reasons);
            }
            None => push_unique(&mut reasons, "읽은 DB 구조가 없습니다"),
        }
    }

    if snapshot.metadata.migration.reindex_required {
        push_unique(&mut reasons, REINDEX_REASON);
    }
    reasons
}

pub(crate) fn save_inventory_snapshot(
    app_data_dir: impl AsRef<Path>,
    snapshot: &InventorySnapshot,
) -> Result<(), String> {
    validate_workspace_id(&snapshot.workspace_id)?;
    let path = snapshot_path(app_data_dir, &snapshot.workspace_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let snapshot = canonicalize_snapshot(snapshot.clone());
    let json = serde_json::to_string_pretty(&snapshot).map_err(|error| error.to_string())?;
    atomic_save(
        &path,
        engine::redact_secrets(&json).as_bytes(),
        &snapshot.workspace_id,
    )?;
    invalidate_cached_snapshot(&path);
    invalidate_snapshot_freshness(&snapshot.workspace_id);
    super::linker::invalidate_candidate_links(&snapshot.workspace_id);
    Ok(())
}

pub(crate) fn replace_inventory_source(
    existing: Option<InventorySnapshot>,
    incoming: InventorySnapshot,
    source: &str,
) -> Result<InventorySnapshot, String> {
    if !matches!(source, "code" | "db") {
        return Err(format!("지원하지 않는 inventory 소스입니다: {source}"));
    }
    let Some(mut merged) = existing else {
        return Ok(canonicalize_snapshot(incoming));
    };
    if merged.workspace_id != incoming.workspace_id {
        return Err("합칠 inventory의 프로젝트 ID가 일치하지 않습니다".to_string());
    }

    let incoming_migration = incoming.metadata.migration.clone();
    let prefix = format!("{source}:");
    let removed_ids = merged
        .items
        .iter()
        .filter(|item| item.source == source)
        .map(|item| item.id.clone())
        .collect::<HashSet<_>>();
    let removed_link_ids = merged
        .links
        .iter()
        .filter(|link| removed_ids.contains(&link.from) || removed_ids.contains(&link.to))
        .map(|link| link.id.clone())
        .collect::<HashSet<_>>();
    merged.items.retain(|item| item.source != source);
    merged
        .links
        .retain(|link| !removed_ids.contains(&link.from) && !removed_ids.contains(&link.to));
    merged.metadata.gaps.retain(|gap| {
        !gap.id.starts_with(&format!("gap:{source}"))
            && !gap.kind.starts_with(source)
            && !gap.related_ids.iter().any(|id| {
                id.starts_with(&prefix) || removed_ids.contains(id) || removed_link_ids.contains(id)
            })
    });
    clear_resolved_migration(&mut merged, source);

    merged.items.extend(incoming.items);
    merged.links.extend(incoming.links);
    merged.metadata.gaps.extend(incoming.metadata.gaps);
    merge_migration(&mut merged.metadata.migration, incoming_migration);
    merged.saved_at = incoming.saved_at;
    merged.stale_reasons.clear();
    if source == "code" {
        merged.metadata.code = incoming.metadata.code;
        merged.metadata.architecture = incoming.metadata.architecture;
    } else {
        merged.metadata.db = incoming.metadata.db;
    }
    Ok(canonicalize_snapshot(merged))
}

fn clear_resolved_migration(snapshot: &mut InventorySnapshot, source: &str) {
    if snapshot
        .metadata
        .migration
        .notes
        .iter()
        .any(|note| note == BACKUP_REINDEX_NOTE)
    {
        snapshot
            .metadata
            .migration
            .notes
            .retain(|note| note != BACKUP_REINDEX_NOTE);
        if snapshot.items.iter().any(|item| item.source == "code") {
            push_unique(
                &mut snapshot.metadata.migration.notes,
                BACKUP_CODE_REINDEX_NOTE,
            );
        }
        if snapshot.items.iter().any(|item| item.source == "db") {
            push_unique(
                &mut snapshot.metadata.migration.notes,
                BACKUP_DB_REINDEX_NOTE,
            );
        }
    }

    if snapshot.items.is_empty() {
        snapshot.metadata.migration = SnapshotMigration::default();
        return;
    }

    match source {
        "code" => snapshot.metadata.migration.notes.retain(|note| {
            note != V1_CODE_REINDEX_NOTE
                && note != UNSCORED_CODE_CALL_REINDEX_NOTE
                && note != BACKUP_CODE_REINDEX_NOTE
        }),
        "db" => snapshot
            .metadata
            .migration
            .notes
            .retain(|note| note != BACKUP_DB_REINDEX_NOTE),
        _ => {}
    }

    snapshot.metadata.migration.reindex_required = migration_has_blocker(snapshot);
}

fn migration_has_blocker(snapshot: &InventorySnapshot) -> bool {
    snapshot
        .metadata
        .migration
        .notes
        .iter()
        .any(|note| note != V1_MIGRATION_NOTE)
        || snapshot.metadata.gaps.iter().any(|gap| {
            matches!(
                gap.kind.as_str(),
                "node-conflict" | "relationship-conflict" | "unscored-code-call"
            )
        })
}

fn merge_migration(target: &mut SnapshotMigration, incoming: SnapshotMigration) {
    target.reindex_required |= incoming.reindex_required;
    target.source_schema_version = incoming
        .source_schema_version
        .or(target.source_schema_version);
    for note in incoming.notes {
        push_unique(&mut target.notes, &note);
    }
}

#[cfg(test)]
pub(crate) fn load_inventory_snapshot(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
) -> Result<InventorySnapshot, String> {
    Ok((*load_inventory_snapshot_cached(app_data_dir, workspace_id)?).clone())
}

pub(crate) fn load_inventory_snapshot_optional(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
) -> Result<Option<InventorySnapshot>, String> {
    Ok(
        load_inventory_snapshot_optional_cached(app_data_dir, workspace_id)?
            .map(|snapshot| (*snapshot).clone()),
    )
}

pub(crate) fn load_inventory_snapshot_optional_cached(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
) -> Result<Option<Arc<InventorySnapshot>>, String> {
    validate_workspace_id(workspace_id)?;
    let path = snapshot_path(app_data_dir.as_ref(), workspace_id);
    if !path.is_file() && !snapshot_backup_path(&path).is_file() {
        return Ok(None);
    }
    load_inventory_snapshot_cached(app_data_dir, workspace_id).map(Some)
}

pub(crate) fn remove_db_inventory_snapshot(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
) -> Result<(), String> {
    let app_data_dir = app_data_dir.as_ref();
    let Some(mut snapshot) = load_inventory_snapshot_optional(app_data_dir, workspace_id)? else {
        return Ok(());
    };

    snapshot.items.retain(|item| item.source != "db");
    let retained_ids = snapshot
        .items
        .iter()
        .map(|item| item.id.as_str())
        .collect::<BTreeSet<_>>();
    snapshot.links.retain(|link| {
        retained_ids.contains(link.from.as_str()) && retained_ids.contains(link.to.as_str())
    });
    snapshot.metadata.db = None;
    snapshot.metadata.gaps.retain(|gap| {
        !gap.id.starts_with("gap:db")
            && !gap.kind.starts_with("db-")
            && gap
                .related_ids
                .iter()
                .all(|id| retained_ids.contains(id.as_str()))
    });
    snapshot.stale_reasons.clear();

    let path = snapshot_path(app_data_dir, workspace_id);
    if snapshot.items.is_empty() {
        remove_file_if_exists(&path)?;
        remove_file_if_exists(&snapshot_backup_path(&path))?;
        invalidate_cached_snapshot(&path);
        invalidate_snapshot_freshness(workspace_id);
        return Ok(());
    }

    save_inventory_snapshot(app_data_dir, &snapshot)?;
    fs::copy(&path, snapshot_backup_path(&path))
        .map_err(|error| format!("DB 구조 백업을 정리하지 못했습니다: {error}"))?;
    Ok(())
}

fn remove_file_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn load_inventory_snapshot_cached(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
) -> Result<Arc<InventorySnapshot>, String> {
    validate_workspace_id(workspace_id)?;
    let path = snapshot_path(app_data_dir, workspace_id);
    let primary = snapshot_file_state(&path);
    let backup_path = snapshot_backup_path(&path);
    let backup = snapshot_file_state(&backup_path);
    let cache = SNAPSHOT_CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    if let Ok(cache) = cache.lock() {
        if let Some(entry) = cache.get(&path) {
            if entry.primary == primary && entry.backup == backup {
                return Ok(Arc::clone(&entry.snapshot));
            }
        }
    }

    let snapshot = Arc::new(load_inventory_snapshot_uncached(&path, workspace_id)?);
    if let Ok(mut cache) = cache.lock() {
        while cache.len() >= SNAPSHOT_CACHE_LIMIT && !cache.contains_key(&path) {
            let Some(evicted) = cache.iter().find_map(|(cached_path, entry)| {
                (Arc::strong_count(&entry.snapshot) == 1).then(|| cached_path.clone())
            }) else {
                break;
            };
            cache.remove(&evicted);
        }
        cache.insert(
            path,
            CachedSnapshot {
                primary,
                backup,
                snapshot: Arc::clone(&snapshot),
            },
        );
    }
    Ok(snapshot)
}

fn load_inventory_snapshot_uncached(
    path: &Path,
    workspace_id: &str,
) -> Result<InventorySnapshot, String> {
    match load_snapshot_file(path, workspace_id) {
        Ok(snapshot) => Ok(snapshot),
        Err(primary_error) => {
            let backup = snapshot_backup_path(path);
            let mut snapshot = load_snapshot_file(&backup, workspace_id).map_err(|backup_error| {
                format!(
                    "스냅샷을 열 수 없습니다: {primary_error}; 백업도 열 수 없습니다: {backup_error}"
                )
            })?;
            mark_reindex_required(&mut snapshot, BACKUP_REINDEX_NOTE);
            Ok(snapshot)
        }
    }
}

fn snapshot_file_state(path: &Path) -> SnapshotFileState {
    match fs::metadata(path) {
        Ok(metadata) => SnapshotFileState {
            modified: metadata.modified().ok(),
            length: Some(metadata.len()),
        },
        Err(_) => SnapshotFileState {
            modified: None,
            length: None,
        },
    }
}

fn invalidate_cached_snapshot(path: &Path) {
    if let Some(cache) = SNAPSHOT_CACHE.get() {
        if let Ok(mut cache) = cache.lock() {
            cache.remove(path);
        }
    }
}

fn load_snapshot_file(path: &Path, workspace_id: &str) -> Result<InventorySnapshot, String> {
    let json = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let value: Value = serde_json::from_str(&json).map_err(|error| error.to_string())?;
    let version = value.get("schemaVersion");
    let mut snapshot = match version {
        None => {
            let mut snapshot: InventorySnapshot =
                serde_json::from_value(value).map_err(|error| error.to_string())?;
            snapshot.schema_version = 1;
            snapshot
        }
        Some(version) if version.as_u64() == Some(1) || version.as_u64() == Some(2) => {
            serde_json::from_value(value).map_err(|error| error.to_string())?
        }
        Some(version) => incompatible_snapshot(&value, version.as_u64()),
    };

    if snapshot.workspace_id != workspace_id {
        return Err("스냅샷 프로젝트 ID가 경로와 일치하지 않습니다".to_string());
    }
    snapshot = canonicalize_snapshot(snapshot);
    Ok(snapshot)
}

fn incompatible_snapshot(value: &Value, version: Option<u64>) -> InventorySnapshot {
    let workspace_id = value
        .get("workspaceId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let saved_at = value
        .get("savedAt")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mut snapshot = InventorySnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        workspace_id,
        saved_at,
        metadata: SnapshotMetadata::default(),
        stale_reasons: Vec::new(),
        links: Vec::new(),
        items: Vec::new(),
    };
    snapshot.metadata.migration.source_schema_version =
        version.and_then(|value| u32::try_from(value).ok());
    mark_reindex_required(
        &mut snapshot,
        "지원하지 않는 스냅샷 버전은 최신 형식으로 해석하지 않았습니다.",
    );
    snapshot
}

fn canonicalize_snapshot(mut snapshot: InventorySnapshot) -> InventorySnapshot {
    let source_version = snapshot.schema_version;
    snapshot.schema_version = SNAPSHOT_SCHEMA_VERSION;
    if source_version == 1 {
        snapshot.metadata.migration.source_schema_version = Some(1);
        push_unique(&mut snapshot.metadata.migration.notes, V1_MIGRATION_NOTE);
        if snapshot.items.iter().any(|entry| entry.source == "code") {
            mark_reindex_required(&mut snapshot, V1_CODE_REINDEX_NOTE);
        }
    } else if source_version != SNAPSHOT_SCHEMA_VERSION {
        snapshot.metadata.migration.source_schema_version = Some(source_version);
        mark_reindex_required(
            &mut snapshot,
            "지원하지 않는 스냅샷 버전은 다시 읽어야 합니다.",
        );
        snapshot.items.clear();
        snapshot.links.clear();
        return snapshot;
    }

    snapshot.items.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.source.cmp(&right.source))
            .then_with(|| left.name.cmp(&right.name))
    });
    let mut items = BTreeMap::<String, InventoryItem>::new();
    let mut gaps = Vec::new();
    for mut entry in std::mem::take(&mut snapshot.items) {
        normalize_item(&mut entry);
        if entry.id.is_empty() {
            gaps.push(gap(
                "gap:node:empty-id".to_string(),
                "invalid-node",
                "ID가 없는 노드를 제외했습니다.",
                Vec::new(),
            ));
            continue;
        }
        match items.entry(entry.id.clone()) {
            Entry::Vacant(slot) => {
                slot.insert(entry);
            }
            Entry::Occupied(mut slot) if compatible_items(slot.get(), &entry) => {
                merge_item(slot.get_mut(), entry);
            }
            Entry::Occupied(slot) => {
                let id = slot.key().clone();
                gaps.push(gap(
                    format!("gap:node-conflict:{id}"),
                    "node-conflict",
                    "같은 ID가 서로 다른 노드를 가리켜 다시 읽기가 필요합니다.",
                    vec![id],
                ));
                snapshot.metadata.migration.reindex_required = true;
            }
        }
    }

    let node_ids = items.keys().cloned().collect::<BTreeSet<_>>();
    for entry in items.values_mut() {
        if entry
            .parent_id
            .as_ref()
            .is_some_and(|parent| !node_ids.contains(parent))
        {
            let parent = entry.parent_id.take().unwrap_or_default();
            gaps.push(gap(
                format!("gap:parent:{}", entry.id),
                "dangling-parent",
                "존재하지 않는 상위 노드 참조를 제거했습니다.",
                vec![entry.id.clone(), parent],
            ));
        }
    }

    snapshot.links.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.from.cmp(&right.from))
            .then_with(|| left.to.cmp(&right.to))
    });
    let mut links = BTreeMap::<String, SnapshotLink>::new();
    let mut relationships = BTreeSet::new();
    let mut unscored_code_calls = 0usize;
    for mut link in std::mem::take(&mut snapshot.links) {
        if link.kind == "code_call"
            && !link
                .evidence
                .iter()
                .any(|evidence| evidence.kind == "engine-confidence")
        {
            link.truth_class = "unknown".to_string();
            link.evidence.push(Evidence {
                kind: "engine-confidence".to_string(),
                text: "unknown".to_string(),
            });
            link.evidence.push(Evidence {
                kind: "engine-confidence-score".to_string(),
                text: "점수 없음".to_string(),
            });
            unscored_code_calls += 1;
        }
        normalize_link(&mut link);
        if !node_ids.contains(&link.from) || !node_ids.contains(&link.to) {
            gaps.push(gap(
                format!("gap:link:{}", link.id),
                "dangling-relationship",
                "끝점이 없는 관계를 제외했습니다.",
                vec![link.from, link.to],
            ));
            continue;
        }
        let relationship = format!(
            "{}\0{}\0{}\0{}\0{}",
            link.kind,
            link.from,
            link.to,
            link.label.as_deref().unwrap_or_default(),
            link.engine_edge_type.as_deref().unwrap_or_default()
        );
        if !relationships.insert(relationship) {
            continue;
        }
        match links.entry(link.id.clone()) {
            Entry::Vacant(slot) => {
                slot.insert(link);
            }
            Entry::Occupied(slot) if slot.get() == &link => {}
            Entry::Occupied(slot) => {
                let id = slot.key().clone();
                gaps.push(gap(
                    format!("gap:link-conflict:{id}"),
                    "relationship-conflict",
                    "같은 관계 ID에 서로 다른 근거가 있어 다시 읽기가 필요합니다.",
                    vec![id],
                ));
                snapshot.metadata.migration.reindex_required = true;
            }
        }
    }

    if unscored_code_calls > 0 {
        gaps.push(gap(
            "gap:code-call-confidence".to_string(),
            "unscored-code-call",
            &format!(
                "엔진 신뢰도 정보가 없는 CALLS {unscored_code_calls}개를 확정 관계에서 제외했습니다."
            ),
            Vec::new(),
        ));
        mark_reindex_required(&mut snapshot, UNSCORED_CODE_CALL_REINDEX_NOTE);
    }

    snapshot.items = items.into_values().collect();
    snapshot.links = links.into_values().collect();
    snapshot.metadata.gaps.extend(gaps);
    snapshot
        .metadata
        .gaps
        .sort_by(|left, right| left.id.cmp(&right.id));
    snapshot
        .metadata
        .gaps
        .dedup_by(|left, right| left.id == right.id);
    if snapshot.metadata.migration.reindex_required {
        push_unique(&mut snapshot.stale_reasons, REINDEX_REASON);
    }
    snapshot
}

fn code_item(
    entry: &CodeInventoryItem,
    kind: &str,
    layer: &str,
    fallback_project: &str,
) -> InventoryItem {
    let path = entry
        .file_path
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let project = if entry.project.is_empty() {
        fallback_project
    } else {
        &entry.project
    };
    let qualified_name = if entry.qualified_name.is_empty() {
        &entry.id
    } else {
        &entry.qualified_name
    };
    let engine_label = if entry.engine_label.is_empty() {
        &entry.kind
    } else {
        &entry.engine_label
    };
    let location = path.clone().map(|path| SourceLocation {
        path,
        line: entry.line,
        column: entry
            .column
            .or_else(|| detail_u64(&entry.detail, &["startColumn", "start_column", "column"])),
        end_line: entry.end_line,
        end_column: entry
            .end_column
            .or_else(|| detail_u64(&entry.detail, &["endColumn", "end_column"])),
    });

    InventoryItem {
        id: format!("code:{}", entry.id),
        kind: kind.to_string(),
        name: entry.name.clone(),
        layer: layer.to_string(),
        source: "code".to_string(),
        parent_id: None,
        path,
        qualified_name: non_empty(qualified_name),
        engine_label: non_empty(engine_label),
        project_id: non_empty(project),
        group_id: detail_string(
            &entry.detail,
            &[
                "groupId",
                "group_id",
                "parentQualifiedName",
                "parent_qualified_name",
                "module",
                "namespace",
                "package",
            ],
        ),
        location,
        is_primary_key: false,
        is_foreign_key: false,
        nullable: None,
    }
}

fn resolve_db_table_key(
    stable_key: Option<&str>,
    schema: Option<&str>,
    name: Option<&str>,
    table_keys: &BTreeSet<String>,
    stable_table_keys: &BTreeMap<String, String>,
    named_table_keys: &BTreeMap<String, Vec<String>>,
) -> Option<String> {
    if let Some(table_key) = stable_key.and_then(|key| stable_table_keys.get(key)) {
        return Some(table_key.clone());
    }
    let name = name?;
    if let Some(schema) = schema.filter(|schema| !schema.is_empty()) {
        let table_key = db_table_key(Some(schema), name);
        if table_keys.contains(&table_key) {
            return Some(table_key);
        }
    }
    named_table_keys
        .get(name)
        .filter(|matches| matches.len() == 1)
        .and_then(|matches| matches.first().cloned())
}

fn constraint_from_foreign_key(foreign_key: &DbForeignKey, direction: &str) -> DbConstraint {
    DbConstraint {
        key: foreign_key.key.clone(),
        name: foreign_key.name.clone(),
        kind: "foreign_key".to_string(),
        columns: foreign_key.columns.clone(),
        column_keys: foreign_key.column_keys.clone(),
        referenced_table_key: foreign_key.referenced_table_key.clone(),
        referenced_schema: foreign_key.referenced_schema.clone(),
        referenced_table: Some(foreign_key.referenced_table.clone()),
        referenced_columns: foreign_key.referenced_columns.clone(),
        referenced_column_keys: foreign_key.referenced_column_keys.clone(),
        expression: None,
        source: format!("foreign_keys.{direction}"),
    }
}

fn append_db_constraint(
    snapshot: &mut InventorySnapshot,
    table_key: &str,
    profile_id: &str,
    constraint: &DbConstraint,
    column_ids: &BTreeSet<String>,
    stable_column_ids: &BTreeMap<String, String>,
) {
    let table_id = format!("db:table:{table_key}");
    let identity = constraint
        .key
        .clone()
        .or_else(|| constraint.name.clone())
        .unwrap_or_else(|| {
            format!(
                "{}:{:016x}",
                constraint.kind,
                stable_hash(&serde_json::to_string(constraint).unwrap_or_default())
            )
        });
    let constraint_id = format!("db:constraint:{table_key}:{identity}");
    let mut constraint_item = item(
        &constraint_id,
        "constraint",
        constraint.name.as_deref().unwrap_or(&constraint.kind),
        "data",
        "db",
        Some(&table_id),
        constraint.expression.as_deref(),
    );
    constraint_item.qualified_name = constraint.key.clone().or(Some(identity));
    constraint_item.engine_label = Some(format!("Constraint:{}", constraint.kind));
    constraint_item.project_id = Some(profile_id.to_string());
    constraint_item.is_primary_key = constraint.kind == "primary_key";
    constraint_item.is_foreign_key = constraint.kind == "foreign_key";
    snapshot.items.push(constraint_item);

    let edge_type = constraint.kind.to_ascii_uppercase();
    snapshot.links.push(db_evidence_link(
        table_id,
        constraint_id.clone(),
        "contains",
        constraint.name.clone(),
        &edge_type,
        "structural",
        constraint_evidence(constraint),
    ));
    let endpoint_count = constraint.columns.len().max(constraint.column_keys.len());
    for index in 0..endpoint_count {
        let column = constraint.columns.get(index);
        let stable_key = constraint.column_keys.get(index);
        let Some(column_id) = resolve_db_column_id(
            table_key,
            column.map(String::as_str),
            stable_key.map(String::as_str),
            column_ids,
            stable_column_ids,
        ) else {
            let endpoint = stable_key
                .or(column)
                .map(String::as_str)
                .unwrap_or("unknown");
            snapshot.metadata.gaps.push(gap(
                format!("gap:db-constraint-column:{constraint_id}:{endpoint}"),
                "db-constraint-missing-column",
                "Constraint column이 inventory에 없어 구조 관계를 만들지 않았습니다.",
                vec![constraint_id.clone()],
            ));
            continue;
        };
        let mut link = db_evidence_link(
            constraint_id.clone(),
            column_id,
            "db_constraint",
            constraint.name.clone(),
            &edge_type,
            "confirmed",
            constraint_evidence(constraint),
        );
        push_evidence(
            &mut link.evidence,
            "db-column-key",
            stable_key.map(String::as_str),
        );
        snapshot.links.push(link);
    }
}

fn append_db_index(
    snapshot: &mut InventorySnapshot,
    table_key: &str,
    profile_id: &str,
    index: &DbIndex,
    column_ids: &BTreeSet<String>,
    stable_column_ids: &BTreeMap<String, String>,
) {
    let table_id = format!("db:table:{table_key}");
    let identity = index.key.as_deref().unwrap_or(&index.name);
    let index_id = format!("db:index:{table_key}:{identity}");
    let mut index_item = item(
        &index_id,
        "index",
        &index.name,
        "data",
        "db",
        Some(&table_id),
        index.predicate.as_deref().or(index.expression.as_deref()),
    );
    index_item.qualified_name = index.key.clone().or_else(|| Some(index.name.clone()));
    index_item.engine_label = Some("Index".to_string());
    index_item.project_id = Some(profile_id.to_string());
    index_item.is_primary_key = index.primary;
    snapshot.items.push(index_item);

    snapshot.links.push(db_evidence_link(
        table_id,
        index_id.clone(),
        "contains",
        Some(index.name.clone()),
        "INDEX",
        "structural",
        index_evidence(index),
    ));
    let endpoint_count = index.columns.len().max(index.column_keys.len());
    for ordinal in 0..endpoint_count {
        let column = index.columns.get(ordinal);
        let stable_key = index.column_keys.get(ordinal);
        let Some(column_id) = resolve_db_column_id(
            table_key,
            column.map(String::as_str),
            stable_key.map(String::as_str),
            column_ids,
            stable_column_ids,
        ) else {
            let endpoint = stable_key
                .or(column)
                .map(String::as_str)
                .unwrap_or("unknown");
            snapshot.metadata.gaps.push(gap(
                format!("gap:db-index-column:{index_id}:{endpoint}"),
                "db-index-missing-column",
                "Index column이 inventory에 없어 구조 관계를 만들지 않았습니다.",
                vec![index_id.clone()],
            ));
            continue;
        };
        let mut link = db_evidence_link(
            index_id.clone(),
            column_id,
            "db_index",
            Some(index.name.clone()),
            "INDEX",
            "confirmed",
            index_evidence(index),
        );
        push_evidence(
            &mut link.evidence,
            "db-column-key",
            stable_key.map(String::as_str),
        );
        snapshot.links.push(link);
    }
}

fn append_db_dependent(
    snapshot: &mut InventorySnapshot,
    table_key: &str,
    table_schema: Option<&str>,
    profile_id: &str,
    dependent: &DbDependentObject,
    stable_column_ids: &BTreeMap<String, String>,
) {
    let table_id = format!("db:table:{table_key}");
    let dependent_id = format!(
        "db:{}:{}",
        dependent.kind,
        encode_db_identity_component(&dependent.key)
    );
    let parent_id = (dependent.kind == "trigger").then_some(table_id.as_str());
    let mut dependent_item = item(
        &dependent_id,
        &dependent.kind,
        &dependent.name,
        "data",
        "db",
        parent_id,
        None,
    );
    dependent_item.qualified_name = Some(dependent.key.clone());
    dependent_item.engine_label = Some(
        match dependent.kind.as_str() {
            "view" => "View",
            "trigger" => "Trigger",
            "routine" => "Routine",
            _ => "DB Object",
        }
        .to_string(),
    );
    dependent_item.project_id = Some(profile_id.to_string());
    dependent_item.group_id = if dependent.kind == "trigger" {
        table_schema.map(str::to_string)
    } else {
        None
    };
    snapshot.items.push(dependent_item);

    let evidence = dependent_evidence(dependent);
    if dependent.kind == "trigger" {
        snapshot.links.push(db_evidence_link(
            table_id,
            dependent_id,
            "db_trigger",
            Some(dependent.name.clone()),
            "TABLE_HAS_TRIGGER",
            "confirmed",
            evidence,
        ));
        return;
    }

    let column_edge_type = match dependent.kind.as_str() {
        "view" => "VIEW_DEPENDS_ON_COLUMN",
        "routine" => "ROUTINE_DEPENDS_ON_COLUMN",
        _ => "DB_OBJECT_DEPENDS_ON_COLUMN",
    };
    let table_edge_type = match dependent.kind.as_str() {
        "view" => "VIEW_DEPENDS_ON_TABLE",
        "routine" => "ROUTINE_DEPENDS_ON_TABLE",
        _ => "DB_OBJECT_DEPENDS_ON_TABLE",
    };
    let mut resolved_columns = 0usize;
    for column_key in &dependent.column_keys {
        let Some(column_id) = stable_column_ids.get(column_key).cloned() else {
            snapshot.metadata.gaps.push(gap(
                format!("gap:db-dependent-column:{dependent_id}:{column_key}"),
                "db-dependent-missing-column",
                "DB 의존 객체의 컬럼 endpoint가 inventory에 없어 해당 컬럼 관계를 만들지 않았습니다.",
                vec![dependent_id.clone(), table_id.clone()],
            ));
            continue;
        };
        let mut link = db_evidence_link(
            dependent_id.clone(),
            column_id,
            "db_dependency",
            Some(dependent.name.clone()),
            column_edge_type,
            "confirmed",
            evidence.clone(),
        );
        push_evidence(&mut link.evidence, "db-column-key", Some(column_key));
        snapshot.links.push(link);
        resolved_columns += 1;
    }

    if dependent.column_keys.is_empty() {
        snapshot.links.push(db_evidence_link(
            dependent_id,
            table_id,
            "db_dependency",
            Some(dependent.name.clone()),
            table_edge_type,
            "confirmed",
            evidence,
        ));
    } else if resolved_columns == 0 {
        let mut link = db_evidence_link(
            dependent_id,
            table_id,
            "db_dependency",
            Some(dependent.name.clone()),
            "DEPENDENCY_SCOPE",
            "structural",
            evidence,
        );
        link.evidence.push(Evidence {
            kind: "db-normalization".to_string(),
            text: "확정된 컬럼 endpoint를 복원하지 못해 의존 객체가 이 테이블 범위에 속한다는 사실만 보존했습니다."
                .to_string(),
        });
        snapshot.links.push(link);
    }
}

fn dependent_evidence(dependent: &DbDependentObject) -> Vec<Evidence> {
    vec![
        Evidence {
            kind: "db-object-key".to_string(),
            text: dependent.key.clone(),
        },
        Evidence {
            kind: "db-dependent-kind".to_string(),
            text: dependent.kind.clone(),
        },
        Evidence {
            kind: "db-relation".to_string(),
            text: dependent.relation.clone(),
        },
        Evidence {
            kind: "db-column-keys".to_string(),
            text: serde_json::to_string(&dependent.column_keys)
                .unwrap_or_else(|_| "[]".to_string()),
        },
        Evidence {
            kind: "db-contract-field".to_string(),
            text: "dependents".to_string(),
        },
    ]
}

fn resolve_db_column_id(
    table_key: &str,
    column: Option<&str>,
    stable_key: Option<&str>,
    column_ids: &BTreeSet<String>,
    stable_column_ids: &BTreeMap<String, String>,
) -> Option<String> {
    stable_key
        .and_then(|key| stable_column_ids.get(key).cloned())
        .or_else(|| {
            column
                .map(|column| db_column_id(table_key, column))
                .filter(|column_id| column_ids.contains(column_id))
        })
}

fn constraint_evidence(constraint: &DbConstraint) -> Vec<Evidence> {
    let mut evidence = vec![
        Evidence {
            kind: "db-constraint-kind".to_string(),
            text: constraint.kind.clone(),
        },
        Evidence {
            kind: "db-columns".to_string(),
            text: serde_json::to_string(&constraint.columns).unwrap_or_else(|_| "[]".to_string()),
        },
        Evidence {
            kind: "db-referenced-columns".to_string(),
            text: serde_json::to_string(&constraint.referenced_columns)
                .unwrap_or_else(|_| "[]".to_string()),
        },
        Evidence {
            kind: "db-column-keys".to_string(),
            text: serde_json::to_string(&constraint.column_keys)
                .unwrap_or_else(|_| "[]".to_string()),
        },
        Evidence {
            kind: "db-referenced-column-keys".to_string(),
            text: serde_json::to_string(&constraint.referenced_column_keys)
                .unwrap_or_else(|_| "[]".to_string()),
        },
        Evidence {
            kind: "db-contract-field".to_string(),
            text: constraint.source.clone(),
        },
    ];
    push_evidence(&mut evidence, "db-object-key", constraint.key.as_deref());
    push_evidence(&mut evidence, "db-object-name", constraint.name.as_deref());
    push_evidence(
        &mut evidence,
        "db-referenced-table-key",
        constraint.referenced_table_key.as_deref(),
    );
    push_evidence(
        &mut evidence,
        "db-referenced-schema",
        constraint.referenced_schema.as_deref(),
    );
    push_evidence(
        &mut evidence,
        "db-referenced-table",
        constraint.referenced_table.as_deref(),
    );
    push_evidence(
        &mut evidence,
        "db-expression",
        constraint.expression.as_deref(),
    );
    evidence
}

fn index_evidence(index: &DbIndex) -> Vec<Evidence> {
    let mut evidence = vec![
        Evidence {
            kind: "db-columns".to_string(),
            text: serde_json::to_string(&index.columns).unwrap_or_else(|_| "[]".to_string()),
        },
        Evidence {
            kind: "db-column-keys".to_string(),
            text: serde_json::to_string(&index.column_keys).unwrap_or_else(|_| "[]".to_string()),
        },
        Evidence {
            kind: "db-index-unique".to_string(),
            text: index.unique.to_string(),
        },
        Evidence {
            kind: "db-index-primary".to_string(),
            text: index.primary.to_string(),
        },
    ];
    push_evidence(&mut evidence, "db-object-key", index.key.as_deref());
    push_evidence(&mut evidence, "db-object-name", Some(&index.name));
    push_evidence(
        &mut evidence,
        "db-index-predicate",
        index.predicate.as_deref(),
    );
    push_evidence(
        &mut evidence,
        "db-index-expression",
        index.expression.as_deref(),
    );
    evidence
}

fn push_evidence(evidence: &mut Vec<Evidence>, kind: &str, text: Option<&str>) {
    if let Some(text) = text.filter(|text| !text.is_empty()) {
        evidence.push(Evidence {
            kind: kind.to_string(),
            text: text.to_string(),
        });
    }
}

fn db_evidence_link(
    from: String,
    to: String,
    kind: &str,
    label: Option<String>,
    engine_edge_type: &str,
    truth_class: &str,
    evidence: Vec<Evidence>,
) -> SnapshotLink {
    SnapshotLink {
        id: format!("{kind}:{from}->{to}"),
        from,
        to,
        kind: kind.to_string(),
        label,
        truth_class: truth_class.to_string(),
        direction: "outbound".to_string(),
        engine_edge_type: Some(engine_edge_type.to_string()),
        evidence,
    }
}

fn stable_hash(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

fn code_call_link(call: &CodeCall) -> SnapshotLink {
    let (truth_class, confidence) = match call.confidence {
        Some(score) if score >= CONFIRMED_CODE_CALL_CONFIDENCE => ("confirmed", "high"),
        Some(score) if score >= CANDIDATE_CODE_CALL_CONFIDENCE => ("candidate", "medium"),
        Some(_) => ("unknown", "low"),
        None => ("unknown", "unknown"),
    };
    let mut evidence = vec![
        Evidence {
            kind: "engine-edge".to_string(),
            text: "codebase-memory CALLS".to_string(),
        },
        Evidence {
            kind: "engine-confidence".to_string(),
            text: confidence.to_string(),
        },
        Evidence {
            kind: "engine-confidence-score".to_string(),
            text: call
                .confidence
                .map(|score| format!("{score}%"))
                .unwrap_or_else(|| "점수 없음".to_string()),
        },
    ];
    if let Some(strategy) = call.strategy.as_deref() {
        evidence.push(Evidence {
            kind: "engine-strategy".to_string(),
            text: strategy.to_string(),
        });
    }
    if let Some(expression) = call.expression.as_deref() {
        evidence.push(Evidence {
            kind: "engine-callee".to_string(),
            text: expression.to_string(),
        });
    }

    SnapshotLink {
        id: format!("code-call:{}->{}", call.from, call.to),
        from: format!("code:{}", call.from),
        to: format!("code:{}", call.to),
        kind: "code_call".to_string(),
        label: Some("CALLS".to_string()),
        truth_class: truth_class.to_string(),
        direction: "outbound".to_string(),
        engine_edge_type: Some("CALLS".to_string()),
        evidence,
    }
}

fn confirmed_link(
    id: String,
    from: String,
    to: String,
    kind: &str,
    engine_edge_type: &str,
    evidence: &str,
) -> SnapshotLink {
    SnapshotLink {
        id,
        from,
        to,
        kind: kind.to_string(),
        label: Some(engine_edge_type.to_string()),
        truth_class: "confirmed".to_string(),
        direction: "outbound".to_string(),
        engine_edge_type: Some(engine_edge_type.to_string()),
        evidence: vec![Evidence {
            kind: "engine-edge".to_string(),
            text: evidence.to_string(),
        }],
    }
}

fn normalize_item(entry: &mut InventoryItem) {
    if entry.location.is_none() && entry.source == "code" {
        entry.location = entry.path.clone().map(|path| SourceLocation {
            path,
            line: None,
            column: None,
            end_line: None,
            end_column: None,
        });
    }
}

fn compatible_items(left: &InventoryItem, right: &InventoryItem) -> bool {
    left.kind == right.kind
        && left.name == right.name
        && left.layer == right.layer
        && left.source == right.source
        && compatible_option(left.path.as_ref(), right.path.as_ref())
        && compatible_option(left.qualified_name.as_ref(), right.qualified_name.as_ref())
        && compatible_option(left.engine_label.as_ref(), right.engine_label.as_ref())
        && compatible_option(left.project_id.as_ref(), right.project_id.as_ref())
}

fn compatible_option<T: PartialEq>(left: Option<&T>, right: Option<&T>) -> bool {
    left.zip(right).is_none_or(|(left, right)| left == right)
}

fn merge_item(target: &mut InventoryItem, source: InventoryItem) {
    target.parent_id = target.parent_id.take().or(source.parent_id);
    target.path = target.path.take().or(source.path);
    target.qualified_name = target.qualified_name.take().or(source.qualified_name);
    target.engine_label = target.engine_label.take().or(source.engine_label);
    target.project_id = target.project_id.take().or(source.project_id);
    target.group_id = target.group_id.take().or(source.group_id);
    target.location = merge_location(target.location.take(), source.location);
    target.is_primary_key |= source.is_primary_key;
    target.is_foreign_key |= source.is_foreign_key;
    target.nullable = target.nullable.or(source.nullable);
}

fn merge_location(
    target: Option<SourceLocation>,
    source: Option<SourceLocation>,
) -> Option<SourceLocation> {
    match (target, source) {
        (Some(mut target), Some(source)) if target.path == source.path => {
            target.line = target.line.or(source.line);
            target.column = target.column.or(source.column);
            target.end_line = target.end_line.or(source.end_line);
            target.end_column = target.end_column.or(source.end_column);
            Some(target)
        }
        (Some(target), _) => Some(target),
        (None, source) => source,
    }
}

fn normalize_link(link: &mut SnapshotLink) {
    if link.id.is_empty() {
        link.id = format!("{}:{}->{}", link.kind, link.from, link.to);
    }
    if link.truth_class.is_empty() {
        link.truth_class = match link.kind.as_str() {
            "code_call" | "code_handle" | "db_fk" => "confirmed",
            "contains" | "db_constraint" => "structural",
            kind if kind.starts_with("candidate") => "candidate",
            _ => "unknown",
        }
        .to_string();
    }
    if link.direction.is_empty() {
        link.direction = "outbound".to_string();
    }
    if link.engine_edge_type.is_none() {
        link.engine_edge_type = match link.kind.as_str() {
            "code_call" => Some("CALLS"),
            "code_handle" => Some("HANDLES"),
            "db_fk" => Some("FOREIGN_KEY"),
            _ => None,
        }
        .map(str::to_string);
    }
    if link.evidence.is_empty() {
        if let Some(edge_type) = link.engine_edge_type.as_deref() {
            link.evidence.push(Evidence {
                kind: "engine-edge".to_string(),
                text: edge_type.to_string(),
            });
        }
    }
    link.evidence.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.text.cmp(&right.text))
    });
    link.evidence.dedup();
}

fn mark_reindex_required(snapshot: &mut InventorySnapshot, note: &str) {
    snapshot.metadata.migration.reindex_required = true;
    push_unique(&mut snapshot.metadata.migration.notes, note);
    push_unique(&mut snapshot.stale_reasons, REINDEX_REASON);
}

fn gap(id: String, kind: &str, message: &str, related_ids: Vec<String>) -> SnapshotGap {
    SnapshotGap {
        id,
        kind: kind.to_string(),
        message: message.to_string(),
        related_ids,
    }
}

fn atomic_save(path: &Path, contents: &[u8], workspace_id: &str) -> Result<(), String> {
    let sequence = NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed);
    let temp = path.with_file_name(format!(
        "inventory-snapshot.{}.{}.{}.tmp",
        std::process::id(),
        timestamp(),
        sequence
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)
        .map_err(|error| error.to_string())?;
    if let Err(error) = file.write_all(contents).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&temp);
        return Err(error.to_string());
    }
    drop(file);

    let backup = snapshot_backup_path(path);
    let had_current = path.is_file();
    let rotate_current = had_current && load_snapshot_file(path, workspace_id).is_ok();
    if rotate_current {
        if backup.exists() {
            fs::remove_file(&backup).map_err(|error| {
                let _ = fs::remove_file(&temp);
                error.to_string()
            })?;
        }
        fs::rename(path, &backup).map_err(|error| {
            let _ = fs::remove_file(&temp);
            error.to_string()
        })?;
    } else if had_current {
        fs::remove_file(path).map_err(|error| {
            let _ = fs::remove_file(&temp);
            error.to_string()
        })?;
    }

    if let Err(error) = fs::rename(&temp, path) {
        if rotate_current {
            let _ = fs::rename(&backup, path);
        }
        let _ = fs::remove_file(&temp);
        return Err(error.to_string());
    }
    Ok(())
}

fn engine_version(registry: &EngineRegistry, engine_id: &str) -> Option<String> {
    registry
        .engines
        .iter()
        .find(|engine| engine.id == engine_id)
        .map(|engine| engine.expected_version.trim())
        .filter(|version| !version.is_empty() && *version != "unknown")
        .map(str::to_string)
}

fn engine_checksum(registry: &EngineRegistry, engine_id: &str) -> Option<String> {
    registry
        .engines
        .iter()
        .find(|engine| engine.id == engine_id)
        .and_then(|engine| engine.sha256.as_deref())
        .map(str::trim)
        .filter(|checksum| !checksum.is_empty())
        .map(str::to_string)
}

fn engine_contract_version(registry: &EngineRegistry, engine_id: &str) -> Option<String> {
    registry
        .engines
        .iter()
        .find(|engine| engine.id == engine_id)
        .map(|engine| engine.contract_version.trim())
        .filter(|version| !version.is_empty())
        .map(str::to_string)
}

fn mark_engine_staleness(
    metadata: &SnapshotSourceMetadata,
    registry: &EngineRegistry,
    engine_id: &str,
    label: &str,
    reasons: &mut Vec<String>,
) {
    let Some(current) = registry
        .engines
        .iter()
        .find(|engine| engine.id == engine_id)
    else {
        push_unique(reasons, &format!("{label} 엔진 정보를 확인할 수 없습니다"));
        return;
    };
    if metadata.engine_id.as_deref() != Some(engine_id) {
        push_unique(reasons, &format!("{label} 엔진 식별자가 바뀌었습니다"));
    }
    if engine_version(registry, engine_id).as_deref() != metadata.engine_version.as_deref() {
        push_unique(reasons, &format!("{label} 엔진 버전이 바뀌었습니다"));
    }
    let current_checksum = current.sha256.as_deref().map(str::trim);
    let saved_checksum = metadata.engine_checksum.as_deref().map(str::trim);
    if current_checksum.is_none()
        || saved_checksum.is_none()
        || !current_checksum
            .zip(saved_checksum)
            .is_some_and(|(current, saved)| current.eq_ignore_ascii_case(saved))
    {
        push_unique(
            reasons,
            &format!("{label} 엔진 checksum이 바뀌었거나 확인되지 않았습니다"),
        );
    }
    if engine_contract_version(registry, engine_id).as_deref()
        != metadata.contract_version.as_deref()
    {
        push_unique(reasons, &format!("{label} 엔진 contract가 바뀌었습니다"));
    }
}

fn mark_source_revision_staleness(
    saved: Option<&str>,
    current: Option<&(String, String)>,
    source_exists: bool,
    missing_reason: &str,
    changed_reason: &str,
    unavailable_reason: &str,
    reasons: &mut Vec<String>,
) {
    match (saved, current) {
        (Some(saved), Some((current, _))) if saved != current => {
            push_unique(reasons, changed_reason)
        }
        (Some(_), None) => push_unique(reasons, unavailable_reason),
        (None, Some(_)) => push_unique(reasons, missing_reason),
        (None, None) if source_exists => push_unique(reasons, missing_reason),
        (None, None) => push_unique(reasons, unavailable_reason),
        _ => {}
    }
}

fn code_source_revision(workspace: &Workspace) -> Option<(String, String)> {
    let root = Path::new(&workspace.repo_path);
    git_source_revision(root).or_else(|| folder_source_revision(root))
}

fn git_source_revision(root: &Path) -> Option<(String, String)> {
    let head = git_output(root, &["rev-parse", "HEAD"])?;
    let head = String::from_utf8(head).ok()?.trim().to_string();
    if head.len() < 7 {
        return None;
    }
    let status = git_output(
        root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    let paths = git_changed_paths(&status)?;
    let mut hasher = Sha256::new();
    hasher.update(b"git\0");
    hasher.update(head.as_bytes());
    hasher.update(b"\0status\0");
    hasher.update(&status);
    for relative in &paths {
        hasher.update(b"\0path\0");
        hasher.update(relative.to_string_lossy().as_bytes());
        hash_path_state(&mut hasher, &root.join(relative))?;
    }
    let revision = format!("{:X}", hasher.finalize());
    let state = if paths.is_empty() {
        "clean".to_string()
    } else {
        format!("변경 {}개", paths.len())
    };
    Some((revision, format!("git {} · {state}", &head[..7])))
}

fn git_output(root: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    output.status.success().then_some(output.stdout)
}

fn git_changed_paths(status: &[u8]) -> Option<BTreeSet<PathBuf>> {
    let records = status
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
        .collect::<Vec<_>>();
    let mut paths = BTreeSet::new();
    let mut index = 0;
    while index < records.len() {
        let record = records[index];
        if record.len() < 4 || record[2] != b' ' {
            return None;
        }
        let relative = PathBuf::from(String::from_utf8_lossy(&record[3..]).into_owned());
        if relative.is_absolute()
            || relative.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
        {
            return None;
        }
        paths.insert(relative);
        if matches!(record[0], b'R' | b'C') || matches!(record[1], b'R' | b'C') {
            index += 1;
        }
        index += 1;
    }
    Some(paths)
}

fn folder_source_revision(root: &Path) -> Option<(String, String)> {
    let root = fs::canonicalize(root).ok()?;
    let mut files = Vec::new();
    // ponytail: non-Git folders are scanned in full; add a persisted manifest cache only if
    // measured startup time becomes material on very large source trees.
    collect_source_files(&root, &mut files)?;
    files.sort();
    let mut hasher = Sha256::new();
    hasher.update(b"folder\0");
    for path in &files {
        let relative = path.strip_prefix(&root).ok()?;
        hasher.update(relative.to_string_lossy().as_bytes());
        hash_path_state(&mut hasher, path)?;
    }
    let revision = format!("{:X}", hasher.finalize());
    Some((
        revision.clone(),
        format!("파일 {}개 · {}", files.len(), short_revision(&revision)),
    ))
}

fn collect_source_files(directory: &Path, files: &mut Vec<PathBuf>) -> Option<()> {
    collect_files(directory, files, ignored_source_directory)
}

fn collect_files(
    directory: &Path,
    files: &mut Vec<PathBuf>,
    skip_directory: fn(&str) -> bool,
) -> Option<()> {
    let mut entries = fs::read_dir(directory)
        .ok()?
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let file_type = entry.file_type().ok()?;
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            if !skip_directory(&entry.file_name().to_string_lossy()) {
                collect_files(&path, files, skip_directory)?;
            }
        } else if file_type.is_file() {
            files.push(path);
        }
    }
    Some(())
}

fn ignored_source_directory(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        ".git"
            | ".codex"
            | ".idea"
            | ".next"
            | ".openai"
            | ".venv"
            | ".vscode"
            | "__pycache__"
            | "build"
            | "coverage"
            | "dist"
            | "node_modules"
            | "out"
            | "target"
            | "venv"
    )
}

fn db_source_revision(profile: &DbProfile) -> Option<(String, String)> {
    let path = Path::new(profile.path.as_deref()?);
    match &profile.source {
        DbSource::DdlSqlite => ddl_source_revision(path),
        DbSource::Sqlite => {
            let metadata = path.metadata().ok()?;
            let modified = metadata
                .modified()
                .ok()?
                .duration_since(UNIX_EPOCH)
                .ok()?
                .as_nanos();
            let mut hasher = Sha256::new();
            hasher.update(metadata.len().to_le_bytes());
            hasher.update(modified.to_le_bytes());
            let revision = format!("{:X}", hasher.finalize());
            Some((
                revision.clone(),
                format!("SQLite {}", short_revision(&revision)),
            ))
        }
        _ => None,
    }
}

fn ddl_source_revision(path: &Path) -> Option<(String, String)> {
    if path.is_file() {
        let revision = hash_file(path)?;
        return Some((
            revision.clone(),
            format!("DDL {}", short_revision(&revision)),
        ));
    }
    if !path.is_dir() {
        return None;
    }

    let root = fs::canonicalize(path).ok()?;
    let mut files = Vec::new();
    collect_files(&root, &mut files, |_| false)?;
    files.sort();
    let mut hasher = Sha256::new();
    hasher.update(b"ddl-directory\0");
    for file in &files {
        let relative = file.strip_prefix(&root).ok()?;
        hasher.update(relative.to_string_lossy().as_bytes());
        hash_path_state(&mut hasher, file)?;
    }
    let revision = format!("{:X}", hasher.finalize());
    Some((
        revision.clone(),
        format!("DDL {} · 파일 {}개", short_revision(&revision), files.len()),
    ))
}

fn hash_file(path: &Path) -> Option<String> {
    if !path.is_file() {
        return None;
    }
    let mut hasher = Sha256::new();
    hash_path_state(&mut hasher, path)?;
    Some(format!("{:X}", hasher.finalize()))
}

fn hash_path_state(hasher: &mut Sha256, path: &Path) -> Option<()> {
    if !path.exists() {
        hasher.update(b"\0missing");
        return Some(());
    }
    if path.is_dir() {
        hasher.update(b"\0directory");
        return Some(());
    }
    let mut file = fs::File::open(path).ok()?;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Some(())
}

fn short_revision(revision: &str) -> &str {
    revision.get(..8).unwrap_or(revision)
}

fn code_source_type(workspace: &Workspace) -> String {
    let normalized = workspace.repo_path.replace('\\', "/");
    if normalized.ends_with(&format!("/workspaces/{}/repo", workspace.id)) {
        "github-clone".to_string()
    } else {
        "local-folder".to_string()
    }
}

fn db_source_key(source: &impl Serialize) -> String {
    serde_json::to_value(source)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

fn localized_db_capability_warning(value: &str) -> String {
    if let Some((capability, source)) = db_capability_parts(value, " is partially tracked by the ")
    {
        return format!("{source} 어댑터는 {capability}를 일부만 추적합니다.");
    }
    if let Some((capability, source)) = db_capability_parts(value, " is not tracked by the ") {
        return format!("{source} 어댑터는 {capability}를 추적하지 않습니다.");
    }
    if let Some((capability, source)) = db_capability_parts(value, " support is unknown for the ") {
        return format!("{source} 어댑터의 {capability} 지원 여부를 확인할 수 없습니다.");
    }

    match value {
        "SQLite CHECK and UNIQUE constraints are not emitted as constraint nodes." => {
            "SQLite CHECK·UNIQUE 제약은 제약 노드로 수집하지 않습니다.".to_string()
        }
        "SQLite partial-index predicates and expression-index expressions are not extracted." => {
            "SQLite 부분 인덱스 조건식과 표현식 인덱스 식은 수집하지 않습니다.".to_string()
        }
        "SQLite generated columns are identified, but generation expressions are not extracted." => {
            "SQLite 생성 열 여부는 식별하지만 생성식은 수집하지 않습니다.".to_string()
        }
        "SQLite view dependencies are resolved from prepare-time read authorization; trigger-body dependencies are not emitted." => {
            "SQLite 뷰 의존성은 준비 단계 읽기 권한으로 확인하며, 트리거 본문의 의존성은 수집하지 않습니다.".to_string()
        }
        _ => value.to_string(),
    }
}

fn db_capability_parts<'a>(value: &'a str, separator: &str) -> Option<(&'static str, &'a str)> {
    let (capability, source) = value.split_once(separator)?;
    let source = source.strip_suffix(" adapter.")?;
    Some((
        match capability {
            "view dependency metadata" => "뷰 의존성 메타데이터",
            "trigger dependency metadata" => "트리거 의존성 메타데이터",
            "routine dependency metadata" => "프로시저·함수 의존성 메타데이터",
            "cross-object dependency metadata" => "객체 간 의존성 메타데이터",
            _ => return None,
        },
        match source {
            "ddl-sqlite" => "SQLite DDL",
            "sqlite" => "SQLite",
            "postgres" => "PostgreSQL",
            "mysql" => "MySQL/MariaDB",
            "sqlserver" => "SQL Server",
            "oracle" => "Oracle",
            _ => source,
        },
    ))
}

pub(crate) fn item(
    id: &str,
    kind: &str,
    name: &str,
    layer: &str,
    source: &str,
    parent_id: Option<&str>,
    path: Option<&str>,
) -> InventoryItem {
    InventoryItem {
        id: id.to_string(),
        kind: kind.to_string(),
        name: name.to_string(),
        layer: layer.to_string(),
        source: source.to_string(),
        parent_id: parent_id.map(str::to_string),
        path: path.map(str::to_string),
        qualified_name: None,
        engine_label: None,
        project_id: None,
        group_id: None,
        location: None,
        is_primary_key: false,
        is_foreign_key: false,
        nullable: None,
    }
}

fn db_table_key(schema: Option<&str>, name: &str) -> String {
    let name = encode_db_identity_component(name);
    match schema.filter(|value| !value.is_empty()) {
        Some(schema) => format!("{}.{name}", encode_db_identity_component(schema)),
        None => name,
    }
}

fn db_qualified_table_name(schema: Option<&str>, name: &str) -> String {
    match schema.filter(|value| !value.is_empty()) {
        Some(schema) => format!("{schema}.{name}"),
        None => name.to_string(),
    }
}

fn db_column_id(table_key: &str, column_name: &str) -> String {
    format!(
        "db:column:{table_key}:{}",
        encode_db_identity_component(column_name)
    )
}

fn encode_db_identity_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '%' => encoded.push_str("%25"),
            '.' => encoded.push_str("%2E"),
            ':' => encoded.push_str("%3A"),
            character => encoded.push(character),
        }
    }
    encoded
}

fn detail_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(key).and_then(Value::as_str))
        .and_then(non_empty)
}

fn detail_u64(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| value.get(key).and_then(Value::as_u64))
}

fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}

pub(crate) fn snapshot_path(app_data_dir: impl AsRef<Path>, workspace_id: &str) -> PathBuf {
    base_paths(app_data_dir)
        .workspaces_dir
        .join(workspace_id)
        .join("atlas")
        .join("inventory-snapshot.json")
}

pub(crate) fn snapshot_backup_path(path: &Path) -> PathBuf {
    path.with_file_name("inventory-snapshot.backup.json")
}

pub(crate) fn timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}
