#[derive(Clone)]
struct ProviderDefinition {
    symbol: String,
    language: String,
    path: String,
    range: Vec<i32>,
    kind: String,
    name: Option<String>,
    declaration: String,
    has_body: bool,
}

fn reconcile_syntax_relations(
    protocol: ProviderProtocol,
    documents: &[DocumentOutput],
    relations: &mut Vec<RelationOutput>,
    syntax_sites: &HashMap<(String, String), Vec<SyntaxCallSite>>,
    source_cache: &HashMap<String, String>,
) {
    let definitions = provider_definitions(protocol, documents, source_cache);
    let definitions_by_symbol = definitions
        .iter()
        .map(|definition| (definition.symbol.clone(), definition.clone()))
        .collect::<HashMap<_, _>>();
    let definitions_by_location = provider_definitions_by_location(&definitions);
    let type_definitions_by_path_and_name = definitions
        .iter()
        .filter(|definition| is_type_definition(definition))
        .filter_map(|definition| {
            Some((
                (definition.path.clone(), definition.name.clone()?),
                definition,
            ))
        })
        .fold(HashMap::<_, Vec<_>>::new(), |mut index, (key, definition)| {
            index.entry(key).or_default().push(definition);
            index
        });
    let constructor_symbols = definitions
        .iter()
        .filter(|definition| {
            is_constructor_definition_indexed(definition, &type_definitions_by_path_and_name)
        })
        .map(|definition| definition.symbol.clone())
        .collect::<HashSet<_>>();
    let syntax_site_indexes = syntax_sites
        .iter()
        .map(|(key, sites)| {
            let mut index = ExactSourceRangeIndex::default();
            for (site_index, site) in sites.iter().enumerate() {
                index.insert(site_index, &site.callee_utf8_range);
                index.insert(site_index, &site.callee_utf16_range);
            }
            (key.clone(), index)
        })
        .collect::<HashMap<_, _>>();
    let document_languages = documents
        .iter()
        .map(|document| (document.path.clone(), document.language.clone()))
        .collect::<HashMap<_, _>>();

    let mut normalized = Vec::with_capacity(relations.len());
    for mut relation in relations.drain(..) {
        if !definitions_by_symbol.contains_key(&relation.to) {
            if let Some(canonical_target) = canonical_target_by_provider_location_indexed(
                &relation.to,
                &definitions_by_location,
            ) {
                relation.to = canonical_target;
                relation.strategy = Some("provider-location-canonicalized".to_string());
            }
        }
        if relation.kind == "CALLS"
            && definitions_by_symbol
                .get(&relation.to)
                .is_some_and(|definition| {
                    is_type_definition(definition)
                        || constructor_symbols.contains(&definition.symbol)
                })
        {
            relation.kind = "CONSTRUCTS".to_string();
        }
        if !is_executable_relation(&relation.kind) {
            normalized.push(relation);
            continue;
        }
        let relation_language = document_languages.get(&relation.path);
        if !relation_language.is_some_and(|language| {
            matches!(
                language.as_str(),
                "csharp" | "java" | "c" | "cpp" | "go" | "rust"
            )
        }) {
            normalized.push(relation);
            continue;
        }
        let sites = relation_language
            .and_then(|language| syntax_sites.get(&(language.clone(), relation.path.clone())))
            .map(Vec::as_slice)
            .unwrap_or_default();

        if let Some(site) = relation_language
            .and_then(|language| {
                syntax_site_indexes.get(&(language.clone(), relation.path.clone()))
            })
            .and_then(|index| index.find(&relation.range))
            .and_then(|index| sites.get(index))
        {
            relation.kind = relation_kind_for_site(site).to_string();
            relation.range = site.callee_range(protocol);
            normalized.push(relation);
            continue;
        }

        let mut containing_sites = sites
            .iter()
            .filter(|site| site.expression_contains_provider_range(&relation.range))
            .collect::<Vec<_>>();
        containing_sites.sort_by_key(|site| range_measure(site.expression_range(protocol)));
        if let Some(site) = containing_sites.first().copied().filter(|site| {
            containing_sites.get(1).is_none_or(|next| {
                range_measure(site.expression_range(protocol))
                    < range_measure(next.expression_range(protocol))
            })
        }) {
            relation.kind = relation_kind_for_site(site).to_string();
            relation.range = site.callee_range(protocol);
            relation.strategy = Some("syntax-call-site-reconciled".to_string());
            normalized.push(relation);
            continue;
        }

        let source_definition = definitions_by_symbol.get(&relation.from);
        let target_definition = definitions_by_symbol.get(&relation.to);
        if source_definition.is_some_and(is_callable_definition)
            && target_definition.is_some_and(is_callable_definition)
            && same_definition_name(source_definition, target_definition)
            && definition_range_matches(source_definition, &relation.path, &relation.range)
        {
            relation.kind = "IMPLEMENTATION".to_string();
            relation.path = source_definition.expect("checked above").path.clone();
            relation.range.clear();
            relation.strategy = Some("provider-binding-not-call".to_string());
            normalized.push(relation);
            continue;
        }

        // The provider proved a symbol reference, but source syntax did not
        // prove an executable site. Preserve the weaker fact and never publish
        // it as a confirmed call.
        relation.kind = "REFERENCES".to_string();
        relation.strategy = Some("provider-reference-syntax-rejected-call".to_string());
        normalized.push(relation);
    }

    synthesize_unique_csharp_constructions(
        protocol,
        documents,
        &definitions,
        syntax_sites,
        &mut normalized,
    );
    *relations = enforce_one_target_per_call_site(normalized, &definitions_by_symbol);
}

