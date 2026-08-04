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
    let discovery_started = Instant::now();
    let mut jobs = Vec::new();
    let mut discovered_files = Vec::new();
    let mut cached_analyses = Vec::new();
    let mut planned_units = Vec::new();
    let mut unit_runs = HashMap::new();
    let mut language_files = HashMap::new();
    let mut active_cache_files = HashSet::new();
    let mut all_extensions: HashSet<&str> = LANGUAGES
        .iter()
        .flat_map(|language| language.extensions.iter().copied())
        .collect();
    // Vue SFCs are structural TypeScript/JavaScript sources. They are read by
    // the project model, not sent to SCIP as a standalone language document.
    all_extensions.insert("vue");
    let file_walk_started = Instant::now();
    let all_source_files =
        collect_files(&root, &all_extensions.iter().copied().collect::<Vec<_>>());
    emit_progress("manifest", 10, 100, "파일 목록과 변경 범위 계산 중");
    let file_walk_elapsed = file_walk_started.elapsed();
    let source_hash_started = Instant::now();
    let mut source_snapshot = load_source_snapshot_metadata_from_files(&root, &all_source_files);
    let source_hash_elapsed = source_hash_started.elapsed();
    let cache_invalidation_started = Instant::now();
    let cache_impact_state = cache_impact(&root, out, architecture_out, &source_snapshot);
    let cache_invalidation_elapsed = cache_invalidation_started.elapsed();
    if cache_impact_state.force_all {
        eprintln!("language cache invalidation: no previous source manifest");
    } else if !cache_impact_state.affected_paths.is_empty() {
        eprintln!(
            "language cache invalidation: {} affected source files",
            cache_impact_state.affected_paths.len()
        );
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
    for lang in LANGUAGES {
        let Some(files) = language_files.get(lang.id) else {
            continue;
        };
        let modules =
            if matches!(lang.id, "typescript" | "javascript") && !typescript_units.is_empty() {
                plan_typescript_modules(&root, *lang, files, &typescript_units)
            } else {
                plan_language_modules(&root, *lang, files)
            };
        for mut module in modules {
            let has_cpp_translation_unit =
                module.files.iter().any(|file| {
                    matches!(
                        file.extension()
                            .and_then(|extension| extension.to_str())
                            .map(|extension| extension.to_ascii_lowercase())
                            .as_deref(),
                        Some("cc" | "cp" | "cpp" | "cxx")
                    )
                }) || compile_database_files_for_scope(&module.root, &module.files).is_some_and(
                    |files| {
                        files.iter().any(|file| {
                            matches!(
                                file.extension()
                                    .and_then(|extension| extension.to_str())
                                    .map(|extension| extension.to_ascii_lowercase())
                                    .as_deref(),
                                Some("cc" | "cp" | "cpp" | "cxx")
                            )
                        })
                    },
                );
            // A shared `.h` belongs to the C++ context when the module also
            // contains C++ translation units. In a pure C module C owns the
            // headers; in a mixed module C++ owns them.
            if lang.id == "c" && has_cpp_translation_unit {
                module
                    .files
                    .retain(|file| !is_cpp_header(file) && !is_cpp_header_fragment(file));
                if module.files.is_empty() {
                    continue;
                }
            } else if lang.id == "cpp" && !has_cpp_translation_unit {
                module
                    .files
                    .retain(|file| !is_cpp_header(file) && !is_cpp_header_fragment(file));
                if module.files.is_empty() {
                    continue;
                }
            }
            if !module.files.is_empty() {
                planned_units.push((
                    module.id.clone(),
                    lang.id.to_string(),
                    module.root.clone(),
                    module.files.clone(),
                    module.project_excluded_files,
                ));
            }
            let unit_key = (lang.id.to_string(), module.id.clone());
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
            if lang.id == "rust" && files.len() > rust_semantic_file_limit() {
                unit_runs.insert(
                    unit_key,
                    AnalysisUnitRun {
                        provider,
                        execution: "skipped",
                        elapsed_ms: 0,
                    },
                );
                cached_analyses.push(language_excluded(
                    *lang,
                    "native-lsp",
                    &module.files,
                    DiagnosticCode::LargeWorkspacePartial,
                    &format!(
                        "Rust semantic analysis deferred for this {}-file workspace (limit {}); structural map remains available",
                        files.len(),
                        rust_semantic_file_limit()
                    ),
                ));
                continue;
            }
            if !provider_ready(lang, providers_root.as_deref()) {
                unit_runs.insert(
                    unit_key,
                    AnalysisUnitRun {
                        provider,
                        execution: "unavailable",
                        elapsed_ms: 0,
                    },
                );
                cached_analyses.push(LanguageAnalysis {
                    language: LanguageOutput {
                        id: lang.id.to_string(),
                        name: lang.name.to_string(),
                        provider,
                        files_found: module.files.len(),
                        files_indexed: 0,
                        files_excluded: 0,
                        files_missing: module.files.len(),
                        status: "missing-tool",
                    },
                    documents: Vec::new(),
                    relations: Vec::new(),
                    diagnostics: vec![Diagnostic {
                        language: lang.id.to_string(),
                        level: "error",
                        code: DiagnosticCode::ProviderMissing,
                        message: missing_tool_message(lang),
                        detail: None,
                        path: None,
                        line: None,
                    }],
                    project_excluded_files: 0,
                });
                continue;
            }

            let cache_key_started = Instant::now();
            let cache_key = language_cache_key(
                &module.root,
                lang,
                &module.files,
                providers_root.as_deref(),
                project_config_digest,
                &source_snapshot,
            );
            active_cache_files.insert(language_cache_path(&module.root, lang, &cache_key));
            cache_key_elapsed += cache_key_started.elapsed();
            let module_affected = cache_impact_state.force_all
                || module.files.iter().any(|file| {
                    let relative = file
                        .strip_prefix(&root)
                        .unwrap_or(file)
                        .to_string_lossy()
                        .replace('\\', "/");
                    cache_impact_state.affected_paths.contains(&relative)
                });
            if module_affected {
                println!(
                    "invalidated {} module={} (affected source range)",
                    lang.name, module.id
                );
            } else {
                let cache_read = load_language_cache(&module.root, lang, &cache_key);
                cache_io_elapsed += Duration::from_millis(cache_read.io_ms as u64);
                cache_deserialize_elapsed +=
                    Duration::from_millis(cache_read.deserialize_ms as u64);
                if let Some(cached) = cache_read.value {
                unit_runs.insert(
                    unit_key.clone(),
                    AnalysisUnitRun {
                        provider,
                        execution: "cache",
                        elapsed_ms: cache_read.io_ms + cache_read.deserialize_ms,
                    },
                );
                let coverage = language_document_coverage(
                    &module.root,
                    *lang,
                    &module.files,
                    &cached.documents,
                );
                let (status, mut diagnostics) = classify_language_documents(
                    &module.root,
                    lang,
                    &module.files,
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
                let mut analysis = LanguageAnalysis {
                    language: LanguageOutput {
                        id: lang.id.to_string(),
                        name: lang.name.to_string(),
                        provider,
                        files_found: module.files.len(),
                        files_indexed: coverage.indexed,
                        files_excluded: coverage.excluded,
                        files_missing: coverage.missing,
                        status,
                    },
                    documents: cached.documents,
                    relations: cached.relations,
                    diagnostics,
                    project_excluded_files: module.project_excluded_files,
                };
                rebase_language_analysis(&mut analysis, &module.root, &root);
                cached_analyses.push(analysis);
                println!(
                    "cached {} module={} ({} files)",
                    lang.name,
                    module.id,
                    module.files.len()
                );
                continue;
                }
            }

            jobs.push(LanguageJob {
                lang: *lang,
                project_root: root.clone(),
                files: module.files,
                cache_key,
                root: module.root,
                work: provider_work.clone(),
                providers_root: providers_root.clone(),
                module_id: module.id,
                provider_config: module.provider_config,
                project_excluded_files: module.project_excluded_files,
                project_config_digest,
                call_ranges: if matches!(lang.id, "typescript" | "javascript") {
                    typescript_call_ranges.clone()
                } else {
                    Arc::new(HashMap::new())
                },
            });
        }
    }
    output.timings.push(StageTiming {
        stage: "file_walk",
        elapsed_ms: file_walk_elapsed.as_millis(),
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
            unit_runs.insert(
                (unit.language, unit.id),
                AnalysisUnitRun {
                    provider: unit.provider,
                    execution: "provider",
                    elapsed_ms: result.elapsed_ms,
                },
            );
        }
        analyses.extend(result.analyses);
    }
    let (languages, documents, relations, diagnostics) = merge_language_analyses(analyses);
    output.languages = languages;
    output.documents = documents;
    output.relations = relations;
    output.diagnostics.extend(diagnostics);
    output.coverage = build_file_coverage(
        &root,
        &discovered_files,
        &output.documents,
        &output.languages,
        &output.project_model_files,
    );
    output.analysis_units =
        build_analysis_units(&root, &planned_units, &output.coverage, &unit_runs);
    eprintln!(
        "timing stage=provider_merge elapsed_ms={} documents={} relations={} diagnostics={}",
        provider_started.elapsed().as_millis(),
        output.documents.len(),
        output.relations.len(),
        output.diagnostics.len()
    );
    output.timings.push(StageTiming {
        stage: "provider_and_scip_conversion",
        elapsed_ms: provider_started.elapsed().as_millis(),
    });

    let framework_started = Instant::now();
    emit_progress("frameworks", 74, 100, "프레임워크 의미 분석 중");
    let framework_key = framework_cache_key(
        &root,
        &pack_root,
        &output.documents,
        &source_snapshot,
        project_config_digest,
    );
    let framework_cache = project_cache_root(&root).join(format!("framework-{framework_key}.json"));
    active_cache_files.insert(framework_cache.clone());
    match load_framework_cache(&framework_cache) {
        Some(analysis) => {
            output.frameworks = analysis.frameworks;
            output.framework_relations = analysis.relations;
            eprintln!("cached framework analysis");
        }
        None => {
            load_source_contents(&root, &mut source_snapshot);
            match frameworks::analyze_with_sources(
                &root,
                &output.documents,
                &pack_root,
                &source_snapshot,
            ) {
                Ok(analysis) => {
                    let _ = write_framework_cache(&framework_cache, &analysis);
                    output.frameworks = analysis.frameworks;
                    output.framework_relations = analysis.relations;
                }
                Err(error) => output.diagnostics.push(Diagnostic {
                    language: "framework".to_string(),
                    level: "error",
                    code: DiagnosticCode::Internal,
                    message: error,
                    detail: None,
                    path: None,
                    line: None,
                }),
            }
        }
    }
    output.timings.push(StageTiming {
        stage: "framework_analysis",
        elapsed_ms: framework_started.elapsed().as_millis(),
    });
    eprintln!(
        "timing stage=framework_analysis elapsed_ms={} frameworks={} framework_relations={} diagnostics={}",
        framework_started.elapsed().as_millis(),
        output.frameworks.len(),
        output.framework_relations.len(),
        output.diagnostics.len()
    );
    canonicalize_index_output(&mut output);

    emit_progress("architecture", 78, 100, "계층과 호출 구조 통합 중");
    let architecture_cache = write_index_outputs(
        &root,
        out,
        architecture_out,
        &pack_root,
        &output,
        &mut source_snapshot,
        project_config_digest,
    )?;
    active_cache_files.insert(architecture_cache);
    if env::var("CODE_MEMORY_STRICT").as_deref() == Ok("1") {
        enforce_quality_gate(&output)?;
    }
    if let Err(error) = write_source_manifest(&root, &source_snapshot) {
        eprintln!("source manifest cache unavailable: {error}");
    }
    if let Err(error) = commit_cache_generation(&root, active_cache_files) {
        eprintln!("cache generation cleanup unavailable: {error}");
    }
    emit_progress("index-complete", 80, 100, "코드 인덱스 완료");
    Ok(())
}
