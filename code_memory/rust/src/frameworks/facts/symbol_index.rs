pub(crate) struct FrameworkSymbolIndex {
    definitions_by_file_name: HashMap<(String, String), Vec<String>>,
    references_by_file_name: HashMap<(String, String), Vec<String>>,
    definitions_by_file_line_name: HashMap<(String, usize, String), Vec<String>>,
    references_by_file_line_name: HashMap<(String, usize, String), Vec<(usize, String)>>,
    definitions_by_name: HashMap<String, Vec<String>>,
    defined: HashSet<String>,
    implementation_scores: HashMap<String, u8>,
    definition_locations: HashMap<String, (String, Vec<i32>)>,
}

pub(crate) fn build_framework_symbol_index(documents: &[DocumentOutput]) -> FrameworkSymbolIndex {
    let mut index = FrameworkSymbolIndex {
        definitions_by_file_name: HashMap::new(),
        references_by_file_name: HashMap::new(),
        definitions_by_file_line_name: HashMap::new(),
        references_by_file_line_name: HashMap::new(),
        definitions_by_name: HashMap::new(),
        defined: HashSet::new(),
        implementation_scores: HashMap::new(),
        definition_locations: HashMap::new(),
    };
    for document in documents {
        for occurrence in &document.occurrences {
            if occurrence.symbol.is_empty() {
                continue;
            }
            let name = symbol_short_name(&occurrence.symbol).to_string();
            let key = (document.path.clone(), name.clone());
            if occurrence.definition {
                index.defined.insert(occurrence.symbol.clone());
                index.definition_locations.insert(
                    occurrence.symbol.clone(),
                    (document.path.clone(), occurrence.range.clone()),
                );
                index
                    .definitions_by_file_name
                    .entry(key.clone())
                    .or_default()
                    .push(occurrence.symbol.clone());
                if let Some(line) = occurrence.range.first().copied() {
                    index
                        .definitions_by_file_line_name
                        .entry((document.path.clone(), line.max(0) as usize, name))
                        .or_default()
                        .push(occurrence.symbol.clone());
                }
                index
                    .definitions_by_name
                    .entry(key.1)
                    .or_default()
                    .push(occurrence.symbol.clone());
                index
                    .implementation_scores
                    .entry(occurrence.symbol.clone())
                    .or_insert_with(|| implementation_file_score(&occurrence.symbol));
            } else {
                if let Some(line) = occurrence.range.first().copied() {
                    let column =
                        occurrence.range.get(1).copied().unwrap_or_default().max(0) as usize;
                    index
                        .references_by_file_line_name
                        .entry((document.path.clone(), line.max(0) as usize, name.clone()))
                        .or_default()
                        .push((column, occurrence.symbol.clone()));
                }
                index
                    .references_by_file_name
                    .entry(key)
                    .or_default()
                    .push(occurrence.symbol.clone());
            }
        }
    }
    index
}

fn select_rightmost_reference(
    index: &FrameworkSymbolIndex,
    references: Option<&Vec<(usize, String)>>,
) -> Option<String> {
    let references = references?;
    let rightmost = references.iter().map(|(column, _)| *column).max()?;
    let symbols = references
        .iter()
        .filter(|(column, _)| *column == rightmost)
        .map(|(_, symbol)| symbol.clone())
        .collect::<Vec<_>>();
    select_indexed_symbol(index, Some(&symbols))
}

fn select_indexed_symbol(
    index: &FrameworkSymbolIndex,
    symbols: Option<&Vec<String>>,
) -> Option<String> {
    let symbols = symbols?;
    if symbols.len() == 1 {
        return symbols.first().cloned();
    }
    let unique = unique_symbols(symbols.clone());
    if unique.len() == 1 {
        return unique.into_iter().next();
    }
    let mut ranked = unique
        .iter()
        .map(|symbol| {
            (
                index
                    .implementation_scores
                    .get(symbol)
                    .copied()
                    .unwrap_or(0),
                symbol,
            )
        })
        .collect::<Vec<_>>();
    ranked.sort_by_key(|(score, symbol)| (*score, *symbol));
    let (best_score, best_symbol) = ranked.pop()?;
    if best_score == 0 || ranked.last().map(|(score, _)| *score) == Some(best_score) {
        return None;
    }
    Some(best_symbol.clone())
}

fn select_indexed_definition(
    index: &FrameworkSymbolIndex,
    symbols: Option<&Vec<String>>,
) -> Option<String> {
    let symbols = symbols?;
    // SCIP TypeScript emits parameter definitions as child symbols such as
    // `login().(loginDto)`. They are not competing method implementations.
    let method_symbols = symbols
        .iter()
        .filter(|symbol| !symbol.contains("().("))
        .cloned()
        .collect::<Vec<_>>();
    if method_symbols.is_empty() {
        select_indexed_symbol(index, Some(symbols))
    } else {
        select_indexed_symbol(index, Some(&method_symbols))
    }
}

