use crate::models::{CodexSettings, WorkspaceMetadata, WorkspacePaths};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn prepare_workspace(
    project_path: &Path,
    model: &str,
    source_config: &Path,
) -> Result<WorkspacePaths, String> {
    let root = storage_root()?;
    let workspace_key = workspace_key(project_path);
    let directory = root.join("workspaces").join(&workspace_key);
    let analysis_directory = directory.join("analysis");
    let config_directory = directory.join("config");
    fs::create_dir_all(&analysis_directory)
        .map_err(|error| format!("워크스페이스 분석 폴더를 만들지 못했습니다: {error}"))?;
    fs::create_dir_all(&config_directory)
        .map_err(|error| format!("워크스페이스 설정 폴더를 만들지 못했습니다: {error}"))?;

    let app_config_directory = root.join("config");
    fs::create_dir_all(&app_config_directory)
        .map_err(|error| format!("VisualMap 설정 폴더를 만들지 못했습니다: {error}"))?;
    let app_settings = app_config_directory.join("app-settings.toml");
    if !app_settings.is_file() {
        let contents = format!(
            "schemaVersion = 1\nenvironment = \"{}\"\n\n[storage]\nworkspaceDirectory = \"workspaces\"\n",
            environment_name()
        );
        fs::write(&app_settings, contents)
            .map_err(|error| format!("VisualMap 기본 설정을 저장하지 못했습니다: {error}"))?;
    }

    let workspace_config = config_directory.join("analysis.toml");
    if !workspace_config.is_file() {
        fs::copy(source_config, &workspace_config)
            .map_err(|error| format!("워크스페이스 설정을 복사하지 못했습니다: {error}"))?;
    }

    let existing_codex_settings = load_codex_settings().unwrap_or_default();
    write_codex_settings(&CodexSettings {
        executable: existing_codex_settings.executable,
        model: model.to_string(),
        cli_version: existing_codex_settings.cli_version,
        catalog_source: existing_codex_settings.catalog_source,
    })?;

    let metadata = WorkspaceMetadata {
        project_path: project_path.display().to_string(),
        workspace_key,
        model: model.to_string(),
        environment: environment_name().to_string(),
        updated_at_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_millis(),
    };
    let metadata_json = serde_json::to_vec_pretty(&metadata)
        .map_err(|error| format!("워크스페이스 메타데이터를 만들지 못했습니다: {error}"))?;
    fs::write(directory.join("workspace.json"), metadata_json)
        .map_err(|error| format!("워크스페이스 메타데이터를 저장하지 못했습니다: {error}"))?;

    Ok(WorkspacePaths {
        directory,
        config: workspace_config,
        static_output: analysis_directory.join("static-result.json"),
        context_output: analysis_directory.join("codex-context.json"),
        semantic_output: analysis_directory.join("semantic-result.json"),
    })
}

pub(crate) fn resolve_codex_cli(settings: &CodexSettings) -> String {
    env::var("CODEX_CLI_PATH")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            (!settings.executable.trim().is_empty()).then(|| settings.executable.trim().to_string())
        })
        .unwrap_or_else(|| "codex".to_string())
}

pub(crate) fn codex_settings_path() -> Result<PathBuf, String> {
    let root = storage_root()?;
    let directory = root.join("config");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Codex 설정 폴더를 만들지 못했습니다: {error}"))?;
    Ok(directory.join("codex.toml"))
}

pub(crate) fn load_codex_settings() -> Result<CodexSettings, String> {
    let path = codex_settings_path()?;
    if !path.is_file() {
        return Ok(CodexSettings::default());
    }
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("VisualMap Codex 설정을 읽지 못했습니다: {error}"))?;
    toml::from_str(&contents)
        .map_err(|error| format!("VisualMap Codex 설정 형식이 올바르지 않습니다: {error}"))
}

pub(crate) fn write_codex_settings(settings: &CodexSettings) -> Result<(), String> {
    let path = codex_settings_path()?;
    let contents = toml::to_string_pretty(settings)
        .map_err(|error| format!("VisualMap Codex 설정을 만들지 못했습니다: {error}"))?;
    fs::write(path, contents)
        .map_err(|error| format!("VisualMap Codex 설정을 저장하지 못했습니다: {error}"))
}

