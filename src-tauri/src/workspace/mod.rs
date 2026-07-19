mod code;
mod db;
mod model;
mod store;

pub(crate) use code::focused_code_search;
pub(crate) use code::{code_inventory, index_code_repository};
pub(crate) use db::{db_inventory, delete_db_profile, index_db_profile, save_db_profile};
pub(crate) use model::{
    CodeIndexResult, CodeInventory, CreateWorkspaceRequest, DbConstraint, DbForeignKey, DbIndex,
    DbIndexResult, DbInventory, IndexCodeRequest, IndexDbProfileRequest, SaveDbProfileRequest,
    Workspace,
};
pub(crate) use model::{CodeInventoryItem, DbProfile, DbSource};
pub(crate) use model::{FocusedCodeSearch, FocusedCodeSearchMatch};
pub(crate) use store::{
    create_workspace, delete_workspace, list_workspaces, open_workspace, refresh_github_workspace,
    repair_workspace_from_backup, validate_workspace_id, workspace_recovery_warnings,
    WorkspaceRecoveryWarning,
};

#[cfg(test)]
pub(crate) use code::{
    attach_code_handles, code_index_payload, code_label_payload, code_project_from_index_stdout,
    downgrade_unverified_routes, enrich_code_locations, extract_code_calls, extract_code_handles,
    extract_code_inventory, focused_code_search_args, focused_code_search_pattern,
    parse_focused_code_search_output, CALLS_QUERY, HANDLES_QUERY, SOURCE_LOCATIONS_QUERY,
};
#[cfg(test)]
pub(crate) use db::{
    apply_inventory_description_metadata, apply_table_description, db_cache_path,
    db_connection_config_path, db_connection_env_var, db_describe_plan, db_describe_table_args,
    db_find_args, db_index_args, db_inventory_args, extract_bulk_db_inventory,
    extract_db_inventory, merge_db_inventory_lines, record_bulk_fallback_gap,
    record_db_identity_gaps,
};
#[cfg(test)]
pub(crate) use model::{
    CodeCall, CodeHandle, CodeInventorySummary, DbInventoryColumn, DbInventoryTable,
    FocusedCodeSearchTotals, RepoSource, WorkspaceEngineCache,
};
#[cfg(test)]
pub(crate) use store::{
    engine_json_value, github_repo_name, workspace_code_cache_path, workspace_db_cache_dir,
    workspace_id, workspace_repo_dir,
};

#[cfg(test)]
mod tests;
