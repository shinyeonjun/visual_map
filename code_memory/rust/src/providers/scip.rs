use scip::types::SymbolRole;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::{
    collect_files, forward_provider_stderr, javascript_workspace, project_cache_root,
    range_contains, tool_command, typescript_config_files, DocumentOutput, LanguageSpec,
    OccurrenceOutput, RelationOutput, SymbolOutput,
};

type SourceRange = (i32, i32, i32, i32);
type SymbolsByRange = HashMap<String, HashMap<SourceRange, String>>;
type CallRangesByPath = HashMap<String, HashSet<SourceRange>>;

pub(crate) fn ensure_default_scip_output(root: &Path, out: &Path) -> Result<(), String> {
    if out.is_file() {
        return Ok(());
    }
    let default = root.join("index.scip");
    if default.is_file() {
        fs::copy(&default, out).map_err(|e| {
            format!(
                "cannot copy {} to {}: {e}",
                default.display(),
                out.display()
            )
        })?;
        return Ok(());
    }
    Err(format!(
        "indexer completed but no SCIP output was found at {}",
        out.display()
    ))
}

pub(crate) fn run_command(
    mut command: Command,
    language: &str,
    provider: &str,
) -> Result<(), String> {
    let mut child = command
        .spawn()
        .map_err(|e| format!("{} indexer could not start: {e}", language))?;
    if let Some(stderr) = child.stderr.take() {
        forward_provider_stderr(provider, stderr);
    }
    let deadline = Instant::now() + provider_timeout();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    return Ok(());
                }
                return Err(format!("{} indexer exited with {}", language, status));
            }
            Ok(None) if Instant::now() >= deadline => {
                terminate_process_tree(&mut child);
                return Err(format!(
                    "{} indexer timeout after {} seconds",
                    language,
                    provider_timeout().as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(250)),
            Err(error) => {
                terminate_process_tree(&mut child);
                return Err(format!("{} indexer wait failed: {error}", language));
            }
        }
    }
}