pub(crate) fn storage_root() -> Result<PathBuf, String> {
    if let Ok(value) = env::var("VISUAL_MAP_DATA_DIR") {
        if !value.trim().is_empty() {
            let root = PathBuf::from(value);
            fs::create_dir_all(&root)
                .map_err(|error| format!("지정한 데이터 폴더를 만들지 못했습니다: {error}"))?;
            return Ok(root);
        }
    }

    if cfg!(debug_assertions) {
        let relative = Path::new("backend/code_analysis_engine/tests/dev");
        for root in search_roots() {
            let candidate = root.join(relative);
            if root.join("backend/code_analysis_engine").is_dir() {
                fs::create_dir_all(&candidate)
                    .map_err(|error| format!("개발 데이터 폴더를 만들지 못했습니다: {error}"))?;
                return Ok(candidate);
            }
        }
        let fallback = env::current_dir()
            .map_err(|error| format!("개발 데이터 폴더 기준 경로를 읽지 못했습니다: {error}"))?
            .join(relative);
        fs::create_dir_all(&fallback)
            .map_err(|error| format!("개발 데이터 폴더를 만들지 못했습니다: {error}"))?;
        return Ok(fallback);
    }

    let local_app_data = env::var_os("LOCALAPPDATA")
        .or_else(|| env::var_os("APPDATA"))
        .ok_or_else(|| "Windows 사용자 데이터 폴더를 찾지 못했습니다.".to_string())?;
    let root = PathBuf::from(local_app_data).join("VisualMap");
    fs::create_dir_all(&root)
        .map_err(|error| format!("VisualMap 저장소를 만들지 못했습니다: {error}"))?;
    Ok(root)
}

fn workspace_key(project_path: &Path) -> String {
    let canonical = project_path
        .canonicalize()
        .unwrap_or_else(|_| project_path.to_path_buf());
    let display = canonical.to_string_lossy();
    let name = canonical
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("project");
    format!(
        "{}-{:016x}",
        sanitize_segment(name),
        stable_hash(display.as_bytes())
    )
}

fn sanitize_segment(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
            result.push(character);
        } else {
            result.push('_');
        }
    }
    if result.is_empty() {
        "project".to_string()
    } else {
        result
    }
}

fn stable_hash(value: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in value {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn environment_name() -> &'static str {
    if cfg!(debug_assertions) {
        "development"
    } else {
        "release"
    }
}

pub(crate) fn resolve_executable(value: &str, environment_key: &str, file_name: &str) -> PathBuf {
    if !value.trim().is_empty() {
        return PathBuf::from(value);
    }
    if let Ok(value) = env::var(environment_key) {
        if !value.trim().is_empty() {
            return PathBuf::from(value);
        }
    }
    let executable = if cfg!(windows) {
        format!("{file_name}.exe")
    } else {
        file_name.to_string()
    };
    for root in search_roots() {
        let candidate = root
            .join("backend/code_analysis_engine/target/release")
            .join(&executable);
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from(executable)
}

pub(crate) fn resolve_config(value: &str, resource_dir: Option<&Path>) -> PathBuf {
    if !value.trim().is_empty() {
        return PathBuf::from(value);
    }
    if let Some(resource_dir) = resource_dir {
        let candidate = resource_dir.join("config/analysis.default.toml");
        if candidate.is_file() {
            return candidate;
        }
    }
    for root in search_roots() {
        let candidate = root.join("backend/code_analysis_engine/config/analysis.default.toml");
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from("analysis.default.toml")
}

fn search_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(current) = env::current_dir() {
        roots.extend(current.ancestors().map(Path::to_path_buf));
    }
    if let Ok(executable) = env::current_exe() {
        roots.extend(
            executable
                .ancestors()
                .filter_map(|path| path.parent())
                .map(Path::to_path_buf),
        );
    }
    roots.sort();
    roots.dedup();
    roots
}
