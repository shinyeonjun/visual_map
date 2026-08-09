pub(crate) fn run_scip_indexer(
    lang: &LanguageSpec,
    roots: ProviderRoots<'_>,
    out: &Path,
    providers_root: Option<&Path>,
    provider_config: Option<&Path>,
    source_files: &[PathBuf],
    max_project_source_file_bytes: u64,
) -> Result<ProviderRunOutcome, String> {
    let provider_configs = provider_config
        .into_iter()
        .map(Path::to_path_buf)
        .collect::<Vec<_>>();
    run_scip_indexer_with_configs(
        lang,
        roots,
        out,
        providers_root,
        &provider_configs,
        source_files,
        max_project_source_file_bytes,
    )
}

pub(crate) fn run_scip_indexer_with_configs(
    lang: &LanguageSpec,
    roots: ProviderRoots<'_>,
    out: &Path,
    providers_root: Option<&Path>,
    provider_configs: &[PathBuf],
    source_files: &[PathBuf],
    max_project_source_file_bytes: u64,
) -> Result<ProviderRunOutcome, String> {
    let project_root = roots.project;
    let root = roots.analysis;
    let canonical_project_root = project_root.canonicalize().map_err(|error| {
        format!(
            "cannot resolve selected project root {}: {error}",
            project_root.display()
        )
    })?;
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("cannot resolve {} root {}: {error}", lang.name, root.display()))?;
    if !canonical_root.starts_with(&canonical_project_root) {
        return Err(format!(
            "{} semantic root escaped the selected project root: {}",
            lang.name,
            root.display()
        ));
    }
    let mut mode = ProviderExecutionMode::Project;
    let mut explicit_context_files = Vec::new();
    let mut lineage_context_files = Vec::new();
    let mut generated_context_parts = Vec::<Vec<u8>>::new();
    let mut execution_dimensions = Vec::new();
    let mut direct_typescript_source_only = false;
    let mut command = tool_command(lang.tool, providers_root)?;
    let mut generated_solution = None;
    let mut mixed_dotnet_solution_filtered = false;
    command
        .current_dir(root)
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped());
    if lang.tool == "scip-clang" {
        let compdb = root.join("compile_commands.json");
        if !compdb.is_file() {
            return Err(format!("{} requires compile_commands.json", lang.name));
        }
        command.arg(format!("--compdb-path={}", compdb.display()));
        explicit_context_files.push(compdb);
    } else {
        command.arg("index");
        if lang.tool == "scip-dotnet" {
            let _resolved_sdk = probe_dotnet_sdk(root, providers_root)?;
            let selected_solution = select_dotnet_solution(root, source_files);
            let solution = if selected_solution
                .as_ref()
                .is_some_and(|solution| !solution_has_non_csharp_projects(solution))
            {
                let solution = selected_solution.expect("checked selected solution");
                explicit_context_files.push(solution.clone());
                solution
            } else {
                mixed_dotnet_solution_filtered = selected_solution.is_some();
                let solution = generated_dotnet_solution(root, out)?;
                if let Ok(bytes) = fs::read(&solution) {
                    generated_context_parts.push(bytes);
                }
                mode = ProviderExecutionMode::GeneratedProject;
                generated_solution = Some(solution.clone());
                solution
            };
            // scip-dotnet 0.2.14 can emit a semantically incomplete index
            // under --skip-dotnet-restore even when the project digest is
            // unchanged (a confirmed local call disappeared in the shadow
            // corpus). Restore is therefore part of the correctness boundary,
            // not an optional performance cache.
            command.arg(solution);
        }
        command.arg(format!("--output={}", out.display()));
        if matches!(lang.id, "javascript" | "typescript") {
            if !provider_configs.is_empty() {
                append_typescript_working_directory(&mut command, root);
                for config in provider_configs {
                    command.arg(config);
                    if config
                        .canonicalize()
                        .is_ok_and(|path| path.starts_with(&canonical_project_root))
                    {
                        explicit_context_files.push(config.to_path_buf());
                    } else {
                        if let Ok(bytes) = fs::read(config) {
                            generated_context_parts.push(bytes);
                        }
                        let lineage =
                            typescript_generated_config_lineage(config, &canonical_project_root);
                        lineage_context_files.extend(lineage);
                        mode = ProviderExecutionMode::GeneratedProject;
                    }
                }
            } else {
                // Never point scip-typescript --infer-tsconfig at the selected
                // source directory.  The provider writes tsconfig.json into the
                // target root; Prometheus proved this mutates the repository and
                // correctly trips source-stability after several minutes of Go
                // analysis.  Configless shards and files left outside modeled
                // projects both use one generated config beside the provider
                // output, with an exact `files` list and no source-tree writes.
                direct_typescript_source_only = true;
                mode = ProviderExecutionMode::SourceOnlyFallback;
                if let Some(value) = context_dimension(ContextDimensionKind::ModuleMode, "esnext") {
                    execution_dimensions.push(value);
                }
                if let Some(value) = context_dimension(ContextDimensionKind::Target, "es2022") {
                    execution_dimensions.push(value);
                }
            }
            // scip-typescript defaults to a 1 MB per-file ceiling. The Source
            // Census has already measured and admitted these exact files, so
            // derive the provider ceiling from the scheduled source set rather
            // than silently dropping a legitimate large source.
            command
                .arg("--max-file-byte-size")
                .arg(typescript_max_file_byte_size(
                    source_files,
                    max_project_source_file_bytes,
                )?);
            if source_files.len() >= 2_000 || provider_configs.len() > 1 {
                // scip-typescript's process-global source cache is safe for one
                // project, but a multi-project invocation can otherwise reuse a
                // source result across different config programs.  A real ESLint
                // shadow run lost CustomParserServices.program (and its two
                // relations) only in the batched form.  Keep process batching,
                // but isolate every config's compiler program so batching cannot
                // change the canonical facts.  Large single projects also use
                // the flag to keep memory bounded.
                command.arg("--no-global-caches");
            }
        }
    }
    let mut diagnostics = Vec::new();
    if mixed_dotnet_solution_filtered {
        diagnostics.push(Diagnostic {
            language: lang.id.to_string(),
            level: "info",
            code: DiagnosticCode::ProviderDiagnostic,
            message: "Mixed-language .NET solution was projected to its C# projects before semantic indexing".to_string(),
            detail: None,
            path: None,
            line: None,
        });
    }
    let mut result = if direct_typescript_source_only {
        let fallback = run_typescript_source_only_fallback(
            lang,
            root,
            out,
            providers_root,
            source_files,
            max_project_source_file_bytes,
        );
        if let Ok(digest) = fallback.as_ref() {
            generated_context_parts.push(digest.as_bytes().to_vec());
            diagnostics.push(Diagnostic {
                language: lang.id.to_string(),
                level: "warning",
                code: DiagnosticCode::TypescriptSourceFallback,
                message: "Sources without an owning modeled TypeScript/JavaScript project were analyzed in an isolated source-only project".to_string(),
                detail: None,
                path: None,
                line: None,
            });
        }
        fallback.map(|_| ())
    } else {
        run_command_with_timeout(
            command,
            lang.name,
            lang.tool,
            scip_index_timeout(lang.tool, source_files.len()),
        )
            .and_then(|_| ensure_default_scip_output(root, out))
    };
    if result.is_err()
        && matches!(lang.id, "javascript" | "typescript")
        && provider_configs.len() == 1
    {
        let original_error = result.expect_err("checked above");
        let _ = fs::remove_file(out);
        let fallback_result = run_typescript_source_only_fallback(
            lang,
            root,
            out,
            providers_root,
            source_files,
            max_project_source_file_bytes,
        );
        if let Ok(digest) = fallback_result.as_ref() {
            mode = ProviderExecutionMode::SourceOnlyFallback;
            // The successful retry did not execute the failed configured
            // project. Keep those files visible only as workspace context,
            // never as explicit arguments of the accepted result.
            explicit_context_files.clear();
            lineage_context_files.clear();
            generated_context_parts.clear();
            generated_context_parts.push(digest.as_bytes().to_vec());
            execution_dimensions.clear();
            if let Some(value) = context_dimension(ContextDimensionKind::ModuleMode, "esnext") {
                execution_dimensions.push(value);
            }
            if let Some(value) = context_dimension(ContextDimensionKind::Target, "es2022") {
                execution_dimensions.push(value);
            }
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
        result = fallback_result.map(|_| ());
    }
    if let Some(solution) = generated_solution {
        if env::var("CODE_MEMORY_KEEP_GENERATED_SOLUTION").as_deref() != Ok("1") {
            let _ = fs::remove_file(solution);
        }
    }
    result?;
    let execution_context = scip_execution_context(ScipExecutionContextInput {
        project_root,
        lang,
        analysis_root: root,
        source_files,
        mode,
        explicit_context_files,
        lineage_context_files,
        generated_context_parts,
        execution_dimensions,
    })?;
    Ok(ProviderRunOutcome {
        diagnostics,
        execution_context,
    })
}

