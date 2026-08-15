use crate::map::build_map;
use crate::models::AnalyzeResponse;
use crate::semantic::load_semantic_domains;
use crate::storage::{list_workspaces, workspace_directory_for_project};
use std::path::Path;

#[tauri::command]
pub(crate) fn list_saved_projects() -> Result<Vec<crate::models::WorkspaceMetadata>, String> {
    list_workspaces()
}

#[tauri::command]
pub(crate) async fn load_project_map(project_path: String) -> Result<AnalyzeResponse, String> {
    tauri::async_runtime::spawn_blocking(move || load_project_map_blocking(Path::new(&project_path)))
        .await
        .map_err(|error| format!("저장된 분석을 불러오지 못했습니다: {error}"))?
}

fn load_project_map_blocking(project_path: &Path) -> Result<AnalyzeResponse, String> {
    let workspace_dir = workspace_directory_for_project(project_path)?;
    let clean_output = workspace_dir.join("analysis/clean");
    if !clean_output.join("manifest.json").is_file() {
        return Err("저장된 분석 결과가 없습니다.".to_string());
    }

    let semantic_path = workspace_dir.join("analysis/ai/semantic-result.json");
    let semantic_domains = load_semantic_domains(&semantic_path);

    let (domains, stats) = build_map(&clean_output, &semantic_domains)?;
    Ok(AnalyzeResponse {
        project_path: project_path.display().to_string(),
        workspace_path: workspace_dir.display().to_string(),
        semantic_result_path: semantic_path.display().to_string(),
        domains,
        stats,
    })
}