fn provider_definitions(
    protocol: ProviderProtocol,
    documents: &[DocumentOutput],
    source_cache: &HashMap<String, String>,
) -> Vec<ProviderDefinition> {
    let mut symbol_kinds = HashMap::<String, String>::new();
    let document_languages = documents
        .iter()
        .map(|document| (document.path.clone(), document.language.clone()))
        .collect::<HashMap<_, _>>();
    for document in documents {
        for symbol in &document.symbols {
            symbol_kinds
                .entry(symbol.symbol.clone())
                .or_insert_with(|| symbol.kind.clone());
        }
    }

    let mut ranges = HashMap::<(String, String), Vec<Vec<i32>>>::new();
    for document in documents {
        for occurrence in document
            .occurrences
            .iter()
            .filter(|occurrence| occurrence.definition)
        {
            ranges
                .entry((document.path.clone(), occurrence.symbol.clone()))
                .or_default()
                .push(occurrence.range.clone());
        }
    }
    // Some SCIP indexers omit enclosing ranges. Reconstruct the lexical
    // definition scopes from the source so call sites can still acquire an
    // exact caller without falling back to a name-only relation.
    for document in documents {
        let source = source_cache
            .get(&document.path)
            .map(String::as_str)
            .unwrap_or_default();
        let source_definitions = document
            .occurrences
            .iter()
            .filter(|occurrence| occurrence.definition)
            .map(|occurrence| (occurrence.symbol.clone(), occurrence.range.clone()))
            .collect::<Vec<_>>();
        for (symbol, scope) in source_scopes(source, &source_definitions) {
            ranges
                .entry((document.path.clone(), symbol))
                .or_default()
                .push(scope);
        }
    }

    let mut output = Vec::new();
    for ((path, symbol), mut candidates) in ranges {
        candidates.sort_by_key(|range| range_measure(range));
        let Some(range) = candidates.last().cloned() else {
            continue;
        };
        let source = source_cache.get(&path).map(String::as_str).unwrap_or_default();
        let name = candidates
            .iter()
            .filter_map(|candidate| source_slice(source, candidate, protocol))
            .map(str::trim)
            .filter(|candidate| is_source_identifier(candidate))
            .min_by_key(|candidate| candidate.len())
            .map(str::to_string)
            .or_else(|| provider_symbol_short_name(&symbol));
        let kind = symbol_kinds
            .get(&symbol)
            .cloned()
            .unwrap_or_else(|| "Unspecified".to_string());
        let has_body = is_callable_kind_name(&kind)
            && candidates.iter().any(|candidate| {
                source_slice(source, candidate, protocol).is_some_and(|text| text.contains('{'))
            });
        output.push(ProviderDefinition {
            symbol,
            language: document_languages.get(&path).cloned().unwrap_or_default(),
            path,
            declaration: source_slice(source, &range, protocol)
                .unwrap_or_default()
                .to_string(),
            range,
            kind,
            name,
            has_body,
        });
    }
    output
}

