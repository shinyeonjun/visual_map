pub(crate) fn create_workspace(
    app_data_dir: impl AsRef<Path>,
    request: CreateWorkspaceRequest,
) -> Result<Workspace, String> {
    let name = request.name.trim();
    let requested_repo_path = request.repo_path.trim();

    if name.is_empty() {
        return Err("프로젝트 이름이 필요합니다".to_string());
    }
    if requested_repo_path.is_empty() {
        return Err("프로젝트 경로가 필요합니다".to_string());
    }

    let paths = base_paths(app_data_dir);
    ensure_base_dirs(&paths).map_err(|error| error.to_string())?;

    let now = timestamp();
    let id = workspace_id(name);
    let (repo_path, repo_source, repo_origin) = if is_remote_url(requested_repo_path) {
        let Some(_) = github_repo_name(requested_repo_path) else {
            return Err("지원하는 GitHub URL 형식이 아닙니다".to_string());
        };
        let target = workspace_repo_dir(&paths.workspaces_dir, &id);
        clone_github_repo(requested_repo_path, &target)?;
        (
            path_for_storage(&target),
            RepoSource::Github,
            Some(requested_repo_path.to_string()),
        )
    } else {
        (
            canonical_local_repo_path(requested_repo_path)?,
            RepoSource::Local,
            None,
        )
    };
    let workspace = Workspace {
        id: id.clone(),
        name: name.to_string(),
        repo_path,
        repo_source,
        repo_origin,
        code_project: None,
        engine_cache: WorkspaceEngineCache {
            code_cache_path: Some(
                workspace_code_cache_path(&paths.workspaces_dir, &id)
                    .display()
                    .to_string(),
            ),
            db_cache_dir: Some(
                workspace_db_cache_dir(&paths.workspaces_dir, &id)
                    .display()
                    .to_string(),
            ),
        },
        db_profiles: Vec::new(),
        active_db_profile_id: None,
        created_at: now.clone(),
        updated_at: now,
    };

    write_workspace(&paths.workspaces_dir, &workspace)?;

    Ok(workspace)
}

pub(crate) fn refresh_github_workspace(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
) -> Result<Workspace, String> {
    validate_workspace_id(workspace_id)?;
    let paths = base_paths(app_data_dir);
    let mut workspace = read_workspace_by_id(&paths.workspaces_dir, workspace_id)?;
    if workspace.repo_source != RepoSource::Github {
        return Err("앱이 복제한 GitHub 프로젝트만 업데이트할 수 있습니다".to_string());
    }

    let expected_repo = workspace_repo_dir(&paths.workspaces_dir, workspace_id);
    let actual_repo = fs::canonicalize(&workspace.repo_path)
        .map_err(|error| format!("GitHub 복제본을 찾을 수 없습니다: {error}"))?;
    let expected_repo = fs::canonicalize(expected_repo)
        .map_err(|error| format!("관리 GitHub 복제본을 찾을 수 없습니다: {error}"))?;
    if actual_repo != expected_repo || !actual_repo.join(".git").exists() {
        return Err("앱이 관리하는 GitHub 복제본 경로가 아닙니다".to_string());
    }

    let repo = path_for_storage(&actual_repo);
    let status = run_git(
        &["-C", repo.as_str(), "status", "--porcelain"],
        Duration::from_secs(30),
    )?;
    if !status.ok {
        return Err(git_failure("GitHub 프로젝트 상태 확인 실패", &status));
    }
    if !status.stdout.trim().is_empty() {
        return Err(
            "로컬 변경이 있어 업데이트를 중단했습니다. 변경을 커밋하거나 별도로 보관한 뒤 다시 시도하세요"
                .to_string(),
        );
    }

    let pull = run_git(
        &["-C", repo.as_str(), "pull", "--ff-only"],
        Duration::from_secs(180),
    )?;
    if !pull.ok {
        return Err(git_failure("GitHub 프로젝트 업데이트 실패", &pull));
    }

    workspace.repo_path = repo;
    workspace.updated_at = timestamp();
    write_workspace(&paths.workspaces_dir, &workspace)?;
    Ok(workspace)
}

pub(crate) fn open_workspace(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
) -> Result<Workspace, String> {
    let paths = base_paths(app_data_dir);
    read_workspace_by_id(&paths.workspaces_dir, workspace_id)
}

pub(crate) fn list_workspaces(app_data_dir: impl AsRef<Path>) -> Result<Vec<Workspace>, String> {
    let paths = base_paths(app_data_dir);
    ensure_base_dirs(&paths).map_err(|error| error.to_string())?;

    let mut workspaces = Vec::new();

    for entry in fs::read_dir(&paths.workspaces_dir).map_err(|error| error.to_string())? {
        let Ok(entry) = entry else {
            continue;
        };
        let Some(workspace_id) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if validate_workspace_id(&workspace_id).is_err()
            || (!workspace_file(&paths.workspaces_dir, &workspace_id).is_file()
                && !workspace_backup_file(&paths.workspaces_dir, &workspace_id).is_file())
        {
            continue;
        }
        if let Ok(workspace) = read_workspace_by_id(&paths.workspaces_dir, &workspace_id) {
            workspaces.push(workspace);
        }
    }

    workspaces.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(workspaces)
}

