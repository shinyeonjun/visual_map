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

pub(crate) fn large_workspace_workload(
    server: &str,
    source_files: usize,
    semantic_query_symbols: usize,
) -> bool {
    source_files > 500
        || (matches!(server, "clangd" | "gopls") && source_files > 250)
        // Per-symbol LSP calls dominate runtime even when a project keeps a
        // large amount of code in relatively few files.
        || semantic_query_symbols > 500
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
    // `pub` on an impl method is a real Rust visibility boundary. Requiring
    // column zero here silently discarded public APIs such as
    // `Runtime::spawn` in large workspaces because methods are indented inside
    // an `impl` block. The provider already gave us the exact symbol and source
    // location, so retaining it is not a name-based guess.
    visible
}

fn declaration_has_modifier(text: &str, modifier: &str) -> bool {
    text.split(|character: char| !character.is_alphanumeric() && character != '_')
        .any(|token| token == modifier)
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
        // Dart uses underscore visibility for the common public API.
        "dart" => !symbol.name.starts_with('_'),
        // Java package-private declarations are still useful module API. Only
        // explicit private members are cut from the large-workspace pass.
        "java" => {
            !declaration_has_modifier(declaration, "private")
                && !declaration_has_modifier(source_line, "private")
        }
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

pub(crate) fn large_symbol_call_priority(
    language: &str,
    relative_path: &str,
    source: &str,
    symbol: &LspSymbol,
) -> u8 {
    if language != "java" {
        return 0;
    }
    let declaration = symbol.detail.as_deref().unwrap_or_default();
    let source_line = source
        .lines()
        .nth(symbol.selection_line as usize)
        .unwrap_or_default();
    let explicit_api = symbol.name == "main"
        || ["public", "protected"].iter().any(|modifier| {
            declaration_has_modifier(declaration, modifier)
                || declaration_has_modifier(source_line, modifier)
        });
    let test_scope = relative_path.split('/').any(|segment| {
        matches!(
            segment.to_ascii_lowercase().as_str(),
            "test" | "tests" | "integration-tests" | "src-test"
        )
    });
    u8::from(!explicit_api) + 2 * u8::from(test_scope)
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct LargeCallSiteQuery {
    pub(crate) priority: u8,
    pub(crate) group: String,
    pub(crate) uri: String,
    pub(crate) line: u32,
    pub(crate) character: u32,
}

pub(crate) fn large_call_query_group(relative_path: &str) -> String {
    let parts = relative_path
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let Some(first) = parts.first().copied() else {
        return ".".to_string();
    };
    if matches!(first, "apps" | "crates" | "libs" | "modules" | "packages") {
        return parts
            .get(1)
            .map(|second| format!("{first}/{second}"))
            .unwrap_or_else(|| first.to_string());
    }
    if matches!(first, "src" | "test" | "tests") {
        ".".to_string()
    } else {
        first.to_string()
    }
}

pub(crate) fn lsp_symbol_base_name(name: &str) -> &str {
    name.split(['(', '<', ':'])
        .next()
        .unwrap_or(name)
        .trim()
}

/// Selects a deterministic, priority-ordered call slice without allowing one
/// alphabetically early module to consume the whole request budget. Within a
/// priority band every source group receives one query before any group
/// receives its next query.
pub(crate) fn fair_large_call_site_queries(
    mut candidates: Vec<LargeCallSiteQuery>,
    limit: usize,
) -> Vec<(String, u32, u32)> {
    candidates.sort();
    candidates.dedup_by(|left, right| {
        left.uri == right.uri
            && left.line == right.line
            && left.character == right.character
    });

    let mut by_group = std::collections::BTreeMap::<String, Vec<LargeCallSiteQuery>>::new();
    for candidate in candidates {
        by_group
            .entry(candidate.group.clone())
            .or_default()
            .push(candidate);
    }
    for candidates in by_group.values_mut() {
        candidates.sort_by(|left, right| {
            (left.priority, &left.uri, left.line, left.character).cmp(&(
                right.priority,
                &right.uri,
                right.line,
                right.character,
            ))
        });
    }

    // Seed every file/module group with its best candidate before applying the
    // global priority bands. This prevents a repository with many public calls
    // from starving files whose only executable owner is package-private.
    let mut selected = Vec::new();
    let mut seeded = HashSet::new();
    for (group, candidates) in &by_group {
        let Some(candidate) = candidates.first() else {
            continue;
        };
        selected.push((candidate.uri.clone(), candidate.line, candidate.character));
        seeded.insert((
            group.clone(),
            candidate.uri.clone(),
            candidate.line,
            candidate.character,
        ));
        if selected.len() == limit {
            return selected;
        }
    }

    let mut remaining = std::collections::BTreeMap::<
        (u8, String),
        Vec<(String, u32, u32)>,
    >::new();
    for (group, candidates) in by_group {
        for candidate in candidates {
            if seeded.contains(&(
                group.clone(),
                candidate.uri.clone(),
                candidate.line,
                candidate.character,
            )) {
                continue;
            }
            remaining
                .entry((candidate.priority, group.clone()))
                .or_default()
                .push((candidate.uri, candidate.line, candidate.character));
        }
    }
    let mut grouped = remaining
        .into_iter()
        .map(|((priority, group), queries)| (priority, group, queries, 0usize))
        .collect::<Vec<_>>();
    let mut priorities = grouped
        .iter()
        .map(|(priority, _, _, _)| *priority)
        .collect::<Vec<_>>();
    priorities.sort();
    priorities.dedup();

    // `usize::MAX` means "order every candidate, then let the caller split
    // the shared request budget". Reserving that sentinel value attempts an
    // impossible allocation and panics before a single Java query is sent.
    // The groups already own the remaining allocations; grow `selected`
    // normally so an unbounded logical limit never becomes a memory size.
    for priority in priorities {
        loop {
            let mut progressed = false;
            for (group_priority, _, queries, cursor) in &mut grouped {
                if *group_priority != priority || *cursor >= queries.len() {
                    continue;
                }
                selected.push(queries[*cursor].clone());
                *cursor += 1;
                progressed = true;
                if selected.len() == limit {
                    return selected;
                }
            }
            if !progressed {
                break;
            }
        }
    }
    selected
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
