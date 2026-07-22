use crate::paths::base_paths;
use crate::EngineRegistry;
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use super::database_memory::DatabaseMemoryAdapter;
use super::model::{
    DbConstraint, DbDependentObject, DbForeignKey, DbIndex, DbIndexResult, DbInventory,
    DbInventoryColumn, DbInventoryGap, DbInventoryTable, DbProfile, DbSource,
    IndexDbProfileRequest, SaveDbProfileRequest, Workspace,
};
#[cfg(test)]
use super::store::value_items;
use super::store::{
    engine_json_value, object_bool, object_string, read_workspace_by_id, timestamp,
    validate_workspace_id, workspace_db_cache_dir, workspace_id, write_workspace,
};

const DB_INVENTORY_PAGE_LIMIT: usize = 1_000;
const MAX_DB_INVENTORY_TABLES: usize = 20_000;
pub(crate) fn save_db_profile(
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

pub(crate) fn delete_db_profile(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
    profile_id: &str,
) -> Result<Workspace, String> {
    validate_workspace_id(workspace_id)?;
    validate_workspace_id(profile_id)?;
    let paths = base_paths(app_data_dir);
    let mut workspace = read_workspace_by_id(&paths.workspaces_dir, workspace_id)?;
    let profile_index = workspace
        .db_profiles
        .iter()
        .position(|profile| profile.id == profile_id)
        .ok_or_else(|| "삭제할 DB 연결을 찾을 수 없습니다".to_string())?;
    let cache_dir = db_cache_path(&paths.workspaces_dir, workspace_id, profile_id)
        .parent()
        .map(Path::to_path_buf);

    if let Some(cache_dir) = cache_dir.filter(|path| path.is_dir()) {
        fs::remove_dir_all(cache_dir)
            .map_err(|error| format!("DB 연결 캐시를 삭제하지 못했습니다: {error}"))?;
    }
    workspace.db_profiles.remove(profile_index);
    workspace.active_db_profile_id = workspace
        .db_profiles
        .first()
        .map(|profile| profile.id.clone());
    workspace.updated_at = timestamp();
    write_workspace(&paths.workspaces_dir, &workspace)?;
    Ok(workspace)
}

pub(crate) fn index_db_profile(
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
    let adapter = DatabaseMemoryAdapter::new(registry)?;
    let snapshot_key = db_snapshot_alias(&profile)?;

    let run = if db_source_uses_path(&profile.source) {
        adapter.index(&args, &[], &snapshot_key)?
    } else {
        let connection_string = request
            .connection_string
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "DB 연결 문자열이 필요합니다".to_string())?;
        let config_path = db_connection_config_path(&cache_path);
        write_db_connection_config(&profile, &config_path)?;
        let env_name = db_connection_env_var(&profile.id);
        let result = adapter.index(
            &args,
            &[(env_name.as_str(), connection_string)],
            &snapshot_key,
        );
        let _ = fs::remove_file(config_path);
        result?
    };

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
        inventory: None,
        inventory_error: None,
    })
}

pub(crate) fn db_inventory(
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
    let adapter = DatabaseMemoryAdapter::new(registry)?;
    read_complete_db_inventory(&adapter, profile, &cache_path, selected_profile_id)
}

