#![warn(unreachable_pub)]

mod atlas;
mod command_error;
mod engine;
mod paths;
mod source;
mod workspace;

use atlas::{ChangeIntent, InventorySnapshot, VisualMap};
use command_error::CommandResult;
use engine::{EngineRegistry, EngineRuntimeMode};
use paths::{base_paths, ensure_base_dirs, migrate_roaming_data_to_local, AppPaths};
use source::{OpenSourceLocationRequest, RevealSourceLocationRequest, SourceActionResult};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use tauri::Manager;
use workspace::{
    CodeIndexResult, CodeInventory, CreateWorkspaceRequest, DbIndexResult, DbInventory,
    IndexCodeRequest, IndexDbProfileRequest, SaveDbProfileRequest, Workspace,
};

fn app_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    #[cfg(debug_assertions)]
    {
        if let Some(path) = std::env::var_os("BACKEND_VISUAL_MAP_APP_DATA_DIR") {
            return Ok(PathBuf::from(path));
        }
    }
    let local = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("로컬 앱 데이터 디렉터리를 찾지 못했습니다: {error}"))?;
    let roaming = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("기존 앱 데이터 디렉터리를 찾지 못했습니다: {error}"))?;
    migrate_roaming_data_to_local(local, roaming)
        .map_err(|error| format!("앱 데이터 디렉터리 이전 실패: {error}"))
}

#[tauri::command]
fn get_app_paths(app: tauri::AppHandle) -> CommandResult<AppPaths> {
    let app_data_dir = app_data_dir(&app)?;
    let paths = base_paths(app_data_dir);

    ensure_base_dirs(&paths).map_err(|error| format!("앱 데이터 디렉터리 생성 실패: {error}"))?;

    Ok(paths.into())
}

