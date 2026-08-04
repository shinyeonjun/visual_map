use std::collections::{HashMap, HashSet};
use std::env;

use std::fs;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::{
    dotnet_requires_unavailable_legacy_sdk, find_tool, has_compile_context_for_files,
    is_fatal_lsp_error, lsp_failure_code, normalize_scip_path, read_scip, run_native_lsp,
    run_native_lsp_source_only, run_native_lsp_with_server, run_scip_indexer, write_language_cache,
    Diagnostic, DiagnosticCode, DocumentCoverage, DocumentOutput, FileCoverageOutput,
    LanguageAnalysis, LanguageJob, LanguageOutput, LanguageSpec, ProviderKind, RelationOutput,
};

static RUST_BUILD_TOOL_GAP_CACHE: OnceLock<Mutex<HashMap<PathBuf, bool>>> = OnceLock::new();

pub(crate) fn language_failure(
    lang: LanguageSpec,
    provider: &'static str,
    files: &[PathBuf],
    error: String,
) -> LanguageAnalysis {
    let (message, detail) = provider_error_parts(&error, lang.name);
    LanguageAnalysis {
        language: LanguageOutput {
            id: lang.id.to_string(),
            name: lang.name.to_string(),
            provider,
            files_found: files.len(),
            files_indexed: 0,
            files_excluded: 0,
            files_missing: files.len(),
            status: "indexer-failed",
        },
        documents: Vec::new(),
        relations: Vec::new(),
        diagnostics: vec![Diagnostic {
            language: lang.id.to_string(),
            level: "error",
            code: DiagnosticCode::ProviderFailed,
            message,
            detail,
            path: None,
            line: None,
        }],
        project_excluded_files: 0,
    }
}

fn provider_error_parts(error: &str, language: &str) -> (String, Option<String>) {
    let Some((_message, detail)) = error.split_once("; stderr_tail: ") else {
        return (error.to_string(), None);
    };
    (
        format!("{language} semantic provider failed; provider output is attached"),
        (!detail.trim().is_empty()).then(|| detail.trim().to_string()),
    )
}

pub(crate) fn language_invalid_output(
    lang: LanguageSpec,
    provider: &'static str,
    files: &[PathBuf],
    error: String,
) -> LanguageAnalysis {
    LanguageAnalysis {
        language: LanguageOutput {
            id: lang.id.to_string(),
            name: lang.name.to_string(),
            provider,
            files_found: files.len(),
            files_indexed: 0,
            files_excluded: 0,
            files_missing: files.len(),
            status: "invalid-output",
        },
        documents: Vec::new(),
        relations: Vec::new(),
        diagnostics: vec![Diagnostic {
            language: lang.id.to_string(),
            level: "error",
            code: DiagnosticCode::InvalidOutput,
            message: error,
            detail: None,
            path: None,
            line: None,
        }],
        project_excluded_files: 0,
    }
}

pub(crate) fn language_empty_output(
    lang: LanguageSpec,
    provider: &'static str,
    root: &Path,
    files: &[PathBuf],
) -> LanguageAnalysis {
    let coverage = language_document_coverage(root, lang, files, &[]);
    LanguageAnalysis {
        language: LanguageOutput {
            id: lang.id.to_string(),
            name: lang.name.to_string(),
            provider,
            files_found: files.len(),
            files_indexed: 0,
            // An empty provider result is not a deliberate source exclusion.
            // Keep the files in coverage as missing so VisualMap never treats
            // an unanalyzed module as complete.
            files_excluded: coverage.excluded,
            files_missing: coverage.missing,
            status: "empty-semantic",
        },
        documents: Vec::new(),
        relations: Vec::new(),
        diagnostics: vec![Diagnostic {
            language: lang.id.to_string(),
            level: "info",
            code: DiagnosticCode::EmptySemantic,
            message: format!(
                "{} provider analyzed the unit but returned no semantic facts",
                lang.name
            ),
            detail: None,
            path: None,
            line: None,
        }],
        project_excluded_files: 0,
    }
}

