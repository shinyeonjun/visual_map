pub(crate) fn read_scip(
    path: &Path,
    fallback_language: &str,
    project_root: &Path,
    allowed_paths: &HashSet<String>,
    call_ranges: Option<&HashMap<String, Vec<Vec<i32>>>>,
) -> Result<(Vec<DocumentOutput>, Vec<RelationOutput>), String> {
    let mut scip_documents = read_scoped_scip_documents(path, project_root, allowed_paths)?;
    let mut definitions: HashMap<String, Vec<(String, Vec<i32>)>> = HashMap::new();
    let mut definition_indexes: HashMap<String, DefinitionRangeIndex> = HashMap::new();
    let mut definitions_by_range = SymbolsByRange::new();
    let mut type_symbols = HashSet::new();
    let mut source_cache = HashMap::<String, String>::new();
    let mut scope_fallback_paths = HashSet::<String>::new();
    for document in &scip_documents {
        let document_path = normalize_scip_path(&document.relative_path, project_root);
        if !allowed_paths.contains(&document_path) {
            continue;
        }
        for occurrence in &document.occurrences {
            if occurrence.enclosing_range.is_empty() {
                scope_fallback_paths.insert(document_path.clone());
            }
            if has_role(occurrence.symbol_roles, SymbolRole::Definition)
                && !occurrence.symbol.is_empty()
            {
                let definition_range = if occurrence.enclosing_range.is_empty() {
                    occurrence.range.clone()
                } else {
                    occurrence.enclosing_range.clone()
                };
                definitions
                    .entry(document_path.clone())
                    .or_default()
                    .push((occurrence.symbol.clone(), definition_range.clone()));
                if let Some(range) = range_parts(&definition_range) {
                    definitions_by_range
                        .entry(document_path.clone())
                        .or_default()
                        .entry(range)
                        .or_insert_with(|| occurrence.symbol.clone());
                }
            }
        }
        for information in &document.symbols {
            if is_type_symbol_kind(information.kind, &information.symbol)
                && !information.symbol.is_empty()
            {
                type_symbols.insert(information.symbol.clone());
            }
        }
    }
    for (document_path, document_definitions) in &definitions {
        definition_indexes.insert(
            document_path.clone(),
            DefinitionRangeIndex::from_definitions(document_definitions),
        );
    }
    for document in &scip_documents {
        let document_path = normalize_scip_path(&document.relative_path, project_root);
        if !allowed_paths.contains(&document_path) {
            continue;
        }
        for occurrence in &document.occurrences {
            if has_role(occurrence.symbol_roles, SymbolRole::Definition)
                || occurrence.enclosing_range.is_empty()
            {
                continue;
            }
            let exact_owner = range_parts(&occurrence.enclosing_range)
                .and_then(|range| definitions_by_range.get(&document_path)?.get(&range));
            if exact_owner.is_none() {
                scope_fallback_paths.insert(document_path.clone());
            }
        }
    }
    let owner_scopes: HashMap<String, Vec<(String, Vec<i32>)>> = definitions
        .iter()
        .filter_map(|(document_path, document_definitions)| {
            if !scope_fallback_paths.contains(document_path) {
                return None;
            }
            let source = source_cache
                .entry(document_path.clone())
                .or_insert_with(|| {
                    fs::read_to_string(project_root.join(document_path)).unwrap_or_default()
                });
            Some((
                document_path.clone(),
                source_scopes(source, document_definitions),
            ))
        })
        .collect();
    let type_owner_scopes: HashMap<String, Vec<(String, Vec<i32>)>> = definitions
        .iter()
        .filter_map(|(document_path, document_definitions)| {
            let type_definitions = document_definitions
                .iter()
                .filter(|(symbol, _)| type_symbols.contains(symbol))
                .cloned()
                .collect::<Vec<_>>();
            if type_definitions.is_empty() {
                return None;
            }
            let source = source_cache
                .entry(document_path.clone())
                .or_insert_with(|| {
                    fs::read_to_string(project_root.join(document_path)).unwrap_or_default()
                });
            let scopes = source_scopes(source, &type_definitions);
            (!scopes.is_empty()).then_some((document_path.clone(), scopes))
        })
        .collect();
    let mut document_symbol_aliases = HashMap::<String, HashMap<String, String>>::new();
    let mut alias_targets = HashMap::<String, HashSet<String>>::new();
    for document in &scip_documents {
        let document_path = normalize_scip_path(&document.relative_path, project_root);
        for info in &document.symbols {
            let provider_kind = info
                .kind
                .enum_value()
                .map(|value| format!("{value:?}"))
                .unwrap_or_else(|_| "Unspecified".to_string());
            let kind = normalized_scip_symbol_kind(provider_kind, &info.symbol);
            if !is_type_member_kind(&kind) {
                continue;
            }
            let source_owner = definitions
                .get(&document_path)
                .and_then(|definitions| {
                    definitions
                        .iter()
                        .find(|(symbol, _)| symbol == &info.symbol)
                })
                .and_then(|(_, range)| {
                    find_source_owner(type_owner_scopes.get(&document_path), range)
                });
            let Some(alias) = source_owner
                .as_deref()
                .and_then(|owner| reparent_scip_symbol(&info.symbol, owner))
                .filter(|alias| alias != &info.symbol)
            else {
                continue;
            };
            document_symbol_aliases
                .entry(document_path.clone())
                .or_default()
                .insert(info.symbol.clone(), alias.clone());
            alias_targets
                .entry(info.symbol.clone())
                .or_default()
                .insert(alias);
        }
    }
    let unique_symbol_aliases = alias_targets
        .into_iter()
        .filter_map(|(symbol, aliases)| {
            (aliases.len() == 1).then(|| (symbol, aliases.into_iter().next().unwrap()))
        })
        .collect::<HashMap<_, _>>();

    let type_script_call_ranges: Option<CallRangesByPath> = call_ranges.map(|ranges| {
        ranges
            .iter()
            .filter_map(|(path, values)| {
                let ranges: HashSet<(i32, i32, i32, i32)> = values
                    .iter()
                    .filter_map(|range| range_parts(range))
                    .collect();
                (!ranges.is_empty()).then_some((path.clone(), ranges))
            })
            .collect()
    });
    let needs_source_for_call_detection = !matches!(fallback_language, "typescript" | "javascript")
        || call_ranges.is_some_and(HashMap::is_empty);
    let mut relations = Vec::new();
    let mut documents = Vec::new();
    for document in scip_documents.drain(..) {
        let document_path = normalize_scip_path(&document.relative_path, project_root);
        if !allowed_paths.contains(&document_path) {
            continue;
        }
        if needs_source_for_call_detection {
            source_cache
                .entry(document_path.clone())
                .or_insert_with(|| {
                    fs::read_to_string(project_root.join(&document_path)).unwrap_or_default()
                });
        }
        let language = normalize_scip_language(&document.language, fallback_language);
        let local_symbol_aliases = document_symbol_aliases.get(&document_path);
        let mut symbols = Vec::new();
        for info in document.symbols {
            for relationship in &info.relationships {
                if relationship.symbol.is_empty()
                    || is_non_visual_symbol(&info.symbol)
                    || is_non_visual_symbol(&relationship.symbol)
                    || info.symbol == relationship.symbol
                {
                    continue;
                }
                let kind = if relationship.is_implementation {
                    "IMPLEMENTATION"
                } else if relationship.is_type_definition {
                    "TYPE_DEFINITION"
                } else if relationship.is_definition {
                    if matches!(fallback_language, "c" | "cpp") {
                        "DEFINITION"
                    } else {
                        "DEFINITION_OVERRIDE"
                    }
                } else if relationship.is_reference {
                    "SYMBOL_REFERENCE"
                } else {
                    continue;
                };
                let from = canonical_scip_symbol(
                    &info.symbol,
                    local_symbol_aliases,
                    &unique_symbol_aliases,
                );
                let to = canonical_scip_symbol(
                    &relationship.symbol,
                    local_symbol_aliases,
                    &unique_symbol_aliases,
                );
                if from == to {
                    continue;
                }
                relations.push(RelationOutput {
                    from,
                    to,
                    kind: kind.to_string(),
                    path: document_path.clone(),
                    range: Vec::new(),
                    confidence: Some(1.0),
                    strategy: Some("provider-relationship".to_string()),
                });
            }
            let provider_kind = info
                .kind
                .enum_value()
                .map(|value| format!("{value:?}"))
                .unwrap_or_else(|_| "Unspecified".to_string());
            let kind = normalized_scip_symbol_kind(provider_kind, &info.symbol);
            let symbol =
                canonical_scip_symbol(&info.symbol, local_symbol_aliases, &unique_symbol_aliases);
            let inferred_enclosing_symbol = inferred_scip_enclosing_symbol(&symbol, &kind);
            let source_enclosing_symbol = is_type_member_kind(&kind)
                .then(|| {
                    definitions
                        .get(&document_path)
                        .and_then(|definitions| {
                            definitions
                                .iter()
                                .find(|(symbol, _)| symbol == &info.symbol)
                        })
                        .and_then(|(_, range)| {
                            find_source_owner(type_owner_scopes.get(&document_path), range)
                        })
                })
                .flatten();
            let display_name = (!info.display_name.trim().is_empty()).then_some(info.display_name);
            let provider_enclosing_symbol = (!info.enclosing_symbol.is_empty()).then(|| {
                canonical_scip_symbol(
                    &info.enclosing_symbol,
                    local_symbol_aliases,
                    &unique_symbol_aliases,
                )
            });
            let enclosing_symbol = if is_type_member_kind(&kind) {
                provider_enclosing_symbol
                    .as_ref()
                    .filter(|symbol| type_symbols.contains(*symbol))
                    .cloned()
                    .or(source_enclosing_symbol)
                    .or(provider_enclosing_symbol)
                    .or(inferred_enclosing_symbol)
            } else {
                provider_enclosing_symbol.or(inferred_enclosing_symbol)
            };
            symbols.push(SymbolOutput {
                symbol,
                kind,
                display_name,
                documentation: info.documentation,
                signature: info
                    .signature_documentation
                    .as_ref()
                    .map(|s| s.text.clone()),
                enclosing_symbol,
            });
        }

        let mut occurrences = Vec::new();
        for occurrence in document.occurrences {
            let definition = has_role(occurrence.symbol_roles, SymbolRole::Definition);
            let import = has_role(occurrence.symbol_roles, SymbolRole::Import);
            let occurrence_symbol = canonical_scip_symbol(
                &occurrence.symbol,
                local_symbol_aliases,
                &unique_symbol_aliases,
            );
            if !definition && !occurrence.symbol.is_empty() {
                let owner = (!occurrence.enclosing_range.is_empty())
                    .then(|| {
                        definitions_by_range
                            .get(&document_path)
                            .and_then(|ranges| {
                                range_parts(&occurrence.enclosing_range)
                                    .and_then(|range| ranges.get(&range))
                            })
                            .cloned()
                    })
                    .flatten()
                    .or_else(|| {
                        find_definition_for_enclosing_range_index(
                            definition_indexes.get(&document_path),
                            definitions.get(&document_path),
                            &occurrence.enclosing_range,
                            &occurrence.range,
                        )
                    })
                    .or_else(|| {
                        find_source_owner(owner_scopes.get(&document_path), &occurrence.range)
                    })
                    .map(|owner| {
                        canonical_scip_symbol(&owner, local_symbol_aliases, &unique_symbol_aliases)
                    });
                if let Some(owner) = owner {
                    let kind = if import {
                        "IMPORTS"
                    } else if (matches!(fallback_language, "typescript" | "javascript")
                        && call_ranges.is_some_and(|ranges| {
                            if ranges.is_empty() {
                                is_call_occurrence(
                                    source_cache.get(&document_path).map(String::as_str),
                                    &occurrence.range,
                                )
                            } else {
                                type_script_call_occurrence(
                                    &type_script_call_ranges,
                                    &document_path,
                                    &occurrence.range,
                                )
                            }
                        }))
                        || (!matches!(fallback_language, "typescript" | "javascript")
                            && (is_call_occurrence(
                                source_cache.get(&document_path).map(String::as_str),
                                &occurrence.range,
                            ) || (fallback_language == "ruby"
                                && is_ruby_member_call_occurrence(
                                    source_cache.get(&document_path).map(String::as_str),
                                    &occurrence.range,
                                ))))
                    {
                        "CALLS"
                    } else if matches!(fallback_language, "c" | "cpp")
                        && type_symbols.contains(&occurrence.symbol)
                    {
                        "USES_TYPE"
                    } else {
                        "REFERENCES"
                    };
                    let relation_from = if kind == "CALLS" && owner == occurrence_symbol {
                        if fallback_language == "ruby" {
                            // Ruby top-level calls have no enclosing LSP
                            // symbol. Keep the real target and anchor the
                            // caller at the source file instead of inventing
                            // a recursive self-call.
                            Some(format!("file:{document_path}"))
                        } else {
                            None
                        }
                    } else {
                        Some(owner)
                    };
                    if let Some(relation_from) = relation_from.filter(|from| {
                        !is_non_visual_symbol(from)
                            && !is_non_visual_symbol(&occurrence_symbol)
                            && (kind == "CALLS" || from != &occurrence_symbol)
                    }) {
                        relations.push(RelationOutput {
                            from: relation_from,
                            to: occurrence_symbol.clone(),
                            kind: kind.to_string(),
                            path: document_path.clone(),
                            range: occurrence.range.clone(),
                            confidence: Some(1.0),
                            strategy: Some("provider-symbol-resolution".to_string()),
                        });
                    }
                }
            }
            occurrences.push(OccurrenceOutput {
                symbol: occurrence_symbol,
                range: occurrence.range,
                enclosing_range: occurrence.enclosing_range,
                definition,
                import,
                read: has_role(occurrence.symbol_roles, SymbolRole::ReadAccess),
                write: has_role(occurrence.symbol_roles, SymbolRole::WriteAccess),
            });
        }
        documents.push(DocumentOutput {
            language,
            path: document_path,
            symbols,
            occurrences,
        });
    }
    let mut seen = HashSet::new();
    relations.retain(|relation| {
        let range_key = if relation.kind == "CALLS" {
            relation.range.clone()
        } else {
            Vec::new()
        };
        seen.insert((
            relation.from.clone(),
            relation.to.clone(),
            relation.kind.clone(),
            relation.path.clone(),
            range_key,
        ))
    });
    Ok((documents, relations))
}

