use crate::paths::base_paths;
use crate::{engine, EngineRegistry};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use super::model::{
    DbConstraint, DbForeignKey, DbIndex, DbIndexResult, DbInventory, DbInventoryColumn,
    DbInventoryGap, DbInventoryTable, DbProfile, DbSource, IndexDbProfileRequest,
    SaveDbProfileRequest, Workspace,
};
use super::store::{
    engine_json_value, object_bool, object_string, read_workspace_by_id, timestamp,
    validate_workspace_id, value_items, workspace_db_cache_dir, workspace_id, write_workspace,
};

const DB_INVENTORY_LIMIT: usize = 1_000;
// ponytail: legacy fallback starts one process per table; bulk inventory removes this ceiling.
const MAX_FALLBACK_DESCRIBED_TABLES: usize = 200;
pub fn save_db_profile(
    app_data_dir: impl AsRef<Path>,
    request: SaveDbProfileRequest,
) -> Result<Workspace, String> {
    validate_workspace_id(&request.workspace_id)?;

    let name = request.name.trim();
    if name.is_empty() {
        return Err("DB 연결 이름이 필요합니다".to_string());
    }

    let source_path = request.path.unwrap_or_default().trim().to_string();
    let profile_path = if db_source_uses_path(&request.source) {
        if source_path.is_empty() {
            return Err("SQLite/DDL 연결에는 DB 경로가 필요합니다".to_string());
        }
        Some(source_path)
    } else {
        None
    };

    let paths = base_paths(app_data_dir);
    let mut workspace = read_workspace_by_id(&paths.workspaces_dir, &request.workspace_id)?;
    let id = workspace_id(name);
    let workspace_dir = paths.workspaces_dir.join(&request.workspace_id);
    let absolute_cache_path = workspace_db_cache_dir(&paths.workspaces_dir, &request.workspace_id)
        .join(&id)
        .join("graph.sqlite");
    let relative_cache_path = absolute_cache_path
        .strip_prefix(&workspace_dir)
        .map_err(|_| "DB 캐시 경로를 만들지 못했습니다".to_string())?
        .display()
        .to_string();
    let profile = DbProfile {
        id: id.clone(),
        name: name.to_string(),
        source: request.source,
        path: profile_path,
        host: None,
        port: None,
        database: None,
        username: None,
        cache_path: relative_cache_path,
        last_indexed_at: None,
        password_stored: false,
    };

    fs::create_dir_all(
        absolute_cache_path
            .parent()
            .ok_or_else(|| "DB 캐시 경로를 만들지 못했습니다".to_string())?,
    )
    .map_err(|error| error.to_string())?;

    workspace
        .db_profiles
        .retain(|item| item.name != profile.name);
    workspace.active_db_profile_id = Some(profile.id.clone());
    workspace.db_profiles.push(profile);
    workspace.updated_at = timestamp();

    write_workspace(&paths.workspaces_dir, &workspace)?;
    Ok(workspace)
}

pub fn index_db_profile(
    app_data_dir: impl AsRef<Path>,
    registry: &EngineRegistry,
    request: IndexDbProfileRequest,
) -> Result<DbIndexResult, String> {
    validate_workspace_id(&request.workspace_id)?;
    validate_workspace_id(&request.profile_id)?;

    let paths = base_paths(app_data_dir);
    let mut workspace = read_workspace_by_id(&paths.workspaces_dir, &request.workspace_id)?;
    let profile_index = workspace
        .db_profiles
        .iter()
        .position(|profile| profile.id == request.profile_id)
        .ok_or_else(|| "DB 연결을 찾을 수 없습니다".to_string())?;
    let profile = workspace.db_profiles[profile_index].clone();
    workspace.engine_cache.db_cache_dir = Some(
        workspace_db_cache_dir(&paths.workspaces_dir, &request.workspace_id)
            .display()
            .to_string(),
    );
    let cache_path = db_cache_path(&paths.workspaces_dir, &request.workspace_id, &profile.id);

    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let args = db_index_args(&profile, &cache_path, request.connection_string.as_deref())?;
    let db_engine = registry
        .engines
        .iter()
        .find(|engine| engine.id == "database-memory")
        .ok_or_else(|| "DB 읽기 도구가 등록되지 않았습니다".to_string())?;

    let run = engine::run_engine_command(db_engine, &args, Duration::from_secs(120))?;

    if run.ok {
        workspace.db_profiles[profile_index].last_indexed_at = Some(timestamp());
        workspace.updated_at = timestamp();
        write_workspace(&paths.workspaces_dir, &workspace)?;
    }

    let index_json = engine_json_value(&run.stdout);

    Ok(DbIndexResult {
        workspace,
        run,
        index_json,
    })
}

