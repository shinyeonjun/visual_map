/// Runs one immutable, canonical code analysis from census to publication.
///
/// Discovery/planning, provider execution, and publication are deliberately
/// separate stages. No legacy JSON index is produced beside the Fact bundle.
pub(crate) fn index_project(
    root: &Path,
    pack_root: &Path,
    providers_root: Option<&Path>,
    source_manifest_path: Option<&Path>,
    expected_source_manifest: Option<&codebase_fact_model::identity::Sha256Digest>,
    cache_policy: AnalysisCachePolicy,
) -> Result<(), String> {
    let root = crate::source::canonical_project_root(root)?;
    let pack_root = pack_root
        .canonicalize()
        .unwrap_or_else(|_| pack_root.to_path_buf());
    let providers_root = providers_root.map(Path::to_path_buf);
    let provider_work_root = project_cache_root(&root).join("provider-work");
    cleanup_stale_provider_work(&provider_work_root, Duration::from_secs(24 * 60 * 60));
    let provider_work = provider_work_root.join(std::process::id().to_string());
    fs::create_dir_all(&provider_work)
        .map_err(|error| format!("cannot create {}: {error}", provider_work.display()))?;
    let _provider_work_guard = ProviderWorkGuard(provider_work.clone());

    emit_progress("discovery", 3, 100, "소스 파일 찾는 중");
    let discovery_started = Instant::now();
    let file_walk_started = Instant::now();
    let source_census = match (source_manifest_path, expected_source_manifest) {
        (Some(path), Some(digest)) => {
            static_pipeline::source_census::SourceCensus::load_verified_manifest(
                &root, path, digest,
            )?
        }
        (None, None) => static_pipeline::source_census::SourceCensus::scan(&root)?,
        _ => return Err("incomplete preflight source manifest receipt".to_string()),
    };
    let all_source_files = source_census.included_language_files();
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
    let file_walk_elapsed = file_walk_started.elapsed();

    emit_progress("manifest", 10, 100, "파일 목록과 변경 범위 계산 중");
    let plan_started = Instant::now();
    let static_plan = static_pipeline::analysis_unit_planner::plan_analysis_units(
        &root,
        &source_census.manifest,
    )?;
    let plan_elapsed = plan_started.elapsed();
    let included_count = source_census
        .manifest
        .files
        .iter()
        .filter(|file| {
            file.state == codebase_fact_model::source_manifest::SourceEntryState::Included
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

    let project_config_digest = project_config_digest(&root);
    let dependency_context_digest =
        source_dependency_context_digest(providers_root.as_deref(), project_config_digest);
    let source_hash_started = Instant::now();
    let source_snapshot = source_census.source_snapshot_metadata();
    let source_hash_elapsed = source_hash_started.elapsed();
    let cache_invalidation_started = Instant::now();
    let cache_impact_state = if cache_policy.reuses_results() {
        cache_impact(&root, &source_snapshot, dependency_context_digest)
    } else {
        CacheImpact::fresh(&source_snapshot)
    };
    let cache_invalidation_elapsed = cache_invalidation_started.elapsed();
    if cache_policy == AnalysisCachePolicy::Fresh {
        eprintln!("analysis cache policy: fresh; bypassing all prior analysis results");
    } else if cache_impact_state.force_all {
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

    let language_inventory = inventory_language_files(&all_source_files);
    let provider_provenance = LANGUAGES
        .iter()
        .copied()
        .map(|lang| provider_provenance(lang, providers_root.as_deref()))
        .collect::<Vec<_>>();
    enforce_managed_provider_policy(&provider_provenance, &language_inventory.discovered)?;

    emit_progress("project-model", 16, 100, "워크스페이스와 패키지 경계 분석 중");
    let project_model = prepare_typescript_project_model(
        &root,
        &provider_work,
        providers_root.as_deref(),
        project_config_digest,
        &source_snapshot,
        &all_source_files,
        cache_policy,
    )?;
    let mut active_cache_files = HashSet::new();
    if let Some(cache_file) = project_model.cache_file.clone() {
        active_cache_files.insert(cache_file);
    }
    let mut timings = vec![
        StageTiming {
            stage: "file_walk",
            elapsed_ms: file_walk_elapsed.as_millis(),
        },
        StageTiming {
            stage: "analysis_unit_planning",
            elapsed_ms: plan_elapsed.as_millis(),
        },
        StageTiming {
            stage: "source_hashing",
            elapsed_ms: source_hash_elapsed.as_millis(),
        },
        StageTiming {
            stage: "cache_invalidation",
            elapsed_ms: cache_invalidation_elapsed.as_millis(),
        },
        project_model.timing,
    ];

    emit_progress("planning", 24, 100, "언어별 분석 단위 계획 중");
    let planning = plan_provider_execution(ProviderPlanningInput {
        root: &root,
        provider_work: &provider_work,
        providers_root: providers_root.clone(),
        manifest: &source_census.manifest,
        static_plan: &static_plan,
        language_files: &language_inventory.by_language,
        source_snapshot: &source_snapshot,
        cache_impact: &cache_impact_state,
        project_config_digest,
        max_project_source_file_bytes,
        typescript_units: project_model.units,
        typescript_call_ranges: project_model.call_ranges,
        active_cache_files,
        cache_policy,
    })?;
    timings.extend(planning.timings);
    timings.push(StageTiming {
        stage: "file_discovery_and_cache_lookup",
        elapsed_ms: discovery_started.elapsed().as_millis(),
    });

    let provider_started = Instant::now();
    emit_progress("providers", 35, 100, "언어 공급자 실행 중");
    let mut analyses = planning.cached_analyses;
    for result in run_provider_jobs(planning.jobs, |completed, total| {
        let label = if completed == total {
            "언어별 분석 결과 병합 중".to_string()
        } else {
            format!("언어 공급자 실행 중 · {completed}/{total} 작업 완료")
        };
        emit_progress(
            "providers",
            35 + completed.saturating_mul(35) / total.max(1),
            100,
            &label,
        );
    })? {
        analyses.extend(result.batches);
    }
    let provider_elapsed = provider_started.elapsed();
    eprintln!(
        "timing stage=provider_and_scip_conversion elapsed_ms={} batches={}",
        provider_elapsed.as_millis(),
        analyses.len()
    );
    timings.push(StageTiming {
        stage: "provider_and_scip_conversion",
        elapsed_ms: provider_elapsed.as_millis(),
    });

    let stability_started = Instant::now();
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
    timings.push(StageTiming {
        stage: "source_stability_verification",
        elapsed_ms: stability_started.elapsed().as_millis(),
    });

    publish_canonical_fact_bundle(CanonicalPublicationContext {
        root,
        pack_root,
        providers_root,
        source_census,
        static_plan,
        analyses,
        discovered_files: language_inventory.discovered,
        file_relations: project_model.file_relations,
        project_model_files: project_model.modeled_files,
        diagnostics: project_model.diagnostics,
        provider_work,
        source_snapshot,
        project_config_digest,
        cache_impact_state,
        dependency_context_digest,
        active_cache_files: planning.active_cache_files,
        timings,
        cache_policy,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceLanguageReceipt {
    schema: &'static str,
    manifest_digest: codebase_fact_model::identity::Sha256Digest,
    languages: Vec<String>,
}

pub(crate) fn detect_source_languages(
    root: &Path,
    manifest_out: Option<&Path>,
) -> Result<(), String> {
    let root = crate::source::canonical_project_root(root)?;
    emit_progress("provider-selection", 0, 1, "필요한 언어 분석 도구 확인 중");
    let census = static_pipeline::source_census::SourceCensus::scan(&root)?;
    if let Some(path) = manifest_out {
        let path = resolve_output_path(path.to_path_buf())?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "cannot create source manifest directory {}: {error}",
                    parent.display()
                )
            })?;
        }
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| {
                format!("cannot create source manifest {}: {error}", path.display())
            })?;
        write_json(&mut output, &census.manifest)
            .map_err(|error| format!("cannot encode source manifest: {error}"))?;
        output.sync_all().map_err(|error| {
            format!("cannot sync source manifest {}: {error}", path.display())
        })?;
    }
    let mut languages = census
        .manifest
        .files
        .iter()
        .filter(|file| {
            file.state == codebase_fact_model::source_manifest::SourceEntryState::Included
        })
        .flat_map(|file| file.languages.iter().map(|language| language.as_str().to_string()))
        .collect::<Vec<_>>();
    languages.sort();
    languages.dedup();
    eprintln!(
        "@codebase-workspace-source-languages {}",
        serde_json::to_string(&SourceLanguageReceipt {
            schema: "codebase-workspace.source-languages.v1",
            manifest_digest: census.manifest.manifest_digest,
            languages,
        })
        .map_err(|error| format!("cannot encode source language receipt: {error}"))?
    );
    emit_progress("provider-selection", 1, 1, "필요한 언어 분석 도구 확인 완료");
    Ok(())
}
