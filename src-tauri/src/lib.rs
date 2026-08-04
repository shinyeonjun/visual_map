#![warn(unreachable_pub)]

mod atlas;
mod command_error;
mod engine;
mod paths;
mod source;
mod workspace;

use atlas::{
    enrich_composition_code_evidence, enrich_integrated_snapshot_code_evidence,
    enrich_snapshot_code_evidence, normalized_change_intent, ChangeIntent, InventorySnapshot,
    VisualMap,
};
use command_error::CommandResult;
use engine::{EngineRegistry, EngineRuntimeMode};
use fs2::FileExt;
use paths::{base_paths, ensure_base_dirs, AppPaths};
use source::{OpenSourceLocationRequest, RevealSourceLocationRequest, SourceActionResult};
use std::{
    borrow::Cow,
    collections::HashSet,
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex},
    thread,
};
use tauri::{Emitter, Manager};
use workspace::{
    bounded_code_inventory, bounded_db_inventory, validate_workspace_id, CodeIndexResult,
    CodeInventory, CreateWorkspaceRequest, DbIndexResult, DbInventory, IndexCodeRequest,
    IndexDbProfileRequest, SaveDbProfileRequest, Workspace,
};

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompositionMapRequest {
    focus_ids: Vec<String>,
    relation_view: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct InitializeWorkspaceAnalysisRequest {
    workspace_id: String,
    #[serde(default)]
    analysis_mode: AnalysisSourceMode,
    db_profile_id: Option<String>,
    connection_string: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
enum AnalysisSourceMode {
    #[default]
    CodeOnly,
    DbOnly,
    CodeAndDb,
}

impl AnalysisSourceMode {
    fn includes_code(self) -> bool {
        matches!(self, Self::CodeOnly | Self::CodeAndDb)
    }

    fn includes_db(self) -> bool {
        matches!(self, Self::DbOnly | Self::CodeAndDb)
    }

    fn required_sources_ready(self, code_ready: bool, db_ready: bool) -> bool {
        (!self.includes_code() || code_ready) && (!self.includes_db() || db_ready)
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct InitializeWorkspaceAnalysisResult {
    workspace: Workspace,
    code: Option<CodeIndexResult>,
    db: Option<DbIndexResult>,
    code_error: Option<String>,
    db_error: Option<String>,
    snapshot_saved: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AnalysisProgressEvent {
    workspace_id: String,
    source: String,
    stage: String,
    completed: usize,
    total: usize,
    percent: u8,
    label: String,
    determinate: bool,
}

fn emit_analysis_progress(
    app: &tauri::AppHandle,
    workspace_id: &str,
    source: &str,
    stage: &str,
    completed: usize,
    total: usize,
    percent: u8,
    label: impl Into<String>,
) {
    let _ = app.emit(
        "analysis-progress",
        AnalysisProgressEvent {
            workspace_id: workspace_id.to_string(),
            source: source.to_string(),
            stage: stage.to_string(),
            completed,
            total: total.max(1),
            percent: percent.min(100),
            label: label.into(),
            determinate: total > 0,
        },
    );
}

fn code_progress_observer(
    app: tauri::AppHandle,
    workspace_id: String,
    includes_db: bool,
) -> engine::EngineObserver {
    Arc::new(move |event| {
        let Some(raw) = event.line.strip_prefix("@visual-map-progress ") else {
            return;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
            return;
        };
        let completed = value
            .get("completed")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_default() as usize;
        let total = value
            .get("total")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(100) as usize;
        let stage = value
            .get("stage")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("code");
        let label = value
            .get("label")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("코드 구조 분석 중");
        let share = if includes_db { 65usize } else { 82usize };
        let percent = 5usize + completed.min(total).saturating_mul(share) / total.max(1);
        emit_analysis_progress(
            &app,
            &workspace_id,
            "code",
            stage,
            completed,
            total,
            percent.min(90) as u8,
            label,
        );
    })
}

// The desktop app is single-instance, but commands can still overlap within it.
static ACTIVE_WORKSPACE_MUTATIONS: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

struct WorkspaceMutationGuard {
    workspace_id: String,
    lock_file: File,
}

fn begin_workspace_mutation(
    app_data_dir: &Path,
    workspace_id: &str,
) -> Result<WorkspaceMutationGuard, String> {
    validate_workspace_id(workspace_id)?;
    let mut active = ACTIVE_WORKSPACE_MUTATIONS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if !active.insert(workspace_id.to_string()) {
        return Err("이 작업공간에서 다른 변경 작업이 진행 중입니다".to_string());
    }

    let lock_dir = app_data_dir.join("workspace-locks");
    if let Err(error) = fs::create_dir_all(&lock_dir) {
        active.remove(workspace_id);
        return Err(format!("작업공간 잠금 폴더를 만들지 못했습니다: {error}"));
    }
    let lock_file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_dir.join(format!("{workspace_id}.lock")));
    let lock_file = match lock_file {
        Ok(lock_file) => lock_file,
        Err(error) => {
            active.remove(workspace_id);
            return Err(format!("작업공간 잠금 파일을 열지 못했습니다: {error}"));
        }
    };
    if let Err(error) = lock_file.try_lock_exclusive() {
        active.remove(workspace_id);
        return Err(format!(
            "다른 앱 인스턴스에서 이 작업공간의 변경 작업이 진행 중입니다: {error}"
        ));
    }

    Ok(WorkspaceMutationGuard {
        workspace_id: workspace_id.to_string(),
        lock_file,
    })
}

impl Drop for WorkspaceMutationGuard {
    fn drop(&mut self) {
        let _ = self.lock_file.unlock();
        ACTIVE_WORKSPACE_MUTATIONS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.workspace_id);
    }
}

fn app_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    #[cfg(any(debug_assertions, backend_visual_map_internal_build))]
    {
        if let Some(path) = std::env::var_os("BACKEND_VISUAL_MAP_APP_DATA_DIR") {
            return Ok(PathBuf::from(path));
        }
    }
    let runtime_local = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("로컬 앱 데이터 디렉터리를 찾지 못했습니다: {error}"))?;
    // Keep the product data root stable while the Tauri identifier follows the
    // reverse-domain convention, so existing workspaces survive upgrades.
    Ok(runtime_local
        .parent()
        .map(|parent| parent.join("VisualMap"))
        .unwrap_or(runtime_local))
}

include!("commands/analysis.rs");
include!("commands/inventory.rs");
include!("commands/source.rs");
include!("commands/workspace.rs");

#[cfg(test)]
include!("lib_tests.rs");

pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
    }));

    builder
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_app_paths,
            get_engine_availability,
            save_db_profile,
            index_db_profile,
            index_code_repository,
            initialize_workspace_analysis,
            load_inventory_bootstrap,
            search_inventory,
            refresh_snapshot_freshness,
            get_visual_map,
            cancel_visual_map,
            open_source_location,
            reveal_source_location,
            create_workspace,
            open_workspace,
            refresh_github_workspace,
            list_workspaces,
            get_workspace_recovery_warnings,
            repair_workspace_from_backup,
            delete_workspace,
            delete_db_profile
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
