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
        language: None,
        role_basis: None,
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
        .join("inventory-snapshot.json.zip")
}

pub(crate) fn snapshot_backup_path(path: &Path) -> PathBuf {
    path.with_file_name("inventory-snapshot.backup.json.zip")
}

pub(crate) fn legacy_snapshot_path(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
) -> PathBuf {
    base_paths(app_data_dir)
        .workspaces_dir
        .join(workspace_id)
        .join("atlas")
        .join("inventory-snapshot.json")
}

fn legacy_snapshot_backup_path(path: &Path) -> PathBuf {
    path.with_file_name("inventory-snapshot.backup.json")
}

pub(crate) fn timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}
mod normalization;

use normalization::*;
