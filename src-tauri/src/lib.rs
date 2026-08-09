#![warn(unreachable_pub)]

pub use codebase_fact_model as fact_model_contract;

mod analysis;
mod command_error;
pub mod engine;
pub mod fact_graph;
mod paths;
mod provider;
mod provider_assets;
mod semantic;
mod source;
mod static_query;
mod workspace;

use command_error::{CommandError, CommandResult};
use engine::{EngineRegistry, EngineRuntimeMode};
use fact_graph::FactGraphStatus;
use paths::{base_paths, ensure_base_dirs, AppPaths};
use provider::{AiProviderAvailability, ProviderRegistry};
use source::{OpenSourceLocationRequest, RevealSourceLocationRequest, SourceActionResult};
use std::{path::PathBuf, sync::Mutex};
use tauri::{Emitter, Manager};
use workspace::{CreateWorkspaceRequest, SetWorkspaceProviderRequest, Workspace};

static WORKSPACE_MUTATION_LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn app_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    #[cfg(any(debug_assertions, codebase_workspace_internal_build))]
    {
        if let Some(path) = std::env::var_os("CODEBASE_WORKSPACE_APP_DATA_DIR") {
            return Ok(PathBuf::from(path));
        }
    }

    let runtime_local = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("로컬 앱 데이터 디렉터리를 찾지 못했습니다: {error}"))?;
    Ok(runtime_local
        .parent()
        .map(|parent| parent.join("CodebaseWorkspace"))
        .unwrap_or(runtime_local))
}

pub(crate) fn engine_registry_for_app(app: &tauri::AppHandle) -> Result<EngineRegistry, String> {
    let app_data_dir = app_data_dir(app)?;
    let paths = base_paths(&app_data_dir);
    ensure_base_dirs(&paths)
        .map_err(|error| format!("앱 데이터 디렉터리를 만들지 못했습니다: {error}"))?;
    let resource_dir = app.path().resource_dir().ok();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(PathBuf::from));
    let override_dir = std::env::var_os("CODEBASE_WORKSPACE_ENGINE_DIR").map(PathBuf::from);
    let mode = if cfg!(debug_assertions) {
        EngineRuntimeMode::Dev
    } else if cfg!(codebase_workspace_internal_build) {
        EngineRuntimeMode::Internal
    } else {
        EngineRuntimeMode::Production
    };
    Ok(engine::engine_registry(
        mode,
        app_data_dir,
        resource_dir.as_deref(),
        exe_dir.as_deref(),
        override_dir.as_deref(),
    ))
}

#[tauri::command]
fn get_app_paths(app: tauri::AppHandle) -> CommandResult<AppPaths> {
    let paths = base_paths(app_data_dir(&app)?);
    ensure_base_dirs(&paths)
        .map_err(|error| format!("앱 데이터 디렉터리를 만들지 못했습니다: {error}"))?;
    Ok(paths.into())
}

#[tauri::command(async)]
fn get_engine_availability(app: tauri::AppHandle) -> CommandResult<EngineRegistry> {
    engine_registry_for_app(&app).map_err(Into::into)
}

#[tauri::command(async)]
fn analyze_workspace(
    app: tauri::AppHandle,
    providers: tauri::State<'_, ProviderRegistry>,
    request: analysis::AnalyzeWorkspaceRequest,
) -> CommandResult<analysis::AnalyzeWorkspaceResult> {
    let app_data_dir = app_data_dir(&app)?;
    let workspace = workspace::open_workspace(&app_data_dir, &request.workspace_id)?;
    let analysis_guard = analysis::begin_workspace_analysis(&workspace.id)?;
    let previous_pointer = fact_graph::checkpoint_published_pointer(&app_data_dir, &workspace.id)?;
    let fact_graph = analysis::run_code_analysis(
        &app,
        &app_data_dir,
        &workspace,
        analysis_guard.operation_id(),
    )?;
    let _ = app.emit(
        "analysis-progress",
        analysis::AnalysisProgressEvent {
            workspace_id: workspace.id.clone(),
            stage: "semantic".to_string(),
            completed: 0,
            total: 1,
            label: "검증된 코드 사실로 의미 지도를 만드는 중".to_string(),
        },
    );
    let semantic_progress = |label: &str, completed: u64, total: u64| {
        let _ = app.emit(
            "analysis-progress",
            analysis::AnalysisProgressEvent {
                workspace_id: workspace.id.clone(),
                stage: "semantic".to_string(),
                completed,
                total,
                label: label.to_string(),
            },
        );
    };
    let semantic = semantic::analyze_and_publish(
        &app_data_dir,
        &workspace,
        providers.inner(),
        analysis_guard.operation_id(),
        Some(&semantic_progress),
    );
    match semantic {
        Ok(revision_id) => Ok(analysis::AnalyzeWorkspaceResult {
            fact_graph,
            semantic_revision_id: Some(revision_id),
            semantic_error: None,
        }),
        Err(error) => {
            fact_graph::restore_published_pointer(
                &app_data_dir,
                &workspace.id,
                previous_pointer.as_deref(),
            )?;
            let restored = fact_graph::status_for_workspace(&app_data_dir, &workspace.id)?;
            Ok(analysis::AnalyzeWorkspaceResult {
                fact_graph: restored,
                semantic_revision_id: None,
                semantic_error: Some(engine::redact_secrets(&error)),
            })
        }
    }
}

