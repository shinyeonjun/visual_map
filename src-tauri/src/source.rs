use serde::{Deserialize, Serialize};
use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::workspace;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SourceEditor {
    Vscode,
    Cursor,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OpenSourceLocationRequest {
    pub workspace_id: String,
    pub path: String,
    pub line: Option<u64>,
    pub column: Option<u64>,
    pub editor: SourceEditor,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RevealSourceLocationRequest {
    pub workspace_id: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SourceActionResult {
    pub path: String,
    pub line: Option<u64>,
    pub column: Option<u64>,
    pub action: String,
}

pub(crate) fn open_source_location(
    app_data_dir: impl AsRef<Path>,
    request: OpenSourceLocationRequest,
) -> Result<SourceActionResult, String> {
    let workspace = workspace::open_workspace(app_data_dir, &request.workspace_id)?;
    let path = resolve_repo_source(&workspace.repo_path, &request.path)?;
    let action_path = source_action_path(&path);
    let line = positive_position(request.line, "라인")?;
    let column = positive_position(request.column, "컬럼")?;
    let executable = find_editor(request.editor).ok_or(match request.editor {
        SourceEditor::Vscode => "VS Code를 찾지 못했습니다. 설치 경로 또는 PATH를 확인하세요.",
        SourceEditor::Cursor => "Cursor를 찾지 못했습니다. 설치 경로 또는 PATH를 확인하세요.",
    })?;
    let target = format!("{}:{line}:{column}", action_path.display());

    Command::new(&executable)
        .arg("--goto")
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("에디터를 열지 못했습니다: {error}"))?;

    Ok(SourceActionResult {
        path: action_path.display().to_string(),
        line: Some(line),
        column: Some(column),
        action: match request.editor {
            SourceEditor::Vscode => "vscode".to_string(),
            SourceEditor::Cursor => "cursor".to_string(),
        },
    })
}

pub(crate) fn reveal_source_location(
    app_data_dir: impl AsRef<Path>,
    request: RevealSourceLocationRequest,
) -> Result<SourceActionResult, String> {
    let workspace = workspace::open_workspace(app_data_dir, &request.workspace_id)?;
    let path = resolve_repo_source(&workspace.repo_path, &request.path)?;
    let action_path = source_action_path(&path);

    #[cfg(target_os = "windows")]
    Command::new("explorer.exe")
        .arg(format!("/select,{}", action_path.display()))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("파일 탐색기를 열지 못했습니다: {error}"))?;

    #[cfg(not(target_os = "windows"))]
    return Err("파일 탐색기 이동은 Windows에서만 지원합니다".to_string());

    Ok(SourceActionResult {
        path: action_path.display().to_string(),
        line: None,
        column: None,
        action: "reveal".to_string(),
    })
}

fn positive_position(value: Option<u64>, label: &str) -> Result<u64, String> {
    let value = value.unwrap_or(1);
    if value == 0 || value > u32::MAX as u64 {
        return Err(format!("{label}은 1 이상 {} 이하여야 합니다", u32::MAX));
    }
    Ok(value)
}

pub(crate) fn resolve_repo_source(repo_path: &str, source_path: &str) -> Result<PathBuf, String> {
    let root = Path::new(repo_path)
        .canonicalize()
        .map_err(|error| format!("프로젝트 경로를 확인하지 못했습니다: {error}"))?;
    let requested = Path::new(source_path);
    let candidate = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        root.join(requested)
    };
    let candidate = candidate
        .canonicalize()
        .map_err(|error| format!("소스 파일을 찾지 못했습니다: {error}"))?;

    if !candidate.starts_with(&root) {
        return Err("프로젝트 폴더 밖의 파일은 열 수 없습니다".to_string());
    }
    if !candidate.is_file() {
        return Err("선택한 소스 위치가 파일이 아닙니다".to_string());
    }
    Ok(candidate)
}

#[cfg(target_os = "windows")]
fn source_action_path(path: &Path) -> PathBuf {
    use std::{
        ffi::OsString,
        os::windows::ffi::{OsStrExt, OsStringExt},
    };

    const VERBATIM: &[u16] = &[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
    const VERBATIM_UNC: &[u16] = &[
        b'\\' as u16,
        b'\\' as u16,
        b'?' as u16,
        b'\\' as u16,
        b'U' as u16,
        b'N' as u16,
        b'C' as u16,
        b'\\' as u16,
    ];

    let wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
    let standard = if let Some(rest) = wide.strip_prefix(VERBATIM_UNC) {
        [vec![b'\\' as u16, b'\\' as u16], rest.to_vec()].concat()
    } else if let Some(rest) = wide.strip_prefix(VERBATIM) {
        rest.to_vec()
    } else {
        return path.to_path_buf();
    };
    PathBuf::from(OsString::from_wide(&standard))
}

#[cfg(not(target_os = "windows"))]
fn source_action_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}