pub(crate) fn language_analysis_from_index(
    lang: LanguageSpec,
    provider: &'static str,
    root: &Path,
    files: &[PathBuf],
    documents: Vec<DocumentOutput>,
    relations: Vec<RelationOutput>,
) -> LanguageAnalysis {
    let semantic_items: usize = documents
        .iter()
        .map(|doc| doc.symbols.len() + doc.occurrences.len())
        .sum();
    if documents.is_empty() || semantic_items == 0 {
        return language_empty_output(lang, provider, root, files);
    }
    let (status, diagnostics) = classify_language_documents(root, &lang, files, &documents);
    let coverage = language_document_coverage(root, lang, files, &documents);
    LanguageAnalysis {
        language: LanguageOutput {
            id: lang.id.to_string(),
            name: lang.name.to_string(),
            provider,
            files_found: files.len(),
            files_indexed: coverage.indexed,
            files_excluded: coverage.excluded,
            files_missing: coverage.missing,
            status,
        },
        documents,
        relations,
        diagnostics,
        project_excluded_files: 0,
    }
}

pub(crate) fn analyze_language(job: &LanguageJob) -> LanguageAnalysis {
    let lang = job.lang;
    let root = &job.root;
    let work = &job.work;
    let files = &job.files;
    let cache_key = &job.cache_key;
    let providers_root = job.providers_root.as_deref();
    let provider_config = job.provider_config.as_deref();
    let call_ranges = job.call_ranges.as_ref();
    let project_config_digest = job.project_config_digest;
    if lang.id == "rust" && files.len() > rust_semantic_file_limit() {
        return language_excluded(
            lang,
            "native-lsp",
            files,
            DiagnosticCode::LargeWorkspacePartial,
            "Rust semantic analysis deferred for a large crate; structural map remains available",
        );
    }
    if lang.id == "rust" && rust_build_requires_missing_tool(root, providers_root) {
        return language_excluded(
            lang,
            "native-lsp",
            files,
            DiagnosticCode::MissingExternalTool,
            "Rust semantic analysis skipped because the crate build script requires an unavailable external tool; structural map remains available",
        );
    }
    if lang.id == "csharp" && dotnet_requires_unavailable_legacy_sdk(root, files) {
        return language_excluded(
            lang,
            "scip",
            files,
            DiagnosticCode::MissingLegacySdk,
            "C# semantic analysis skipped because the project targets an unavailable legacy SDK; structural map remains available",
        );
    }
    let use_clangd_fallback =
        matches!(lang.id, "c" | "cpp") && find_tool(lang.tool, providers_root).is_none();
    if use_clangd_fallback && !has_compile_context_for_files(root, files) {
        if files.iter().all(|file| is_c_family_header(file)) {
            return language_excluded(
                lang,
                "native-lsp",
                files,
                DiagnosticCode::MissingCompileContext,
                "C/C++ headers have no compile context; kept out of semantic indexing",
            );
        }
        return language_excluded(
            lang,
            "native-lsp",
            files,
            DiagnosticCode::MissingCompileContext,
            &format!(
                "{} semantic analysis skipped because no usable compile context was found; structural map remains available",
                lang.name
            ),
        );
    }
    let provider = if use_clangd_fallback {
        "native-lsp"
    } else if matches!(lang.provider, ProviderKind::Scip) {
        "scip"
    } else {
        "native-lsp"
    };
    // Do not send files that the coverage policy already excludes to the
    // provider. Build-tag-only Go modules are the common case, but the same
    // rule also prevents generated/oversized files from producing a false
    // empty-semantic unit. Keep the original file list for final coverage.
    let provider_files: Vec<PathBuf> = files
        .iter()
        .filter(|file| source_exclusion_reason(file).is_none())
        .cloned()
        .collect();
    if provider_files.is_empty() {
        return language_excluded(
            lang,
            provider,
            files,
            DiagnosticCode::Internal,
            "all files in this provider unit are excluded by source policy",
        );
    }
    let ruby_bundle_warning = (lang.id == "ruby")
        .then(|| root.join(".ruby-lsp").join("install_error"))
        .filter(|path| path.is_file())
        .map(|path| Diagnostic {
            language: lang.id.to_string(),
            level: "warning",
            code: DiagnosticCode::RubyBundleWarning,
            message: format!(
                "Ruby LSP previously reported a bundle setup problem at {}; provider results are retained, but project gem resolution may be incomplete",
                path.display()
            ),
            detail: None,
            path: Some(".ruby-lsp/install_error".to_string()),
            line: None,
        });
    // Each project unit can run concurrently. Include its content cache key so
    // providers never write to the same SCIP file.
    let scip_path = work.join(format!("{}_{}.scip", lang.id, cache_key));
    let _ = fs::remove_file(&scip_path);
    let result = match lang.provider {
        ProviderKind::Scip if use_clangd_fallback => run_native_lsp_with_server(
            &lang,
            "clangd",
            root,
            &scip_path,
            providers_root,
            &provider_files,
        ),
        ProviderKind::Scip => run_scip_indexer(
            &lang,
            root,
            &scip_path,
            providers_root,
            provider_config,
            &provider_files,
            project_config_digest,
        ),
        ProviderKind::Lsp => {
            run_native_lsp(&lang, root, &scip_path, providers_root, &provider_files)
        }
    };

    match result {
        Ok(mut provider_diagnostics) => {
            let allowed_paths = allowed_document_paths(root, &provider_files);
            let mut parsed =
                read_scip(&scip_path, lang.id, root, &allowed_paths, Some(call_ranges));
            let mut java_source_fallback_used = false;
            let java_needs_source_fallback = lang.id == "java"
                && parsed
                    .as_ref()
                    .is_ok_and(|(documents, _)| semantic_output_is_empty(documents));
            if java_needs_source_fallback {
                let _ = fs::remove_file(&scip_path);
                match run_native_lsp_source_only(
                    &lang,
                    root,
                    &scip_path,
                    providers_root,
                    &provider_files,
                ) {
                    Ok(fallback_diagnostics) => {
                        match read_scip(
                            &scip_path,
                            lang.id,
                            root,
                            &allowed_paths,
                            Some(call_ranges),
                        ) {
                            Ok((documents, relations)) if !semantic_output_is_empty(&documents) => {
                                java_source_fallback_used = true;
                                provider_diagnostics.push(Diagnostic {
                                    language: lang.id.to_string(),
                                    level: "warning",
                                    code: DiagnosticCode::JavaSourceFallback,
                                    message: "Java build import returned no semantic facts; source-only fallback retained project declarations and local relationships without a complete build classpath".to_string(),
                                    detail: None,
                                    path: None,
                                    line: None,
                                });
                                provider_diagnostics.extend(fallback_diagnostics);
                                parsed = Ok((documents, relations));
                            }
                            Ok(_) => provider_diagnostics.extend(fallback_diagnostics),
                            Err(error) => provider_diagnostics.push(Diagnostic {
                                language: lang.id.to_string(),
                                level: "warning",
                                code: DiagnosticCode::JavaSourceFallbackFailed,
                                message: format!(
                                    "Java source-only fallback returned invalid output: {error}"
                                ),
                                detail: None,
                                path: None,
                                line: None,
                            }),
                        }
                    }
                    Err(error) => provider_diagnostics.push(Diagnostic {
                        language: lang.id.to_string(),
                        level: "warning",
                        code: DiagnosticCode::JavaSourceFallbackFailed,
                        message: format!("Java source-only fallback failed: {error}"),
                        detail: None,
                        path: None,
                        line: None,
                    }),
                }
            }
            let _ = fs::remove_file(&scip_path);
            match parsed {
                Ok((documents, relations)) => {
                    let provider_stopped = provider == "native-lsp"
                        && documents.is_empty()
                        && provider_diagnostics
                            .iter()
                            .any(|diagnostic| diagnostic.code == DiagnosticCode::ProviderTimeout);
                    let provider_partial = java_source_fallback_used
                        || provider_diagnostics
                            .iter()
                            .any(provider_diagnostic_is_partial);
                    let mut analysis = if provider_stopped {
                        language_excluded(
                            lang,
                            provider,
                            files,
                            DiagnosticCode::ProviderStopped,
                            &format!(
                                "{} semantic provider stopped; structural map remains available",
                                lang.name
                            ),
                        )
                    } else {
                        language_analysis_from_index(
                            lang, provider, root, files, documents, relations,
                        )
                    };
                    if provider_partial && analysis.language.status == "indexed" {
                        analysis.language.status = "indexed-partial";
                    }
                    if let Some(warning) = ruby_bundle_warning.clone() {
                        analysis.diagnostics.push(warning);
                    }
                    analysis.diagnostics.extend(provider_diagnostics);
                    write_language_cache(
                        root,
                        lang,
                        cache_key,
                        &analysis.documents,
                        &analysis.relations,
                        &analysis.diagnostics,
                    );
                    analysis
                }
                Err(error) => language_invalid_output(lang, provider, files, error),
            }
        }
        Err(error) => {
            let _ = fs::remove_file(&scip_path);
            if provider == "native-lsp" && is_fatal_lsp_error(&error) {
                language_excluded(
                    lang,
                    provider,
                    files,
                    lsp_failure_code(&error).unwrap_or(DiagnosticCode::ProviderStopped),
                    &format!(
                        "{} semantic provider stopped ({error}); structural map remains available",
                        lang.name
                    ),
                )
            } else {
                language_failure(lang, provider, files, error)
            }
        }
    }
}

