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
    let _mutation_guard = begin_workspace_mutation(&app_data_dir, &request.workspace_id)?;
    let previous_workspace = workspace::open_workspace(&app_data_dir, &request.workspace_id)?;
    let previous_snapshot =
        atlas::load_inventory_snapshot_optional(&app_data_dir, &request.workspace_id)?;

    let workspace = workspace::save_db_profile(&app_data_dir, request)?;
    // A saved profile may point to a different database/file than the one
    // represented by the stored DB inventory. Keep code data, but discard the
    // DB source before the next bootstrap can restore it as current.
    if let Err(error) = atlas::remove_db_inventory_snapshot(&app_data_dir, &workspace.id) {
        let mut rollback_errors = Vec::new();
        if let Err(rollback) = workspace::write_workspace(
            &base_paths(&app_data_dir).workspaces_dir,
            &previous_workspace,
        ) {
            rollback_errors.push(format!("workspace 복구 실패: {rollback}"));
        }
        if let Err(rollback) =
            restore_inventory_snapshot(&app_data_dir, &workspace.id, previous_snapshot.as_ref())
        {
            rollback_errors.push(format!("snapshot 복구 실패: {rollback}"));
        }
        return Err(format_with_rollback_error(error, rollback_errors).into());
    }
    Ok(workspace)
}

