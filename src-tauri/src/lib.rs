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
    #[cfg(any(debug_assertions, backend_visual_map_internal_build))]
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
    let workspace_id = request.workspace_id.clone();
    let profile_id = request.profile_id.clone();

    let mut result = workspace::index_db_profile(&app_data_dir, &registry, request)?;
    if result.run.ok {
        match workspace::db_inventory(&app_data_dir, &registry, &workspace_id, Some(&profile_id)) {
            Ok(inventory) => {
                match persist_db_inventory(&app_data_dir, &result.workspace, &registry, &inventory)
                {
                    Ok(()) => result.inventory = Some(bounded_db_inventory(inventory)),
                    Err(error) => result.inventory_error = Some(error),
                }
            }
            Err(error) => result.inventory_error = Some(error),
        }
    }
    Ok(result)
}

#[tauri::command(async)]
fn index_code_repository(
    app: tauri::AppHandle,
    request: IndexCodeRequest,
) -> CommandResult<CodeIndexResult> {
    let app_data_dir = app_data_dir(&app)?;
    let registry = get_engine_availability(app)?;
    let workspace_id = request.workspace_id.clone();

    let mut result = workspace::index_code_repository(&app_data_dir, &registry, request)?;
    if result.run.ok {
        match workspace::code_inventory(&app_data_dir, &registry, &workspace_id) {
            Ok(inventory) => match persist_code_inventory(
                &app_data_dir,
                &result.workspace,
                &registry,
                &inventory,
            ) {
                Ok(()) => result.inventory = Some(bounded_code_inventory(inventory)),
                Err(error) => result.inventory_error = Some(error),
            },
            Err(error) => result.inventory_error = Some(error),
        }
    }
    Ok(result)
}

#[tauri::command(async)]
fn load_inventory_bootstrap(
    app: tauri::AppHandle,
    workspace_id: String,
) -> CommandResult<Option<atlas::InventoryBootstrap>> {
    let app_data_dir = app_data_dir(&app)?;
    let workspace = workspace::open_workspace(&app_data_dir, &workspace_id)?;
    let registry = get_engine_availability(app)?;
    let Some(snapshot) =
        atlas::load_inventory_snapshot_optional_cached(&app_data_dir, &workspace_id)?
    else {
        return Ok(None);
    };
    let stale_reasons = atlas::snapshot_staleness_reasons_cached(&snapshot, &workspace, &registry);
    let mut bootstrap = atlas::inventory_bootstrap(&snapshot);
    bootstrap.snapshot.stale_reasons = stale_reasons;
    Ok(Some(bootstrap))
}

fn persist_code_inventory(
    app_data_dir: &Path,
    workspace: &Workspace,
    registry: &EngineRegistry,
    inventory: &CodeInventory,
) -> Result<(), String> {
    let snapshot = atlas::build_inventory_snapshot(workspace.id.clone(), Some(inventory), None);
    persist_inventory_source(app_data_dir, workspace, registry, snapshot, "code")
}

fn persist_db_inventory(
    app_data_dir: &Path,
    workspace: &Workspace,
    registry: &EngineRegistry,
    inventory: &DbInventory,
) -> Result<(), String> {
    let snapshot = atlas::build_inventory_snapshot(workspace.id.clone(), None, Some(inventory));
    persist_inventory_source(app_data_dir, workspace, registry, snapshot, "db")
}

fn persist_inventory_source(
    app_data_dir: &Path,
    workspace: &Workspace,
    registry: &EngineRegistry,
    snapshot: InventorySnapshot,
    source: &str,
) -> Result<(), String> {
    let incoming = atlas::snapshot_with_metadata(snapshot, workspace, registry);
    let existing = atlas::load_inventory_snapshot_optional(app_data_dir, &workspace.id)?;
    let merged = atlas::replace_inventory_source(existing, incoming, source)?;
    atlas::save_inventory_snapshot(app_data_dir, &merged)
}

fn bounded_code_inventory(mut inventory: CodeInventory) -> CodeInventory {
    const LIMIT: usize = 100;
    let mut partial = truncate_to(&mut inventory.routes, LIMIT);
    partial |= truncate_to(&mut inventory.services, LIMIT);
    partial |= truncate_to(&mut inventory.files, LIMIT);
    partial |= truncate_to(&mut inventory.handlers, LIMIT);
    partial |= truncate_to(&mut inventory.repositories, LIMIT);
    partial |= truncate_to(&mut inventory.functions, LIMIT);
    partial |= truncate_to(&mut inventory.classes, LIMIT);
    partial |= truncate_to(&mut inventory.modules, LIMIT);
    partial |= truncate_to(&mut inventory.unknown, LIMIT);
    let retained = inventory
        .routes
        .iter()
        .chain(inventory.services.iter())
        .chain(inventory.files.iter())
        .chain(inventory.handlers.iter())
        .chain(inventory.repositories.iter())
        .chain(inventory.functions.iter())
        .chain(inventory.classes.iter())
        .chain(inventory.modules.iter())
        .chain(inventory.unknown.iter())
        .map(|item| item.id.as_str())
        .collect::<BTreeSet<_>>();
    inventory.calls.retain(|call| {
        retained.contains(call.from.as_str()) && retained.contains(call.to.as_str())
    });
    inventory.handles.retain(|handle| {
        retained.contains(handle.route.as_str()) && retained.contains(handle.handler.as_str())
    });
    inventory.partial = partial;
    inventory
}

