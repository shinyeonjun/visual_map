use crate::models::{
    AnalysisProgress, AnalyzeRequest, AnalyzeResponse, CodexModelCatalog, CodexModelOption,
    CodexSettings, RawCodexModelCatalog, SemanticResult,
};
use crate::process::{
    command_output, parse_semantic_progress, run_engine, run_engine_with_progress,
};
use crate::storage::{
    load_codex_settings, prepare_workspace, resolve_codex_cli, resolve_config, resolve_executable,
    write_codex_settings,
};
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};

#[tauri::command]
pub(crate) fn get_codex_models() -> Result<CodexModelCatalog, String> {
    let settings = load_codex_settings().unwrap_or_default();
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

    Ok(CodexModelCatalog {
        executable,
        version: version.lines().next().unwrap_or_default().to_string(),
        source: "bundled".to_string(),
        selected_model,
        models,
    })
}

#[tauri::command]
pub(crate) fn save_codex_settings(
    model: String,
    cli_version: String,
    executable: String,
) -> Result<(), String> {
    if model.trim().is_empty() {
        return Err("저장할 Codex 모델이 없습니다.".to_string());
    }
    let settings = CodexSettings {
        executable,
        model,
        cli_version,
        catalog_source: "bundled".to_string(),
    };
    write_codex_settings(&settings)
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
    let project_path = PathBuf::from(&request.project_path);
    if !project_path.is_dir() {
        return Err(format!(
            "프로젝트 폴더를 찾을 수 없습니다: {}",
            project_path.display()
        ));
    }

    let engine = resolve_executable(
        &request.engine_path,
        "VISUAL_MAP_ENGINE",
        "code-analysis-engine",
    );
    let resource_dir = app.path().resource_dir().ok();
    let source_config = resolve_config(&request.config_path, resource_dir.as_deref());
    if !source_config.is_file() {
        return Err(format!(
            "분석 설정 파일을 찾을 수 없습니다: {}",
            source_config.display()
        ));
    }
    let workspace = prepare_workspace(&project_path, &request.model, &source_config)?;

    emit_analysis_progress(
        &app,
        &started,
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
    run_engine(
        &engine,
        [
            project_path.as_os_str(),
            OsString::from(format!("--config={}", workspace.config.display())).as_os_str(),
            OsString::from(format!("--output={}", workspace.static_output.display())).as_os_str(),
            OsString::from("--no-cache").as_os_str(),
        ],
    )?;

    emit_analysis_progress(
        &app,
        &started,
        ProgressUpdate {
            phase: "context",
            label: "분석 결과를 정리하는 중입니다",
            detail: "Codex가 읽을 핵심 도메인·기능·흐름 컨텍스트를 만들고 있습니다.",
            percent: 40,
            step: 3,
            current: None,
            total: None,
            indeterminate: true,
        },
    );
    run_engine(
        &engine,
        [
            OsString::from("postprocess").as_os_str(),
            OsString::from("codex-context").as_os_str(),
            OsString::from(format!("--input={}", workspace.static_output.display())).as_os_str(),
            OsString::from(format!("--output={}", workspace.context_output.display())).as_os_str(),
            OsString::from(format!("--config={}", workspace.config.display())).as_os_str(),
            OsString::from("--pretty").as_os_str(),
        ],
    )?;

    emit_analysis_progress(
        &app,
        &started,
        ProgressUpdate {
            phase: "semantic",
            label: "Codex가 의미를 분석하는 중입니다",
            detail: "코드 구조를 사람이 이해할 수 있는 이름과 설명으로 바꾸고 있습니다.",
            percent: 60,
            step: 4,
            current: None,
            total: None,
            indeterminate: true,
        },
    );
    let progress_app = app.clone();
    run_engine_with_progress(
        &engine,
        [
            OsString::from("semantic").as_os_str(),
            OsString::from("review").as_os_str(),
            OsString::from(format!("--input={}", workspace.context_output.display())).as_os_str(),
            OsString::from(format!("--output={}", workspace.semantic_output.display())).as_os_str(),
            OsString::from(format!("--project-root={}", project_path.display())).as_os_str(),
            OsString::from(format!("--config={}", workspace.config.display())).as_os_str(),
            OsString::from(format!("--model={}", request.model)).as_os_str(),
        ],
        |line| {
            let Some((completed, total)) = parse_semantic_progress(line) else {
                return;
            };
            let percent = if total == 0 {
                60
            } else {
                let completed_percent = completed
                    .min(total)
                    .saturating_mul(20)
                    .checked_div(total)
                    .unwrap_or_default();
                60 + completed_percent as u8
            };
            let detail = if completed == 0 {
                "Codex 분석을 시작했습니다. 프로젝트 규모에 따라 시간이 걸릴 수 있습니다."
                    .to_string()
            } else {
                format!("Codex 분석 청크 {completed}/{total}을 완료했습니다.")
            };
            emit_analysis_progress(
                &progress_app,
                &started,
                ProgressUpdate {
                    phase: "semantic",
                    label: "Codex가 의미를 분석하는 중입니다",
                    detail: &detail,
                    percent,
                    step: 4,
                    current: Some(completed),
                    total: Some(total),
                    indeterminate: completed < total,
                },
            );
        },
    )?;

    emit_analysis_progress(
        &app,
        &started,
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
    let result = fs::read_to_string(&workspace.semantic_output)
        .map_err(|error| format!("의미 분석 결과를 읽지 못했습니다: {error}"))?;
    let semantic: SemanticResult = serde_json::from_str(&result)
        .map_err(|error| format!("의미 분석 결과 형식이 올바르지 않습니다: {error}"))?;
    emit_analysis_progress(
        &app,
        &started,
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
        project_path: request.project_path,
        workspace_path: workspace.directory.display().to_string(),
        semantic_result_path: workspace.semantic_output.display().to_string(),
        domains: semantic.domains,
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
