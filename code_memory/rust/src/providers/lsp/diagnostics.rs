const MAX_LSP_MESSAGE_BYTES: usize = 128 * 1024 * 1024;

fn default_lsp_request_timeout() -> Duration {
    // Bundled providers can spend most of 30 seconds on a cold runtime or the
    // first project load. The session-wide budget still bounds a stalled run.
    Duration::from_secs(60)
}

pub(crate) fn lsp_request_timeout() -> Duration {
    env::var("CODE_MEMORY_LSP_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| (1_000..=300_000).contains(value))
        .map(Duration::from_millis)
        .unwrap_or_else(default_lsp_request_timeout)
}

pub(crate) fn lsp_max_requests() -> usize {
    env::var("CODE_MEMORY_LSP_MAX_REQUESTS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| (100..=5_000_000).contains(value))
        .unwrap_or(100_000)
}

pub(crate) fn lsp_request_batch_size() -> usize {
    env::var("CODE_MEMORY_LSP_BATCH_SIZE")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| (1..=64).contains(value))
        .unwrap_or(16)
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
    language == "rust"
        || env::var("CODE_MEMORY_LSP_REFERENCES").as_deref() == Ok("1")
}

fn large_map_enrichment_language(language: &str) -> bool {
    matches!(
        language,
        "c" | "cpp" | "dart" | "go" | "java" | "python" | "rust"
    )
}

mod connection;
use connection::LspConnection;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LspSymbolParent {
    pub(crate) name: String,
    pub(crate) selection_line: u32,
    pub(crate) selection_character: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// Exact parent from a hierarchical `DocumentSymbol` response. Flat
    /// `SymbolInformation` responses intentionally leave this absent because
    /// `containerName` alone cannot identify an overloaded/local symbol.
    pub(crate) parent: Option<LspSymbolParent>,
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
    lsp_failure_code(error).is_some()
}

pub(crate) fn lsp_failure_code(error: &str) -> Option<DiagnosticCode> {
    if error.contains("native LSP session timeout")
        || error.contains("native LSP response timeout")
        || error.contains("native LSP request budget exceeded")
    {
        Some(DiagnosticCode::ProviderTimeout)
    } else if error.contains("native LSP closed stdout")
        || error.contains("Broken pipe")
        || error.contains("pipe is being closed")
    {
        Some(DiagnosticCode::ProviderStopped)
    } else {
        None
    }
}

fn is_recoverable_lsp_query_error(error: &str) -> bool {
    error.contains("native LSP response timeout")
}

fn is_recoverable_lsp_session_error(error: &str) -> bool {
    is_recoverable_lsp_query_error(error) || is_fatal_lsp_error(error)
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
    collect_lsp_symbols_with_parent(value, output, None);
}

fn collect_lsp_symbols_with_parent(
    value: &Value,
    output: &mut Vec<LspSymbol>,
    parent: Option<LspSymbolParent>,
) {
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
    let current = LspSymbolParent {
        name: name.to_string(),
        selection_line: selection[0] as u32,
        selection_character: selection[1] as u32,
    };
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
        parent,
    });
    if let Some(children) = value.get("children").and_then(Value::as_array) {
        for child in children {
            collect_lsp_symbols_with_parent(child, output, Some(current.clone()));
        }
    }
}

pub(crate) fn canonicalize_lsp_symbols(symbols: &mut Vec<LspSymbol>) {
    symbols.sort_by(|left, right| {
        lsp_symbol_identity(left)
            .cmp(&lsp_symbol_identity(right))
            .then_with(|| left.name.cmp(&right.name))
    });
    let mut canonical = Vec::<LspSymbol>::with_capacity(symbols.len());
    for symbol in symbols.drain(..) {
        if let Some(existing) = canonical
            .last_mut()
            .filter(|existing| lsp_symbol_identity(existing) == lsp_symbol_identity(&symbol))
        {
            if symbol.name.len() < existing.name.len() {
                existing.name = symbol.name.clone();
            }
            if existing.parent.is_none() {
                existing.parent = symbol.parent;
            }
            if existing.detail.is_none() {
                existing.detail = symbol.detail;
            }
            let existing_span = (
                existing.range_end_line.saturating_sub(existing.range_start_line),
                existing
                    .range_end_character
                    .saturating_sub(existing.range_start_character),
            );
            let candidate_span = (
                symbol.range_end_line.saturating_sub(symbol.range_start_line),
                symbol
                    .range_end_character
                    .saturating_sub(symbol.range_start_character),
            );
            if candidate_span > existing_span {
                existing.range_start_line = symbol.range_start_line;
                existing.range_start_character = symbol.range_start_character;
                existing.range_end_line = symbol.range_end_line;
                existing.range_end_character = symbol.range_end_character;
            }
            continue;
        }
        canonical.push(symbol);
    }
    *symbols = canonical;
}