pub fn db_inventory(
    app_data_dir: impl AsRef<Path>,
    registry: &EngineRegistry,
    workspace_id: &str,
    profile_id: Option<&str>,
) -> Result<DbInventory, String> {
    validate_workspace_id(workspace_id)?;
    if let Some(profile_id) = profile_id {
        validate_workspace_id(profile_id)?;
    }

    let paths = base_paths(app_data_dir);
    let workspace = read_workspace_by_id(&paths.workspaces_dir, workspace_id)?;
    let selected_profile_id = profile_id
        .map(str::to_string)
        .or_else(|| workspace.active_db_profile_id.clone())
        .ok_or_else(|| "DB 연결이 필요합니다".to_string())?;
    validate_workspace_id(&selected_profile_id)?;
    let profile = workspace
        .db_profiles
        .iter()
        .find(|profile| profile.id == selected_profile_id)
        .ok_or_else(|| "DB 연결을 찾을 수 없습니다".to_string())?;
    let cache_path = db_cache_path(&paths.workspaces_dir, workspace_id, &profile.id);
    let db_engine = registry
        .engines
        .iter()
        .find(|engine| engine.id == "database-memory")
        .ok_or_else(|| "DB 읽기 도구가 등록되지 않았습니다".to_string())?;

    let bulk_args = db_inventory_args(profile, &cache_path)?;
    if let Some(inventory) =
        try_bulk_db_inventory(db_engine, &bulk_args, selected_profile_id.clone())
    {
        return Ok(inventory);
    }

    let table_output = run_db_query(db_engine, "find-table", profile, &cache_path)?;
    let column_output = run_db_query(db_engine, "find-column", profile, &cache_path)?;
    let table_json = engine_json_value(&table_output.stdout);
    let column_json = engine_json_value(&column_output.stdout);

    match (table_json, column_json) {
        (Some(table_json), Some(column_json)) => {
            let mut inventory =
                extract_db_inventory(selected_profile_id, &table_json, &column_json);
            record_bulk_fallback_gap(&mut inventory);
            enrich_db_inventory_with_describe(&mut inventory, db_engine, profile, &cache_path);
            Ok(inventory)
        }
        (table_json, column_json) => {
            let table_lines = if table_json.is_none() {
                table_output.stdout.as_str()
            } else {
                ""
            };
            let column_lines = if column_json.is_none() {
                column_output.stdout.as_str()
            } else {
                ""
            };
            let mut inventory = extract_db_inventory(
                selected_profile_id.clone(),
                table_json.as_ref().unwrap_or(&serde_json::Value::Null),
                column_json.as_ref().unwrap_or(&serde_json::Value::Null),
            );
            record_bulk_fallback_gap(&mut inventory);
            merge_db_inventory_lines(&mut inventory, table_lines, column_lines);
            enrich_db_inventory_with_describe(&mut inventory, db_engine, profile, &cache_path);
            Ok(inventory)
        }
    }
}

fn try_bulk_db_inventory(
    db_engine: &engine::EngineAvailability,
    args: &[String],
    profile_id: String,
) -> Option<DbInventory> {
    let run = engine::run_engine_command(db_engine, args, Duration::from_secs(120)).ok()?;
    if !run.ok {
        return None;
    }
    let value = engine_json_value(&run.stdout)?;
    let mut inventory = extract_bulk_db_inventory(profile_id, &value).ok()?;
    finalize_db_inventory(&mut inventory);
    Some(inventory)
}

fn run_db_query(
    db_engine: &engine::EngineAvailability,
    command: &str,
    profile: &DbProfile,
    cache_path: &Path,
) -> Result<engine::EngineRunResult, String> {
    let args = db_find_args(profile, command, cache_path)?;
    let run = engine::run_engine_command(db_engine, &args, Duration::from_secs(30))?;

    if run.ok {
        Ok(run)
    } else {
        let legacy_args = db_find_legacy_args(profile, command, cache_path)?;
        let legacy = engine::run_engine_command(db_engine, &legacy_args, Duration::from_secs(30))?;
        if legacy.ok {
            Ok(legacy)
        } else {
            Err("DB inventory fallback 검색에 실패했습니다.".to_string())
        }
    }
}

pub(crate) fn extract_db_inventory(
    profile_id: String,
    table_json: &serde_json::Value,
    column_json: &serde_json::Value,
) -> DbInventory {
    let mut tables = Vec::new();

    let table_values = table_json
        .get("table_matches")
        .and_then(serde_json::Value::as_array)
        .map(|values| values.iter().collect::<Vec<_>>())
        .unwrap_or_else(|| value_items(table_json));
    for table in table_values {
        if let Some(name) = object_string(table, &["tableName", "table_name", "table", "name"]) {
            let schema = object_string(table, &["schema", "schemaName", "schema_name"]);
            let mut db_table = empty_db_table(schema, name);
            db_table.key = object_string(table, &["table_key", "tableKey", "key"]);
            db_table.database =
                object_string(table, &["database", "databaseName", "database_name"]);
            tables.push(db_table);
        } else if let Some(name) = table.as_str() {
            tables.push(empty_db_table(None, name.to_string()));
        }
    }

    for column in value_items(column_json) {
        let Some(table_name) = object_string(column, &["tableName", "table_name", "table"]) else {
            continue;
        };
        let column_schema = object_string(column, &["schema", "schemaName", "schema_name"]);
        let Some(name) = object_string(column, &["columnName", "column_name", "column", "name"])
        else {
            continue;
        };
        let db_column = DbInventoryColumn {
            key: object_string(column, &["column_key", "columnKey", "key"]),
            table_key: object_string(column, &["table_key", "tableKey"]),
            name,
            data_type: object_string(column, &["dataType", "data_type", "type"]),
            nullable: object_nullable_bool(column, &["nullable", "isNullable", "is_nullable"]),
            is_primary_key: object_bool(
                column,
                &["primaryKey", "isPrimaryKey", "is_primary_key", "pk"],
            ),
            is_foreign_key: object_bool(
                column,
                &["foreignKey", "isForeignKey", "is_foreign_key", "fk"],
            ),
        };

        let table_key = db_column.table_key.as_deref();
        if let Some(index) =
            db_table_index_by_identity(&tables, table_key, column_schema.as_deref(), &table_name)
        {
            tables[index].columns.push(db_column);
        } else {
            let mut table = empty_db_table(column_schema, table_name);
            table.key = db_column.table_key.clone();
            table.database = object_string(column, &["database", "databaseName", "database_name"]);
            table.columns.push(db_column);
            tables.push(table);
        }
    }

    DbInventory {
        profile_id,
        tables,
        snapshot_key: None,
        contract_version: None,
        capability_warnings: Vec::new(),
        limit_requested: None,
        limit_applied: None,
        limit_clamped: None,
        result_count: None,
        total_tables: None,
        truncated: None,
        gaps: Vec::new(),
    }
}

