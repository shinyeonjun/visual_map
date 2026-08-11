// Discovery-to-provider planning without executing providers or publishing facts.
//
// Keeping this stage separate makes the top-level index flow read like the
// product pipeline and prevents cache, writable-workspace, and language-
// specific scheduling policy from leaking into canonical publication.

struct ProjectModelPreparation {
    file_relations: Vec<FileRelationOutput>,
    modeled_files: Vec<String>,
    units: Vec<project_model::ProjectModelUnit>,
    call_ranges: Arc<HashMap<String, Vec<Vec<i32>>>>,
    diagnostics: Vec<Diagnostic>,
    cache_file: Option<PathBuf>,
    timing: StageTiming,
}

fn prepare_typescript_project_model(
    root: &Path,
    provider_work: &Path,
    providers_root: Option<&Path>,
    project_config_digest: u64,
    source_snapshot: &SourceSnapshot,
    all_source_files: &[PathBuf],
    cache_policy: AnalysisCachePolicy,
) -> Result<ProjectModelPreparation, String> {
    let started = Instant::now();
    let tsjs_files = all_source_files
        .iter()
        .filter(|path| is_typescript_or_javascript_source(path))
        .cloned()
        .collect::<Vec<_>>();
    if tsjs_files.is_empty() {
        return Ok(ProjectModelPreparation {
            file_relations: Vec::new(),
            modeled_files: Vec::new(),
            units: Vec::new(),
            call_ranges: Arc::new(HashMap::new()),
            diagnostics: Vec::new(),
            cache_file: None,
            timing: StageTiming {
                stage: "typescript_project_model",
                elapsed_ms: started.elapsed().as_millis(),
            },
        });
    }

    let cache_key = typescript_project_model_cache_key(
        root,
        &tsjs_files,
        providers_root,
        project_config_digest,
        source_snapshot,
    );
    let cache_file = project_cache_root(root).join(format!("tsjs-project-model-{cache_key}.json"));
    let mut result = match project_model::analyze_typescript_project(
        root,
        providers_root,
        &cache_key,
        cache_policy.reuses_results(),
    ) {
        Ok(model) => ProjectModelPreparation {
            file_relations: model.relations,
            modeled_files: model.modeled_files,
            units: model.units,
            call_ranges: Arc::new(model.call_ranges),
            diagnostics: model.diagnostics,
            cache_file: Some(cache_file),
            timing: StageTiming {
                stage: "typescript_project_model",
                elapsed_ms: 0,
            },
        },
        Err(error) => ProjectModelPreparation {
            file_relations: Vec::new(),
            modeled_files: Vec::new(),
            units: Vec::new(),
            call_ranges: Arc::new(HashMap::new()),
            diagnostics: vec![Diagnostic {
                language: "typescript".to_string(),
                level: "warning",
                code: DiagnosticCode::ProviderFailed,
                message: format!("TypeScript project model unavailable: {error}"),
                detail: None,
                path: None,
                line: None,
            }],
            cache_file: Some(cache_file),
            timing: StageTiming {
                stage: "typescript_project_model",
                elapsed_ms: 0,
            },
        },
    };
    prepare_typescript_units(root, provider_work, &mut result.units)?;
    result.timing.elapsed_ms = started.elapsed().as_millis();
    Ok(result)
}

fn is_typescript_or_javascript_source(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "ts" | "tsx" | "mts" | "cts" | "js" | "jsx" | "mjs" | "cjs" | "vue"
            )
        })
}

struct LanguageFileInventory {
    by_language: HashMap<&'static str, Vec<PathBuf>>,
    discovered: Vec<(String, PathBuf)>,
}

fn inventory_language_files(all_source_files: &[PathBuf]) -> LanguageFileInventory {
    let mut by_language = HashMap::new();
    let mut discovered = Vec::new();
    for lang in LANGUAGES {
        let files = all_source_files
            .iter()
            .filter(|path| {
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .map(|extension| extension.to_ascii_lowercase())
                    .is_some_and(|extension| lang.extensions.contains(&extension.as_str()))
            })
            .cloned()
            .collect::<Vec<_>>();
        if files.is_empty() {
            continue;
        }
        discovered.extend(
            files
                .iter()
                .cloned()
                .map(|path| (lang.id.to_string(), path)),
        );
        by_language.insert(lang.id, files);
    }
    discovered.extend(
        all_source_files
            .iter()
            .filter(|path| {
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("vue"))
            })
            .cloned()
            .map(|path| ("typescript".to_string(), path)),
    );
    LanguageFileInventory {
        by_language,
        discovered,
    }
}