pub(crate) fn configured_scip_execution_context(
    lang: &LanguageSpec,
    roots: ProviderRoots<'_>,
    config: &Path,
    source_files: &[PathBuf],
) -> Result<codebase_fact_model::analysis::ProviderExecutionContext, String> {
    let canonical_project_root = roots.project.canonicalize().map_err(|error| {
        format!(
            "cannot resolve selected project root {}: {error}",
            roots.project.display()
        )
    })?;
    let mut mode = ProviderExecutionMode::Project;
    let mut explicit_context_files = Vec::new();
    let mut lineage_context_files = Vec::new();
    let mut generated_context_parts = Vec::new();
    if config
        .canonicalize()
        .is_ok_and(|path| path.starts_with(&canonical_project_root))
    {
        explicit_context_files.push(config.to_path_buf());
    } else {
        let bytes = fs::read(config)
            .map_err(|error| format!("cannot read generated provider config: {error}"))?;
        generated_context_parts.push(bytes);
        lineage_context_files.extend(typescript_generated_config_lineage(
            config,
            &canonical_project_root,
        ));
        mode = ProviderExecutionMode::GeneratedProject;
    }
    scip_execution_context(ScipExecutionContextInput {
        project_root: roots.project,
        lang,
        analysis_root: roots.analysis,
        source_files,
        mode,
        explicit_context_files,
        lineage_context_files,
        generated_context_parts,
        execution_dimensions: Vec::new(),
    })
}

