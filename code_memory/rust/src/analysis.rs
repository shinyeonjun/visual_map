fn enforce_quality_gate(
    languages: &[LanguageOutput],
    diagnostics: &[Diagnostic],
) -> Result<(), String> {
    let failures: Vec<String> = languages
        .iter()
        .filter(|language| {
            language.files_missing > 0
                || !matches!(language.status, "indexed" | "indexed-partial" | "excluded")
                || ((language.files_found > 0)
                    && language.files_indexed == 0
                    && language.files_excluded == 0)
        })
        .map(|language| format!("{}={}", language.id, language.status))
        .collect();
    let errors = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.level == "error")
        .count();
    if failures.is_empty() && errors == 0 {
        return Ok(());
    }
    Err(format!(
        "strict quality gate failed: languages=[{}], error_diagnostics={errors}",
        failures.join(", ")
    ))
}
fn prepare_typescript_units(
    root: &Path,
    scratch: &Path,
    units: &mut [project_model::ProjectModelUnit],
) -> Result<(), String> {
    let directory = scratch.join("tsjs-configs");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("cannot create {}: {error}", directory.display()))?;
    for (index, unit) in units.iter_mut().enumerate() {
        if !unit.synthetic {
            continue;
        }
        let path = directory.join(format!("unit-{index}.json"));
        let mut config = serde_json::Map::new();
        if let Some(base) = &unit.base_config {
            let base_path = root.join(base);
            if base_path.is_file() {
                config.insert(
                    "extends".to_string(),
                    Value::String(base_path.to_string_lossy().into_owned()),
                );
            }
        }
        config.insert(
            "compilerOptions".to_string(),
            serde_json::json!({
                "allowJs": unit.allow_js,
                "checkJs": false,
                "noEmit": true
            }),
        );
        config.insert(
            "files".to_string(),
            Value::Array(
                unit.files
                    .iter()
                    .map(|file| Value::String(root.join(file).to_string_lossy().into_owned()))
                    .collect(),
            ),
        );
        let bytes = serde_json::to_vec(&Value::Object(config))
            .map_err(|error| format!("cannot serialize TypeScript analysis config: {error}"))?;
        fs::write(&path, bytes)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
        unit.generated_config = Some(path);
    }
    Ok(())
}