fn find_editor(editor: SourceEditor) -> Option<PathBuf> {
    editor_candidates(editor)
        .into_iter()
        .find(|candidate| candidate.is_file())
}

fn editor_candidates(editor: SourceEditor) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let executable_name = match editor {
        SourceEditor::Vscode => "Code.exe",
        SourceEditor::Cursor => "Cursor.exe",
    };

    if let Some(path) = env::var_os("PATH") {
        candidates.extend(env::split_paths(&path).map(|entry| entry.join(executable_name)));
    }

    if let Some(local_app_data) = env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        match editor {
            SourceEditor::Vscode => candidates.push(
                local_app_data
                    .join("Programs")
                    .join("Microsoft VS Code")
                    .join("Code.exe"),
            ),
            SourceEditor::Cursor => {
                candidates.push(
                    local_app_data
                        .join("Programs")
                        .join("cursor")
                        .join("Cursor.exe"),
                );
                candidates.push(
                    local_app_data
                        .join("Programs")
                        .join("Cursor")
                        .join("Cursor.exe"),
                );
            }
        }
    }

    for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
        let Some(program_files) = env::var_os(variable).map(PathBuf::from) else {
            continue;
        };
        match editor {
            SourceEditor::Vscode => {
                candidates.push(program_files.join("Microsoft VS Code").join("Code.exe"))
            }
            SourceEditor::Cursor => {
                candidates.push(program_files.join("Cursor").join("Cursor.exe"));
            }
        }
    }

    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::SystemTime};

    #[test]
    fn source_resolution_allows_repo_files_and_rejects_escape() {
        let root = temp_root("source-resolution");
        let repo = root.join("repo");
        let outside = root.join("outside.rs");
        fs::create_dir_all(repo.join("src")).unwrap();
        fs::write(repo.join("src").join("main.rs"), "fn main() {}\n").unwrap();
        fs::write(&outside, "secret\n").unwrap();

        let resolved = resolve_repo_source(
            repo.to_str().unwrap(),
            Path::new("src").join("main.rs").to_str().unwrap(),
        )
        .unwrap();
        assert_eq!(
            resolved,
            repo.join("src").join("main.rs").canonicalize().unwrap()
        );
        assert!(
            resolve_repo_source(repo.to_str().unwrap(), outside.to_str().unwrap())
                .unwrap_err()
                .contains("밖")
        );
        assert!(
            resolve_repo_source(repo.to_str().unwrap(), r"..\outside.rs")
                .unwrap_err()
                .contains("밖")
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn positions_are_positive_and_bounded() {
        assert_eq!(positive_position(None, "라인").unwrap(), 1);
        assert_eq!(positive_position(Some(42), "라인").unwrap(), 42);
        assert!(positive_position(Some(0), "라인").is_err());
        assert!(positive_position(Some(u32::MAX as u64 + 1), "라인").is_err());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn source_resolution_rejects_reparse_escape_when_symlinks_are_available() {
        use std::os::windows::fs::symlink_file;

        let root = temp_root("source-reparse");
        let repo = root.join("repo");
        let outside = root.join("outside.rs");
        let link = repo.join("linked.rs");
        fs::create_dir_all(&repo).unwrap();
        fs::write(&outside, "secret\n").unwrap();
        if symlink_file(&outside, &link).is_err() {
            fs::remove_dir_all(root).unwrap();
            return;
        }

        assert!(
            resolve_repo_source(repo.to_str().unwrap(), link.to_str().unwrap())
                .unwrap_err()
                .contains("밖")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn source_action_paths_remove_verbatim_prefix_without_losing_unicode_or_unc() {
        assert_eq!(
            source_action_path(Path::new(r"\\?\C:\저장 소\main.rs")),
            PathBuf::from(r"C:\저장 소\main.rs")
        );
        assert_eq!(
            source_action_path(Path::new(r"\\?\UNC\server\share\소스 폴더\main.rs")),
            PathBuf::from(r"\\server\share\소스 폴더\main.rs")
        );
        assert_eq!(
            source_action_path(Path::new(r"C:\plain path\main.rs")),
            PathBuf::from(r"C:\plain path\main.rs")
        );
    }

    fn temp_root(label: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "backend-visual-map-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
