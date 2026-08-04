pub(crate) fn run_scip_indexer(
    lang: &LanguageSpec,
    root: &Path,
    out: &Path,
    providers_root: Option<&Path>,
    provider_config: Option<&Path>,
    source_files: &[PathBuf],
    project_config_digest: u64,
) -> Result<Vec<Diagnostic>, String> {
    let php_source_only = lang.id == "php" && php_dependency_metadata_gap(root);
    let php_workspace = if php_source_only {
        let provider_autoload = scip_php_provider_autoload(providers_root).ok_or_else(|| {
            "managed scip-php runtime is missing its bundled Composer autoloader".to_string()
        })?;
        Some(prepare_php_source_only_workspace(
            root,
            out,
            source_files,
            &provider_autoload,
        )?)
    } else {
        None
    };
    let command_root = php_workspace
        .as_ref()
        .map(|workspace| workspace.root.as_path())
        .unwrap_or(root);
    let command_source_files = php_workspace
        .as_ref()
        .map(|workspace| workspace.files.as_slice())
        .unwrap_or(source_files);
    let mut command = tool_command(lang.tool, providers_root)?;
    let mut generated_solution = None;
    let mut dotnet_restore_state = None;
    let mut php_include_file = None;
    command
        .current_dir(command_root)
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped());
    if lang.tool == "scip-clang" {
        let compdb = root.join("compile_commands.json");
        if !compdb.is_file() {
            return Err(format!("{} requires compile_commands.json", lang.name));
        }
        command.arg(format!("--compdb-path={}", compdb.display()));
    } else {
        command.arg("index");
        if lang.tool == "scip-dotnet" {
            let solution = if let Some(solution) = select_dotnet_solution(root, source_files) {
                solution
            } else {
                let solution = generated_dotnet_solution(root, out)?;
                generated_solution = Some(solution.clone());
                solution
            };
            let skip_restore = dotnet_restore_is_current(root, &solution, project_config_digest);
            if skip_restore {
                command.arg("--skip-dotnet-restore");
            }
            dotnet_restore_state = Some((solution.clone(), project_config_digest));
            command.arg(solution);
        }
        command.arg(format!("--output={}", out.display()));
        if lang.id == "php" {
            let php_files: Vec<_> = command_source_files
                .iter()
                .filter(|file| {
                    file.extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("php"))
                })
                .collect();
            if !php_files.is_empty() {
                let include_file = out.with_extension("php-files.txt");
                let content = php_files
                    .iter()
                    .map(|file| file.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("\n");
                fs::write(&include_file, content)
                    .map_err(|error| format!("cannot write PHP include list: {error}"))?;
                command.arg(format!("--include-file={}", include_file.display()));
                php_include_file = Some(include_file);
            }
        }
        if matches!(lang.id, "javascript" | "typescript") {
            if let Some(config) = provider_config {
                command.arg("--cwd").arg(root);
                command.arg("--workspace-root").arg(root);
                command.arg(config);
            } else {
                let configs = typescript_config_files(root);
                if configs.is_empty() {
                    let workspace = javascript_workspace(root, lang.id);
                    fs::create_dir_all(&workspace)
                        .map_err(|e| format!("cannot create JavaScript workspace: {e}"))?;
                    command.arg("--cwd").arg(&workspace);
                    command.args(["--infer-tsconfig", "--no-progress-bar"]);
                    command.arg(root);
                } else {
                    // Keep project resolution inside scip-typescript. It handles
                    // projectReferences and package/module resolution from each
                    // config better than a Rust-side file partition can.
                    for config in configs {
                        command.arg(config);
                    }
                }
            }
            if source_files.len() >= 2_000 {
                // ponytail: disable the provider's cross-project source cache
                // for large inputs; this trades repeated parsing for bounded RAM.
                command.arg("--no-global-caches");
            }
        }
    }
    let mut diagnostics = Vec::new();
    if php_source_only {
        diagnostics.push(Diagnostic {
            language: lang.id.to_string(),
            level: "warning",
            code: DiagnosticCode::DependencyMetadataGap,
            message: "Composer dependency metadata is unavailable; a temporary exact-file workspace retained project declarations and local relationships while leaving unavailable packages external".to_string(),
            detail: None,
            path: Some("composer.json".to_string()),
            line: None,
        });
    }
    let mut result = run_command(command, lang.name, lang.tool)
        .and_then(|_| ensure_default_scip_output(command_root, out));
    if result.is_err()
        && matches!(lang.id, "javascript" | "typescript")
        && provider_config.is_some()
    {
        let original_error = result.expect_err("checked above");
        let _ = fs::remove_file(out);
        result = run_typescript_source_only_fallback(
            lang,
            root,
            out,
            providers_root,
            source_files,
        );
        if result.is_ok() {
            diagnostics.push(Diagnostic {
                language: lang.id.to_string(),
                level: "warning",
                code: DiagnosticCode::TypescriptSourceFallback,
                message: "Configured TypeScript project failed; source-only fallback retained local declarations and relationships without claiming complete package resolution".to_string(),
                detail: Some(original_error),
                path: None,
                line: None,
            });
        }
    }
    if result.is_ok() {
        if let Some(workspace) = php_workspace.as_ref() {
            result = rewrite_php_source_only_paths(out, &workspace.root);
        }
        if let Some((solution, digest)) = dotnet_restore_state {
            let _ = write_dotnet_restore_state(root, &solution, digest);
        }
    }
    if let Some(solution) = generated_solution {
        if env::var("CODE_MEMORY_KEEP_GENERATED_SOLUTION").as_deref() != Ok("1") {
            let _ = fs::remove_file(solution);
        }
    }
    if let Some(include_file) = php_include_file {
        let _ = fs::remove_file(include_file);
    }
    if let Some(workspace) = php_workspace {
        let _ = fs::remove_dir_all(workspace.root);
    }
    result.map(|()| diagnostics)
}