fn analyze_provider_job(job: ProviderJob) -> Vec<ProviderUnitBatch> {
    if job.members.len() == 1 {
        let member = &job.members[0];
        let mut analysis = analyze_language(member);
        analysis.project_excluded_files = member.project_excluded_files;
        rebase_provider_batch(&mut analysis, &member.root, &member.project_root);
        return vec![analysis];
    }

    let primary = &job.members[0];
    let files = combined_job_files(&job.members);
    let scip_path = primary
        .work
        .join(format!("{}.scip", job.key.replace(':', "_")));
    let _ = fs::remove_file(&scip_path);
    let is_clangd_job = job.key.starts_with("provider:clangd:");
    let is_batched_typescript_job = job
        .key
        .starts_with("provider:scip-typescript:configured:");
    let provider_execution_root = if is_batched_typescript_job {
        &primary.project_root
    } else {
        &primary.root
    };
    if is_clangd_job && !has_compile_context_for_files(&primary.root, &files) {
        return job
            .members
            .iter()
            .map(|member| {
                language_excluded(
                    member.lang,
                    "native-lsp",
                    &member.files,
                    DiagnosticCode::MissingCompileContext,
                    "C/C++ semantic analysis skipped because no usable compile context was found; structural map remains available",
                )
            })
            .collect();
    }
    let result = if is_clangd_job {
        run_native_lsp_with_server(
            &primary.lang,
            "clangd",
            ProviderRoots::new(&primary.project_root, &primary.root),
            &scip_path,
            primary.providers_root.as_deref(),
            &files,
        )
    } else if is_batched_typescript_job {
        let provider_configs = job
            .members
            .iter()
            .filter_map(|member| member.provider_config.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        run_scip_indexer_with_configs(
            &primary.lang,
            ProviderRoots::new(&primary.project_root, provider_execution_root),
            &scip_path,
            primary.providers_root.as_deref(),
            &provider_configs,
            &files,
            primary.max_project_source_file_bytes,
        )
    } else {
        run_scip_indexer(
            &primary.lang,
            ProviderRoots::new(&primary.project_root, &primary.root),
            &scip_path,
            primary.providers_root.as_deref(),
            primary.provider_config.as_deref(),
            &files,
            primary.max_project_source_file_bytes,
        )
    };

    let provider_outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => {
            if is_batched_typescript_job {
                // One malformed project must not downgrade unrelated projects.
                // Retry the established exact per-config path; each member can
                // then succeed or fail independently with its own receipt.
                return job
                    .members
                    .iter()
                    .map(|member| {
                        let mut analysis = analyze_language(member);
                        analysis.project_excluded_files = member.project_excluded_files;
                        rebase_provider_batch(
                            &mut analysis,
                            &member.root,
                            &member.project_root,
                        );
                        analysis
                    })
                    .collect();
            }
            return job
                .members
                .iter()
                .map(|member| {
                    language_failure(
                        member.lang,
                        if is_clangd_job { "native-lsp" } else { "scip" },
                        &member.files,
                        error.clone(),
                    )
                })
                .collect();
        }
    };
    let provider_diagnostics = provider_outcome.diagnostics;
    let execution_context = provider_outcome.execution_context;

    let parsed = read_scip(
        &scip_path,
        primary.lang.id,
        if is_clangd_job {
            codebase_fact_model::analysis::ProviderProtocol::LanguageServerProtocol
        } else {
            codebase_fact_model::analysis::ProviderProtocol::Scip
        },
        provider_execution_root,
        &allowed_document_paths(provider_execution_root, &files),
        Some(&primary.call_ranges),
    );
    let _ = fs::remove_file(&scip_path);
    let (all_documents, all_relations) = match parsed {
        Ok(value) => value,
        Err(error) => {
            return job
                .members
                .iter()
                .map(|member| {
                    language_invalid_output(
                        member.lang,
                        if is_clangd_job { "native-lsp" } else { "scip" },
                        &member.files,
                        error.clone(),
                    )
                })
                .collect();
        }
    };

    let mut documents_by_path: HashMap<String, Vec<DocumentOutput>> = HashMap::new();
    for document in all_documents {
        documents_by_path
            .entry(document.path.clone())
            .or_default()
            .push(document);
    }
    let mut relations_by_path: HashMap<String, Vec<RelationOutput>> = HashMap::new();
    for relation in all_relations {
        relations_by_path
            .entry(relation.path.clone())
            .or_default()
            .push(relation);
    }

    job.members
        .iter()
        .map(|member| {
            let member_execution_context = if is_batched_typescript_job {
                let Some(config) = member.provider_config.as_deref() else {
                    return language_failure(
                        member.lang,
                        "scip",
                        &member.files,
                        "batched TypeScript project omitted its planned config".to_string(),
                    );
                };
                match configured_scip_execution_context(
                    &member.lang,
                    ProviderRoots::new(&member.project_root, &member.root),
                    config,
                    &member.files,
                ) {
                    Ok(context) => context,
                    Err(error) => {
                        return language_failure(
                            member.lang,
                            "scip",
                            &member.files,
                            format!("cannot reconstruct batched provider context: {error}"),
                        )
                    }
                }
            } else {
                execution_context.clone()
            };
            let member_payload_root = if is_batched_typescript_job {
                &member.project_root
            } else {
                &member.root
            };
            let allowed = allowed_document_paths(member_payload_root, &member.files);
            let mut documents: Vec<DocumentOutput> = allowed
                .iter()
                .filter_map(|path| documents_by_path.get(path))
                .flatten()
                .cloned()
                .collect();
            for document in &mut documents {
                document.language = member.lang.id.to_string();
            }
            let relations = allowed
                .iter()
                .filter_map(|path| relations_by_path.get(path))
                .flatten()
                .cloned()
                .collect();
            let provider = if is_clangd_job { "native-lsp" } else { "scip" };
            let provider_stopped = provider == "native-lsp"
                && documents.is_empty()
                && provider_diagnostics.iter().any(|diagnostic| {
                    matches!(
                        diagnostic.code,
                        DiagnosticCode::ProviderTimeout | DiagnosticCode::ProviderStopped
                    )
                });
            let mut analysis = if provider_stopped {
                language_excluded(
                    member.lang,
                    provider,
                    &member.files,
                    DiagnosticCode::ProviderStopped,
                    &format!(
                        "{} semantic provider stopped; structural map remains available",
                        member.lang.name
                    ),
                )
            } else {
                language_analysis_from_index(
                    member.lang,
                    provider,
                    member_payload_root,
                    &member.files,
                    documents,
                    relations,
                )
            };
            let provider_partial = provider_diagnostics
                .iter()
                .any(provider_diagnostic_is_partial);
            if provider_partial && analysis.language.status == "indexed" {
                analysis.language.status = "indexed-partial";
            }
            analysis.execution_context = member_execution_context;
            let member_paths: HashSet<String> = member
                .files
                .iter()
                .filter_map(|file| file.strip_prefix(provider_execution_root).ok())
                .map(|path| path.to_string_lossy().replace('\\', "/"))
                .collect();
            analysis.diagnostics.extend(
                provider_diagnostics
                    .iter()
                    .filter(|diagnostic| {
                        diagnostic
                            .path
                            .as_ref()
                            .is_none_or(|path| member_paths.contains(path))
                    })
                    .map(|diagnostic| {
                        let mut diagnostic = diagnostic.clone();
                        diagnostic.language = member.lang.id.to_string();
                        diagnostic
                    }),
            );
            if is_batched_typescript_job && member.root != member.project_root {
                // The shared SCIP process emits project-root-relative paths.
                // Member caches retain their established module-local shape so
                // cache hits rebase exactly as before the batching change.
                rebase_provider_batch(&mut analysis, &member.project_root, &member.root);
            }
            write_language_cache(
                &member.root,
                member.lang,
                &member.cache_key,
                LanguageCacheWriteInput {
                    documents: &analysis.documents,
                    relations: &analysis.relations,
                    diagnostics: &analysis.diagnostics,
                    execution_context: &analysis.execution_context,
                    cache_policy: member.cache_policy,
                },
            );
            rebase_provider_batch(&mut analysis, &member.root, &member.project_root);
            analysis.project_excluded_files = member.project_excluded_files;
            analysis
        })
        .collect()
}
