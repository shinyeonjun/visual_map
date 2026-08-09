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
    source: Option<&str>,
) -> bool {
    let Some(range) = range_parts(occurrence) else {
        return false;
    };
    match call_ranges.as_ref().and_then(|ranges| ranges.get(path)) {
        Some(ranges) => ranges.contains(&range),
        None => is_call_occurrence(source, occurrence),
    }
}

#[cfg(test)]
mod call_detection_tests {
    use super::*;

    #[test]
    fn typescript_falls_back_when_project_model_omits_a_file() {
        let ranges = Some(HashMap::from([(
            "modeled.ts".to_string(),
            HashSet::from([(0, 0, 0, 3)]),
        )]));
        assert!(type_script_call_occurrence(
            &ranges,
            "fallback.ts",
            &[0, 0, 3],
            Some("run()")
        ));
    }
}

pub(crate) fn normalize_scip_language(raw: &str, fallback_language: &str) -> String {
    let language = raw.trim();
    // ponytail: some providers serialize SCIP's language enum as a number; use the
    // language worker's authoritative id instead of leaking that internal value.
    if language.is_empty() || language.chars().all(|character| character.is_ascii_digit()) {
        return fallback_language.to_string();
    }

    if language.eq_ignore_ascii_case("c#") {
        return "csharp".to_string();
    }
    if language.eq_ignore_ascii_case("c++") {
        return "cpp".to_string();
    }

    // Provider display names are not the product language IDs. In particular,
    // scip-dotnet has emitted `C#`, while the analysis plan and Language IR use
    // `csharp`. Keep this normalization closed to the ten-language contract;
    // an unknown value falls back to the worker that launched the provider
    // instead of creating a thirteenth, unplanned language partition.
    let compact = language
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    match compact.as_str() {
        "typescript" | "ts" => "typescript",
        "javascript" | "js" => "javascript",
        "python" | "py" => "python",
        "java" => "java",
        "csharp" | "cs" => "csharp",
        "c" => "c",
        "cplusplus" | "cpp" | "cxx" => "cpp",
        "go" | "golang" => "go",
        "rust" => "rust",
        "dart" => "dart",
        _ => fallback_language,
    }
    .to_string()
}

#[cfg(test)]
mod language_normalization_tests {
    use super::normalize_scip_language;

    #[test]
    fn provider_display_names_map_to_closed_product_language_ids() {
        assert_eq!(normalize_scip_language("C#", "csharp"), "csharp");
        assert_eq!(normalize_scip_language("C++", "cpp"), "cpp");
        assert_eq!(normalize_scip_language("TypeScript", "typescript"), "typescript");
        assert_eq!(normalize_scip_language("Golang", "go"), "go");
    }

    #[test]
    fn unknown_or_numeric_provider_language_uses_the_launched_worker() {
        assert_eq!(normalize_scip_language("17", "java"), "java");
        assert_eq!(normalize_scip_language("provider-private", "rust"), "rust");
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

/// Deterministic interval lookup used while converting large SCIP indexes.
///
/// SCIP occurrences are already sorted by the provider, but repeatedly
/// scanning every source-side syntax site turns conversion into O(n*m).  This
/// index only prunes ranges that cannot overlap the query.  A match is still
/// selected by the original source-site index, so replacing the linear scan
/// cannot change which fact wins when ranges overlap.
struct SourceRangeIndex {
    entries: Vec<(usize, SourceRange)>,
    prefix_max_end: Vec<(i32, i32)>,
}

impl SourceRangeIndex {
    fn from_ranges<'a>(ranges: impl IntoIterator<Item = (usize, &'a [i32])>) -> Self {
        let mut entries = ranges
            .into_iter()
            .filter_map(|(index, range)| range_parts(range).map(|range| (index, range)))
            .collect::<Vec<_>>();
        entries.sort_by_key(|(index, range)| (range.0, range.1, range.2, range.3, *index));

        let mut prefix_max_end = Vec::with_capacity(entries.len());
        let mut maximum = (i32::MIN, i32::MIN);
        for (_, range) in &entries {
            maximum = maximum.max((range.2, range.3));
            prefix_max_end.push(maximum);
        }
        Self {
            entries,
            prefix_max_end,
        }
    }

    fn first_bidirectional_containment(&self, target: &[i32]) -> Option<usize> {
        let target = range_parts(target)?;
        self.overlapping_indices(target)
            .filter(|(_, candidate)| {
                range_parts_contains(*candidate, target)
                    || range_parts_contains(target, *candidate)
            })
            .map(|(index, _)| index)
            .min()
    }

    fn smallest_container(&self, target: &[i32]) -> Option<usize> {
        self.smallest_container_by_key(target, |index, range| {
            (
                range.2 - range.0,
                range.3 - range.1,
                range.0,
                range.1,
                index,
            )
        })
    }

    fn smallest_container_by_key<K: Ord>(
        &self,
        target: &[i32],
        mut key: impl FnMut(usize, SourceRange) -> K,
    ) -> Option<usize> {
        let target = range_parts(target)?;
        self.overlapping_indices(target)
            .filter(|(_, candidate)| range_parts_contains(*candidate, target))
            .min_by_key(|(index, range)| {
                key(*index, *range)
            })
            .map(|(index, _)| index)
    }