fn read_complete_db_inventory(
    adapter: &DatabaseMemoryAdapter<'_>,
    profile: &DbProfile,
    cache_path: &Path,
    profile_id: String,
) -> Result<DbInventory, String> {
    let snapshot_key = db_snapshot_alias(profile)?;
    adapter.verify_complete_snapshot(&snapshot_key, cache_path)?;
    let mut offset = 0;
    let mut inventory: Option<DbInventory> = None;
    let mut table_keys = HashSet::new();
    let mut column_keys = HashSet::new();

    loop {
        let value =
            adapter.inventory_page(&snapshot_key, cache_path, offset, DB_INVENTORY_PAGE_LIMIT)?;
        let page = parse_bulk_db_inventory(profile_id.clone(), &value)?;
        validate_complete_inventory_page(&page, &mut table_keys, &mut column_keys)?;

        if let Some(existing) = inventory.as_ref() {
            if existing.snapshot_key != page.snapshot_key
                || existing.contract_version != page.contract_version
                || existing.total_tables != page.total_tables
            {
                return Err(
                    "DB inventory 페이지의 snapshot, 계약 또는 전체 테이블 수가 일치하지 않습니다"
                        .to_string(),
                );
            }
        }

        let page_count = page.tables.len();
        let has_more = value
            .get("has_more")
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| "DB inventory has_more 값이 없습니다".to_string())?;
        let next_offset = value
            .get("next_offset")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| usize::try_from(value).ok());
        if let Some(existing) = inventory.as_mut() {
            existing.tables.extend(page.tables);
            existing
                .capability_warnings
                .extend(page.capability_warnings);
            existing.gaps.extend(page.gaps);
        } else {
            inventory = Some(page);
        }

        let total_tables = inventory
            .as_ref()
            .and_then(|inventory| inventory.total_tables)
            .ok_or_else(|| "DB inventory total_tables 값이 없습니다".to_string())?;
        if total_tables > MAX_DB_INVENTORY_TABLES {
            return Err(format!(
                "DB 테이블 수가 제품 안전 한도 {MAX_DB_INVENTORY_TABLES}개를 초과했습니다: {total_tables}개"
            ));
        }
        if !has_more {
            let mut inventory =
                inventory.ok_or_else(|| "DB inventory가 비어 있습니다".to_string())?;
            if inventory.tables.len() != total_tables {
                return Err(format!(
                    "DB inventory가 완전하지 않습니다: expected {total_tables} tables, got {}",
                    inventory.tables.len()
                ));
            }
            inventory.result_count = Some(inventory.tables.len());
            inventory.truncated = Some(false);
            finalize_db_inventory(&mut inventory);
            if let Some(gap) = inventory.gaps.first() {
                return Err(format!(
                    "DB inventory 계약 검증에 실패했습니다: {}",
                    gap.message
                ));
            }
            return Ok(inventory);
        }
        let next_offset =
            next_offset.ok_or_else(|| "DB inventory 다음 페이지 offset이 없습니다".to_string())?;
        if page_count == 0 || next_offset != offset.saturating_add(page_count) {
            return Err("DB inventory 페이지 offset이 연속적이지 않습니다".to_string());
        }
        offset = next_offset;
    }
}

#[cfg(test)]
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

#[cfg(test)]
pub(crate) fn extract_bulk_db_inventory(
    profile_id: String,
    value: &serde_json::Value,
) -> Result<DbInventory, String> {
    let mut inventory = parse_bulk_db_inventory(profile_id, value)?;
    record_bulk_completion_gaps(&mut inventory);
    Ok(inventory)
}

fn parse_bulk_db_inventory(
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
    if inventory.limit_clamped == Some(true) {
        inventory.gaps.push(inventory_gap(
            "db-inventory-limit-clamped",
            "요청한 DB inventory 한도가 엔진 안전 한도로 조정되었습니다.",
        ));
    }

    Ok(inventory)
}