pub(crate) fn extract_bulk_db_inventory(
    profile_id: String,
    value: &serde_json::Value,
) -> Result<DbInventory, String> {
    let table_values = value
        .get("tables")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "inventory tables 배열이 없습니다.".to_string())?;
    let mut inventory = DbInventory {
        profile_id,
        tables: Vec::with_capacity(table_values.len()),
        snapshot_key: object_string(value, &["snapshot_key", "snapshotKey"]),
        contract_version: json_scalar_string(
            value
                .get("contract_version")
                .or_else(|| value.get("contractVersion")),
        ),
        capability_warnings: string_array(value, &["capability_warnings", "capabilityWarnings"]),
        limit_requested: object_usize(value, &["limit_requested", "limitRequested"]),
        limit_applied: object_usize(value, &["limit_applied", "limitApplied"]),
        limit_clamped: object_optional_bool(value, &["limit_clamped", "limitClamped"]),
        result_count: object_usize(value, &["result_count", "resultCount"]),
        total_tables: object_usize(value, &["total_tables", "totalTables"]),
        truncated: object_optional_bool(value, &["truncated"]),
        gaps: Vec::new(),
    };

    for description in table_values {
        let table_key = object_string(description, &["table_key", "tableKey", "key"]);
        let key_parts = table_key.as_deref().and_then(stable_object_key_parts);
        let first_column = description
            .get("columns")
            .and_then(serde_json::Value::as_array)
            .and_then(|columns| columns.first());
        let name = object_string(description, &["table", "table_name", "tableName", "name"])
            .or_else(|| key_parts.as_ref().map(|parts| parts.object_name.clone()))
            .ok_or_else(|| "inventory table 이름이 없습니다.".to_string())?;
        let schema = object_string(description, &["schema", "schemaName", "schema_name"])
            .or_else(|| {
                first_column.and_then(|column| {
                    object_string(column, &["schema", "schemaName", "schema_name"])
                })
            })
            .or_else(|| key_parts.as_ref().map(|parts| parts.schema.clone()));
        let database = object_string(description, &["database", "databaseName", "database_name"])
            .or_else(|| {
                first_column.and_then(|column| {
                    object_string(column, &["database", "databaseName", "database_name"])
                })
            })
            .or_else(|| key_parts.as_ref().map(|parts| parts.database.clone()));
        let table_ref = table_key.as_deref().unwrap_or(&name).to_string();
        apply_inventory_description_metadata(&mut inventory, &table_ref, description);
        let mut table = empty_db_table(schema, name);
        table.key = table_key;
        table.database = database;
        apply_table_description(&mut table, description);
        inventory.tables.push(table);
    }

    if inventory.result_count != Some(inventory.tables.len()) {
        inventory.gaps.push(inventory_gap(
            "db-inventory-result-count",
            "inventory result_count와 실제 tables 수가 달라 실제 tables만 보존했습니다.",
        ));
    }
    let omitted = inventory
        .total_tables
        .is_some_and(|total| total > inventory.tables.len());
    if inventory.truncated == Some(true) || omitted {
        inventory.gaps.push(inventory_gap(
            "db-inventory-truncated",
            "DB inventory 안전 한도로 일부 테이블이 생략되어 누락 영역은 알 수 없음입니다.",
        ));
    }
    if inventory.limit_clamped == Some(true) {
        inventory.gaps.push(inventory_gap(
            "db-inventory-limit-clamped",
            "요청한 DB inventory 한도가 엔진 안전 한도로 조정되었습니다.",
        ));
    }

    Ok(inventory)
}

pub(crate) fn record_bulk_fallback_gap(inventory: &mut DbInventory) {
    inventory.gaps.push(inventory_gap(
        "db-inventory-bulk-unavailable",
        "bulk inventory를 사용할 수 없어 legacy find+describe를 사용했습니다. 대규모 DB에서는 일부 테이블 메타데이터가 알 수 없음일 수 있습니다.",
    ));
}