fn synthesize_unique_csharp_constructions(
    protocol: ProviderProtocol,
    documents: &[DocumentOutput],
    definitions: &[ProviderDefinition],
    syntax_sites: &HashMap<(String, String), Vec<SyntaxCallSite>>,
    relations: &mut Vec<RelationOutput>,
) {
    let executable_ranges = relations
        .iter()
        .filter(|relation| is_executable_relation(&relation.kind))
        .filter_map(|relation| {
            range_parts(&relation.range).map(|range| (relation.path.clone(), range))
        })
        .collect::<HashSet<_>>();
    let callable_definitions_by_path = definitions
        .iter()
        .filter(|definition| is_callable_definition(definition))
        .fold(HashMap::<String, Vec<_>>::new(), |mut index, definition| {
            index
                .entry(definition.path.clone())
                .or_default()
                .push(definition);
            index
        });
    let callable_indexes_by_path = callable_definitions_by_path
        .iter()
        .map(|(path, candidates)| {
            (
                path.clone(),
                SourceRangeIndex::from_ranges(
                    candidates
                        .iter()
                        .enumerate()
                        .map(|(index, definition)| (index, definition.range.as_slice())),
                ),
            )
        })
        .collect::<HashMap<_, _>>();
    let type_definitions_by_namespace_and_name = definitions
        .iter()
        .filter(|definition| is_type_definition(definition))
        .filter_map(|definition| {
            Some((
                (
                    provider_namespace(&definition.symbol)?.to_string(),
                    definition.name.clone()?,
                ),
                definition,
            ))
        })
        .fold(HashMap::<_, Vec<_>>::new(), |mut index, (key, definition)| {
            index.entry(key).or_default().push(definition);
            index
        });

    for document in documents
        .iter()
        .filter(|document| document.language == "csharp")
    {
        let Some(sites) = syntax_sites.get(&(document.language.clone(), document.path.clone()))
        else {
            continue;
        };
        for site in sites
            .iter()
            .filter(|site| site.form == CallSiteForm::Construct)
        {
            let already_resolved = [&site.callee_utf8_range, &site.callee_utf16_range]
                .into_iter()
                .filter_map(|range| range_parts(range))
                .any(|range| executable_ranges.contains(&(document.path.clone(), range)));
            if already_resolved {
                continue;
            }
            let site_range = site.callee_range(protocol);
            let Some(caller) = callable_indexes_by_path
                .get(&document.path)
                .zip(callable_definitions_by_path.get(&document.path))
                .and_then(|(index, candidates)| {
                    index
                        .smallest_container_by_key(&site_range, |position, range| {
                            (
                                i64::from(range.2 - range.0) * 1_000_000
                                    + i64::from(range.3 - range.1),
                                position,
                            )
                        })
                        .and_then(|position| candidates.get(position).copied())
                })
            else {
                continue;
            };
            let Some(caller_namespace) = provider_namespace(&caller.symbol) else {
                continue;
            };
            let targets = type_definitions_by_namespace_and_name
                .get(&(caller_namespace.to_string(), site.callee_text.clone()))
                .map(Vec::as_slice)
                .unwrap_or_default();
            if targets.len() != 1 {
                continue;
            }
            relations.push(RelationOutput {
                from: caller.symbol.clone(),
                to: targets[0].symbol.clone(),
                kind: "CONSTRUCTS".to_string(),
                path: document.path.clone(),
                range: site_range,
                confidence: Some(1.0),
                strategy: Some("syntax-same-namespace-unique-type".to_string()),
            });
        }
    }
}

