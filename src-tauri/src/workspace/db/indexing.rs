fn index_db_profile_with_persistence(
    app_data_dir: impl AsRef<Path>,
    registry: &EngineRegistry,
    request: IndexDbProfileRequest,
    persist_workspace: bool,
) -> Result<DbIndexResult, String> {
    validate_workspace_id(&request.workspace_id)?;
    validate_workspace_id(&request.profile_id)?;

    let paths = base_paths(app_data_dir);
    let mut workspace = read_workspace_by_id(&paths.workspaces_dir, &request.workspace_id)?;
    let profile_index = workspace
        .db_profiles
        .iter()
        .position(|profile| profile.id == request.profile_id)
        .ok_or_else(|| "DB 연결을 찾을 수 없습니다".to_string())?;
    let profile = workspace.db_profiles[profile_index].clone();
    workspace.engine_cache.db_cache_dir = Some(
        workspace_db_cache_dir(&paths.workspaces_dir, &request.workspace_id)
            .display()
            .to_string(),
    );
    let cache_path = db_cache_path(&paths.workspaces_dir, &request.workspace_id, &profile.id);

    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let args = db_index_args(&profile, &cache_path, request.connection_string.as_deref())?;
    let adapter = DatabaseMemoryAdapter::new(registry)?;
    let snapshot_key = db_snapshot_alias(&profile)?;

    let run = if db_source_uses_path(&profile.source) {
        adapter.index(&args, &[], &snapshot_key)?
    } else {
        let connection_string = request
            .connection_string
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "DB 연결 문자열이 필요합니다".to_string())?;
        let config_path = db_connection_config_path(&cache_path);
        write_db_connection_config(&profile, &config_path)?;
        let env_name = db_connection_env_var(&profile.id);
        let result = adapter.index(
            &args,
            &[(env_name.as_str(), connection_string)],
            &snapshot_key,
        );
        let cleanup_result = fs::remove_file(&config_path);
        if let Err(error) = cleanup_result {
            return Err(format!(
                "DB 분석 임시 설정 파일을 삭제하지 못했습니다: {error}"
            ));
        }
        result?
    };

    if run.ok {
        workspace.db_profiles[profile_index].last_indexed_at = Some(timestamp());
        workspace.updated_at = timestamp();
        if persist_workspace {
            write_workspace(&paths.workspaces_dir, &workspace)?;
        }
    }

    let index_json = engine_json_value(&run.stdout);

    Ok(DbIndexResult {
        workspace,
        run,
        index_json,
        inventory: None,
        inventory_error: None,
    })
}

pub(crate) fn db_inventory(
    app_data_dir: impl AsRef<Path>,
    registry: &EngineRegistry,
    workspace_id: &str,
    profile_id: Option<&str>,
) -> Result<DbInventory, String> {
    validate_workspace_id(workspace_id)?;
    if let Some(profile_id) = profile_id {
        validate_workspace_id(profile_id)?;
    }

    let paths = base_paths(app_data_dir);
    let workspace = read_workspace_by_id(&paths.workspaces_dir, workspace_id)?;
    let selected_profile_id = profile_id
        .map(str::to_string)
        .or_else(|| workspace.active_db_profile_id.clone())
        .ok_or_else(|| "DB 연결이 필요합니다".to_string())?;
    validate_workspace_id(&selected_profile_id)?;
    let profile = workspace
        .db_profiles
        .iter()
        .find(|profile| profile.id == selected_profile_id)
        .ok_or_else(|| "DB 연결을 찾을 수 없습니다".to_string())?;
    let cache_path = db_cache_path(&paths.workspaces_dir, workspace_id, &profile.id);
    let adapter = DatabaseMemoryAdapter::new(registry)?;
    read_complete_db_inventory(&adapter, profile, &cache_path, selected_profile_id)
}

fn read_complete_db_inventory(
    adapter: &DatabaseMemoryAdapter<'_>,
    profile: &DbProfile,
    cache_path: &Path,
    profile_id: String,
) -> Result<DbInventory, String> {
    let snapshot_key = db_snapshot_alias(profile)?;
    adapter.verify_complete_snapshot(&snapshot_key, cache_path)?;
    let mut offset = 0;
    let mut inventory: Option<DbInventory> = None;
    let mut table_keys = HashSet::new();
    let mut column_keys = HashSet::new();

    loop {
        let value =
            adapter.inventory_page(&snapshot_key, cache_path, offset, DB_INVENTORY_PAGE_LIMIT)?;
        let page = parse_bulk_db_inventory(profile_id.clone(), &value)?;
        validate_complete_inventory_page(&page, &mut table_keys, &mut column_keys)?;

        if let Some(existing) = inventory.as_ref() {
            if existing.snapshot_key != page.snapshot_key
                || existing.contract_version != page.contract_version
                || existing.total_tables != page.total_tables
            {
                return Err(
                    "DB inventory 페이지의 snapshot, 계약 또는 전체 테이블 수가 일치하지 않습니다"
                        .to_string(),
                );
            }
        }

        let page_count = page.tables.len();
        let has_more = value
            .get("has_more")
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| "DB inventory has_more 값이 없습니다".to_string())?;
        let next_offset = value
            .get("next_offset")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| usize::try_from(value).ok());
        if let Some(existing) = inventory.as_mut() {
            existing.tables.extend(page.tables);
            existing
                .capability_warnings
                .extend(page.capability_warnings);
            existing.gaps.extend(page.gaps);
        } else {
            inventory = Some(page);
        }

        let total_tables = inventory
            .as_ref()
            .and_then(|inventory| inventory.total_tables)
            .ok_or_else(|| "DB inventory total_tables 값이 없습니다".to_string())?;
        if total_tables > MAX_DB_INVENTORY_TABLES {
            return Err(format!(
                "DB 테이블 수가 제품 안전 한도 {MAX_DB_INVENTORY_TABLES}개를 초과했습니다: {total_tables}개"
            ));
        }
        if !has_more {
            let mut inventory =
                inventory.ok_or_else(|| "DB inventory가 비어 있습니다".to_string())?;
            if inventory.tables.len() != total_tables {
                return Err(format!(
                    "DB inventory가 완전하지 않습니다: expected {total_tables} tables, got {}",
                    inventory.tables.len()
                ));
            }
            inventory.result_count = Some(inventory.tables.len());
            inventory.truncated = Some(false);
            finalize_db_inventory(&mut inventory);
            if let Some(gap) = inventory.gaps.first() {
                return Err(format!(
                    "DB inventory 계약 검증에 실패했습니다: {}",
                    gap.message
                ));
            }
            return Ok(inventory);
        }
        let next_offset =
            next_offset.ok_or_else(|| "DB inventory 다음 페이지 offset이 없습니다".to_string())?;
        if page_count == 0 || next_offset != offset.saturating_add(page_count) {
            return Err("DB inventory 페이지 offset이 연속적이지 않습니다".to_string());
        }
        offset = next_offset;
    }
}
