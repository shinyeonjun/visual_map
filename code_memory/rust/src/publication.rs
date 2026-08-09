/// Inputs owned by the canonical publication stage after provider execution
/// has stabilized.
struct CanonicalPublicationContext {
    root: PathBuf,
    pack_root: PathBuf,
    providers_root: Option<PathBuf>,
    source_census: static_pipeline::source_census::SourceCensus,
    static_plan: codebase_fact_model::analysis_plan::AnalysisPlan,
    analyses: Vec<ProviderUnitBatch>,
    discovered_files: Vec<(String, PathBuf)>,
    file_relations: Vec<FileRelationOutput>,
    project_model_files: Vec<String>,
    diagnostics: Vec<Diagnostic>,
    provider_work: PathBuf,
    source_snapshot: SourceSnapshot,
    project_config_digest: u64,
    cache_impact_state: CacheImpact,
    dependency_context_digest: u64,
    active_cache_files: HashSet<PathBuf>,
    timings: Vec<StageTiming>,
}

fn publish_canonical_fact_bundle(input: CanonicalPublicationContext) -> Result<(), String> {
    let CanonicalPublicationContext {
        root,
        pack_root,
        providers_root,
        source_census,
        static_plan,
        analyses,
        discovered_files,
        file_relations,
        project_model_files,
        mut diagnostics,
        provider_work,
        mut source_snapshot,
        project_config_digest,
        cache_impact_state,
        dependency_context_digest,
        mut active_cache_files,
        mut timings,
    } = input;
    let language_ir_started = Instant::now();
    // Scheduler-owned provider batches are the sole Language IR authority.
    // The in-process provider snapshot supports deterministic post-language
    // analyzers only and is never published as a parallel index.
    let execution_context_started = Instant::now();
    let provider_execution_context_receipt =
        static_pipeline::language_ir::reconcile_provider_execution_contexts(
            &analyses,
            &static_plan,
        )?;
    timings.push(StageTiming {
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
            file_relations: &file_relations,
            project_model_files: &project_model_files,
            coordinator_diagnostics: &diagnostics,
            static_analyzer_set_digest,
            artifact_root: &language_ir_artifact_root,
        },
    )?;
    timings.push(StageTiming {
        stage: "direct_language_ir_stream_emission",
        elapsed_ms: direct_language_ir_started.elapsed().as_millis(),
    });

    let direct_language_ir_receipt = direct_language_ir.receipt;
    let direct_language_ir_diagnostics = direct_language_ir.diagnostics;
    let language_ir_artifact = direct_language_ir.artifact;
    let provider_snapshot_started = Instant::now();
    let provider_snapshot = direct_language_ir.provider_snapshot;
    let provider_languages = provider_snapshot.languages;
    let provider_documents = provider_snapshot.documents;
    diagnostics.extend(provider_snapshot.diagnostics);
    timings.push(StageTiming {
        stage: "provider_snapshot_commit",
        elapsed_ms: provider_snapshot_started.elapsed().as_millis(),
    });

    let framework_started = Instant::now();
    emit_progress("frameworks", 70, 100, "API와 handler 경계 분석 중");
    let framework_key = framework_cache_key(
        &root,
        &pack_root,
        &provider_documents,
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
                &provider_documents,
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
    timings.push(StageTiming {
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
    )?;
    eprintln!(
        "@codebase-workspace-test-ir {}",
        serde_json::to_string(&test_ir.receipt)
            .map_err(|error| format!("cannot serialize Test IR receipt: {error}"))?
    );
    timings.push(StageTiming {
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
    timings.push(StageTiming {
        stage: "canonical_normalizer_linker",
        elapsed_ms: canonical_linker_started.elapsed().as_millis(),
    });

    eprintln!(
        "timing stage=framework_analysis elapsed_ms={} frameworks={} framework_relations={} diagnostics={}",
        framework_started.elapsed().as_millis(),
        framework_analysis.frameworks.len(),
        framework_analysis.relations.len(),
        diagnostics.len()
    );
    eprintln!(
        "@codebase-workspace-language-ir {}",
        serde_json::to_string(&direct_language_ir_receipt)
            .map_err(|error| format!("cannot serialize Language IR migration receipt: {error}"))?
    );
    if env::var("CODE_MEMORY_LANGUAGE_IR_DIAGNOSTICS").as_deref() == Ok("1") {
        eprintln!(
            "@codebase-workspace-language-ir-diagnostics {}",
            serde_json::to_string(&direct_language_ir_diagnostics).map_err(|error| format!(
                "cannot serialize Language IR diagnostic receipt: {error}"
            ))?
        );
    }
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
    timings.push(StageTiming {
        stage: "language_ir_adapter_validation",
        elapsed_ms: language_ir_started.elapsed().as_millis(),
    });

    if env::var("CODE_MEMORY_STRICT").as_deref() == Ok("1") {
        enforce_quality_gate(&provider_languages, &diagnostics)?;
    }
    let refresh_dependency_manifest =
        cache_impact_state.force_all || !cache_impact_state.affected_paths.is_empty();
    if refresh_dependency_manifest {
        let reverse_imports = static_pipeline::dependency_manifest::collect_reverse_imports(
            &language_ir_artifact.path,
            &static_plan,
        )?;
        if let Err(error) = write_source_manifest(
            &root,
            &source_snapshot,
            &reverse_imports,
            dependency_context_digest,
        ) {
            eprintln!("source manifest cache unavailable: {error}");
        }
    }
    for timing in timings {
        eprintln!(
            "timing stage={} elapsed_ms={}",
            timing.stage, timing.elapsed_ms
        );
    }
    if let Err(error) = commit_cache_generation(&root, active_cache_files) {
        eprintln!("cache generation cleanup unavailable: {error}");
    }
    emit_progress("index-complete", 80, 100, "canonical 코드 사실 그래프 완료");
    Ok(())
}