pub(crate) fn provider_diagnostic_is_partial(diagnostic: &Diagnostic) -> bool {
    matches!(
        diagnostic.code,
        DiagnosticCode::ProviderTimeout
            | DiagnosticCode::LargeWorkspacePartial
            | DiagnosticCode::DependencyMetadataGap
            | DiagnosticCode::JavaSourceFallback
            | DiagnosticCode::TypescriptSourceFallback
    )
}

fn semantic_output_is_empty(documents: &[DocumentOutput]) -> bool {
    documents.is_empty()
        || documents
            .iter()
            .all(|document| document.symbols.is_empty() && document.occurrences.is_empty())
}

pub(crate) fn rust_semantic_file_limit() -> usize {
    env::var("CODE_MEMORY_RUST_SEMANTIC_MAX_FILES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| (100..=20_000).contains(value))
        .unwrap_or(1_500)
}

fn rust_build_requires_missing_tool(root: &Path, providers_root: Option<&Path>) -> bool {
    if find_tool("make", providers_root).is_some() {
        return false;
    }
    let scan_root = rust_workspace_root(root);
    let cache = RUST_BUILD_TOOL_GAP_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(value) = cache
        .lock()
        .ok()
        .and_then(|cache| cache.get(&scan_root).copied())
    {
        return value;
    }
    let missing = rust_tree_requires_make(&scan_root);
    if let Ok(mut cache) = cache.lock() {
        cache.insert(scan_root, missing);
    }
    missing
}

fn rust_tree_requires_make(scan_root: &Path) -> bool {
    let mut pending = vec![scan_root.to_path_buf()];
    let mut missing = false;
    while let Some(directory) = pending.pop() {
        if crate::source::is_managed_provider_root(&directory) {
            continue;
        }
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                let name = entry.file_name();
                if !matches!(
                    name.to_string_lossy().as_ref(),
                    ".git" | "target" | "node_modules"
                ) {
                    pending.push(path);
                }
            } else if path.file_name().and_then(|name| name.to_str()) == Some("build.rs") {
                let Ok(source) = fs::read_to_string(path) else {
                    continue;
                };
                if source.contains("Command::new(\"make\")")
                    || source.contains("make_cmd::")
                    || source.contains("Make failed to run")
                {
                    missing = true;
                    pending.clear();
                    break;
                }
            }
        }
    }
    missing
}