fn bounded_db_inventory(mut inventory: DbInventory) -> DbInventory {
    inventory.tables.truncate(100);
    inventory
}

fn truncate_to<T>(items: &mut Vec<T>, limit: usize) -> bool {
    let truncated = items.len() > limit;
    items.truncate(limit);
    truncated
}

#[tauri::command(async)]
fn search_inventory(
    app: tauri::AppHandle,
    workspace_id: String,
    query: String,
) -> CommandResult<atlas::InventorySearchResult> {
    let app_data_dir = app_data_dir(&app)?;
    let workspace = workspace::open_workspace(&app_data_dir, &workspace_id)?;
    let registry = get_engine_availability(app)?;
    let snapshot = atlas::load_inventory_snapshot_cached(&app_data_dir, &workspace_id)
        .map_err(|error| format!("검색하려면 먼저 코드/DB 읽기 결과가 필요합니다: {error}"))?;
    let stale_reasons = atlas::snapshot_staleness_reasons_cached(&snapshot, &workspace, &registry);
    if !stale_reasons.is_empty() {
        return Err(format!(
            "코드/DB 읽기 결과가 최신이 아닙니다: {}",
            stale_reasons.join(", ")
        )
        .into());
    }
    Ok(atlas::search_inventory(&snapshot, &query))
}

