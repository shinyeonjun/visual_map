mod client_requests;
mod code;
mod codebase_memory;
mod database_memory;
mod db;
mod fastapi_routes;
mod fastendpoints_routes;
mod limits;
mod model;
mod provider_bundle;
mod store;

pub(crate) use code::focused_code_search_with_operation;
pub(crate) use code::{
    cleanup_code_project, cleanup_previous_code_project, code_inventory,
    index_code_repository_without_persisting_with_observer, route_binding_id,
};
pub(crate) use db::{
    db_inventory, delete_db_profile, index_db_profile_without_persisting, save_db_profile,
};
pub(crate) use limits::{bounded_code_inventory, bounded_db_inventory};
pub(crate) use model::{
    CodeCall, CodeIndexResult, CodeInventory, CreateWorkspaceRequest, DbConstraint,
    DbDependentObject, DbForeignKey, DbIndex, DbIndexResult, DbInventory, IndexCodeRequest,
    IndexDbProfileRequest, SaveDbProfileRequest, Workspace,
};
pub(crate) use model::{CodeInventoryItem, DbProfile, DbSource};
pub(crate) use model::{FocusedCodeSearch, FocusedCodeSearchMatch};

pub(crate) use store::{
    create_workspace, delete_workspace, list_workspaces, open_workspace, refresh_github_workspace,
    repair_workspace_from_backup, validate_workspace_id, workspace_recovery_warnings,
    write_workspace, WorkspaceRecoveryWarning,
};

#[cfg(test)]
pub(crate) use code::{
    attach_code_handles, code_project_from_index_stdout, downgrade_unverified_routes,
    extract_code_calls, extract_code_handles, extract_code_inventory, index_code_repository,
    next_code_project_generation,
};
#[cfg(test)]
pub(crate) use codebase_memory::{
    focused_code_search_pattern, focused_code_search_payload, index_payload,
    parse_focused_code_search_output,
};
#[cfg(test)]
pub(crate) use db::{
    apply_inventory_description_metadata, apply_table_description, db_cache_path,
    db_connection_config_path, db_connection_env_var, db_index_args, extract_bulk_db_inventory,
    extract_db_inventory, index_db_profile, record_db_identity_gaps,
};
#[cfg(test)]
pub(crate) use model::{
    ClientRequest, CodeHandle, CodeInventoryGap, CodeInventorySummary, DbInventoryColumn,
    DbInventoryTable, FocusedCodeSearchTotals, RepoSource, WorkspaceEngineCache,
};
#[cfg(test)]
pub(crate) use store::{
    engine_json_value, github_repo_name, workspace_code_cache_path, workspace_db_cache_dir,
    workspace_id, workspace_repo_dir,
};

#[cfg(test)]
mod tests;
