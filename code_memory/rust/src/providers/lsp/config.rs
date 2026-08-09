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
    java_language_server_settings_for_network(
        source_only,
        env::var("CODE_MEMORY_ALLOW_NETWORK").as_deref() == Ok("1"),
        env::var("CODE_MEMORY_JAVA_TOOLCHAIN_PATHS").ok().as_deref(),
    )
}

fn java_language_server_settings_for_network(
    source_only: bool,
    allow_network: bool,
    toolchain_paths: Option<&str>,
) -> Value {
    let gradle_arguments = java_gradle_arguments(toolchain_paths);
    let runtimes = java_configuration_runtimes(toolchain_paths);
    let mut settings = serde_json::json!({
        "java": {
            "autobuild": {"enabled": false},
            "maxConcurrentBuilds": 1,
            "import": {
                "gradle": {
                    "enabled": !source_only,
                    "offline": {"enabled": !allow_network},
                    "wrapper": {"enabled": allow_network},
                    // Buildship only needs project and classpath models. Test
                    // execution cannot add code facts, but can add minutes and
                    // unrelated runtime dependencies during first analysis.
                    // Test source sets remain present in the imported model.
                    // `org.gradle.java.installations.paths` is a Gradle
                    // property. It must be sent as `-P` through the Gradle
                    // argument channel; a plain `-D` in the JVM argument
                    // channel was proven not to reach Spring's Tooling API
                    // model import on Windows.
                    "arguments": gradle_arguments
                },
                "maven": {
                    "enabled": !source_only,
                    "offline": {"enabled": !allow_network}
                }
            },
            "project": {
                "importOnFirstTimeStartup": if source_only { "disabled" } else { "automatic" }
            },
            "configuration": {},
            "references": {"includeDecompiledSources": false},
            // The product does not consume editor compiler diagnostics. They
            // are not semantic evidence and provider startup/timeout/OOM
            // failures are reported independently. JDTLS still parses and may
            // reconcile opened files, but this prevents the unused diagnostics
            // from being serialized and published to the client.
            "diagnostic": {"filter": ["**/*.java"]},
            "edit": {"validateAllOpenBuffersOnChanges": false},
            "compile": {"nullAnalysis": {"mode": "disabled"}}
        }
    });
    if !runtimes.is_empty() {
        settings["java"]["configuration"]["runtimes"] = Value::Array(runtimes);
    }
    settings
}

fn java_gradle_arguments(toolchain_paths: Option<&str>) -> String {
    let mut arguments = "-x test".to_string();
    let Some(paths) = toolchain_paths.map(str::trim).filter(|paths| !paths.is_empty()) else {
        return arguments;
    };
    // This value is parsed as a Gradle argument string by Buildship. Reject
    // characters that could inject a second argument, then normalize Windows
    // separators. Product-managed toolchains never need embedded quotes.
    if paths.chars().any(|character| character.is_control() || character == '"') {
        return arguments;
    }
    let paths = paths.replace('\\', "/");
    let property = format!("-Porg.gradle.java.installations.paths={paths}");
    if paths.chars().any(char::is_whitespace) {
        arguments.push_str(" \"");
        arguments.push_str(&property);
        arguments.push('"');
    } else {
        arguments.push(' ');
        arguments.push_str(&property);
    }
    arguments
}

fn java_configuration_runtimes(toolchain_paths: Option<&str>) -> Vec<Value> {
    let Some(paths) = toolchain_paths.map(str::trim).filter(|paths| !paths.is_empty()) else {
        return Vec::new();
    };
    let mut runtimes = paths
        .split(',')
        .filter_map(|raw_path| {
            let path = PathBuf::from(raw_path.trim());
            if !java_home_is_usable(&path) {
                return None;
            }
            let release = fs::read_to_string(path.join("release")).ok()?;
            let major = java_release_major(&release)?;
            let name = if major == 8 {
                "JavaSE-1.8".to_string()
            } else {
                format!("JavaSE-{major}")
            };
            Some((major, name, path.to_string_lossy().replace('\\', "/")))
        })
        .collect::<Vec<_>>();
    runtimes.sort();
    runtimes.dedup_by(|left, right| left.1 == right.1);
    let default_major = runtimes.iter().map(|runtime| runtime.0).max();
    runtimes
        .into_iter()
        .map(|(major, name, path)| {
            serde_json::json!({
                "name": name,
                "path": path,
                "default": Some(major) == default_major
            })
        })
        .collect()
}

fn java_release_major(release: &str) -> Option<u32> {
    let version = release.lines().find_map(|line| {
        line.trim()
            .strip_prefix("JAVA_VERSION=")
            .map(|value| value.trim_matches('"'))
    })?;
    let mut parts = version.split(['.', '-']);
    let first = parts.next()?.parse::<u32>().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

fn jdtls_heap_mb(file_count: usize, memory_budget_mb: Option<usize>) -> usize {
    const MIN_HEAP_MB: usize = 1_024;
    const MAX_HEAP_MB: usize = 8_192;
    const LOW_MEMORY_HEAP_MB: usize = 512;

    // JDTLS keeps project indexes and open working copies in one JVM. A fixed
    // 1 GiB heap OOMs on real multi-module repositories (Spring: 8,982 Java
    // files). Scale conservatively with the scheduled semantic workload while
    // reserving at least one quarter of the provider memory budget for native
    // memory, Gradle tooling, and the Rust coordinator.
    let desired = (MIN_HEAP_MB + file_count.div_ceil(3)).clamp(MIN_HEAP_MB, MAX_HEAP_MB);
    let safe_cap = memory_budget_mb
        .map(|budget| budget.saturating_mul(3) / 4)
        .unwrap_or(2_048)
        .max(LOW_MEMORY_HEAP_MB);
    desired.min(safe_cap).max(LOW_MEMORY_HEAP_MB)
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
