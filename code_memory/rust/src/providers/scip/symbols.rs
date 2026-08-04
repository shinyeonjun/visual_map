fn is_type_symbol_kind(
    kind: protobuf::EnumOrUnknown<scip::types::symbol_information::Kind>,
    symbol: &str,
) -> bool {
    use scip::types::symbol_information::Kind;
    match kind.enum_value() {
        Ok(
            Kind::Class
            | Kind::Struct
            | Kind::Interface
            | Kind::Enum
            | Kind::TypeAlias
            | Kind::TypeParameter,
        ) => true,
        Ok(Kind::UnspecifiedKind) | Err(_) => {
            normalized_scip_symbol_kind("UnspecifiedKind".to_string(), symbol) == "Type"
        }
        _ => false,
    }
}

pub(crate) fn normalized_scip_symbol_kind(provider_kind: String, symbol: &str) -> String {
    if !matches!(provider_kind.as_str(), "Unspecified" | "UnspecifiedKind") {
        return provider_kind;
    }
    let Ok(parsed) = scip::symbol::parse_symbol(symbol) else {
        return "Unspecified".to_string();
    };
    let Some(descriptor) = parsed.descriptors.last() else {
        return "Unspecified".to_string();
    };
    use scip::types::descriptor::Suffix;
    match descriptor.suffix.enum_value() {
        Ok(Suffix::Type) => "Type",
        Ok(Suffix::Method) if matches!(descriptor.name.as_str(), ".ctor" | ".cctor" | "<init>") => {
            "Constructor"
        }
        Ok(Suffix::Method) => "Method",
        Ok(Suffix::Term) => "Field",
        Ok(Suffix::Namespace | Suffix::Package) => "Namespace",
        Ok(Suffix::TypeParameter) => "TypeParameter",
        Ok(Suffix::Parameter | Suffix::Local) => "Variable",
        Ok(Suffix::Macro) => "Macro",
        Ok(Suffix::Meta | Suffix::UnspecifiedSuffix) | Err(_) => "Unspecified",
    }
    .to_string()
}

pub(crate) fn inferred_scip_enclosing_symbol(symbol: &str, kind: &str) -> Option<String> {
    if !is_type_member_kind(kind) {
        return None;
    }
    let mut parsed = scip::symbol::parse_symbol(symbol).ok()?;
    (parsed.descriptors.len() > 1).then(|| {
        parsed.descriptors.pop();
        scip::symbol::format_symbol(parsed)
    })
}

pub(crate) fn reparent_scip_symbol(symbol: &str, parent: &str) -> Option<String> {
    let child = scip::symbol::parse_symbol(symbol).ok()?;
    let descriptor = child.descriptors.last()?.clone();
    let mut parent = scip::symbol::parse_symbol(parent).ok()?;
    parent.descriptors.push(descriptor);
    Some(scip::symbol::format_symbol(parent))
}

fn canonical_scip_symbol(
    symbol: &str,
    local_aliases: Option<&HashMap<String, String>>,
    unique_aliases: &HashMap<String, String>,
) -> String {
    local_aliases
        .and_then(|aliases| aliases.get(symbol))
        .or_else(|| unique_aliases.get(symbol))
        .cloned()
        .unwrap_or_else(|| symbol.to_string())
}

fn is_type_member_kind(kind: &str) -> bool {
    matches!(kind, "Method" | "Constructor" | "Field")
}

fn is_non_visual_symbol(symbol: &str) -> bool {
    symbol.starts_with("local ") || symbol.is_empty()
}