fn rust_workspace_root(root: &Path) -> PathBuf {
    root.ancestors()
        .find(|path| {
            fs::read_to_string(path.join("Cargo.toml"))
                .is_ok_and(|source| source.contains("[workspace"))
        })
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.to_path_buf())
}

pub(crate) fn language_excluded(
    lang: LanguageSpec,
    provider: &'static str,
    files: &[PathBuf],
    code: DiagnosticCode,
    reason: &str,
) -> LanguageAnalysis {
    LanguageAnalysis {
        language: LanguageOutput {
            id: lang.id.to_string(),
            name: lang.name.to_string(),
            provider,
            files_found: files.len(),
            files_indexed: 0,
            files_excluded: files.len(),
            files_missing: 0,
            status: "excluded",
        },
        documents: Vec::new(),
        relations: Vec::new(),
        diagnostics: vec![Diagnostic {
            language: lang.id.to_string(),
            level: "warning",
            code,
            message: reason.to_string(),
            detail: None,
            path: None,
            line: None,
        }],
        project_excluded_files: 0,
    }
}

fn is_c_family_header(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("h" | "hh" | "hpp" | "hxx" | "inc" | "inl" | "ipp" | "tpp")
    )
}

pub(crate) fn classify_language_documents(
    root: &Path,
    lang: &LanguageSpec,
    files: &[PathBuf],
    documents: &[DocumentOutput],
) -> (&'static str, Vec<Diagnostic>) {
    let coverage = language_document_coverage(root, *lang, files, documents);
    let expected = allowed_document_paths(root, files);
    let indexed: HashSet<&str> = documents
        .iter()
        .map(|document| document.path.as_str())
        .collect();
    if indexed.len() >= expected.len() || coverage.missing == 0 {
        return ("indexed", Vec::new());
    }

    let missing = coverage.missing;
    (
        "indexed-partial",
        vec![Diagnostic {
            language: lang.id.to_string(),
            level: "warning",
            code: DiagnosticCode::PartialCoverage,
            message: format!(
                "{} provider indexed {} of {} source documents; {} excluded and {} unresolved or outside project configuration",
                lang.name,
                coverage.indexed,
                expected.len(),
                coverage.excluded,
                missing,
            ),
            detail: None,
            path: None,
            line: None,
        }],
    )
}

