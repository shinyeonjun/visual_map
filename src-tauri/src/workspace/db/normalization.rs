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
