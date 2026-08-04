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