pub(crate) fn project_symbol_is_defined_indexed(
    index: &FrameworkSymbolIndex,
    symbol: &str,
) -> bool {
    index.defined.contains(symbol)
}

pub(crate) fn resolve_symbol_indexed(
    index: &FrameworkSymbolIndex,
    path: &str,
    name: &str,
) -> Option<String> {
    let short_name = symbol_short_name(name).to_string();
    if let Some(symbol) = select_indexed_definition(
        index,
        index
            .definitions_by_file_name
            .get(&(path.to_string(), short_name.clone())),
    ) {
        return Some(symbol);
    }
    if let Some(symbol) = select_indexed_symbol(
        index,
        index
            .references_by_file_name
            .get(&(path.to_string(), short_name.clone())),
    ) {
        return Some(symbol);
    }
    select_indexed_definition(index, index.definitions_by_name.get(&short_name))
}

pub(crate) fn resolve_method_in_type_near_path_indexed(
    index: &FrameworkSymbolIndex,
    path: &str,
    type_name: &str,
    method_name: &str,
) -> Option<String> {
    let mut ranked = index
        .definitions_by_file_name
        .iter()
        .filter(|((_, name), _)| name == method_name)
        .flat_map(|((definition_path, _), symbols)| {
            symbols.iter().filter_map(move |symbol| {
                let (owner, _) = symbol.rsplit_once('#')?;
                let owner = owner.rsplit('/').next()?.trim_end_matches('`');
                owner.eq_ignore_ascii_case(type_name).then(|| {
                    (
                        source_path_distance(path, definition_path),
                        definition_path,
                        symbol,
                    )
                })
            })
        })
        .collect::<Vec<_>>();
    ranked.sort();
    ranked.dedup();
    let (distance, _, symbol) = ranked.first()?;
    if ranked.get(1).is_some_and(|(next, _, _)| next == distance) {
        return None;
    }
    Some((*symbol).clone())
}

fn source_path_distance(left: &str, right: &str) -> usize {
    let mut left = left.split(['/', '\\']).collect::<Vec<_>>();
    let mut right = right.split(['/', '\\']).collect::<Vec<_>>();
    left.pop();
    right.pop();
    let shared = left
        .iter()
        .zip(&right)
        .take_while(|(left, right)| left.eq_ignore_ascii_case(right))
        .count();
    left.len() + right.len() - shared * 2
}

pub(crate) fn resolve_symbol_at_indexed(
    index: &FrameworkSymbolIndex,
    path: &str,
    name: &str,
    source_line: usize,
) -> Option<String> {
    let short_name = symbol_short_name(name).to_string();
    if let Some(symbol) = select_indexed_definition(
        index,
        index.definitions_by_file_line_name.get(&(
            path.to_string(),
            source_line,
            short_name.clone(),
        )),
    ) {
        return Some(symbol);
    }
    if let Some(symbol) = select_indexed_symbol(
        index,
        index
            .references_by_file_name
            .get(&(path.to_string(), short_name.clone())),
    ) {
        return Some(symbol);
    }
    resolve_symbol_indexed(index, path, &short_name)
}

pub(crate) fn resolve_symbol_in_file_indexed(
    index: &FrameworkSymbolIndex,
    path: &str,
    name: &str,
    source_line: usize,
) -> Option<String> {
    let short_name = symbol_short_name(name).to_string();
    if let Some(symbol) = select_rightmost_reference(
        index,
        index.references_by_file_line_name.get(&(
            path.to_string(),
            source_line,
            short_name.clone(),
        )),
    ) {
        return Some(symbol);
    }
    if let Some(symbol) = select_indexed_definition(
        index,
        index.definitions_by_file_line_name.get(&(
            path.to_string(),
            source_line,
            short_name.clone(),
        )),
    ) {
        return Some(symbol);
    }
    if let Some(symbol) = select_indexed_symbol(
        index,
        index
            .references_by_file_name
            .get(&(path.to_string(), short_name.clone())),
    ) {
        return Some(symbol);
    }
    select_indexed_definition(
        index,
        index
            .definitions_by_file_name
            .get(&(path.to_string(), short_name)),
    )
}

