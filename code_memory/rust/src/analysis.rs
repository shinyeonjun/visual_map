/// Canonicalize every semantic collection at the serialization boundary.
/// Provider arrival order is allowed to vary; the public index is not.
fn canonicalize_index_output(output: &mut IndexOutput) {
    output
        .provider_provenance
        .sort_by(|left, right| left.language.cmp(&right.language));
    output
        .languages
        .sort_by(|left, right| left.id.cmp(&right.id));
    output.coverage.sort_by(|left, right| {
        (&left.language, &left.path, left.status, &left.reason).cmp(&(
            &right.language,
            &right.path,
            right.status,
            &right.reason,
        ))
    });
    for document in &mut output.documents {
        for symbol in &mut document.symbols {
            symbol.documentation.sort();
        }
        document.symbols.sort_by(|left, right| {
            (
                &left.symbol,
                &left.kind,
                &left.display_name,
                &left.documentation,
                &left.signature,
                &left.enclosing_symbol,
            )
                .cmp(&(
                    &right.symbol,
                    &right.kind,
                    &right.display_name,
                    &right.documentation,
                    &right.signature,
                    &right.enclosing_symbol,
                ))
        });
        document.occurrences.sort_by(|left, right| {
            (
                &left.range,
                &left.symbol,
                &left.enclosing_range,
                left.definition,
                left.import,
                left.read,
                left.write,
            )
                .cmp(&(
                    &right.range,
                    &right.symbol,
                    &right.enclosing_range,
                    right.definition,
                    right.import,
                    right.read,
                    right.write,
                ))
        });
    }
    output
        .documents
        .sort_by(|left, right| (&left.language, &left.path).cmp(&(&right.language, &right.path)));
    output.relations.sort_by(|left, right| {
        let order = (&left.from, &left.to, &left.kind, &left.path, &left.range).cmp(&(
            &right.from,
            &right.to,
            &right.kind,
            &right.path,
            &right.range,
        ));
        if order != Ordering::Equal {
            return order;
        }
        compare_optional_f64(left.confidence, right.confidence)
            .then_with(|| left.strategy.cmp(&right.strategy))
    });
    output.file_relations.sort_by(|left, right| {
        (
            &left.from,
            &left.to,
            &left.kind,
            &left.path,
            &left.range,
            &left.properties,
        )
            .cmp(&(
                &right.from,
                &right.to,
                &right.kind,
                &right.path,
                &right.range,
                &right.properties,
            ))
    });
    output.project_model_files.sort();
    output.project_model_files.dedup();
    output.frameworks.sort_by(|left, right| {
        (&left.id, &left.language, &left.name).cmp(&(&right.id, &right.language, &right.name))
    });
    for framework in &mut output.frameworks {
        framework.matched_signals.sort();
        framework.matched_signals.dedup();
        framework.files.sort();
        framework.files.dedup();
        for fact in &mut framework.facts {
            fact.evidence.sort();
        }
        framework.facts.sort_by(|left, right| {
            (
                &left.id,
                &left.kind,
                &left.source_file,
                left.source_line,
                left.source_end_line,
                &left.source_range,
                &left.symbol,
                &left.method,
                &left.path,
                &left.evidence,
                &left.properties,
            )
                .cmp(&(
                    &right.id,
                    &right.kind,
                    &right.source_file,
                    right.source_line,
                    right.source_end_line,
                    &right.source_range,
                    &right.symbol,
                    &right.method,
                    &right.path,
                    &right.evidence,
                    &right.properties,
                ))
        });
    }
    for relation in &mut output.framework_relations {
        relation.evidence.sort();
    }
    output.framework_relations.sort_by(|left, right| {
        (
            &left.from,
            &left.to,
            &left.kind,
            &left.framework,
            &left.path,
            &left.range,
            &left.evidence,
        )
            .cmp(&(
                &right.from,
                &right.to,
                &right.kind,
                &right.framework,
                &right.path,
                &right.range,
                &right.evidence,
            ))
    });
    output.diagnostics.sort_by(|left, right| {
        (
            &left.language,
            left.level,
            left.code.as_str(),
            &left.path,
            &left.line,
            &left.message,
        )
            .cmp(&(
                &right.language,
                right.level,
                right.code.as_str(),
                &right.path,
                &right.line,
                &right.message,
            ))
    });
    output
        .analysis_units
        .sort_by(|left, right| (&left.language, &left.id).cmp(&(&right.language, &right.id)));
}

fn compare_optional_f64(left: Option<f64>, right: Option<f64>) -> Ordering {
    left.map(f64::to_bits).cmp(&right.map(f64::to_bits))
}