pub(crate) fn document_coverage(
    root: &Path,
    files: &[PathBuf],
    documents: &[DocumentOutput],
) -> DocumentCoverage {
    let indexed: HashSet<&str> = documents
        .iter()
        .map(|document| document.path.as_str())
        .collect();
    let mut coverage = DocumentCoverage::default();
    for file in files {
        let relative = file.strip_prefix(root).unwrap_or(file);
        let path = normalize_scip_path(&relative.to_string_lossy(), root);
        if indexed.contains(path.as_str()) {
            coverage.indexed += 1;
        } else if source_exclusion_reason(file).is_some() {
            coverage.excluded += 1;
        } else {
            coverage.missing += 1;
        }
    }
    coverage
}

pub(crate) fn language_document_coverage(
    root: &Path,
    lang: LanguageSpec,
    files: &[PathBuf],
    documents: &[DocumentOutput],
) -> DocumentCoverage {
    let mut coverage = document_coverage(root, files, documents);
    if matches!(lang.id, "c" | "cpp") {
        let file_refs: Vec<&PathBuf> = files.iter().collect();
        let reachable = crate::providers::reachable_project_headers(root, &file_refs);
        let indexed: HashSet<&str> = documents
            .iter()
            .map(|document| document.path.as_str())
            .collect();
        for file in files {
            if !is_c_family_header(file) || reachable.contains(file) {
                continue;
            }
            let path = normalize_scip_path(
                &file.strip_prefix(root).unwrap_or(file).to_string_lossy(),
                root,
            );
            if !indexed.contains(path.as_str()) {
                coverage.missing = coverage.missing.saturating_sub(1);
                coverage.excluded += 1;
            }
        }
    }
    if lang.id == "csharp" {
        let project_roots = crate::providers::dotnet_project_roots_for_files(root, files);
        if !project_roots.is_empty() {
            let indexed: HashSet<&str> = documents
                .iter()
                .map(|document| document.path.as_str())
                .collect();
            for file in files {
                let path = normalize_scip_path(
                    &file.strip_prefix(root).unwrap_or(file).to_string_lossy(),
                    root,
                );
                let canonical = file.canonicalize().unwrap_or_else(|_| file.clone());
                if !indexed.contains(path.as_str())
                    && !project_roots
                        .iter()
                        .any(|project_root| canonical.starts_with(project_root))
                {
                    coverage.missing = coverage.missing.saturating_sub(1);
                    coverage.excluded += 1;
                }
            }
        }
    }
    coverage
}