fn rewrite_php_source_only_paths(out: &Path, workspace: &Path) -> Result<(), String> {
    let bytes = fs::read(out)
        .map_err(|error| format!("cannot read PHP source-only SCIP output: {error}"))?;
    let mut index = scip::types::Index::parse_from_bytes(&bytes)
        .map_err(|error| format!("cannot parse PHP source-only SCIP output: {error}"))?;
    for document in &mut index.documents {
        document.relative_path = normalize_scip_path(&document.relative_path, workspace);
    }
    scip::write_message_to_file(out, index)
        .map_err(|error| format!("cannot rewrite PHP source-only SCIP paths: {error}"))
}

struct PhpSourceOnlyWorkspace {
    root: PathBuf,
    files: Vec<PathBuf>,
}

pub(crate) fn php_dependency_metadata_gap(root: &Path) -> bool {
    root.join("composer.json").is_file()
        && (!root.join("vendor").join("autoload.php").is_file()
            || !root
                .join("vendor")
                .join("composer")
                .join("installed.php")
                .is_file())
}

fn scip_php_provider_autoload(providers_root: Option<&Path>) -> Option<PathBuf> {
    let tool = find_tool("scip-php", providers_root)?;
    let parent = tool.parent()?;
    [
        parent.join("scip-php").join("vendor").join("autoload.php"),
        parent.join("vendor").join("autoload.php"),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

fn prepare_php_source_only_workspace(
    root: &Path,
    out: &Path,
    source_files: &[PathBuf],
    provider_autoload: &Path,
) -> Result<PhpSourceOnlyWorkspace, String> {
    let workspace = out.with_extension("php-source-only");
    if workspace.exists() {
        fs::remove_dir_all(&workspace).map_err(|error| {
            format!(
                "cannot reset PHP source-only workspace {}: {error}",
                workspace.display()
            )
        })?;
    }
    fs::create_dir_all(workspace.join("vendor").join("composer"))
        .map_err(|error| format!("cannot create PHP source-only workspace: {error}"))?;

    let mut copied = Vec::with_capacity(source_files.len());
    let mut relative_files = Vec::with_capacity(source_files.len());
    for source in source_files {
        let relative = source.strip_prefix(root).map_err(|_| {
            format!(
                "PHP source file is outside the analysis root: {}",
                source.display()
            )
        })?;
        let destination = workspace.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create PHP source directory: {error}"))?;
        }
        fs::copy(source, &destination).map_err(|error| {
            format!(
                "cannot copy PHP source {} into the isolated workspace: {error}",
                source.display()
            )
        })?;
        copied.push(destination);
        relative_files.push(relative.to_string_lossy().replace('\\', "/"));
    }

    let (package_name, package_version) = fs::read(root.join("composer.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .map(|value| {
            (
                value
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|name| !name.is_empty())
                    .unwrap_or("code-memory/source-only")
                    .to_string(),
                value
                    .get("version")
                    .and_then(Value::as_str)
                    .filter(|version| !version.is_empty())
                    .unwrap_or("0.0.0+source-only")
                    .to_string(),
            )
        })
        .unwrap_or_else(|| {
            (
                "code-memory/source-only".to_string(),
                "0.0.0+source-only".to_string(),
            )
        });
    let composer = serde_json::json!({
        "name": package_name,
        "version": package_version,
        "autoload": { "files": relative_files }
    });
    fs::write(
        workspace.join("composer.json"),
        serde_json::to_vec(&composer)
            .map_err(|error| format!("cannot serialize PHP source-only manifest: {error}"))?,
    )
    .map_err(|error| format!("cannot write PHP source-only manifest: {error}"))?;
    fs::write(workspace.join("composer.lock"), b"{\"packages\":[]}")
        .map_err(|error| format!("cannot write PHP source-only lockfile: {error}"))?;

    let autoload_path = provider_autoload.to_string_lossy().replace('\\', "/");
    let autoload_path = autoload_path.replace('\'', "\\'");
    fs::write(
        workspace.join("vendor").join("autoload.php"),
        format!("<?php return require '{autoload_path}';\n"),
    )
    .map_err(|error| format!("cannot write PHP source-only autoloader: {error}"))?;
    let installed = format!(
        "<?php return ['root' => ['name' => {name}, 'version' => {version}, 'reference' => null], 'versions' => []];\n",
        name = serde_json::to_string(&package_name)
            .map_err(|error| format!("cannot encode PHP package name: {error}"))?,
        version = serde_json::to_string(&package_version)
            .map_err(|error| format!("cannot encode PHP package version: {error}"))?,
    );
    fs::write(
        workspace
            .join("vendor")
            .join("composer")
            .join("installed.php"),
        installed,
    )
    .map_err(|error| format!("cannot write PHP source-only package metadata: {error}"))?;

    Ok(PhpSourceOnlyWorkspace {
        root: workspace,
        files: copied,
    })
}