pub(crate) fn provider_timeout() -> Duration {
    env::var("CODE_MEMORY_PROVIDER_TIMEOUT_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value == 0 || (5..=1_800).contains(value))
        .map(|value| {
            if value == 0 {
                // ponytail: use a practical no-timeout sentinel; replace with Option<Duration>
                // only if callers need to distinguish an actual deadline.
                Duration::from_secs(60 * 60 * 24 * 365 * 10)
            } else {
                Duration::from_secs(value)
            }
        })
        .unwrap_or_else(|| Duration::from_secs(180))
}

pub(crate) fn terminate_process_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let mut command = Command::new("taskkill");
        use std::os::windows::process::CommandExt;
        let _ = command
            .creation_flags(0x08000000)
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

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

fn select_dotnet_solution(root: &Path, source_files: &[PathBuf]) -> Option<PathBuf> {
    let mut solutions = collect_files(root, &["sln", "slnx"]);
    solutions.sort_by(|left, right| {
        dotnet_solution_score(right, source_files)
            .cmp(&dotnet_solution_score(left, source_files))
            .then_with(|| left.cmp(right))
    });
    solutions.into_iter().next()
}

pub(crate) fn dotnet_project_roots_for_files(
    root: &Path,
    source_files: &[PathBuf],
) -> Vec<PathBuf> {
    let mut roots = select_dotnet_solution(root, source_files)
        .map(|solution| solution_project_roots(&solution))
        .unwrap_or_default();
    if roots.is_empty() {
        roots = collect_files(root, &["csproj"])
            .into_iter()
            .filter_map(|project| project.parent().map(Path::to_path_buf))
            .collect();
    }
    roots
        .into_iter()
        .map(|path| path.canonicalize().unwrap_or(path))
        .collect()
}

pub(crate) fn dotnet_requires_unavailable_legacy_sdk(
    root: &Path,
    source_files: &[PathBuf],
) -> bool {
    if source_files.is_empty() {
        return false;
    }
    let mut matched = 0;
    let mut legacy = 0;
    for project in collect_files(root, &["csproj"]) {
        let Some(project_root) = project.parent() else {
            continue;
        };
        let project_root = project_root
            .canonicalize()
            .unwrap_or_else(|_| project_root.to_path_buf());
        let project_files = source_files.iter().filter(|file| {
            file.canonicalize()
                .unwrap_or_else(|_| (*file).clone())
                .starts_with(&project_root)
        });
        let count = project_files.count();
        if count == 0 {
            continue;
        }
        matched += count;
        let Ok(source) = fs::read_to_string(project) else {
            continue;
        };
        let legacy_target = source.to_ascii_lowercase().contains("windowsphone")
            || source.to_ascii_lowercase().contains("silverlight");
        if legacy_target {
            legacy += count;
        }
    }
    matched == source_files.len() && legacy == matched
}

fn dotnet_restore_state_path(root: &Path) -> PathBuf {
    project_cache_root(root).join("dotnet-restore-state")
}

fn dotnet_solution_key(solution: &Path) -> String {
    solution
        .canonicalize()
        .unwrap_or_else(|_| solution.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
}

fn dotnet_restore_is_current(root: &Path, solution: &Path, digest: u64) -> bool {
    let expected = format!("{digest}\n{}", dotnet_solution_key(solution));
    fs::read_to_string(dotnet_restore_state_path(root))
        .is_ok_and(|state| state.trim_end() == expected)
}

fn write_dotnet_restore_state(root: &Path, solution: &Path, digest: u64) -> Result<(), String> {
    let path = dotnet_restore_state_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create .NET restore cache: {error}"))?;
    }
    fs::write(path, format!("{digest}\n{}", dotnet_solution_key(solution)))
        .map_err(|error| format!("cannot write .NET restore cache: {error}"))
}

fn dotnet_solution_score(solution: &Path, source_files: &[PathBuf]) -> (usize, bool, bool, usize) {
    let project_roots = solution_project_roots(solution);
    let matched = if project_roots.is_empty() {
        let parent = solution.parent().unwrap_or_else(|| Path::new("."));
        source_files
            .iter()
            .filter(|file| file.starts_with(parent))
            .count()
    } else {
        source_files
            .iter()
            .filter(|file| project_roots.iter().any(|root| file.starts_with(root)))
            .count()
    };
    let lower = solution.to_string_lossy().to_ascii_lowercase();
    (
        matched,
        !lower.contains("\\build\\") && !lower.contains("/build/"),
        !lower.contains("\\test") && !lower.contains("/test"),
        usize::MAX.saturating_sub(solution.components().count()),
    )
}

fn solution_project_roots(solution: &Path) -> Vec<PathBuf> {
    let Ok(source) = fs::read_to_string(solution) else {
        return Vec::new();
    };
    let parent = solution.parent().unwrap_or_else(|| Path::new("."));
    source
        .split('"')
        .filter(|value| {
            let lower = value.to_ascii_lowercase();
            lower.ends_with(".csproj")
        })
        .map(|value| parent.join(value.replace('\\', "/")))
        .map(|path| path.parent().unwrap_or(&path).to_path_buf())
        .collect()
}

fn generated_dotnet_solution(root: &Path, out: &Path) -> Result<PathBuf, String> {
    let projects = collect_files(root, &["csproj"]);
    if projects.is_empty() {
        return Err("C# requires a .sln/.slnx or at least one .csproj file".to_string());
    }
    let directory = root;
    let suffix = out
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("solution");
    let solution = directory.join(format!(".code-memory-generated-{suffix}.sln"));
    let mut text = String::from(
        "Microsoft Visual Studio Solution File, Format Version 12.00\r\n# Visual Studio Version 17\r\nVisualStudioVersion = 17.0.31903.59\r\nMinimumVisualStudioVersion = 10.0.40219.1\r\n",
    );
    let mut project_guids = Vec::new();
    for project in projects {
        let relative = project.canonicalize().unwrap_or(project.clone());
        let relative = relative_path(directory, &relative)
            .to_string_lossy()
            .replace('/', "\\");
        let name = project
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("Project");
        let guid = stable_solution_guid(&relative);
        text.push_str(&format!(
            "Project(\"{{FAE04EC0-301F-11D3-BF4B-00C04F79EFBC}}\") = \"{name}\", \"{relative}\", \"{{{guid}}}\"\r\nEndProject\r\n"
        ));
        project_guids.push(guid);
    }
    text.push_str("Global\r\n\tGlobalSection(SolutionConfigurationPlatforms) = preSolution\r\n\t\tDebug|Any CPU = Debug|Any CPU\r\n\t\tRelease|Any CPU = Release|Any CPU\r\n\tEndGlobalSection\r\n\tGlobalSection(ProjectConfigurationPlatforms) = postSolution\r\n");
    for guid in project_guids {
        text.push_str(&format!(
            "\t\t{{{guid}}}.Debug|Any CPU.ActiveCfg = Debug|Any CPU\r\n\t\t{{{guid}}}.Debug|Any CPU.Build.0 = Debug|Any CPU\r\n\t\t{{{guid}}}.Release|Any CPU.ActiveCfg = Release|Any CPU\r\n\t\t{{{guid}}}.Release|Any CPU.Build.0 = Release|Any CPU\r\n"
        ));
    }
    text.push_str("\tEndGlobalSection\r\nEndGlobal\r\n");
    fs::write(&solution, text)
        .map_err(|error| format!("cannot write generated .NET solution: {error}"))?;
    Ok(solution)
}

fn relative_path(from: &Path, to: &Path) -> PathBuf {
    let from = from.canonicalize().unwrap_or_else(|_| from.to_path_buf());
    let to = to.canonicalize().unwrap_or_else(|_| to.to_path_buf());
    let from: Vec<_> = from.components().collect();
    let to: Vec<_> = to.components().collect();
    let common = from
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count();
    let mut relative = PathBuf::new();
    for _ in common..from.len() {
        relative.push("..");
    }
    for component in &to[common..] {
        relative.push(component.as_os_str());
    }
    relative
}

fn stable_solution_guid(value: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        hash as u32,
        (hash >> 32) as u16,
        (hash >> 48) as u16,
        (hash.rotate_left(17) >> 48) as u16,
        hash.rotate_left(29) & 0x0000_ffff_ffff_ffff,
    )
}

