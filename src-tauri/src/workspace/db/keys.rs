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
