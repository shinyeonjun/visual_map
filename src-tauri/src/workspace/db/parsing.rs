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