pub(crate) fn build_file_coverage(
    root: &Path,
    files: &[(String, PathBuf)],
    documents: &[DocumentOutput],
    languages: &[LanguageOutput],
    project_model_files: &[String],
) -> Vec<FileCoverageOutput> {
    let indexed: HashSet<&str> = documents
        .iter()
        .map(|document| document.path.as_str())
        .collect();
    let language_status: HashMap<&str, &str> = languages
        .iter()
        .map(|language| (language.id.as_str(), language.status))
        .collect();
    let language_excluded_complete: HashSet<&str> = languages
        .iter()
        .filter(|language| language.files_missing == 0 && language.files_excluded > 0)
        .map(|language| language.id.as_str())
        .collect();
    let project_model_files: HashSet<&str> =
        project_model_files.iter().map(String::as_str).collect();
    let c_family_files: Vec<PathBuf> = files
        .iter()
        .filter(|(language, _)| matches!(language.as_str(), "c" | "cpp"))
        .map(|(_, file)| file.clone())
        .collect();
    let active_c_files = crate::compile_database_files_for_scope(root, &c_family_files);
    let csharp_files: Vec<PathBuf> = files
        .iter()
        .filter(|(language, _)| language == "csharp")
        .map(|(_, file)| file.clone())
        .collect();
    let dotnet_project_roots =
        crate::providers::dotnet_project_roots_for_files(root, &csharp_files);
    let reachable_c_headers = if active_c_files.is_some() {
        let file_refs: Vec<&PathBuf> = c_family_files.iter().collect();
        crate::providers::reachable_project_headers(root, &file_refs)
    } else {
        HashSet::new()
    };
    let mut coverage = files
        .iter()
        .map(|(language, file)| {
            let relative = file.strip_prefix(root).unwrap_or(file);
            let path = relative.to_string_lossy().replace('\\', "/");
            let canonical = file.canonicalize().unwrap_or_else(|_| file.clone());
            let header_not_reachable = matches!(language.as_str(), "c" | "cpp")
                && is_c_family_header_path(file)
                && active_c_files.is_some()
                && !reachable_c_headers.contains(&canonical);
            let dotnet_outside_project = language == "csharp"
                && !dotnet_project_roots.is_empty()
                && !dotnet_project_roots
                    .iter()
                    .any(|project_root| canonical.starts_with(project_root));
            let modeled_vue = language == "typescript"
                && file
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("vue"))
                && project_model_files.contains(path.as_str());
            let source_exclusion = source_exclusion_reason(file);
            let provider_status = language_status.get(language.as_str()).copied();
            let no_compile_context = provider_status == Some("excluded");
            let provider_excluded = language_excluded_complete.contains(language.as_str());
            let inactive_c_file = matches!(language.as_str(), "c" | "cpp")
                && active_c_files.as_ref().is_some_and(|active| {
                    !is_c_family_header_path(file) && !active.contains(&canonical)
                });
            let status = if indexed.contains(path.as_str()) || modeled_vue {
                "indexed"
            } else if source_exclusion.is_some()
                || no_compile_context
                || provider_excluded
                || header_not_reachable
                || dotnet_outside_project
                || inactive_c_file
            {
                "excluded"
            } else {
                "missing"
            };
            let reason = if status == "excluded" {
                source_exclusion
                    .map(str::to_string)
                    .or_else(|| no_compile_context.then_some("no-compile-context".to_string()))
                    .or_else(|| provider_excluded.then_some("provider-excluded".to_string()))
                    .or_else(|| header_not_reachable.then_some("header-not-reachable".to_string()))
                    .or_else(|| dotnet_outside_project.then_some("project-config".to_string()))
                    .or_else(|| inactive_c_file.then_some("not-in-active-build".to_string()))
            } else if status == "missing" {
                let project_scoped = matches!(language.as_str(), "typescript" | "javascript")
                    && !project_model_files.is_empty()
                    && project_model_files.contains(path.as_str());
                Some(
                    if matches!(provider_status, Some("missing-tool")) {
                        "provider-missing"
                    } else if matches!(provider_status, Some("indexer-failed" | "invalid-output")) {
                        "provider-failed"
                    } else if matches!(provider_status, Some("excluded-by-project-config"))
                        || (matches!(language.as_str(), "typescript" | "javascript")
                            && !project_scoped)
                    {
                        "project-config"
                    } else {
                        "not-returned-by-provider"
                    }
                    .to_string(),
                )
            } else {
                None
            };
            FileCoverageOutput {
                language: language.clone(),
                path,
                status,
                reason,
            }
        })
        .collect::<Vec<_>>();
    coverage.sort_by(|left, right| {
        let rank = |item: &FileCoverageOutput| match item.status {
            "indexed" => 0,
            "excluded" => 1,
            _ => 2,
        };
        (&left.path, rank(left), &left.language).cmp(&(&right.path, rank(right), &right.language))
    });
    // A C/C++ header can match both language extension sets. Keep one
    // coverage record, preferring the provider that actually indexed it.
    coverage.dedup_by(|left, right| left.path == right.path);
    coverage
}

