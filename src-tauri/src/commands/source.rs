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

