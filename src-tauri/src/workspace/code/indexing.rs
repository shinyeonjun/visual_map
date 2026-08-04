fn index_code_repository_with_persistence(
    app_data_dir: impl AsRef<Path>,
    registry: &EngineRegistry,
    request: IndexCodeRequest,
    persist_workspace: bool,
) -> Result<CodeIndexResult, String> {
    validate_workspace_id(&request.workspace_id)?;

    let paths = base_paths(app_data_dir);
    let mut workspace = read_workspace_by_id(&paths.workspaces_dir, &request.workspace_id)?;
    let code_cache_path = workspace_code_cache_path(&paths.workspaces_dir, &request.workspace_id);
    fs::create_dir_all(&code_cache_path).map_err(|error| error.to_string())?;
    workspace.engine_cache.code_cache_path = Some(code_cache_path.display().to_string());
    workspace.engine_cache.db_cache_dir = Some(
        workspace_db_cache_dir(&paths.workspaces_dir, &request.workspace_id)
            .display()
            .to_string(),
    );
    let adapter = CodebaseMemoryAdapter::new_with_provider_cache(
        registry,
        &code_cache_path,
        paths.app_data_dir.join("providers"),
    )?;
    let previous_project = workspace
        .code_project
        .clone()
        .unwrap_or_else(|| workspace.name.clone());
    let requested_project = next_code_project_generation();
    let mut run = adapter.index_repository(&workspace.repo_path, &requested_project)?;
    let mut inventory = None;
    let mut inventory_error = None;

    if run.ok {
        let project = code_project_from_index_stdout(&run.stdout, &requested_project);
        match code_inventory_from_adapter(&adapter, project.clone(), &workspace.repo_path) {
            Ok(indexed_inventory) => {
                workspace.code_project = Some(project.clone());
                workspace.updated_at = timestamp();
                if persist_workspace {
                    if let Err(error) = write_workspace(&paths.workspaces_dir, &workspace) {
                        let _ = adapter.delete_project(&project);
                        return Err(error);
                    }
                }
                if persist_workspace && previous_project != project {
                    let _ = adapter.delete_project(&previous_project);
                }
                inventory = Some(indexed_inventory);
            }
            Err(error) => {
                let _ = adapter.delete_project(&project);
                run.ok = false;
                run.stderr = format!("새 코드 인덱스를 검증하지 못했습니다: {error}");
                inventory_error = Some(error);
            }
        }
    } else {
        let project = code_project_from_index_stdout(&run.stdout, &requested_project);
        let _ = adapter.delete_project(&project);
    }

    Ok(CodeIndexResult {
        workspace,
        run,
        inventory,
        inventory_error,
        previous_code_project: (!persist_workspace).then_some(previous_project),
    })
}

pub(crate) fn cleanup_previous_code_project(
    app_data_dir: impl AsRef<Path>,
    registry: &EngineRegistry,
    workspace_id: &str,
    previous_project: Option<&str>,
    current_project: Option<&str>,
) {
    let Some(previous_project) = previous_project else {
        return;
    };
    if current_project == Some(previous_project) {
        return;
    }
    cleanup_code_project(app_data_dir, registry, workspace_id, Some(previous_project));
}

pub(crate) fn cleanup_code_project(
    app_data_dir: impl AsRef<Path>,
    registry: &EngineRegistry,
    workspace_id: &str,
    project: Option<&str>,
) {
    let Some(project) = project else {
        return;
    };
    let paths = base_paths(app_data_dir);
    let cache_path = workspace_code_cache_path(&paths.workspaces_dir, workspace_id);
    if let Ok(adapter) = CodebaseMemoryAdapter::new_with_provider_cache(
        registry,
        cache_path,
        paths.app_data_dir.join("providers"),
    ) {
        let _ = adapter.delete_project(project);
    }
}

pub(crate) fn code_inventory(
    app_data_dir: impl AsRef<Path>,
    registry: &EngineRegistry,
    workspace_id: &str,
) -> Result<CodeInventory, String> {
    validate_workspace_id(workspace_id)?;

    let paths = base_paths(app_data_dir);
    let workspace = read_workspace_by_id(&paths.workspaces_dir, workspace_id)?;
    let code_cache_path = workspace_code_cache_path(&paths.workspaces_dir, workspace_id);
    let project = workspace
        .code_project
        .clone()
        .unwrap_or_else(|| workspace.name.clone());
    let adapter = CodebaseMemoryAdapter::new_with_provider_cache(
        registry,
        code_cache_path,
        paths.app_data_dir.join("providers"),
    )?;
    code_inventory_from_adapter(&adapter, project, &workspace.repo_path)
}

fn code_inventory_from_adapter(
    adapter: &CodebaseMemoryAdapter<'_>,
    project: String,
    repo_path: &str,
) -> Result<CodeInventory, String> {
    let result: CodebaseMemoryInventory = adapter.inventory(&project)?;
    let (routes, services, files) = split_inventory_nodes(&result.nodes)?;
    let mut inventory = extract_code_inventory(
        project,
        Some(result.architecture),
        &routes,
        &services,
        &files,
    )?;
    inventory.evidence = Some(result.evidence);
    inventory.relation_gaps.extend(architecture_diagnostics(
        inventory.architecture.as_ref(),
        &inventory.project,
    ));
    let (calls, call_gaps) = extract_code_calls_with_gaps(&result.calls, &inventory);
    inventory.calls = calls;
    inventory.relation_gaps.extend(call_gaps);
    attach_code_handles(&result.handles, &mut inventory);
    assign_structural_roles(&mut inventory);
    super::fastapi_routes::enrich_fastapi_evidence(repo_path, &mut inventory);
    super::fastendpoints_routes::enrich_fastendpoints_routes(repo_path, &mut inventory);
    match extract_client_requests(repo_path, &inventory) {
        Ok(scan) => {
            let unknown_count = scan
                .requests
                .iter()
                .filter(|request| request.resolution == "unknown")
                .count();
            inventory.client_requests = scan.requests;
            if unknown_count > 0 {
                inventory.relation_gaps.push(CodeInventoryGap::new(
                    "client-request-unresolved",
                    format!("provider:{}", inventory.project),
                    inventory.project.clone(),
                    format!(
                        "클라이언트 요청 {unknown_count}개는 URL 또는 method를 정적으로 해석하지 못해 서버 API에 연결하지 않았습니다."
                    ),
                ));
            }
            if scan.truncated {
                inventory.relation_gaps.push(CodeInventoryGap::new(
                    "client-request-scan-bounded",
                    format!("provider:{}", inventory.project),
                    inventory.project.clone(),
                    format!(
                        "클라이언트 요청 스캔에서 {}개 파일({} bytes)을 안전 한도 또는 읽기 오류로 제외했습니다. 제외된 파일의 요청은 확인되지 않은 상태입니다.",
                        scan.skipped_files, scan.skipped_bytes
                    ),
                ));
            }
        }
        Err(error) => inventory.relation_gaps.push(CodeInventoryGap::new(
            "client-request-scan",
            format!("provider:{}", inventory.project),
            inventory.project.clone(),
            error,
        )),
    }
    downgrade_unverified_routes(&mut inventory);
    inventory.partial = !inventory.relation_gaps.is_empty();
    Ok(inventory)
}
