use scip::types::Index;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::{
    find_tool, prepare_clangd_compile_database, project_cache_root, provider_timeout,
    providers::scip::terminate_process_tree, range_parts, range_span, tool_command, Diagnostic,
    LanguageSpec,
};

fn bundled_java_home(jdtls_path: &Path) -> Option<PathBuf> {
    let parent = jdtls_path.parent()?;
    let candidates = [parent.join("runtime"), parent.parent()?.join("runtime")];
    candidates
        .into_iter()
        .find(|candidate| candidate.join("bin").is_dir())
}

pub(crate) fn run_native_lsp(
    lang: &LanguageSpec,
    root: &Path,
    out: &Path,
    providers_root: Option<&Path>,
    files: &[PathBuf],
) -> Result<Vec<Diagnostic>, String> {
    let server = lang.tool;
    run_native_lsp_with_server(lang, server, root, out, providers_root, files)
}

pub(crate) fn run_native_lsp_source_only(
    lang: &LanguageSpec,
    root: &Path,
    out: &Path,
    providers_root: Option<&Path>,
    files: &[PathBuf],
) -> Result<Vec<Diagnostic>, String> {
    run_native_lsp_with_server_mode(lang, "jdtls", root, out, providers_root, files, true)
}

pub(crate) fn run_native_lsp_with_server(
    lang: &LanguageSpec,
    server: &str,
    root: &Path,
    out: &Path,
    providers_root: Option<&Path>,
    files: &[PathBuf],
) -> Result<Vec<Diagnostic>, String> {
    run_native_lsp_with_server_mode(lang, server, root, out, providers_root, files, false)
}

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
                message: format!(
                    "{reason}; using a temporary local-only package map and leaving unavailable packages external"
                ),
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
    let large_workspace = semantic_files.len() > 500
        || (server == "clangd" && semantic_files.len() > 250)
        // gopls rebuilds package/type state for many per-symbol requests; a
        // 400-file module can hit the normal session ceiling before the map
        // is complete. Keep exported package boundaries and provider-backed
        // calls, while skipping private-symbol fan-out.
        || (server == "gopls" && semantic_files.len() > 250);
    let dart_synthetic_package_map = dart_package_config
        .as_ref()
        .is_some_and(|path| path.ends_with("package_config.synthetic.json"));
    let large_map_enrichment = large_workspace
        && large_map_enrichment_language(lang.id)
        && !(server == "dart" && dart_synthetic_package_map);
    // Large-workspace call queries are already restricted below to map-boundary
    // symbols. Keep that provider-backed boundary pass for Rust as well; the
    // previous Rust-only skip removed too much of the VisualMap flow layer.
    let large_call_enrichment = large_map_enrichment;
    let skip_large_workspace_type_enrichment = large_workspace && !large_map_enrichment;
    let skip_large_workspace_call_enrichment = large_workspace && !large_call_enrichment;
    let mut source_cache = HashMap::<String, String>::new();
    let mut symbol_cache = HashMap::<String, Vec<LspSymbol>>::new();
    let mut document_symbol_files = HashSet::new();
    // ponytail: cap open documents for very large workspaces; the server still
    // reads the remaining files from disk for document requests. Raising this
    // is only needed if a provider version proves it needs editor buffers.
    const DART_LARGE_OPEN_LIMIT: usize = 256;
    const RUST_LARGE_OPEN_LIMIT: usize = 256;
    let open_limit = if server == "dart" && large_workspace {
        Some(DART_LARGE_OPEN_LIMIT)
    } else if server == "rust-analyzer" && large_workspace {
        Some(RUST_LARGE_OPEN_LIMIT)
    } else if server == "jdtls" && large_workspace {
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
            if server == "dart" && large_workspace && (opened_index + 1) % 8 == 0 {
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
    if server == "jdtls" && large_workspace {
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
        let retries = if large_workspace && lang.id != "rust" {
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
                for (target_uri, target_range) in connection.definitions_at(&uri, line, character) {
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
            path: None,
            line: None,
        });
    }
    if workspace_symbol_mode {
        diagnostics.push(Diagnostic {
            language: lang.id.to_string(),
            level: "warning",
            message: format!(
                "large Java workspace reused the provider workspace-symbol index for {} of {} source documents; remaining files were queried individually",
                document_symbol_files.len(), semantic_file_count
            ),
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
            message: format!(
                "large-workspace semantic enrichment limited for {} source files; declarations and imports retained, {} skipped",
                semantic_files.len(), omitted
            ),
            path: None,
            line: None,
        });
        if let Some(limit) = open_limit.filter(|_| server == "dart") {
            diagnostics.push(Diagnostic {
                language: lang.id.to_string(),
                level: "warning",
                message: format!(
                    "large Dart workspace opened {} documents as editor buffers; remaining files were queried from the workspace index",
                    limit.min(semantic_files.len())
                ),
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

pub(crate) fn forward_provider_stderr(server: &str, stderr: impl Read + Send + 'static) {
    let server = server.to_string();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines() {
            let Ok(line) = line else {
                break;
            };
            if !is_benign_provider_stderr(&server, &line) {
                eprintln!("[provider:{server}] {line}");
            }
        }
    });
}

pub(crate) fn configure_lsp_workspace(
    connection: &mut LspConnection,
    server: &str,
    _language: &str,
    root: &Path,
    java_source_only: bool,
) -> Result<(), String> {
    let settings = match server {
        "rust-analyzer" => rust_analyzer_settings(),
        "jdtls" => java_language_server_settings(java_source_only),
        _ => serde_json::json!({}),
    };
    connection.set_workspace_settings(settings.clone());
    if server != "rust-analyzer" {
        connection.notify(
            "workspace/didChangeConfiguration",
            serde_json::json!({"settings":settings}),
        )?;
    }
    if server == "dart" {
        connection.notify(
            "analysis.setAnalysisRoots",
            serde_json::json!({
                "included": [path_to_uri(root)],
                "excluded": []
            }),
        )?;
    }
    Ok(())
}

fn rust_analyzer_settings() -> Value {
    serde_json::json!({
        "rust-analyzer": {
            "checkOnSave": {"enable": false},
            "cargo": {
                "noDeps": true,
                "allTargets": false,
                "autoreload": false,
                "buildScripts": {"enable": false},
                "loadOutDirsFromCheck": false
            },
            "procMacro": {"enable": false}
        }
    })
}

fn java_language_server_settings(source_only: bool) -> Value {
    serde_json::json!({
        "java": {
            "autobuild": {"enabled": false},
            "import": {
                "gradle": {
                    "enabled": !source_only,
                    "offline": {"enabled": true},
                    "wrapper": {"enabled": false}
                },
                "maven": {
                    "enabled": !source_only,
                    "offline": {"enabled": true}
                }
            },
            "project": {
                "importOnFirstTimeStartup": if source_only { "disabled" } else { "automatic" }
            },
            "references": {"includeDecompiledSources": false}
        }
    })
}

fn java_home_is_usable(path: &Path) -> bool {
    let executable = if cfg!(windows) { "java.exe" } else { "java" };
    path.join("bin").join(executable).is_file()
}

fn configuration_value(settings: &Value, section: &str) -> Option<Value> {
    section
        .split('.')
        .try_fold(settings, |value, part| value.get(part))
        .cloned()
}

pub(crate) fn is_benign_provider_stderr(server: &str, line: &str) -> bool {
    let trimmed = line.trim_start();
    (server == "rust-analyzer"
        && line.contains("notify error: Input watch path is neither a file nor a directory"))
        || (server == "clangd"
            && (trimmed.starts_with("I[")
                || trimmed.starts_with("argv[")
                || trimmed.contains("Found definition heuristically")
                || trimmed.starts_with("[") && trimmed.ends_with("]")
                || trimmed.contains("--driver-mode=")
                || trimmed.contains("-resource-dir=")))
}

pub(crate) fn lsp_workspace_root(lang: &LanguageSpec, root: &Path, files: &[PathBuf]) -> PathBuf {
    // Module planning already selected an explicit package/crate root. Do not
    // silently widen it back to a parent workspace: that recreates the large
    // single-session failure this planner is meant to avoid.
    if matches!(lang.id, "java" | "rust" | "dart") && workspace_has_marker(lang.id, root) {
        return root.to_path_buf();
    }
    if let Some(workspace_root) = ancestor_workspace_root(lang.id, root) {
        return workspace_root;
    }
    let mut candidates = HashSet::new();
    for file in files {
        if let Some(candidate) = file
            .ancestors()
            .take_while(|ancestor| ancestor.starts_with(root))
            .find(|ancestor| workspace_has_marker(lang.id, ancestor))
        {
            candidates.insert(candidate.to_path_buf());
        }
    }
    // A single nested module can use its own project root. Multiple modules
    // stay at the caller root so one module cannot hide the others.
    if candidates.len() == 1 {
        candidates
            .into_iter()
            .next()
            .unwrap_or_else(|| root.to_path_buf())
    } else {
        root.to_path_buf()
    }
}

fn ancestor_workspace_root(language: &str, root: &Path) -> Option<PathBuf> {
    for ancestor in root.ancestors().take(8) {
        let marker = match language {
            "go" if ancestor.join("go.work").is_file() => true,
            "java"
                if ancestor.join("settings.gradle").is_file()
                    || ancestor.join("settings.gradle.kts").is_file() =>
            {
                true
            }
            "java"
                if fs::read_to_string(ancestor.join("pom.xml"))
                    .ok()
                    .is_some_and(|source| {
                        source.contains("<modules>") && source.contains("<module>")
                    }) =>
            {
                true
            }
            "rust" => fs::read_to_string(ancestor.join("Cargo.toml"))
                .ok()
                .is_some_and(|source| source.lines().any(|line| line.trim() == "[workspace]")),
            _ => false,
        };
        if marker {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

pub(crate) fn workspace_has_marker(language: &str, path: &Path) -> bool {
    let markers: &[&str] = match language {
        "rust" => &["Cargo.toml"],
        "go" => &["go.work", "go.mod"],
        "java" => &[
            "pom.xml",
            "settings.gradle",
            "settings.gradle.kts",
            "build.gradle",
            "build.gradle.kts",
        ],
        "dart" => &["pubspec.yaml"],
        "ruby" => &["Gemfile", ".ruby-version"],
        "python" => &[
            "pyproject.toml",
            "pyrightconfig.json",
            "setup.py",
            "setup.cfg",
        ],
        _ => &[],
    };
    markers.iter().any(|marker| path.join(marker).is_file())
}

pub(crate) fn dart_dependency_metadata_gap(root: &Path) -> Option<String> {
    for ancestor in root.ancestors().take(8) {
        let manifest = ancestor.join("pubspec.yaml");
        let Ok(source) = fs::read_to_string(&manifest) else {
            continue;
        };
        let requires_workspace_resolution = source.lines().any(|line| {
            let trimmed = line.trim();
            trimmed == "workspace:"
                || trimmed.starts_with("workspace:")
                || trimmed == "resolution: workspace"
                || trimmed == "flutter:"
                || trimmed == "flutter_test:"
                || (trimmed.starts_with("dependencies:") && !trimmed.contains("{}"))
                || (trimmed.starts_with("dev_dependencies:") && !trimmed.contains("{}"))
                || (trimmed.starts_with("dependency_overrides:") && !trimmed.contains("{}"))
        });
        if !requires_workspace_resolution {
            continue;
        }
        let package_config = ancestor.join(".dart_tool").join("package_config.json");
        if !package_config.is_file() {
            return Some(format!(
                "Dart dependency metadata is unavailable at {}; refusing analysis_server startup without the project's resolved .dart_tool/package_config.json (no dependency installation is performed)",
                ancestor.display()
            ));
        }
        return None;
    }
    None
}

pub(crate) fn dart_package_config(root: &Path) -> Result<PathBuf, String> {
    for ancestor in root.ancestors().take(8) {
        let path = ancestor.join(".dart_tool").join("package_config.json");
        if dart_package_config_is_valid(&path) {
            return Ok(path);
        }
    }

    let output = project_cache_root(root)
        .join("dart")
        .join("package_config.synthetic.json");
    let package_root = root
        .ancestors()
        .find(|ancestor| ancestor.join("melos.yaml").is_file())
        .unwrap_or(root);
    let mut manifests = Vec::new();
    collect_dart_package_manifests(package_root, &mut manifests);
    manifests.sort();
    manifests.dedup();

    let mut packages = Vec::new();
    let mut names = HashSet::new();
    for manifest in manifests {
        let Some(package_name) = fs::read_to_string(&manifest)
            .ok()
            .and_then(|source| dart_yaml_scalar(&source, "name"))
        else {
            continue;
        };
        let Some(package_root) = manifest.parent() else {
            continue;
        };
        if !names.insert(package_name.clone()) {
            continue;
        }
        let package_uri = if package_root.join("lib").is_dir() {
            "lib/"
        } else {
            "./"
        };
        packages.push(serde_json::json!({
            "name": package_name,
            "rootUri": path_to_uri(package_root),
            "packageUri": package_uri
        }));
    }

    if packages.is_empty() {
        return Err(format!(
            "Dart package map could not find a local package under {}",
            root.display()
        ));
    }
    let value = serde_json::json!({
        "configVersion": 2,
        "packages": packages
    });
    let bytes = serde_json::to_vec_pretty(&value)
        .map_err(|error| format!("cannot serialize Dart package map: {error}"))?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create Dart package map directory: {error}"))?;
    }
    fs::write(&output, bytes).map_err(|error| {
        format!(
            "cannot write Dart package map {}: {error}",
            output.display()
        )
    })?;
    Ok(output)
}

fn dart_package_config_is_valid(path: &Path) -> bool {
    let Ok(value) = fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .ok_or(())
    else {
        return false;
    };
    value.get("configVersion").and_then(Value::as_u64) == Some(2)
        && value.get("packages").and_then(Value::as_array).is_some()
}

fn collect_dart_package_manifests(root: &Path, output: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_file() && name == "pubspec.yaml" {
            output.push(path);
        } else if file_type.is_dir()
            && !matches!(
                name.as_str(),
                ".git" | ".dart_tool" | "build" | "node_modules" | "vendor" | "target"
            )
        {
            collect_dart_package_manifests(&path, output);
        }
    }
}

fn dart_yaml_scalar(source: &str, key: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let (left, right) = line.split_once(':')?;
        if left.trim() != key {
            return None;
        }
        let value = right.trim().trim_matches(['"', '\'']);
        (!value.is_empty()).then(|| value.to_string())
    })
}

fn compact_dart_synthetic_diagnostics(diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let mut output = Vec::new();
    let mut missing_packages = HashSet::new();
    let mut suppressed = 0usize;
    for diagnostic in diagnostics {
        if let Some(package) = dart_missing_package(&diagnostic.message) {
            if missing_packages.insert(package) {
                output.push(diagnostic);
            } else {
                suppressed += 1;
            }
        } else if dart_external_cascade(&diagnostic.message) {
            suppressed += 1;
        } else {
            output.push(diagnostic);
        }
    }
    if suppressed > 0 {
        output.push(Diagnostic {
            language: "dart".to_string(),
            level: "warning",
            message: format!(
                "{suppressed} Dart provider diagnostics were collapsed because local-only package analysis cannot resolve external package symbols"
            ),
            path: None,
            line: None,
        });
    }
    output
}

fn compact_large_workspace_diagnostics(
    diagnostics: Vec<Diagnostic>,
    language: &str,
) -> Vec<Diagnostic> {
    let mut seen = HashSet::new();
    let mut suppressed = 0usize;
    let mut output = Vec::new();
    for diagnostic in diagnostics {
        let key = format!(
            "{}:{}:{}",
            diagnostic.language, diagnostic.level, diagnostic.message
        );
        if seen.insert(key) {
            output.push(diagnostic);
        } else {
            suppressed += 1;
        }
    }
    if suppressed > 0 {
        output.push(Diagnostic {
            language: language.to_string(),
            level: "warning",
            message: format!(
                "{suppressed} repeated Java provider diagnostics were collapsed for the large-workspace view"
            ),
            path: None,
            line: None,
        });
    }
    output
}

fn dart_missing_package(message: &str) -> Option<String> {
    let start = message.find("package:")? + "package:".len();
    let remainder = &message[start..];
    let end = remainder.find(['/', '\'', '"']).unwrap_or(remainder.len());
    (!remainder[..end].is_empty()).then(|| remainder[..end].to_string())
}

fn dart_external_cascade(message: &str) -> bool {
    [
        "Undefined name",
        "Undefined class",
        "Undefined getter",
        "Undefined setter",
        "Undefined operator",
        "The function '",
        "The method '",
        "The name '",
        "Classes can only extend",
        "No associated named super constructor",
        "doesn't override an inherited method",
        "Method invocation or property access on a 'dynamic' target",
    ]
    .iter()
    .any(|prefix| message.contains(prefix))
}

static NEXT_LSP_ID: AtomicI64 = AtomicI64::new(1);
const MAX_LSP_MESSAGE_BYTES: usize = 128 * 1024 * 1024;

pub(crate) fn lsp_request_timeout() -> Duration {
    env::var("CODE_MEMORY_LSP_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| (1_000..=300_000).contains(value))
        .map(Duration::from_millis)
        .unwrap_or_else(|| Duration::from_secs(30))
}

pub(crate) fn lsp_max_requests() -> usize {
    env::var("CODE_MEMORY_LSP_MAX_REQUESTS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| (100..=5_000_000).contains(value))
        .unwrap_or(100_000)
}

pub(crate) fn lsp_session_timeout(large_workspace: bool) -> Duration {
    env::var("CODE_MEMORY_LSP_MAX_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value == 0 || (5..=1_800).contains(value))
        .map(|value| {
            if value == 0 {
                // ponytail: match provider no-timeout mode without widening the session API.
                Duration::from_secs(60 * 60 * 24 * 365 * 10)
            } else {
                Duration::from_secs(value)
            }
        })
        .unwrap_or_else(|| {
            if large_workspace && env::var_os("CODE_MEMORY_PROVIDER_TIMEOUT_SECONDS").is_none() {
                Duration::from_secs(900)
            } else {
                provider_timeout()
            }
        })
}

pub(crate) fn lsp_reference_enrichment_enabled(language: &str) -> bool {
    language == "ruby" || env::var("CODE_MEMORY_LSP_REFERENCES").as_deref() == Ok("1")
}

fn large_map_enrichment_language(language: &str) -> bool {
    matches!(
        language,
        "c" | "cpp" | "dart" | "go" | "java" | "python" | "ruby" | "rust"
    )
}

pub(crate) struct LspConnection {
    child: std::process::Child,
    stdin: ChildStdin,
    messages: Receiver<Value>,
    timeout: Duration,
    deadline: Instant,
    request_count: usize,
    max_requests: usize,
    request_cache: HashMap<String, Value>,
    outgoing_call_cache: HashMap<String, Vec<(String, String, Vec<i32>)>>,
    workspace_settings: Value,
    fatal_error: Option<String>,
    provider_diagnostics: Vec<ProviderDiagnostic>,
    type_hierarchy_supported: bool,
}

struct ProviderDiagnostic {
    uri: String,
    level: &'static str,
    line: Option<u32>,
    message: String,
}

pub(crate) struct LspSymbol {
    pub(crate) name: String,
    pub(crate) kind: u32,
    pub(crate) detail: Option<String>,
    pub(crate) range_start_line: u32,
    pub(crate) range_start_character: u32,
    pub(crate) range_end_line: u32,
    pub(crate) range_end_character: u32,
    pub(crate) selection_line: u32,
    pub(crate) selection_character: u32,
}

struct LspReference {
    uri: String,
    range: Vec<i32>,
}

impl LspConnection {
    fn new(
        child: std::process::Child,
        stdin: ChildStdin,
        stdout: BufReader<ChildStdout>,
        timeout: Duration,
        large_workspace: bool,
    ) -> Result<Self, String> {
        let (sender, messages) = mpsc::channel();
        std::thread::spawn(move || {
            let mut stdout = stdout;
            loop {
                let mut length = None;
                loop {
                    let mut line = String::new();
                    if stdout.read_line(&mut line).is_err() || line.is_empty() {
                        return;
                    }
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        break;
                    }
                    if let Some(value) = trimmed.strip_prefix("Content-Length:") {
                        let Ok(value) = value.trim().parse::<usize>() else {
                            return;
                        };
                        length = Some(value);
                    }
                }
                let Some(length) = length else {
                    return;
                };
                if !lsp_message_length_allowed(length) {
                    return;
                }
                let mut body = vec![0; length];
                if stdout.read_exact(&mut body).is_err() {
                    return;
                }
                let Ok(value) = serde_json::from_slice(&body) else {
                    return;
                };
                if sender.send(value).is_err() {
                    return;
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            messages,
            timeout,
            deadline: Instant::now() + lsp_session_timeout(large_workspace),
            request_count: 0,
            max_requests: lsp_max_requests(),
            request_cache: HashMap::new(),
            outgoing_call_cache: HashMap::new(),
            workspace_settings: Value::Object(serde_json::Map::new()),
            fatal_error: None,
            provider_diagnostics: Vec::new(),
            type_hierarchy_supported: false,
        })
    }

    fn send(&mut self, value: &Value) -> Result<(), String> {
        let body = serde_json::to_vec(value).map_err(|e| e.to_string())?;
        write!(self.stdin, "Content-Length: {}\r\n\r\n", body.len()).map_err(|e| e.to_string())?;
        self.stdin.write_all(&body).map_err(|e| e.to_string())?;
        self.stdin.flush().map_err(|e| e.to_string())
    }

    fn receive(&mut self) -> Result<Value, String> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("native LSP session timeout".to_string());
        }
        match self.messages.recv_timeout(self.timeout.min(remaining)) {
            Ok(value) => Ok(value),
            Err(RecvTimeoutError::Timeout) => Err(format!(
                "native LSP response timeout after {} ms",
                self.timeout.as_millis()
            )),
            Err(RecvTimeoutError::Disconnected) => Err("native LSP closed stdout".to_string()),
        }
    }

    fn wait_for_retry(&self, duration: Duration) -> Result<(), String> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining < duration {
            return Err("native LSP session timeout".to_string());
        }
        std::thread::sleep(duration);
        Ok(())
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        if Instant::now() >= self.deadline {
            return Err("native LSP session timeout".to_string());
        }
        if self.request_count >= self.max_requests {
            return Err(format!(
                "native LSP request budget exceeded after {} requests",
                self.max_requests
            ));
        }
        self.request_count += 1;
        let id = NEXT_LSP_ID.fetch_add(1, Ordering::Relaxed);
        self.send(&serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
        loop {
            let response = self.receive()?;
            if response.get("method").and_then(Value::as_str)
                == Some("textDocument/publishDiagnostics")
            {
                self.record_provider_diagnostics(&response);
                continue;
            }
            if response.get("id") == Some(&Value::from(id)) {
                if let Some(error) = response.get("error") {
                    return Err(format!("native LSP {method} failed: {error}"));
                }
                return Ok(response.get("result").cloned().unwrap_or(Value::Null));
            }
            if let (Some(other_id), Some(other_method)) =
                (response.get("id"), response.get("method"))
            {
                let result = if other_method.as_str() == Some("workspace/configuration") {
                    self.workspace_configuration(response.get("params"))
                } else {
                    Value::Null
                };
                self.send(&serde_json::json!({"jsonrpc":"2.0","id":other_id,"result":result}))?;
                let _ = other_method;
            }
        }
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        self.send(&serde_json::json!({"jsonrpc":"2.0","method":method,"params":params}))
    }

    fn set_workspace_settings(&mut self, settings: Value) {
        self.workspace_settings = settings;
    }

    fn workspace_configuration(&self, params: Option<&Value>) -> Value {
        if self
            .workspace_settings
            .as_object()
            .is_some_and(serde_json::Map::is_empty)
        {
            return serde_json::json!([]);
        }
        let values = params
            .and_then(|params| params.get("items"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|item| {
                item.get("section")
                    .and_then(Value::as_str)
                    .and_then(|section| configuration_value(&self.workspace_settings, section))
                    .unwrap_or(Value::Null)
            })
            .collect();
        Value::Array(values)
    }

    fn record_provider_diagnostics(&mut self, message: &Value) {
        let Some(params) = message.get("params") else {
            return;
        };
        let Some(uri) = params.get("uri").and_then(Value::as_str) else {
            return;
        };
        for diagnostic in params
            .get("diagnostics")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let level = match diagnostic.get("severity").and_then(Value::as_u64) {
                Some(1) => "error",
                Some(2) => "warning",
                Some(3) => "info",
                _ => "warning",
            };
            let line = diagnostic
                .get("range")
                .and_then(|range| range.get("start"))
                .and_then(|start| start.get("line"))
                .and_then(Value::as_u64)
                .map(|line| line as u32 + 1);
            let Some(text) = diagnostic.get("message").and_then(Value::as_str) else {
                continue;
            };
            self.provider_diagnostics.push(ProviderDiagnostic {
                uri: uri.to_string(),
                level,
                line,
                message: text.to_string(),
            });
        }
    }

    fn drain_provider_notifications(&mut self) {
        while let Ok(message) = self.messages.try_recv() {
            if message.get("method").and_then(Value::as_str)
                == Some("textDocument/publishDiagnostics")
            {
                self.record_provider_diagnostics(&message);
            }
        }
    }

    fn take_provider_diagnostics(&mut self, root: &Path, language: &str) -> Vec<Diagnostic> {
        self.drain_provider_notifications();
        let mut seen = HashSet::new();
        self.provider_diagnostics
            .drain(..)
            .filter_map(|diagnostic| {
                let path = uri_to_relative_path(&diagnostic.uri, root);
                let key = format!(
                    "{}:{}:{}:{}",
                    path,
                    diagnostic.line.unwrap_or_default(),
                    diagnostic.level,
                    diagnostic.message
                );
                if !seen.insert(key) {
                    return None;
                }
                let header_context = matches!(language, "c" | "cpp")
                    && matches!(
                        Path::new(&path)
                            .extension()
                            .and_then(|extension| extension.to_str())
                            .map(|extension| extension.to_ascii_lowercase())
                            .as_deref(),
                        Some("h" | "hh" | "hpp" | "hxx" | "inc" | "inl" | "ipp" | "tpp")
                    );
                // LSP publishDiagnostics are source-code diagnostics, not
                // provider failures. Missing external packages, type-check
                // errors, and project-specific warnings must not make a
                // VisualMap index fail; startup, timeout, and invalid-output
                // failures are reported by the outer analysis layer.
                let level = if diagnostic.level == "error" {
                    "warning"
                } else {
                    diagnostic.level
                };
                let message = if header_context && diagnostic.level == "error" {
                    format!("header-context: {}", diagnostic.message)
                } else if diagnostic.level == "error" {
                    format!("provider-diagnostic: {}", diagnostic.message)
                } else {
                    diagnostic.message
                };
                Some(Diagnostic {
                    language: diagnostic_language(&path, language),
                    level,
                    message,
                    path: Some(path),
                    line: diagnostic.line,
                })
            })
            .collect()
    }

    fn cached_request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let key = format!(
            "{}:{}",
            method,
            serde_json::to_string(&params).map_err(|error| error.to_string())?
        );
        if let Some(value) = self.request_cache.get(&key) {
            return Ok(value.clone());
        }
        let value = self.request(method, params)?;
        self.request_cache.insert(key, value.clone());
        Ok(value)
    }

    fn initialize(
        &mut self,
        root_uri: &str,
        root_path: &str,
        language: &str,
    ) -> Result<(), String> {
        let workspace_capabilities = if language == "rust" {
            serde_json::json!({"workspaceFolders": true, "configuration": true})
        } else {
            serde_json::json!({"workspaceFolders": true})
        };
        let initialization_options = if language == "java" {
            serde_json::json!({"settings": self.workspace_settings})
        } else {
            Value::Null
        };
        let response = self.request(
            "initialize",
            serde_json::json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "rootPath": root_path,
                "capabilities": {
                    "textDocument": {
                        "documentSymbol": {"hierarchicalDocumentSymbolSupport": true},
                        "references": {},
                        "typeDefinition": {},
                        "implementation": {},
                        "callHierarchy": {},
                        "typeHierarchy": {}
                    },
                    "workspace": workspace_capabilities
                },
                "workspaceFolders": [{"uri": root_uri, "name": "code_memory"}],
                "initializationOptions": initialization_options
            }),
        )?;
        self.type_hierarchy_supported = response
            .get("capabilities")
            .and_then(|capabilities| capabilities.get("typeHierarchyProvider"))
            .is_some_and(|provider| provider.as_bool().unwrap_or(provider.is_object()));
        Ok(())
    }

    fn did_open(&mut self, uri: &str, language: &str, text: &str) -> Result<(), String> {
        self.notify("textDocument/didOpen", serde_json::json!({"textDocument":{"uri":uri,"languageId":language,"version":1,"text":text}}))
    }

    fn document_symbols(&mut self, uri: &str) -> Result<Vec<LspSymbol>, String> {
        // Rust projects can publish a partial symbol tree while Cargo and
        // proc-macro state are loading. Retries must reach the provider;
        // caching the first response would make the retry loop ineffective.
        let value = self.request(
            "textDocument/documentSymbol",
            serde_json::json!({"textDocument":{"uri":uri}}),
        )?;
        let mut symbols = Vec::new();
        if let Some(items) = value.as_array() {
            for item in items {
                collect_lsp_symbols(item, &mut symbols);
            }
        }
        Ok(symbols)
    }

    fn workspace_symbols(&mut self) -> Result<Vec<(String, LspSymbol)>, String> {
        let value = self.cached_request("workspace/symbol", serde_json::json!({"query":""}))?;
        let mut symbols = Vec::new();
        for item in value.as_array().into_iter().flatten() {
            let Some(uri) = item
                .get("location")
                .and_then(|location| location.get("uri").or_else(|| location.get("targetUri")))
                .and_then(Value::as_str)
            else {
                continue;
            };
            let mut parsed = Vec::new();
            collect_lsp_symbols(item, &mut parsed);
            symbols.extend(parsed.into_iter().map(|symbol| (uri.to_string(), symbol)));
        }
        Ok(symbols)
    }

    fn references(
        &mut self,
        uri: &str,
        line: u32,
        character: u32,
    ) -> Result<Vec<LspReference>, String> {
        let value = self.optional_request(
            "textDocument/references",
            serde_json::json!({
                "textDocument":{"uri":uri},
                "position":{"line":line,"character":character},
                "context":{"includeDeclaration":false}
            }),
        );
        Ok(value
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(parse_lsp_reference)
            .collect())
    }

    fn optional_request(&mut self, method: &str, params: Value) -> Value {
        if self.fatal_error.is_some() {
            return Value::Null;
        }
        match self.cached_request(method, params) {
            Ok(value) => value,
            Err(error) => {
                // Optional call/type enrichment may time out on a large
                // workspace with unavailable external modules. Drop only
                // that enrichment request; document indexing remains usable.
                if is_fatal_lsp_error(&error)
                    && !error.contains("native LSP response timeout")
                    && self.fatal_error.is_none()
                {
                    self.fatal_error = Some(error);
                }
                Value::Null
            }
        }
    }

    fn type_definitions(&mut self, uri: &str, line: u32, character: u32) -> Vec<Value> {
        self.optional_request(
            "textDocument/typeDefinition",
            serde_json::json!({
                "textDocument":{"uri":uri},
                "position":{"line":line,"character":character}
            }),
        )
        .as_array()
        .cloned()
        .unwrap_or_default()
    }

    fn supertypes(&mut self, uri: &str, line: u32, character: u32) -> Vec<Value> {
        if !self.type_hierarchy_supported {
            return Vec::new();
        }
        let items = self
            .optional_request(
                "textDocument/prepareTypeHierarchy",
                serde_json::json!({
                    "textDocument":{"uri":uri},
                    "position":{"line":line,"character":character}
                }),
            )
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut output = Vec::new();
        for item in items {
            let supers =
                self.optional_request("typeHierarchy/supertypes", serde_json::json!({"item":item}));
            if let Some(values) = supers.as_array() {
                output.extend(values.iter().cloned());
            }
        }
        output
    }

    fn implementations(&mut self, uri: &str, line: u32, character: u32) -> Vec<Value> {
        self.optional_request(
            "textDocument/implementation",
            serde_json::json!({
                "textDocument":{"uri":uri},
                "position":{"line":line,"character":character}
            }),
        )
        .as_array()
        .cloned()
        .unwrap_or_default()
    }

    fn outgoing_calls(
        &mut self,
        uri: &str,
        line: u32,
        character: u32,
        root: &Path,
    ) -> Vec<(String, String, Vec<i32>)> {
        let cache_key = format!("{uri}:{line}:{character}");
        if let Some(cached) = self.outgoing_call_cache.get(&cache_key) {
            return cached.clone();
        }
        let items = self
            .optional_request(
                "textDocument/prepareCallHierarchy",
                serde_json::json!({
                    "textDocument":{"uri":uri},
                    "position":{"line":line,"character":character}
                }),
            )
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut output = Vec::new();
        for item in items {
            let calls = self.optional_request(
                "callHierarchy/outgoingCalls",
                serde_json::json!({"item":item}),
            );
            for call in calls.as_array().into_iter().flatten() {
                let Some(target) = call.get("to") else {
                    continue;
                };
                let Some(target_symbol) = lsp_item_symbol(target, root) else {
                    continue;
                };
                let Some(target_uri) = target.get("uri").and_then(Value::as_str) else {
                    continue;
                };
                let target_relative = uri_to_relative_path(target_uri, root);
                for range in call
                    .get("fromRanges")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(parse_lsp_range)
                {
                    output.push((target_symbol.clone(), target_relative.clone(), range));
                }
            }
        }
        if self.fatal_error.is_none() {
            self.outgoing_call_cache.insert(cache_key, output.clone());
        }
        output
    }

    fn definitions_at(
        &mut self,
        uri: &str,
        line: u32,
        character: u32,
    ) -> Vec<(Option<String>, Vec<i32>)> {
        if self.fatal_error.is_some() {
            return Vec::new();
        }
        for _ in 0..3 {
            if Instant::now() >= self.deadline {
                return Vec::new();
            }
            let value = self.optional_request(
                "textDocument/definition",
                serde_json::json!({
                    "textDocument":{"uri":uri},
                    "position":{"line":line,"character":character}
                }),
            );
            let values = match value {
                Value::Array(values) => values,
                Value::Object(_) => vec![value],
                _ => Vec::new(),
            };
            let results: Vec<_> = values
                .into_iter()
                .filter_map(|location| {
                    let target_uri = location
                        .get("uri")
                        .or_else(|| location.get("targetUri"))
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    let range = location
                        .get("range")
                        .or_else(|| location.get("targetSelectionRange"))
                        .and_then(parse_lsp_range)?;
                    Some((target_uri, range))
                })
                .collect();
            if !results.is_empty() {
                return results;
            }
            if self.wait_for_retry(Duration::from_millis(250)).is_err() {
                return Vec::new();
            }
        }
        Vec::new()
    }

    fn shutdown(&mut self) -> Result<(), String> {
        self.request("shutdown", Value::Null)?;
        self.notify("exit", Value::Null)
    }

    fn ensure_healthy(&self) -> Result<(), String> {
        self.fatal_error
            .as_ref()
            .map(|error| Err(error.clone()))
            .unwrap_or(Ok(()))
    }
}

fn diagnostic_language(path: &str, fallback: &str) -> String {
    match Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("c" | "m") => "c".to_string(),
        Some("cc" | "cp" | "cpp" | "cxx" | "hh" | "hpp" | "hxx" | "inl" | "ipp" | "tpp") => {
            "cpp".to_string()
        }
        Some("h" | "inc") => fallback.to_string(),
        _ => fallback.to_string(),
    }
}

pub(crate) fn is_fatal_lsp_error(error: &str) -> bool {
    error.contains("native LSP session timeout")
        || error.contains("native LSP response timeout")
        || error.contains("native LSP request budget exceeded")
        || error.contains("native LSP closed stdout")
        || error.contains("Broken pipe")
        || error.contains("pipe is being closed")
}

fn is_recoverable_lsp_query_error(error: &str) -> bool {
    error.contains("native LSP response timeout")
}

fn is_recoverable_lsp_session_error(error: &str) -> bool {
    is_recoverable_lsp_query_error(error) || is_fatal_lsp_error(error)
}

impl Drop for LspConnection {
    fn drop(&mut self) {
        // Give the server a short grace period after the LSP exit notification.
        // Force-killing rust-analyzer while it is reloading can make it print a
        // misleading worker panic even though indexing succeeded.
        for _ in 0..100 {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                Err(_) => break,
            }
        }

        #[cfg(windows)]
        {
            if self.child.try_wait().ok().flatten().is_none() {
                use std::os::windows::process::CommandExt;
                let mut command = Command::new("taskkill");
                let _ = command
                    .creation_flags(0x08000000)
                    .args(["/PID", &self.child.id().to_string(), "/T", "/F"])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
        }
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

pub(crate) fn lsp_item_symbol(value: &Value, root: &Path) -> Option<String> {
    let name = value.get("name")?.as_str()?;
    let uri = value.get("uri")?.as_str()?;
    let range = value
        .get("selectionRange")
        .or_else(|| value.get("range"))
        .and_then(parse_lsp_range)?;
    Some(symbol_string(
        &uri_to_relative_path(uri, root),
        name,
        range[0] as u32,
        range[1] as u32,
    ))
}

pub(crate) fn collect_lsp_symbols(value: &Value, output: &mut Vec<LspSymbol>) {
    let Some(name) = value.get("name").and_then(Value::as_str) else {
        return;
    };
    let Some(kind) = value.get("kind").and_then(Value::as_u64) else {
        return;
    };
    let range = value.get("range").or_else(|| {
        value
            .get("location")
            .and_then(|location| location.get("range"))
    });
    let Some(range) = range.and_then(parse_lsp_range) else {
        return;
    };
    let selection = value
        .get("selectionRange")
        .and_then(parse_lsp_range)
        .unwrap_or(range.clone());
    output.push(LspSymbol {
        name: name.to_string(),
        kind: kind as u32,
        detail: value
            .get("detail")
            .and_then(Value::as_str)
            .map(str::to_string),
        range_start_line: range[0] as u32,
        range_start_character: range[1] as u32,
        range_end_line: range[2] as u32,
        range_end_character: range[3] as u32,
        selection_line: selection[0] as u32,
        selection_character: selection[1] as u32,
    });
    if let Some(children) = value.get("children").and_then(Value::as_array) {
        for child in children {
            collect_lsp_symbols(child, output);
        }
    }
}

pub(crate) fn parse_lsp_range(value: &Value) -> Option<Vec<i32>> {
    Some(vec![
        value.get("start")?.get("line")?.as_i64()? as i32,
        value.get("start")?.get("character")?.as_i64()? as i32,
        value.get("end")?.get("line")?.as_i64()? as i32,
        value.get("end")?.get("character")?.as_i64()? as i32,
    ])
}

fn parse_lsp_reference(value: &Value) -> Option<LspReference> {
    Some(LspReference {
        uri: value.get("uri")?.as_str()?.to_string(),
        range: parse_lsp_range(value.get("range")?)?,
    })
}

pub(crate) fn find_enclosing_symbol_range(
    symbols: Option<&Vec<LspSymbol>>,
    range: &[i32],
) -> Option<Vec<i32>> {
    symbols?
        .iter()
        .map(|symbol| {
            vec![
                symbol.range_start_line as i32,
                symbol.range_start_character as i32,
                symbol.range_end_line as i32,
                symbol.range_end_character as i32,
            ]
        })
        .filter(|candidate| range_contains(candidate, range))
        .min_by_key(|candidate| {
            (candidate[2] - candidate[0]) * 1_000_000 + candidate[3] - candidate[1]
        })
}

pub(crate) fn range_contains(outer: &[i32], inner: &[i32]) -> bool {
    let Some(outer) = range_parts(outer) else {
        return false;
    };
    let Some(inner) = range_parts(inner) else {
        return false;
    };
    (outer.0, outer.1) <= (inner.0, inner.1) && (inner.2, inner.3) <= (outer.2, outer.3)
}

pub(crate) fn path_to_uri(path: &Path) -> String {
    let mut path = path.to_string_lossy().replace('\\', "/");
    // Windows LSP servers normalize drive letters in file URIs. Match that
    // canonical form so providers such as rust-analyzer resolve their own
    // returned locations back to the indexed files.
    if path.as_bytes().get(1) == Some(&b':') {
        let drive = path[..1].to_ascii_lowercase();
        path.replace_range(..1, &drive);
    }
    if path.starts_with("//") {
        format!("file:{path}")
    } else {
        format!("file:///{path}")
    }
}

pub(crate) fn uri_to_relative_path(uri: &str, root: &Path) -> String {
    let path = percent_decode(
        uri.strip_prefix("file:///")
            .unwrap_or(uri)
            .replace('/', "\\"),
    );
    let path_ref = Path::new(&path);
    if let Ok(relative) = path_ref.strip_prefix(root) {
        return relative.to_string_lossy().replace('\\', "/");
    }
    #[cfg(windows)]
    {
        let path = path.trim_end_matches('\\');
        let root = root.to_string_lossy().replace('/', "\\");
        let root = root.trim_end_matches('\\');
        if path.eq_ignore_ascii_case(root) {
            return String::new();
        }
        if path.len() > root.len()
            && path[..root.len()].eq_ignore_ascii_case(root)
            && path.as_bytes()[root.len()] == b'\\'
        {
            return path[root.len() + 1..].replace('\\', "/");
        }
    }
    path_ref.to_string_lossy().replace('\\', "/")
}

pub(crate) fn percent_decode(value: String) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                decoded.push((high << 4) | low);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

pub(crate) fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

pub(crate) fn symbol_string(file: &str, name: &str, line: u32, character: u32) -> String {
    // Some LSP servers include the return type in call-hierarchy item names
    // (`method(args) : ReturnType`) while document symbols use
    // `method(args)`. Keep one stable identity for both forms.
    let canonical_name = name
        .rsplit_once(" : ")
        .and_then(|(base, return_type)| (!return_type.is_empty()).then_some(base))
        .unwrap_or(name);
    format!(
        "lsp . . . {}#{}@{}:{}",
        file.replace('/', "."),
        canonical_name,
        line,
        character
    )
}

pub(crate) fn lsp_kind_to_scip(kind: u32) -> scip::types::symbol_information::Kind {
    use scip::types::symbol_information::Kind;
    match kind {
        2..=4 => Kind::Module,
        5 => Kind::Class,
        6 | 9 => Kind::Method,
        10 | 22 => Kind::Enum,
        11 => Kind::Interface,
        12 => Kind::Function,
        13 | 14 | 7 | 8 => Kind::Variable,
        23 => Kind::Struct,
        26 => Kind::TypeParameter,
        _ => Kind::UnspecifiedKind,
    }
}

pub(crate) fn is_type_hierarchy_kind(kind: u32) -> bool {
    matches!(kind, 5 | 11 | 23)
}

pub(crate) fn is_callable_kind(kind: u32) -> bool {
    matches!(kind, 6 | 9 | 12)
}

fn has_callable_body(source: &str, symbol: &LspSymbol) -> bool {
    source
        .lines()
        .skip(symbol.range_start_line as usize)
        .take(
            symbol
                .range_end_line
                .saturating_sub(symbol.range_start_line)
                .saturating_add(1) as usize,
        )
        .any(|line| line.contains('{'))
}

pub(crate) fn rust_large_symbol_is_public(source: &str, symbol: &LspSymbol) -> bool {
    if symbol.name == "main" {
        return true;
    }
    let declaration = symbol.detail.as_deref().unwrap_or_default();
    let source_line = source
        .lines()
        .nth(symbol.selection_line as usize)
        .unwrap_or_default();
    let visible = declaration
        .split_whitespace()
        .any(|token| token == "pub" || token.starts_with("pub("))
        || source_line
            .split_whitespace()
            .any(|token| token == "pub" || token.starts_with("pub("));
    visible && (source_line == source_line.trim_start() || is_type_hierarchy_kind(symbol.kind))
}

pub(crate) fn large_symbol_is_map_boundary(
    language: &str,
    source: &str,
    symbol: &LspSymbol,
) -> bool {
    if language == "rust" {
        return rust_large_symbol_is_public(source, symbol);
    }
    if symbol.name == "main" {
        return true;
    }
    let declaration = symbol.detail.as_deref().unwrap_or_default();
    let source_line = source
        .lines()
        .nth(symbol.selection_line as usize)
        .unwrap_or_default();
    match language {
        // Python module-level names are the stable map entry points. Private
        // names are implementation details unless the provider reports them
        // through a public caller.
        "python" => {
            !symbol.name.starts_with('_')
                && !source_line.chars().next().is_some_and(char::is_whitespace)
        }
        // Go's exported identifier rule is part of the language, not a target
        // mapping. In a large workspace query package functions first; asking
        // gopls for every receiver method rebuilds too much workspace state.
        "go" => {
            let exported = symbol
                .name
                .chars()
                .next()
                .is_some_and(|character| character.is_uppercase());
            let declaration_line = source_line.trim_start();
            exported
                && declaration_line.starts_with("func ")
                && !declaration_line.starts_with("func (")
        }
        // Ruby and Dart use underscore visibility for the common public API.
        "ruby" | "dart" => !symbol.name.starts_with('_'),
        // Java package-private declarations are still useful module API. Only
        // explicit private members are cut from the large-workspace pass.
        "java" => !declaration.contains("private") && !source_line.contains(" private "),
        // C/C++ has no universal public keyword. A non-static declaration is
        // the provider-backed module boundary; internal calls remain in the
        // source tree but do not trigger one LSP query per symbol.
        "c" | "cpp" => {
            !declaration
                .split_whitespace()
                .any(|token| token == "static")
                && !source_line
                    .split_whitespace()
                    .any(|token| token == "static")
        }
        _ => false,
    }
}

pub(crate) fn is_callable_or_type_kind(kind: u32) -> bool {
    is_callable_kind(kind) || matches!(kind, 5 | 10 | 11 | 22 | 23 | 26)
}

#[cfg(test)]
pub(crate) fn lexical_call_candidates(
    source: &str,
    symbols: &[LspSymbol],
    known_names: &[String],
) -> Vec<(u32, u32, String)> {
    let known_names: HashSet<String> = known_names.iter().cloned().collect();
    lexical_call_candidates_with_set(source, symbols, &known_names)
}

fn lexical_call_candidates_with_set(
    source: &str,
    symbols: &[LspSymbol],
    known_names: &HashSet<String>,
) -> Vec<(u32, u32, String)> {
    // ponytail: lexical candidates only seed LSP definition queries; add a language parser if a provider cannot resolve these positions.
    let definition_positions: HashSet<(u32, u32)> = symbols
        .iter()
        .map(|symbol| (symbol.selection_line, symbol.selection_character))
        .collect();
    let mut candidates = Vec::new();
    for (line_number, line) in source.lines().enumerate() {
        let bytes = line.as_bytes();
        let mut offset = 0;
        while offset < bytes.len() {
            if !is_identifier_start(bytes[offset]) {
                offset += 1;
                continue;
            }

            let start = offset;
            offset += 1;
            while offset < bytes.len() && is_identifier_continue(bytes[offset]) {
                offset += 1;
            }
            let name = &line[start..offset];
            if !known_names.contains(name) || !line[offset..].trim_start().starts_with('(') {
                continue;
            }
            let character = utf16_len(&line[..start]);
            if !definition_positions.contains(&(line_number as u32, character)) {
                candidates.push((line_number as u32, character, name.to_string()));
            }
        }
    }
    candidates
}

pub(crate) fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

pub(crate) fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

pub(crate) fn is_cpp_header(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("h" | "hh" | "hpp" | "hxx")
    )
}

pub(crate) fn is_cpp_header_fragment(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("inc" | "inl" | "ipp" | "tpp")
    )
}

fn lsp_message_length_allowed(length: usize) -> bool {
    length <= MAX_LSP_MESSAGE_BYTES
}

pub(crate) fn reachable_project_headers(root: &Path, files: &[&PathBuf]) -> HashSet<PathBuf> {
    let headers: Vec<PathBuf> = files
        .iter()
        .filter(|file| is_cpp_header(file))
        .map(|file| (*file).clone())
        .collect();
    let header_index = build_header_lookup(&headers);
    let mut queue: Vec<PathBuf> = files
        .iter()
        .filter(|file| !is_cpp_header(file) && !is_cpp_header_fragment(file))
        .map(|file| (*file).clone())
        .collect();
    let mut reachable = HashSet::new();
    while let Some(file) = queue.pop() {
        let Ok(source) = fs::read_to_string(&file) else {
            continue;
        };
        for target in source.lines().filter_map(include_target) {
            let candidates = resolve_project_header(&file, &target, root, &header_index);
            if candidates.len() != 1 {
                continue;
            }
            let header = candidates.into_iter().next().expect("one header");
            reachable.insert(header);
        }
    }
    reachable
}

fn build_header_lookup(headers: &[PathBuf]) -> HashMap<String, Vec<PathBuf>> {
    let mut index = HashMap::<String, Vec<PathBuf>>::new();
    for header in headers {
        let normalized = header_lookup_path(header);
        let components: Vec<&str> = normalized
            .split('/')
            .filter(|part| !part.is_empty())
            .collect();
        for start in 0..components.len() {
            let key = components[start..].join("/");
            let entries = index.entry(key).or_default();
            if !entries.contains(header) {
                entries.push(header.clone());
            }
        }
    }
    index
}

fn header_lookup_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn include_target(line: &str) -> Option<String> {
    let value = line.trim().strip_prefix("#include")?.trim();
    let (open, close) = if value.starts_with('<') {
        ('<', '>')
    } else if value.starts_with('"') {
        ('"', '"')
    } else {
        return None;
    };
    let value = value.strip_prefix(open)?;
    let end = value.find(close)?;
    (!value[..end].is_empty()).then(|| value[..end].replace('\\', "/"))
}

fn resolve_project_header(
    source: &Path,
    target: &str,
    root: &Path,
    header_index: &HashMap<String, Vec<PathBuf>>,
) -> Vec<PathBuf> {
    let target = target
        .trim_start_matches("./")
        .replace('\\', "/")
        .to_ascii_lowercase();
    let mut candidates = Vec::new();
    for candidate in [
        source.parent().map(|parent| parent.join(&target)),
        Some(root.join(&target)),
    ]
    .into_iter()
    .flatten()
    {
        let key = header_lookup_path(&candidate);
        if let Some(headers) = header_index.get(&key) {
            for header in headers {
                if !candidates.contains(header) {
                    candidates.push(header.clone());
                }
            }
        }
    }
    if let Some(headers) = header_index.get(&target) {
        for header in headers {
            if !candidates.contains(header) {
                candidates.push(header.clone());
            }
        }
    }
    candidates
}

pub(crate) fn lsp_language_id<'a>(server: &str, path: &Path, fallback: &'a str) -> &'a str {
    if server != "clangd" {
        return fallback;
    }
    if (is_cpp_header(path) || is_cpp_header_fragment(path))
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref()
            != Some("h")
    {
        return "cpp";
    }
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("c") => "c",
        Some("cc" | "cp" | "cpp" | "cxx" | "hh" | "hpp" | "hxx" | "ipp" | "tpp") => "cpp",
        _ => fallback,
    }
}