pub(crate) fn delete_workspace(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
) -> Result<(), String> {
    validate_workspace_id(workspace_id)?;
    let paths = base_paths(app_data_dir);
    let workspace_dir = paths.workspaces_dir.join(workspace_id);
    if !workspace_file(&paths.workspaces_dir, workspace_id).is_file()
        && !workspace_backup_file(&paths.workspaces_dir, workspace_id).is_file()
    {
        return Err("삭제할 프로젝트를 찾을 수 없습니다".to_string());
    }
    fs::remove_dir_all(workspace_dir)
        .map_err(|error| format!("프로젝트 메타데이터를 삭제하지 못했습니다: {error}"))
}

pub(crate) fn workspace_recovery_warnings(
    app_data_dir: impl AsRef<Path>,
) -> Result<Vec<WorkspaceRecoveryWarning>, String> {
    let paths = base_paths(app_data_dir);
    ensure_base_dirs(&paths).map_err(|error| error.to_string())?;
    let mut warnings = Vec::new();

    for entry in fs::read_dir(&paths.workspaces_dir).map_err(|error| error.to_string())? {
        let Ok(entry) = entry else {
            continue;
        };
        let Some(workspace_id) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if validate_workspace_id(&workspace_id).is_err() {
            warnings.push(WorkspaceRecoveryWarning {
                workspace_id,
                kind: "unrecoverable".to_string(),
                message: "프로젝트 폴더 이름이 올바르지 않아 목록에서 제외했습니다.".to_string(),
                action: "recreate-workspace".to_string(),
            });
            continue;
        }

        let primary = workspace_file(&paths.workspaces_dir, &workspace_id);
        let backup = workspace_backup_file(&paths.workspaces_dir, &workspace_id);
        if !primary.exists() && !backup.exists() {
            continue;
        }
        if read_workspace_for_id(&primary, &workspace_id).is_ok() {
            continue;
        }

        if read_workspace_for_id(&backup, &workspace_id).is_ok() {
            warnings.push(WorkspaceRecoveryWarning {
                workspace_id,
                kind: "backup-recovered".to_string(),
                message: "workspace.json을 열 수 없어 보존된 백업으로 프로젝트를 열었습니다."
                    .to_string(),
                action: "repair-from-backup".to_string(),
            });
        } else {
            warnings.push(WorkspaceRecoveryWarning {
                workspace_id,
                kind: "unrecoverable".to_string(),
                message:
                    "workspace.json과 백업을 모두 열 수 없어 이 프로젝트를 목록에서 제외했습니다."
                        .to_string(),
                action: "recreate-workspace".to_string(),
            });
        }
    }

    warnings.sort_by(|left, right| left.workspace_id.cmp(&right.workspace_id));
    Ok(warnings)
}

pub(crate) fn repair_workspace_from_backup(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
) -> Result<Workspace, String> {
    validate_workspace_id(workspace_id)?;
    let paths = base_paths(app_data_dir);
    let primary = workspace_file(&paths.workspaces_dir, workspace_id);
    if let Ok(workspace) = read_workspace_for_id(&primary, workspace_id) {
        return Ok(workspace);
    }

    let workspace = read_workspace_for_id(
        &workspace_backup_file(&paths.workspaces_dir, workspace_id),
        workspace_id,
    )?;
    write_workspace(&paths.workspaces_dir, &workspace)?;
    Ok(workspace)
}

pub(crate) fn value_items(value: &serde_json::Value) -> Vec<&serde_json::Value> {
    if let Some(items) = value.as_array() {
        return items.iter().collect();
    }
    for key in ["items", "results", "nodes", "matches", "tables", "columns"] {
        if let Some(items) = value.get(key).and_then(serde_json::Value::as_array) {
            return items.iter().collect();
        }
    }

    Vec::new()
}

pub(crate) fn object_string(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(key).and_then(serde_json::Value::as_str))
        .map(str::to_string)
}

pub(crate) fn object_bool(value: &serde_json::Value, keys: &[&str]) -> bool {
    keys.iter()
        .find_map(|key| value.get(key).and_then(serde_json::Value::as_bool))
        .unwrap_or(false)
}

pub(crate) fn engine_json_value(stdout: &str) -> Option<serde_json::Value> {
    let trimmed = stdout.trim_start_matches('\u{feff}').trim();
    if let Ok(value) = serde_json::from_str(trimmed) {
        return Some(value);
    }

    let bytes = stdout.as_bytes();
    let line_candidates = json_candidates(bytes, true);
    if line_candidates.len() == 1 {
        return line_candidates.into_iter().next();
    }
    if !line_candidates.is_empty() {
        return None;
    }

    // A same-line log prefix is supported for existing engines, but only when
    // exactly one JSON value can be found. This rejects ambiguous log text
    // instead of silently treating an arbitrary nested object as the payload.
    let candidates = json_candidates(bytes, false);
    (candidates.len() == 1)
        .then(|| candidates.into_iter().next())
        .flatten()
}

fn json_candidates(bytes: &[u8], line_start_only: bool) -> Vec<serde_json::Value> {
    bytes
        .iter()
        .enumerate()
        .filter(|(offset, byte)| {
            matches!(byte, b'{' | b'[') && (!line_start_only || json_starts_at_line(bytes, *offset))
        })
        .filter_map(|(offset, _)| parse_json_prefix(&bytes[offset..]))
        .collect()
}

fn json_starts_at_line(bytes: &[u8], offset: usize) -> bool {
    let line_start = bytes[..offset]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index + 1);
    bytes[line_start..offset]
        .iter()
        .all(|byte| byte.is_ascii_whitespace())
}

fn parse_json_prefix(bytes: &[u8]) -> Option<serde_json::Value> {
    serde_json::Deserializer::from_slice(bytes)
        .into_iter::<serde_json::Value>()
        .next()
        .and_then(Result::ok)
}