fn validate_complete_inventory_page(
    page: &DbInventory,
    table_keys: &mut HashSet<String>,
    column_keys: &mut HashSet<String>,
) -> Result<(), String> {
    if page.contract_version.as_deref() != Some("2") {
        return Err("DB inventory가 contract v2 응답이 아닙니다".to_string());
    }
    if page.snapshot_key.as_deref().is_none() {
        return Err("DB inventory snapshot key가 없습니다".to_string());
    }
    if let Some(gap) = page.gaps.first() {
        return Err(format!(
            "DB inventory 계약 검증에 실패했습니다: {}",
            gap.message
        ));
    }

    for table in &page.tables {
        let table_key = table
            .key
            .as_ref()
            .ok_or_else(|| format!("DB 테이블 {}의 stable key가 없습니다", table.name))?;
        if stable_object_key_parts(table_key).is_none_or(|parts| parts.object_kind != "table") {
            return Err(format!(
                "DB 테이블 stable key가 올바르지 않습니다: {table_key}"
            ));
        }
        if !table_keys.insert(table_key.clone()) {
            return Err(format!("DB 테이블 stable key가 중복됩니다: {table_key}"));
        }

        for column in &table.columns {
            let column_key = column.key.as_ref().ok_or_else(|| {
                format!(
                    "DB 컬럼 {}.{}의 stable key가 없습니다",
                    table.name, column.name
                )
            })?;
            if stable_object_key_parts(column_key).is_none_or(|parts| parts.object_kind != "column")
            {
                return Err(format!(
                    "DB 컬럼 stable key가 올바르지 않습니다: {column_key}"
                ));
            }
            if column.table_key.as_deref() != Some(table_key.as_str()) {
                return Err(format!(
                    "DB 컬럼 {}의 table key가 상위 테이블과 일치하지 않습니다",
                    column.name
                ));
            }
            if !column_keys.insert(column_key.clone()) {
                return Err(format!("DB 컬럼 stable key가 중복됩니다: {column_key}"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
fn record_bulk_completion_gaps(inventory: &mut DbInventory) {
    let omitted = inventory
        .total_tables
        .is_some_and(|total| total > inventory.tables.len());
    if inventory.truncated == Some(true) || omitted {
        inventory.gaps.push(inventory_gap(
            "db-inventory-truncated",
            "DB inventory 안전 한도로 일부 테이블이 생략되어 누락 영역은 알 수 없음입니다.",
        ));
    }
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
        dependents: Vec::new(),
    }
}

#[cfg(test)]
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

#[cfg(test)]
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
        connection_string
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("{source} 연결에는 DB 연결 문자열이 필요합니다"))?;
        args.extend([
            "--config-path".to_string(),
            db_connection_config_path(cache_path).display().to_string(),
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

pub(crate) fn db_connection_config_path(cache_path: &Path) -> PathBuf {
    cache_path.with_file_name("database-memory-profile.toml")
}

pub(crate) fn db_connection_env_var(alias: &str) -> String {
    let alias = alias
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("DATABASE_MEMORY_{alias}_CONNECTION_STRING")
}

fn write_db_connection_config(profile: &DbProfile, path: &Path) -> Result<(), String> {
    let source = db_cli_source(profile)?;
    let contents = format!("[{}]\nsource = \"{}\"\n", profile.id, source);
    fs::write(path, contents)
        .map_err(|error| format!("DB 연결 준비 파일을 만들지 못했습니다: {error}"))
}

fn db_source_uses_path(source: &DbSource) -> bool {
    matches!(source, DbSource::Sqlite | DbSource::DdlSqlite)
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
    table.dependents = dependents_from_description(description);

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

fn dependents_from_description(description: &serde_json::Value) -> Vec<DbDependentObject> {
    let mut dependents = description
        .get("dependents")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| {
            let key = object_string(value, &["key", "object_key", "objectKey"])?;
            let kind = object_string(value, &["kind"])?;
            let name = object_string(value, &["name"])?;
            let relation = object_string(value, &["relation"])?;
            matches!(kind.as_str(), "view" | "trigger" | "routine").then(|| {
                let mut column_keys = string_array(value, &["column_keys", "columnKeys"]);
                column_keys.sort();
                column_keys.dedup();
                DbDependentObject {
                    key,
                    kind,
                    name,
                    relation,
                    column_keys,
                }
            })
        })
        .collect::<Vec<_>>();
    dependents.sort_by(|left, right| {
        left.key
            .cmp(&right.key)
            .then_with(|| left.relation.cmp(&right.relation))
    });
    let mut merged: Vec<DbDependentObject> = Vec::with_capacity(dependents.len());
    for dependent in dependents {
        if let Some(existing) = merged.last_mut().filter(|existing| {
            existing.key == dependent.key && existing.relation == dependent.relation
        }) {
            existing.column_keys.extend(dependent.column_keys);
            existing.column_keys.sort();
            existing.column_keys.dedup();
        } else {
            merged.push(dependent);
        }
    }
    merged
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
    let parts = stable_object_key_parts(value)?;
    (parts.object_kind == "table").then_some((parts.schema, parts.object_name))
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
    object_kind: String,
    object_name: String,
}

fn stable_object_key_parts(value: &str) -> Option<StableObjectKeyParts> {
    let (value, encoded) = value
        .strip_prefix("v2:")
        .map_or((value, false), |value| (value, true));
    let raw_parts = value.split(':').collect::<Vec<_>>();
    if !(raw_parts.len() == 6 || raw_parts.len() == 7)
        || raw_parts.iter().any(|part| part.is_empty())
    {
        return None;
    }
    let parts = raw_parts
        .into_iter()
        .map(|part| {
            if encoded {
                decode_stable_object_key_part(part)
            } else {
                Some(part.to_string())
            }
        })
        .collect::<Option<Vec<_>>>()?;
    if !matches!(
        parts[4].as_str(),
        "database"
            | "schema"
            | "table"
            | "column"
            | "primary_key"
            | "foreign_key"
            | "unique_constraint"
            | "check_constraint"
            | "index"
            | "view"
            | "trigger"
            | "routine"
    ) {
        return None;
    }
    Some(StableObjectKeyParts {
        database: parts[2].clone(),
        schema: parts[3].clone(),
        object_kind: parts[4].clone(),
        object_name: parts[5].clone(),
    })
}

fn decode_stable_object_key_part(value: &str) -> Option<String> {
    let mut decoded = String::with_capacity(value.len());
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character != '%' {
            decoded.push(character);
            continue;
        }
        match (characters.next(), characters.next()) {
            (Some('2'), Some('5')) => decoded.push('%'),
            (Some('3'), Some('A' | 'a')) => decoded.push(':'),
            _ => return None,
        }
    }
    Some(decoded)
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
        DbSource::Yugabytedb => Ok("yugabytedb"),
        DbSource::Mysql => Ok("mysql"),
        DbSource::Mariadb => Ok("mariadb"),
        DbSource::Sqlserver => Ok("sqlserver"),
        DbSource::Oracle => Ok("oracle"),
    }
}
