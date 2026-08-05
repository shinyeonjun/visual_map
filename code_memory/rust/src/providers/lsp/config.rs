fn configure_lsp_workspace(
    connection: &mut LspConnection,
    server: &str,
    _language: &str,
    root: &Path,
    java_source_only: bool,
) -> Result<(), String> {
    let settings = match server {
        "rust-analyzer" => rust_analyzer_settings(),
        "jdtls" => java_language_server_settings(java_source_only),
        _ => serde_json::json!({}),
    };
    connection.set_workspace_settings(settings.clone());
    if server != "rust-analyzer" {
        connection.notify(
            "workspace/didChangeConfiguration",
            serde_json::json!({"settings":settings}),
        )?;
    }
    if server == "dart" {
        connection.notify(
            "analysis.setAnalysisRoots",
            serde_json::json!({
                "included": [path_to_uri(root)],
                "excluded": []
            }),
        )?;
    }
    Ok(())
}

fn rust_analyzer_settings() -> Value {
    serde_json::json!({
        "rust-analyzer": {
            "checkOnSave": {"enable": false},
            "cargo": {
                "noDeps": true,
                // The bundled rust-src Cargo manifests reference crates.io
                // packages that are not part of a user's project. Loading
                // that sysroot in offline mode can stall or empty otherwise
                // valid project semantics, so keep the provider focused on
                // the repository graph.
                "sysroot": null,
                "allTargets": false,
                "autoreload": false,
                "buildScripts": {"enable": false},
                "loadOutDirsFromCheck": false
            },
            "procMacro": {"enable": false}
        }
    })
}

fn java_language_server_settings(source_only: bool) -> Value {
    serde_json::json!({
        "java": {
            "autobuild": {"enabled": false},
            "import": {
                "gradle": {
                    "enabled": !source_only,
                    "offline": {"enabled": true},
                    "wrapper": {"enabled": false}
                },
                "maven": {
                    "enabled": !source_only,
                    "offline": {"enabled": true}
                }
            },
            "project": {
                "importOnFirstTimeStartup": if source_only { "disabled" } else { "automatic" }
            },
            "references": {"includeDecompiledSources": false}
        }
    })
}

fn java_home_is_usable(path: &Path) -> bool {
    let executable = if cfg!(windows) { "java.exe" } else { "java" };
    path.join("bin").join(executable).is_file()
}

fn configuration_value(settings: &Value, section: &str) -> Option<Value> {
    section
        .split('.')
        .try_fold(settings, |value, part| value.get(part))
        .cloned()
}

pub(crate) fn is_benign_provider_stderr(server: &str, line: &str) -> bool {
    let trimmed = line.trim_start();
    (server == "rust-analyzer"
        && line.contains("notify error: Input watch path is neither a file nor a directory"))
        || (server == "clangd"
            && (trimmed.starts_with("I[")
                || trimmed.starts_with("argv[")
                || trimmed.contains("Found definition heuristically")
                || trimmed.starts_with("[") && trimmed.ends_with("]")
                || trimmed.contains("--driver-mode=")
                || trimmed.contains("-resource-dir=")))
}

pub(crate) fn lsp_workspace_root(lang: &LanguageSpec, root: &Path, files: &[PathBuf]) -> PathBuf {
    // Module planning already selected an explicit package/crate root. Do not
    // silently widen it back to a parent workspace: that recreates the large
    // single-session failure this planner is meant to avoid.
    if matches!(lang.id, "java" | "rust" | "dart") && workspace_has_marker(lang.id, root) {
        return root.to_path_buf();
    }
    if let Some(workspace_root) = ancestor_workspace_root(lang.id, root) {
        return workspace_root;
    }
    let mut candidates = HashSet::new();
    for file in files {
        if let Some(candidate) = file
            .ancestors()
            .take_while(|ancestor| ancestor.starts_with(root))
            .find(|ancestor| workspace_has_marker(lang.id, ancestor))
        {
            candidates.insert(candidate.to_path_buf());
        }
    }
    // A single nested module can use its own project root. Multiple modules
    // stay at the caller root so one module cannot hide the others.
    if candidates.len() == 1 {
        candidates
            .into_iter()
            .next()
            .unwrap_or_else(|| root.to_path_buf())
    } else {
        root.to_path_buf()
    }
}
