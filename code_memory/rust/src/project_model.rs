use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::{
    project_cache_root, provider_timeout, run_bounded_command, tool_command, Diagnostic,
    DiagnosticCode, FileRelationOutput,
};

const MAX_PROJECT_MODEL_STREAM_BYTES: usize = 128 * 1024 * 1024;

#[derive(Deserialize)]
struct ProjectModelJson {
    relations: Vec<FileRelationOutput>,
    #[serde(default)]
    modeled_files: Vec<String>,
    #[serde(default)]
    units: Vec<ProjectModelUnit>,
    #[serde(default)]
    call_ranges: HashMap<String, Vec<Vec<i32>>>,
    #[serde(default)]
    diagnostics: Vec<ProjectModelDiagnostic>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct ProjectModelUnit {
    pub(crate) id: String,
    pub(crate) config: Option<String>,
    pub(crate) base_config: Option<String>,
    pub(crate) files: Vec<String>,
    #[serde(default)]
    pub(crate) allow_js: bool,
    #[serde(default)]
    pub(crate) synthetic: bool,
    #[serde(skip)]
    pub(crate) generated_config: Option<std::path::PathBuf>,
}

#[derive(Deserialize)]
struct ProjectModelDiagnostic {
    level: String,
    message: String,
}

pub(crate) struct ProjectModelResult {
    pub(crate) relations: Vec<FileRelationOutput>,
    pub(crate) modeled_files: Vec<String>,
    pub(crate) units: Vec<ProjectModelUnit>,
    pub(crate) call_ranges: HashMap<String, Vec<Vec<i32>>>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

pub(crate) fn analyze_typescript_project(
    root: &Path,
    providers_root: Option<&Path>,
    cache_key: &str,
    reuse_cached_result: bool,
) -> Result<ProjectModelResult, String> {
    let providers_root = providers_root.ok_or("providers root is not configured")?;
    let script = providers_root.join("node").join("project-model.cjs");
    if !script.is_file() {
        return Err(format!(
            "missing project model provider {}",
            script.display()
        ));
    }
    let cache = project_cache_root(root).join(format!("tsjs-project-model-{cache_key}.json"));
    let bytes = match reuse_cached_result.then(|| fs::read(&cache)) {
        Some(Ok(bytes)) if serde_json::from_slice::<ProjectModelJson>(&bytes).is_ok() => {
            eprintln!("cached TypeScript/JavaScript project model");
            bytes
        }
        Some(Ok(_)) => {
            let _ = fs::remove_file(&cache);
            load_project_model_from_provider(providers_root, &script, root, &cache)?
        }
        Some(Err(_)) | None => {
            load_project_model_from_provider(providers_root, &script, root, &cache)?
        }
    };
    let model: ProjectModelJson = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid project model output: {error}"))?;
    let diagnostics = model
        .diagnostics
        .into_iter()
        .map(|diagnostic| Diagnostic {
            language: "typescript".to_string(),
            level: match diagnostic.level.as_str() {
                "error" => "error",
                "warning" => "warning",
                _ => "info",
            },
            code: DiagnosticCode::ProviderDiagnostic,
            message: diagnostic.message,
            detail: None,
            path: None,
            line: None,
        })
        .collect();
    Ok(ProjectModelResult {
        relations: dedupe_relations(model.relations),
        modeled_files: model.modeled_files,
        units: model.units,
        call_ranges: model.call_ranges,
        diagnostics,
    })
}

fn load_project_model_from_provider(
    providers_root: &Path,
    script: &Path,
    root: &Path,
    cache: &Path,
) -> Result<Vec<u8>, String> {
    let mut command = tool_command("node", Some(providers_root))?;
    command.arg(script).arg(root);
    let output = run_project_model_command(command)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "project model provider exited with {}: {}",
            output.status,
            stderr.trim()
        ));
    }
    if let Some(parent) = cache.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create project model cache: {error}"))?;
    }
    fs::write(cache, &output.stdout)
        .map_err(|error| format!("cannot write project model cache: {error}"))?;
    Ok(output.stdout)
}

fn run_project_model_command(
    command: std::process::Command,
) -> Result<std::process::Output, String> {
    let output = run_bounded_command(
        command,
        "project model provider",
        provider_timeout(),
        MAX_PROJECT_MODEL_STREAM_BYTES,
        MAX_PROJECT_MODEL_STREAM_BYTES,
    )?;
    if output.stdout_truncated {
        return Err(format!(
            "project model provider stdout exceeded {} bytes",
            MAX_PROJECT_MODEL_STREAM_BYTES
        ));
    }
    if output.stderr_truncated {
        eprintln!(
            "project model provider stderr exceeded {} bytes; retained failure tail",
            MAX_PROJECT_MODEL_STREAM_BYTES
        );
    }
    Ok(std::process::Output {
        status: output.status,
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

fn dedupe_relations(relations: Vec<FileRelationOutput>) -> Vec<FileRelationOutput> {
    let mut seen = std::collections::HashSet::new();
    relations
        .into_iter()
        .filter(|relation| {
            seen.insert((
                relation.from.clone(),
                relation.to.clone(),
                relation.kind.clone(),
                relation.path.clone(),
                relation.range.clone(),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::MAX_PROJECT_MODEL_STREAM_BYTES;

    #[test]
    fn project_model_stream_limit_is_finite() {
        assert_eq!(MAX_PROJECT_MODEL_STREAM_BYTES, 128 * 1024 * 1024);
    }
}
