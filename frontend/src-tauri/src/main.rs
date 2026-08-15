#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod analysis;
mod map;
mod models;
mod process;
mod projects;
mod semantic;
mod storage;
mod timing;

fn main() {
    run()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            analysis::analyze_project,
            analysis::get_codex_models,
            analysis::get_claude_models,
            analysis::get_claude_status,
            analysis::save_ai_settings,
            projects::list_saved_projects,
            projects::load_project_map,
        ])
        .run(tauri::generate_context!())
        .expect("error while running VisualMap");
}