fn enforce_one_target_per_call_site(
    relations: Vec<RelationOutput>,
    definitions: &HashMap<String, ProviderDefinition>,
) -> Vec<RelationOutput> {
    let mut non_executable = Vec::new();
    let mut groups = HashMap::<(String, Vec<i32>, String), Vec<RelationOutput>>::new();
    for relation in relations {
        if is_executable_relation(&relation.kind) {
            groups
                .entry((
                    relation.path.clone(),
                    canonical_range(&relation.range),
                    relation.kind.clone(),
                ))
                .or_default()
                .push(relation);
        } else {
            non_executable.push(relation);
        }
    }

    for mut candidates in groups.into_values() {
        candidates.sort_by(|left, right| left.to.cmp(&right.to));
        candidates.dedup_by(|left, right| left.from == right.from && left.to == right.to);
        let winner = if candidates.len() == 1 {
            Some(0)
        } else {
            unique_candidate_index(&candidates, |candidate| {
                definitions
                    .get(&candidate.to)
                    .is_some_and(|definition| definition.has_body)
            })
            .or_else(|| {
                unique_candidate_index(&candidates, |candidate| {
                    definitions.contains_key(&candidate.to)
                })
            })
        };
        let Some(winner) = winner else {
            // Ambiguous provider targets are not guessed into a confirmed
            // relation. Their source occurrences remain available as syntax
            // coverage for a typed unresolved gap in the canonical layer.
            continue;
        };
        let selected = candidates.remove(winner);
        if let Some(concrete) = definitions.get(&selected.to).filter(|value| value.has_body) {
            for rejected in &candidates {
                let Some(abstract_target) = definitions.get(&rejected.to) else {
                    continue;
                };
                if is_callable_definition(abstract_target)
                    && !abstract_target.has_body
                    && concrete.name.is_some()
                    && concrete.name == abstract_target.name
                {
                    non_executable.push(RelationOutput {
                        from: concrete.symbol.clone(),
                        to: abstract_target.symbol.clone(),
                        kind: "IMPLEMENTATION".to_string(),
                        path: concrete.path.clone(),
                        range: Vec::new(),
                        confidence: Some(1.0),
                        strategy: Some("call-target-binding".to_string()),
                    });
                }
            }
        }
        non_executable.push(selected);
    }
    non_executable
}

fn unique_candidate_index(
    candidates: &[RelationOutput],
    predicate: impl Fn(&RelationOutput) -> bool,
) -> Option<usize> {
    let mut matches = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| predicate(candidate).then_some(index));
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn relation_kind_for_site(site: &SyntaxCallSite) -> &'static str {
    match site.form {
        CallSiteForm::Construct => "CONSTRUCTS",
        CallSiteForm::Call | CallSiteForm::MethodCall => "CALLS",
    }
}

fn is_executable_relation(kind: &str) -> bool {
    matches!(kind, "CALLS" | "CONSTRUCTS")
}

fn is_callable_definition(definition: &ProviderDefinition) -> bool {
    is_callable_kind_name(&definition.kind)
}

fn is_callable_kind_name(kind: &str) -> bool {
    matches!(
        kind.to_ascii_lowercase().as_str(),
        "function" | "method" | "constructor" | "macro"
    )
}

fn is_type_definition(definition: &ProviderDefinition) -> bool {
    matches!(
        definition.kind.to_ascii_lowercase().as_str(),
        "type" | "class" | "struct" | "interface" | "enum" | "trait"
    )
}

fn is_constructor_definition_indexed(
    definition: &ProviderDefinition,
    type_definitions_by_path_and_name: &HashMap<
        (String, String),
        Vec<&ProviderDefinition>,
    >,
) -> bool {
    if definition.kind.eq_ignore_ascii_case("constructor")
        || definition.symbol.contains("<constructor>")
        || definition.symbol.contains("`.ctor`")
        || definition.symbol.contains("<init>")
        || definition.name.as_deref() == Some("constructor")
    {
        return true;
    }
    definition.kind.eq_ignore_ascii_case("method")
        && definition.name.as_ref().is_some_and(|name| {
            type_definitions_by_path_and_name
                .get(&(definition.path.clone(), name.clone()))
                .is_some_and(|candidates| {
                    candidates.iter().any(|candidate| {
                        range_contains_coordinates(&candidate.range, &definition.range)
                    })
                })
                && constructor_declaration_shape(
                    &definition.language,
                    name,
                    &definition.declaration,
                )
        })
}

#[cfg(test)]
fn is_constructor_definition(
    definition: &ProviderDefinition,
    definitions: &[ProviderDefinition],
) -> bool {
    let type_definitions_by_path_and_name = definitions
        .iter()
        .filter(|candidate| is_type_definition(candidate))
        .filter_map(|candidate| {
            Some(((candidate.path.clone(), candidate.name.clone()?), candidate))
        })
        .fold(HashMap::<_, Vec<_>>::new(), |mut index, (key, candidate)| {
            index.entry(key).or_default().push(candidate);
            index
        });
    is_constructor_definition_indexed(definition, &type_definitions_by_path_and_name)
}

