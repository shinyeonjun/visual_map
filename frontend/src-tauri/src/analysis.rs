use crate::map::build_map;
use crate::models::{
    AiSettings, AnalysisProgress, AnalyzeRequest, AnalyzeResponse, ClaudeModelCatalog, ClaudeStatus,
    CodexModelCatalog, CodexModelOption, RawCodexModelCatalog,
};
use crate::process::{
    command_output, parse_semantic_progress, run_engine_with_progress, SemanticProgressEvent,
};
use crate::storage::{
    load_ai_settings, prepare_workspace, resolve_claude_models, resolve_claude_models_path,
    resolve_codex_cli, resolve_config, resolve_executable, write_ai_settings,
};
use crate::timing::AnalysisTimingLog;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};

#[tauri::command]
pub(crate) fn get_claude_models(app: AppHandle) -> Result<ClaudeModelCatalog, String> {
    let settings = load_ai_settings().unwrap_or_default();
    let resource_dir = app.path().resource_dir().ok();
    let models_path = resolve_claude_models_path(resource_dir.as_deref());
    let models = resolve_claude_models(&models_path)?;
    let selected_model = if models.iter().any(|model| model.slug == settings.claude_model) {
        Some(settings.claude_model)
    } else {
        models.first().map(|model| model.slug.clone())
    };
    Ok(ClaudeModelCatalog {
        models,
        selected_model,
    })
}

#[tauri::command]
pub(crate) fn get_codex_models() -> Result<CodexModelCatalog, String> {
    let settings = load_ai_settings().unwrap_or_default();
    let executable = resolve_codex_cli(&settings);
    let version_output = Command::new(&executable)
        .arg("--version")
        .output()
        .map_err(|error| format!("Codex CLI를 실행하지 못했습니다: {error}"))?;
    if !version_output.status.success() {
        return Err(format!(
            "Codex CLI 확인에 실패했습니다: {}",
            command_output(&version_output)
        ));
    }
    let version = command_output(&version_output);

    let output = Command::new(&executable)
        .args(["debug", "models", "--bundled"])
        .output()
        .map_err(|error| format!("Codex 모델 목록을 읽지 못했습니다: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Codex 모델 목록을 읽지 못했습니다: {}",
            command_output(&output)
        ));
    }

    let raw: RawCodexModelCatalog = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Codex 모델 목록 형식이 올바르지 않습니다: {error}"))?;
    let models = raw
        .models
        .into_iter()
        .filter(|model| model.visibility.as_deref() != Some("hidden"))
        .map(|model| CodexModelOption {
            slug: model.slug,
            display_name: model.display_name,
            description: model.description,
            default_reasoning_level: model.default_reasoning_level,
            supported_reasoning_levels: model
                .supported_reasoning_levels
                .into_iter()
                .map(|level| level.effort)
                .collect(),
        })
        .collect::<Vec<_>>();
    let selected_model = if models.iter().any(|model| model.slug == settings.model) {
        Some(settings.model)
    } else {
        models.first().map(|model| model.slug.clone())
    };

    let saved_provider = if settings.provider.is_empty() {
        "codex".to_string()
    } else {
        settings.provider
    };
    let saved_claude_model = if settings.claude_model.is_empty() {
        "claude-opus-4-6".to_string()
    } else {
        settings.claude_model
    };

    Ok(CodexModelCatalog {
        executable,
        version: version.lines().next().unwrap_or_default().to_string(),
        source: "bundled".to_string(),
        selected_model,
        models,
        saved_provider,
        saved_claude_model,
    })
}

#[tauri::command]
pub(crate) fn save_ai_settings(
    model: String,
    cli_version: String,
    executable: String,
    provider: Option<String>,
    claude_model: Option<String>,
) -> Result<(), String> {
    let existing = load_ai_settings().unwrap_or_default();
    let settings = AiSettings {
        executable: if executable.trim().is_empty() {
            existing.executable
        } else {
            executable
        },
        model: if model.trim().is_empty() {
            existing.model
        } else {
            model
        },
        cli_version: if cli_version.trim().is_empty() {
            existing.cli_version
        } else {
            cli_version
        },
        catalog_source: existing.catalog_source,
        provider: provider.unwrap_or(existing.provider),
        claude_model: claude_model.unwrap_or(existing.claude_model),
    };
    write_ai_settings(&settings)
}

