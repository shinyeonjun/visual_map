use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

mod architecture;
mod cache;
mod compat;
mod frameworks;
mod model;
mod module_plan;
mod project_model;
mod providers;
mod source;
pub(crate) use cache::*;
pub(crate) use model::*;
pub(crate) use module_plan::*;
pub(crate) use providers::*;
#[cfg(test)]
pub(crate) use source::load_source_snapshot;
#[cfg(test)]
pub(crate) use source::load_source_snapshot_from_files;
pub(crate) use source::{
    collect_files, is_excluded_source_dir, load_source_contents,
    load_source_snapshot_metadata_from_files,
};

#[cfg(test)]
mod tests;

fn main() {
    if let Err(error) = run() {
        eprintln!("code-memory-language: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("list") => list_languages(),
        Some("framework-packs") => {
            let rest: Vec<String> = args.collect();
            let root = optional_path(&rest, "--root")
                .unwrap_or(env::current_dir().map_err(|e| e.to_string())?);
            if rest.iter().any(|arg| arg == "--self-test") {
                frameworks::self_test(&root).map(|_| ())
            } else {
                validate_framework_packs(&root)
            }
        }
        Some("doctor") => {
            let rest: Vec<String> = args.collect();
            doctor(optional_path(&rest, "--providers-root").as_deref())
        }
        Some("index") => {
            let rest: Vec<String> = args.collect();
            let root = required_path(&rest, "--root")?;
            let pack_root = optional_path(&rest, "--packs-root")
                .unwrap_or(env::current_dir().map_err(|e| e.to_string())?);
            let providers_root = optional_path(&rest, "--providers-root");
            let out = optional_path(&rest, "--out")
                .unwrap_or_else(|| root.join(r".code_memory\language-index.json"));
            let out = if out.is_absolute() {
                out
            } else {
                env::current_dir()
                    .map_err(|e| format!("cannot resolve output path: {e}"))?
                    .join(out)
            };
            let architecture_out = optional_path(&rest, "--architecture-out")
                .map(|path| resolve_output_path(path))
                .transpose()?
                .unwrap_or_else(|| default_architecture_output(&out));
            index_project(
                &root,
                &out,
                &architecture_out,
                &pack_root,
                providers_root.as_deref(),
            )
        }
        Some("cli") => compat::run_cli(&args.collect::<Vec<_>>()),
        Some(command) => Err(format!(
            "unknown command '{command}'. Use list, doctor, or index."
        )),
        None => Err("missing command. Use list, doctor, or index.".to_string()),
    }
}

fn list_languages() -> Result<(), String> {
    for lang in LANGUAGES {
        let provider = match lang.provider {
            ProviderKind::Scip => "SCIP",
            ProviderKind::Lsp => "native LSP -> SCIP",
        };
        println!("{}\t{}\t{}\t{}", lang.id, lang.name, provider, lang.tool);
    }
    Ok(())
}

fn validate_framework_packs(root: &Path) -> Result<(), String> {
    let catalog_path = root.join("packs").join("framework").join("catalog.json");
    let catalog: Value = serde_json::from_slice(
        &fs::read(&catalog_path)
            .map_err(|e| format!("cannot read {}: {e}", catalog_path.display()))?,
    )
    .map_err(|e| format!("invalid framework catalog: {e}"))?;
    if catalog.get("schema").and_then(Value::as_str)
        != Some("code-memory.framework-pack-catalog.v1")
    {
        return Err("invalid framework catalog schema".to_string());
    }
    let adapter_path = root.join("packs").join("framework").join("adapters.json");
    let adapter_catalog: Value = serde_json::from_slice(
        &fs::read(&adapter_path)
            .map_err(|e| format!("cannot read {}: {e}", adapter_path.display()))?,
    )
    .map_err(|e| format!("invalid framework adapter catalog: {e}"))?;
    if adapter_catalog.get("schema").and_then(Value::as_str)
        != Some("code-memory.framework-adapter-catalog.v1")
    {
        return Err("invalid framework adapter catalog schema".to_string());
    }
    let adapters = adapter_catalog
        .get("adapters")
        .and_then(Value::as_object)
        .ok_or("framework adapter catalog has no adapters")?;
    let languages = catalog
        .get("languages")
        .and_then(Value::as_array)
        .ok_or("framework catalog has no languages")?;
    let mut seen = HashSet::new();
    let mut total = 0usize;
    for language in languages {
        let id = language
            .get("id")
            .and_then(Value::as_str)
            .ok_or("framework catalog language has no id")?;
        if !LANGUAGES.iter().any(|supported| supported.id == id) {
            return Err(format!("framework catalog language is not supported: {id}"));
        }
        let file = language
            .get("file")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("framework catalog language has no file: {id}"))?;
        let path = catalog_path.parent().unwrap_or(Path::new(".")).join(file);
        let document: Value = serde_json::from_slice(
            &fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?,
        )
        .map_err(|e| format!("invalid framework pack file {}: {e}", path.display()))?;
        if document.get("language").and_then(Value::as_str) != Some(id) {
            return Err(format!(
                "framework pack file has wrong language: {}",
                path.display()
            ));
        }
        for pack_ref in document
            .get("packs")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("framework pack file has no packs: {}", path.display()))?
        {
            let pack_id = pack_ref
                .get("id")
                .and_then(Value::as_str)
                .ok_or("framework pack has no id")?;
            let pack_file = pack_ref
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("framework pack has no path: {id}/{pack_id}"))?;
            let pack_path = path.parent().unwrap_or(Path::new(".")).join(pack_file);
            let pack: Value = serde_json::from_slice(
                &fs::read(&pack_path)
                    .map_err(|e| format!("cannot read {}: {e}", pack_path.display()))?,
            )
            .map_err(|e| format!("invalid framework pack {}: {e}", pack_path.display()))?;
            if pack.get("schema").and_then(Value::as_str) != Some("code-memory.framework-pack.v1")
                || pack.get("language").and_then(Value::as_str) != Some(id)
                || pack.get("id").and_then(Value::as_str) != Some(pack_id)
            {
                return Err(format!(
                    "framework pack reference mismatch: {}",
                    pack_path.display()
                ));
            }
            let fixture_path = pack_path
                .parent()
                .unwrap_or(Path::new("."))
                .join("fixture.json");
            let fixture: Value = serde_json::from_slice(
                &fs::read(&fixture_path)
                    .map_err(|e| format!("cannot read {}: {e}", fixture_path.display()))?,
            )
            .map_err(|e| format!("invalid framework fixture {}: {e}", fixture_path.display()))?;
            if fixture.get("schema").and_then(Value::as_str)
                != Some("code-memory.framework-fixture.v1")
                || fixture.get("language").and_then(Value::as_str) != Some(id)
                || fixture.get("framework").and_then(Value::as_str) != Some(pack_id)
            {
                return Err(format!(
                    "framework fixture reference mismatch: {}",
                    fixture_path.display()
                ));
            }
            let fixture_files =
                fixture
                    .get("files")
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        format!("framework fixture has no files: {}", fixture_path.display())
                    })?;
            if fixture_files.is_empty() {
                return Err(format!(
                    "framework fixture has no files: {}",
                    fixture_path.display()
                ));
            }
            for file in fixture_files {
                for field in ["path", "source"] {
                    if file
                        .get(field)
                        .and_then(Value::as_str)
                        .is_none_or(str::is_empty)
                    {
                        return Err(format!(
                            "framework fixture file has no {field}: {}",
                            fixture_path.display()
                        ));
                    }
                }
            }
            let fixture_facts = fixture
                .get("expected")
                .and_then(|expected| expected.get("facts"))
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    format!(
                        "framework fixture has no expected facts: {}",
                        fixture_path.display()
                    )
                })?;
            let rules = pack
                .get("rule_sets")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("framework pack has no rule_sets: {id}/{pack_id}"))?;
            let fixture_fact_names = fixture_facts
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>();
            let rule_names = rules.iter().filter_map(Value::as_str).collect::<Vec<_>>();
            if fixture_fact_names != rule_names {
                return Err(format!(
                    "framework fixture facts do not match rule_sets: {}",
                    fixture_path.display()
                ));
            }
            let qualified = format!("{id}/{pack_id}");
            if !seen.insert(qualified.clone()) {
                return Err(format!("duplicate framework pack: {qualified}"));
            }
            if adapters
                .get(&qualified)
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            {
                return Err(format!("framework pack has no adapter: {qualified}"));
            }
            for field in ["name", "kind"] {
                if pack.get(field).and_then(Value::as_str).is_none() {
                    return Err(format!("framework pack {qualified} has no {field}"));
                }
            }
            for field in ["signals", "outputs", "rule_sets"] {
                if pack
                    .get(field)
                    .and_then(Value::as_array)
                    .is_none_or(Vec::is_empty)
                {
                    return Err(format!("framework pack {qualified} has no {field}"));
                }
            }
            let outputs = pack
                .get("outputs")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("framework pack {qualified} has invalid outputs"))?;
            let rules = pack
                .get("rule_sets")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("framework pack {qualified} has invalid rule_sets"))?;
            for output in outputs.iter().filter_map(Value::as_str) {
                if output != "HANDLES"
                    && !rules
                        .iter()
                        .filter_map(Value::as_str)
                        .any(|rule| rule == output)
                {
                    return Err(format!(
                        "framework pack {qualified} output has no rule_set: {output}"
                    ));
                }
                if output == "HANDLES"
                    && !rules
                        .iter()
                        .filter_map(Value::as_str)
                        .any(|rule| rule == "HTTP_ROUTE" || rule == "RPC_ENDPOINT")
                {
                    return Err(format!(
                        "framework pack {qualified} HANDLES has no route or RPC rule"
                    ));
                }
            }
            println!("{qualified}");
            total += 1;
        }
    }
    println!("framework-packs\t{total}");
    Ok(())
}

