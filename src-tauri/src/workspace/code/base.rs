use crate::paths::base_paths;
use crate::{engine, EngineRegistry};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use super::client_requests::extract_client_requests;
use super::codebase_memory::{CodebaseMemoryAdapter, CodebaseMemoryInventory, CODE_NODE_LABELS};
use super::model::{
    CodeCall, CodeHandle, CodeIndexResult, CodeInventory, CodeInventoryGap, CodeInventoryItem,
    CodeInventorySummary, FocusedCodeSearch, IndexCodeRequest,
};
use super::store::{
    engine_json_value, object_bool, object_string, read_workspace_by_id, timestamp,
    validate_workspace_id, value_items, workspace_code_cache_path, workspace_db_cache_dir,
    write_workspace,
};

static NEXT_CODE_PROJECT_GENERATION: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
pub(crate) fn index_code_repository(
    app_data_dir: impl AsRef<Path>,
    registry: &EngineRegistry,
    request: IndexCodeRequest,
) -> Result<CodeIndexResult, String> {
    index_code_repository_with_persistence(app_data_dir, registry, request, true, None)
}

pub(crate) fn index_code_repository_without_persisting_with_observer(
    app_data_dir: impl AsRef<Path>,
    registry: &EngineRegistry,
    request: IndexCodeRequest,
    observer: engine::EngineObserver,
) -> Result<CodeIndexResult, String> {
    index_code_repository_with_persistence(app_data_dir, registry, request, false, Some(observer))
}