#[tauri::command]
pub(crate) fn get_claude_status() -> Result<ClaudeStatus, String> {
    let executable = std::env::var("CLAUDE_CLI_PATH")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "claude".to_string());
    let output = Command::new(&executable)
        .arg("--version")
        .output()
        .map_err(|error| format!("Claude CLI를 실행하지 못했습니다: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Claude CLI 확인에 실패했습니다: {}",
            command_output(&output)
        ));
    }
    let version = command_output(&output)
        .lines()
        .next()
        .unwrap_or_default()
        .to_string();
    Ok(ClaudeStatus {
        version,
        executable,
    })
}

#[tauri::command]
pub(crate) async fn analyze_project(
    app: AppHandle,
    request: AnalyzeRequest,
) -> Result<AnalyzeResponse, String> {
    tauri::async_runtime::spawn_blocking(move || analyze_project_blocking(app, request))
        .await
        .map_err(|error| format!("분석 백그라운드 작업이 중단되었습니다: {error}"))?
}

fn analyze_project_blocking(
    app: AppHandle,
    request: AnalyzeRequest,
) -> Result<AnalyzeResponse, String> {
    let started = Instant::now();
    let project_path = PathBuf::from(&request.project_path);
    if !project_path.is_dir() {
        return Err(format!(
            "프로젝트 폴더를 찾을 수 없습니다: {}",
            project_path.display()
        ));
    }

    let resource_dir = app.path().resource_dir().ok();
    let engine = resolve_executable(
        &request.engine_path,
        "VISUAL_MAP_ENGINE",
        "code-analysis-engine",
        resource_dir.as_deref(),
    );
    let source_config = resolve_config(&request.config_path, resource_dir.as_deref());
    if !source_config.is_file() {
        return Err(format!(
            "분석 설정 파일을 찾을 수 없습니다: {}",
            source_config.display()
        ));
    }

    emit_analysis_progress(
        &app,
        &started,
        ProgressUpdate {
            phase: "preparing",
            label: "분석을 준비하는 중입니다",
            detail: "프로젝트 워크스페이스와 분석 설정을 확인하고 있습니다.",
            percent: 0,
            step: 1,
            current: None,
            total: None,
            indeterminate: true,
        },
    );

    let workspace = prepare_workspace(
        &project_path,
        &request.provider,
        &request.model,
        &source_config,
    )?;
    let mut timing = AnalysisTimingLog::new(
        &workspace.directory,
        &request.project_path,
        &request.provider,
        &request.model,
    );
    timing.begin_step(1, "preparing", "분석 준비");

    timing.flush()?;
    let result = run_analysis_steps(&app, &request, engine.as_path(), &workspace, &started, &mut timing);
    match &result {
        Ok(_) => {
            let _ = timing.complete("complete", None);
        }
        Err(error) => {
            let _ = timing.complete("error", Some(error.clone()));
        }
    }
    result
}