fn empty_db_table(schema: Option<String>, name: String) -> DbInventoryTable {
    DbInventoryTable {
        key: None,
        database: None,
        schema,
        name,
        columns: Vec::new(),
        foreign_keys: Vec::new(),
        inbound_foreign_keys: Vec::new(),
        constraints: Vec::new(),
        indexes: Vec::new(),
    }
}

fn db_table_index_by_identity(
    tables: &[DbInventoryTable],
    stable_key: Option<&str>,
    schema: Option<&str>,
    name: &str,
) -> Option<usize> {
    stable_key
        .and_then(|key| {
            tables
                .iter()
                .position(|table| table.key.as_deref() == Some(key))
        })
        .or_else(|| db_table_index(tables, schema, name))
}

fn db_table_index(tables: &[DbInventoryTable], schema: Option<&str>, name: &str) -> Option<usize> {
    let mut matches = tables
        .iter()
        .enumerate()
        .filter(|(_, table)| {
            table.name == name
                && match schema {
                    Some(schema) => table.schema.as_deref() == Some(schema),
                    None => true,
                }
        })
        .map(|(index, _)| index);
    let first = matches.next()?;

    if schema.is_some() || matches.next().is_none() {
        Some(first)
    } else {
        None
    }
}

pub(crate) fn db_index_args(
    profile: &DbProfile,
    cache_path: &Path,
    connection_string: Option<&str>,
) -> Result<Vec<String>, String> {
    let source = db_cli_source(profile)?;
    let mut args = vec![
        "index".to_string(),
        "--format".to_string(),
        "json".to_string(),
        "--source".to_string(),
        source.to_string(),
    ];

    if db_source_uses_path(&profile.source) {
        let path = profile
            .path
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "DB 경로가 필요합니다".to_string())?;
        args.extend(["--path".to_string(), path.to_string()]);
    } else {
        let connection_string = connection_string
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("{source} 연결에는 DB 연결 문자열이 필요합니다"))?;
        args.extend([
            "--connection-string".to_string(),
            connection_string.to_string(),
        ]);
    }

    args.extend([
        "--alias".to_string(),
        profile.id.clone(),
        "--cache-path".to_string(),
        cache_path.display().to_string(),
    ]);

    Ok(args)
}

fn db_source_uses_path(source: &DbSource) -> bool {
    matches!(source, DbSource::Sqlite | DbSource::DdlSqlite)
}

pub(crate) fn db_find_args(
    profile: &DbProfile,
    command: &str,
    cache_path: &Path,
) -> Result<Vec<String>, String> {
    Ok(vec![
        command.to_string(),
        db_snapshot_alias(profile)?,
        String::new(),
        "--format".to_string(),
        "json".to_string(),
        "--cache-path".to_string(),
        cache_path.display().to_string(),
    ])
}

fn db_find_legacy_args(
    profile: &DbProfile,
    command: &str,
    cache_path: &Path,
) -> Result<Vec<String>, String> {
    Ok(vec![
        command.to_string(),
        db_snapshot_alias(profile)?,
        String::new(),
        "--cache-path".to_string(),
        cache_path.display().to_string(),
    ])
}

pub(crate) fn db_inventory_args(
    profile: &DbProfile,
    cache_path: &Path,
) -> Result<Vec<String>, String> {
    Ok(vec![
        "inventory".to_string(),
        db_snapshot_alias(profile)?,
        "--limit".to_string(),
        DB_INVENTORY_LIMIT.to_string(),
        "--format".to_string(),
        "json".to_string(),
        "--cache-path".to_string(),
        cache_path.display().to_string(),
    ])
}

pub(crate) fn db_describe_table_args(
    profile: &DbProfile,
    table_ref: &str,
    cache_path: &Path,
) -> Result<Vec<String>, String> {
    let mut args = vec!["describe-table".to_string(), db_snapshot_alias(profile)?];
    if stable_object_key_parts(table_ref).is_some() {
        args.extend(["--object-key".to_string(), table_ref.to_string()]);
    } else {
        args.push(table_ref.to_string());
    }
    args.extend([
        "--format".to_string(),
        "json".to_string(),
        "--cache-path".to_string(),
        cache_path.display().to_string(),
    ]);
    Ok(args)
}

pub(crate) fn merge_db_inventory_lines(
    inventory: &mut DbInventory,
    table_stdout: &str,
    column_stdout: &str,
) {
    for line in table_stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let (schema, name) = db_ref_schema_name(line);
        if db_table_index(&inventory.tables, schema.as_deref(), &name).is_none() {
            inventory.tables.push(empty_db_table(schema, name));
        }
    }

    for line in column_stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Some((table_ref, column_name)) = line.rsplit_once('.') else {
            continue;
        };
        let (schema, table_name) = db_ref_schema_name(table_ref);
        let column = DbInventoryColumn {
            key: None,
            table_key: None,
            name: column_name.to_string(),
            data_type: None,
            nullable: None,
            is_primary_key: false,
            is_foreign_key: false,
        };

        if let Some(index) = db_table_index(&inventory.tables, schema.as_deref(), &table_name) {
            if !inventory.tables[index]
                .columns
                .iter()
                .any(|item| item.name == column.name)
            {
                inventory.tables[index].columns.push(column);
            }
        } else {
            let mut table = empty_db_table(schema, table_name);
            table.columns.push(column);
            inventory.tables.push(table);
        }
    }
}