fn is_c_family_header_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("h" | "hh" | "hpp" | "hxx" | "inc" | "inl" | "ipp" | "tpp")
    )
}

pub(crate) fn source_exclusion_reason(path: &Path) -> Option<&'static str> {
    let normalized = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    if normalized.contains("/generated/")
        || normalized.contains("/generated-")
        || normalized.contains("/codegen/")
        || normalized.contains("/__generated__/")
        || normalized.ends_with(".generated.ts")
        || normalized.ends_with(".generated.tsx")
    {
        return Some("generated");
    }
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("go"))
    {
        if let Ok(source) = fs::read_to_string(path) {
            if source.lines().take(32).any(|line| {
                let line = line.trim();
                line.starts_with("//go:build") || line.starts_with("// +build")
            }) {
                return Some("go-build-constraint");
            }
            let mut has_package = false;
            let package_marker_only = source.lines().map(str::trim).all(|line| {
                if line.is_empty() || line.starts_with("//") {
                    true
                } else if line.starts_with("package ") && !has_package {
                    has_package = true;
                    true
                } else {
                    false
                }
            });
            if has_package && package_marker_only {
                return Some("go-package-marker");
            }
        }
    }
    fs::metadata(path)
        .ok()
        .filter(|metadata| metadata.len() > 1_000_000)
        .map(|_| "provider-size-limit")
}

pub(crate) fn allowed_document_paths(root: &Path, files: &[PathBuf]) -> HashSet<String> {
    files
        .iter()
        .map(|file| {
            let relative = file.strip_prefix(root).unwrap_or(file);
            normalize_scip_path(&relative.to_string_lossy(), root)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::rust_tree_requires_make;

    #[test]
    fn rust_build_tool_scan_does_not_escape_the_crate_root() {
        let parent = std::env::temp_dir().join(format!(
            "code-memory-rust-build-boundary-{}",
            std::process::id()
        ));
        let crate_root = parent.join("crate");
        let _ = std::fs::remove_dir_all(&parent);
        std::fs::create_dir_all(&crate_root).unwrap();
        std::fs::write(parent.join("build.rs"), "Command::new(\"make\");").unwrap();

        assert!(!rust_tree_requires_make(&crate_root));

        std::fs::write(crate_root.join("build.rs"), "Command::new(\"make\");").unwrap();
        assert!(rust_tree_requires_make(&crate_root));
        let _ = std::fs::remove_dir_all(parent);
    }
}
