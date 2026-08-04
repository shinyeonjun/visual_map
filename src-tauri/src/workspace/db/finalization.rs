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
