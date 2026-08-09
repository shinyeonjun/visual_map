pub(crate) fn index_project(
    root: &Path,
    out: &Path,
    architecture_out: &Path,
    pack_root: &Path,
    providers_root: Option<&Path>,
) -> Result<(), String> {
    let root = crate::source::canonical_project_root(root)?;
    emit_progress("discovery", 3, 100, "소스 파일 찾는 중");
    let pack_root = pack_root
        .canonicalize()
        .unwrap_or_else(|_| pack_root.to_path_buf());
    let mut output = IndexOutput {
        schema: "code-memory.language-index.v2",
        project_root: root.to_string_lossy().into_owned(),
        provider_provenance: LANGUAGES
            .iter()
            .copied()
            .map(|lang| provider_provenance(lang, providers_root))
            .collect(),
        languages: Vec::new(),
        coverage: Vec::new(),
        documents: Vec::new(),
        relations: Vec::new(),
        file_relations: Vec::new(),
        project_model_files: Vec::new(),
        frameworks: Vec::new(),
        framework_relations: Vec::new(),
        diagnostics: Vec::new(),
        timings: Vec::new(),
        analysis_units: Vec::new(),
    };

    let work = out.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(work).map_err(|e| format!("cannot create {}: {e}", work.display()))?;
    // ponytail: provider scratch files belong in a project/process cache, not beside the final output.
    // This prevents parallel projects from overwriting the same language.scip file.
    let provider_work_root = project_cache_root(&root).join("provider-work");
    cleanup_stale_provider_work(&provider_work_root, Duration::from_secs(24 * 60 * 60));
    let provider_work = provider_work_root.join(std::process::id().to_string());
    fs::create_dir_all(&provider_work)
        .map_err(|e| format!("cannot create {}: {e}", provider_work.display()))?;
    let _provider_work_guard = ProviderWorkGuard(provider_work.clone());

    let providers_root = providers_root.map(Path::to_path_buf);
    let project_config_digest = project_config_digest(&root);
    let dependency_context_digest =
        source_dependency_context_digest(providers_root.as_deref(), project_config_digest);
    let discovery_started = Instant::now();
    let mut jobs = Vec::new();
    let mut discovered_files = Vec::new();
    let mut cached_analyses = Vec::new();
    let mut unit_runs = HashMap::new();
    let mut language_files = HashMap::new();
    let mut active_cache_files = HashSet::new();
    let mut writable_workspaces: HashMap<&'static str, Arc<ProviderWorkspace>> = HashMap::new();
    let mut writable_workspace_turns: HashMap<&'static str, u64> = HashMap::new();
    let file_walk_started = Instant::now();
    let source_census = static_pipeline::source_census::SourceCensus::scan(&root)?;
    let max_project_source_file_bytes = source_census
        .manifest
        .files
        .iter()
        .filter(|file| {
            file.state == codebase_fact_model::source_manifest::SourceEntryState::Included
                && !file.languages.is_empty()
        })
        .map(|file| file.byte_size)
        .max()
        .unwrap_or(1);
    let all_source_files = source_census.included_language_files();
    emit_progress("manifest", 10, 100, "파일 목록과 변경 범위 계산 중");
    let file_walk_elapsed = file_walk_started.elapsed();
    let static_plan_started = Instant::now();
    let static_plan = static_pipeline::analysis_unit_planner::plan_analysis_units(
        &root,
        &source_census.manifest,
    )?;
    let static_plan_elapsed = static_plan_started.elapsed();
    let included_count = source_census
        .manifest
        .files
        .iter()
        .filter(|file| {
            file.state
                == codebase_fact_model::source_manifest::SourceEntryState::Included
        })
        .count();
    eprintln!(
        "@codebase-workspace-source-manifest {}",
        serde_json::json!({
            "schema": source_census.manifest.schema.as_str(),
            "manifestDigest": source_census.manifest.manifest_digest,
            "fileCount": source_census.manifest.files.len(),
            "includedFileCount": included_count,
            "nonEnumeratedScopeCount": source_census.manifest.scopes.len(),
            "analysisPlanDigest": static_plan.plan_digest,
            "analysisUnitCount": static_plan.units.len(),
        })
    );
    let source_hash_started = Instant::now();
    let mut source_snapshot = source_census.source_snapshot_metadata();
    let source_hash_elapsed = source_hash_started.elapsed();
    let cache_invalidation_started = Instant::now();
    let cache_impact_state = cache_impact(
        &root,
        &source_snapshot,
        dependency_context_digest,
    );
    let cache_invalidation_elapsed = cache_invalidation_started.elapsed();
    if cache_impact_state.force_all {
        eprintln!("language cache invalidation: no previous source manifest");
    } else if !cache_impact_state.affected_paths.is_empty() {
        eprintln!(
            "language cache invalidation: {} affected source files",
            cache_impact_state.affected_paths.len()
        );
    }
    if let Err(error) = write_provider_run_input_manifest(
        &root,
        &source_snapshot,
        dependency_context_digest,
    ) {
        eprintln!("provider-run input cache unavailable: {error}");
    }
    for lang in LANGUAGES {
        let files: Vec<PathBuf> = all_source_files
            .iter()
            .filter(|path| {
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .map(|extension| extension.to_ascii_lowercase())
                    .is_some_and(|extension| lang.extensions.contains(&extension.as_str()))
            })
            .cloned()
            .collect();
        if files.is_empty() {
            continue;
        }
        language_files.insert(lang.id, files.clone());
        discovered_files.extend(
            files
                .iter()
                .cloned()
                .map(|path| (lang.id.to_string(), path)),
        );
    }

    discovered_files.extend(
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
    enforce_managed_provider_policy(&output.provider_provenance, &discovered_files)?;

    let project_model_started = Instant::now();
    emit_progress("project-model", 16, 100, "워크스페이스와 패키지 경계 분석 중");
    let mut typescript_units = Vec::new();
    let mut typescript_call_ranges = Arc::new(HashMap::new());
    if all_source_files.iter().any(|path| {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "ts" | "tsx" | "mts" | "cts" | "js" | "jsx" | "mjs" | "cjs" | "vue"
                )
            })
    }) {
        let tsjs_files: Vec<PathBuf> = all_source_files
            .iter()
            .filter(|path| {
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| {
                        matches!(
                            extension.to_ascii_lowercase().as_str(),
                            "ts" | "tsx" | "mts" | "cts" | "js" | "jsx" | "mjs" | "cjs" | "vue"
                        )
                    })
            })
            .cloned()
            .collect();
        let project_model_key = typescript_project_model_cache_key(
            &root,
            &tsjs_files,
            providers_root.as_deref(),
            project_config_digest,
            &source_snapshot,
        );
        active_cache_files.insert(
            project_cache_root(&root).join(format!("tsjs-project-model-{project_model_key}.json")),
        );
        match project_model::analyze_typescript_project(
            &root,
            providers_root.as_deref(),
            &project_model_key,
        ) {
            Ok(model) => {
                output.file_relations = model.relations;
                output.project_model_files = model.modeled_files;
                output.diagnostics.extend(model.diagnostics);
                typescript_units = model.units;
                typescript_call_ranges = Arc::new(model.call_ranges);
                prepare_typescript_units(&root, &provider_work, &mut typescript_units)?;
            }
            Err(error) => output.diagnostics.push(Diagnostic {
                language: "typescript".to_string(),
                level: "warning",
                code: DiagnosticCode::ProviderFailed,
                message: format!("TypeScript project model unavailable: {error}"),
                detail: None,
                path: None,
                line: None,
            }),
        }
    }
    output.timings.push(StageTiming {
        stage: "typescript_project_model",
        elapsed_ms: project_model_started.elapsed().as_millis(),
    });
    emit_progress("planning", 24, 100, "언어별 분석 단위 계획 중");

    let cache_lookup_started = Instant::now();
    let mut cache_key_elapsed = Duration::ZERO;
    let mut cache_io_elapsed = Duration::ZERO;
    let mut cache_deserialize_elapsed = Duration::ZERO;
    let provider_schedule =
        static_pipeline::provider_schedule::schedule_provider_units(
            &root,
            &static_plan,
            &typescript_units,
        )?;
    eprintln!(
        "@codebase-workspace-provider-schedule {}",
        serde_json::to_string(&provider_schedule.receipt)
            .map_err(|error| format!("cannot serialize provider schedule receipt: {error}"))?
    );
    for scheduled in provider_schedule.units {
        let lang = scheduled.lang;
        let analysis_unit_id = scheduled.analysis_unit_id;
        let execution_scope_id = scheduled.execution_scope_id;
        let unit_language_file_count = language_files
            .get(lang.id)
            .map_or(scheduled.files.len(), Vec::len);
        let provider = if matches!(lang.id, "c" | "cpp")
                && find_tool(lang.tool, providers_root.as_deref()).is_none()
            {
                "native-lsp"
            } else {
                match lang.provider {
                    ProviderKind::Scip => "scip",
                    ProviderKind::Lsp => "native-lsp",
                }
        };
        if lang.id == "rust" && unit_language_file_count > rust_semantic_file_limit() {
                record_analysis_unit_run(
                    &mut unit_runs,
                    lang.id,
                    &analysis_unit_id,
                    AnalysisUnitRun {
                        provider,
                        execution: "skipped",
                        elapsed_ms: 0,
                    },
                );
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
                assign_provider_batch_scope(&mut batch, &root, &scheduled.files);
                cached_analyses.push(batch);
                continue;
        }
        if !provider_ready(&lang, providers_root.as_deref()) {
                record_analysis_unit_run(
                    &mut unit_runs,
                    lang.id,
                    &analysis_unit_id,
                    AnalysisUnitRun {
                        provider,
                        execution: "unavailable",
                        elapsed_ms: 0,
                    },
                );
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
                assign_provider_batch_scope(&mut batch, &root, &scheduled.files);
                cached_analyses.push(batch);
                continue;
        }

        let cache_key_started = Instant::now();
        let cache_key = language_cache_key(LanguageCacheKeyInput {
            root: &scheduled.root,
            lang: &lang,
            files: &scheduled.files,
            providers_root: providers_root.as_deref(),
            config_digest: project_config_digest,
            source_snapshot: &source_snapshot,
            execution_scope_id: &execution_scope_id,
            provider_config: scheduled.provider_config.as_deref(),
        });
        active_cache_files.insert(language_cache_path(&scheduled.root, &lang, &cache_key));
        cache_key_elapsed += cache_key_started.elapsed();
        let unit_affected = cache_impact_state.force_all
            || scheduled.files.iter().any(|file| {
                    let relative = file
                        .strip_prefix(&root)
                        .unwrap_or(file)
                        .to_string_lossy()
                        .replace('\\', "/");
                    cache_impact_state.affected_paths.contains(&relative)
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
                record_analysis_unit_run(
                    &mut unit_runs,
                    lang.id,
                    &analysis_unit_id,
                    AnalysisUnitRun {
                        provider,
                        execution: "cache",
                        elapsed_ms: cache_read.io_ms + cache_read.deserialize_ms,
                    },
                );
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
                rebase_provider_batch(&mut analysis, &scheduled.root, &root);
                assign_provider_batch_scope(&mut analysis, &root, &scheduled.files);
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
                    &root,
                    provider_work
                        .join("writable-workspaces")
                        .join(lang.id),
                    &source_census.manifest,
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
            project_root: root.clone(),
            files: scheduled.files,
            cache_key,
            root: scheduled.root,
            work: provider_work.clone(),
            providers_root: providers_root.clone(),
            analysis_unit_id,
            execution_scope_id,
            provider_config: scheduled.provider_config,
            project_excluded_files: scheduled.project_excluded_files,
            max_project_source_file_bytes,
            writable_workspace,
            call_ranges: if matches!(lang.id, "typescript" | "javascript") {
                typescript_call_ranges.clone()
            } else {
                Arc::new(HashMap::new())
            },
        });
    }
    output.timings.push(StageTiming {
        stage: "file_walk",
        elapsed_ms: file_walk_elapsed.as_millis(),
    });
    output.timings.push(StageTiming {
        stage: "analysis_unit_planning",
        elapsed_ms: static_plan_elapsed.as_millis(),
    });
    output.timings.push(StageTiming {
        stage: "source_hashing",
        elapsed_ms: source_hash_elapsed.as_millis(),
    });
    output.timings.push(StageTiming {
        stage: "cache_invalidation",
        elapsed_ms: cache_invalidation_elapsed.as_millis(),
    });
    output.timings.push(StageTiming {
        stage: "cache_key_hashing",
        elapsed_ms: cache_key_elapsed.as_millis(),
    });
    output.timings.push(StageTiming {
        stage: "cache_io",
        elapsed_ms: cache_io_elapsed.as_millis(),
    });
    output.timings.push(StageTiming {
        stage: "cache_json_deserialize",
        elapsed_ms: cache_deserialize_elapsed.as_millis(),
    });
    output.timings.push(StageTiming {
        stage: "cache_lookup_and_planning",
        elapsed_ms: cache_lookup_started.elapsed().as_millis(),
    });
    output.timings.push(StageTiming {
        stage: "file_discovery_and_cache_lookup",
        elapsed_ms: discovery_started.elapsed().as_millis(),
    });
    let jobs = merge_provider_jobs(jobs);

    let provider_started = Instant::now();
    emit_progress("providers", 35, 100, "언어 공급자 실행 중");
    let mut analyses = cached_analyses;
    for result in run_provider_jobs(jobs, |completed, total| {
        emit_progress(
            "providers",
            35 + completed.saturating_mul(35) / total.max(1),
            100,
            if completed == total {
                "언어별 의미 분석 병합 중"
            } else {
                "언어 공급자 실행 중"
            },
        );
    })? {
        for unit in result.units {
            record_analysis_unit_run(
                &mut unit_runs,
                &unit.language,
                &unit.id,
                AnalysisUnitRun {
                    provider: unit.provider,
                    execution: "provider",
                    elapsed_ms: result.elapsed_ms,
                },
            );
        }
        analyses.extend(result.batches);
    }
    let provider_elapsed = provider_started.elapsed();
    eprintln!(
        "timing stage=provider_and_scip_conversion elapsed_ms={} batches={}",
        provider_elapsed.as_millis(),
        analyses.len()
    );
    output.timings.push(StageTiming {
        stage: "provider_and_scip_conversion",
        elapsed_ms: provider_elapsed.as_millis(),
    });

    let source_stability_started = Instant::now();
    let final_source_census = static_pipeline::source_census::SourceCensus::scan(&root)?;
    if final_source_census.manifest.manifest_digest != source_census.manifest.manifest_digest {
        return Err(format!(
            "selected repository changed during semantic provider execution; refusing to publish a mixed snapshot (before={}, after={})",
            source_census.manifest.manifest_digest,
            final_source_census.manifest.manifest_digest
        ));
    }
    eprintln!(
        "@codebase-workspace-source-stability {}",
        serde_json::json!({
            "schema": "codebase-workspace.source-stability.v1",
            "manifestDigest": source_census.manifest.manifest_digest,
            "unchanged": true,
        })
    );
    output.timings.push(StageTiming {
        stage: "source_stability_verification",
        elapsed_ms: source_stability_started.elapsed().as_millis(),
    });

    let language_ir_started = Instant::now();
    // Scheduler-owned provider batches are the sole Language IR authority.
    // The compatibility projection is returned from the same merge and is
    // never converted back into a second IR stream.
    let execution_context_started = Instant::now();
    let provider_execution_context_receipt =
        static_pipeline::language_ir::reconcile_provider_execution_contexts(
            &analyses,
            &static_plan,
        )?;
    output.timings.push(StageTiming {
        stage: "provider_execution_context_reconciliation",
        elapsed_ms: execution_context_started.elapsed().as_millis(),
    });

    let direct_language_ir_started = Instant::now();
    let language_ir_artifact_root = provider_work.join("language-ir");
    let framework_analyzer_set_digest =
        static_pipeline::framework_ir::framework_analyzer_set_digest(&pack_root)?;
    let static_analyzer_set_digest = static_pipeline::test_ir::combine_static_analyzer_digests(
        framework_analyzer_set_digest,
        static_pipeline::test_ir::test_analyzer_digest(),
    );
    let direct_language_ir = static_pipeline::language_ir::emit_direct_language_ir(
        static_pipeline::language_ir::DirectLanguageIrInput {
            project_root: &root,
            manifest: &source_census.manifest,
            plan: &static_plan,
            providers_root: providers_root.as_deref(),
            batches: analyses,
            discovered_files: &discovered_files,
            file_relations: &output.file_relations,
            project_model_files: &output.project_model_files,
            coordinator_diagnostics: &output.diagnostics,
            static_analyzer_set_digest,
            artifact_root: &language_ir_artifact_root,
        },
    )?;
    output.timings.push(StageTiming {
        stage: "direct_language_ir_stream_emission",
        elapsed_ms: direct_language_ir_started.elapsed().as_millis(),
    });

    let direct_language_ir_receipt = direct_language_ir.receipt;
    let language_ir_artifact = direct_language_ir.artifact;
    let compatibility_projection_started = Instant::now();
    let compatibility_projection = direct_language_ir.compatibility_projection;
    output.languages = compatibility_projection.languages;
    output.coverage = compatibility_projection.coverage;
    output.documents = compatibility_projection.documents;
    output.relations = compatibility_projection.relations;
    output
        .diagnostics
        .extend(compatibility_projection.diagnostics);
    output.analysis_units = build_analysis_units(&static_plan, &output.coverage, &unit_runs);
    output.timings.push(StageTiming {
        stage: "compatibility_projection_commit",
        elapsed_ms: compatibility_projection_started.elapsed().as_millis(),
    });

    let framework_started = Instant::now();
    emit_progress("frameworks", 70, 100, "API와 handler 경계 분석 중");
    let framework_key = framework_cache_key(
        &root,
        &pack_root,
        &output.documents,
        &source_snapshot,
        project_config_digest,
    );
    let framework_cache =
        project_cache_root(&root).join(format!("framework-{framework_key}.json.gz"));
    active_cache_files.insert(framework_cache.clone());
    let framework_analysis = match load_framework_cache(&framework_cache) {
        Some(analysis) => {
            eprintln!("cached framework analysis");
            analysis
        }
        None => {
            load_source_contents(&root, &mut source_snapshot);
            let analysis = frameworks::analyze_with_sources(
                &root,
                &output.documents,
                &pack_root,
                &source_snapshot,
            )?;
            let _ = write_framework_cache(&framework_cache, &analysis);
            analysis
        }
    };
    let framework_ir = static_pipeline::framework_ir::adapt_framework_routes(
        &root,
        &source_census.manifest,
        &static_plan,
        &language_ir_artifact.snapshot_id,
        &framework_analysis,
    )?;
    eprintln!(
        "@codebase-workspace-framework-ir {}",
        serde_json::to_string(&framework_ir.receipt)
            .map_err(|error| format!("cannot serialize Framework IR receipt: {error}"))?
    );
    output.timings.push(StageTiming {
        stage: "framework_route_ir",
        elapsed_ms: framework_started.elapsed().as_millis(),
    });

    let test_ir_started = Instant::now();
    emit_progress("test-relations", 71, 100, "테스트와 실제 코드 관계 분석 중");
    let test_ir = static_pipeline::test_ir::adapt_test_relations(
        &root,
        &source_census.manifest,
        &static_plan,
        &language_ir_artifact.snapshot_id,
        &language_ir_artifact.path,
        &output.relations,
    )?;
    eprintln!(
        "@codebase-workspace-test-ir {}",
        serde_json::to_string(&test_ir.receipt)
            .map_err(|error| format!("cannot serialize Test IR receipt: {error}"))?
    );
    output.timings.push(StageTiming {
        stage: "test_relation_ir",
        elapsed_ms: test_ir_started.elapsed().as_millis(),
    });

    let canonical_linker_started = Instant::now();
    emit_progress("canonical-linker", 72, 100, "정확한 코드 사실 그래프 조립 중");
    let canonical_bundle_root = project_cache_root(&root).join("canonical-bundles");
    let repository_display_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("Repository");
    let canonical_language = static_pipeline::canonical::normalize_language_ir(
        static_pipeline::canonical::CanonicalLanguageInput {
            project_root: &root,
            repository_display_name,
            manifest: &source_census.manifest,
            plan: &static_plan,
            ir_path: &language_ir_artifact.path,
            ir_snapshot_id: &language_ir_artifact.snapshot_id,
            ir_content_digest: language_ir_artifact.content_digest,
            ir_record_count: language_ir_artifact.record_count,
            provider_set_digest: direct_language_ir_receipt.provider_set_digest,
            execution_context_set_digest: direct_language_ir_receipt
                .execution_context_set_digest,
            framework_ir: Some(&framework_ir),
            test_ir: Some(&test_ir),
            output_root: &canonical_bundle_root,
        },
    )?;
    eprintln!(
        "@codebase-workspace-canonical-linker {}",
        serde_json::to_string(&canonical_language.receipt)
            .map_err(|error| format!("cannot serialize canonical linker receipt: {error}"))?
    );
    eprintln!(
        "@codebase-workspace-canonical-fact-manifest {}",
        serde_json::to_string(&canonical_language.manifest)
            .map_err(|error| format!("cannot serialize canonical Fact manifest: {error}"))?
    );
    eprintln!(
        "@codebase-workspace-canonical-fact-bundle {}",
        serde_json::to_string(&canonical_language.artifact)
            .map_err(|error| format!("cannot serialize canonical Fact artifact: {error}"))?
    );
    output.timings.push(StageTiming {
        stage: "canonical_normalizer_linker",
        elapsed_ms: canonical_linker_started.elapsed().as_millis(),
    });

    output.frameworks = framework_analysis.frameworks;
    output.framework_relations = framework_analysis.relations;
    eprintln!(
        "timing stage=framework_analysis elapsed_ms={} frameworks={} framework_relations={} diagnostics={}",
        framework_started.elapsed().as_millis(),
        output.frameworks.len(),
        output.framework_relations.len(),
        output.diagnostics.len()
    );
    eprintln!(
        "@codebase-workspace-language-ir {}",
        serde_json::to_string(&direct_language_ir_receipt)
            .map_err(|error| format!("cannot serialize Language IR migration receipt: {error}"))?
    );
    eprintln!(
        "@codebase-workspace-language-ir-stream-authority {}",
        serde_json::to_string(&language_ir_artifact).map_err(|error| format!(
            "cannot serialize Language IR stream authority receipt: {error}"
        ))?
    );
    eprintln!(
        "@codebase-workspace-provider-execution-context {}",
        serde_json::to_string(&provider_execution_context_receipt).map_err(|error| format!(
            "cannot serialize provider execution context receipt: {error}"
        ))?
    );
    output.timings.push(StageTiming {
        stage: "language_ir_adapter_validation",
        elapsed_ms: language_ir_started.elapsed().as_millis(),
    });

    canonicalize_index_output(&mut output);

    emit_progress("architecture", 78, 100, "계층과 호출 구조 통합 중");
    let capture_reverse_imports =
        cache_impact_state.force_all || !cache_impact_state.affected_paths.is_empty();
    let index_write = write_index_outputs(
        &root,
        out,
        architecture_out,
        &pack_root,
        &output,
        &mut source_snapshot,
        project_config_digest,
        capture_reverse_imports,
    )?;
    active_cache_files.insert(index_write.architecture_cache);
    if env::var("CODE_MEMORY_STRICT").as_deref() == Ok("1") {
        enforce_quality_gate(&output)?;
    }
    if let Some(reverse_imports) = index_write.reverse_imports {
        if let Err(error) = write_source_manifest(
            &root,
            &source_snapshot,
            &reverse_imports,
            dependency_context_digest,
        ) {
            eprintln!("source manifest cache unavailable: {error}");
        }
    }
    if let Err(error) = commit_cache_generation(&root, active_cache_files) {
        eprintln!("cache generation cleanup unavailable: {error}");
    }
    emit_progress("index-complete", 80, 100, "코드 인덱스 완료");
    Ok(())
}
