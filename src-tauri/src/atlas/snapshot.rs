use crate::{
    engine::{self, EngineRegistry},
    paths::base_paths,
    workspace::{
        validate_workspace_id, CodeInventory, CodeInventoryItem, DbConstraint, DbForeignKey,
        DbIndex, DbInventory, Workspace,
    },
};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{btree_map::Entry, BTreeMap, BTreeSet, HashMap},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use super::model::{
    Evidence, InventoryItem, InventorySnapshot, SnapshotGap, SnapshotLink, SnapshotMetadata,
    SnapshotSourceMetadata, SourceLocation, SNAPSHOT_SCHEMA_VERSION,
};

const REINDEX_REASON: &str = "스냅샷 형식이 호환되지 않아 다시 읽어야 합니다";
static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(0);
static SNAPSHOT_CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedSnapshot>>> = OnceLock::new();
// Large code inventories can occupy tens of MB after deserialization. Two entries cover a
// workspace switch without turning this process-wide cache into an unbounded memory reserve.
const SNAPSHOT_CACHE_LIMIT: usize = 2;

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

pub fn build_inventory_snapshot(
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
            let kind = entry.engine_label.to_ascii_lowercase();
            let kind = if kind.is_empty() { "code" } else { &kind };
            code_item(entry, kind, "code", &code.project)
        }));
        snapshot.items.extend(
            code.files
                .iter()
                .map(|entry| code_item(entry, "file", "code", &code.project)),
        );
        snapshot.links.extend(code.calls.iter().map(|call| {
            confirmed_link(
                format!("code-call:{}->{}", call.from, call.to),
                format!("code:{}", call.from),
                format!("code:{}", call.to),
                "code_call",
                "CALLS",
                "codebase-memory CALLS",
            )
        }));
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
            source_path: None,
            source_type: "unknown".to_string(),
            profile_id: Some(db.profile_id.clone()),
        });
        for (index, warning) in db.capability_warnings.iter().enumerate() {
            snapshot.metadata.gaps.push(gap(
                format!("gap:db-capability:{index}"),
                "db-capability",
                warning,
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
                    .map(move |column| format!("db:column:{table_key}:{}", column.name))
            })
            .collect::<BTreeSet<_>>();
        let stable_column_ids = db
            .tables
            .iter()
            .flat_map(|table| {
                let table_key = db_table_key(table.schema.as_deref(), &table.name);
                table.columns.iter().filter_map(move |column| {
                    column.key.as_ref().map(|key| {
                        (
                            key.clone(),
                            format!("db:column:{table_key}:{}", column.name),
                        )
                    })
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
            table_item.qualified_name = table.key.clone().or_else(|| Some(table_key.clone()));
            table_item.engine_label = Some("Table".to_string());
            table_item.project_id = Some(db.profile_id.clone());
            table_item.group_id = table.schema.clone();
            snapshot.items.push(table_item);

            snapshot.items.extend(table.columns.iter().map(|column| {
                let mut column_item = InventoryItem {
                    id: format!("db:column:{table_key}:{}", column.name),
                    kind: "column".to_string(),
                    name: column.name.clone(),
                    layer: "data".to_string(),
                    source: "db".to_string(),
                    parent_id: Some(table_id.clone()),
                    path: column.data_type.clone(),
                    qualified_name: column
                        .key
                        .clone()
                        .or_else(|| Some(format!("{table_key}.{}", column.name))),
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

pub fn snapshot_with_metadata(
    mut snapshot: InventorySnapshot,
    workspace: &Workspace,
    registry: &EngineRegistry,
) -> InventorySnapshot {
    let saved_at = timestamp();
    let has_code = snapshot.metadata.code.is_some()
        || snapshot.items.iter().any(|entry| entry.source == "code");
    let has_db =
        snapshot.metadata.db.is_some() || snapshot.items.iter().any(|entry| entry.source == "db");
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
    snapshot.metadata.db = has_db.then(|| {
        let profile = workspace.active_db_profile_id.as_deref().and_then(|id| {
            workspace
                .db_profiles
                .iter()
                .find(|profile| profile.id == id)
        });
        SnapshotSourceMetadata {
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
            source_path: profile.and_then(|profile| profile.path.clone()),
            source_type: profile
                .map(|profile| db_source_key(&profile.source))
                .unwrap_or_else(|| "unknown".to_string()),
            profile_id: profile.map(|profile| profile.id.clone()),
        }
    });
    snapshot.stale_reasons.clear();
    canonicalize_snapshot(snapshot)
}

pub fn mark_snapshot_staleness(
    mut snapshot: InventorySnapshot,
    workspace: &Workspace,
    registry: &EngineRegistry,
) -> InventorySnapshot {
    snapshot.stale_reasons = snapshot_staleness_reasons(&snapshot, workspace, registry);
    snapshot
}

pub fn snapshot_staleness_reasons(
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
                if code.source_path.as_deref() != Some(workspace.repo_path.as_str()) {
                    push_unique(&mut reasons, "코드 프로젝트 경로가 바뀌었습니다");
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

pub fn save_inventory_snapshot(
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
    Ok(())
}

pub fn load_inventory_snapshot(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
) -> Result<InventorySnapshot, String> {
    Ok((*load_inventory_snapshot_cached(app_data_dir, workspace_id)?).clone())
}

pub fn load_inventory_snapshot_cached(
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
        if cache.len() >= SNAPSHOT_CACHE_LIMIT && !cache.contains_key(&path) {
            if let Some(evicted) = cache.keys().next().cloned() {
                cache.remove(&evicted);
            }
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
            mark_reindex_required(
                &mut snapshot,
                "주 스냅샷 대신 이전 백업을 복구했습니다. 다시 읽어 최신 상태를 확인하세요.",
            );
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
        push_unique(
            &mut snapshot.metadata.migration.notes,
            "Snapshot V1의 안전한 필드를 V2로 이전했습니다.",
        );
        if snapshot.items.iter().any(|entry| entry.source == "code") {
            mark_reindex_required(
                &mut snapshot,
                "Snapshot V1 코드 항목은 이전 BM25 bucket 분류를 신뢰할 수 없어 다시 읽어야 합니다.",
            );
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
    for mut link in std::mem::take(&mut snapshot.links) {
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
                .map(|column| format!("db:column:{table_key}:{column}"))
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
    match schema.filter(|value| !value.is_empty()) {
        Some(schema) => format!("{schema}.{name}"),
        None => name.to_string(),
    }
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
