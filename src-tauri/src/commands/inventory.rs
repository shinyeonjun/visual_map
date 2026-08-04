#[tauri::command(async)]
fn load_inventory_bootstrap(
    app: tauri::AppHandle,
    workspace_id: String,
) -> CommandResult<Option<atlas::InventoryBootstrap>> {
    let app_data_dir = app_data_dir(&app)?;
    let _workspace = workspace::open_workspace(&app_data_dir, &workspace_id)?;
    let Some(snapshot) =
        atlas::load_inventory_snapshot_optional_cached(&app_data_dir, &workspace_id)?
    else {
        return Ok(None);
    };
    // Load the saved result first. Current source/engine freshness is checked by
    // refresh_snapshot_freshness after the workspace is visible; hashing a large
    // bundled engine must not block restoring an already-computed snapshot.
    let stale_reasons = snapshot.stale_reasons.clone();
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

fn restore_inventory_snapshot(
    app_data_dir: &Path,
    workspace_id: &str,
    previous: Option<&InventorySnapshot>,
) -> Result<(), String> {
    match previous {
        Some(snapshot) => atlas::save_inventory_snapshot(app_data_dir, snapshot),
        None => atlas::remove_inventory_snapshot(app_data_dir, workspace_id),
    }
}

fn format_with_rollback_error(error: String, rollback_errors: Vec<String>) -> String {
    if rollback_errors.is_empty() {
        error
    } else {
        format!("{error} ({})", rollback_errors.join("; "))
    }
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
    let mut merged = atlas::replace_inventory_source(existing, incoming, source)?;
    enrich_integrated_snapshot_code_evidence(workspace, &mut merged);
    atlas::save_inventory_snapshot(app_data_dir, &merged)
}

#[tauri::command(async)]
fn search_inventory(
    app: tauri::AppHandle,
    workspace_id: String,
    query: String,
) -> CommandResult<atlas::InventorySearchResult> {
    let app_data_dir = app_data_dir(&app)?;
    let snapshot = atlas::load_inventory_snapshot_cached(&app_data_dir, &workspace_id)
        .map_err(|error| format!("검색하려면 먼저 코드/DB 읽기 결과가 필요합니다: {error}"))?;
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
#[allow(clippy::too_many_arguments)]
fn get_visual_map(
    app: tauri::AppHandle,
    workspace_id: String,
    focus_id: Option<String>,
    mode: String,
    change_intent: Option<ChangeIntent>,
    enrich_code_evidence: Option<bool>,
    composition: Option<CompositionMapRequest>,
    operation_id: Option<String>,
) -> CommandResult<VisualMap> {
    let _operation_guard = operation_id
        .as_deref()
        .map(engine::begin_engine_operation)
        .transpose()?;
    let app_data_dir = app_data_dir(&app)?;
    let workspace = workspace::open_workspace(&app_data_dir, &workspace_id)?;
    let snapshot = atlas::load_inventory_snapshot_cached(&app_data_dir, &workspace_id)
        .map_err(|error| format!("캔버스를 보려면 먼저 코드/DB 읽기 결과가 필요합니다: {error}"))?;
    let requested_enrichment = enrich_code_evidence.unwrap_or(false);
    let registry = requested_enrichment
        .then(|| get_engine_availability(app.clone()))
        .transpose()?;
    let stale_reasons = registry
        .as_ref()
        .map(|registry| atlas::snapshot_staleness_reasons_cached(&snapshot, &workspace, registry))
        .unwrap_or_else(|| snapshot.stale_reasons.clone());
    let snapshot_is_stale = !stale_reasons.is_empty();
    let snapshot = if snapshot_is_stale {
        let mut stale_snapshot = (*snapshot).clone();
        stale_snapshot.stale_reasons = stale_reasons;
        Cow::Owned(stale_snapshot)
    } else {
        Cow::Borrowed(snapshot.as_ref())
    };
    let allow_live_evidence = enrich_code_evidence.unwrap_or(false) && !snapshot_is_stale;
    let change_intent = normalized_change_intent(change_intent)?;

    if mode == "composition" {
        let composition = composition.unwrap_or_default();
        let focus_ids = composition.focus_ids;
        let relation_view = composition
            .relation_view
            .unwrap_or_else(|| "connections".to_string());
        atlas::validate_composition_request(&snapshot, &focus_ids, &relation_view)?;
        if allow_live_evidence && relation_view != "calls" {
            let mut enriched_snapshot = (*snapshot).clone();
            enrich_composition_code_evidence(
                &app_data_dir,
                &workspace_id,
                &focus_ids,
                &mut enriched_snapshot,
            );
            return atlas::composition_map(&enriched_snapshot, focus_ids, &relation_view)
                .map_err(Into::into);
        }
        return atlas::composition_map(&snapshot, focus_ids, &relation_view).map_err(Into::into);
    }

    if allow_live_evidence
        && matches!(mode.as_str(), "api-flow" | "table-usage" | "column-impact")
        && focus_id.is_some()
    {
        let registry = registry
            .as_ref()
            .ok_or_else(|| "코드 근거 분석 도구 상태를 확인하지 못했습니다".to_string())?;
        let mut enriched_snapshot = (*snapshot).clone();
        enrich_snapshot_code_evidence(
            &app_data_dir,
            registry,
            &workspace_id,
            focus_id.as_deref(),
            &mode,
            &mut enriched_snapshot,
            operation_id.as_deref(),
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

#[tauri::command(async)]
fn cancel_visual_map(operation_id: String) -> CommandResult<bool> {
    Ok(engine::cancel_engine_operation(&operation_id))
}