    fn overlapping_indices(
        &self,
        target: SourceRange,
    ) -> impl Iterator<Item = (usize, SourceRange)> + '_ {
        let target_start = (target.0, target.1);
        let target_end = (target.2, target.3);
        let upper = self
            .entries
            .partition_point(|(_, range)| (range.0, range.1) <= target_end);
        let mut cursor = upper;
        std::iter::from_fn(move || {
            while cursor > 0 {
                cursor -= 1;
                if self.prefix_max_end[cursor] < target_start {
                    return None;
                }
                let (index, range) = self.entries[cursor];
                if (range.2, range.3) >= target_start {
                    return Some((index, range));
                }
            }
            None
        })
    }
}

#[derive(Default)]
struct ExactSourceRangeIndex {
    first_by_range: HashMap<SourceRange, usize>,
}

impl ExactSourceRangeIndex {
    fn insert(&mut self, index: usize, range: &[i32]) {
        let Some(range) = range_parts(range) else {
            return;
        };
        self.first_by_range
            .entry(range)
            .and_modify(|current| *current = (*current).min(index))
            .or_insert(index);
    }

    fn find(&self, range: &[i32]) -> Option<usize> {
        self.first_by_range.get(&range_parts(range)?).copied()
    }
}

fn range_parts_contains(container: SourceRange, nested: SourceRange) -> bool {
    (container.0, container.1) <= (nested.0, nested.1)
        && (nested.2, nested.3) <= (container.2, container.3)
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

#[cfg(test)]
pub(crate) fn find_source_owner(
    owner_scopes: Option<&Vec<(String, Vec<i32>)>>,
    occurrence_range: &[i32],
) -> Option<String> {
    owner_scopes?
        .iter()
        .filter_map(|(symbol, scope)| {
            range_parts(scope)
                .zip(range_parts(occurrence_range))
                .is_some_and(|(scope, occurrence)| range_parts_contains(scope, occurrence))
                .then_some((symbol, scope))
        })
        .min_by_key(|(_, scope)| range_span(scope))
        .map(|(symbol, _)| symbol.clone())
}

fn find_source_owner_indexed(
    owner_scopes: Option<&Vec<(String, Vec<i32>)>>,
    owner_index: Option<&SourceRangeIndex>,
    occurrence_range: &[i32],
) -> Option<String> {
    let scopes = owner_scopes?;
    owner_index?
        .smallest_container(occurrence_range)
        .and_then(|index| scopes.get(index))
        .map(|(symbol, _)| symbol.clone())
}

#[cfg(test)]
mod source_range_index_tests {
    use super::*;

    #[test]
    fn indexed_containment_preserves_the_first_linear_match() {
        let ranges = [
            vec![4, 2, 4, 18],
            vec![4, 8, 4, 12],
            vec![4, 8, 4, 12],
            vec![8, 0, 9, 4],
        ];
        let index = SourceRangeIndex::from_ranges(
            ranges
                .iter()
                .enumerate()
                .map(|(position, range)| (position, range.as_slice())),
        );

        assert_eq!(index.first_bidirectional_containment(&[4, 9, 4, 11]), Some(0));
        assert_eq!(index.first_bidirectional_containment(&[8, 2, 8, 3]), Some(3));
        assert_eq!(index.first_bidirectional_containment(&[20, 0, 20, 1]), None);
    }

    #[test]
    fn indexed_owner_selection_matches_the_smallest_linear_scope() {
        let scopes = vec![
            ("outer".to_string(), vec![1, 0, 20, 1]),
            ("inner".to_string(), vec![5, 2, 9, 3]),
            ("sibling".to_string(), vec![11, 0, 14, 1]),
        ];
        let index = SourceRangeIndex::from_ranges(
            scopes
                .iter()
                .enumerate()
                .map(|(position, (_, range))| (position, range.as_slice())),
        );

        let linear = find_source_owner(Some(&scopes), &[6, 1, 6, 2]);
        let indexed = find_source_owner_indexed(Some(&scopes), Some(&index), &[6, 1, 6, 2]);
        assert_eq!(linear, Some("inner".to_string()));
        assert_eq!(indexed, linear);
    }

    #[test]
    fn exact_index_keeps_the_earliest_site_across_utf_encodings() {
        let mut index = ExactSourceRangeIndex::default();
        index.insert(4, &[2, 3, 2, 7]);
        index.insert(1, &[2, 3, 2, 7]);
        index.insert(3, &[9, 0, 9, 2]);
        assert_eq!(index.find(&[2, 3, 7]), Some(1));
        assert_eq!(index.find(&[9, 0, 9, 2]), Some(3));
    }

    #[test]
    fn interval_index_matches_exhaustive_linear_queries() {
        let mut ranges = Vec::new();
        for start_line in 0..4 {
            for end_line in start_line..4 {
                ranges.push(vec![start_line, 1, end_line, 6]);
            }
        }
        // Preserve a duplicate to exercise the original `.find()` tie rule.
        ranges.push(vec![1, 1, 2, 6]);
        let index = SourceRangeIndex::from_ranges(
            ranges
                .iter()
                .enumerate()
                .map(|(position, range)| (position, range.as_slice())),
        );

        for start_line in 0..5 {
            for end_line in start_line..5 {
                let query = vec![start_line, 2, end_line, 5];
                let expected = ranges.iter().position(|candidate| {
                    range_parts(candidate)
                        .zip(range_parts(&query))
                        .is_some_and(|(candidate, query)| {
                            range_parts_contains(candidate, query)
                                || range_parts_contains(query, candidate)
                        })
                });
                assert_eq!(
                    index.first_bidirectional_containment(&query),
                    expected,
                    "query={query:?}"
                );
            }
        }
    }
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