fn constructor_declaration_shape(language: &str, name: &str, declaration: &str) -> bool {
    let Some(line) = declaration.lines().find(|line| {
        line.match_indices(name).any(|(index, _)| {
            identifier_boundary(line, index, name.len())
                && line[index + name.len()..].trim_start().starts_with('(')
        })
    }) else {
        return false;
    };
    let Some((index, _)) = line.match_indices(name).find(|(index, _)| {
        identifier_boundary(line, *index, name.len())
            && line[*index + name.len()..].trim_start().starts_with('(')
    }) else {
        return false;
    };
    let prefix = line[..index].trim();
    match language {
        "java" => declaration_prefix_is_only_modifiers(
            prefix,
            &["public", "protected", "private"],
            true,
        ),
        "dart" => declaration_prefix_is_only_modifiers(
            prefix,
            &["const", "factory", "external"],
            false,
        ),
        _ => true,
    }
}

fn identifier_boundary(line: &str, start: usize, length: usize) -> bool {
    let before = line[..start].chars().next_back();
    let after = line[start + length..].chars().next();
    before.is_none_or(|character| !(character == '_' || character.is_alphanumeric()))
        && after.is_none_or(|character| !(character == '_' || character.is_alphanumeric()))
}

fn declaration_prefix_is_only_modifiers(
    mut prefix: &str,
    modifiers: &[&str],
    allow_type_parameters: bool,
) -> bool {
    loop {
        prefix = prefix.trim_start();
        if prefix.is_empty() {
            return true;
        }
        if allow_type_parameters && prefix.starts_with('<') {
            let Some(end) = matching_angle_end(prefix) else {
                return false;
            };
            prefix = &prefix[end + 1..];
            continue;
        }
        let token_end = prefix
            .find(char::is_whitespace)
            .unwrap_or(prefix.len());
        let token = &prefix[..token_end];
        if modifiers.contains(&token) || token.starts_with('@') {
            prefix = &prefix[token_end..];
            continue;
        }
        return false;
    }
}