fn run_analysis_steps(
    app: &AppHandle,
    request: &AnalyzeRequest,
    engine: &Path,
    workspace: &crate::models::WorkspacePaths,
    started: &Instant,
    timing: &mut AnalysisTimingLog,
) -> Result<AnalyzeResponse, String> {
    let project_path = PathBuf::from(&request.project_path);

    timing.begin_step(2, "static", "코드 구조 분석");
    emit_analysis_progress(
        app,
        started,
        ProgressUpdate {
            phase: "static",
            label: "코드 구조를 분석하는 중입니다",
            detail: "파일, 코드 유닛, 참조와 실행 흐름을 추출하고 있습니다.",
            percent: 20,
            step: 2,
            current: None,
            total: None,
            indeterminate: true,
        },
    );
    let static_lines = run_engine_with_progress(
        engine,
        [
            project_path.as_os_str(),
            OsString::from(format!("--config={}", workspace.config.display())).as_os_str(),
            OsString::from("--no-output").as_os_str(),
            OsString::from(format!("--clean-output={}", workspace.clean_output.display())).as_os_str(),
            OsString::from("--no-cache").as_os_str(),
            OsString::from("--profile").as_os_str(),
        ],
        |_| {},
    )?;
    timing.append_engine_profile(&static_lines);

    timing.begin_step(3, "context", "AI 컨텍스트 생성");
    emit_analysis_progress(
        app,
        started,
        ProgressUpdate {
            phase: "context",
            label: "분석 결과를 정리하는 중입니다",
            detail: "AI가 읽을 핵심 도메인·기능·흐름 컨텍스트를 만들고 있습니다.",
            percent: 40,
            step: 3,
            current: None,
            total: None,
            indeterminate: true,
        },
    );
    let context_lines = run_engine_with_progress(
        engine,
        [
            OsString::from("postprocess").as_os_str(),
            OsString::from("ai-context").as_os_str(),
            OsString::from(format!("--input={}", workspace.clean_output.display())).as_os_str(),
            OsString::from(format!("--output={}", workspace.context_output.display())).as_os_str(),
            OsString::from(format!("--config={}", workspace.config.display())).as_os_str(),
            OsString::from("--pretty").as_os_str(),
            OsString::from("--profile").as_os_str(),
        ],
        |_| {},
    )?;
    timing.append_engine_profile(&context_lines);

    let provider_label = if request.provider == "claude" {
        "Claude"
    } else {
        "Codex"
    };
    timing.begin_step(4, "semantic", "의미 분석");
    emit_analysis_progress(
        app,
        started,
        ProgressUpdate {
            phase: "semantic",
            label: &format!("{provider_label}가 의미를 분석하는 중입니다"),
            detail: "비즈니스 도메인 이름만 AI로 생성합니다. 기능과 실행 흐름은 정적 라벨을 사용합니다.",
            percent: 60,
            step: 4,
            current: None,
            total: None,
            indeterminate: true,
        },
    );
    let provider_arg = OsString::from(format!("--provider={}", request.provider));
    let model_arg = OsString::from(format!("--model={}", request.model));
    let progress_app = app.clone();
    let progress_label = format!("{provider_label}가 의미를 분석하는 중입니다");
    let semantic_lines = run_engine_with_progress(
        engine,
        [
            OsString::from("semantic").as_os_str(),
            OsString::from("review").as_os_str(),
            OsString::from(format!("--input={}", workspace.context_output.display())).as_os_str(),
            OsString::from(format!("--output={}", workspace.semantic_output.display())).as_os_str(),
            OsString::from(format!("--project-root={}", project_path.display())).as_os_str(),
            OsString::from(format!("--config={}", workspace.config.display())).as_os_str(),
            model_arg.as_os_str(),
            provider_arg.as_os_str(),
            OsString::from("--profile").as_os_str(),
        ],
        |line| {
            let Some(event) = parse_semantic_progress(line) else {
                return;
            };
            let Some(update) = semantic_progress_update(&event, provider_label) else {
                return;
            };
            let percent = if update.total == 0 {
                60
            } else {
                let completed_percent = update
                    .completed
                    .min(update.total)
                    .saturating_mul(20)
                    .checked_div(update.total)
                    .unwrap_or_default();
                60 + completed_percent as u8
            };
            emit_analysis_progress(
                &progress_app,
                started,
                ProgressUpdate {
                    phase: "semantic",
                    label: &progress_label,
                    detail: &update.detail,
                    percent,
                    step: 4,
                    current: Some(update.completed),
                    total: Some(update.total),
                    indeterminate: update.indeterminate,
                },
            );
        },
    )?;
    timing.append_engine_profile(&semantic_lines);

    timing.begin_step(5, "finalizing", "결과 정리");
    emit_analysis_progress(
        app,
        started,
        ProgressUpdate {
            phase: "finalizing",
            label: "분석 결과를 준비하는 중입니다",
            detail: "생성된 의미 정보를 워크스페이스에 저장하고 있습니다.",
            percent: 80,
            step: 5,
            current: None,
            total: None,
            indeterminate: true,
        },
    );
    let semantic_domains = crate::semantic::load_semantic_domains_or_error(&workspace.semantic_output)?;
    let (domains, stats) = build_map(&workspace.clean_output, &semantic_domains)?;
    emit_analysis_progress(
        app,
        started,
        ProgressUpdate {
            phase: "complete",
            label: "분석이 완료되었습니다",
            detail: "도메인 지도를 준비했습니다.",
            percent: 100,
            step: 5,
            current: None,
            total: None,
            indeterminate: false,
        },
    );
    Ok(AnalyzeResponse {
        project_path: request.project_path.clone(),
        workspace_path: workspace.directory.display().to_string(),
        semantic_result_path: workspace.semantic_output.display().to_string(),
        domains,
        stats,
    })
}