fn enrich_db_inventory_with_describe(
    inventory: &mut DbInventory,
    db_engine: &engine::EngineAvailability,
    profile: &DbProfile,
    cache_path: &Path,
) {
    let (targets, limit_gaps) = db_describe_plan(inventory);
    inventory.gaps.extend(limit_gaps);

    for (index, table_ref) in targets {
        match describe_table(db_engine, profile, cache_path, &table_ref) {
            Ok(description) => {
                apply_inventory_description_metadata(inventory, &table_ref, &description);
                apply_table_description(&mut inventory.tables[index], &description);
            }
            Err(message) => inventory
                .gaps
                .push(db_gap("db-describe-failure", &table_ref, message)),
        }
    }

    finalize_db_inventory(inventory);
}

fn finalize_db_inventory(inventory: &mut DbInventory) {
    inventory.capability_warnings.sort();
    inventory.capability_warnings.dedup();
    record_db_identity_gaps(inventory);
    inventory.gaps.sort_by(|left, right| left.id.cmp(&right.id));
    inventory.gaps.dedup_by(|left, right| left.id == right.id);
}

pub(crate) fn record_db_identity_gaps(inventory: &mut DbInventory) {
    let mut product_keys = BTreeMap::<String, usize>::new();
    let mut engine_keys = BTreeMap::<String, usize>::new();
    for table in &inventory.tables {
        *product_keys
            .entry(db_table_key(table.schema.as_deref(), &table.name))
            .or_default() += 1;
        if let Some(key) = table.key.as_ref() {
            *engine_keys.entry(key.clone()).or_default() += 1;
        } else if inventory.contract_version.is_some() {
            inventory.gaps.push(db_gap(
                "db-table-identity-missing",
                &db_table_key(table.schema.as_deref(), &table.name),
                "버전된 DB 계약이 stable table key를 반환하지 않아 table identity를 확인할 수 없습니다.",
            ));
        }
    }
    for (table_key, count) in product_keys {
        if count > 1 {
            inventory.gaps.push(db_gap(
                "db-table-identity-ambiguous",
                &table_key,
                "같은 schema/name table이 여러 번 발견되어 임의로 병합하지 않았습니다.",
            ));
        }
    }
    for (engine_key, count) in engine_keys {
        if count > 1 {
            inventory.gaps.push(db_gap(
                "db-table-key-collision",
                &engine_key,
                "같은 stable table key가 여러 table 설명에서 반복되어 identity를 확인할 수 없습니다.",
            ));
        }
    }
}

pub(crate) fn db_describe_plan(
    inventory: &DbInventory,
) -> (Vec<(usize, String)>, Vec<DbInventoryGap>) {
    let targets = inventory
        .tables
        .iter()
        .enumerate()
        .map(|(index, table)| {
            (
                index,
                table
                    .key
                    .clone()
                    .unwrap_or_else(|| db_table_key(table.schema.as_deref(), &table.name)),
            )
        })
        .collect::<Vec<_>>();
    let gaps = targets
        .iter()
        .skip(MAX_FALLBACK_DESCRIBED_TABLES)
        .map(|(_, table_ref)| {
            db_gap(
                "db-describe-limit",
                table_ref,
                "안전 한도를 넘어 describe-table 메타데이터를 읽지 못했습니다.",
            )
        })
        .collect();
    (
        targets
            .into_iter()
            .take(MAX_FALLBACK_DESCRIBED_TABLES)
            .collect(),
        gaps,
    )
}

fn describe_table(
    db_engine: &engine::EngineAvailability,
    profile: &DbProfile,
    cache_path: &Path,
    table_ref: &str,
) -> Result<serde_json::Value, &'static str> {
    let args = db_describe_table_args(profile, table_ref, cache_path)
        .map_err(|_| "describe-table 인자를 만들지 못했습니다.")?;
    let run = engine::run_engine_command(db_engine, &args, Duration::from_secs(30))
        .map_err(|_| "describe-table 실행에 실패했습니다.")?;
    if !run.ok {
        return Err("describe-table가 메타데이터를 반환하지 못했습니다.");
    }
    engine_json_value(&run.stdout).ok_or("describe-table JSON을 해석하지 못했습니다.")
}

