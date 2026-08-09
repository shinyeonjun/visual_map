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
    let target = range_parts(range)?;
    symbols
        .iter()
        .filter_map(|symbol| {
            let selection = [
                symbol.selection_line as i32,
                symbol.selection_character as i32,
                symbol.selection_line as i32,
                symbol.selection_character as i32,
            ];
            let definition = [
                symbol.range_start_line as i32,
                symbol.range_start_character as i32,
                symbol.range_end_line as i32,
                symbol.range_end_character as i32,
            ];
            let definition_parts = range_parts(&definition)?;
            // A selection/name position outside its own declaration is an
            // invalid provider symbol. JDTLS has emitted this for a method as
            // selection 0:0 with a real declaration at lines 47-55. Treating
            // the impossible point as an exact target can turn
            // `new ClassEmitter(...)` into a confirmed call to `setTarget`.
            // Abstain instead of guessing another symbol by file/name.
            if !range_contains(&definition, &selection) {
                return None;
            }
            let span = range_span(&definition);
            let rank = if (symbol.selection_line as i32, symbol.selection_character as i32)
                == (target.0, target.1)
            {
                // LocationLink.targetSelectionRange and ordinary identifier
                // locations both start at the declared symbol name.
                (0, 0, 0, span.2, span.3)
            } else if definition_parts == target {
                // Some servers return the complete declaration range. This is
                // the exact owner and must beat every nested symbol inside it.
                (1, 0, 0, span.2, span.3)
            } else if range_contains(&definition, range) {
                // The declaration encloses a narrower provider location. Pick
                // the smallest enclosing declaration.
                (2, span.0, span.1, span.2, span.3)
            } else if range_contains(range, &selection) {
                // Last-resort compatibility for a provider range that encloses
                // several selections. Prefer the largest declaration inside
                // that range; choosing the smallest is what previously mapped
                // a method location to an unrelated nested symbol.
                (3, -span.0, -span.1, span.2, span.3)
            } else {
                return None;
            };
            Some((rank, symbol))
        })
        .min_by_key(|(rank, _)| *rank)
        .map(|(_, symbol)| symbol)
}
