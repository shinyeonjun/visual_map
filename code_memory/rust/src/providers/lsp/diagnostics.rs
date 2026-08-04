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
    matches!(language, "ruby" | "rust")
        || env::var("CODE_MEMORY_LSP_REFERENCES").as_deref() == Ok("1")
}

fn large_map_enrichment_language(language: &str) -> bool {
    matches!(
        language,
        "c" | "cpp" | "dart" | "go" | "java" | "python" | "ruby" | "rust"
    )
}

mod connection;
use connection::LspConnection;

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