pub(crate) fn apply_inventory_description_metadata(
    inventory: &mut DbInventory,
    table_ref: &str,
    description: &serde_json::Value,
) {
    if let Some(snapshot_key) = object_string(description, &["snapshot_key", "snapshotKey"]) {
        if inventory
            .snapshot_key
            .as_ref()
            .is_some_and(|existing| existing != &snapshot_key)
        {
            inventory.gaps.push(db_gap(
                "db-contract-mismatch",
                table_ref,
                "테이블 설명의 snapshot key가 서로 달라 다시 읽어야 합니다.",
            ));
        } else {
            inventory.snapshot_key = Some(snapshot_key);
        }
    }

    if let Some(contract_version) = json_scalar_string(
        description
            .get("contract_version")
            .or_else(|| description.get("contractVersion")),
    ) {
        if inventory
            .contract_version
            .as_ref()
            .is_some_and(|existing| existing != &contract_version)
        {
            inventory.gaps.push(db_gap(
                "db-contract-mismatch",
                table_ref,
                "테이블 설명의 contract version이 서로 달라 다시 읽어야 합니다.",
            ));
        } else {
            inventory.contract_version = Some(contract_version);
        }
    }

    inventory.capability_warnings.extend(
        string_array(description, &["capability_warnings", "capabilityWarnings"])
            .into_iter()
            .filter(|warning| !warning.trim().is_empty()),
    );

    let complete_contract = description
        .get("contract_version")
        .or_else(|| description.get("contractVersion"))
        .is_some()
        && description
            .get("constraints")
            .is_some_and(serde_json::Value::is_array)
        && description
            .get("indexes")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|indexes| {
                indexes.iter().all(|index| {
                    index.get("predicate").is_some() && index.get("expression").is_some()
                })
            });
    if !complete_contract {
        inventory.gaps.push(db_gap(
            "db-contract-coverage",
            table_ref,
            "이 엔진 계약은 전체 unique/check constraint 및 index predicate/expression을 노출하지 않아 해당 항목은 알 수 없습니다.",
        ));
    }
}

pub(crate) fn apply_table_description(
    table: &mut DbInventoryTable,
    description: &serde_json::Value,
) {
    table.key =
        object_string(description, &["table_key", "tableKey", "key"]).or_else(|| table.key.clone());
    let mut primary_key = string_array(
        description,
        &["primary_key", "primaryKey", "primaryKeyColumns"],
    );
    let mut constraints = constraints_from_description(description);
    for constraint in &constraints {
        if constraint.kind == "primary_key" {
            primary_key.extend(constraint.columns.iter().cloned());
        }
    }
    primary_key.sort();
    primary_key.dedup();

    let outbound_keys = foreign_keys(description, "outbound", table);
    let inbound_foreign_keys = foreign_keys(description, "inbound", table);
    let mut foreign_key_columns = outbound_keys
        .iter()
        .flat_map(|foreign_key| foreign_key.columns.iter())
        .cloned()
        .collect::<HashSet<_>>();
    for constraint in &constraints {
        if constraint.kind == "foreign_key" {
            foreign_key_columns.extend(constraint.columns.iter().cloned());
        }
    }

    if !primary_key.is_empty()
        && !constraints
            .iter()
            .any(|constraint| constraint.kind == "primary_key")
    {
        constraints.push(DbConstraint {
            key: None,
            name: None,
            kind: "primary_key".to_string(),
            columns: primary_key.clone(),
            column_keys: Vec::new(),
            referenced_table_key: None,
            referenced_schema: None,
            referenced_table: None,
            referenced_columns: Vec::new(),
            referenced_column_keys: Vec::new(),
            expression: None,
            source: "primary_key".to_string(),
        });
    }
    for foreign_key in &outbound_keys {
        if !constraints.iter().any(|constraint| {
            constraint.kind == "foreign_key"
                && ((constraint.key.is_some() && constraint.key == foreign_key.key)
                    || (constraint.name == foreign_key.name
                        && constraint.columns == foreign_key.columns))
        }) {
            constraints.push(constraint_from_foreign_key(foreign_key));
        }
    }

    table.foreign_keys = outbound_keys;
    table.inbound_foreign_keys = inbound_foreign_keys;
    table.constraints = constraints;
    table.indexes = indexes_from_description(description);

    for column in description
        .get("columns")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(name) = object_string(column, &["name", "columnName", "column_name", "column"])
        else {
            continue;
        };
        let described = DbInventoryColumn {
            key: object_string(column, &["column_key", "columnKey", "key"]),
            table_key: object_string(column, &["table_key", "tableKey"])
                .or_else(|| table.key.clone()),
            data_type: object_string(column, &["type", "dataType", "data_type"]),
            nullable: object_nullable_bool(column, &["nullable", "isNullable", "is_nullable"]),
            is_primary_key: primary_key.iter().any(|key| key == &name),
            is_foreign_key: foreign_key_columns.contains(&name),
            name,
        };

        if let Some(existing) = table
            .columns
            .iter_mut()
            .find(|column| column.name == described.name)
        {
            if described.key.is_some() {
                existing.key = described.key;
            }
            if described.table_key.is_some() {
                existing.table_key = described.table_key;
            }
            if described.data_type.is_some() {
                existing.data_type = described.data_type;
            }
            if described.nullable.is_some() {
                existing.nullable = described.nullable;
            }
            existing.is_primary_key = described.is_primary_key;
            existing.is_foreign_key = described.is_foreign_key;
        } else {
            table.columns.push(described);
        }
    }
}