#[tauri::command(async)]
fn refresh_snapshot_freshness(
    app: tauri::AppHandle,
    workspace_id: String,
) -> CommandResult<Vec<String>> {
    let app_data_dir = app_data_dir(&app)?;
    let workspace = workspace::open_workspace(&app_data_dir, &workspace_id)?;
    let registry = get_engine_availability(app)?;
    let snapshot = atlas::load_inventory_snapshot_cached(&app_data_dir, &workspace_id)
        .map_err(|error| format!("읽은 결과의 최신 상태를 확인할 수 없습니다: {error}"))?;
    atlas::invalidate_snapshot_freshness(&workspace_id);
    Ok(atlas::snapshot_staleness_reasons_cached(
        &snapshot, &workspace, &registry,
    ))
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
    let stale_reasons = atlas::snapshot_staleness_reasons_cached(&snapshot, &workspace, &registry);
    if !stale_reasons.is_empty() {
        return Err(format!(
            "코드/DB 읽기 결과가 최신이 아닙니다: {}",
            stale_reasons.join(", ")
        )
        .into());
    }
    let change_intent = normalized_change_intent(change_intent)?;

    if enrich_code_evidence.unwrap_or(false)
        && matches!(mode.as_str(), "api-flow" | "table-usage" | "column-impact")
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
    if mode == "api-flow" {
        enrich_api_code_evidence(app_data_dir, registry, workspace_id, focus_id, snapshot);
        return;
    }
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
    let evidence_target_id = column
        .as_ref()
        .map_or(table.id.as_str(), |column| column.id.as_str())
        .to_string();
    let Some((matched_files, schema_ambiguous)) = enrich_table_code_evidence(
        app_data_dir,
        registry,
        workspace_id,
        table.id.as_str(),
        evidence_target_id.as_str(),
        snapshot,
    ) else {
        return;
    };
    let Some(column) = column else {
        return;
    };

    let (path_filter, omitted_files) = focused_code_path_filter(&matched_files);
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

fn enrich_api_code_evidence(
    app_data_dir: &Path,
    registry: &EngineRegistry,
    workspace_id: &str,
    focus_id: &str,
    snapshot: &mut InventorySnapshot,
) {
    for target_id in api_code_evidence_target_ids(snapshot, focus_id) {
        let _ = enrich_table_code_evidence(
            app_data_dir,
            registry,
            workspace_id,
            target_id.as_str(),
            target_id.as_str(),
            snapshot,
        );
    }
}

fn api_code_evidence_target_ids(snapshot: &InventorySnapshot, focus_id: &str) -> Vec<String> {
    let map = atlas::visual_map_with_change(
        snapshot,
        Some(focus_id.to_string()),
        "api-flow".to_string(),
        None,
    );
    let Some(answer) = map.api_reading else {
        return Vec::new();
    };
    answer
        .db_candidates
        .into_iter()
        .filter_map(|candidate| candidate.node_id)
        .filter(|target_id| {
            snapshot
                .items
                .iter()
                .any(|item| item.id == *target_id && item.source == "db" && item.kind == "table")
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn enrich_table_code_evidence(
    app_data_dir: &Path,
    registry: &EngineRegistry,
    workspace_id: &str,
    table_id: &str,
    evidence_target_id: &str,
    snapshot: &mut InventorySnapshot,
) -> Option<(Vec<String>, bool)> {
    let table = snapshot
        .items
        .iter()
        .find(|item| item.id == table_id && item.source == "db" && item.kind == "table")
        .cloned()?;
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
            evidence_target_id,
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
            return None;
        }
    };
    let table_evidence = atlas::apply_focused_code_evidence(
        snapshot,
        table.id.as_str(),
        &table_search,
        schema_ambiguous,
    );
    Some((table_evidence.matched_files, schema_ambiguous))
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
    use super::{api_code_evidence_target_ids, focused_code_path_filter, normalized_change_intent};
    use crate::atlas::{ChangeIntent, InventorySnapshot};

    #[test]
    fn api_evidence_search_targets_only_reachable_db_candidates() {
        let snapshot: InventorySnapshot = serde_json::from_value(serde_json::json!({
            "schemaVersion": 2,
            "workspaceId": "shop",
            "savedAt": "1",
            "items": [
                { "id": "code:route", "kind": "api", "name": "/sessions", "layer": "api", "source": "code", "parentId": null, "path": "routes.py" },
                { "id": "code:handler", "kind": "function", "name": "listSessions", "layer": "code", "source": "code", "parentId": null, "path": "routes.py" },
                { "id": "code:repository", "kind": "function", "name": "sessionsRepository", "layer": "code", "source": "code", "parentId": null, "path": "repository.py" },
                { "id": "code:unreachable", "kind": "function", "name": "auditRepository", "layer": "code", "source": "code", "parentId": null, "path": "audit.py" },
                { "id": "db:table:public.sessions", "kind": "table", "name": "sessions", "layer": "db", "source": "db", "parentId": null, "path": null },
                { "id": "db:table:public.audit", "kind": "table", "name": "audit", "layer": "db", "source": "db", "parentId": null, "path": null }
            ],
            "links": [
                { "id": "handles", "from": "code:route", "to": "code:handler", "kind": "code_handle", "truthClass": "confirmed", "direction": "outbound", "engineEdgeType": "HANDLES", "label": null, "evidence": [] },
                { "id": "calls", "from": "code:handler", "to": "code:repository", "kind": "code_call", "truthClass": "confirmed", "direction": "outbound", "engineEdgeType": "CALLS", "label": null, "evidence": [] }
            ]
        }))
        .unwrap();

        assert_eq!(
            api_code_evidence_target_ids(&snapshot, "code:route"),
            vec!["db:table:public.sessions"]
        );
    }

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

#[cfg(test)]
mod command_tests {
    use super::*;

    #[test]
    fn bounded_code_inventory_keeps_totals_and_drops_dangling_relationships() {
        let functions = (0..101)
            .map(|index| {
                serde_json::json!({
                    "id": format!("function-{index}"),
                    "kind": "function",
                    "name": format!("function_{index}"),
                    "detail": {}
                })
            })
            .collect::<Vec<_>>();
        let inventory: CodeInventory = serde_json::from_value(serde_json::json!({
            "project": "test",
            "routes": [],
            "services": [],
            "files": [],
            "handlers": [],
            "repositories": [],
            "functions": functions,
            "classes": [],
            "modules": [],
            "unknown": [],
            "summary": {
                "routes": 0,
                "handlers": 0,
                "services": 0,
                "repositories": 0,
                "functions": 101,
                "classes": 0,
                "modules": 0,
                "files": 0,
                "unknown": 0
            },
            "architecture": null,
            "calls": [{ "from": "function-0", "to": "function-100" }],
            "handles": []
        }))
        .unwrap();

        let bounded = bounded_code_inventory(inventory);
        assert_eq!(bounded.functions.len(), 100);
        assert_eq!(bounded.summary.functions, 101);
        assert!(bounded.partial);
        assert!(bounded.calls.is_empty());
    }

    #[test]
    fn bounded_db_inventory_keeps_exact_engine_totals() {
        let tables = (0..101)
            .map(|index| {
                serde_json::json!({
                    "key": format!("sqlite:test:main:main:table:table_{index}"),
                    "schema": "main",
                    "name": format!("table_{index}"),
                    "columns": []
                })
            })
            .collect::<Vec<_>>();
        let inventory: DbInventory = serde_json::from_value(serde_json::json!({
            "profileId": "test",
            "tables": tables,
            "resultCount": 101,
            "totalTables": 101,
            "truncated": false
        }))
        .unwrap();

        let bounded = bounded_db_inventory(inventory);
        assert_eq!(bounded.tables.len(), 100);
        assert_eq!(bounded.result_count, Some(101));
        assert_eq!(bounded.total_tables, Some(101));
        assert_eq!(bounded.truncated, Some(false));
    }
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
            index_code_repository,
            load_inventory_bootstrap,
            search_inventory,
            refresh_snapshot_freshness,
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