struct ScipExecutionContextInput<'a> {
    project_root: &'a Path,
    lang: &'a LanguageSpec,
    analysis_root: &'a Path,
    source_files: &'a [PathBuf],
    mode: ProviderExecutionMode,
    explicit_context_files: Vec<PathBuf>,
    lineage_context_files: Vec<PathBuf>,
    generated_context_parts: Vec<Vec<u8>>,
    execution_dimensions: Vec<codebase_fact_model::analysis::ContextDimension>,
}

fn scip_execution_context(
    input: ScipExecutionContextInput<'_>,
) -> Result<codebase_fact_model::analysis::ProviderExecutionContext, String> {
    let ScipExecutionContextInput {
        project_root,
        lang,
        analysis_root,
        source_files,
        mut mode,
        explicit_context_files,
        lineage_context_files,
        generated_context_parts,
        execution_dimensions,
    } = input;
    // A config can legitimately live above a planned package (for example a
    // monorepo tsconfig used by legacy/frontend). It is execution evidence,
    // not the execution boundary. The provider is launched with
    // `analysis_root` as both the process cwd and provider `--cwd`, so its
    // receipt must keep that AnalysisPlan-owned root instead of replacing it
    // with config.parent().
    let mut config_files = if mode == ProviderExecutionMode::SourceOnlyFallback {
        // A source-only run deliberately does not execute adjacent repository
        // configs. Recording them as workspace discovery would overstate the
        // provider context and can attach another AnalysisPlan unit's config to
        // this shard.
        Vec::new()
    } else {
        workspace_context_files(
            lang.contract_language,
            project_root,
            analysis_root,
            source_files,
        )
        .into_iter()
        .map(|path| (path, ProviderConfigUse::WorkspaceDiscovery))
        .collect::<Vec<_>>()
    };
    config_files.extend(
        explicit_context_files
            .into_iter()
            .map(|path| (path, ProviderConfigUse::ExplicitArgument)),
    );
    config_files.extend(
        lineage_context_files
            .into_iter()
            .map(|path| (path, ProviderConfigUse::GeneratedLineage)),
    );
    let generated_context_digest = (!generated_context_parts.is_empty())
        .then(|| generated_context_digest(&generated_context_parts));
    if mode == ProviderExecutionMode::Project && config_files.is_empty() {
        mode = ProviderExecutionMode::InferredWorkspace;
    }
    executed_provider_context(ExecutedProviderContextInput {
        project_root,
        language: lang,
        mode,
        analysis_root,
        source_files,
        config_files,
        generated_context_digest,
        dimensions: execution_dimensions,
    })
}