fn read_scoped_scip_documents(
    path: &Path,
    project_root: &Path,
    allowed_paths: &HashSet<String>,
) -> Result<Vec<scip::types::Document>, String> {
    let file = fs::File::open(path)
        .map_err(|e| format!("cannot read SCIP file {}: {e}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut input = protobuf::CodedInputStream::from_buf_read(&mut reader);
    let mut documents = Vec::new();
    while let Some(tag) = input
        .read_raw_tag_or_eof()
        .map_err(|e| format!("invalid SCIP file {}: {e}", path.display()))?
    {
        match tag {
            10 => {
                let _: scip::types::Metadata = input
                    .read_message()
                    .map_err(|e| format!("invalid SCIP metadata {}: {e}", path.display()))?;
            }
            18 => {
                let document: scip::types::Document = input
                    .read_message()
                    .map_err(|e| format!("invalid SCIP document {}: {e}", path.display()))?;
                let relative = normalize_scip_path(&document.relative_path, project_root);
                if allowed_paths.contains(&relative) {
                    documents.push(document);
                }
            }
            26 => {
                let _: scip::types::SymbolInformation = input
                    .read_message()
                    .map_err(|e| format!("invalid SCIP external symbol {}: {e}", path.display()))?;
            }
            other => protobuf::rt::skip_field_for_tag(other, &mut input)
                .map_err(|e| format!("invalid SCIP field {}: {e}", path.display()))?,
        }
    }
    Ok(documents)
}

fn type_script_call_occurrence(
    call_ranges: &Option<CallRangesByPath>,
    path: &str,
    occurrence: &[i32],
) -> bool {
    let Some(range) = range_parts(occurrence) else {
        return false;
    };
    call_ranges
        .as_ref()
        .and_then(|ranges| ranges.get(path))
        .is_some_and(|ranges| ranges.contains(&range))
}

pub(crate) fn normalize_scip_language(raw: &str, fallback_language: &str) -> String {
    let language = raw.trim();
    // ponytail: some providers serialize SCIP's language enum as a number; use the
    // language worker's authoritative id instead of leaking that internal value.
    if language.is_empty() || language.chars().all(|character| character.is_ascii_digit()) {
        fallback_language.to_string()
    } else {
        language.to_string()
    }
}

pub(crate) fn normalize_scip_path(raw: &str, project_root: &Path) -> String {
    let normalized = raw.replace('\\', "/");
    let path = Path::new(&normalized);
    if path.is_absolute() {
        if let Ok(relative) = path.strip_prefix(project_root) {
            return relative.to_string_lossy().replace('\\', "/");
        }
        if let (Ok(path), Ok(root)) = (path.canonicalize(), project_root.canonicalize()) {
            if let Ok(relative) = path.strip_prefix(root) {
                return relative.to_string_lossy().replace('\\', "/");
            }
        }
    }
    normalized
}

struct DefinitionRangeIndex {
    entries: Vec<(usize, (i32, i32, i32, i32))>,
}

impl DefinitionRangeIndex {
    fn from_definitions(definitions: &[(String, Vec<i32>)]) -> Self {
        let mut entries: Vec<_> = definitions
            .iter()
            .enumerate()
            .filter_map(|(index, (_, range))| range_parts(range).map(|range| (index, range)))
            .collect();
        entries.sort_by_key(|(_, range)| (range.0, range.1, range.2, range.3));
        Self { entries }
    }

    fn find(
        &self,
        definitions: &[(String, Vec<i32>)],
        enclosing_range: &[i32],
        occurrence_range: &[i32],
    ) -> Option<String> {
        let target = range_parts(if enclosing_range.is_empty() {
            occurrence_range
        } else {
            enclosing_range
        })?;
        let definition_inside_enclosing = !enclosing_range.is_empty();
        let upper = self
            .entries
            .partition_point(|(_, range)| (range.0, range.1) <= (target.2, target.3));
        if definition_inside_enclosing {
            let lower = self
                .entries
                .partition_point(|(_, range)| (range.0, range.1) < (target.0, target.1));
            self.entries[lower..upper]
                .iter()
                .filter(|(_, range)| {
                    (target.0, target.1) <= (range.0, range.1)
                        && (range.2, range.3) <= (target.2, target.3)
                })
                .min_by_key(|(_, range)| (range.2 - range.0, range.3 - range.1))
                .and_then(|(index, _)| definitions.get(*index).map(|(symbol, _)| symbol.clone()))
        } else {
            self.entries[..upper]
                .iter()
                .filter(|(_, range)| {
                    (range.0, range.1) <= (target.0, target.1)
                        && (target.2, target.3) <= (range.2, range.3)
                })
                .min_by_key(|(_, range)| (range.2 - range.0, range.3 - range.1))
                .and_then(|(index, _)| definitions.get(*index).map(|(symbol, _)| symbol.clone()))
        }
    }
}

fn find_definition_for_enclosing_range_index(
    definitions: Option<&DefinitionRangeIndex>,
    definition_values: Option<&Vec<(String, Vec<i32>)>>,
    enclosing_range: &[i32],
    occurrence_range: &[i32],
) -> Option<String> {
    definitions?.find(definition_values?, enclosing_range, occurrence_range)
}

pub(crate) fn range_span(range: &[i32]) -> (i32, i32, i32, i32) {
    let Some(range) = range_parts(range) else {
        return (i32::MAX, i32::MAX, i32::MAX, i32::MAX);
    };
    (range.2 - range.0, range.3 - range.1, range.0, range.1)
}

pub(crate) fn range_parts(range: &[i32]) -> Option<(i32, i32, i32, i32)> {
    match range {
        [line, start, end] => Some((*line, *start, *line, *end)),
        [start_line, start_character, end_line, end_character, ..] => {
            Some((*start_line, *start_character, *end_line, *end_character))
        }
        _ => None,
    }
}

pub(crate) fn is_call_occurrence(source: Option<&str>, range: &[i32]) -> bool {
    if range.len() < 3 {
        return false;
    }
    let Some(line) = source.and_then(|source| source.lines().nth(range[0] as usize)) else {
        return false;
    };
    let start_character = range[1].max(0) as usize;
    let end_character = if range.len() >= 4 { range[3] } else { range[2] };
    if line
        .get(start_character..end_character.max(0) as usize)
        .map(|segment| segment.contains('('))
        .unwrap_or(false)
    {
        return true;
    }
    line.get(end_character.max(0) as usize..)
        .map(|suffix| suffix.trim_start().starts_with('('))
        .unwrap_or(false)
}

pub(crate) fn is_ruby_member_call_occurrence(source: Option<&str>, range: &[i32]) -> bool {
    let Some(source) = source else {
        return false;
    };
    let Some((line_number, start, end_line, end)) = range_parts(range) else {
        return false;
    };
    if end_line != line_number {
        return false;
    }
    let Some(line) = source.lines().nth(line_number.max(0) as usize) else {
        return false;
    };
    let start = start.max(0) as usize;
    let end = end.max(0) as usize;
    let Some(prefix) = line.get(..start) else {
        return false;
    };
    let Some(token) = line.get(start..end) else {
        return false;
    };
    if token.is_empty()
        || !token.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '!' | '?' | '=')
        })
    {
        return false;
    }
    let prefix = prefix.trim_end();
    prefix.ends_with('.') || prefix.ends_with("&.") || prefix.ends_with("::")
}

