use crate::paths::{base_paths, ensure_base_dirs};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const WORKSPACE_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum WorkspaceProviderKind {
    Codex,
    Claude,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum WorkspaceReasoningEffort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
    Ultra,
}

const fn default_reasoning_effort() -> WorkspaceReasoningEffort {
    WorkspaceReasoningEffort::High
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspaceProvider {
    pub kind: WorkspaceProviderKind,
    pub model: String,
    #[serde(default = "default_reasoning_effort")]
    pub effort: WorkspaceReasoningEffort,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Workspace {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub repo_path: String,
    pub provider: Option<WorkspaceProvider>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreateWorkspaceRequest {
    pub name: String,
    pub repo_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SetWorkspaceProviderRequest {
    pub workspace_id: String,
    pub kind: WorkspaceProviderKind,
    pub model: String,
    #[serde(default = "default_reasoning_effort")]
    pub effort: WorkspaceReasoningEffort,
}

pub(crate) fn create_workspace(
    app_data_dir: impl AsRef<Path>,
    request: CreateWorkspaceRequest,
) -> Result<Workspace, String> {
    let name = validated_name(&request.name)?;
    let repo_path = validated_repo_path(&request.repo_path)?;
    let id = workspace_id(&repo_path);
    let root = workspace_root(app_data_dir.as_ref())?;
    let target = workspace_file(&root, &id)?;
    if target.is_file() {
        return open_workspace(app_data_dir, &id);
    }

    fs::create_dir_all(target.parent().expect("workspace file has a parent"))
        .map_err(|error| format!("workspace directory를 만들지 못했습니다: {error}"))?;
    let now = unix_seconds();
    let workspace = Workspace {
        schema_version: WORKSPACE_SCHEMA_VERSION,
        id,
        name,
        repo_path: repo_path.display().to_string(),
        provider: None,
        created_at: now,
        updated_at: now,
    };
    write_workspace(&target, &workspace)?;
    Ok(workspace)
}

pub(crate) fn list_workspaces(app_data_dir: impl AsRef<Path>) -> Result<Vec<Workspace>, String> {
    let root = workspace_root(app_data_dir.as_ref())?;
    let mut workspaces = Vec::new();
    for entry in
        fs::read_dir(&root).map_err(|error| format!("workspace 목록을 읽지 못했습니다: {error}"))?
    {
        let entry = entry.map_err(|error| format!("workspace 항목을 읽지 못했습니다: {error}"))?;
        if !entry
            .file_type()
            .map_err(|error| format!("workspace 항목 종류를 읽지 못했습니다: {error}"))?
            .is_dir()
        {
            continue;
        }
        let path = entry.path().join("workspace.json");
        if !path.is_file() {
            continue;
        }
        workspaces.push(read_workspace(&path)?);
    }
    workspaces.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(workspaces)
}

pub(crate) fn open_workspace(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
) -> Result<Workspace, String> {
    validate_workspace_id(workspace_id)?;
    let root = workspace_root(app_data_dir.as_ref())?;
    let path = workspace_file(&root, workspace_id)?;
    if !path.is_file() {
        return Err("workspace를 찾을 수 없습니다".to_string());
    }
    let workspace = read_workspace(&path)?;
    if workspace.id != workspace_id {
        return Err("workspace ID와 저장 경로가 일치하지 않습니다".to_string());
    }
    Ok(workspace)
}

pub(crate) fn set_workspace_provider(
    app_data_dir: impl AsRef<Path>,
    request: SetWorkspaceProviderRequest,
) -> Result<Workspace, String> {
    let mut workspace = open_workspace(&app_data_dir, &request.workspace_id)?;
    let model = request.model.trim();
    if model.is_empty() || model.len() > 120 || model.chars().any(char::is_control) {
        return Err("모델 이름은 1~120자의 일반 문자열이어야 합니다".to_string());
    }
    workspace.provider = Some(WorkspaceProvider {
        kind: request.kind,
        model: model.to_string(),
        effort: request.effort,
    });
    workspace.updated_at = unix_seconds();
    let root = workspace_root(app_data_dir.as_ref())?;
    let path = workspace_file(&root, &workspace.id)?;
    write_workspace(&path, &workspace)?;
    Ok(workspace)
}

pub(crate) fn delete_workspace(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
) -> Result<(), String> {
    validate_workspace_id(workspace_id)?;
    let root = workspace_root(app_data_dir.as_ref())?;
    let workspace_dir = root.join(workspace_id);
    if !workspace_dir.exists() {
        return Ok(());
    }
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("workspace root를 확인하지 못했습니다: {error}"))?;
    let canonical_target = workspace_dir
        .canonicalize()
        .map_err(|error| format!("workspace 경로를 확인하지 못했습니다: {error}"))?;
    if canonical_target.parent() != Some(canonical_root.as_path()) {
        return Err("workspace 삭제 경로가 허용 범위를 벗어났습니다".to_string());
    }
    fs::remove_dir_all(&canonical_target)
        .map_err(|error| format!("workspace를 삭제하지 못했습니다: {error}"))
}

pub(crate) fn workspace_data_dir(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
) -> Result<PathBuf, String> {
    validate_workspace_id(workspace_id)?;
    Ok(workspace_root(app_data_dir.as_ref())?.join(workspace_id))
}

fn workspace_root(app_data_dir: &Path) -> Result<PathBuf, String> {
    let paths = base_paths(app_data_dir);
    ensure_base_dirs(&paths)
        .map_err(|error| format!("앱 데이터 디렉터리를 만들지 못했습니다: {error}"))?;
    let root = paths.workspaces_dir.join("v2");
    fs::create_dir_all(&root)
        .map_err(|error| format!("workspace root를 만들지 못했습니다: {error}"))?;
    Ok(root)
}

fn workspace_file(root: &Path, workspace_id: &str) -> Result<PathBuf, String> {
    validate_workspace_id(workspace_id)?;
    Ok(root.join(workspace_id).join("workspace.json"))
}

fn read_workspace(path: &Path) -> Result<Workspace, String> {
    let bytes = fs::read(path).map_err(|error| format!("workspace를 읽지 못했습니다: {error}"))?;
    let workspace: Workspace = serde_json::from_slice(&bytes)
        .map_err(|error| format!("workspace 형식이 올바르지 않습니다: {error}"))?;
    if workspace.schema_version != WORKSPACE_SCHEMA_VERSION {
        return Err(format!(
            "지원하지 않는 workspace schema입니다: {}",
            workspace.schema_version
        ));
    }
    validate_workspace_id(&workspace.id)?;
    validated_name(&workspace.name)?;
    Ok(workspace)
}

fn write_workspace(path: &Path, workspace: &Workspace) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "workspace 저장 경로가 올바르지 않습니다".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("workspace 저장 폴더를 만들지 못했습니다: {error}"))?;
    let bytes = serde_json::to_vec_pretty(workspace)
        .map_err(|error| format!("workspace를 직렬화하지 못했습니다: {error}"))?;
    let temporary = parent.join("workspace.json.next");
    let backup = parent.join("workspace.json.previous");
    {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| format!("workspace 임시 파일을 만들지 못했습니다: {error}"))?;
        file.write_all(&bytes)
            .map_err(|error| format!("workspace를 쓰지 못했습니다: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("workspace를 디스크에 반영하지 못했습니다: {error}"))?;
    }
    if path.exists() {
        if backup.exists() {
            fs::remove_file(&backup)
                .map_err(|error| format!("이전 workspace backup을 지우지 못했습니다: {error}"))?;
        }
        fs::rename(path, &backup)
            .map_err(|error| format!("workspace backup을 만들지 못했습니다: {error}"))?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        if backup.exists() {
            let _ = fs::rename(&backup, path);
        }
        return Err(format!("workspace를 교체하지 못했습니다: {error}"));
    }
    if backup.exists() {
        fs::remove_file(&backup)
            .map_err(|error| format!("workspace backup을 정리하지 못했습니다: {error}"))?;
    }
    Ok(())
}

fn validated_name(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 120 || value.chars().any(char::is_control) {
        return Err("workspace 이름은 1~120자의 일반 문자열이어야 합니다".to_string());
    }
    Ok(value.to_string())
}

fn validated_repo_path(value: &str) -> Result<PathBuf, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("프로젝트 폴더가 필요합니다".to_string());
    }
    let path = Path::new(value)
        .canonicalize()
        .map_err(|error| format!("프로젝트 폴더를 확인하지 못했습니다: {error}"))?;
    if !path.is_dir() {
        return Err("프로젝트 경로는 폴더여야 합니다".to_string());
    }
    Ok(path)
}