fn matching_angle_end(value: &str) -> Option<usize> {
    let mut depth = 0_u32;
    for (index, character) in value.char_indices() {
        match character {
            '<' => depth += 1,
            '>' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn same_definition_name(
    left: Option<&ProviderDefinition>,
    right: Option<&ProviderDefinition>,
) -> bool {
    left.zip(right)
        .is_some_and(|(left, right)| left.name.is_some() && left.name == right.name)
}

fn definition_range_matches(
    definition: Option<&ProviderDefinition>,
    path: &str,
    range: &[i32],
) -> bool {
    definition.is_some_and(|definition| {
        definition.path == path && range_contains_coordinates(&definition.range, range)
    })
}

fn provider_namespace(symbol: &str) -> Option<&str> {
    let type_or_member = symbol.split('#').next()?;
    type_or_member.rsplit_once('/').map(|(namespace, _)| namespace)
}

type ProviderLocationKey = (String, u32, u32, String);

fn provider_definitions_by_location(
    definitions: &[ProviderDefinition],
) -> HashMap<ProviderLocationKey, Vec<String>> {
    let mut index = HashMap::<ProviderLocationKey, Vec<String>>::new();
    for definition in definitions {
        let Some((prefix, line, column, name)) = provider_symbol_location(&definition.symbol)
        else {
            continue;
        };
        index
            .entry((prefix.to_string(), line, column, name.to_string()))
            .or_default()
            .push(definition.symbol.clone());
    }
    index
}

fn canonical_target_by_provider_location_indexed(
    raw: &str,
    definitions: &HashMap<ProviderLocationKey, Vec<String>>,
) -> Option<String> {
    let (raw_prefix, raw_line, raw_column, raw_name) = provider_symbol_location(raw)?;
    let candidates = definitions.get(&(
        raw_prefix.to_string(),
        raw_line,
        raw_column,
        raw_name.to_string(),
    ))?;
    (candidates.len() == 1).then(|| candidates[0].clone())
}

fn provider_symbol_location(symbol: &str) -> Option<(&str, u32, u32, &str)> {
    let (head, location) = symbol.rsplit_once('@')?;
    let (line, column) = location.split_once(':')?;
    let prefix = head.split('#').next()?;
    let name = head
        .split('#')
        .nth(1)?
        .rsplit(['.', ')'])
        .find(|part| !part.is_empty())?
        .trim_start_matches('(');
    Some((prefix, line.parse().ok()?, column.parse().ok()?, name))
}

fn provider_symbol_short_name(symbol: &str) -> Option<String> {
    if let Some((_, provider_name)) = symbol.split_once('#') {
        let provider_name = provider_name.split('@').next().unwrap_or(provider_name);
        let provider_name = provider_name
            .rsplit_once(").")
            .map(|(_, name)| name)
            .unwrap_or(provider_name);
        let provider_name = provider_name
            .split(['(', '<', '.', '['])
            .next()
            .unwrap_or(provider_name)
            .trim_matches('`');
        if is_source_identifier(provider_name) {
            return Some(provider_name.to_string());
        }
    }
    let type_name = symbol
        .split('#')
        .next()?
        .rsplit('/')
        .next()?
        .trim_matches('`');
    is_source_identifier(type_name).then(|| type_name.to_string())
}

fn canonical_range(range: &[i32]) -> Vec<i32> {
    match range {
        [line, start, end] => vec![*line, *start, *line, *end],
        [start_line, start_column, end_line, end_column, ..] => {
            vec![*start_line, *start_column, *end_line, *end_column]
        }
        _ => range.to_vec(),
    }
}

fn coordinate_bounds(range: &[i32]) -> Option<((i32, i32), (i32, i32))> {
    let range = canonical_range(range);
    match range.as_slice() {
        [start_line, start_column, end_line, end_column] => {
            Some(((*start_line, *start_column), (*end_line, *end_column)))
        }
        _ => None,
    }
}

fn range_contains_coordinates(outer: &[i32], inner: &[i32]) -> bool {
    let Some((outer_start, outer_end)) = coordinate_bounds(outer) else {
        return false;
    };
    let Some((inner_start, inner_end)) = coordinate_bounds(inner) else {
        return false;
    };
    outer_start <= inner_start && inner_end <= outer_end
}

fn range_measure(range: &[i32]) -> i64 {
    let Some((start, end)) = coordinate_bounds(range) else {
        return i64::MAX;
    };
    i64::from(end.0 - start.0) * 1_000_000 + i64::from(end.1 - start.1)
}

fn source_slice<'a>(
    source: &'a str,
    range: &[i32],
    protocol: ProviderProtocol,
) -> Option<&'a str> {
    let ((start_line, start_column), (end_line, end_column)) = coordinate_bounds(range)?;
    if [start_line, start_column, end_line, end_column]
        .iter()
        .any(|value| *value < 0)
    {
        return None;
    }
    let line_starts = source_line_starts(source);
    let start = source_offset(
        source,
        &line_starts,
        start_line as usize,
        start_column as usize,
        protocol,
    )?;
    let end = source_offset(
        source,
        &line_starts,
        end_line as usize,
        end_column as usize,
        protocol,
    )?;
    (start <= end).then(|| source.get(start..end)).flatten()
}

fn source_line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(
        source
            .bytes()
            .enumerate()
            .filter_map(|(index, byte)| (byte == b'\n').then_some(index + 1)),
    );
    starts
}

fn source_offset(
    source: &str,
    line_starts: &[usize],
    line: usize,
    column: usize,
    protocol: ProviderProtocol,
) -> Option<usize> {
    let start = *line_starts.get(line)?;
    let mut end = line_starts.get(line + 1).copied().unwrap_or(source.len());
    while end > start && matches!(source.as_bytes()[end - 1], b'\n' | b'\r') {
        end -= 1;
    }
    let line_text = source.get(start..end)?;
    let byte_column = match protocol {
        ProviderProtocol::LanguageServerProtocol => utf16_to_byte_column(line_text, column)?,
        ProviderProtocol::Scip | ProviderProtocol::CompilerApi => {
            (column <= line_text.len() && line_text.is_char_boundary(column)).then_some(column)?
        }
    };
    Some(start + byte_column)
}

