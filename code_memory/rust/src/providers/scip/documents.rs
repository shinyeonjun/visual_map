pub(crate) fn read_scip(
    path: &Path,
    fallback_language: &str,
    protocol: ProviderProtocol,
    project_root: &Path,
    allowed_paths: &HashSet<String>,
    call_ranges: Option<&HashMap<String, Vec<Vec<i32>>>>,
) -> Result<(Vec<DocumentOutput>, Vec<RelationOutput>), String> {
    let mut scip_documents = read_scoped_scip_documents(path, project_root, allowed_paths)?;
    if fallback_language == "csharp" {
        normalize_scip_dotnet_utf16_ranges(
            &mut scip_documents,
            project_root,
            allowed_paths,
        );
    }
    let mut definitions: HashMap<String, Vec<(String, Vec<i32>)>> = HashMap::new();
    let mut definition_ranges_by_symbol =
        HashMap::<String, HashMap<String, Vec<i32>>>::new();
    let mut definition_indexes: HashMap<String, DefinitionRangeIndex> = HashMap::new();
    let mut definitions_by_range = SymbolsByRange::new();
    let mut definitions_by_name_range = SymbolsByRange::new();
    let mut type_symbols = HashSet::new();
    let mut source_cache = HashMap::<String, String>::new();
    let mut type_use_sites_by_document =
        HashMap::<(String, String), Vec<SyntaxTypeUseSite>>::new();
    let mut type_use_indexes_by_document =
        HashMap::<(String, String), SourceRangeIndex>::new();
    let mut hierarchy_sites_by_document =
        HashMap::<(String, String), Vec<SyntaxTypeRelationSite>>::new();
    let mut hierarchy_indexes_by_document =
        HashMap::<(String, String), SourceRangeIndex>::new();
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
                if let Some(range) = range_parts(&occurrence.range) {
                    definitions_by_name_range
                        .entry(document_path.clone())
                        .or_default()
                        .entry(range)
                        .or_insert_with(|| occurrence.symbol.clone());
                }
                let definition_range = if occurrence.enclosing_range.is_empty() {
                    occurrence.range.clone()
                } else {
                    occurrence.enclosing_range.clone()
                };
                definitions
                    .entry(document_path.clone())
                    .or_default()
                    .push((occurrence.symbol.clone(), definition_range.clone()));
                definition_ranges_by_symbol
                    .entry(document_path.clone())
                    .or_default()
                    .entry(occurrence.symbol.clone())
                    .or_insert_with(|| definition_range.clone());
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
    let owner_scope_indexes = owner_scopes
        .iter()
        .map(|(path, scopes)| {
            (
                path.clone(),
                SourceRangeIndex::from_ranges(
                    scopes
                        .iter()
                        .enumerate()
                        .map(|(index, (_, range))| (index, range.as_slice())),
                ),
            )
        })
        .collect::<HashMap<_, _>>();
    let type_owner_scope_indexes = type_owner_scopes
        .iter()
        .map(|(path, scopes)| {
            (
                path.clone(),
                SourceRangeIndex::from_ranges(
                    scopes
                        .iter()
                        .enumerate()
                        .map(|(index, (_, range))| (index, range.as_slice())),
                ),
            )
        })
        .collect::<HashMap<_, _>>();
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
                .and_then(|_| {
                    definition_ranges_by_symbol
                        .get(&document_path)
                        .and_then(|ranges| ranges.get(&info.symbol))
                })
                .and_then(|range| {
                    find_source_owner_indexed(
                        type_owner_scopes.get(&document_path),
                        type_owner_scope_indexes.get(&document_path),
                        range,
                    )
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
    let mut relations = Vec::new();
    let mut documents = Vec::new();
    let mut syntax_sites_by_document =
        HashMap::<(String, String), Vec<SyntaxCallSite>>::new();
    for document in scip_documents.drain(..) {
        let document_path = normalize_scip_path(&document.relative_path, project_root);
        if !allowed_paths.contains(&document_path) {
            continue;
        }
        source_cache
            .entry(document_path.clone())
            .or_insert_with(|| {
                fs::read_to_string(project_root.join(&document_path)).unwrap_or_default()
            });
        let language = normalize_scip_language(&document.language, fallback_language);
        if let Some(contract_language) = contract_language(&language) {
            if let Ok(type_syntax) = inventory_type_syntax(
                contract_language,
                &document_path,
                source_cache
                    .get(&document_path)
                    .map(String::as_str)
                    .unwrap_or_default(),
            ) {
                // The syntax inventory enriches exact type relations only.
                // Its grammar coverage cannot be allowed to discard an
                // otherwise valid SCIP document; the downstream canonical
                // adapter records the capability-specific source gap.
                let key = (language.clone(), document_path.clone());
                hierarchy_indexes_by_document.insert(
                    key.clone(),
                    SourceRangeIndex::from_ranges(
                        type_syntax.relations.iter().enumerate().map(|(index, site)| {
                            (index, site.target_range(protocol))
                        }),
                    ),
                );
                type_use_indexes_by_document.insert(
                    key.clone(),
                    SourceRangeIndex::from_ranges(
                        type_syntax.uses.iter().enumerate().map(|(index, site)| {
                            (index, site.target_range(protocol))
                        }),
                    ),
                );
                hierarchy_sites_by_document.insert(key.clone(), type_syntax.relations);
                type_use_sites_by_document.insert(key, type_syntax.uses);
            }
        }
        let document_call_sites = inventory_call_sites(
            &language,
            source_cache
                .get(&document_path)
                .map(String::as_str)
                .unwrap_or_default(),
        )?;
        syntax_sites_by_document.insert(
            (language.clone(), document_path.clone()),
            document_call_sites.clone(),
        );
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
                    definition_ranges_by_symbol
                        .get(&document_path)
                        .and_then(|ranges| ranges.get(&info.symbol))
                        .and_then(|range| {
                            find_source_owner_indexed(
                                type_owner_scopes.get(&document_path),
                                type_owner_scope_indexes.get(&document_path),
                                range,
                            )
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
                    .map(|signature| signature.text.trim())
                    .filter(|signature| !signature.is_empty())
                    .map(str::to_string),
                enclosing_symbol,
            });
        }

        let mut occurrences = Vec::new();
        let mut call_site_index = ExactSourceRangeIndex::default();
        for (index, site) in document_call_sites.iter().enumerate() {
            call_site_index.insert(index, &site.callee_utf8_range);
            call_site_index.insert(index, &site.callee_utf16_range);
        }
        for occurrence in document.occurrences {
            let definition = has_role(occurrence.symbol_roles, SymbolRole::Definition);
            let import = has_role(occurrence.symbol_roles, SymbolRole::Import);
            let occurrence_symbol = canonical_scip_symbol(
                &occurrence.symbol,
                local_symbol_aliases,
                &unique_symbol_aliases,
            );
            if !definition && !occurrence.symbol.is_empty() {
                let type_use_site = type_use_sites_by_document
                    .get(&(language.clone(), document_path.clone()))
                    .zip(type_use_indexes_by_document.get(&(
                        language.clone(),
                        document_path.clone(),
                    )))
                    .and_then(|(sites, index)| {
                        index
                            .first_bidirectional_containment(&occurrence.range)
                            .and_then(|index| sites.get(index))
                    });
                let hierarchy_site = hierarchy_sites_by_document
                    .get(&(language.clone(), document_path.clone()))
                    .zip(hierarchy_indexes_by_document.get(&(
                        language.clone(),
                        document_path.clone(),
                    )))
                    .and_then(|(sites, index)| {
                        index
                            .first_bidirectional_containment(&occurrence.range)
                            .and_then(|index| sites.get(index))
                    });
                let exact_relation_owner_range = type_use_site
                    .map(|site| site.source_name_range(protocol))
                    .or_else(|| hierarchy_site.map(|site| site.source_range(protocol)));
                let exact_relation_owner = exact_relation_owner_range
                    .and_then(range_parts)
                    .and_then(|range| {
                        definitions_by_name_range
                            .get(&document_path)
                            .and_then(|ranges| ranges.get(&range))
                    })
                    .cloned();
                let syntax_site = call_site_index
                    .find(&occurrence.range)
                    .and_then(|index| document_call_sites.get(index));
                let exact_call_owner = syntax_site
                    .and_then(|site| site.owner_name_range(protocol))
                    .and_then(range_parts)
                    .and_then(|range| {
                        definitions_by_name_range
                            .get(&document_path)
                            .and_then(|ranges| ranges.get(&range))
                    })
                    .cloned();
                let owner = exact_relation_owner
                    .or(exact_call_owner)
                    .or_else(|| (!occurrence.enclosing_range.is_empty())
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
                        find_source_owner_indexed(
                            owner_scopes.get(&document_path),
                            owner_scope_indexes.get(&document_path),
                            &occurrence.range,
                        )
                    }))
                    .map(|owner| {
                        canonical_scip_symbol(&owner, local_symbol_aliases, &unique_symbol_aliases)
                    });
                if let Some(owner) = owner {
                    let kind = if import {
                        "IMPORTS"
                    } else if syntax_site
                        .is_some_and(|site| site.form == CallSiteForm::Construct)
                    {
                        "CONSTRUCTS"
                    } else if syntax_site.is_some()
                        || (matches!(fallback_language, "typescript" | "javascript")
                            && call_ranges.is_some()
                            && type_script_call_occurrence(
                                &type_script_call_ranges,
                                &document_path,
                                &occurrence.range,
                                source_cache.get(&document_path).map(String::as_str),
                            ))
                        || (!matches!(
                            language.as_str(),
                            "csharp" | "c" | "cpp" | "go" | "rust"
                        ) && !matches!(fallback_language, "typescript" | "javascript")
                            && is_call_occurrence(
                                source_cache.get(&document_path).map(String::as_str),
                                &occurrence.range,
                            ))
                    {
                        "CALLS"
                    } else if hierarchy_site.is_some()
                        && type_symbols.contains(&occurrence.symbol)
                    {
                        "IMPLEMENTATION"
                    } else if type_use_site.is_some()
                        && type_symbols.contains(&occurrence.symbol)
                    {
                        "USES_TYPE"
                    } else {
                        "REFERENCES"
                    };
                    let executable = matches!(kind, "CALLS" | "CONSTRUCTS");
                    let relation_from = if executable && owner == occurrence_symbol {
                        None
                    } else {
                        Some(owner)
                    };
                    if let Some(relation_from) = relation_from.filter(|from| {
                        !is_non_visual_symbol(from)
                            && !is_non_visual_symbol(&occurrence_symbol)
                            && (executable || from != &occurrence_symbol)
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
    reconcile_syntax_relations(
        protocol,
        &documents,
        &mut relations,
        &syntax_sites_by_document,
        &source_cache,
    );
    let mut seen = HashSet::new();
    relations.retain(|relation| {
        let range_key = if matches!(relation.kind.as_str(), "CALLS" | "CONSTRUCTS") {
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

/// scip-dotnet 0.2.14 forwards Roslyn `TextSpan` columns as UTF-16 code-unit
/// offsets. SCIP's portable source contract uses UTF-8 byte columns, so the
/// provider output must be normalized before any exact-range matching or
/// evidence construction. ASCII-only repositories conceal this difference;
/// identifiers such as `Entityß` otherwise end in the middle of a UTF-8 code
/// point and are rejected by the canonical evidence boundary.
fn normalize_scip_dotnet_utf16_ranges(
    documents: &mut [scip::types::Document],
    project_root: &Path,
    allowed_paths: &HashSet<String>,
) {
    for document in documents {
        let document_path = normalize_scip_path(&document.relative_path, project_root);
        if !allowed_paths.contains(&document_path) {
            continue;
        }
        let Ok(source) = fs::read_to_string(project_root.join(&document_path)) else {
            continue;
        };
        let lines = source.lines().collect::<Vec<_>>();
        for occurrence in &mut document.occurrences {
            normalize_scip_dotnet_range(&lines, &mut occurrence.range);
            normalize_scip_dotnet_range(&lines, &mut occurrence.enclosing_range);
        }
    }
}

fn normalize_scip_dotnet_range(lines: &[&str], range: &mut Vec<i32>) {
    let converted = match range.as_slice() {
        [line, start, end] => {
            let Some(text) = usize::try_from(*line)
                .ok()
                .and_then(|line| lines.get(line))
            else {
                return;
            };
            let (Some(start), Some(end)) = (
                usize::try_from(*start)
                    .ok()
                    .and_then(|column| utf16_to_byte_column(text, column)),
                usize::try_from(*end)
                    .ok()
                    .and_then(|column| utf16_to_byte_column(text, column)),
            ) else {
                return;
            };
            vec![*line, start as i32, end as i32]
        }
        [start_line, start_column, end_line, end_column, ..] => {
            let (Some(start_text), Some(end_text)) = (
                usize::try_from(*start_line)
                    .ok()
                    .and_then(|line| lines.get(line)),
                usize::try_from(*end_line)
                    .ok()
                    .and_then(|line| lines.get(line)),
            ) else {
                return;
            };
            let (Some(start), Some(end)) = (
                usize::try_from(*start_column)
                    .ok()
                    .and_then(|column| utf16_to_byte_column(start_text, column)),
                usize::try_from(*end_column)
                    .ok()
                    .and_then(|column| utf16_to_byte_column(end_text, column)),
            ) else {
                return;
            };
            vec![*start_line, start as i32, *end_line, end as i32]
        }
        _ => return,
    };
    *range = converted;
}

fn contract_language(language: &str) -> Option<ProgrammingLanguage> {
    match language {
        "typescript" => Some(ProgrammingLanguage::TypeScript),
        "javascript" => Some(ProgrammingLanguage::JavaScript),
        "python" => Some(ProgrammingLanguage::Python),
        "java" => Some(ProgrammingLanguage::Java),
        "csharp" => Some(ProgrammingLanguage::CSharp),
        "c" => Some(ProgrammingLanguage::C),
        "cpp" => Some(ProgrammingLanguage::Cpp),
        "go" => Some(ProgrammingLanguage::Go),
        "rust" => Some(ProgrammingLanguage::Rust),
        "dart" => Some(ProgrammingLanguage::Dart),
        _ => None,
    }
}

#[cfg(test)]
mod document_coordinate_tests {
    use super::*;

    #[test]
    fn scip_dotnet_utf16_columns_become_utf8_byte_columns() {
        let lines = ["protected class Entityß", "class Rocket🚀"];
        let mut bmp = vec![0, 16, 23];
        normalize_scip_dotnet_range(&lines, &mut bmp);
        assert_eq!(bmp, vec![0, 16, 24]);

        let mut supplementary = vec![1, 6, 14];
        normalize_scip_dotnet_range(&lines, &mut supplementary);
        assert_eq!(supplementary, vec![1, 6, 16]);
    }

    #[test]
    fn invalid_scip_dotnet_column_is_left_for_typed_gap_handling() {
        let lines = ["class A"];
        let mut range = vec![0, 0, 99];
        normalize_scip_dotnet_range(&lines, &mut range);
        assert_eq!(range, vec![0, 0, 99]);
    }
}