pub(crate) fn resolve_symbol_on_line_indexed(
    index: &FrameworkSymbolIndex,
    path: &str,
    name: &str,
    source_line: usize,
) -> Option<String> {
    let short_name = symbol_short_name(name).to_string();
    select_rightmost_reference(
        index,
        index.references_by_file_line_name.get(&(
            path.to_string(),
            source_line,
            short_name.clone(),
        )),
    )
    .or_else(|| {
        select_indexed_definition(
            index,
            index
                .definitions_by_file_line_name
                .get(&(path.to_string(), source_line, short_name)),
        )
    })
}

pub(crate) fn resolve_go_method_indexed(
    index: &FrameworkSymbolIndex,
    name: &str,
    receiver_type: &str,
) -> Option<String> {
    let short_name = symbol_short_name(name);
    let candidates = index
        .definitions_by_name
        .get(short_name)?
        .iter()
        .filter(|symbol| go_method_receiver_type(symbol).as_deref() == Some(receiver_type))
        .cloned()
        .collect::<Vec<_>>();
    select_indexed_symbol(index, Some(&candidates))
}

pub(crate) fn resolve_nested_definition_indexed(
    index: &FrameworkSymbolIndex,
    owner: &str,
    name: &str,
) -> Option<String> {
    let (owner_path, owner_range) = index.definition_locations.get(owner)?;
    let owner_start = *owner_range.first()?;
    let owner_end = if owner_range.len() >= 4 {
        owner_range[2]
    } else {
        owner_start
    };
    let candidates = index
        .definitions_by_file_name
        .get(&(owner_path.clone(), name.to_string()))?
        .iter()
        .filter(|symbol| {
            index
                .definition_locations
                .get(*symbol)
                .is_some_and(|(_, range)| {
                    let start = range.first().copied().unwrap_or(-1);
                    let end = if range.len() >= 4 { range[2] } else { start };
                    start >= owner_start && end <= owner_end
                })
        })
        .cloned()
        .collect::<Vec<_>>();
    select_indexed_definition(index, Some(&candidates))
}

pub(crate) fn nested_definition_symbols_indexed(
    index: &FrameworkSymbolIndex,
    owner: &str,
) -> Vec<String> {
    let Some((owner_path, owner_range)) = index.definition_locations.get(owner) else {
        return Vec::new();
    };
    let Some(owner_start) = owner_range.first().copied() else {
        return Vec::new();
    };
    let owner_end = if owner_range.len() >= 4 {
        owner_range[2]
    } else {
        owner_start
    };
    let mut symbols = index
        .definitions_by_file_name
        .iter()
        .filter(|((path, _), _)| path == owner_path)
        .flat_map(|(_, candidates)| candidates.iter())
        .filter(|symbol| *symbol != owner)
        .filter(|symbol| {
            index
                .definition_locations
                .get(*symbol)
                .is_some_and(|(_, range)| {
                    let start = range.first().copied().unwrap_or(-1);
                    let end = if range.len() >= 4 { range[2] } else { start };
                    start >= owner_start && end <= owner_end
                })
        })
        .cloned()
        .collect::<Vec<_>>();
    symbols.sort();
    symbols.dedup();
    symbols
}

fn go_method_receiver_type(symbol: &str) -> Option<String> {
    let (_, suffix) = symbol.rsplit_once('#')?;
    let open = suffix.find('(')?;
    let close = suffix[open..].find(").")? + open;
    let receiver = suffix[open + 1..close].trim().trim_start_matches('*');
    (!receiver.is_empty()).then(|| receiver.to_string())
}

pub(crate) fn project_definition_for_symbol_indexed(
    index: &FrameworkSymbolIndex,
    symbol: &str,
) -> Option<String> {
    if index.defined.contains(symbol) {
        return Some(symbol.to_string());
    }
    let name = symbol_short_name(symbol);
    let owner = symbol.rsplit_once('/').map(|(owner, _)| owner)?;
    let candidates = index.definitions_by_name.get(name)?;
    let matching_owner = candidates
        .iter()
        .filter(|candidate| candidate.rsplit_once('/').map(|(value, _)| value) == Some(owner))
        .cloned()
        .collect::<Vec<_>>();
    select_indexed_symbol(index, Some(&matching_owner))
}

pub(crate) fn resolve_java_type_indexed(
    index: &FrameworkSymbolIndex,
    path: &str,
    name: &str,
) -> Option<String> {
    let candidates = index
        .definitions_by_file_name
        .get(&(path.to_string(), name.to_string()))?;
    let type_definitions = candidates
        .iter()
        .filter(|symbol| {
            symbol
                .split('@')
                .next()
                .and_then(|value| value.rsplit_once('#'))
                .is_some_and(|(_, short_name)| short_name == name)
        })
        .cloned()
        .collect::<Vec<_>>();
    select_indexed_symbol(index, Some(&type_definitions))
}