pub(crate) fn utf16_len(text: &str) -> u32 {
    text.encode_utf16().count() as u32
}

pub(crate) fn find_lsp_symbol_at_range<'a>(
    symbols: &'a [LspSymbol],
    range: &[i32],
) -> Option<&'a LspSymbol> {
    symbols
        .iter()
        .filter(|symbol| {
            let selection = [
                symbol.selection_line as i32,
                symbol.selection_character as i32,
                symbol.selection_line as i32,
                symbol.selection_character as i32,
            ];
            range_contains(range, &selection) || range_contains(&selection, range)
        })
        .min_by_key(|symbol| {
            range_span(&[
                symbol.range_start_line as i32,
                symbol.range_start_character as i32,
                symbol.range_end_line as i32,
                symbol.range_end_character as i32,
            ])
        })
}

#[cfg(test)]
mod tests {
    use super::{
        bundled_java_home, java_home_is_usable, java_language_server_settings,
        lsp_message_length_allowed, symbol_string, uri_to_relative_path, MAX_LSP_MESSAGE_BYTES,
    };
    use std::fs;
    use std::path::Path;

    #[test]
    fn lsp_message_size_is_bounded() {
        assert!(lsp_message_length_allowed(0));
        assert!(lsp_message_length_allowed(MAX_LSP_MESSAGE_BYTES));
        assert!(!lsp_message_length_allowed(MAX_LSP_MESSAGE_BYTES + 1));
    }

