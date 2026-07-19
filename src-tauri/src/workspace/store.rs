use crate::{
    engine,
    paths::{base_paths, ensure_base_dirs},
};
use serde::Serialize;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use super::model::{CreateWorkspaceRequest, RepoSource, Workspace, WorkspaceEngineCache};

static NEXT_WORKSPACE_ID: AtomicU64 = AtomicU64::new(0);
static NEXT_WORKSPACE_WRITE_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspaceRecoveryWarning {
    pub workspace_id: String,
    pub kind: String,
    pub message: String,
    pub action: String,
}

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
    serde_json::from_str(stdout).ok().or_else(|| {
        stdout.lines().find_map(|line| {
            let line = line.trim();
            if line.starts_with('{') || line.starts_with('[') {
                serde_json::from_str(line).ok()
            } else {
                None
            }
        })
    })
}

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

pub(crate) fn validate_workspace_id(workspace_id: &str) -> Result<(), String> {
    if workspace_id.is_empty() {
        return Err("프로젝트 ID가 필요합니다".to_string());
    }

    if workspace_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        Ok(())
    } else {
        Err("프로젝트 ID에 허용되지 않는 문자가 있습니다".to_string())
    }
}

pub(crate) fn workspace_id(name: &str) -> String {
    let slug = slugify(name);
    let sequence = NEXT_WORKSPACE_ID.fetch_add(1, Ordering::Relaxed);
    format!(
        "{}-{}-{}-{}",
        slug,
        timestamp(),
        std::process::id(),
        sequence
    )
}

pub(crate) fn github_repo_name(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_end_matches('/');
    let path = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("git@github.com:"))?;
    let parts = path.split('/').collect::<Vec<_>>();

    if parts.len() != 2 || !valid_github_segment(parts[0]) {
        return None;
    }

    let repo = parts[1].strip_suffix(".git").unwrap_or(parts[1]);
    if valid_github_segment(repo) {
        Some(repo.to_string())
    } else {
        None
    }
}

fn valid_github_segment(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn is_remote_url(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("git@")
}

fn canonical_local_repo_path(value: &str) -> Result<String, String> {
    let path = fs::canonicalize(value)
        .map_err(|error| format!("프로젝트 폴더를 찾을 수 없습니다: {error}"))?;
    if !path.is_dir() {
        return Err("프로젝트 경로는 폴더여야 합니다".to_string());
    }
    Ok(path_for_storage(&path))
}

#[cfg(windows)]
fn path_for_storage(path: &Path) -> String {
    let value = path.to_string_lossy();
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    value.strip_prefix(r"\\?\").unwrap_or(&value).to_string()
}

#[cfg(not(windows))]
fn path_for_storage(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn clone_github_repo(url: &str, target: &Path) -> Result<(), String> {
    if target.exists() {
        return Err("관리 프로젝트 폴더가 이미 있어 복제할 수 없습니다".to_string());
    }

    let parent = target
        .parent()
        .ok_or_else(|| "관리 프로젝트 경로를 만들 수 없습니다".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;

    let target_string = target.display().to_string();
    let args = ["clone", "--depth", "1", url, target_string.as_str()];
    let run = run_git(&args, Duration::from_secs(180))?;

    if run.ok {
        Ok(())
    } else {
        Err(git_failure("GitHub 프로젝트 복제 실패", &run))
    }
}

fn run_git(args: &[&str], timeout: Duration) -> Result<engine::EngineRunResult, String> {
    engine::run_command_with_env(
        Path::new("git"),
        args,
        timeout,
        &[("GIT_TERMINAL_PROMPT", "0")],
    )
    .map_err(|error| format!("git 실행 실패: {error}"))
}

fn git_failure(context: &str, run: &engine::EngineRunResult) -> String {
    let detail = if run.stderr.trim().is_empty() {
        run.stdout.trim()
    } else {
        run.stderr.trim()
    };
    format!("{context}: {detail}")
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();

    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }

    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "workspace".to_string()
    } else {
        slug.to_string()
    }
}

pub(crate) fn timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}