#[tauri::command(async)]
fn get_engine_availability(app: tauri::AppHandle) -> CommandResult<EngineRegistry> {
    let app_data_dir = app_data_dir(&app)?;
    let paths = base_paths(&app_data_dir);

    ensure_base_dirs(&paths).map_err(|error| format!("앱 데이터 디렉터리 생성 실패: {error}"))?;

    let resource_dir = app.path().resource_dir().ok();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(PathBuf::from));
    let override_dir = std::env::var_os("BACKEND_VISUAL_MAP_ENGINE_DIR").map(PathBuf::from);
    let mode = if cfg!(debug_assertions) {
        EngineRuntimeMode::Dev
    } else if cfg!(backend_visual_map_internal_build) {
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
fn save_db_profile(
    app: tauri::AppHandle,
    request: SaveDbProfileRequest,
) -> CommandResult<Workspace> {
    let app_data_dir = app_data_dir(&app)?;

    Ok(workspace::save_db_profile(app_data_dir, request)?)
}

#[tauri::command(async)]
fn index_db_profile(
    app: tauri::AppHandle,
    request: IndexDbProfileRequest,
) -> CommandResult<DbIndexResult> {
    let app_data_dir = app_data_dir(&app)?;
    let registry = get_engine_availability(app)?;

    Ok(workspace::index_db_profile(
        app_data_dir,
        &registry,
        request,
    )?)
}

#[tauri::command(async)]
fn get_db_inventory(
    app: tauri::AppHandle,
    workspace_id: String,
    profile_id: Option<String>,
) -> CommandResult<DbInventory> {
    let app_data_dir = app_data_dir(&app)?;
    let registry = get_engine_availability(app)?;

    Ok(workspace::db_inventory(
        app_data_dir,
        &registry,
        &workspace_id,
        profile_id.as_deref(),
    )?)
}

#[tauri::command(async)]
fn index_code_repository(
    app: tauri::AppHandle,
    request: IndexCodeRequest,
) -> CommandResult<CodeIndexResult> {
    let app_data_dir = app_data_dir(&app)?;
    let registry = get_engine_availability(app)?;

    Ok(workspace::index_code_repository(
        app_data_dir,
        &registry,
        request,
    )?)
}

#[tauri::command(async)]
fn get_code_inventory(app: tauri::AppHandle, workspace_id: String) -> CommandResult<CodeInventory> {
    let app_data_dir = app_data_dir(&app)?;
    let registry = get_engine_availability(app)?;

    Ok(workspace::code_inventory(
        app_data_dir,
        &registry,
        &workspace_id,
    )?)
}

#[tauri::command(async)]
fn save_inventory_snapshot(
    app: tauri::AppHandle,
    workspace_id: String,
    code: Option<CodeInventory>,
    db: Option<DbInventory>,
) -> CommandResult<InventorySnapshot> {
    let app_data_dir = app_data_dir(&app)?;
    let workspace = workspace::open_workspace(&app_data_dir, &workspace_id)?;
    if code.is_none() && db.is_none() {
        return Err("저장할 코드 또는 DB 읽기 결과가 없습니다".into());
    }
    let registry = get_engine_availability(app)?;
    let snapshot = atlas::build_inventory_snapshot(workspace_id, code.as_ref(), db.as_ref());
    let snapshot = atlas::snapshot_with_metadata(snapshot, &workspace, &registry);

    atlas::save_inventory_snapshot(app_data_dir, &snapshot)?;
    Ok(snapshot)
}

#[tauri::command(async)]
fn load_inventory_snapshot(
    app: tauri::AppHandle,
    workspace_id: String,
) -> CommandResult<Option<InventorySnapshot>> {
    let app_data_dir = app_data_dir(&app)?;
    let workspace = workspace::open_workspace(&app_data_dir, &workspace_id)?;
    let registry = get_engine_availability(app)?;
    let Some(snapshot) = atlas::load_inventory_snapshot_optional(&app_data_dir, &workspace_id)?
    else {
        return Ok(None);
    };

    Ok(Some(atlas::mark_snapshot_staleness(
        snapshot, &workspace, &registry,
    )))
}

#[tauri::command(async)]
fn get_visual_map(
    app: tauri::AppHandle,
    workspace_id: String,
    focus_id: Option<String>,
    mode: String,
    change_intent: Option<ChangeIntent>,
    enrich_code_evidence: Option<bool>,
) -> CommandResult<VisualMap> {
    let app_data_dir = app_data_dir(&app)?;
    let workspace = workspace::open_workspace(&app_data_dir, &workspace_id)?;
    let registry = get_engine_availability(app)?;
    let snapshot = atlas::load_inventory_snapshot_cached(&app_data_dir, &workspace_id)
        .map_err(|error| format!("캔버스를 보려면 먼저 코드/DB 읽기 결과가 필요합니다: {error}"))?;
    let stale_reasons = atlas::snapshot_staleness_reasons(&snapshot, &workspace, &registry);
    if !stale_reasons.is_empty() {
        return Err(format!(
            "코드/DB 읽기 결과가 최신이 아닙니다: {}",
            stale_reasons.join(", ")
        )
        .into());
    }
    let change_intent = normalized_change_intent(change_intent)?;

    if enrich_code_evidence.unwrap_or(false)
        && matches!(mode.as_str(), "table-usage" | "column-impact")
        && focus_id.is_some()
    {
        let mut enriched_snapshot = (*snapshot).clone();
        enrich_snapshot_code_evidence(
            &app_data_dir,
            &registry,
            &workspace_id,
            focus_id.as_deref(),
            &mode,
            &mut enriched_snapshot,
        );
        return Ok(atlas::visual_map_with_change(
            &enriched_snapshot,
            focus_id,
            mode,
            change_intent,
        ));
    }

    Ok(atlas::visual_map_with_change(
        &snapshot,
        focus_id,
        mode,
        change_intent,
    ))
}

fn normalized_change_intent(intent: Option<ChangeIntent>) -> Result<Option<ChangeIntent>, String> {
    let Some(mut intent) = intent else {
        return Ok(None);
    };
    if !matches!(
        intent.kind.as_str(),
        "rename" | "drop" | "type" | "nullability"
    ) {
        return Err("지원하는 변경 종류는 이름 변경, 삭제, 타입 변경, NULL 제약입니다".to_string());
    }
    intent.value = intent
        .value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if intent
        .value
        .as_deref()
        .is_some_and(|value| value.len() > 128 || value.chars().any(char::is_control))
    {
        return Err("변경 대상 값은 제어 문자 없이 128자 이하여야 합니다".to_string());
    }
    if intent.kind == "nullability"
        && intent
            .value
            .as_deref()
            .is_some_and(|value| !matches!(value, "nullable" | "required"))
    {
        return Err("NULL 제약 값은 nullable 또는 required여야 합니다".to_string());
    }
    if intent.kind == "drop" {
        intent.value = None;
    }
    Ok(Some(intent))
}

fn enrich_snapshot_code_evidence(
    app_data_dir: &Path,
    registry: &EngineRegistry,
    workspace_id: &str,
    focus_id: Option<&str>,
    mode: &str,
    snapshot: &mut InventorySnapshot,
) {
    let Some(focus_id) = focus_id else {
        return;
    };
    let Some(focus) = snapshot
        .items
        .iter()
        .find(|item| item.id == focus_id)
        .cloned()
    else {
        return;
    };
    let (table, column) = match (mode, focus.kind.as_str()) {
        ("table-usage", "table") => (focus, None),
        ("column-impact", "column") => {
            let Some(table) = focus
                .parent_id
                .as_deref()
                .and_then(|parent_id| snapshot.items.iter().find(|item| item.id == parent_id))
                .cloned()
            else {
                atlas::record_code_search_gap(
                    snapshot,
                    focus.id.as_str(),
                    "code-search-scope-missing",
                    "상위 테이블이 없어 컬럼 코드 검색 범위를 만들 수 없습니다.",
                    Vec::new(),
                );
                return;
            };
            (table, Some(focus))
        }
        _ => return,
    };
    let ambiguous_table_ids = snapshot
        .items
        .iter()
        .filter(|item| {
            item.kind == "table"
                && item.source == "db"
                && item.name.eq_ignore_ascii_case(&table.name)
        })
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    let schema_ambiguous = ambiguous_table_ids.len() > 1;
    if schema_ambiguous {
        atlas::record_code_search_gap(
            snapshot,
            column.as_ref().map_or(table.id.as_str(), |column| column.id.as_str()),
            "code-search-schema-ambiguous",
            "동일한 테이블명이 여러 스키마에 있어 텍스트 검색 후보의 신뢰도를 high로 표시하지 않습니다.",
            ambiguous_table_ids,
        );
    }

    let table_search = match workspace::focused_code_search(
        app_data_dir,
        registry,
        workspace_id,
        table.name.as_str(),
        None,
        32,
    ) {
        Ok(search) => search,
        Err(_) => {
            atlas::record_code_search_gap(
                snapshot,
                table.id.as_str(),
                "code-search-failure",
                "테이블 식별자 코드 검색에 실패했습니다. 기본 snapshot 후보는 그대로 유지합니다.",
                Vec::new(),
            );
            return;
        }
    };
    let table_evidence = atlas::apply_focused_code_evidence(
        snapshot,
        table.id.as_str(),
        &table_search,
        schema_ambiguous,
    );
    let Some(column) = column else {
        return;
    };

    let (path_filter, omitted_files) = focused_code_path_filter(&table_evidence.matched_files);
    if omitted_files > 0 {
        atlas::record_code_search_gap(
            snapshot,
            column.id.as_str(),
            "code-search-scope-truncated",
            &format!(
                "테이블 검색 파일 중 {omitted_files}개를 컬럼 검색의 16개/512-byte 경로 범위에 포함하지 못했습니다."
            ),
            vec![table.id.clone()],
        );
    }
    let Some(path_filter) = path_filter else {
        atlas::record_code_search_gap(
            snapshot,
            column.id.as_str(),
            "code-search-scope-empty",
            "테이블 식별자가 확인된 파일이 없어 일반적인 컬럼명을 저장소 전체에서 검색하지 않았습니다.",
            vec![table.id.clone()],
        );
        return;
    };
    match workspace::focused_code_search(
        app_data_dir,
        registry,
        workspace_id,
        column.name.as_str(),
        Some(path_filter.as_str()),
        32,
    ) {
        Ok(search) => {
            atlas::apply_focused_code_evidence(
                snapshot,
                column.id.as_str(),
                &search,
                schema_ambiguous,
            );
        }
        Err(_) => atlas::record_code_search_gap(
            snapshot,
            column.id.as_str(),
            "code-search-failure",
            "컬럼 식별자 코드 검색에 실패했습니다. 기본 snapshot 후보는 그대로 유지합니다.",
            vec![table.id.clone()],
        ),
    }
}

fn focused_code_path_filter(paths: &[String]) -> (Option<String>, usize) {
    const MAX_FILES: usize = 16;
    const MAX_FILTER_BYTES: usize = 512;

    let paths = paths
        .iter()
        .map(|path| path.replace('\\', "/"))
        .collect::<BTreeSet<_>>();
    let total = paths.len();
    let mut escaped = Vec::new();
    for path in paths {
        if escaped.len() == MAX_FILES {
            break;
        }
        let path = escape_regex(path.as_str());
        let mut candidate_paths = escaped.clone();
        candidate_paths.push(path.clone());
        let candidate = format!("^({})$", candidate_paths.join("|"));
        if candidate.len() > MAX_FILTER_BYTES {
            continue;
        }
        escaped.push(path);
    }
    let selected = escaped.len();
    (
        (!escaped.is_empty()).then(|| format!("^({})$", escaped.join("|"))),
        total.saturating_sub(selected),
    )
}

fn escape_regex(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(
            character,
            '\\' | '^' | '$' | '.' | '|' | '?' | '*' | '+' | '(' | ')' | '[' | ']' | '{' | '}'
        ) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

#[cfg(test)]
mod code_evidence_tests {
    use super::{focused_code_path_filter, normalized_change_intent};
    use crate::atlas::ChangeIntent;

    #[test]
    fn column_search_path_filter_is_exact_deduplicated_and_bounded() {
        let mut paths = (0..20)
            .map(|index| format!("src/r{index}/orders.query.ts"))
            .collect::<Vec<_>>();
        paths.push("src/r0/orders.query.ts".to_string());

        let (filter, omitted) = focused_code_path_filter(&paths);
        let filter = filter.unwrap();

        assert!(filter.starts_with("^("));
        assert!(filter.ends_with(")$"));
        assert!(filter.contains(r"orders\.query\.ts"));
        assert!(filter.len() <= 512);
        assert_eq!(omitted, 4);
    }

    #[test]
    fn change_intent_is_bounded_and_normalized() {
        let intent = normalized_change_intent(Some(ChangeIntent {
            kind: "rename".to_string(),
            value: Some("  display_name  ".to_string()),
        }))
        .unwrap()
        .unwrap();
        assert_eq!(intent.value.as_deref(), Some("display_name"));

        assert!(normalized_change_intent(Some(ChangeIntent {
            kind: "nullability".to_string(),
            value: Some("sometimes".to_string()),
        }))
        .is_err());
        assert!(normalized_change_intent(Some(ChangeIntent {
            kind: "rename".to_string(),
            value: Some("x".repeat(129)),
        }))
        .is_err());
    }
}

#[tauri::command]
fn open_source_location(
    app: tauri::AppHandle,
    request: OpenSourceLocationRequest,
) -> CommandResult<SourceActionResult> {
    Ok(source::open_source_location(app_data_dir(&app)?, request)?)
}

#[tauri::command]
fn reveal_source_location(
    app: tauri::AppHandle,
    request: RevealSourceLocationRequest,
) -> CommandResult<SourceActionResult> {
    Ok(source::reveal_source_location(
        app_data_dir(&app)?,
        request,
    )?)
}

#[tauri::command(async)]
fn create_workspace(
    app: tauri::AppHandle,
    request: CreateWorkspaceRequest,
) -> CommandResult<Workspace> {
    let app_data_dir = app_data_dir(&app)?;

    Ok(workspace::create_workspace(app_data_dir, request)?)
}

#[tauri::command]
fn open_workspace(app: tauri::AppHandle, workspace_id: String) -> CommandResult<Workspace> {
    let app_data_dir = app_data_dir(&app)?;

    Ok(workspace::open_workspace(app_data_dir, &workspace_id)?)
}

#[tauri::command(async)]
fn refresh_github_workspace(
    app: tauri::AppHandle,
    workspace_id: String,
) -> CommandResult<Workspace> {
    Ok(workspace::refresh_github_workspace(
        app_data_dir(&app)?,
        &workspace_id,
    )?)
}

#[tauri::command(async)]
fn get_workspace_recovery_warnings(
    app: tauri::AppHandle,
) -> CommandResult<Vec<workspace::WorkspaceRecoveryWarning>> {
    Ok(workspace::workspace_recovery_warnings(app_data_dir(&app)?)?)
}

#[tauri::command(async)]
fn repair_workspace_from_backup(
    app: tauri::AppHandle,
    workspace_id: String,
) -> CommandResult<Workspace> {
    Ok(workspace::repair_workspace_from_backup(
        app_data_dir(&app)?,
        &workspace_id,
    )?)
}

#[tauri::command(async)]
fn delete_workspace(app: tauri::AppHandle, workspace_id: String) -> CommandResult<()> {
    Ok(workspace::delete_workspace(
        app_data_dir(&app)?,
        &workspace_id,
    )?)
}

#[tauri::command(async)]
fn delete_db_profile(
    app: tauri::AppHandle,
    workspace_id: String,
    profile_id: String,
) -> CommandResult<Workspace> {
    let app_data_dir = app_data_dir(&app)?;
    let workspace = workspace::open_workspace(&app_data_dir, &workspace_id)?;
    if !workspace
        .db_profiles
        .iter()
        .any(|profile| profile.id == profile_id)
    {
        return Err("삭제할 DB 연결을 찾을 수 없습니다".into());
    }
    atlas::remove_db_inventory_snapshot(&app_data_dir, &workspace_id)?;
    Ok(workspace::delete_db_profile(
        app_data_dir,
        &workspace_id,
        &profile_id,
    )?)
}

#[tauri::command(async)]
fn list_workspaces(app: tauri::AppHandle) -> CommandResult<Vec<Workspace>> {
    let app_data_dir = app_data_dir(&app)?;

    Ok(workspace::list_workspaces(app_data_dir)?)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_app_paths,
            get_engine_availability,
            save_db_profile,
            index_db_profile,
            get_db_inventory,
            index_code_repository,
            get_code_inventory,
            save_inventory_snapshot,
            load_inventory_snapshot,
            get_visual_map,
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