    #[test]
    fn lsp_symbol_identity_ignores_call_hierarchy_return_type_suffix() {
        assert_eq!(
            symbol_string("src/Client.java", "getOwner(int) : Mono<Owner>", 3, 8),
            symbol_string("src/Client.java", "getOwner(int)", 3, 8)
        );
        assert_eq!(
            symbol_string("src/Client.java", "getOwner(int)", 3, 8),
            "lsp . . . src.Client.java#getOwner(int)@3:8"
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_file_uri_drive_case_is_relative_to_the_workspace() {
        assert_eq!(
            uri_to_relative_path("file:///d:/Project/src/App.java", Path::new(r"D:\Project")),
            "src/App.java"
        );
    }

    #[test]
    fn bundled_java_home_supports_a_jdtls_bin_launcher() {
        let root =
            std::env::temp_dir().join(format!("code-memory-jdtls-runtime-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let launcher = root.join("jdtls").join("bin").join("jdtls.cmd");
        let runtime_bin = root.join("jdtls").join("runtime").join("bin");
        fs::create_dir_all(launcher.parent().expect("launcher parent")).expect("create launcher");
        fs::create_dir_all(&runtime_bin).expect("create runtime");
        fs::write(&launcher, b"launcher").expect("write launcher");
        assert_eq!(
            bundled_java_home(&launcher),
            Some(root.join("jdtls/runtime"))
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn java_source_only_settings_disable_build_importers() {
        let settings = java_language_server_settings(true);
        assert_eq!(
            settings.pointer("/java/import/gradle/enabled"),
            Some(&serde_json::Value::Bool(false))
        );
        assert_eq!(
            settings.pointer("/java/import/maven/enabled"),
            Some(&serde_json::Value::Bool(false))
        );
        assert_eq!(
            settings.pointer("/java/project/importOnFirstTimeStartup"),
            Some(&serde_json::Value::String("disabled".to_string()))
        );
    }

    #[test]
    fn java_home_requires_a_real_launcher() {
        let root =
            std::env::temp_dir().join(format!("code-memory-java-home-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        assert!(!java_home_is_usable(&root));
        let bin = root.join("bin");
        fs::create_dir_all(&bin).expect("create java bin");
        let executable = if cfg!(windows) { "java.exe" } else { "java" };
        fs::write(bin.join(executable), b"launcher").expect("write java launcher");
        assert!(java_home_is_usable(&root));
        let _ = fs::remove_dir_all(root);
    }
}
