//! One code-only analysis job from sidecar execution to canonical publication.

use crate::{
    engine::{self, EngineObserver, EngineProcessEvent, EngineRunPolicy},
    fact_graph::{self, CanonicalFactBundleArtifact, FactGraphStatus},
    provider_assets,
    workspace::Workspace,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::Emitter;

const CANONICAL_BUNDLE_MARKER: &str = "@codebase-workspace-canonical-fact-bundle ";
const ENGINE_ERROR_PREFIX: &str = "code-memory-language: ";
const MAX_ENGINE_FAILURE_DETAIL_CHARS: usize = 800;
static ACTIVE_WORKSPACE_ANALYSES: OnceLock<Mutex<BTreeSet<String>>> = OnceLock::new();

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AnalyzeWorkspaceRequest {
    pub workspace_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AnalysisProgressEvent {
    pub workspace_id: String,
    pub stage: String,
    pub completed: u64,
    pub total: u64,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AnalyzeWorkspaceResult {
    pub fact_graph: FactGraphStatus,
    pub semantic_revision_id: Option<String>,
    pub semantic_error: Option<String>,
}

pub(crate) fn begin_workspace_analysis(
    workspace_id: &str,
) -> Result<WorkspaceAnalysisGuard, String> {
    let mut active = ACTIVE_WORKSPACE_ANALYSES
        .get_or_init(|| Mutex::new(BTreeSet::new()))
        .lock()
        .map_err(|_| "analysis 작업 잠금이 손상되었습니다".to_string())?;
    if !active.insert(workspace_id.to_string()) {
        return Err("이 workspace는 이미 분석 중입니다".to_string());
    }
    Ok(WorkspaceAnalysisGuard {
        workspace_id: workspace_id.to_string(),
    })
}

pub(crate) struct WorkspaceAnalysisGuard {
    workspace_id: String,
}

impl Drop for WorkspaceAnalysisGuard {
    fn drop(&mut self) {
        if let Ok(mut active) = ACTIVE_WORKSPACE_ANALYSES
            .get_or_init(|| Mutex::new(BTreeSet::new()))
            .lock()
        {
            active.remove(&self.workspace_id);
        }
    }
}

pub(crate) fn run_code_analysis(
    app: &tauri::AppHandle,
    app_data_dir: &Path,
    workspace: &Workspace,
) -> Result<FactGraphStatus, String> {
    let registry = crate::engine_registry_for_app(app)?;
    let code_engine = registry
        .engines
        .iter()
        .find(|item| item.id == "codebase-memory")
        .ok_or_else(|| "codebase-memory 읽기 도구 등록을 찾지 못했습니다".to_string())?;
    if !code_engine.available {
        return Err(code_engine
            .error
            .clone()
            .unwrap_or_else(|| format!("읽기 도구가 없습니다: {}", code_engine.executable)));
    }

    let staging = AnalysisStaging::create()?;
    let engine_dir = PathBuf::from(&registry.engine_dir);
    let packs_root = resolve_packs_root(&engine_dir)?;
    let provider_progress = |label: &str, completed: u64, total: u64| {
        let _ = app.emit(
            "analysis-progress",
            AnalysisProgressEvent {
                workspace_id: workspace.id.clone(),
                stage: "provider-setup".to_string(),
                completed,
                total,
                label: label.to_string(),
            },
        );
    };
    let providers_root = provider_assets::resolve_provider_root(
        app_data_dir,
        &engine_dir,
        Some(&provider_progress),
    )?;
    let index_out = staging.path.join("language-index.json");
    let architecture_out = staging.path.join("architecture-index.json");
    let args = vec![
        "index".to_string(),
        "--root".to_string(),
        workspace.repo_path.clone(),
        "--packs-root".to_string(),
        path_text(&packs_root, "framework pack root")?,
        "--providers-root".to_string(),
        path_text(&providers_root, "provider root")?,
        "--out".to_string(),
        path_text(&index_out, "analysis output")?,
        "--architecture-out".to_string(),
        path_text(&architecture_out, "architecture output")?,
    ];
    let operation_id = format!("analysis-{}-{}", workspace.id, unix_millis());
    let _operation_guard = engine::begin_engine_operation(&operation_id)?;
    let observer = progress_observer(app.clone(), workspace.id.clone());
    let result = engine::run_engine_command_with_env_observer(
        code_engine,
        &args,
        EngineRunPolicy {
            hard_timeout: Duration::from_secs(2 * 60 * 60),
            idle_timeout: Duration::from_secs(10 * 60),
        },
        &[
            ("CODEBASE_WORKSPACE_OPERATION_ID", operation_id.as_str()),
            ("CODE_MEMORY_REQUIRE_MANAGED_PROVIDERS", "1"),
        ],
        Some(observer),
    )?;
    if !result.ok {
        return Err(match engine_failure_detail(&result.stderr) {
            Some(detail) => format!("codebase-memory 분석이 실패했습니다: {detail}"),
            None => "codebase-memory 분석이 실패했습니다".to_string(),
        });
    }
    let artifact: CanonicalFactBundleArtifact = parse_last_marker(&result.stderr)?;
    let status = fact_graph::import_and_publish(app_data_dir, workspace.id.as_str(), &artifact)?;
    let _ = app.emit(
        "analysis-progress",
        AnalysisProgressEvent {
            workspace_id: workspace.id.clone(),
            stage: "facts-ready".to_string(),
            completed: 1,
            total: 1,
            label: "검증된 코드 사실을 준비했습니다".to_string(),
        },
    );
    Ok(status)
}

fn engine_failure_detail(stderr: &str) -> Option<String> {
    let lines = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let detail = lines
        .iter()
        .rev()
        .find_map(|line| line.strip_prefix(ENGINE_ERROR_PREFIX))
        .or_else(|| {
            lines
                .iter()
                .rev()
                .copied()
                .find(|line| !is_engine_progress_or_telemetry(line))
        })?;
    let mut characters = detail.chars();
    let mut bounded = characters
        .by_ref()
        .take(MAX_ENGINE_FAILURE_DETAIL_CHARS)
        .collect::<String>();
    if characters.next().is_some() {
        bounded.push('…');
    }
    (!bounded.is_empty()).then_some(bounded)
}

fn is_engine_progress_or_telemetry(line: &str) -> bool {
    line.starts_with("@codebase-workspace-")
        || line.starts_with("scheduler providers ")
        || line.starts_with("[provider:")
        || line.starts_with("cached ")
        || line.starts_with("invalidated ")
        || line.contains(" elapsed_ms=")
}

fn parse_last_marker<T: for<'de> Deserialize<'de>>(stderr: &str) -> Result<T, String> {
    let payload = stderr
        .lines()
        .filter_map(|line| line.strip_prefix(CANONICAL_BUNDLE_MARKER))
        .next_back()
        .ok_or_else(|| {
            "codebase-memory가 canonical bundle receipt를 내보내지 않았습니다".to_string()
        })?;
    serde_json::from_str(payload)
        .map_err(|error| format!("canonical bundle receipt 형식이 올바르지 않습니다: {error}"))
}

fn progress_observer(app: tauri::AppHandle, workspace_id: String) -> EngineObserver {
    Arc::new(move |event: EngineProcessEvent| {
        let Some(payload) = event.line.strip_prefix("@codebase-workspace-progress ") else {
            return;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) else {
            return;
        };
        let _ = app.emit(
            "analysis-progress",
            AnalysisProgressEvent {
                workspace_id: workspace_id.clone(),
                stage: value
                    .get("stage")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("code")
                    .to_string(),
                completed: value
                    .get("completed")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or_default(),
                total: value
                    .get("total")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(100)
                    .max(1),
                label: value
                    .get("label")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("코드 구조 분석 중")
                    .to_string(),
            },
        );
    })
}

fn resolve_packs_root(engine_dir: &Path) -> Result<PathBuf, String> {
    if engine_dir.join("packs/framework/catalog.json").is_file() {
        return Ok(engine_dir.to_path_buf());
    }
    #[cfg(any(debug_assertions, codebase_workspace_internal_build))]
    {
        let source_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or_else(|| "개발 source root를 계산하지 못했습니다".to_string())?
            .join("code_memory");
        if source_root.join("packs/framework/catalog.json").is_file() {
            return Ok(source_root);
        }
    }
    Err(format!(
        "framework pack catalog를 찾지 못했습니다: {}",
        engine_dir.display()
    ))
}

fn path_text(path: &Path, label: &str) -> Result<String, String> {
    path.to_str()
        .filter(|value| !value.is_empty() && !value.chars().any(char::is_control))
        .map(str::to_string)
        .ok_or_else(|| format!("{label} 경로를 실행 인자로 표현할 수 없습니다"))
}

struct AnalysisStaging {
    path: PathBuf,
}

impl AnalysisStaging {
    fn create() -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!(
            "codebase-workspace-analysis-{}-{}",
            std::process::id(),
            unix_millis()
        ));
        fs::create_dir(&path)
            .map_err(|error| format!("analysis staging 폴더를 만들지 못했습니다: {error}"))?;
        Ok(Self { path })
    }
}

impl Drop for AnalysisStaging {
    fn drop(&mut self) {
        let temp_root = std::env::temp_dir();
        if self.path.parent() == Some(temp_root.as_path())
            && self
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("codebase-workspace-analysis-"))
        {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_parser_uses_the_last_complete_receipt() {
        let raw = format!(
            "noise\n{CANONICAL_BUNDLE_MARKER}{{\"value\":1}}\n{CANONICAL_BUNDLE_MARKER}{{\"value\":2}}\n"
        );
        let parsed: serde_json::Value = parse_last_marker(&raw).unwrap();
        assert_eq!(parsed["value"], 2);
    }

    #[test]
    fn engine_failure_keeps_only_the_actionable_error_line() {
        let stderr = format!(
            "{progress}\n{progress}\nprovider_and_scip_conversion elapsed_ms=556053 batches=7\n\
             code-memory-language: typescript provider executed root . but AnalysisPlan unit unit-1 requires legacy/frontend\n",
            progress = "@codebase-workspace-progress {\"completed\":55,\"stage\":\"providers\",\"total\":100}"
        );

        assert_eq!(
            engine_failure_detail(&stderr).as_deref(),
            Some(
                "typescript provider executed root . but AnalysisPlan unit unit-1 requires legacy/frontend"
            )
        );
    }

    #[test]
    fn engine_failure_does_not_render_progress_as_an_error() {
        let stderr = "@codebase-workspace-progress {\"completed\":55}\n\
                      scheduler providers jobs=7 max_parallel=4\n\
                      provider_and_scip_conversion elapsed_ms=556053 batches=7\n";
        assert_eq!(engine_failure_detail(stderr), None);
    }

    #[test]
    fn engine_failure_detail_is_bounded_for_the_ui() {
        let stderr = format!("{ENGINE_ERROR_PREFIX}{}", "오".repeat(2_000));
        let detail = engine_failure_detail(&stderr).unwrap();
        assert_eq!(detail.chars().count(), MAX_ENGINE_FAILURE_DETAIL_CHARS + 1);
        assert!(detail.ends_with('…'));
    }

    #[test]
    fn one_workspace_cannot_start_two_analysis_jobs() {
        let workspace_id = format!("ws-test-{}", unix_millis());
        let guard = begin_workspace_analysis(&workspace_id).unwrap();
        assert!(begin_workspace_analysis(&workspace_id).is_err());
        drop(guard);
        assert!(begin_workspace_analysis(&workspace_id).is_ok());
    }
}