fn typescript_generated_config_lineage(config: &Path, project_root: &Path) -> Vec<PathBuf> {
    let Ok(bytes) = fs::read(config) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        return Vec::new();
    };
    let Some(extends) = value.get("extends").and_then(Value::as_str) else {
        return Vec::new();
    };
    let path = PathBuf::from(extends);
    path.canonicalize()
        .ok()
        .filter(|path| path.starts_with(project_root) && path.is_file())
        .into_iter()
        .collect()
}

fn run_typescript_source_only_fallback(
    lang: &LanguageSpec,
    root: &Path,
    out: &Path,
    providers_root: Option<&Path>,
    source_files: &[PathBuf],
    max_project_source_file_bytes: u64,
) -> Result<codebase_fact_model::identity::Sha256Digest, String> {
    let config_path = out.with_extension("source-only.tsconfig.json");
    let config = typescript_source_only_config(lang.id == "javascript", source_files);
    let config_bytes = serde_json::to_vec(&config)
        .map_err(|error| format!("cannot serialize TypeScript fallback config: {error}"))?;
    let config_digest = generated_context_digest(std::slice::from_ref(&config_bytes));
    fs::write(&config_path, &config_bytes)
    .map_err(|error| format!("cannot write {}: {error}", config_path.display()))?;

    let result = (|| {
        let mut command = tool_command(lang.tool, providers_root)?;
        command
            .current_dir(root)
            .stdout(Stdio::inherit())
            .stderr(Stdio::piped())
            .arg("index")
            .arg(format!("--output={}", out.display()));
        append_typescript_working_directory(&mut command, root);
        command
            .arg("--max-file-byte-size")
            .arg(typescript_max_file_byte_size(
                source_files,
                max_project_source_file_bytes,
            )?)
            .arg(&config_path);
        if source_files.len() >= 2_000 {
            command.arg("--no-global-caches");
        }
        run_command(command, lang.name, lang.tool)
            .and_then(|_| ensure_default_scip_output(root, out))
    })();
    let _ = fs::remove_file(config_path);
    result.map(|()| config_digest)
}

/// scip-typescript 0.4.0 exposes `--cwd`; it has no `--workspace-root` option.
/// Keep the provider CLI boundary in one place so configured and generated
/// project runs cannot silently drift apart.
fn append_typescript_working_directory(command: &mut Command, root: &Path) {
    command.arg("--cwd").arg(root);
}

fn typescript_max_file_byte_size(
    source_files: &[PathBuf],
    max_project_source_file_bytes: u64,
) -> Result<String, String> {
    let mut maximum = max_project_source_file_bytes.max(1);
    for file in source_files {
        let metadata = fs::metadata(file)
            .map_err(|error| format!("cannot measure scheduled source {}: {error}", file.display()))?;
        if !metadata.is_file() {
            return Err(format!(
                "scheduled TypeScript/JavaScript source is not a file: {}",
                file.display()
            ));
        }
        maximum = maximum.max(metadata.len());
    }
    Ok(maximum.to_string())
}

fn scip_index_timeout(tool: &str, source_file_count: usize) -> Duration {
    if env::var_os("CODE_MEMORY_PROVIDER_TIMEOUT_SECONDS").is_some() {
        return provider_timeout();
    }
    default_scip_index_timeout(tool, source_file_count)
}

