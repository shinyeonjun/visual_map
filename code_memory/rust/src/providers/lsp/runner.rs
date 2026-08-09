fn lsp_open_document_limit(
    server: &str,
    file_large_workspace: bool,
    java_source_only: bool,
) -> Option<usize> {
    if !file_large_workspace {
        return None;
    }
    match server {
        "dart" | "rust-analyzer" => Some(256),
        // A modeled JDTLS workspace indexes unopened source through the build
        // project. Source-only fallback has no such project: unopened Java
        // files return empty document symbols and silently disappear from the
        // definition denominator. Keep the cap only for the modeled pass.
        "jdtls" if !java_source_only => Some(256),
        _ => None,
    }
}

fn trace_lsp_phase(server: &str, phase: &str, state: &str, started: Instant) {
    if std::env::var_os("CODE_MEMORY_LSP_TIMING").is_some() {
        eprintln!(
            "lsp phase server={server} phase={phase} state={state} elapsed_ms={}",
            started.elapsed().as_millis()
        );
    }
}

fn run_native_lsp_with_server_mode(
    lang: &LanguageSpec,
    server: &str,
    roots: ProviderRoots<'_>,
    out: &Path,
    providers_root: Option<&Path>,
    files: &[PathBuf],
    java_source_only: bool,
) -> Result<ProviderRunOutcome, String> {
    let provider_started = Instant::now();
    trace_lsp_phase(server, "provider", "start", provider_started);
    let project_root = roots.project;
    let root = roots.analysis;
    let analysis_root = lsp_workspace_root(lang, root, files);
    let canonical_project_root = project_root.canonicalize().map_err(|error| {
        format!(
            "cannot resolve selected project root {}: {error}",
            project_root.display()
        )
    })?;
    let canonical_analysis_root = analysis_root.canonicalize().map_err(|error| {
        format!(
            "cannot resolve {} LSP workspace {}: {error}",
            lang.name,
            analysis_root.display()
        )
    })?;
    if !canonical_analysis_root.starts_with(&canonical_project_root) {
        return Err(format!(
            "{} LSP workspace escaped the selected project root: {}",
            lang.name,
            analysis_root.display()
        ));
    }
    let analysis_files: Vec<&PathBuf> = files
        .iter()
        .filter(|file| file.starts_with(&analysis_root))
        .collect();
    if analysis_files.is_empty() {
        return Err(format!("{} has no files in its LSP workspace", lang.name));
    }
    let mut startup_diagnostics = Vec::new();
    let dart_package_config = if server == "dart" {
        if let Some(reason) = dart_dependency_metadata_gap(&analysis_root) {
            startup_diagnostics.push(Diagnostic {
                language: lang.id.to_string(),
                level: "warning",
                code: DiagnosticCode::DependencyMetadataGap,
                message: format!(
                    "{reason}; using a temporary local-only package map and leaving unavailable packages external"
                ),
                detail: None,
                path: Some("pubspec.yaml".to_string()),
                line: None,
            });
        }
        Some(dart_package_config(&analysis_root)?)
    } else {
        None
    };
    let mut explicit_context_file = dart_package_config.clone();
    let mut generated_context_file = dart_package_config
        .as_ref()
        .filter(|path| path.ends_with("package_config.synthetic.json"))
        .cloned();
    let mut command = tool_command(server, providers_root)?;
    if server == "clangd" {
        // The desktop opens and queries project files explicitly. Disable
        // clangd's second, repository-wide background index to avoid keeping
        // duplicate ASTs for every compile-database configuration in memory.
        command.arg("--background-index=false");
        if let Some(directory) = prepare_clangd_compile_database(
            root,
            files,
            out.parent().unwrap_or_else(|| Path::new(".")),
        ) {
            command.arg(format!("--compile-commands-dir={}", directory.display()));
            let compile_database = directory.join("compile_commands.json");
            if compile_database
                .canonicalize()
                .is_ok_and(|path| path.starts_with(&canonical_project_root))
            {
                explicit_context_file = Some(compile_database);
            } else {
                generated_context_file = Some(compile_database);
            }
        }
    }
    if server == "jdtls" {
        let heap_mb = jdtls_heap_mb(
            analysis_files.len(),
            crate::providers::scheduler::provider_memory_budget_mb(),
        );
        command.env("CODE_MEMORY_JDTLS_HEAP_MB", heap_mb.to_string());
        eprintln!(
            "jdtls execution files={} source_only={} heap_mb={}",
            analysis_files.len(),
            java_source_only,
            heap_mb
        );
        command.env(
            "CODE_MEMORY_JDTLS_WORKSPACE",
            project_cache_root(&analysis_root)
                .join("lsp-workspaces")
                .join(if java_source_only {
                    "java-source-v1"
                } else {
                    "java-v2"
                }),
        );
        // The bundled launcher uses its own Java executable. Preserve a valid
        // project JAVA_HOME so Gradle can satisfy an exact toolchain request;
        // replace only missing or stale values with the bundled runtime.
        let inherited_java_home = env::var_os("JAVA_HOME")
            .map(PathBuf::from)
            .filter(|path| java_home_is_usable(path));
        if inherited_java_home.is_none() {
            if let Some(jdtls_path) = find_tool("jdtls", providers_root) {
                if let Some(bundled_java_home) = bundled_java_home(&jdtls_path) {
                    command.env("JAVA_HOME", bundled_java_home);
                }
            }
        }
    }
    if server == "gopls" {
        let go_context = go_execution_environment();
        // Hidden per-user GOENV state would make the same folder produce a
        // different graph. Execute only the explicit app environment and the
        // semantic build tags that are recorded in the context receipt.
        command.env("GOENV", "off");
        command.env("GOOS", &go_context.platform);
        command.env("GOARCH", &go_context.architecture);
        command.env("GOFLAGS", &go_context.flags);
        command.arg("serve");
    } else if server == "pyright-langserver" {
        command.arg("--stdio");
    } else if server == "dart" {
        let dart = find_tool("dart", providers_root).ok_or("Dart SDK executable not found")?;
        let snapshot = dart
            .parent()
            .and_then(Path::parent)
            .map(|bin| bin.join(r"bin\snapshots\analysis_server.dart.snapshot"))
            .ok_or("cannot resolve Dart analysis server snapshot")?;
        if !snapshot.is_file() {
            return Err(format!(
                "Dart analysis server snapshot not found at {}",
                snapshot.display()
            ));
        }
        command.arg(snapshot);
        command.arg("--packages").arg(
            dart_package_config
                .as_ref()
                .expect("Dart package config was prepared"),
        );
        command.args([
            "--lsp",
            "--client-id",
            "code-memory-language",
            "--client-version",
            "0.1",
        ]);
    }
    let mut child = command
        .current_dir(&analysis_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{} native LSP could not start: {e}", lang.name))?;
    let process_guard = ProviderProcessGuard::attach(&child);
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            process_guard.terminate(&mut child);
            return Err("native LSP stderr unavailable".to_string());
        }
    };
    forward_provider_stderr(server, stderr);
    let stdin = match child.stdin.take() {
        Some(stdin) => stdin,
        None => {
            process_guard.terminate(&mut child);
            return Err("native LSP stdin unavailable".to_string());
        }
    };
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            process_guard.terminate(&mut child);
            return Err("native LSP stdout unavailable".to_string());
        }
    };
    let mut connection = LspConnection::new(
        child,
        process_guard,
        stdin,
        BufReader::new(stdout),
        lsp_request_timeout(),
        analysis_files.len() > 500,
    )?;
    if server == "rust-analyzer" {
        connection.set_workspace_settings(rust_analyzer_settings());
    } else if server == "jdtls" {
        connection.set_workspace_settings(java_language_server_settings(java_source_only));
    }
    let initialization_started = Instant::now();
    trace_lsp_phase(server, "initialize", "start", initialization_started);
    connection.initialize(
        &path_to_uri(&analysis_root),
        &analysis_root.to_string_lossy(),
        lang.id,
    )?;
    connection.notify("initialized", serde_json::json!({}))?;
    configure_lsp_workspace(
        &mut connection,
        server,
        lang.id,
        &analysis_root,
        java_source_only,
    )?;
    trace_lsp_phase(server, "initialize", "finish", initialization_started);

    let mut index = Index::new();
    let mut document_indexes = HashMap::new();
    for file in &analysis_files {
        let relative = file
            .strip_prefix(root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        let mut document = scip::types::Document::new();
        document.language = lang.id.to_string();
        document.relative_path = relative.clone();
        document_indexes.insert(relative, index.documents.len());
        index.documents.push(document);
    }

    let reachable_headers = if server == "clangd" {
        reachable_project_headers(root, &analysis_files)
    } else {
        HashSet::new()
    };
    let mut seen_semantic_files = HashSet::new();
    let mut semantic_files: Vec<&PathBuf> = analysis_files
        .iter()
        .copied()
        // clangd needs a translation unit to establish flags and include
        // state. Header fragments are included by a TU; opening them as
        // standalone documents creates duplicate/invalid compiler contexts.
        .filter(|file| !(server == "clangd" && is_cpp_header_fragment(file)))
        .filter(|file| {
            server != "clangd" || !is_cpp_header(file) || reachable_headers.contains(*file)
        })
        .filter(|file| seen_semantic_files.insert(file.to_path_buf()))
        .collect();
    // ponytail: large workspaces keep declarations and imports, and skip the
    // most expensive reference/lexical passes. Map-boundary symbols still get
    // provider call/type queries; the ceiling is query breadth, not guessed
    // replacement edges.
    let file_large_workspace = large_workspace_workload(server, semantic_files.len(), 0);
    let dart_synthetic_package_map = dart_package_config
        .as_ref()
        .is_some_and(|path| path.ends_with("package_config.synthetic.json"));
    let mut source_cache = HashMap::<String, String>::new();
    let mut symbol_cache = HashMap::<String, Vec<LspSymbol>>::new();
    let mut document_symbol_files = HashSet::new();
    // ponytail: cap open documents for very large workspaces; the server still
    // reads the remaining files from disk for document requests. Raising this
    // is only needed if a provider version proves it needs editor buffers.
    let open_limit =
        lsp_open_document_limit(server, file_large_workspace, java_source_only);
    let mut partial_reason = None;
    let document_open_started = Instant::now();
    trace_lsp_phase(server, "document-open", "start", document_open_started);
    for (opened_index, file) in semantic_files.iter().enumerate() {
        let relative = file
            .strip_prefix(root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        let should_open = open_limit.is_none_or(|limit| opened_index < limit)
            && (server != "clangd" || !is_cpp_header(file) || reachable_headers.contains(*file));
        // clangd needs a translation unit to establish compiler flags. Opening
        // every header as a standalone document makes it parse with fake flags
        // and is both slow and misleading; headers are queried after active
        // translation units have been opened.
        if should_open {
            let text = fs::read_to_string(file).unwrap_or_default();
            source_cache.insert(relative.clone(), text);
            let uri = path_to_uri(file);
            let text = source_cache
                .get(&relative)
                .map(String::as_str)
                .unwrap_or_default();
            if let Err(error) =
                connection.did_open(&uri, lsp_language_id(server, file, lang.id), text)
            {
                if is_recoverable_lsp_session_error(&error) {
                    partial_reason = Some(error.clone());
                    connection.fatal_error = Some(error);
                    break;
                }
                return Err(error);
            }
            // Dart's analysis server can stop draining stdin while it queues a
            // large package graph. Throttle notifications before the pipe
            // buffer fills; this keeps the session deadline effective instead
            // of blocking inside write_all on large workspaces.
            if server == "dart" && file_large_workspace && (opened_index + 1) % 8 == 0 {
                connection.wait_for_retry(Duration::from_millis(50))?;
            }
        }
    }
    trace_lsp_phase(server, "document-open", "finish", document_open_started);
    // rust-analyzer can publish a complete syntax symbol tree before its
    // Cargo reload finishes. Waiting for that reload can replace the useful
    // tree with an empty/partial response, so start polling early and retain
    // the best response seen below.
    std::thread::sleep(Duration::from_millis(if lang.id == "rust" {
        1000
    } else {
        500
    }));

    let symbol_census_started = Instant::now();
    trace_lsp_phase(server, "symbol-census", "start", symbol_census_started);
    let semantic_file_count = semantic_files.len();
    let mut workspace_symbol_mode = false;
    if server == "jdtls" && file_large_workspace {
        match connection.workspace_symbols() {
            Ok(symbols) if !symbols.is_empty() => {
                for (uri, symbol) in symbols {
                    let relative = uri_to_relative_path(&uri, root);
                    if document_indexes.contains_key(&relative) {
                        document_symbol_files.insert(relative.clone());
                        symbol_cache.entry(relative).or_default().push(symbol);
                    }
                }
                workspace_symbol_mode = !symbol_cache.is_empty();
            }
            Ok(_) => {}
            Err(error) if is_recoverable_lsp_session_error(&error) => {
                partial_reason = Some(error.clone());
                connection.fatal_error.get_or_insert(error);
            }
            Err(error) => return Err(error),
        }
    }

    // Large non-Rust workspaces already use exactly one document-symbol query
    // per file. Pipeline those independent requests in bounded chunks instead
    // of paying one process/JSON-RPC round trip at a time. Responses are
    // restored to input order by LspConnection, so facts and evidence remain
    // byte-for-byte deterministic.
    if file_large_workspace
        && lang.id != "rust"
        && !workspace_symbol_mode
        && partial_reason.is_none()
    {
        let candidates = semantic_files
            .iter()
            .filter_map(|file| {
                let file = *file;
                if server == "clangd" && is_cpp_header(file) && !reachable_headers.contains(file) {
                    return None;
                }
                let relative = file
                    .strip_prefix(root)
                    .unwrap_or(file)
                    .to_string_lossy()
                    .replace('\\', "/");
                Some((relative, path_to_uri(file)))
            })
            .collect::<Vec<_>>();
        for chunk in candidates.chunks(lsp_request_batch_size()) {
            let uris = chunk
                .iter()
                .map(|(_, uri)| uri.clone())
                .collect::<Vec<_>>();
            let responses = match connection.document_symbols_batch(&uris) {
                Ok(responses) => responses,
                Err(error) if is_recoverable_lsp_session_error(&error) => {
                    partial_reason = Some(error.clone());
                    connection.fatal_error.get_or_insert(error);
                    break;
                }
                Err(error) => return Err(error),
            };
            for ((relative, _), response) in chunk.iter().zip(responses) {
                match response {
                    Ok(symbols) => {
                        document_symbol_files.insert(relative.clone());
                        symbol_cache.insert(relative.clone(), symbols);
                    }
                    Err(error) if is_recoverable_lsp_session_error(&error) => {
                        partial_reason = Some(error.clone());
                        connection.fatal_error.get_or_insert(error);
                        break;
                    }
                    Err(error) => return Err(error),
                }
            }
            if partial_reason.is_some() {
                break;
            }
        }
    }
    'document_symbols: for file in &semantic_files {
        let relative = file
            .strip_prefix(root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        if symbol_cache.contains_key(&relative) {
            continue;
        }
        if workspace_symbol_mode && document_symbol_files.contains(&relative) {
            continue;
        }
        if partial_reason.is_some() {
            break 'document_symbols;
        }
        if server == "clangd" && is_cpp_header(file) && !reachable_headers.contains(*file) {
            continue;
        }
        let uri = path_to_uri(file);
        let mut symbols: Vec<LspSymbol> = Vec::new();
        let retries = if file_large_workspace && lang.id != "rust" {
            1
        } else if lang.id == "rust" {
            // Small Cargo projects can publish a partial document-symbol tree
            // while rust-analyzer finishes macro and dependency loading.
            if semantic_files.len() <= 8 {
                // Tauri projects commonly need more than one Cargo reload
                // before callable symbols become queryable. Keep the retry
                // bounded by the normal LSP session deadline.
                60
            } else {
                6
            }
        } else {
            2
        };
        for attempt in 0..retries {
            match connection.document_symbols(&uri) {
                Ok(value) => {
                    document_symbol_files.insert(relative.clone());
                    let candidate_callable = value
                        .iter()
                        .filter(|symbol| is_callable_kind(symbol.kind))
                        .count();
                    let current_callable = symbols
                        .iter()
                        .filter(|symbol| is_callable_kind(symbol.kind))
                        .count();
                    if candidate_callable > current_callable
                        || (candidate_callable == current_callable && value.len() > symbols.len())
                    {
                        symbols = value;
                    }
                }
                Err(error) if is_recoverable_lsp_session_error(&error) => {
                    partial_reason = Some(error.clone());
                    connection.fatal_error.get_or_insert(error);
                    break 'document_symbols;
                }
                Err(error) => return Err(error),
            }
            let symbols_ready = !symbols.is_empty()
                && (lang.id != "rust"
                    || symbols.iter().any(|symbol| is_callable_kind(symbol.kind)));
            if symbols_ready || attempt + 1 == retries {
                break;
            }
            if connection
                .wait_for_retry(Duration::from_millis(500))
                .is_err()
            {
                break;
            }
        }
        symbol_cache.insert(relative, symbols);
    }
    trace_lsp_phase(server, "symbol-census", "finish", symbol_census_started);
    if partial_reason.is_some() {
        semantic_files.retain(|file| {
            let relative = file
                .strip_prefix(root)
                .unwrap_or(file)
                .to_string_lossy()
                .replace('\\', "/");
            document_symbol_files.contains(&relative)
        });
    }
    let semantic_query_symbols = symbol_cache
        .values()
        .flatten()
        .filter(|symbol| is_callable_kind(symbol.kind) || is_type_hierarchy_kind(symbol.kind))
        .count();
    if server == "jdtls" && !java_source_only && semantic_query_symbols == 0 {
        // The build-backed pass has already asked every scheduled document for
        // symbols. With no callable or type boundary, the later type/call
        // queries cannot create a usable Java definition and only spend the
        // entire request budget before `analysis.rs` chooses source fallback.
        // Empty the transient symbol cache now; the outer fail-closed fallback
        // remains the authority and no provider fact is synthesized here.
        semantic_files.clear();
        symbol_cache.clear();
    }
    let large_workspace = large_workspace_workload(
        server,
        semantic_files.len(),
        semantic_query_symbols,
    );
    if large_workspace && !file_large_workspace {
        connection.extend_session_for_large_workspace();
    }
    let large_map_enrichment = large_workspace
        && large_map_enrichment_language(lang.id)
        && !(server == "dart" && dart_synthetic_package_map);
    // Large-workspace call queries are restricted below to stable map-boundary
    // symbols. This makes the work set deterministic instead of processing
    // arbitrary symbols until the wall-clock deadline happens to expire.
    let large_call_enrichment = large_map_enrichment;
    let skip_large_workspace_type_enrichment = large_workspace && !large_map_enrichment;
    let skip_large_workspace_call_enrichment = large_workspace && !large_call_enrichment;
    // A cold language server can publish declarations before definition and
    // call-hierarchy indexes are ready. Retry only provider-resolvable local
    // names, under one small session-wide budget; never replace a missing
    // provider answer with a name-match edge.
    let semantic_prep_started = Instant::now();
    trace_lsp_phase(server, "semantic-prep", "start", semantic_prep_started);
    let mut lexical_definition_retry_budget = 6usize;
    for file in &semantic_files {
        let relative = file
            .strip_prefix(root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        source_cache
            .entry(relative)
            .or_insert_with(|| fs::read_to_string(file).unwrap_or_default());
    }
    trace_lsp_phase(server, "semantic-prep", "finish", semantic_prep_started);
    let enrichment_started = Instant::now();
    trace_lsp_phase(server, "enrichment", "start", enrichment_started);
    if server == "clangd"
        || (!large_workspace
            && lang.id == "rust"
            && symbol_cache
                .values()
                .any(|symbols| !symbols.iter().any(|symbol| is_callable_kind(symbol.kind))))
    {
        match connection.workspace_symbols() {
            Ok(symbols) => {
                for (uri, symbol) in symbols {
                    let relative = uri_to_relative_path(&uri, root);
                    if document_indexes.contains_key(&relative) {
                        symbol_cache.entry(relative).or_default().push(symbol);
                    }
                }
            }
            Err(error) if is_recoverable_lsp_session_error(&error) => {
                partial_reason.get_or_insert(error);
            }
            Err(error) => return Err(error),
        }
    }
    for file in &semantic_files {
        let relative = file
            .strip_prefix(root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        symbol_cache.entry(relative).or_default();
    }
    let mut repaired_java_symbol_selection_count = 0usize;
    let mut rejected_java_symbol_selection_count = 0usize;
    for (relative, symbols) in &mut symbol_cache {
        if lang.id == "java" {
            let source = source_cache.get(relative).map(String::as_str).unwrap_or_default();
            let (repaired, rejected) = repair_java_lsp_symbol_selections(source, symbols);
            repaired_java_symbol_selection_count += repaired;
            rejected_java_symbol_selection_count += rejected;
        }
        canonicalize_lsp_symbols(symbols);
        reconcile_lsp_symbol_owners(lang.id, symbols);
    }

    let known_symbol_names: HashSet<String> = symbol_cache
        .values()
        .flatten()
        .filter(|symbol| is_callable_or_type_kind(symbol.kind))
        .map(|symbol| symbol.name.clone())
        .collect();
    let callable_body_names: HashSet<String> = symbol_cache
        .iter()
        .flat_map(|(relative, symbols)| {
            let text = source_cache
                .get(relative)
                .map(String::as_str)
                .unwrap_or_default();
            symbols
                .iter()
                .filter(|symbol| is_callable_kind(symbol.kind) && has_callable_body(text, symbol))
                .map(|symbol| symbol.name.clone())
        })
        .collect();
    let local_call_target_names: HashSet<String> = symbol_cache
        .values()
        .flatten()
        .filter(|symbol| is_callable_or_type_kind(symbol.kind))
        .map(|symbol| lsp_symbol_base_name(&symbol.name).to_string())
        .filter(|name| !name.is_empty())
        .collect();

    let large_java_direct_call_mode = large_workspace && lang.id == "java";
    let mut call_syntax_by_file = HashMap::<String, Vec<SyntaxCallSite>>::new();
    let mut large_call_queries = Vec::<(String, u32, u32)>::new();
    let mut prefetched_large_call_queries = HashSet::<(String, u32, u32)>::new();
    let mut prefetched_large_definition_queries = HashSet::<(String, u32, u32)>::new();
    let mut large_call_syntax_site_count = 0usize;
    let mut large_call_owned_site_count = 0usize;
    let mut large_call_eligible_group_count = 0usize;
    if large_workspace && !skip_large_workspace_call_enrichment && partial_reason.is_none() {
        if large_java_direct_call_mode {
            let mut candidates = Vec::<LargeCallSiteQuery>::new();
            let mut groups = HashSet::new();
            for file in &semantic_files {
                let relative = file
                    .strip_prefix(root)
                    .unwrap_or(file)
                    .to_string_lossy()
                    .replace('\\', "/");
                let text = source_cache
                    .get(&relative)
                    .map(String::as_str)
                    .unwrap_or_default();
                let uri = path_to_uri(file);
                let sites = inventory_call_sites(lang.id, text)?;
                large_call_syntax_site_count += sites.len();
                if let Some(symbols) = symbol_cache.get(&relative) {
                    for site in &sites {
                        let Some(owner_range) = site.owner_name_utf16_range.as_deref() else {
                            continue;
                        };
                        let Some(owner) = find_lsp_symbol_at_range(symbols, owner_range)
                            .filter(|symbol| is_callable_kind(symbol.kind))
                        else {
                            continue;
                        };
                        let [line, character, ..] = site.callee_utf16_range.as_slice() else {
                            continue;
                        };
                        if *line < 0 || *character < 0 {
                            continue;
                        }
                        large_call_owned_site_count += 1;
                        if !matches!(site.callee_text.as_str(), "this" | "super")
                            && !local_call_target_names.contains(&site.callee_text)
                        {
                            continue;
                        }
                        groups.insert(large_call_query_group(&relative));
                        candidates.push(LargeCallSiteQuery {
                            priority: large_symbol_call_priority(
                                lang.id, &relative, text, owner,
                            ),
                            group: relative.clone(),
                            uri: uri.clone(),
                            line: *line as u32,
                            character: *character as u32,
                        });
                    }
                }
                call_syntax_by_file.insert(relative, sites);
            }
            large_call_eligible_group_count = groups.len();
            large_call_queries = fair_large_call_site_queries(candidates, usize::MAX);
        } else {
            let mut prioritized_call_queries = Vec::new();
            for file in &semantic_files {
                let relative = file
                    .strip_prefix(root)
                    .unwrap_or(file)
                    .to_string_lossy()
                    .replace('\\', "/");
                let text = source_cache
                    .get(&relative)
                    .map(String::as_str)
                    .unwrap_or_default();
                let uri = path_to_uri(file);
                if let Some(symbols) = symbol_cache.get(&relative) {
                    for symbol in symbols {
                        let symbol_semantic_enrichment = !large_map_enrichment
                            || large_symbol_is_map_boundary(lang.id, text, symbol);
                        if symbol_semantic_enrichment
                            && is_callable_kind(symbol.kind)
                            && (server != "clangd"
                                || has_callable_body(text, symbol)
                                || callable_body_names.contains(&symbol.name))
                        {
                            prioritized_call_queries.push((
                                large_symbol_call_priority(lang.id, &relative, text, symbol),
                                uri.clone(),
                                symbol.selection_line,
                                symbol.selection_character,
                            ));
                        }
                    }
                }
            }
            prioritized_call_queries.sort();
            prioritized_call_queries.dedup();
            large_call_queries.extend(
                prioritized_call_queries
                    .into_iter()
                    .map(|(_, uri, line, character)| (uri, line, character)),
            );
        }
    }

    // Keep an exact owner-indexed member table for provider-confirmed local
    // inheritance pairs. Python uses it because servers do not consistently
    // implement member override lookup. Large Java workspaces use it only for
    // signature-identical members with an explicit @Override annotation; a
    // provider-wide implementation search can rebuild the full JDT hierarchy
    // for a single method. This is deliberately not a repository-wide name
    // match: both owners and both member definitions are exact local symbols.
    let large_java_local_override_mode = large_workspace && lang.id == "java";
    let callable_members_by_owner = symbol_cache
        .iter()
        .flat_map(|(relative, symbols)| {
            let text = source_cache
                .get(relative)
                .map(String::as_str)
                .unwrap_or_default();
            let source_lines = large_java_local_override_mode
                .then(|| text.lines().collect::<Vec<_>>());
            symbols
                .iter()
                .filter_map(|symbol| {
                    let parent = symbol.parent.as_ref()?;
                    is_callable_kind(symbol.kind).then(|| {
                        (
                            symbol_string(
                                relative,
                                &parent.name,
                                parent.selection_line,
                                parent.selection_character,
                            ),
                            (
                                symbol.name.clone(),
                                symbol_string(
                                    relative,
                                    &symbol.name,
                                    symbol.selection_line,
                                    symbol.selection_character,
                                ),
                                source_lines.as_deref().is_some_and(|lines| {
                                    java_explicit_override_annotation_in_lines(lines, symbol)
                                }),
                            ),
                        )
                    })
                })
                .collect::<Vec<_>>()
        })
        .fold(
            HashMap::<String, Vec<(String, String, bool)>>::new(),
            |mut members, (owner, member)| {
                members.entry(owner).or_default().push(member);
                members
            },
        );

    let mut explicit_hierarchy_target_names = HashSet::<String>::new();
    let mut type_syntax_by_file = HashMap::<String, SyntaxTypeInventory>::new();
    let mut type_syntax_failures = Vec::<(String, String)>::new();
    // Keep the cold definition-query denominator visible by semantic purpose.
    // A total request count alone cannot tell us whether a provider is slow or
    // whether the bridge is asking a question whose exact answer it already
    // owns in the document-symbol cache.
    let mut type_relation_source_queries = HashSet::<(String, u32, u32)>::new();
    let mut type_relation_target_queries = HashSet::<(String, u32, u32)>::new();
    let mut type_use_target_queries = HashSet::<(String, u32, u32)>::new();
    let mut locally_owned_type_relation_sources = HashSet::<(String, u32, u32)>::new();
    let mut type_definition_query_count = 0usize;
    let mut type_definition_unique_query_count = 0usize;
    let mut call_definition_query_count = 0usize;
    let mut call_definition_unique_query_count = 0usize;
    let local_type_definition_count = symbol_cache
        .values()
        .flatten()
        .filter(|symbol| is_type_hierarchy_kind(symbol.kind))
        .count();
    let mut accepted_type_relation_site_count = 0usize;
    let mut accepted_type_use_site_count = 0usize;
    let mut accepted_large_direct_call_site_count = 0usize;
    let mut unresolved_large_direct_call_site_count = 0usize;
    let mut ambiguous_large_direct_call_site_count = 0usize;
    if !skip_large_workspace_type_enrichment {
        for file in &semantic_files {
            let relative = file
                .strip_prefix(root)
                .unwrap_or(file)
                .to_string_lossy()
                .replace('\\', "/");
            let text = source_cache
                .get(&relative)
                .map(String::as_str)
                .unwrap_or_default();
            let type_syntax = match inventory_type_syntax(
                lang.contract_language,
                &relative,
                text,
            ) {
                Ok(inventory) => inventory,
                Err(error) => {
                    // This parser is an auxiliary exactness check for type
                    // enrichment, not the language provider itself. A grammar
                    // gap must downgrade that capability without discarding
                    // provider-confirmed definitions, calls, imports, or later
                    // framework facts for the whole file.
                    type_syntax_failures.push((relative, error));
                    continue;
                }
            };
            for site in &type_syntax.relations {
                explicit_hierarchy_target_names.insert(site.target_name.clone());
            }
            type_syntax_by_file.insert(relative, type_syntax);
        }
    }
    if !skip_large_workspace_type_enrichment && partial_reason.is_none() {
        let mut hierarchy_definition_queries = Vec::new();
        let mut type_use_definition_queries = Vec::new();
        for file in &semantic_files {
            let relative = file
                .strip_prefix(root)
                .unwrap_or(file)
                .to_string_lossy()
                .replace('\\', "/");
            let Some(type_syntax) = type_syntax_by_file.get(&relative) else {
                continue;
            };
            let uri = path_to_uri(file);
            let symbols = symbol_cache.get(&relative).map(Vec::as_slice).unwrap_or(&[]);
            let add_range = |
                range: &[i32],
                category: &mut HashSet<(String, u32, u32)>,
                queries: &mut Vec<(String, u32, u32)>,
            | {
                if let [line, character, ..] = range {
                    if *line >= 0 && *character >= 0 {
                        let query = (uri.clone(), *line as u32, *character as u32);
                        category.insert(query.clone());
                        queries.push(query);
                    }
                }
            };
            for site in &type_syntax.relations {
                if let [line, character, ..] = site.source_utf16_range.as_slice() {
                    if *line >= 0 && *character >= 0 {
                        type_relation_source_queries.insert((
                            uri.clone(),
                            *line as u32,
                            *character as u32,
                        ));
                    }
                }
                if find_lsp_symbol_at_range(symbols, &site.source_utf16_range)
                    .is_some_and(|symbol| is_type_hierarchy_kind(symbol.kind))
                {
                    if let [line, character, ..] = site.source_utf16_range.as_slice() {
                        if *line >= 0 && *character >= 0 {
                            locally_owned_type_relation_sources.insert((
                                uri.clone(),
                                *line as u32,
                                *character as u32,
                            ));
                        }
                    }
                }
                add_range(
                    &site.target_utf16_range,
                    &mut type_relation_target_queries,
                    &mut hierarchy_definition_queries,
                );
            }
            for site in &type_syntax.uses {
                add_range(
                    &site.target_utf16_range,
                    &mut type_use_target_queries,
                    &mut type_use_definition_queries,
                );
            }
        }
        type_definition_query_count =
            hierarchy_definition_queries.len() + type_use_definition_queries.len();
        let mut all_unique_queries = HashSet::new();
        all_unique_queries.extend(hierarchy_definition_queries.iter().cloned());
        all_unique_queries.extend(type_use_definition_queries.iter().cloned());
        type_definition_unique_query_count = all_unique_queries.len();

        let mut seen = HashSet::new();
        hierarchy_definition_queries.retain(|query| seen.insert(query.clone()));
        seen.clear();
        type_use_definition_queries.retain(|query| seen.insert(query.clone()));

        if large_workspace {
            // The product's first question is execution flow. Complete a
            // deterministic production/public call slice before type lookups;
            // the previous hierarchy-first order spent the entire Java session
            // on empty definition retries and emitted zero calls.
            let call_limit = if large_java_direct_call_mode {
                let reserved_type_requests = hierarchy_definition_queries
                    .len()
                    .saturating_add(type_use_definition_queries.len().min(2_048))
                    .saturating_add(1_024);
                connection
                    .remaining_request_budget()
                    .saturating_sub(reserved_type_requests)
                    .min(large_call_queries.len())
            } else {
                large_call_queries
                    .len()
                    .min(semantic_files.len().saturating_mul(2))
                    .min(4_096)
                    .min(connection.remaining_request_budget() / 4)
            };
            let selected_calls = &large_call_queries[..call_limit];
            prefetched_large_call_queries.extend(selected_calls.iter().cloned());
            if large_java_direct_call_mode {
                call_definition_query_count = large_call_queries.len();
                call_definition_unique_query_count = large_call_queries.len();
                connection.prefetch_definitions_once(selected_calls, lsp_request_batch_size());
            } else {
                connection.prefetch_outgoing_calls(
                    selected_calls,
                    root,
                    lsp_request_batch_size(),
                );
            }

            // The source side of extends/implements is already an exact local
            // document symbol. Ask only for the target side, once, after the
            // full document-symbol census. Repeating unresolved positions three
            // times cannot improve a source-only Java project and previously
            // tripled Spring's cold latency.
            let hierarchy_limit = hierarchy_definition_queries
                .len()
                .min(connection.remaining_request_budget() / 2);
            let selected_hierarchy_queries = &hierarchy_definition_queries
                [..hierarchy_definition_queries.len().min(hierarchy_limit)];
            prefetched_large_definition_queries
                .extend(selected_hierarchy_queries.iter().cloned());
            connection.prefetch_definitions_once(
                selected_hierarchy_queries,
                lsp_request_batch_size(),
            );

            // Declaration type uses are useful detail, but never allowed to
            // displace flow or hierarchy facts on a large repository.
            let type_use_limit = type_use_definition_queries
                .len()
                .min(2_048)
                .min(connection.remaining_request_budget() / 2);
            let selected_type_use_queries =
                &type_use_definition_queries[..type_use_definition_queries.len().min(type_use_limit)];
            prefetched_large_definition_queries
                .extend(selected_type_use_queries.iter().cloned());
            connection.prefetch_definitions_once(
                selected_type_use_queries,
                lsp_request_batch_size(),
            );
        } else {
            hierarchy_definition_queries.extend(type_use_definition_queries);
            connection.prefetch_definitions(
                &hierarchy_definition_queries,
                lsp_request_batch_size(),
            );
        }
    }
    if !large_workspace && partial_reason.is_none() {
        let mut call_definition_queries = Vec::new();
        for file in &semantic_files {
            let relative = file
                .strip_prefix(root)
                .unwrap_or(file)
                .to_string_lossy()
                .replace('\\', "/");
            let text = source_cache
                .get(&relative)
                .map(String::as_str)
                .unwrap_or_default();
            let symbols = symbol_cache.get(&relative).map(Vec::as_slice).unwrap_or(&[]);
            let syntax_call_sites = inventory_call_sites(lang.id, text)?;
            let syntax_inventory_available = !syntax_call_sites.is_empty()
                || matches!(lang.id, "csharp" | "c" | "cpp" | "go" | "rust");
            let positions = if syntax_inventory_available {
                syntax_call_sites
                    .iter()
                    .filter_map(|site| match site.callee_utf16_range.as_slice() {
                        [line, character, ..] if *line >= 0 && *character >= 0 => {
                            Some((*line as u32, *character as u32))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            } else if server != "clangd" {
                lexical_call_candidates_with_set(text, symbols, &known_symbol_names)
                    .into_iter()
                    .map(|(line, character, _)| (line, character))
                    .collect()
            } else {
                Vec::new()
            };
            let uri = path_to_uri(file);
            call_definition_queries.extend(
                positions
                    .into_iter()
                    .map(|(line, character)| (uri.clone(), line, character)),
            );
        }
        connection.prefetch_definitions(
            &call_definition_queries,
            lsp_request_batch_size(),
        );
        call_definition_query_count = call_definition_queries.len();
        call_definition_unique_query_count = call_definition_queries
            .iter()
            .collect::<HashSet<_>>()
            .len();
    }
    let mut explicit_member_relationships = HashMap::<String, Vec<String>>::new();
    if !explicit_hierarchy_target_names.is_empty() && !large_java_local_override_mode {
        for (relative, symbols) in &symbol_cache {
            let uri = path_to_uri(&root.join(relative));
            for symbol in symbols.iter().filter(|symbol| {
                is_callable_kind(symbol.kind)
                    && symbol.parent.as_ref().is_some_and(|parent| {
                        explicit_hierarchy_target_names.contains(&parent.name)
                    })
            }) {
                let base_symbol = symbol_string(
                    relative,
                    &symbol.name,
                    symbol.selection_line,
                    symbol.selection_character,
                );
                for implementation in connection.implementations(
                    &uri,
                    symbol.selection_line,
                    symbol.selection_character,
                ) {
                    let Some(implementation_symbol) = local_callable_symbol_from_lsp_item(
                        &implementation,
                        root,
                        &document_indexes,
                        &symbol_cache,
                    ) else {
                        continue;
                    };
                    if implementation_symbol != base_symbol {
                        explicit_member_relationships
                            .entry(implementation_symbol)
                            .or_default()
                            .push(base_symbol.clone());
                    }
                }
            }
        }
        for targets in explicit_member_relationships.values_mut() {
            targets.sort();
            targets.dedup();
        }
    }

    for file in &semantic_files {
        let relative = file
            .strip_prefix(root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        let text = source_cache
            .get(&relative)
            .map(String::as_str)
            .unwrap_or_default();
        let uri = path_to_uri(file);
        let symbols = symbol_cache
            .get(&relative)
            .ok_or_else(|| format!("missing symbols for {}", relative))?;
        let computed_syntax_call_sites;
        let syntax_call_sites: &[SyntaxCallSite] =
            if let Some(cached) = call_syntax_by_file.get(&relative) {
                cached.as_slice()
            } else {
                computed_syntax_call_sites = inventory_call_sites(lang.id, text)?;
                computed_syntax_call_sites.as_slice()
            };
        let mut explicit_type_relationships = HashMap::<String, Vec<String>>::new();
        let mut explicit_type_occurrences = Vec::<(String, Vec<i32>)>::new();
        let mut explicit_type_use_occurrences =
            Vec::<(String, Vec<i32>, Vec<i32>)>::new();
        if !skip_large_workspace_type_enrichment {
            if let Some(type_syntax) = type_syntax_by_file.get(&relative) {
                let type_resolution_context = LocalTypeResolutionContext {
                    current_relative: &relative,
                    current_symbols: symbols,
                    root,
                    document_indexes: &document_indexes,
                    symbol_cache: &symbol_cache,
                };
                for site in &type_syntax.relations {
                let source_symbols = local_declared_type_symbols_at(
                    &site.source_utf16_range,
                    &type_resolution_context,
                );
                let target_symbols = if type_resolution_query_is_planned(
                    large_workspace,
                    &prefetched_large_definition_queries,
                    &uri,
                    &site.target_utf16_range,
                ) {
                    resolved_local_type_symbols_at(
                        &mut connection,
                        &uri,
                        &site.target_utf16_range,
                        &type_resolution_context,
                    )
                } else {
                    Vec::new()
                };
                let ([source_symbol], [target_symbol]) =
                    (source_symbols.as_slice(), target_symbols.as_slice())
                else {
                    continue;
                };
                if source_symbol != target_symbol {
                    accepted_type_relation_site_count += 1;
                    explicit_type_relationships
                        .entry(source_symbol.clone())
                        .or_default()
                        .push(target_symbol.clone());
                    explicit_type_occurrences
                        .push((source_symbol.clone(), site.source_utf16_range.clone()));
                    explicit_type_occurrences
                        .push((target_symbol.clone(), site.target_utf16_range.clone()));
                }
            }
                for site in &type_syntax.uses {
                let Some(source) = find_lsp_symbol_at_range(
                    symbols,
                    site.source_name_range(ProviderProtocol::LanguageServerProtocol),
                ) else {
                    continue;
                };
                let target_symbols = if type_resolution_query_is_planned(
                    large_workspace,
                    &prefetched_large_definition_queries,
                    &uri,
                    &site.target_utf16_range,
                ) {
                    resolved_local_type_symbols_at(
                        &mut connection,
                        &uri,
                        &site.target_utf16_range,
                        &type_resolution_context,
                    )
                } else {
                    Vec::new()
                };
                let [target_symbol] = target_symbols.as_slice() else {
                    continue;
                };
                let source_symbol = symbol_string(
                    &relative,
                    &source.name,
                    source.selection_line,
                    source.selection_character,
                );
                if source_symbol != *target_symbol {
                    accepted_type_use_site_count += 1;
                    explicit_type_use_occurrences.push((
                        target_symbol.clone(),
                        site.target_utf16_range.clone(),
                        site.source_declaration_range(ProviderProtocol::LanguageServerProtocol)
                            .to_vec(),
                    ));
                }
            }
                for targets in explicit_type_relationships.values_mut() {
                targets.sort();
                targets.dedup();
            }
                if lang.id == "python" || large_java_local_override_mode {
                for (source_owner, target_owners) in &explicit_type_relationships {
                    let Some(source_members) = callable_members_by_owner.get(source_owner) else {
                        continue;
                    };
                    for target_owner in target_owners {
                        let Some(target_members) = callable_members_by_owner.get(target_owner)
                        else {
                            continue;
                        };
                        for (source_name, source_symbol, source_has_java_override) in source_members {
                            for (target_name, target_symbol, _) in target_members {
                                if source_name == target_name
                                    && source_symbol != target_symbol
                                    && (if lang.id == "python" {
                                        !python_private_member_name(source_name)
                                    } else {
                                        *source_has_java_override
                                    })
                                {
                                    explicit_member_relationships
                                        .entry(source_symbol.clone())
                                        .or_default()
                                        .push(target_symbol.clone());
                                }
                            }
                        }
                    }
                }
                    for targets in explicit_member_relationships.values_mut() {
                        targets.sort();
                        targets.dedup();
                    }
                }
            }
        }
        let mut resolved_syntax_ranges = Vec::<Vec<i32>>::new();
        let source_index = *document_indexes
            .get(&relative)
            .ok_or_else(|| format!("missing document for {}", relative))?;
        explicit_type_occurrences.sort();
        explicit_type_occurrences.dedup();
        for (symbol, range) in explicit_type_occurrences {
            let mut occurrence = scip::types::Occurrence::new();
            occurrence.symbol = symbol;
            occurrence.range = range;
            index.documents[source_index].occurrences.push(occurrence);
        }
        explicit_type_use_occurrences.sort();
        explicit_type_use_occurrences.dedup();
        for (symbol, range, enclosing_range) in explicit_type_use_occurrences {
            let mut occurrence = scip::types::Occurrence::new();
            occurrence.symbol = symbol;
            occurrence.range = range;
            occurrence.enclosing_range = enclosing_range;
            index.documents[source_index].occurrences.push(occurrence);
        }
        for symbol in symbols {
            let symbol_semantic_enrichment = !large_workspace
                || !large_map_enrichment
                || large_symbol_is_map_boundary(lang.id, text, symbol);
            let symbol_id = symbol_string(
                &relative,
                &symbol.name,
                symbol.selection_line,
                symbol.selection_character,
            );
            let mut occurrence = scip::types::Occurrence::new();
            occurrence.symbol = symbol_id.clone();
            occurrence.symbol_roles = scip::types::SymbolRole::Definition as i32;
            let definition_name = if lang.id == "java" && is_callable_or_type_kind(symbol.kind) {
                lsp_symbol_base_name(&symbol.name)
            } else {
                &symbol.name
            };
            occurrence.range = vec![
                symbol.selection_line as i32,
                symbol.selection_character as i32,
                symbol.selection_line as i32,
                (symbol.selection_character + definition_name.encode_utf16().count() as u32) as i32,
            ];
            occurrence.enclosing_range = vec![
                symbol.range_start_line as i32,
                symbol.range_start_character as i32,
                symbol.range_end_line as i32,
                symbol.range_end_character as i32,
            ];
            index.documents[source_index].occurrences.push(occurrence);

            let mut information = scip::types::SymbolInformation::new();
            information.symbol = symbol_id.clone();
            information.kind = lsp_kind_to_scip(symbol.kind).into();
            if let Some(parent) = &symbol.parent {
                information.enclosing_symbol = symbol_string(
                    &relative,
                    &parent.name,
                    parent.selection_line,
                    parent.selection_character,
                );
            }
            if let Some(detail) = &symbol.detail {
                information.documentation.push(detail.clone());
                let mut signature = scip::types::Signature::new();
                signature.language = lang.id.to_string();
                signature.text = detail.clone();
                information.signature_documentation = protobuf::MessageField::some(signature);
            }
            if !(skip_large_workspace_type_enrichment || large_workspace && lang.id == "java")
                && symbol_semantic_enrichment
                && is_type_hierarchy_kind(symbol.kind)
            {
                for target in connection.type_definitions(
                    &uri,
                    symbol.selection_line,
                    symbol.selection_character,
                ) {
                    if let Some(target_symbol) = lsp_item_symbol(&target, root) {
                        if target_symbol != symbol_id {
                            let mut relationship = scip::types::Relationship::new();
                            relationship.symbol = target_symbol;
                            relationship.is_type_definition = true;
                            information.relationships.push(relationship);
                        }
                    }
                }
            }
            if !(skip_large_workspace_type_enrichment || large_workspace && lang.id == "java")
                && matches!(
                    lang.id,
                    "java" | "csharp" | "dart" | "python" | "c" | "cpp" | "go" | "rust"
                )
                && is_type_hierarchy_kind(symbol.kind)
            {
                for target in
                    connection.supertypes(&uri, symbol.selection_line, symbol.selection_character)
                {
                    if let Some(target_symbol) = lsp_item_symbol(&target, root) {
                        if target_symbol != symbol_id {
                            let mut relationship = scip::types::Relationship::new();
                            relationship.symbol = target_symbol;
                            relationship.is_implementation = true;
                            information.relationships.push(relationship);
                        }
                    }
                }
            }
            if let Some(targets) = explicit_type_relationships.get(&symbol_id) {
                for target_symbol in targets {
                    let mut relationship = scip::types::Relationship::new();
                    relationship.symbol = target_symbol.clone();
                    relationship.is_implementation = true;
                    information.relationships.push(relationship);
                }
            }
            if let Some(targets) = explicit_member_relationships.get(&symbol_id) {
                for target_symbol in targets {
                    let mut relationship = scip::types::Relationship::new();
                    relationship.symbol = target_symbol.clone();
                    relationship.is_implementation = true;
                    information.relationships.push(relationship);
                }
            }
            if lang.id == "cpp"
                && is_callable_kind(symbol.kind)
                && text
                    .lines()
                    .nth(symbol.selection_line as usize)
                    .is_some_and(|line| line.contains("virtual") || line.contains("override"))
            {
                for target in connection.implementations(
                    &uri,
                    symbol.selection_line,
                    symbol.selection_character,
                ) {
                    if let Some(target_symbol) = lsp_item_symbol(&target, root) {
                        if target_symbol != symbol_id {
                            let mut relationship = scip::types::Relationship::new();
                            relationship.symbol = target_symbol;
                            relationship.is_implementation = true;
                            information.relationships.push(relationship);
                        }
                    }
                }
            }
            if server == "clangd" && !large_workspace && is_cpp_header(file) {
                for (target_uri, target_range) in connection.definitions_at(
                    &uri,
                    symbol.selection_line,
                    symbol.selection_character,
                ) {
                    let Some(target_uri) = target_uri else {
                        continue;
                    };
                    let target_relative = uri_to_relative_path(&target_uri, root);
                    let Some(target_symbols) = symbol_cache.get(&target_relative) else {
                        continue;
                    };
                    let Some(target) = find_lsp_symbol_at_range(target_symbols, &target_range)
                    else {
                        continue;
                    };
                    let target_symbol = symbol_string(
                        &target_relative,
                        &target.name,
                        target.selection_line,
                        target.selection_character,
                    );
                    if target_symbol != symbol_id {
                        let mut relationship = scip::types::Relationship::new();
                        relationship.symbol = target_symbol;
                        relationship.is_definition = true;
                        information.relationships.push(relationship);
                    }
                }
            }
            index.documents[source_index].symbols.push(information);

            // ponytail: clangd's call hierarchy supplies callable edges; only
            // Type references are retained for the desktop Fact Graph type layer.
            // Call hierarchy and lexical definition queries already produce
            // the flow edges the desktop needs. Per-callable reference queries
            // are an expensive enrichment pass and commonly duplicate those
            // edges. Keep them opt-in for diagnostics/debugging.
            let query_references = !large_workspace
                && (server != "clangd"
                    && lsp_reference_enrichment_enabled(lang.id)
                    && is_callable_kind(symbol.kind))
                || (server == "clangd" && !large_workspace && is_type_hierarchy_kind(symbol.kind));
            let mut references = if query_references {
                connection.references(&uri, symbol.selection_line, symbol.selection_character)?
            } else {
                Vec::new()
            };
            if server != "clangd" && query_references && references.is_empty() {
                if let Some(line) = text.lines().nth(symbol.selection_line as usize) {
                    if let Some(character) = line.find(&symbol.name) {
                        if character as u32 != symbol.selection_character {
                            references = connection.references(
                                &uri,
                                symbol.selection_line,
                                character as u32,
                            )?;
                        }
                    }
                }
            }
            for reference in references {
                let reference_relative = uri_to_relative_path(&reference.uri, root);
                if let Some(target_index) = document_indexes.get(&reference_relative).copied() {
                    let mut ref_occurrence = scip::types::Occurrence::new();
                    ref_occurrence.symbol = symbol_id.clone();
                    ref_occurrence.range = reference.range;
                    if let Some(owner_range) = find_enclosing_symbol_range(
                        symbol_cache.get(&reference_relative),
                        &ref_occurrence.range,
                    ) {
                        ref_occurrence.enclosing_range = owner_range;
                    }
                    index.documents[target_index]
                        .occurrences
                        .push(ref_occurrence);
                }
            }

            let call_query = (uri.clone(), symbol.selection_line, symbol.selection_character);
            let outgoing_calls = if !skip_large_workspace_call_enrichment
                && !large_java_direct_call_mode
                && symbol_semantic_enrichment
                && is_callable_kind(symbol.kind)
                && (!large_workspace || prefetched_large_call_queries.contains(&call_query))
                && (server != "clangd"
                    || has_callable_body(text, symbol)
                    || callable_body_names.contains(&symbol.name))
            {
                connection.outgoing_calls(
                    &uri,
                    symbol.selection_line,
                    symbol.selection_character,
                    root,
                )
            } else {
                Vec::new()
            };
            for (target_symbol, target_relative, range) in outgoing_calls {
                if document_indexes.contains_key(&target_relative) {
                    resolved_syntax_ranges.push(range.clone());
                    let mut call_occurrence = scip::types::Occurrence::new();
                    call_occurrence.symbol = target_symbol;
                    call_occurrence.range = range;
                    call_occurrence.enclosing_range = vec![
                        symbol.range_start_line as i32,
                        symbol.range_start_character as i32,
                        symbol.range_end_line as i32,
                        symbol.range_end_character as i32,
                    ];
                    index.documents[source_index]
                        .occurrences
                        .push(call_occurrence);
                }
            }
        }

        if large_java_direct_call_mode {
            for site in syntax_call_sites {
                let [line, character, ..] = site.callee_utf16_range.as_slice() else {
                    continue;
                };
                if *line < 0 || *character < 0 {
                    continue;
                }
                let query = (uri.clone(), *line as u32, *character as u32);
                if !prefetched_large_call_queries.contains(&query) {
                    continue;
                }
                let Some(owner_range) = site.owner_name_utf16_range.as_deref() else {
                    unresolved_large_direct_call_site_count += 1;
                    continue;
                };
                let Some(owner) = find_lsp_symbol_at_range(symbols, owner_range)
                    .filter(|symbol| is_callable_kind(symbol.kind))
                else {
                    unresolved_large_direct_call_site_count += 1;
                    continue;
                };
                let mut targets = connection
                    .definitions_at(&uri, *line as u32, *character as u32)
                    .into_iter()
                    .filter_map(|(target_uri, target_range)| {
                        let target_uri = target_uri?;
                        let target_relative = uri_to_relative_path(&target_uri, root);
                        if !document_indexes.contains_key(&target_relative) {
                            return None;
                        }
                        let target = symbol_cache
                            .get(&target_relative)
                            .and_then(|symbols| find_lsp_symbol_at_range(symbols, &target_range))?;
                        let valid_target = match site.form {
                            CallSiteForm::Construct => {
                                is_callable_kind(target.kind)
                                    || is_type_hierarchy_kind(target.kind)
                            }
                            CallSiteForm::Call | CallSiteForm::MethodCall => {
                                is_callable_kind(target.kind)
                            }
                        };
                        valid_target.then(|| {
                            symbol_string(
                                &target_relative,
                                &target.name,
                                target.selection_line,
                                target.selection_character,
                            )
                        })
                    })
                    .collect::<Vec<_>>();
                targets.sort();
                targets.dedup();
                let [target_symbol] = targets.as_slice() else {
                    if targets.len() > 1 {
                        ambiguous_large_direct_call_site_count += 1;
                    } else {
                        unresolved_large_direct_call_site_count += 1;
                    }
                    continue;
                };
                let mut occurrence = scip::types::Occurrence::new();
                occurrence.symbol = target_symbol.clone();
                occurrence.range = site.callee_utf16_range.clone();
                occurrence.enclosing_range = vec![
                    owner.range_start_line as i32,
                    owner.range_start_character as i32,
                    owner.range_end_line as i32,
                    owner.range_end_character as i32,
                ];
                index.documents[source_index].occurrences.push(occurrence);
                accepted_large_direct_call_site_count += 1;
            }
        } else if !large_workspace {
            let syntax_inventory_available = !syntax_call_sites.is_empty()
                || matches!(lang.id, "csharp" | "c" | "cpp" | "go" | "rust");
            let lexical_candidates = if syntax_inventory_available {
                syntax_call_sites
                    .iter()
                    .filter(|site| {
                        !resolved_syntax_ranges
                            .iter()
                            .any(|range| site.matches_provider_range(range))
                    })
                    .filter_map(|site| match site.callee_utf16_range.as_slice() {
                        [line, character, _, _] if *line >= 0 && *character >= 0 => Some((
                            *line as u32,
                            *character as u32,
                            site.callee_text.clone(),
                        )),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            } else if server != "clangd" {
                lexical_call_candidates_with_set(text, symbols, &known_symbol_names)
            } else {
                Vec::new()
            };
            if connection.fatal_error.is_some() {
                continue;
            }
            let mut queried_positions = HashSet::new();
            for (line, character, name) in lexical_candidates {
                if !queried_positions.insert((line, character, name.clone())) {
                    continue;
                }
                let call_range = vec![
                    line as i32,
                    character as i32,
                    line as i32,
                    (character + name.encode_utf16().count() as u32) as i32,
                ];
                let Some(owner_range) = find_enclosing_symbol_range(Some(symbols), &call_range)
                else {
                    continue;
                };
                let mut definitions = connection.definitions_at(&uri, line, character);
                while definitions.is_empty()
                    && lexical_definition_retry_budget > 0
                    && connection.fatal_error.is_none()
                {
                    lexical_definition_retry_budget -= 1;
                    if connection
                        .wait_for_retry(Duration::from_millis(500))
                        .is_err()
                    {
                        break;
                    }
                    definitions = connection.definitions_at(&uri, line, character);
                }
                for (target_uri, target_range) in definitions {
                    let Some(target_uri) = target_uri else {
                        continue;
                    };
                    let target_relative = uri_to_relative_path(&target_uri, root);
                    let Some(target_symbols) = symbol_cache.get(&target_relative) else {
                        continue;
                    };
                    let Some(target) = find_lsp_symbol_at_range(target_symbols, &target_range)
                    else {
                        continue;
                    };
                    if !document_indexes.contains_key(&target_relative) {
                        continue;
                    }
                    let mut call_occurrence = scip::types::Occurrence::new();
                    call_occurrence.symbol = symbol_string(
                        &target_relative,
                        &target.name,
                        target.selection_line,
                        target.selection_character,
                    );
                    call_occurrence.range = call_range.clone();
                    call_occurrence.enclosing_range = owner_range.clone();
                    index.documents[source_index]
                        .occurrences
                        .push(call_occurrence);
                }
            }
        }
    }
    trace_lsp_phase(server, "enrichment", "finish", enrichment_started);
    if partial_reason.is_none() {
        partial_reason = connection.fatal_error.clone();
        if partial_reason.is_some() && !workspace_symbol_mode {
            semantic_files.retain(|file| {
                let relative = file
                    .strip_prefix(root)
                    .unwrap_or(file)
                    .to_string_lossy()
                    .replace('\\', "/");
                document_symbol_files.contains(&relative)
            });
        }
    }
    if lang.id == "rust" && partial_reason.is_none() {
        // rust-analyzer may still finish sysroot loading after the last LSP
        // response. Let that worker settle before shutdown to avoid its
        // shutdown-time SendError panic.
        connection.wait_for_retry(Duration::from_secs(2))?;
    }
    let shutdown_started = Instant::now();
    trace_lsp_phase(server, "shutdown", "start", shutdown_started);
    if server == "rust-analyzer" {
        // rust-analyzer can still have Cargo reload workers alive after the
        // last response. A shutdown request closes that worker channel first
        // and makes the bundled provider print a SendError panic. The result
        // is complete already, so use the protocol exit notification and let
        // Drop reap the process without that race.
        if partial_reason.is_none() {
            connection.notify("exit", Value::Null)?;
        } else {
            let _ = connection.notify("exit", Value::Null);
        }
    } else if let Err(error) = connection.shutdown() {
        if partial_reason.is_none() && !is_recoverable_lsp_session_error(&error) {
            return Err(error);
        }
    }
    trace_lsp_phase(server, "shutdown", "finish", shutdown_started);
    if partial_reason.is_none() {
        connection.ensure_healthy()?;
    }
    eprintln!(
        "@codebase-workspace-lsp-performance {}",
        serde_json::json!({
            "language": lang.id,
            "server": server,
            "files": analysis_files.len(),
            "elapsedMs": provider_started.elapsed().as_millis(),
            "requests": connection.request_performance_summary(),
            "definitionPlan": {
                "typeRelationSourceUnique": type_relation_source_queries.len(),
                "typeRelationSourceLocallyOwned": locally_owned_type_relation_sources.len(),
                "typeRelationTargetUnique": type_relation_target_queries.len(),
                "typeUseTargetUnique": type_use_target_queries.len(),
                "typeInput": type_definition_query_count,
                "typeUnique": type_definition_unique_query_count,
                "callInput": call_definition_query_count,
                "callUnique": call_definition_unique_query_count,
                "largeCallEligible": large_call_queries.len(),
                "largeCallSelected": prefetched_large_call_queries.len(),
                "largeCallSyntaxSites": large_call_syntax_site_count,
                "largeCallOwnedSites": large_call_owned_site_count,
                "largeCallEligibleGroups": large_call_eligible_group_count,
                "largeCallSelectedGroups": prefetched_large_call_queries
                    .iter()
                    .map(|(uri, _, _)| large_call_query_group(&uri_to_relative_path(uri, root)))
                    .collect::<HashSet<_>>()
                    .len(),
                "largeDirectCallAccepted": accepted_large_direct_call_site_count,
                "largeDirectCallUnresolved": unresolved_large_direct_call_site_count,
                "largeDirectCallAmbiguous": ambiguous_large_direct_call_site_count,
                "largeDirectCallMode": large_java_direct_call_mode,
                "largeDefinitionSelected": prefetched_large_definition_queries.len(),
                "localTypeDefinitions": local_type_definition_count,
                "acceptedTypeRelationSites": accepted_type_relation_site_count,
                "acceptedTypeUseSites": accepted_type_use_site_count,
                "largeJavaLocalOverrideMode": large_java_local_override_mode,
                "repairedMalformedJavaSymbols": repaired_java_symbol_selection_count,
                "rejectedMalformedJavaSymbols": rejected_java_symbol_selection_count,
            },
        })
    );
    let mut provider_diagnostics = connection.take_provider_diagnostics(root, lang.id);
    if server == "dart" && dart_synthetic_package_map {
        provider_diagnostics = compact_dart_synthetic_diagnostics(provider_diagnostics);
    } else if server == "jdtls" && large_workspace {
        provider_diagnostics = compact_large_workspace_diagnostics(provider_diagnostics, lang.id);
    }
    let mut diagnostics = startup_diagnostics;
    diagnostics.extend(provider_diagnostics);
    if large_java_local_override_mode {
        diagnostics.push(Diagnostic {
            language: lang.id.to_string(),
            level: "warning",
            code: DiagnosticCode::PartialCoverage,
            message: "Large Java workspace avoided provider-wide member implementation searches; only exact @Override members inside provider-confirmed local type pairs were retained".to_string(),
            detail: Some(
                "Unannotated or signature-ambiguous member overrides remain gaps; type-level extends/implements and call relations are unaffected".to_string(),
            ),
            path: None,
            line: None,
        });
    }
    if rejected_java_symbol_selection_count > 0 {
        diagnostics.push(Diagnostic {
            language: lang.id.to_string(),
            level: "warning",
            code: DiagnosticCode::PartialCoverage,
            message: format!(
                "Rejected {rejected_java_symbol_selection_count} Java symbols whose provider name position was outside the declaration and could not be repaired uniquely from source"
            ),
            detail: None,
            path: None,
            line: None,
        });
    }
    diagnostics.extend(type_syntax_failures.into_iter().map(|(path, detail)| Diagnostic {
        language: lang.id.to_string(),
        level: "warning",
        code: DiagnosticCode::PartialCoverage,
        message: "Exact type-relation syntax enrichment was unavailable for this file; other provider facts were retained".to_string(),
        detail: Some(detail),
        path: Some(path),
        line: None,
    }));
    if let Some(reason) = partial_reason {
        diagnostics.push(Diagnostic {
            language: lang.id.to_string(),
            level: "warning",
            code: DiagnosticCode::ProviderTimeout,
            message: format!(
                "{} semantic provider reached its time/resource limit; indexed {} of {} source documents. {} ({reason})",
                lang.name,
                document_symbol_files.len(),
                semantic_file_count,
                if document_symbol_files.len() < semantic_file_count {
                    "The remaining files are marked missing"
                } else {
                    "File coverage was retained; deeper semantic enrichment may be incomplete"
                }
            ),
            detail: None,
            path: None,
            line: None,
        });
    }
    if workspace_symbol_mode {
        diagnostics.push(Diagnostic {
            language: lang.id.to_string(),
            level: "warning",
            code: DiagnosticCode::LargeWorkspacePartial,
            message: format!(
                "large Java workspace reused the provider workspace-symbol index for {} of {} source documents; remaining files were queried individually",
                document_symbol_files.len(), semantic_file_count
            ),
            detail: None,
            path: None,
            line: None,
        });
    }
    if large_workspace {
        let omitted = if large_map_enrichment && !large_call_enrichment {
            "large-project call hierarchy enrichment"
        } else if large_map_enrichment {
            "non-boundary per-symbol reference and lexical queries"
        } else {
            "per-symbol call, type, reference, and lexical queries"
        };
        diagnostics.push(Diagnostic {
            language: lang.id.to_string(),
            level: "warning",
            code: DiagnosticCode::LargeWorkspacePartial,
            message: format!(
                "large-workspace semantic enrichment limited for {} source files; declarations and imports retained, {} skipped",
                semantic_files.len(), omitted
            ),
            detail: None,
            path: None,
            line: None,
        });
        if let Some(limit) = open_limit.filter(|_| server == "dart") {
            diagnostics.push(Diagnostic {
                language: lang.id.to_string(),
                level: "warning",
                code: DiagnosticCode::LargeWorkspacePartial,
                message: format!(
                    "large Dart workspace opened {} documents as editor buffers; remaining files were queried from the workspace index",
                    limit.min(semantic_files.len())
                ),
                detail: None,
                path: None,
                line: None,
            });
        }
    }
    // Do not claim coverage for files for which the provider returned no
    // symbols or occurrences. The source tree still records those files as
    // missing/empty, so the desktop can show the gap instead of a fake node.
    index.documents.retain(|document| {
        !document.symbols.is_empty()
            || !document.occurrences.is_empty()
            || document_symbol_files.contains(&document.relative_path)
    });
    scip::write_message_to_file(out, index)
        .map_err(|e| format!("cannot write {}: {e}", out.display()))?;
    let mut config_files = workspace_context_files(
        lang.contract_language,
        project_root,
        &analysis_root,
        files,
    )
    .into_iter()
    .map(|path| (path, ProviderConfigUse::WorkspaceDiscovery))
    .collect::<Vec<_>>();
    if let Some(path) = explicit_context_file.filter(|path| {
        path.canonicalize()
            .is_ok_and(|path| path.starts_with(&canonical_project_root))
    }) {
        config_files.push((path, ProviderConfigUse::ExplicitArgument));
    }
    let generated_context_digest = generated_context_file
        .as_ref()
        .and_then(|path| generated_context_digest_from_files(std::slice::from_ref(path)));
    let mode = if java_source_only {
        ProviderExecutionMode::SourceOnlyFallback
    } else if generated_context_digest.is_some() {
        ProviderExecutionMode::GeneratedProject
    } else if config_files.is_empty() {
        ProviderExecutionMode::InferredWorkspace
    } else {
        ProviderExecutionMode::Project
    };
    let actual_source_files = analysis_files
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    let execution_context = executed_provider_context(ExecutedProviderContextInput {
        project_root,
        language: lang,
        mode,
        analysis_root: &analysis_root,
        source_files: &actual_source_files,
        config_files,
        generated_context_digest,
        dimensions: Vec::new(),
    })?;
    Ok(ProviderRunOutcome {
        diagnostics,
        execution_context,
    })
}

struct LocalTypeResolutionContext<'a> {
    current_relative: &'a str,
    current_symbols: &'a [LspSymbol],
    root: &'a Path,
    document_indexes: &'a HashMap<String, usize>,
    symbol_cache: &'a HashMap<String, Vec<LspSymbol>>,
}

fn type_resolution_query_is_planned(
    large_workspace: bool,
    planned_queries: &HashSet<(String, u32, u32)>,
    uri: &str,
    range: &[i32],
) -> bool {
    if !large_workspace {
        return true;
    }
    let [line, character, ..] = range else {
        return false;
    };
    if *line < 0 || *character < 0 {
        return false;
    }
    planned_queries.contains(&(uri.to_string(), *line as u32, *character as u32))
}

fn resolved_local_type_symbols_at(
    connection: &mut LspConnection,
    uri: &str,
    range: &[i32],
    context: &LocalTypeResolutionContext<'_>,
) -> Vec<String> {
    let [line, character, ..] = range else {
        return Vec::new();
    };
    if *line < 0 || *character < 0 {
        return Vec::new();
    }
    let definition_locations = connection.definitions_at(uri, *line as u32, *character as u32);
    let mut resolved = definition_locations
        .into_iter()
        .filter_map(|(target_uri, target_range)| {
            let target_relative = target_uri
                .as_deref()
                .map(|uri| uri_to_relative_path(uri, context.root))
                .unwrap_or_else(|| context.current_relative.to_string());
            if !context.document_indexes.contains_key(&target_relative) {
                return None;
            }
            let target = context
                .symbol_cache
                .get(&target_relative)
                .and_then(|symbols| find_lsp_symbol_at_range(symbols, &target_range))
                .filter(|symbol| is_type_hierarchy_kind(symbol.kind))?;
            Some(symbol_string(
                &target_relative,
                &target.name,
                target.selection_line,
                target.selection_character,
            ))
        })
        .collect::<Vec<_>>();
    resolved.sort();
    resolved.dedup();
    resolved
}

fn local_declared_type_symbols_at(
    range: &[i32],
    context: &LocalTypeResolutionContext<'_>,
) -> Vec<String> {
    find_lsp_symbol_at_range(context.current_symbols, range)
        .filter(|symbol| is_type_hierarchy_kind(symbol.kind))
        .map(|symbol| {
            vec![symbol_string(
                context.current_relative,
                &symbol.name,
                symbol.selection_line,
                symbol.selection_character,
            )]
        })
        .unwrap_or_default()
}

fn local_callable_symbol_from_lsp_item(
    value: &Value,
    root: &Path,
    document_indexes: &HashMap<String, usize>,
    symbol_cache: &HashMap<String, Vec<LspSymbol>>,
) -> Option<String> {
    let uri = value
        .get("uri")
        .or_else(|| value.get("targetUri"))?
        .as_str()?;
    let relative = uri_to_relative_path(uri, root);
    if !document_indexes.contains_key(&relative) {
        return None;
    }
    let range = value
        .get("selectionRange")
        .or_else(|| value.get("targetSelectionRange"))
        .or_else(|| value.get("range"))
        .or_else(|| value.get("targetRange"))
        .and_then(parse_lsp_range)?;
    let symbol = symbol_cache
        .get(&relative)
        .and_then(|symbols| find_lsp_symbol_at_range(symbols, &range))
        .filter(|symbol| is_callable_kind(symbol.kind))?;
    Some(symbol_string(
        &relative,
        &symbol.name,
        symbol.selection_line,
        symbol.selection_character,
    ))
}

fn python_private_member_name(name: &str) -> bool {
    let base = name.split(['(', ':']).next().unwrap_or(name);
    base.starts_with("__") && !base.ends_with("__")
}

#[cfg(test)]
fn java_explicit_override_annotation(source: &str, symbol: &LspSymbol) -> bool {
    let lines = source.lines().collect::<Vec<_>>();
    java_explicit_override_annotation_in_lines(&lines, symbol)
}

fn java_explicit_override_annotation_in_lines(lines: &[&str], symbol: &LspSymbol) -> bool {
    if symbol.range_start_line > symbol.selection_line {
        return false;
    }
    let start = symbol.range_start_line as usize;
    let end = symbol.selection_line as usize;
    lines
        .get(start..=end)
        .into_iter()
        .flatten()
        .filter_map(|line| line.split_whitespace().next())
        .map(|token| token.trim_end_matches("()"))
        .any(|token| matches!(token, "@Override" | "@java.lang.Override"))
}

pub(crate) fn capture_provider_stderr(
    server: &str,
    stderr: impl Read + Send + 'static,
) -> std::thread::JoinHandle<String> {
    let server = server.to_string();
    std::thread::spawn(move || {
        let mut tail = std::collections::VecDeque::new();
        let mut total_bytes = 0usize;
        for line in BufReader::new(stderr).lines() {
            let Ok(line) = line else {
                break;
            };
            if !is_benign_provider_stderr(&server, &line) {
                eprintln!("[provider:{server}] {line}");
                total_bytes += line.len();
                tail.push_back(line);
                while total_bytes > 4096 || tail.len() > 12 {
                    if let Some(oldest) = tail.pop_front() {
                        total_bytes = total_bytes.saturating_sub(oldest.len());
                    } else {
                        break;
                    }
                }
            }
        }
        tail.into_iter().collect::<Vec<_>>().join("\n")
    })
}

pub(crate) fn forward_provider_stderr(server: &str, stderr: impl Read + Send + 'static) {
    let _ = capture_provider_stderr(server, stderr);
}