#[tauri::command(async)]
fn index_db_profile(
    app: tauri::AppHandle,
    request: IndexDbProfileRequest,
) -> CommandResult<DbIndexResult> {
    let app_data_dir = app_data_dir(&app)?;
    let _mutation_guard = begin_workspace_mutation(&app_data_dir, &request.workspace_id)?;
    let registry = get_engine_availability(app)?;
    let workspace_id = request.workspace_id.clone();
    let profile_id = request.profile_id.clone();

    let previous_workspace = workspace::open_workspace(&app_data_dir, &workspace_id)?;
    let previous_snapshot = atlas::load_inventory_snapshot_optional(&app_data_dir, &workspace_id)?;
    let mut result =
        workspace::index_db_profile_without_persisting(&app_data_dir, &registry, request)?;
    if result.run.ok {
        match workspace::db_inventory(&app_data_dir, &registry, &workspace_id, Some(&profile_id)) {
            Ok(inventory) => {
                match persist_db_inventory(&app_data_dir, &result.workspace, &registry, &inventory)
                {
                    Ok(()) => {
                        if let Err(error) = workspace::write_workspace(
                            &base_paths(&app_data_dir).workspaces_dir,
                            &result.workspace,
                        ) {
                            let mut rollback_errors = Vec::new();
                            if let Err(rollback) = restore_inventory_snapshot(
                                &app_data_dir,
                                &workspace_id,
                                previous_snapshot.as_ref(),
                            ) {
                                rollback_errors.push(format!("snapshot 복구 실패: {rollback}"));
                            }
                            result.workspace = previous_workspace.clone();
                            result.inventory_error =
                                Some(format_with_rollback_error(error, rollback_errors));
                        } else {
                            result.inventory = Some(bounded_db_inventory(inventory));
                        }
                    }
                    Err(error) => {
                        let mut rollback_errors = Vec::new();
                        if let Err(rollback) = restore_inventory_snapshot(
                            &app_data_dir,
                            &workspace_id,
                            previous_snapshot.as_ref(),
                        ) {
                            rollback_errors.push(format!("snapshot 복구 실패: {rollback}"));
                        }
                        result.workspace = previous_workspace.clone();
                        result.inventory_error =
                            Some(format_with_rollback_error(error, rollback_errors));
                    }
                }
            }
            Err(error) => {
                result.workspace = previous_workspace.clone();
                result.inventory_error = Some(error);
            }
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
    let _mutation_guard = begin_workspace_mutation(&app_data_dir, &request.workspace_id)?;
    let registry = get_engine_availability(app.clone())?;
    let workspace_id = request.workspace_id.clone();
    let observer = code_progress_observer(app.clone(), workspace_id.clone(), false);
    emit_analysis_progress(
        &app,
        &workspace_id,
        "prepare",
        0,
        1,
        2,
        "코드 분석 준비 중",
    );

    let previous_workspace = workspace::open_workspace(&app_data_dir, &workspace_id)?;
    let previous_snapshot = atlas::load_inventory_snapshot_optional(&app_data_dir, &workspace_id)?;
    let mut result = workspace::index_code_repository_without_persisting_with_observer(
        &app_data_dir,
        &registry,
        request,
        observer,
    )?;
    if result.run.ok {
        let inventory = result
            .inventory
            .take()
            .ok_or_else(|| "코드 분석 결과 inventory가 없습니다".to_string());
        match inventory {
            Ok(inventory) => match persist_code_inventory(
                &app_data_dir,
                &result.workspace,
                &registry,
                &inventory,
            ) {
                Ok(()) => {
                    if let Err(error) = workspace::write_workspace(
                        &base_paths(&app_data_dir).workspaces_dir,
                        &result.workspace,
                    ) {
                        let new_project = result.workspace.code_project.clone();
                        let mut rollback_errors = Vec::new();
                        if let Err(rollback) = restore_inventory_snapshot(
                            &app_data_dir,
                            &workspace_id,
                            previous_snapshot.as_ref(),
                        ) {
                            rollback_errors.push(format!("snapshot 복구 실패: {rollback}"));
                        }
                        workspace::cleanup_code_project(
                            &app_data_dir,
                            &registry,
                            &workspace_id,
                            new_project.as_deref(),
                        );
                        result.workspace = previous_workspace.clone();
                        result.inventory_error =
                            Some(format_with_rollback_error(error, rollback_errors));
                    } else {
                        workspace::cleanup_previous_code_project(
                            &app_data_dir,
                            &registry,
                            &workspace_id,
                            result.previous_code_project.as_deref(),
                            result.workspace.code_project.as_deref(),
                        );
                        result.inventory = Some(bounded_code_inventory(inventory));
                    }
                }
                Err(error) => {
                    workspace::cleanup_code_project(
                        &app_data_dir,
                        &registry,
                        &workspace_id,
                        result.workspace.code_project.as_deref(),
                    );
                    let mut rollback_errors = Vec::new();
                    if let Err(rollback) = restore_inventory_snapshot(
                        &app_data_dir,
                        &workspace_id,
                        previous_snapshot.as_ref(),
                    ) {
                        rollback_errors.push(format!("snapshot 복구 실패: {rollback}"));
                    }
                    result.workspace = previous_workspace.clone();
                    result.inventory_error =
                        Some(format_with_rollback_error(error, rollback_errors));
                }
            },
            Err(error) => {
                workspace::cleanup_code_project(
                    &app_data_dir,
                    &registry,
                    &workspace_id,
                    result.workspace.code_project.as_deref(),
                );
                result.workspace = previous_workspace.clone();
                result.inventory_error = Some(error);
            }
        }
    }
    Ok(result)
}

#[tauri::command(async)]
fn initialize_workspace_analysis(
    app: tauri::AppHandle,
    request: InitializeWorkspaceAnalysisRequest,
) -> CommandResult<InitializeWorkspaceAnalysisResult> {
    validate_workspace_id(&request.workspace_id)?;
    if request.analysis_mode.includes_db() {
        let Some(profile_id) = request.db_profile_id.as_deref() else {
            return Err("DB 분석 모드에는 DB 프로필이 필요합니다".to_string().into());
        };
        validate_workspace_id(profile_id)?;
    }

    let workspace_id = request.workspace_id.clone();
    let app_data_dir = app_data_dir(&app)?;
    let _mutation_guard = begin_workspace_mutation(&app_data_dir, &workspace_id)?;
    let registry = get_engine_availability(app.clone())?;
    emit_analysis_progress(
        &app,
        &workspace_id,
        "prepare",
        0,
        1,
        2,
        "분석 실행 계획 준비 중",
    );
    let code_request = IndexCodeRequest {
        workspace_id: workspace_id.clone(),
    };
    let db_request = request
        .db_profile_id
        .clone()
        .map(|profile_id| IndexDbProfileRequest {
            workspace_id: workspace_id.clone(),
            profile_id,
            connection_string: request.connection_string.clone(),
        });

    let code_app_data_dir = app_data_dir.clone();
    let db_app_data_dir = app_data_dir.clone();
    let code_registry = registry.clone();
    let db_registry = registry.clone();
    let code_observer = code_progress_observer(
        app.clone(),
        workspace_id.clone(),
        request.analysis_mode.includes_db(),
    );
    let db_app = app.clone();
    let db_workspace_id = workspace_id.clone();
    let (code_result, db_result) = thread::scope(|scope| {
        let code_handle = request.analysis_mode.includes_code().then(|| {
            scope.spawn(|| {
                workspace::index_code_repository_without_persisting_with_observer(
                    &code_app_data_dir,
                    &code_registry,
                    code_request,
                    code_observer,
                )
            })
        });
        let db_handle = request.analysis_mode.includes_db().then(|| {
            let db_request = db_request.expect("DB mode validated its profile");
            scope.spawn(|| {
                emit_analysis_progress(
                    &db_app,
                    &db_workspace_id,
                    "db-index",
                    0,
                    1,
                    8,
                    "DB 구조 읽는 중",
                );
                let result = workspace::index_db_profile_without_persisting(
                    &db_app_data_dir,
                    &db_registry,
                    db_request,
                );
                emit_analysis_progress(
                    &db_app,
                    &db_workspace_id,
                    "db-index",
                    1,
                    1,
                    70,
                    "DB 구조 읽기 완료",
                );
                result
            })
        });
        let code_result = code_handle.map(|handle| {
            handle
                .join()
                .map_err(|_| "코드 분석 작업이 비정상 종료되었습니다".to_string())
                .and_then(|result| result)
        });
        let db_result = db_handle.map(|handle| {
            handle
                .join()
                .map_err(|_| "DB 분석 작업이 비정상 종료되었습니다".to_string())
                .and_then(|result| result)
        });
        (code_result, db_result)
    });
    emit_analysis_progress(
        &app,
        &workspace_id,
        "reduce",
        1,
        1,
        82,
        "코드와 DB 결과 통합 중",
    );

    let mut code_error = code_result
        .as_ref()
        .and_then(|result| result.as_ref().err().cloned());
    let mut db_error = db_result
        .as_ref()
        .and_then(|result| result.as_ref().err().cloned());
    let code = code_result.and_then(Result::ok);
    let mut db = db_result.and_then(Result::ok);
    let mut workspace = workspace::open_workspace(&app_data_dir, &workspace_id)?;
    let previous_workspace = workspace.clone();
    let mut code_inventory = None;
    let mut db_inventory = None;

    if let Some(result) = code.as_ref() {
        if result.run.ok {
            code_inventory = result.inventory.clone();
            if code_inventory.is_none() {
                code_inventory =
                    workspace::code_inventory(&app_data_dir, &registry, &workspace_id).ok();
            }
            if code_inventory.is_none() {
                code_error = Some(
                    result
                        .inventory_error
                        .clone()
                        .unwrap_or_else(|| "코드 inventory를 만들지 못했습니다".to_string()),
                );
            }
            workspace.code_project = result.workspace.code_project.clone();
            workspace.engine_cache.code_cache_path =
                result.workspace.engine_cache.code_cache_path.clone();
            workspace.engine_cache.db_cache_dir =
                result.workspace.engine_cache.db_cache_dir.clone();
            workspace.updated_at = result.workspace.updated_at.clone();
        } else if code_error.is_none() {
            code_error = Some(if result.run.stderr.trim().is_empty() {
                "코드 분석에 실패했습니다".to_string()
            } else {
                result.run.stderr.clone()
            });
        }
    }

    if let Some(result) = db.as_ref() {
        if result.run.ok {
            let profile_id = request
                .db_profile_id
                .as_deref()
                .ok_or_else(|| "DB 프로필이 필요합니다".to_string())?;
            db_inventory =
                workspace::db_inventory(&app_data_dir, &registry, &workspace_id, Some(profile_id))
                    .ok();
            if db_inventory.is_none() {
                db_error = Some("DB inventory를 만들지 못했습니다".to_string());
            }
            if let Some(updated_profile) = result
                .workspace
                .db_profiles
                .iter()
                .find(|profile| profile.id == profile_id)
                .cloned()
            {
                if let Some(profile) = workspace
                    .db_profiles
                    .iter_mut()
                    .find(|profile| profile.id == profile_id)
                {
                    profile.last_indexed_at = updated_profile.last_indexed_at;
                }
            }
            workspace.engine_cache.db_cache_dir =
                result.workspace.engine_cache.db_cache_dir.clone();
            workspace.updated_at = result.workspace.updated_at.clone();
        } else if db_error.is_none() {
            db_error = Some(if result.run.stderr.trim().is_empty() {
                "DB 분석에 실패했습니다".to_string()
            } else {
                result.run.stderr.clone()
            });
        }
    }

    let required_sources_ready = request
        .analysis_mode
        .required_sources_ready(code_inventory.is_some(), db_inventory.is_some());
    let mut snapshot_saved = false;
    if required_sources_ready {
        let previous_snapshot =
            atlas::load_inventory_snapshot_optional(&app_data_dir, &workspace.id)?;
        let mut merged = previous_snapshot.clone();
        if let Some(inventory) = code_inventory.as_ref() {
            let incoming = atlas::snapshot_with_metadata(
                atlas::build_inventory_snapshot(workspace.id.clone(), Some(inventory), None),
                &workspace,
                &registry,
            );
            merged = match atlas::replace_inventory_source(merged, incoming, "code") {
                Ok(snapshot) => Some(snapshot),
                Err(error) => {
                    if let Some(result) = code.as_ref() {
                        workspace::cleanup_code_project(
                            &app_data_dir,
                            &registry,
                            &workspace_id,
                            result.workspace.code_project.as_deref(),
                        );
                    }
                    return Err(error.into());
                }
            };
        }
        if let Some(inventory) = db_inventory.as_ref() {
            let incoming = atlas::snapshot_with_metadata(
                atlas::build_inventory_snapshot(workspace.id.clone(), None, Some(inventory)),
                &workspace,
                &registry,
            );
            merged = match atlas::replace_inventory_source(merged, incoming, "db") {
                Ok(snapshot) => Some(snapshot),
                Err(error) => {
                    if let Some(result) = code.as_ref() {
                        workspace::cleanup_code_project(
                            &app_data_dir,
                            &registry,
                            &workspace_id,
                            result.workspace.code_project.as_deref(),
                        );
                    }
                    return Err(error.into());
                }
            };
        }
        if let Some(snapshot) = merged {
            let mut snapshot = snapshot;
            emit_analysis_progress(
                &app,
                &workspace_id,
                "snapshot",
                0,
                1,
                90,
                "통합 스냅샷 검증 중",
            );
            enrich_integrated_snapshot_code_evidence(&workspace, &mut snapshot);
            if let Err(error) = validate_candidate_sources(&snapshot, request.analysis_mode) {
                if let Some(result) = code.as_ref() {
                    workspace::cleanup_code_project(
                        &app_data_dir,
                        &registry,
                        &workspace_id,
                        result.workspace.code_project.as_deref(),
                    );
                }
                return Err(error.into());
            }
            if let Err(error) = atlas::save_inventory_snapshot(&app_data_dir, &snapshot) {
                if let Some(result) = code.as_ref() {
                    workspace::cleanup_code_project(
                        &app_data_dir,
                        &registry,
                        &workspace_id,
                        result.workspace.code_project.as_deref(),
                    );
                }
                return Err(error.into());
            }
            // Commit workspace metadata only after the matching snapshot is
            // durable. If snapshot persistence fails, the previous workspace
            // still points at the previous readable result.
            if let Err(error) =
                workspace::write_workspace(&base_paths(&app_data_dir).workspaces_dir, &workspace)
            {
                let mut rollback_errors = Vec::new();
                if let Err(rollback) = restore_inventory_snapshot(
                    &app_data_dir,
                    &workspace.id,
                    previous_snapshot.as_ref(),
                ) {
                    rollback_errors.push(format!("snapshot 복구 실패: {rollback}"));
                }
                if let Some(result) = code.as_ref() {
                    workspace::cleanup_code_project(
                        &app_data_dir,
                        &registry,
                        &workspace_id,
                        result.workspace.code_project.as_deref(),
                    );
                }
                return Err(format_with_rollback_error(error, rollback_errors).into());
            }
            if let Some(result) = code.as_ref() {
                workspace::cleanup_previous_code_project(
                    &app_data_dir,
                    &registry,
                    &workspace_id,
                    result.previous_code_project.as_deref(),
                    result.workspace.code_project.as_deref(),
                );
            }
            snapshot_saved = true;
            emit_analysis_progress(
                &app,
                &workspace_id,
                "complete",
                1,
                1,
                100,
                "분석과 시각화 데이터 준비 완료",
            );
        }
    } else {
        if let Some(result) = code.as_ref().filter(|result| result.run.ok) {
            workspace::cleanup_code_project(
                &app_data_dir,
                &registry,
                &workspace_id,
                result.workspace.code_project.as_deref(),
            );
        }
        workspace = previous_workspace;
    }

    let mut code = code;
    if let Some(result) = code.as_mut() {
        if !snapshot_saved {
            result.workspace = workspace.clone();
        }
        if let Some(inventory) = result.inventory.take() {
            result.inventory = Some(bounded_code_inventory(inventory));
        }
    }
    if let Some(result) = db.as_mut() {
        if !snapshot_saved {
            result.workspace = workspace.clone();
        }
        if let Some(inventory) = db_inventory {
            result.inventory = Some(bounded_db_inventory(inventory));
        }
    }

    Ok(InitializeWorkspaceAnalysisResult {
        workspace,
        code,
        db,
        code_error,
        db_error,
        snapshot_saved,
    })
}

fn validate_candidate_sources(
    snapshot: &InventorySnapshot,
    mode: AnalysisSourceMode,
) -> Result<(), String> {
    if mode.includes_code() && snapshot.metadata.code.is_none() {
        return Err("완성 후보에 코드 분석 결과가 없습니다".to_string());
    }
    if mode.includes_db() && snapshot.metadata.db.is_none() {
        return Err("완성 후보에 DB 분석 결과가 없습니다".to_string());
    }
    Ok(())
}
