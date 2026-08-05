fn run_native_lsp_with_server_mode(
    lang: &LanguageSpec,
    server: &str,
    root: &Path,
    out: &Path,
    providers_root: Option<&Path>,
    files: &[PathBuf],
    java_source_only: bool,
) -> Result<Vec<Diagnostic>, String> {
    let analysis_root = lsp_workspace_root(lang, root, files);
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
    let mut command = tool_command(server, providers_root)?;
    if server == "clangd" {
        // VisualMap opens and queries the project files explicitly. Disable
        // clangd's second, repository-wide background index to avoid keeping
        // duplicate ASTs for every compile-database configuration in memory.
        command.arg("--background-index=false");
        if let Some(directory) = prepare_clangd_compile_database(
            root,
            files,
            out.parent().unwrap_or_else(|| Path::new(".")),
        ) {
            command.arg(format!("--compile-commands-dir={}", directory.display()));
        }
    }
    if server == "jdtls" {
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
    } else if server == "ruby-lsp" {
        let bundle_cache = project_cache_root(&analysis_root).join("bundler");
        command.env("BUNDLE_USER_CACHE", &bundle_cache);
        command.env("BUNDLE_USER_CONFIG", bundle_cache.join("config"));
        command.env("BUNDLE_USER_PLUGIN", bundle_cache.join("plugin"));
        if env::var("CODE_MEMORY_OFFLINE").as_deref() == Ok("1") {
            command.env("BUNDLE_ALLOW_OFFLINE_INSTALL", "1");
            command.env("BUNDLE_FROZEN", "1");
        }
    }
    if server == "gopls" {
        command.arg("serve");
    } else if server == "pyright-langserver" {
        command.arg("--stdio");
    } else if server == "ruby-lsp" {
        command.arg("--use-launcher");
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
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            terminate_process_tree(&mut child);
            return Err("native LSP stderr unavailable".to_string());
        }
    };
    forward_provider_stderr(server, stderr);
    let stdin = match child.stdin.take() {
        Some(stdin) => stdin,
        None => {
            terminate_process_tree(&mut child);
            return Err("native LSP stdin unavailable".to_string());
        }
    };
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            terminate_process_tree(&mut child);
            return Err("native LSP stdout unavailable".to_string());
        }
    };
    let mut connection = LspConnection::new(
        child,
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
    const DART_LARGE_OPEN_LIMIT: usize = 256;
    const RUST_LARGE_OPEN_LIMIT: usize = 256;
    let open_limit = if server == "dart" && file_large_workspace {
        Some(DART_LARGE_OPEN_LIMIT)
    } else if server == "rust-analyzer" && file_large_workspace {
        Some(RUST_LARGE_OPEN_LIMIT)
    } else if server == "jdtls" && file_large_workspace {
        // JDTLS indexes source files from the workspace. Sending thousands of
        // didOpen buffers duplicates that state and slows Gradle reactors.
        Some(256)
    } else {
        None
    };
    let mut partial_reason = None;
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
    // rust-analyzer can publish a complete syntax symbol tree before its
    // Cargo reload finishes. Waiting for that reload can replace the useful
    // tree with an empty/partial response, so start polling early and retain
    // the best response seen below.
    std::thread::sleep(Duration::from_millis(if lang.id == "rust" {
        1000
    } else {
        500
    }));

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
    'document_symbols: for file in &semantic_files {
        if workspace_symbol_mode {
            let relative = file
                .strip_prefix(root)
                .unwrap_or(file)
                .to_string_lossy()
                .replace('\\', "/");
            if document_symbol_files.contains(&relative) {
                continue;
            }
        }
        if partial_reason.is_some() {
            break 'document_symbols;
        }
        if server == "clangd" && is_cpp_header(file) && !reachable_headers.contains(*file) {
            continue;
        }
        let relative = file
            .strip_prefix(root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
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
        let source_index = *document_indexes
            .get(&relative)
            .ok_or_else(|| format!("missing document for {}", relative))?;
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
            occurrence.range = vec![
                symbol.range_start_line as i32,
                symbol.range_start_character as i32,
                symbol.range_end_line as i32,
                symbol.range_end_character as i32,
            ];
            index.documents[source_index].occurrences.push(occurrence);

            let mut information = scip::types::SymbolInformation::new();
            information.symbol = symbol_id.clone();
            information.kind = lsp_kind_to_scip(symbol.kind).into();
            if let Some(detail) = &symbol.detail {
                information.documentation.push(detail.clone());
                let mut signature = scip::types::Signature::new();
                signature.language = lang.id.to_string();
                signature.text = detail.clone();
                information.signature_documentation = protobuf::MessageField::some(signature);
            }
            if !skip_large_workspace_type_enrichment
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
            if !skip_large_workspace_type_enrichment
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
            // type references are retained for the Visual Map type layer.
            // Call hierarchy and lexical definition queries already produce
            // the flow edges VisualMap needs. Per-callable reference queries
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

            let outgoing_calls = if !skip_large_workspace_call_enrichment
                && symbol_semantic_enrichment
                && is_callable_kind(symbol.kind)
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

        if server != "clangd" && !large_workspace {
            let lexical_candidates =
                lexical_call_candidates_with_set(text, symbols, &known_symbol_names);
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
                    (character + name.chars().count() as u32) as i32,
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
    if partial_reason.is_none() {
        connection.ensure_healthy()?;
    }
    let mut provider_diagnostics = connection.take_provider_diagnostics(root, lang.id);
    if server == "dart" && dart_synthetic_package_map {
        provider_diagnostics = compact_dart_synthetic_diagnostics(provider_diagnostics);
    } else if server == "jdtls" && large_workspace {
        provider_diagnostics = compact_large_workspace_diagnostics(provider_diagnostics, lang.id);
    }
    let mut diagnostics = startup_diagnostics;
    diagnostics.extend(provider_diagnostics);
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
    // missing/empty, so VisualMap can show the gap instead of a fake node.
    index.documents.retain(|document| {
        !document.symbols.is_empty()
            || !document.occurrences.is_empty()
            || document_symbol_files.contains(&document.relative_path)
    });
    scip::write_message_to_file(out, index)
        .map_err(|e| format!("cannot write {}: {e}", out.display()))?;
    Ok(diagnostics)
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