#[derive(Clone, Copy)]
struct AnalysisUnitRun {
    provider: &'static str,
    execution: &'static str,
    elapsed_ms: u128,
}

fn build_analysis_units(
    project_root: &Path,
    plans: &[(String, String, PathBuf, Vec<PathBuf>, usize)],
    coverage: &[FileCoverageOutput],
    runs: &HashMap<(String, String), AnalysisUnitRun>,
) -> Vec<AnalysisUnitOutput> {
    let mut units = plans
        .iter()
        .map(|(id, language, root, files, project_excluded)| {
            let paths: HashSet<String> = files
                .iter()
                .map(|file| {
                    file.strip_prefix(project_root)
                        .unwrap_or(file)
                        .to_string_lossy()
                        .replace('\\', "/")
                })
                .collect();
            let entries: Vec<&FileCoverageOutput> = coverage
                .iter()
                .filter(|entry| entry.language == *language && paths.contains(&entry.path))
                .collect();
            let indexed = entries
                .iter()
                .filter(|entry| entry.status == "indexed")
                .count();
            let excluded = entries
                .iter()
                .filter(|entry| entry.status == "excluded")
                .count();
            let missing = entries
                .iter()
                .filter(|entry| entry.status == "missing")
                .count();
            let status = if missing == 0 && excluded == 0 && *project_excluded == 0 {
                "indexed"
            } else if indexed > 0 {
                "indexed-partial"
            } else if missing == 0 && excluded > 0 {
                "excluded"
            } else {
                "provider-failed"
            };
            let reason = entries
                .iter()
                .filter_map(|entry| entry.reason.clone())
                .next();
            let run = runs
                .get(&(language.clone(), id.clone()))
                .copied()
                .unwrap_or(AnalysisUnitRun {
                    provider: "unknown",
                    execution: "not-run",
                    elapsed_ms: 0,
                });
            AnalysisUnitOutput {
                id: id.clone(),
                language: language.clone(),
                root: root.to_string_lossy().into_owned(),
                files_found: files.len() + *project_excluded,
                files_indexed: indexed,
                files_excluded: excluded + *project_excluded,
                files_missing: missing,
                status,
                provider: run.provider,
                execution: run.execution,
                elapsed_ms: run.elapsed_ms,
                reason,
            }
        })
        .collect::<Vec<_>>();
    units.sort_by(|left, right| (&left.language, &left.id).cmp(&(&right.language, &right.id)));
    units
}

fn enforce_quality_gate(output: &IndexOutput) -> Result<(), String> {
    let failures: Vec<String> = output
        .languages
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
    let errors = output
        .diagnostics
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

fn analyze_provider_job(job: ProviderJob) -> Vec<LanguageAnalysis> {
    if job.members.len() == 1 {
        let member = &job.members[0];
        let mut analysis = analyze_language(member);
        analysis.project_excluded_files = member.project_excluded_files;
        rebase_language_analysis(&mut analysis, &member.root, &member.project_root);
        return vec![analysis];
    }

    let primary = &job.members[0];
    let files = combined_job_files(&job.members);
    let scip_path = primary
        .work
        .join(format!("{}.scip", job.key.replace(':', "_")));
    let _ = fs::remove_file(&scip_path);
    let is_clangd_job = job.key.starts_with("provider:clangd-c-cpp:");
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
            &primary.root,
            &scip_path,
            primary.providers_root.as_deref(),
            &files,
        )
    } else {
        run_scip_indexer(
            &primary.lang,
            &primary.root,
            &scip_path,
            primary.providers_root.as_deref(),
            primary.provider_config.as_deref(),
            &files,
            primary.project_config_digest,
        )
    };

    let provider_diagnostics = match result {
        Ok(diagnostics) => diagnostics,
        Err(error) => {
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

    let parsed = read_scip(
        &scip_path,
        primary.lang.id,
        &primary.root,
        &allowed_document_paths(&primary.root, &files),
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
            let allowed = allowed_document_paths(&member.root, &member.files);
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
                    &member.root,
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
            let member_paths: HashSet<String> = member
                .files
                .iter()
                .filter_map(|file| file.strip_prefix(&primary.root).ok())
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
            write_language_cache(
                &member.root,
                member.lang,
                &member.cache_key,
                &analysis.documents,
                &analysis.relations,
                &analysis.diagnostics,
            );
            rebase_language_analysis(&mut analysis, &member.root, &member.project_root);
            analysis.project_excluded_files = member.project_excluded_files;
            analysis
        })
        .collect()
}