#[tauri::command]
fn cancel_workspace_analysis(workspace_id: String) -> CommandResult<bool> {
    analysis::cancel_workspace_analysis(&workspace_id).map_err(Into::into)
}

#[tauri::command]
fn list_ai_providers(providers: tauri::State<'_, ProviderRegistry>) -> Vec<AiProviderAvailability> {
    providers.list()
}

#[tauri::command]
fn list_workspaces(app: tauri::AppHandle) -> CommandResult<Vec<Workspace>> {
    workspace::list_workspaces(app_data_dir(&app)?).map_err(Into::into)
}

#[tauri::command]
fn create_workspace(
    app: tauri::AppHandle,
    request: CreateWorkspaceRequest,
) -> CommandResult<Workspace> {
    let _guard = WORKSPACE_MUTATION_LOCK
        .lock()
        .map_err(|_| CommandError::from("workspace mutation lock is poisoned"))?;
    workspace::create_workspace(app_data_dir(&app)?, request).map_err(Into::into)
}

#[tauri::command]
fn open_workspace(app: tauri::AppHandle, workspace_id: String) -> CommandResult<Workspace> {
    workspace::open_workspace(app_data_dir(&app)?, &workspace_id).map_err(Into::into)
}

#[tauri::command]
fn set_workspace_provider(
    app: tauri::AppHandle,
    request: SetWorkspaceProviderRequest,
) -> CommandResult<Workspace> {
    let _guard = WORKSPACE_MUTATION_LOCK
        .lock()
        .map_err(|_| CommandError::from("workspace mutation lock is poisoned"))?;
    workspace::set_workspace_provider(app_data_dir(&app)?, request).map_err(Into::into)
}

#[tauri::command]
fn delete_workspace(app: tauri::AppHandle, workspace_id: String) -> CommandResult<()> {
    let _guard = WORKSPACE_MUTATION_LOCK
        .lock()
        .map_err(|_| CommandError::from("workspace mutation lock is poisoned"))?;
    workspace::delete_workspace(app_data_dir(&app)?, &workspace_id).map_err(Into::into)
}

#[tauri::command]
fn get_fact_graph_status(
    app: tauri::AppHandle,
    workspace_id: String,
) -> CommandResult<FactGraphStatus> {
    workspace::open_workspace(app_data_dir(&app)?, &workspace_id)?;
    fact_graph::status_for_workspace(app_data_dir(&app)?, &workspace_id).map_err(Into::into)
}

#[tauri::command]
fn get_map_view(
    app: tauri::AppHandle,
    workspace_id: String,
) -> CommandResult<Option<semantic::MapView>> {
    let app_data_dir = app_data_dir(&app)?;
    let workspace = workspace::open_workspace(&app_data_dir, &workspace_id)?;
    semantic::get_map_view(&app_data_dir, &workspace).map_err(Into::into)
}

#[tauri::command]
fn get_map_selection(
    app: tauri::AppHandle,
    workspace_id: String,
    selected_id: String,
) -> CommandResult<Option<semantic::MapSelection>> {
    let app_data_dir = app_data_dir(&app)?;
    let workspace = workspace::open_workspace(&app_data_dir, &workspace_id)?;
    semantic::get_map_selection(&app_data_dir, &workspace, &selected_id).map_err(Into::into)
}

#[tauri::command]
fn open_source_location(
    app: tauri::AppHandle,
    request: OpenSourceLocationRequest,
) -> CommandResult<SourceActionResult> {
    source::open_source_location(app_data_dir(&app)?, request).map_err(Into::into)
}

#[tauri::command]
fn reveal_source_location(
    app: tauri::AppHandle,
    request: RevealSourceLocationRequest,
) -> CommandResult<SourceActionResult> {
    source::reveal_source_location(app_data_dir(&app)?, request).map_err(Into::into)
}

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
        .manage(ProviderRegistry::discover())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_app_paths,
            get_engine_availability,
            analyze_workspace,
            cancel_workspace_analysis,
            list_ai_providers,
            list_workspaces,
            create_workspace,
            open_workspace,
            set_workspace_provider,
            delete_workspace,
            get_fact_graph_status,
            get_map_view,
            get_map_selection,
            open_source_location,
            reveal_source_location,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