fn foreign_keys(
    description: &serde_json::Value,
    direction: &str,
    described_table: &DbInventoryTable,
) -> Vec<DbForeignKey> {
    let Some(foreign_keys) = description
        .get("foreign_keys")
        .or_else(|| description.get("foreignKeys"))
    else {
        return Vec::new();
    };
    let values = foreign_keys
        .get(direction)
        .or_else(|| {
            (direction == "outbound")
                .then(|| foreign_keys.get("outboundForeignKeys"))
                .flatten()
        })
        .or_else(|| {
            (direction == "inbound")
                .then(|| foreign_keys.get("inboundForeignKeys"))
                .flatten()
        })
        .and_then(serde_json::Value::as_array)
        .or_else(|| {
            (direction == "outbound")
                .then(|| foreign_keys.as_array())
                .flatten()
        });

    values
        .into_iter()
        .flatten()
        .filter_map(|value| foreign_key_from_value(value, described_table, direction))
        .collect()
}

fn foreign_key_from_value(
    value: &serde_json::Value,
    described_table: &DbInventoryTable,
    direction: &str,
) -> Option<DbForeignKey> {
    let key = object_string(value, &["key", "constraint_key", "constraintKey"]);
    let table_key = object_string(value, &["table_key", "tableKey"]);
    let referenced_table_key = object_string(
        value,
        &[
            "referenced_table_key",
            "referencedTableKey",
            "target_table_key",
            "targetTableKey",
        ],
    );
    let (table_key_schema, table_key_name) = table_key
        .as_deref()
        .and_then(object_key_schema_name)
        .map(|(schema, name)| (Some(schema), Some(name)))
        .unwrap_or_default();
    let (referenced_key_schema, referenced_key_name) = referenced_table_key
        .as_deref()
        .and_then(object_key_schema_name)
        .map(|(schema, name)| (Some(schema), Some(name)))
        .unwrap_or_default();
    let table = object_string(value, &["table", "table_name", "tableName"])
        .or(table_key_name)
        .or_else(|| (direction == "outbound").then(|| described_table.name.clone()));
    let referenced_table = object_string(
        value,
        &[
            "referenced_table",
            "referencedTable",
            "target_table",
            "targetTable",
        ],
    )
    .or(referenced_key_name)?;
    let columns = string_array(value, &["columns", "column_names", "columnNames"]);
    let column_keys = string_array(value, &["column_keys", "columnKeys"]);
    let referenced_columns = string_array(
        value,
        &[
            "referenced_columns",
            "referencedColumns",
            "target_columns",
            "targetColumns",
        ],
    );
    let referenced_column_keys =
        string_array(value, &["referenced_column_keys", "referencedColumnKeys"]);
    if columns.is_empty() || referenced_columns.is_empty() {
        return None;
    }

    Some(DbForeignKey {
        key,
        name: object_string(value, &["name", "constraint_name", "constraintName"]),
        table_key,
        table_schema: object_string(value, &["table_schema", "tableSchema"])
            .or(table_key_schema)
            .or_else(|| {
                (direction == "outbound")
                    .then(|| described_table.schema.clone())
                    .flatten()
            }),
        table,
        columns,
        column_keys,
        referenced_table_key,
        referenced_schema: object_string(
            value,
            &[
                "referenced_schema",
                "referencedSchema",
                "target_schema",
                "targetSchema",
            ],
        )
        .or(referenced_key_schema),
        referenced_table,
        referenced_columns,
        referenced_column_keys,
    })
}

fn constraints_from_description(description: &serde_json::Value) -> Vec<DbConstraint> {
    description
        .get("constraints")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| {
            let kind = object_string(value, &["kind", "constraint_kind", "constraintKind"])?;
            let referenced_table_key =
                object_string(value, &["referenced_table_key", "referencedTableKey"]);
            let (referenced_schema, referenced_table) = referenced_table_key
                .as_deref()
                .and_then(object_key_schema_name)
                .map(|(schema, table)| (Some(schema), Some(table)))
                .unwrap_or_default();
            Some(DbConstraint {
                key: object_string(value, &["key", "constraint_key", "constraintKey"]),
                name: object_string(value, &["name", "constraint_name", "constraintName"]),
                kind: normalize_constraint_kind(&kind),
                columns: string_array(value, &["columns", "column_names", "columnNames"]),
                column_keys: string_array(value, &["column_keys", "columnKeys"]),
                referenced_table_key,
                referenced_schema: object_string(value, &["referenced_schema", "referencedSchema"])
                    .or(referenced_schema),
                referenced_table: object_string(value, &["referenced_table", "referencedTable"])
                    .or(referenced_table),
                referenced_columns: string_array(
                    value,
                    &["referenced_columns", "referencedColumns"],
                ),
                referenced_column_keys: string_array(
                    value,
                    &["referenced_column_keys", "referencedColumnKeys"],
                ),
                expression: object_string(value, &["expression", "definition"]),
                source: "constraints".to_string(),
            })
        })
        .collect()
}

fn constraint_from_foreign_key(foreign_key: &DbForeignKey) -> DbConstraint {
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
        source: "foreign_keys.outbound".to_string(),
    }
}

