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