struct ProviderPlanningInput<'a> {
    root: &'a Path,
    provider_work: &'a Path,
    providers_root: Option<PathBuf>,
    manifest: &'a codebase_fact_model::source_manifest::SourceManifest,
    static_plan: &'a codebase_fact_model::analysis_plan::AnalysisPlan,
    language_files: &'a HashMap<&'static str, Vec<PathBuf>>,
    source_snapshot: &'a SourceSnapshot,
    cache_impact: &'a CacheImpact,
    project_config_digest: u64,
    max_project_source_file_bytes: u64,
    typescript_units: Vec<project_model::ProjectModelUnit>,
    typescript_call_ranges: Arc<HashMap<String, Vec<Vec<i32>>>>,
    active_cache_files: HashSet<PathBuf>,
    cache_policy: AnalysisCachePolicy,
}

struct ProviderPlanningOutput {
    jobs: Vec<ProviderJob>,
    cached_analyses: Vec<ProviderUnitBatch>,
    active_cache_files: HashSet<PathBuf>,
    timings: Vec<StageTiming>,
}

fn plan_provider_execution(input: ProviderPlanningInput<'_>) -> Result<ProviderPlanningOutput, String> {
    let started = Instant::now();
    let mut cache_key_elapsed = Duration::ZERO;
    let mut cache_io_elapsed = Duration::ZERO;
    let mut cache_deserialize_elapsed = Duration::ZERO;
    let mut jobs = Vec::new();
    let mut cached_analyses = Vec::new();
    let mut active_cache_files = input.active_cache_files;
    let mut writable_workspaces: HashMap<&'static str, Arc<ProviderWorkspace>> = HashMap::new();
    let mut writable_workspace_turns: HashMap<&'static str, u64> = HashMap::new();
    let schedule = static_pipeline::provider_schedule::schedule_provider_units(
        input.root,
        input.static_plan,
        &input.typescript_units,
    )?;
    eprintln!(
        "@codebase-workspace-provider-schedule {}",
        serde_json::to_string(&schedule.receipt)
            .map_err(|error| format!("cannot serialize provider schedule receipt: {error}"))?
    );

    for scheduled in schedule.units {
        let lang = scheduled.lang;
        let analysis_unit_id = scheduled.analysis_unit_id;
        let execution_scope_id = scheduled.execution_scope_id;
        let unit_language_file_count = input
            .language_files
            .get(lang.id)
            .map_or(scheduled.files.len(), Vec::len);
        let provider = if matches!(lang.id, "c" | "cpp")
            && find_tool(lang.tool, input.providers_root.as_deref()).is_none()
        {
            "native-lsp"
        } else {
            match lang.provider {
                ProviderKind::Scip => "scip",
                ProviderKind::Lsp => "native-lsp",
            }
        };
        if lang.id == "rust" && unit_language_file_count > rust_semantic_file_limit() {
            let mut batch = language_excluded(
                lang,
                "native-lsp",
                &scheduled.files,
                DiagnosticCode::LargeWorkspacePartial,
                &format!(
                    "Rust semantic analysis deferred for this {}-file workspace (limit {}); structural map remains available",
                    unit_language_file_count,
                    rust_semantic_file_limit()
                ),
            );
            batch.project_excluded_files = scheduled.project_excluded_files;
            assign_provider_batch_scope(&mut batch, input.root, &scheduled.files);
            cached_analyses.push(batch);
            continue;
        }
        if !provider_ready(&lang, input.providers_root.as_deref()) {
            let mut batch = ProviderUnitBatch {
                language: LanguageOutput {
                    id: lang.id.to_string(),
                    name: lang.name.to_string(),
                    provider,
                    files_found: scheduled.files.len(),
                    files_indexed: 0,
                    files_excluded: 0,
                    files_missing: scheduled.files.len(),
                    status: "missing-tool",
                },
                source_files: Vec::new(),
                execution_context: not_executed_provider_context(&lang),
                documents: Vec::new(),
                relations: Vec::new(),
                diagnostics: vec![Diagnostic {
                    language: lang.id.to_string(),
                    level: "error",
                    code: DiagnosticCode::ProviderMissing,
                    message: missing_tool_message(&lang),
                    detail: None,
                    path: None,
                    line: None,
                }],
                project_excluded_files: scheduled.project_excluded_files,
            };
            assign_provider_batch_scope(&mut batch, input.root, &scheduled.files);
            cached_analyses.push(batch);
            continue;
        }

        let cache_key_started = Instant::now();
        let cache_key = language_cache_key(LanguageCacheKeyInput {
            root: &scheduled.root,
            lang: &lang,
            files: &scheduled.files,
            providers_root: input.providers_root.as_deref(),
            config_digest: input.project_config_digest,
            source_snapshot: input.source_snapshot,
            execution_scope_id: &execution_scope_id,
            provider_config: scheduled.provider_config.as_deref(),
        });
        active_cache_files.insert(language_cache_path(&scheduled.root, &lang, &cache_key));
        cache_key_elapsed += cache_key_started.elapsed();
        let unit_affected = input.cache_impact.force_all
            || scheduled.files.iter().any(|file| {
                let relative = file
                    .strip_prefix(input.root)
                    .unwrap_or(file)
                    .to_string_lossy()
                    .replace('\\', "/");
                input.cache_impact.affected_paths.contains(&relative)
            });
        if unit_affected {
            println!(
                "invalidated {} unit={} scope={} (affected source range)",
                lang.name, analysis_unit_id, execution_scope_id
            );
        } else {
            let cache_read = load_language_cache(&scheduled.root, &lang, &cache_key);
            cache_io_elapsed += Duration::from_millis(cache_read.io_ms as u64);
            cache_deserialize_elapsed += Duration::from_millis(cache_read.deserialize_ms as u64);
            if let Some(cached) = cache_read.value {
                let coverage = language_document_coverage(
                    &scheduled.root,
                    lang,
                    &scheduled.files,
                    &cached.documents,
                );
                let (status, mut diagnostics) = classify_language_documents(
                    &scheduled.root,
                    &lang,
                    &scheduled.files,
                    &cached.documents,
                );
                let cached_partial = cached.diagnostics.iter().any(|diagnostic| {
                    matches!(
                        diagnostic.code,
                        DiagnosticCode::ProviderTimeout
                            | DiagnosticCode::LargeWorkspacePartial
                            | DiagnosticCode::JavaSourceFallback
                            | DiagnosticCode::TypescriptSourceFallback
                    )
                });
                let status = if cached_partial && status == "indexed" {
                    "indexed-partial"
                } else {
                    status
                };
                diagnostics.extend(cached.diagnostics.into_iter().map(|diagnostic| Diagnostic {
                    language: diagnostic.language,
                    level: match diagnostic.level.as_str() {
                        "error" => "error",
                        "info" => "info",
                        _ => "warning",
                    },
                    code: diagnostic.code,
                    message: diagnostic.message,
                    detail: diagnostic.detail,
                    path: diagnostic.path,
                    line: diagnostic.line,
                }));
                let mut analysis = ProviderUnitBatch {
                    language: LanguageOutput {
                        id: lang.id.to_string(),
                        name: lang.name.to_string(),
                        provider,
                        files_found: scheduled.files.len(),
                        files_indexed: coverage.indexed,
                        files_excluded: coverage.excluded,
                        files_missing: coverage.missing,
                        status,
                    },
                    source_files: Vec::new(),
                    execution_context: cached.execution_context,
                    documents: cached.documents,
                    relations: cached.relations,
                    diagnostics,
                    project_excluded_files: scheduled.project_excluded_files,
                };
                rebase_provider_batch(&mut analysis, &scheduled.root, input.root);
                assign_provider_batch_scope(&mut analysis, input.root, &scheduled.files);
                cached_analyses.push(analysis);
                println!(
                    "cached {} unit={} scope={} ({} files)",
                    lang.name,
                    analysis_unit_id,
                    execution_scope_id,
                    scheduled.files.len()
                );
                continue;
            }
        }

        let writable_workspace = if matches!(lang.id, "java" | "csharp") {
            let workspace = if let Some(workspace) = writable_workspaces.get(lang.id) {
                workspace.clone()
            } else {
                let workspace = Arc::new(ProviderWorkspace::from_manifest(
                    input.root,
                    input
                        .provider_work
                        .join("writable-workspaces")
                        .join(lang.id),
                    input.manifest,
                )?);
                writable_workspaces.insert(lang.id, workspace.clone());
                workspace
            };
            let ordinal = writable_workspace_turns.entry(lang.id).or_default();
            let binding = ProviderWorkspaceBinding::new(workspace, *ordinal);
            *ordinal += 1;
            Some(binding)
        } else {
            None
        };
        jobs.push(LanguageJob {
            lang,
            project_root: input.root.to_path_buf(),
            files: scheduled.files,
            cache_key,
            root: scheduled.root,
            work: input.provider_work.to_path_buf(),
            providers_root: input.providers_root.clone(),
            execution_scope_id,
            provider_config: scheduled.provider_config,
            project_excluded_files: scheduled.project_excluded_files,
            max_project_source_file_bytes: input.max_project_source_file_bytes,
            writable_workspace,
            cache_policy: input.cache_policy,
            call_ranges: if matches!(lang.id, "typescript" | "javascript") {
                input.typescript_call_ranges.clone()
            } else {
                Arc::new(HashMap::new())
            },
        });
    }

    Ok(ProviderPlanningOutput {
        jobs: merge_provider_jobs(jobs),
        cached_analyses,
        active_cache_files,
        timings: vec![
            StageTiming {
                stage: "cache_key_hashing",
                elapsed_ms: cache_key_elapsed.as_millis(),
            },
            StageTiming {
                stage: "cache_io",
                elapsed_ms: cache_io_elapsed.as_millis(),
            },
            StageTiming {
                stage: "cache_json_deserialize",
                elapsed_ms: cache_deserialize_elapsed.as_millis(),
            },
            StageTiming {
                stage: "cache_lookup_and_planning",
                elapsed_ms: started.elapsed().as_millis(),
            },
        ],
    })
}
