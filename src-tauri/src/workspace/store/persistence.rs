pub(crate) fn write_workspace(workspaces_dir: &Path, workspace: &Workspace) -> Result<(), String> {
    validate_workspace_id(&workspace.id)?;
    let dir = workspaces_dir.join(&workspace.id);
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;

    let json = serde_json::to_vec_pretty(workspace).map_err(|error| error.to_string())?;
    atomic_write_workspace(
        &workspace_file(workspaces_dir, &workspace.id),
        &workspace_backup_file(workspaces_dir, &workspace.id),
        &workspace.id,
        &json,
    )
}

pub(crate) fn workspace_code_cache_path(workspaces_dir: &Path, workspace_id: &str) -> PathBuf {
    workspaces_dir
        .join(workspace_id)
        .join("engines")
        .join("codebase-memory")
        .join(engine::CODEBASE_MEMORY_VERSION)
        .join(format!(
            "contract-{}",
            engine::CODEBASE_MEMORY_CONTRACT_VERSION
        ))
        .join("cache")
}

pub(crate) fn workspace_db_cache_dir(workspaces_dir: &Path, workspace_id: &str) -> PathBuf {
    workspaces_dir
        .join(workspace_id)
        .join("engines")
        .join("database-memory")
        .join(engine::DATABASE_MEMORY_VERSION)
        .join(format!(
            "contract-{}",
            engine::DATABASE_MEMORY_CONTRACT_VERSION
        ))
        .join("profiles")
}

pub(crate) fn workspace_repo_dir(workspaces_dir: &Path, workspace_id: &str) -> PathBuf {
    workspaces_dir.join(workspace_id).join("repo")
}

fn read_workspace(file: impl AsRef<Path>) -> Result<Workspace, String> {
    let json = fs::read_to_string(file).map_err(|error| error.to_string())?;
    serde_json::from_str(&json).map_err(|error| error.to_string())
}

pub(crate) fn read_workspace_by_id(
    workspaces_dir: &Path,
    workspace_id: &str,
) -> Result<Workspace, String> {
    validate_workspace_id(workspace_id)?;
    let primary = workspace_file(workspaces_dir, workspace_id);
    match read_workspace_for_id(&primary, workspace_id) {
        Ok(workspace) => Ok(workspace),
        Err(primary_error) => {
            let backup = workspace_backup_file(workspaces_dir, workspace_id);
            if !backup.is_file() {
                return Err(primary_error);
            }
            read_workspace_for_id(&backup, workspace_id).map_err(|backup_error| {
                format!(
                    "프로젝트 설정을 열 수 없습니다: {primary_error}; 백업도 열 수 없습니다: {backup_error}"
                )
            })
        }
    }
}

fn read_workspace_for_id(file: &Path, workspace_id: &str) -> Result<Workspace, String> {
    let mut workspace = read_workspace(file)?;
    validate_workspace_id(&workspace.id)?;
    if workspace.id != workspace_id {
        return Err("프로젝트 파일 ID가 경로와 일치하지 않습니다".to_string());
    }
    if workspace.repo_source == RepoSource::Local && is_legacy_managed_clone(file, &workspace) {
        workspace.repo_source = RepoSource::Github;
    }
    Ok(workspace)
}

fn is_legacy_managed_clone(workspace_file: &Path, workspace: &Workspace) -> bool {
    let Some(workspace_dir) = workspace_file.parent() else {
        return false;
    };
    let expected_repo = workspace_dir.join("repo");
    let Ok(actual_repo) = fs::canonicalize(&workspace.repo_path) else {
        return false;
    };
    let Ok(expected_repo) = fs::canonicalize(expected_repo) else {
        return false;
    };
    actual_repo == expected_repo && actual_repo.join(".git").exists()
}

fn workspace_file(workspaces_dir: &Path, workspace_id: &str) -> PathBuf {
    workspaces_dir.join(workspace_id).join("workspace.json")
}

pub(crate) fn workspace_backup_file(workspaces_dir: &Path, workspace_id: &str) -> PathBuf {
    workspaces_dir
        .join(workspace_id)
        .join("workspace.backup.json")
}

fn atomic_write_workspace(
    primary: &Path,
    backup: &Path,
    workspace_id: &str,
    contents: &[u8],
) -> Result<(), String> {
    let sequence = NEXT_WORKSPACE_WRITE_ID.fetch_add(1, Ordering::Relaxed);
    let dir = primary
        .parent()
        .ok_or_else(|| "프로젝트 설정 경로를 만들 수 없습니다".to_string())?;
    let temp = dir.join(format!(
        "workspace.{}.{}.{}.tmp",
        std::process::id(),
        timestamp(),
        sequence
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)
        .map_err(|error| error.to_string())?;
    if let Err(error) = file.write_all(contents).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&temp);
        return Err(error.to_string());
    }
    drop(file);

    if !primary.exists() {
        return fs::rename(&temp, primary).map_err(|error| {
            let _ = fs::remove_file(&temp);
            error.to_string()
        });
    }

    if read_workspace_for_id(primary, workspace_id).is_ok() {
        replace_valid_workspace(primary, backup, &temp)
    } else {
        replace_invalid_workspace(primary, &temp, sequence)
    }
}

fn replace_valid_workspace(primary: &Path, backup: &Path, temp: &Path) -> Result<(), String> {
    let previous_backup = backup.with_file_name(format!(
        "workspace.backup.{}.{}.tmp",
        std::process::id(),
        NEXT_WORKSPACE_WRITE_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let had_backup = backup.exists();
    if had_backup {
        fs::rename(backup, &previous_backup).map_err(|error| {
            let _ = fs::remove_file(temp);
            error.to_string()
        })?;
    }
    if let Err(error) = fs::rename(primary, backup) {
        if had_backup {
            let _ = fs::rename(&previous_backup, backup);
        }
        let _ = fs::remove_file(temp);
        return Err(error.to_string());
    }
    if let Err(error) = fs::rename(temp, primary) {
        let _ = fs::rename(backup, primary);
        if had_backup {
            let _ = fs::rename(&previous_backup, backup);
        }
        let _ = fs::remove_file(temp);
        return Err(error.to_string());
    }
    if had_backup {
        let _ = fs::remove_file(previous_backup);
    }
    Ok(())
}

fn replace_invalid_workspace(primary: &Path, temp: &Path, sequence: u64) -> Result<(), String> {
    let corrupt = primary.with_file_name(format!(
        "workspace.corrupt.{}.{}.json",
        timestamp(),
        sequence
    ));
    fs::rename(primary, &corrupt).map_err(|error| {
        let _ = fs::remove_file(temp);
        error.to_string()
    })?;
    if let Err(error) = fs::rename(temp, primary) {
        let _ = fs::rename(&corrupt, primary);
        let _ = fs::remove_file(temp);
        return Err(error.to_string());
    }
    Ok(())
}