fn utf16_to_byte_column(text: &str, requested: usize) -> Option<usize> {
    let mut utf16 = 0usize;
    for (byte, character) in text.char_indices() {
        if utf16 == requested {
            return Some(byte);
        }
        let next = utf16 + character.len_utf16();
        if requested < next {
            return None;
        }
        utf16 = next;
    }
    (utf16 == requested).then_some(text.len())
}

fn is_source_identifier(candidate: &str) -> bool {
    let mut characters = candidate.chars();
    characters
        .next()
        .is_some_and(|character| character == '_' || character.is_alphabetic())
        && characters.all(|character| character == '_' || character.is_alphanumeric())
}

#[cfg(test)]
mod relation_reconciliation_tests {
    use super::*;

    fn definition(
        symbol: &str,
        path: &str,
        range: Vec<i32>,
        kind: &str,
        name: &str,
        has_body: bool,
    ) -> ProviderDefinition {
        ProviderDefinition {
            symbol: symbol.to_string(),
            language: match path.rsplit_once('.').map(|(_, extension)| extension) {
                Some("java") => "java",
                Some("dart") => "dart",
                _ => "rust",
            }
            .to_string(),
            path: path.to_string(),
            range,
            kind: kind.to_string(),
            name: Some(name.to_string()),
            declaration: format!("fn {name}() {{}}"),
            has_body,
        }
    }

    fn call(to: &str) -> RelationOutput {
        RelationOutput {
            from: "caller".to_string(),
            to: to.to_string(),
            kind: "CALLS".to_string(),
            path: "main.rs".to_string(),
            range: vec![4, 8, 4, 10],
            confidence: Some(1.0),
            strategy: Some("test".to_string()),
        }
    }

    #[test]
    fn one_site_keeps_the_unique_concrete_target_and_records_binding() {
        let concrete = definition("impl-id", "types.rs", vec![8, 0, 12, 1], "Method", "id", true);
        let contract = definition("trait-id", "types.rs", vec![1, 4, 1, 20], "Method", "id", false);
        let definitions = HashMap::from([
            (concrete.symbol.clone(), concrete),
            (contract.symbol.clone(), contract),
        ]);
        let output = enforce_one_target_per_call_site(
            vec![call("trait-id"), call("impl-id")],
            &definitions,
        );
        assert_eq!(
            output
                .iter()
                .filter(|relation| relation.kind == "CALLS")
                .count(),
            1
        );
        assert!(output.iter().any(|relation| {
            relation.kind == "CALLS" && relation.to == "impl-id"
        }));
        assert!(output.iter().any(|relation| {
            relation.kind == "IMPLEMENTATION"
                && relation.from == "impl-id"
                && relation.to == "trait-id"
        }));
    }

    #[test]
    fn ambiguous_concrete_targets_fail_closed() {
        let definitions = HashMap::from([
            (
                "left".to_string(),
                definition("left", "a.rs", vec![0, 0, 0, 10], "Method", "id", true),
            ),
            (
                "right".to_string(),
                definition("right", "b.rs", vec![0, 0, 0, 10], "Method", "id", true),
            ),
        ]);
        let output =
            enforce_one_target_per_call_site(vec![call("left"), call("right")], &definitions);
        assert!(output.iter().all(|relation| relation.kind != "CALLS"));
    }

    #[test]
    fn java_same_named_method_with_a_return_type_is_not_a_constructor() {
        let class = ProviderDefinition {
            symbol: "Box-type".to_string(),
            language: "java".to_string(),
            path: "Box.java".to_string(),
            range: vec![0, 0, 6, 1],
            kind: "Class".to_string(),
            name: Some("Box".to_string()),
            declaration: "class Box {}".to_string(),
            has_body: false,
        };
        let method = ProviderDefinition {
            symbol: "Box-method".to_string(),
            language: "java".to_string(),
            path: "Box.java".to_string(),
            range: vec![2, 4, 4, 5],
            kind: "Method".to_string(),
            name: Some("Box".to_string()),
            declaration: "public void Box() {}".to_string(),
            has_body: true,
        };
        assert!(!is_constructor_definition(
            &method,
            &[class.clone(), method.clone()]
        ));

        let constructor = ProviderDefinition {
            declaration: "public <T> Box(T value) {}".to_string(),
            ..method
        };
        assert!(is_constructor_definition(&constructor, &[class, constructor.clone()]));
    }
}