fn default_scip_index_timeout(tool: &str, source_file_count: usize) -> Duration {
    if tool == "scip-dotnet" && source_file_count >= 1_000 {
        // scip-dotnet performs a correctness-critical restore before Roslyn
        // indexing.  A large solution can legitimately exceed the generic
        // three-minute process ceiling even on a warm package cache.  LSP
        // providers already receive the same large-workspace ceiling.
        Duration::from_secs(900)
    } else {
        provider_timeout()
    }
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
    use crate::LANGUAGES;

    #[test]
    fn large_dotnet_solution_uses_the_large_workspace_timeout() {
        assert_eq!(
            default_scip_index_timeout("scip-dotnet", 1_000),
            Duration::from_secs(900)
        );
        assert_eq!(
            default_scip_index_timeout("scip-dotnet", 999),
            Duration::from_secs(180)
        );
        assert_eq!(
            default_scip_index_timeout("scip-typescript", 10_000),
            Duration::from_secs(180)
        );
    }

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
    fn typescript_workspace_argument_matches_the_pinned_provider_cli() {
        let mut command = Command::new("scip-typescript");
        let root = Path::new("C:/repo");
        append_typescript_working_directory(&mut command, root);

        let arguments = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(arguments, vec!["--cwd", "C:/repo"]);
        assert!(!arguments.iter().any(|argument| argument == "--workspace-root"));
    }

    #[test]
    fn typescript_provider_limit_covers_scheduled_and_project_wide_sources() {
        let root = std::env::temp_dir().join(format!(
            "code-memory-typescript-large-source-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create TypeScript limit fixture");
        let small = root.join("small.ts");
        let large = root.join("large.ts");
        fs::write(&small, b"small").expect("write small TypeScript fixture");
        fs::write(&large, vec![b'x'; 1_100_123]).expect("write large TypeScript fixture");

        assert_eq!(
            typescript_max_file_byte_size(&[small.clone(), large.clone()], 1).unwrap(),
            "1100123"
        );
        assert_eq!(
            typescript_max_file_byte_size(&[small, large], 2_000_000).unwrap(),
            "2000000"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parent_typescript_config_does_not_replace_the_planned_analysis_root() {
        let project_root = std::env::temp_dir().join(format!(
            "code-memory-typescript-context-root-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&project_root);
        let analysis_root = project_root.join("legacy").join("frontend");
        let source = analysis_root.join("src").join("app.ts");
        fs::create_dir_all(source.parent().unwrap()).expect("create nested TypeScript fixture");
        let config = project_root.join("tsconfig.json");
        fs::write(&config, b"{\"compilerOptions\":{\"allowJs\":true}}")
            .expect("write parent TypeScript config");
        fs::write(&source, b"export const app = 1;\n").expect("write TypeScript source");
        let lang = LANGUAGES
            .iter()
            .find(|candidate| candidate.id == "typescript")
            .expect("TypeScript provider");

        let context = scip_execution_context(ScipExecutionContextInput {
            project_root: &project_root,
            lang,
            analysis_root: &analysis_root,
            source_files: std::slice::from_ref(&source),
            mode: ProviderExecutionMode::Project,
            explicit_context_files: vec![config],
            lineage_context_files: Vec::new(),
            generated_context_parts: Vec::new(),
            execution_dimensions: Vec::new(),
        })
        .expect("build exact nested execution context");

        assert_eq!(
            context.analysis_root.as_ref().map(|path| path.as_str()),
            Some("legacy/frontend")
        );
        assert!(context
            .config_artifacts
            .iter()
            .any(|artifact| artifact.path.as_str() == "tsconfig.json"));
        let _ = fs::remove_dir_all(project_root);
    }

    #[test]
    fn source_only_typescript_context_does_not_claim_adjacent_project_configs() {
        let project_root = std::env::temp_dir().join(format!(
            "code-memory-typescript-source-only-context-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&project_root);
        let source = project_root.join("fixtures").join("loose.js");
        fs::create_dir_all(source.parent().unwrap()).expect("create source-only fixture");
        fs::write(&source, b"export const loose = true;\n").expect("write source-only source");
        fs::write(
            project_root.join("fixtures").join("tsconfig.json"),
            b"{\"include\":[\"owned-by-another-unit.ts\"]}",
        )
        .expect("write adjacent project config");
        let lang = LANGUAGES
            .iter()
            .find(|candidate| candidate.id == "javascript")
            .expect("JavaScript provider");

        let context = scip_execution_context(ScipExecutionContextInput {
            project_root: &project_root,
            lang,
            analysis_root: &project_root,
            source_files: std::slice::from_ref(&source),
            mode: ProviderExecutionMode::SourceOnlyFallback,
            explicit_context_files: Vec::new(),
            lineage_context_files: Vec::new(),
            generated_context_parts: vec![b"source-only-config".to_vec()],
            execution_dimensions: Vec::new(),
        })
        .expect("build source-only execution context");

        assert_eq!(context.mode, ProviderExecutionMode::SourceOnlyFallback);
        assert!(context.config_artifacts.is_empty());
        assert!(context.generated_context_digest.is_some());
        let _ = fs::remove_dir_all(project_root);
    }

}