fn run_typescript_source_only_fallback(
    lang: &LanguageSpec,
    root: &Path,
    out: &Path,
    providers_root: Option<&Path>,
    source_files: &[PathBuf],
) -> Result<(), String> {
    let config_path = out.with_extension("source-only.tsconfig.json");
    let config = typescript_source_only_config(lang.id == "javascript", source_files);
    fs::write(
        &config_path,
        serde_json::to_vec(&config)
            .map_err(|error| format!("cannot serialize TypeScript fallback config: {error}"))?,
    )
    .map_err(|error| format!("cannot write {}: {error}", config_path.display()))?;

    let result = (|| {
        let mut command = tool_command(lang.tool, providers_root)?;
        command
            .current_dir(root)
            .stdout(Stdio::inherit())
            .stderr(Stdio::piped())
            .arg("index")
            .arg(format!("--output={}", out.display()))
            .arg("--cwd")
            .arg(root)
            .arg("--workspace-root")
            .arg(root)
            .arg(&config_path);
        if source_files.len() >= 2_000 {
            command.arg("--no-global-caches");
        }
        run_command(command, lang.name, lang.tool)
            .and_then(|_| ensure_default_scip_output(root, out))
    })();
    let _ = fs::remove_file(config_path);
    result
}

fn typescript_source_only_config(allow_js: bool, source_files: &[PathBuf]) -> serde_json::Value {
    serde_json::json!({
        "compilerOptions": {
            "allowJs": allow_js,
            "checkJs": false,
            "experimentalDecorators": true,
            "jsx": "preserve",
            "module": "ESNext",
            "moduleResolution": "node",
            "noEmit": true,
            "skipLibCheck": true,
            "target": "ES2022"
        },
        "files": source_files
            .iter()
            .map(|file| file.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
    })
}

#[cfg(test)]
mod runner_tests {
    use super::*;

    #[test]
    fn typescript_source_only_config_has_exact_files_and_no_extends() {
        let files = vec![
            PathBuf::from("C:/repo/src/a.ts"),
            PathBuf::from("C:/repo/src/b.tsx"),
        ];
        let config = typescript_source_only_config(false, &files);

        assert!(config.get("extends").is_none());
        assert_eq!(config["files"].as_array().map(Vec::len), Some(2));
        assert_eq!(config["compilerOptions"]["allowJs"], false);
        assert_eq!(config["compilerOptions"]["noEmit"], true);
        assert_eq!(
            serde_json::to_string(&DiagnosticCode::TypescriptSourceFallback).unwrap(),
            "\"typescript-source-fallback\""
        );
    }

    #[test]
    fn php_source_only_workspace_copies_only_requested_project_files() {
        let root = std::env::temp_dir().join(format!(
            "code-memory-php-source-only-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("composer.json"),
            br#"{"name":"visualmap/source-only","require":{"vendor/pkg":"*"}}"#,
        )
        .unwrap();
        let source = root.join("src").join("Handler.php");
        fs::write(&source, "<?php final class Handler {}\n").unwrap();
        let provider_autoload = root.join("provider-autoload.php");
        fs::write(&provider_autoload, "<?php return null;\n").unwrap();
        let out = root.join("result.scip");

        let workspace = prepare_php_source_only_workspace(
            &root,
            &out,
            std::slice::from_ref(&source),
            &provider_autoload,
        )
        .unwrap();
        let manifest: Value = serde_json::from_slice(
            &fs::read(workspace.root.join("composer.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["name"], "visualmap/source-only");
        assert_eq!(manifest["autoload"]["files"][0], "src/Handler.php");
        assert!(workspace.root.join("src").join("Handler.php").is_file());
        assert!(workspace
            .root
            .join("vendor")
            .join("composer")
            .join("installed.php")
            .is_file());
        let _ = fs::remove_dir_all(root);
    }
}