struct ProgressUpdate<'a> {
    phase: &'a str,
    label: &'a str,
    detail: &'a str,
    percent: u8,
    step: u8,
    current: Option<usize>,
    total: Option<usize>,
    indeterminate: bool,
}

fn emit_analysis_progress(app: &AppHandle, started: &Instant, update: ProgressUpdate<'_>) {
    let _ = app.emit(
        "analysis-progress",
        AnalysisProgress {
            phase: update.phase.to_string(),
            label: update.label.to_string(),
            detail: update.detail.to_string(),
            percent: update.percent,
            step: update.step,
            total_steps: 5,
            current: update.current,
            total: update.total,
            indeterminate: update.indeterminate,
            elapsed_ms: started.elapsed().as_millis(),
        },
    );
}

struct SemanticProgressUpdate {
    completed: usize,
    total: usize,
    detail: String,
    indeterminate: bool,
}

fn semantic_progress_update(
    event: &SemanticProgressEvent,
    provider_label: &str,
) -> Option<SemanticProgressUpdate> {
    let total = event.total?;
    let stage_label = semantic_stage_label(event.stage.as_deref());
    let status = event.status.as_deref().unwrap_or_default();
    match status {
        "stage_started" => Some(SemanticProgressUpdate {
            completed: 0,
            total,
            detail: format!("{stage_label} 단계를 시작했습니다. 총 {total}개 청크를 {provider_label}에 보냅니다."),
            indeterminate: true,
        }),
        "started" => {
            let chunk = event.chunk.unwrap_or(1);
            let completed = chunk.saturating_sub(1);
            Some(SemanticProgressUpdate {
                completed,
                total,
                detail: format!(
                    "{stage_label} 청크 {chunk}/{total} — {provider_label} 응답을 기다리는 중입니다."
                ),
                indeterminate: true,
            })
        }
        "completed" => {
            let chunk = event.chunk.unwrap_or(event.completed.unwrap_or(0));
            Some(SemanticProgressUpdate {
                completed: chunk,
                total,
                detail: format!("{stage_label} 청크 {chunk}/{total}을 완료했습니다."),
                indeterminate: chunk < total,
            })
        }
        "failed" => {
            let chunk = event.chunk.unwrap_or(1);
            Some(SemanticProgressUpdate {
                completed: chunk.saturating_sub(1),
                total,
                detail: format!("{stage_label} 청크 {chunk}/{total}이 실패했습니다."),
                indeterminate: chunk < total,
            })
        }
        _ => None,
    }
}

fn semantic_stage_label(stage: Option<&str>) -> &'static str {
    match stage {
        Some("domain") => "도메인",
        Some("feature") => "기능",
        Some("flow") => "실행 흐름",
        _ => "의미",
    }
}
