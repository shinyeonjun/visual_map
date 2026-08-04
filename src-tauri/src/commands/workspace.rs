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
    let app_data_dir = app_data_dir(&app)?;
    let _mutation_guard = begin_workspace_mutation(&app_data_dir, &workspace_id)?;
    Ok(workspace::refresh_github_workspace(
        app_data_dir,
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
    let app_data_dir = app_data_dir(&app)?;
    let _mutation_guard = begin_workspace_mutation(&app_data_dir, &workspace_id)?;
    Ok(workspace::repair_workspace_from_backup(
        app_data_dir,
        &workspace_id,
    )?)
}

#[tauri::command(async)]
fn delete_workspace(app: tauri::AppHandle, workspace_id: String) -> CommandResult<()> {
    let app_data_dir = app_data_dir(&app)?;
    let _mutation_guard = begin_workspace_mutation(&app_data_dir, &workspace_id)?;
    Ok(workspace::delete_workspace(app_data_dir, &workspace_id)?)
}

#[tauri::command(async)]
fn delete_db_profile(
    app: tauri::AppHandle,
    workspace_id: String,
    profile_id: String,
) -> CommandResult<Workspace> {
    let app_data_dir = app_data_dir(&app)?;
    let _mutation_guard = begin_workspace_mutation(&app_data_dir, &workspace_id)?;
    let workspace = workspace::open_workspace(&app_data_dir, &workspace_id)?;
    if !workspace
        .db_profiles
        .iter()
        .any(|profile| profile.id == profile_id)
    {
        return Err("삭제할 DB 연결을 찾을 수 없습니다".into());
    }
    // Deleting an inactive profile must not discard the snapshot belonging to
    // the active profile. If the active profile is deleted, the snapshot no
    // longer has a valid source and must be re-read for the replacement.
    if should_remove_db_snapshot(&workspace, &profile_id) {
        let previous_snapshot =
            atlas::load_inventory_snapshot_optional(&app_data_dir, &workspace_id)?;
        if let Err(error) = atlas::remove_db_inventory_snapshot(&app_data_dir, &workspace_id) {
            return Err(error.into());
        }
        match workspace::delete_db_profile(&app_data_dir, &workspace_id, &profile_id) {
            Ok(workspace) => return Ok(workspace),
            Err(error) => {
                let mut rollback_errors = Vec::new();
                if let Err(rollback) = restore_inventory_snapshot(
                    &app_data_dir,
                    &workspace_id,
                    previous_snapshot.as_ref(),
                ) {
                    rollback_errors.push(format!("snapshot 복구 실패: {rollback}"));
                }
                return Err(format_with_rollback_error(error, rollback_errors).into());
            }
        }
    }
    Ok(workspace::delete_db_profile(
        app_data_dir,
        &workspace_id,
        &profile_id,
    )?)
}

fn should_remove_db_snapshot(workspace: &Workspace, profile_id: &str) -> bool {
    workspace.active_db_profile_id.as_deref() == Some(profile_id)
}

#[tauri::command(async)]
fn list_workspaces(app: tauri::AppHandle) -> CommandResult<Vec<Workspace>> {
    let app_data_dir = app_data_dir(&app)?;

    Ok(workspace::list_workspaces(app_data_dir)?)
}