/// Repairs Java document symbols only when the provider's full declaration
/// range and the source contain exactly one matching declared name. JDTLS can
/// occasionally report a method selection at 0:0 while its declaration range
/// is correct. A malformed point must never become a confirmed definition or
/// target, but throwing away a uniquely source-backed declaration would also
/// lose valid flow edges.
pub(crate) fn repair_java_lsp_symbol_selections(
    source: &str,
    symbols: &mut Vec<LspSymbol>,
) -> (usize, usize) {
    let mut repaired = 0usize;
    let before = symbols.len();
    symbols.retain_mut(|symbol| {
        if !is_callable_or_type_kind(symbol.kind) {
            return true;
        }
        let name = lsp_symbol_base_name(&symbol.name);
        if name.is_empty() {
            return false;
        }
        if java_symbol_selection_matches_source(source, symbol, name) {
            return true;
        }
        // JDTLS can also return a correct source-backed selection with a
        // malformed declaration end such as `139:1..0:0`. Keep the provider's
        // exact name only when the selection is at or after the reported
        // declaration start and the declaration range itself is impossible.
        // This deliberately does not rescue a valid declaration whose
        // selection points elsewhere (for example `setTarget` reported at
        // 0:0): that could be a use-site and must remain unresolved.
        if java_symbol_selection_is_exact_after_malformed_start(source, symbol, name) {
            symbol.range_end_line = symbol.selection_line;
            symbol.range_end_character = symbol
                .selection_character
                .saturating_add(name.encode_utf16().count() as u32);
            repaired += 1;
            return true;
        }
        let matches = java_symbol_name_matches_in_declaration(source, symbol, name);
        let [(line, character)] = matches.as_slice() else {
            return false;
        };
        symbol.selection_line = *line;
        symbol.selection_character = *character;
        repaired += 1;
        true
    });
    (repaired, before.saturating_sub(symbols.len()))
}

fn java_symbol_selection_is_exact_after_malformed_start(
    source: &str,
    symbol: &LspSymbol,
    name: &str,
) -> bool {
    if java_symbol_declaration_range_is_valid(source, symbol) {
        return false;
    }
    let selection_at_or_after_start = (symbol.selection_line, symbol.selection_character)
        >= (symbol.range_start_line, symbol.range_start_character);
    selection_at_or_after_start
        && source_line_has_java_name(source, symbol.selection_line, symbol.selection_character, name)
}

fn java_symbol_declaration_range_is_valid(source: &str, symbol: &LspSymbol) -> bool {
    if (symbol.range_end_line, symbol.range_end_character)
        < (symbol.range_start_line, symbol.range_start_character)
    {
        return false;
    }
    let Some(start) = source.lines().nth(symbol.range_start_line as usize) else {
        return false;
    };
    let Some(end) = source.lines().nth(symbol.range_end_line as usize) else {
        return false;
    };
    utf16_column_to_byte(start, symbol.range_start_character as usize).is_some()
        && utf16_column_to_byte(end, symbol.range_end_character as usize).is_some()
}

fn java_symbol_selection_matches_source(source: &str, symbol: &LspSymbol, name: &str) -> bool {
    let selection = [
        symbol.selection_line as i32,
        symbol.selection_character as i32,
        symbol.selection_line as i32,
        symbol.selection_character as i32,
    ];
    let declaration = [
        symbol.range_start_line as i32,
        symbol.range_start_character as i32,
        symbol.range_end_line as i32,
        symbol.range_end_character as i32,
    ];
    range_contains(&declaration, &selection)
        && source_line_has_java_name(source, symbol.selection_line, symbol.selection_character, name)
}

fn java_symbol_name_matches_in_declaration(
    source: &str,
    symbol: &LspSymbol,
    name: &str,
) -> Vec<(u32, u32)> {
    let mut matches = Vec::new();
    for (line_index, line) in source.lines().enumerate().skip(symbol.range_start_line as usize) {
        if line_index > symbol.range_end_line as usize {
            break;
        }
        for (byte, _) in line.match_indices(name) {
            let before = line[..byte].chars().next_back();
            let after = line[byte + name.len()..].chars().next();
            if before.is_some_and(java_identifier_continue)
                || after.is_some_and(java_identifier_continue)
            {
                continue;
            }
            let character = line[..byte].encode_utf16().count() as u32;
            let point = [line_index as i32, character as i32, line_index as i32, character as i32];
            let declaration = [
                symbol.range_start_line as i32,
                symbol.range_start_character as i32,
                symbol.range_end_line as i32,
                symbol.range_end_character as i32,
            ];
            if range_contains(&declaration, &point) {
                matches.push((line_index as u32, character));
            }
        }
    }
    matches
}