pub(crate) fn run_scip_indexer(
    lang: &LanguageSpec,
    root: &Path,
    out: &Path,
    providers_root: Option<&Path>,
    provider_config: Option<&Path>,
    source_files: &[PathBuf],
    project_config_digest: u64,
) -> Result<(), String> {
    let mut command = tool_command(lang.tool, providers_root)?;
    let mut generated_solution = None;
    let mut dotnet_restore_state = None;
    let mut php_include_file = None;
    command
        .current_dir(root)
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped());
    if lang.tool == "scip-clang" {
        let compdb = root.join("compile_commands.json");
        if !compdb.is_file() {
            return Err(format!("{} requires compile_commands.json", lang.name));
        }
        command.arg(format!("--compdb-path={}", compdb.display()));
    } else {
        command.arg("index");
        if lang.tool == "scip-dotnet" {
            let solution = if let Some(solution) = select_dotnet_solution(root, source_files) {
                solution
            } else {
                let solution = generated_dotnet_solution(root, out)?;
                generated_solution = Some(solution.clone());
                solution
            };
            let skip_restore = dotnet_restore_is_current(root, &solution, project_config_digest);
            if skip_restore {
                command.arg("--skip-dotnet-restore");
            }
            dotnet_restore_state = Some((solution.clone(), project_config_digest));
            command.arg(solution);
        }
        command.arg(format!("--output={}", out.display()));
        if lang.id == "php" {
            let php_files: Vec<_> = source_files
                .iter()
                .filter(|file| {
                    file.extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("php"))
                })
                .collect();
            if !php_files.is_empty() {
                let include_file = out.with_extension("php-files.txt");
                let content = php_files
                    .iter()
                    .map(|file| file.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("\n");
                fs::write(&include_file, content)
                    .map_err(|error| format!("cannot write PHP include list: {error}"))?;
                command.arg(format!("--include-file={}", include_file.display()));
                php_include_file = Some(include_file);
            }
        }
        if matches!(lang.id, "javascript" | "typescript") {
            if let Some(config) = provider_config {
                command.arg("--cwd").arg(root);
                command.arg("--workspace-root").arg(root);
                command.arg(config);
            } else {
                let configs = typescript_config_files(root);
                if configs.is_empty() {
                    let workspace = javascript_workspace(root, lang.id);
                    fs::create_dir_all(&workspace)
                        .map_err(|e| format!("cannot create JavaScript workspace: {e}"))?;
                    command.arg("--cwd").arg(&workspace);
                    command.args(["--infer-tsconfig", "--no-progress-bar"]);
                    command.arg(root);
                } else {
                    // Keep project resolution inside scip-typescript. It handles
                    // projectReferences and package/module resolution from each
                    // config better than a Rust-side file partition can.
                    for config in configs {
                        command.arg(config);
                    }
                }
            }
            if source_files.len() >= 2_000 {
                // ponytail: disable the provider's cross-project source cache
                // for large inputs; this trades repeated parsing for bounded RAM.
                command.arg("--no-global-caches");
            }
        }
    }
    let result = run_command(command, lang.name, lang.tool)
        .and_then(|_| ensure_default_scip_output(root, out));
    if result.is_ok() {
        if let Some((solution, digest)) = dotnet_restore_state {
            let _ = write_dotnet_restore_state(root, &solution, digest);
        }
    }
    if let Some(solution) = generated_solution {
        if env::var("CODE_MEMORY_KEEP_GENERATED_SOLUTION").as_deref() != Ok("1") {
            let _ = fs::remove_file(solution);
        }
    }
    if let Some(include_file) = php_include_file {
        let _ = fs::remove_file(include_file);
    }
    result
}