fn workspace_id(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/").to_lowercase();
    let digest = Sha256::digest(normalized.as_bytes());
    format!("ws-{}", hex_prefix(&digest, 16))
}

fn validate_workspace_id(value: &str) -> Result<(), String> {
    let Some(hex) = value.strip_prefix("ws-") else {
        return Err("workspace ID가 올바르지 않습니다".to_string());
    };
    if hex.len() != 16 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("workspace ID가 올바르지 않습니다".to_string());
    }
    Ok(())
}

fn hex_prefix(bytes: &[u8], chars: usize) -> String {
    bytes
        .iter()
        .flat_map(|byte| format!("{byte:02x}").chars().collect::<Vec<_>>())
        .take(chars)
        .collect()
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_workspace_round_trip_and_provider_update_use_v2_store() {
        let root = test_root("round-trip");
        let repo = root.join("repo");
        fs::create_dir_all(&repo).unwrap();
        let workspace = create_workspace(
            &root,
            CreateWorkspaceRequest {
                name: "Orders".to_string(),
                repo_path: repo.display().to_string(),
            },
        )
        .unwrap();
        assert_eq!(workspace.schema_version, 2);
        assert!(workspace.provider.is_none());

        let updated = set_workspace_provider(
            &root,
            SetWorkspaceProviderRequest {
                workspace_id: workspace.id.clone(),
                kind: WorkspaceProviderKind::Codex,
                model: "gpt-5.6-sol".to_string(),
                effort: WorkspaceReasoningEffort::High,
            },
        )
        .unwrap();
        let provider = updated.provider.unwrap();
        assert_eq!(provider.model, "gpt-5.6-sol");
        assert_eq!(provider.effort, WorkspaceReasoningEffort::High);
        assert_eq!(list_workspaces(&root).unwrap().len(), 1);

        delete_workspace(&root, &workspace.id).unwrap();
        assert!(list_workspaces(&root).unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn workspace_ids_are_stable_and_reject_path_escape_shapes() {
        let path = Path::new(r"C:\repo\orders");
        assert_eq!(workspace_id(path), workspace_id(path));
        for value in ["../escape", "ws-123", "ws-000000000000000g"] {
            assert!(validate_workspace_id(value).is_err());
        }
    }

    #[test]
    fn existing_provider_without_effort_defaults_to_high() {
        let provider: WorkspaceProvider =
            serde_json::from_str(r#"{"kind":"codex","model":"gpt-5.6-sol"}"#).unwrap();
        assert_eq!(provider.effort, WorkspaceReasoningEffort::High);
    }

    fn test_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "codebase-workspace-v2-{label}-{}-{}",
            std::process::id(),
            unix_seconds()
        ))
    }
}
