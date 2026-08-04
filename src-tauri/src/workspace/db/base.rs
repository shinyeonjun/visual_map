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

    workspace.db_profiles.remove(profile_index);
    workspace.active_db_profile_id = workspace
        .db_profiles
        .first()
        .map(|profile| profile.id.clone());
    workspace.updated_at = timestamp();
    write_workspace(&paths.workspaces_dir, &workspace)?;
    // Metadata is the source of truth. A stale cache is harmless and can be
    // cleaned after the profile deletion is durable; deleting it earlier can
    // leave a live profile without its cache when the metadata write fails.
    if let Some(cache_dir) = cache_dir.filter(|path| path.is_dir()) {
        let _ = fs::remove_dir_all(cache_dir);
    }
    Ok(workspace)
}

#[cfg(test)]
pub(crate) fn index_db_profile(
    app_data_dir: impl AsRef<Path>,
    registry: &EngineRegistry,
    request: IndexDbProfileRequest,
) -> Result<DbIndexResult, String> {
    index_db_profile_with_persistence(app_data_dir, registry, request, true)
}

pub(crate) fn index_db_profile_without_persisting(
    app_data_dir: impl AsRef<Path>,
    registry: &EngineRegistry,
    request: IndexDbProfileRequest,
) -> Result<DbIndexResult, String> {
    index_db_profile_with_persistence(app_data_dir, registry, request, false)
}