fn indexes_from_description(description: &serde_json::Value) -> Vec<DbIndex> {
    description
        .get("indexes")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| {
            let key = object_string(value, &["key", "index_key", "indexKey"]);
            let name = object_string(value, &["name", "index_name", "indexName"])
                .or_else(|| key.clone())?;
            Some(DbIndex {
                key,
                name,
                columns: string_array(value, &["columns", "column_names", "columnNames"]),
                column_keys: string_array(value, &["column_keys", "columnKeys"]),
                unique: object_bool(value, &["unique", "is_unique", "isUnique"]),
                primary: object_bool(value, &["primary", "is_primary", "isPrimary"]),
                predicate: object_string(value, &["predicate", "where"]),
                expression: object_string(value, &["expression"]),
            })
        })
        .collect()
}

fn normalize_constraint_kind(kind: &str) -> String {
    match kind
        .trim()
        .to_ascii_lowercase()
        .replace(['-', ' '], "_")
        .as_str()
    {
        "pk" | "primary" | "primarykey" | "primary_key" => "primary_key".to_string(),
        "fk" | "foreign" | "foreignkey" | "foreign_key" => "foreign_key".to_string(),
        "unique_constraint" => "unique".to_string(),
        "check_constraint" => "check".to_string(),
        kind => kind.to_string(),
    }
}

fn object_key_schema_name(value: &str) -> Option<(String, String)> {
    let parts = value.split(':').collect::<Vec<_>>();
    ((parts.len() == 6 || parts.len() == 7) && parts[4] == "table")
        .then(|| (parts[3].to_string(), parts[5].to_string()))
}

fn json_scalar_string(value: Option<&serde_json::Value>) -> Option<String> {
    value.and_then(|value| {
        value
            .as_str()
            .map(str::to_string)
            .or_else(|| value.as_u64().map(|value| value.to_string()))
    })
}

fn object_usize(value: &serde_json::Value, keys: &[&str]) -> Option<usize> {
    keys.iter()
        .find_map(|key| value.get(key).and_then(serde_json::Value::as_u64))
        .and_then(|value| usize::try_from(value).ok())
}

fn object_optional_bool(value: &serde_json::Value, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| value.get(key).and_then(serde_json::Value::as_bool))
}

#[derive(Debug)]
struct StableObjectKeyParts {
    database: String,
    schema: String,
    object_name: String,
}

fn stable_object_key_parts(value: &str) -> Option<StableObjectKeyParts> {
    let parts = value.split(':').collect::<Vec<_>>();
    if !(parts.len() == 6 || parts.len() == 7) || parts.iter().any(|part| part.is_empty()) {
        return None;
    }
    Some(StableObjectKeyParts {
        database: parts[2].to_string(),
        schema: parts[3].to_string(),
        object_name: parts[5].to_string(),
    })
}

fn inventory_gap(kind: &str, message: &str) -> DbInventoryGap {
    DbInventoryGap {
        id: kind.to_string(),
        kind: kind.to_string(),
        message: message.to_string(),
        table_key: None,
    }
}

fn db_gap(kind: &str, table_key: &str, message: &str) -> DbInventoryGap {
    DbInventoryGap {
        id: format!("{kind}:{table_key}"),
        kind: kind.to_string(),
        message: message.to_string(),
        table_key: Some(table_key.to_string()),
    }
}

fn string_array(value: &serde_json::Value, keys: &[&str]) -> Vec<String> {
    keys.iter()
        .find_map(|key| value.get(key).and_then(serde_json::Value::as_array))
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn object_nullable_bool(value: &serde_json::Value, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| value.get(key))
        .and_then(|value| {
            value.as_bool().or_else(|| {
                value
                    .as_str()
                    .and_then(|value| match value.to_ascii_lowercase().as_str() {
                        "yes" | "true" | "nullable" => Some(true),
                        "no" | "false" | "not null" => Some(false),
                        _ => None,
                    })
            })
        })
}

fn db_ref_schema_name(value: &str) -> (Option<String>, String) {
    match value.rsplit_once('.') {
        Some((schema, name)) if !schema.is_empty() && !name.is_empty() => {
            (Some(schema.to_string()), name.to_string())
        }
        _ => (None, value.to_string()),
    }
}

fn db_table_key(schema: Option<&str>, name: &str) -> String {
    match schema.filter(|value| !value.is_empty()) {
        Some(schema) => format!("{schema}.{name}"),
        None => name.to_string(),
    }
}

pub(crate) fn db_cache_path(
    workspaces_dir: &Path,
    workspace_id: &str,
    profile_id: &str,
) -> PathBuf {
    workspace_db_cache_dir(workspaces_dir, workspace_id)
        .join(profile_id)
        .join("graph.sqlite")
}

fn db_snapshot_alias(profile: &DbProfile) -> Result<String, String> {
    Ok(format!("{}:{}", db_cli_source(profile)?, profile.id))
}

fn db_cli_source(profile: &DbProfile) -> Result<&'static str, String> {
    match profile.source {
        DbSource::Sqlite => Ok("sqlite"),
        DbSource::DdlSqlite => Ok("ddl-sqlite"),
        DbSource::Postgres => Ok("postgres"),
        DbSource::Mysql => Ok("mysql"),
        DbSource::Sqlserver => Ok("sqlserver"),
        DbSource::Oracle => Ok("oracle"),
    }
}