fn doctor(providers_root: Option<&Path>) -> Result<(), String> {
    let mut missing = 0;
    for lang in LANGUAGES {
        let ready = provider_ready(lang, providers_root);
        if !ready {
            missing += 1;
        }
        println!(
            "{}\t{}\ttool={}",
            lang.id,
            if ready { "READY" } else { "MISSING" },
            if matches!(lang.id, "c" | "cpp") && find_tool(lang.tool, providers_root).is_none() {
                "clangd (fallback)"
            } else {
                lang.tool
            }
        );
    }
    if missing > 0 {
        println!("missing_tools\t{}", missing);
    }
    Ok(())
}

pub(crate) fn index_project(
    root: &Path,
    out: &Path,
    architecture_out: &Path,
    pack_root: &Path,
    providers_root: Option<&Path>,
) -> Result<(), String> {
    let mut root = root
        .canonicalize()
        .map_err(|e| format!("invalid project root {}: {e}", root.display()))?;
    if let Some(normal) = root.to_string_lossy().strip_prefix("\\\\?\\") {
        root = PathBuf::from(normal);
    }
    let pack_root = pack_root
        .canonicalize()
        .unwrap_or_else(|_| pack_root.to_path_buf());
    let mut output = IndexOutput {
        schema: "code-memory.language-index.v1",
        project_root: root.to_string_lossy().into_owned(),
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
    let mut language_files = HashMap::new();
    let all_extensions: HashSet<&str> = LANGUAGES
        .iter()
        .flat_map(|language| language.extensions.iter().copied())
        .collect();
    let all_source_files =
        collect_files(&root, &all_extensions.iter().copied().collect::<Vec<_>>());
    let mut source_snapshot = load_source_snapshot_metadata_from_files(&root, &all_source_files);
    let cache_impact_state = cache_impact(&root, out, architecture_out, &source_snapshot);
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

    let project_model_started = Instant::now();
    let mut typescript_units = Vec::new();
    let mut typescript_call_ranges = Arc::new(HashMap::new());
    if discovered_files
        .iter()
        .any(|(language, _)| matches!(language.as_str(), "typescript" | "javascript"))
    {
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
                message: format!("TypeScript project model unavailable: {error}"),
                path: None,
                line: None,
            }),
        }
    }
    output.timings.push(StageTiming {
        stage: "typescript_project_model",
        elapsed_ms: project_model_started.elapsed().as_millis(),
    });

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
            if !provider_ready(lang, providers_root.as_deref()) {
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
                        message: missing_tool_message(lang),
                        path: None,
                        line: None,
                    }],
                    project_excluded_files: 0,
                });
                continue;
            }

            let cache_key = language_cache_key(
                &module.root,
                lang,
                &module.files,
                providers_root.as_deref(),
                project_config_digest,
                &source_snapshot,
            );
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
            } else if let Some(cached) = load_language_cache(&module.root, lang, &cache_key) {
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
                diagnostics.extend(cached.diagnostics.into_iter().map(|diagnostic| Diagnostic {
                    language: diagnostic.language,
                    level: match diagnostic.level.as_str() {
                        "error" => "error",
                        "info" => "info",
                        _ => "warning",
                    },
                    message: diagnostic.message,
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
                allow_js: module.allow_js,
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
        stage: "file_discovery_and_cache_lookup",
        elapsed_ms: discovery_started.elapsed().as_millis(),
    });
    let jobs = merge_provider_jobs(jobs);

    let provider_started = Instant::now();
    let max_parallel = max_parallel_providers(jobs.len());
    let max_weight = max_provider_weight();
    let (result_sender, result_receiver) = mpsc::channel::<(usize, Vec<LanguageAnalysis>)>();
    let mut analyses = cached_analyses;
    let mut next_job = 0usize;
    let mut active_jobs = 0usize;
    let mut active_weight = 0usize;
    while next_job < jobs.len() || active_jobs > 0 {
        while next_job < jobs.len() && active_jobs < max_parallel {
            let job = jobs[next_job].clone();
            let weight = provider_job_weight(&job);
            if active_jobs > 0 && active_weight + weight > max_weight {
                break;
            }
            next_job += 1;
            active_jobs += 1;
            active_weight += weight;
            let sender = result_sender.clone();
            std::thread::spawn(move || {
                let members = job.members.clone();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    analyze_provider_job(job)
                }))
                .unwrap_or_else(|panic| {
                    let message = panic_message(panic);
                    members
                        .iter()
                        .map(|member| {
                            language_failure(
                                member.lang,
                                if matches!(member.lang.id, "c" | "cpp") {
                                    "native-lsp"
                                } else {
                                    match member.lang.provider {
                                        ProviderKind::Scip => "scip",
                                        ProviderKind::Lsp => "native-lsp",
                                    }
                                },
                                &member.files,
                                format!("provider worker panicked: {message}"),
                            )
                        })
                        .collect()
                });
                let _ = sender.send((weight, result));
            });
        }

        let (weight, result) = result_receiver
            .recv()
            .map_err(|error| format!("language worker stopped unexpectedly: {error}"))?;
        active_jobs -= 1;
        active_weight = active_weight.saturating_sub(weight);
        analyses.extend(result);
    }
    drop(result_sender);
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
    output.analysis_units = build_analysis_units(&root, &planned_units, &output.coverage);
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
    let framework_key = framework_cache_key(
        &root,
        &pack_root,
        &output.documents,
        &source_snapshot,
        project_config_digest,
    );
    let framework_cache = project_cache_root(&root).join(format!("framework-{framework_key}.json"));
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
                    message: error,
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

    write_index_outputs(
        &root,
        out,
        architecture_out,
        &pack_root,
        &output,
        &mut source_snapshot,
        project_config_digest,
    )?;
    if env::var("CODE_MEMORY_STRICT").as_deref() == Ok("1") {
        enforce_quality_gate(&output)?;
    }
    if let Err(error) = write_source_manifest(&root, &source_snapshot) {
        eprintln!("source manifest cache unavailable: {error}");
    }
    Ok(())
}

fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|message| (*message).to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_string())
}

fn build_analysis_units(
    project_root: &Path,
    plans: &[(String, String, PathBuf, Vec<PathBuf>, usize)],
    coverage: &[FileCoverageOutput],
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
            AnalysisUnitOutput {
                id: id.clone(),
                language: language.clone(),
                root: root.to_string_lossy().into_owned(),
                files_found: files.len() + *project_excluded,
                files_indexed: indexed,
                files_excluded: excluded + *project_excluded,
                files_missing: missing,
                status,
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
        let mut analysis = analyze_language(
            member.lang,
            &member.root,
            &member.project_root,
            &member.work,
            &member.files,
            &member.cache_key,
            member.providers_root.as_deref(),
            member.provider_config.as_deref(),
            member.allow_js,
            &member.call_ranges,
            member.project_config_digest,
        );
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
            files.len(),
            primary.provider_config.as_deref(),
            primary.allow_js,
            &files,
            primary.project_config_digest,
        )
        .map(|()| Vec::new())
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
                && provider_diagnostics
                    .iter()
                    .any(|diagnostic| is_fatal_lsp_error(&diagnostic.message));
            let mut analysis = if provider_stopped {
                language_excluded(
                    member.lang,
                    provider,
                    &member.files,
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
                            .is_some_and(|path| member_paths.contains(path))
                    })
                    .cloned(),
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

fn write_index_outputs(
    root: &Path,
    out: &Path,
    architecture_out: &Path,
    pack_root: &Path,
    output: &IndexOutput,
    source_snapshot: &mut SourceSnapshot,
    project_config_digest: u64,
) -> Result<(), String> {
    let index_write_started = Instant::now();
    let file = fs::File::create(out).map_err(|e| format!("cannot write {}: {e}", out.display()))?;
    let mut writer = BufWriter::new(file);
    write_json(&mut writer, output).map_err(|e| format!("cannot serialize output: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("cannot flush {}: {e}", out.display()))?;
    eprintln!(
        "timing stage=index_json_write elapsed_ms={}",
        index_write_started.elapsed().as_millis()
    );
    if let Some(parent) = architecture_out.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    let architecture_started = Instant::now();
    let architecture_key = architecture_cache_key(
        root,
        pack_root,
        output,
        source_snapshot,
        project_config_digest,
    );
    let architecture_cache =
        project_cache_root(root).join(format!("architecture-{architecture_key}.json"));
    if architecture_cache.is_file() {
        fs::copy(&architecture_cache, architecture_out).map_err(|e| {
            format!(
                "cannot copy architecture cache {} to {}: {e}",
                architecture_cache.display(),
                architecture_out.display()
            )
        })?;
        eprintln!(
            "timing stage=architecture_and_json elapsed_ms={} cached=true key={architecture_key}",
            architecture_started.elapsed().as_millis()
        );
        println!("wrote {}", out.display());
        println!("wrote {}", architecture_out.display());
        return Ok(());
    }
    load_source_contents(root, source_snapshot);
    let architecture = architecture::build_with_sources(root, output, source_snapshot);
    let file = fs::File::create(architecture_out)
        .map_err(|e| format!("cannot write {}: {e}", architecture_out.display()))?;
    let mut writer = BufWriter::new(file);
    write_json(&mut writer, &architecture)
        .map_err(|e| format!("cannot serialize architecture output: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("cannot flush {}: {e}", architecture_out.display()))?;
    if let Some(parent) = architecture_cache.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::copy(architecture_out, architecture_cache);
    eprintln!(
        "timing stage=architecture_and_json elapsed_ms={} cached=false key={architecture_key}",
        architecture_started.elapsed().as_millis()
    );
    println!("wrote {}", out.display());
    println!("wrote {}", architecture_out.display());
    Ok(())
}

fn write_json<T: Serialize, W: Write>(writer: &mut W, value: &T) -> serde_json::Result<()> {
    if env::var_os("CODE_MEMORY_PRETTY_JSON").is_some() {
        serde_json::to_writer_pretty(writer, value)
    } else {
        serde_json::to_writer(writer, value)
    }
}

fn resolve_output_path(path: PathBuf) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(env::current_dir()
            .map_err(|e| format!("cannot resolve output path: {e}"))?
            .join(path))
    }
}

fn default_architecture_output(out: &Path) -> PathBuf {
    let stem = out
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("language-index");
    out.with_file_name(format!("{stem}.architecture.json"))
}

fn required_path(args: &[String], flag: &str) -> Result<PathBuf, String> {
    optional_path(args, flag).ok_or_else(|| format!("missing {flag} <path>"))
}

fn optional_path(args: &[String], flag: &str) -> Option<PathBuf> {
    for (index, value) in args.iter().enumerate() {
        if value == flag {
            return args.get(index + 1).map(PathBuf::from);
        }
        if let Some(path) = value.strip_prefix(&format!("{flag}=")) {
            return Some(PathBuf::from(path));
        }
    }
    None
}

#[allow(dead_code)]
fn print_json<T: Serialize>(value: &T) -> Result<(), String> {
    serde_json::to_writer_pretty(io::stdout(), value).map_err(|e| e.to_string())?;
    io::stdout().write_all(b"\n").map_err(|e| e.to_string())
}