fn source_line_has_java_name(source: &str, line: u32, character: u32, name: &str) -> bool {
    let Some(line) = source.lines().nth(line as usize) else {
        return false;
    };
    let Some(byte) = utf16_column_to_byte(line, character as usize) else {
        return false;
    };
    let Some(remainder) = line.get(byte..) else {
        return false;
    };
    if !remainder.starts_with(name) {
        return false;
    }
    let before = line[..byte].chars().next_back();
    let after = remainder[name.len()..].chars().next();
    !before.is_some_and(java_identifier_continue) && !after.is_some_and(java_identifier_continue)
}

fn utf16_column_to_byte(line: &str, target: usize) -> Option<usize> {
    let mut utf16 = 0usize;
    for (byte, character) in line.char_indices() {
        if utf16 == target {
            return Some(byte);
        }
        utf16 += character.len_utf16();
        if utf16 > target {
            return None;
        }
    }
    (utf16 == target).then_some(line.len())
}

fn java_identifier_continue(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '_' | '$')
}

pub(crate) fn reconcile_lsp_symbol_owners(language: &str, symbols: &mut [LspSymbol]) {
    let type_symbols = symbols
        .iter()
        .filter(|symbol| is_type_hierarchy_kind(symbol.kind))
        .map(|symbol| {
            (
                normalized_owner_type_name(&symbol.name),
                LspSymbolParent {
                    name: symbol.name.clone(),
                    selection_line: symbol.selection_line,
                    selection_character: symbol.selection_character,
                },
            )
        })
        .collect::<Vec<_>>();

    for symbol in symbols {
        let requested_owner = match language {
            "go" if symbol.parent.is_none() && symbol.kind == 6 => {
                go_receiver_type(&symbol.name)
            }
            "rust" if symbol.kind == 6 => symbol
                .parent
                .as_ref()
                .and_then(|parent| rust_impl_type(&parent.name)),
            _ => None,
        };
        let Some(requested_owner) = requested_owner else {
            continue;
        };
        let mut matches = type_symbols
            .iter()
            .filter(|(name, _)| name == &requested_owner)
            .map(|(_, parent)| parent);
        let Some(parent) = matches.next() else {
            continue;
        };
        if matches.next().is_none() {
            symbol.parent = Some(parent.clone());
        }
    }
}

fn go_receiver_type(name: &str) -> Option<String> {
    let receiver = name.strip_prefix('(')?.split_once(").")?.0;
    Some(normalized_owner_type_name(
        receiver.trim_start_matches('*'),
    ))
}

fn rust_impl_type(name: &str) -> Option<String> {
    let mut declaration = name.trim().strip_prefix("impl ")?.trim();
    if declaration.starts_with('<') {
        let end = matching_generic_end(declaration, '<', '>')?;
        declaration = declaration[end + 1..].trim();
    }
    let implemented = declaration
        .rsplit_once(" for ")
        .map(|(_, implemented)| implemented)
        .unwrap_or(declaration);
    Some(normalized_owner_type_name(implemented))
}

fn normalized_owner_type_name(name: &str) -> String {
    let name = name
        .split_whitespace()
        .next()
        .unwrap_or(name)
        .trim_start_matches(['&', '*'])
        .rsplit("::")
        .next()
        .unwrap_or(name);
    let generic = name.find(['<', '[']).unwrap_or(name.len());
    name[..generic].trim().to_string()
}

fn matching_generic_end(value: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    for (index, character) in value.char_indices() {
        if character == open {
            depth += 1;
        } else if character == close {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

fn lsp_symbol_identity(symbol: &LspSymbol) -> (u32, u32, u32) {
    (
        symbol.selection_line,
        symbol.selection_character,
        symbol.kind,
    )
}

pub(crate) fn parse_lsp_range(value: &Value) -> Option<Vec<i32>> {
    Some(vec![
        value.get("start")?.get("line")?.as_i64()? as i32,
        value.get("start")?.get("character")?.as_i64()? as i32,
        value.get("end")?.get("line")?.as_i64()? as i32,
        value.get("end")?.get("character")?.as_i64()? as i32,
    ])
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
        6 => Kind::Method,
        9 => Kind::Constructor,
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