pub(crate) fn find_source_owner(
    owner_scopes: Option<&Vec<(String, Vec<i32>)>>,
    occurrence_range: &[i32],
) -> Option<String> {
    owner_scopes?
        .iter()
        .filter_map(|(symbol, scope)| {
            range_contains(scope, occurrence_range).then_some((symbol, scope))
        })
        .min_by_key(|(_, scope)| range_span(scope))
        .map(|(symbol, _)| symbol.clone())
}

pub(crate) fn source_scopes(
    source: &str,
    definitions: &[(String, Vec<i32>)],
) -> Vec<(String, Vec<i32>)> {
    let lines: Vec<&str> = source.lines().collect();
    definitions
        .iter()
        .filter_map(|(symbol, definition_range)| {
            source_scope_from_lines(&lines, definition_range).map(|scope| (symbol.clone(), scope))
        })
        .collect()
}

pub(crate) fn source_scope_from_lines(
    lines: &[&str],
    definition_range: &[i32],
) -> Option<Vec<i32>> {
    // ponytail: lexical brace fallback; provider enclosing ranges remain authoritative.
    let (definition_line, definition_character, _, _) = range_parts(definition_range)?;
    let mut opening = None;
    for (line_number, full_line) in lines.iter().enumerate().skip(definition_line as usize) {
        let start = if line_number == definition_line as usize {
            definition_character.max(0) as usize
        } else {
            0
        };
        let line = full_line.get(start..)?;
        let terminator = [line.find(';'), line.find("=>")]
            .into_iter()
            .flatten()
            .min();
        if let Some(brace) = line.find('{') {
            if terminator.is_none() || Some(brace) < terminator {
                opening = Some((line_number, start + brace));
                break;
            }
        }
        if terminator.is_some() {
            return None;
        }
    }
    let (opening_line, opening_character) = opening?;
    let mut depth = 0i32;
    for (line_number, full_line) in lines.iter().enumerate().skip(opening_line) {
        let line = if line_number == opening_line {
            full_line.get(opening_character..)?
        } else {
            full_line
        };
        for character in line.chars() {
            match character {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(vec![
                            definition_line,
                            definition_character,
                            line_number as i32,
                            lines[line_number].chars().count() as i32,
                        ]);
                    }
                }
                _ => {}
            }
        }
    }
    None
}

pub(crate) fn has_role(value: i32, role: SymbolRole) -> bool {
    value & role as i32 != 0
}

