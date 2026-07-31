use std::collections::{BTreeMap, HashMap, HashSet};

use super::{implementation_file_score, signal_needle, symbol_short_name, FrameworkPack};
use crate::DocumentOutput;

#[cfg(test)]
use super::symbol_matches_name;

pub(crate) struct FrameworkSymbolIndex {
    definitions_by_file_name: HashMap<(String, String), Vec<String>>,
    references_by_file_name: HashMap<(String, String), Vec<String>>,
    definitions_by_file_line_name: HashMap<(String, usize, String), Vec<String>>,
    references_by_file_line_name: HashMap<(String, usize, String), Vec<(usize, String)>>,
    definitions_by_name: HashMap<String, Vec<String>>,
    defined: HashSet<String>,
    implementation_scores: HashMap<String, u8>,
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
    if let Some(symbol) = select_indexed_symbol(
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
    select_indexed_symbol(index, index.definitions_by_name.get(&short_name))
}

pub(crate) fn resolve_symbol_at_indexed(
    index: &FrameworkSymbolIndex,
    path: &str,
    name: &str,
    source_line: usize,
) -> Option<String> {
    let short_name = symbol_short_name(name).to_string();
    if let Some(symbol) = select_indexed_symbol(
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
    if let Some(symbol) = select_indexed_symbol(
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
    select_indexed_symbol(
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
        select_indexed_symbol(
            index,
            index
                .definitions_by_file_line_name
                .get(&(path.to_string(), source_line, short_name)),
        )
    })
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

pub(crate) fn output_evidence(pack: &FrameworkPack, output: &str, line: &str) -> Option<String> {
    let line = line.trim();
    if pack.id == "react" && output == "COMPONENT" {
        return react_component_evidence(line);
    }
    if pack.id == "tauri" && output == "ASYNC_CALLS" {
        return line.contains("invoke(").then_some("invoke(".to_string());
    }
    if pack.id == "tauri" && pack.language == "rust" && output == "RPC_ENDPOINT" {
        return line
            .contains("tauri::command")
            .then_some("tauri::command".to_string());
    }
    if output == "COMPONENT"
        && (line.starts_with("#include")
            || line.starts_with("#define")
            || line.starts_with("END_MESSAGE_MAP"))
    {
        return None;
    }
    if output == "EVENT_HANDLER" && line.starts_with("#define") {
        return None;
    }
    if output == "MIDDLEWARE"
        && ((line.starts_with("use ") && !line.contains(".use("))
            || (line.starts_with("import ") && !line.contains(".use("))
            || (line.contains("require(") && !line.contains(".use("))
            || line.contains("void Use(")
            || line.contains("function use(")
            || line.contains("def use("))
    {
        return None;
    }
    if output == "RPC_ENDPOINT" && line.contains("func (") && line.contains("RegisterService(") {
        return None;
    }
    let marker = match output {
        "COMPONENT" => first_marker(
            line,
            &[
                "@Component",
                "defineComponent(",
                "StatelessWidget",
                "StatefulWidget",
                "ComponentBase",
                "ContentPage",
                "GtkWidget",
                "QWidget",
                "CWnd",
                "AActor",
                "UCLASS",
                "QObject",
                "Q_OBJECT",
                "BEGIN_MESSAGE_MAP",
                "class App extends React.Component",
                "=> <",
                "return <",
            ],
        )
        .or_else(|| signal_marker(pack, line, &["jsx", "component", "Widget"])),
        "RENDERS" => first_marker(
            line,
            &[
                "return <",
                "render(",
                "Widget build",
                "build(BuildContext",
                "BuildRenderTree",
                "RenderFragment",
                "template:",
            ],
        ),
        "EVENT_HANDLER" => first_marker(
            line,
            &[
                "g_signal_connect",
                "QObject::connect",
                "addEventListener",
                "addListener",
                "EventListener",
                "onRequest",
                "subscribe(",
                ".on(",
                ".emit(",
                "emit(",
                "@onclick",
                "@click",
                "onClick",
                "onTap",
                "onPressed",
                "Clicked",
                "uv_read_start",
                "async_read_some",
                "ON_COMMAND",
                "signals",
                "slots",
            ],
        )
        .or_else(|| {
            (pack.id == "unreal-engine" && line.contains("UFUNCTION"))
                .then(|| "UFUNCTION".to_string())
        }),
        "SERVICE" => first_marker(
            line,
            &[
                "@Service",
                "@Component",
                "@Repository",
                "@Injectable",
                "@Singleton",
                "@ApplicationScoped",
                "ControllerBase",
                "BaseController",
                "AbstractController",
                "ApiResource",
                "Endpoint",
                "class .*Service",
            ],
        )
        .or_else(|| service_name_marker(line)),
        "MIDDLEWARE" => first_marker(
            line,
            &[
                ".use(",
                "addHook(",
                "app.Use(",
                "Use(",
                "UseMiddleware",
                "@UseGuards(",
                "@UseInterceptors(",
                "@UsePipes(",
                "middleware",
                "before_request(",
                "after_request(",
                "add_middleware(",
                ".layer(",
                ".wrap(",
                ".with(",
                "Middleware",
                "Pipeline",
                "Rack::Builder",
                "Filter",
                "filter(",
                "ADD_FILTER",
                "registerFilter",
                "setFilters",
            ],
        )
        .or_else(|| {
            line.to_ascii_lowercase()
                .contains("middleware")
                .then(|| "middleware".to_string())
        }),
        "DEPENDENCY" => first_marker(
            line,
            &[
                "@Inject",
                "@Autowired",
                "@Singleton",
                "Depends(",
                "inject(",
                "Inject(",
                "Provide(",
                "Dependency",
                "builder.Services",
                "AddScoped(",
                "AddSingleton(",
                "AddTransient(",
            ],
        ),
        "ASYNC_CALLS" => first_marker(
            line,
            &[
                "async ",
                "async(",
                "await ",
                "spawn(",
                "async_",
                "Future<",
                ".then(",
                "publish(",
                "subscribe(",
                "io_context",
                "event_base",
                "event_callback",
                "event_base_dispatch",
                "uv_run",
                "uv_read_start",
                "QtConcurrent",
                "RegisterService",
                "CompletionQueue",
                "Server::builder",
                "add_service",
            ],
        )
        .or_else(|| {
            line.contains("QtConcurrent")
                .then(|| "QtConcurrent".to_string())
        }),
        "RPC_ENDPOINT" => first_marker(
            line,
            &[
                "service ",
                "ServerBuilder",
                "Server::builder",
                "Register",
                "register_",
                "rpc ",
                "Endpoint",
                "tonic::include_proto",
                "grpc",
                ".proto",
            ],
        ),
        "SERVER_ACTION" => first_marker(
            line,
            &["use server", "server action", "serverAction", "action("],
        ),
        "SCHEMA" => first_marker(
            line,
            &[
                "ApiResource",
                "GraphQL",
                "schema",
                "Schema",
                "#[derive(",
                "proto",
                "model ",
                "Entity",
            ],
        ),
        "SCHEDULED_JOB" => first_marker(
            line,
            &[
                "cron",
                "schedule(",
                "Scheduler",
                "@Scheduled",
                "#[tokio::main]",
                "timer(",
                "job(",
            ],
        ),
        _ => None,
    }
    .or_else(|| signal_evidence(pack, output, line))?;

    Some(marker)
}

pub(crate) fn react_component_evidence(line: &str) -> Option<String> {
    for marker in ["function ", "class "] {
        if let Some(name) = identifier_after(line, marker) {
            if name
                .chars()
                .next()
                .is_some_and(|value| value.is_ascii_uppercase())
                && (marker == "function "
                    || line.contains("extends React.Component")
                    || line.contains("extends Component"))
            {
                return Some("component_definition".to_string());
            }
        }
    }
    for marker in ["const ", "let ", "var "] {
        let Some(name) = identifier_after(line, marker) else {
            continue;
        };
        if !name
            .chars()
            .next()
            .is_some_and(|value| value.is_ascii_uppercase())
        {
            continue;
        }
        let Some((_, right)) = line.split_once('=') else {
            continue;
        };
        if right.contains("=>")
            || right.contains("function")
            || right.contains("memo(")
            || right.contains("forwardRef(")
            || right.contains("defineComponent(")
        {
            return Some("component_definition".to_string());
        }
    }
    None
}

pub(crate) fn first_marker(line: &str, needles: &[&str]) -> Option<String> {
    needles
        .iter()
        .find(|needle| {
            if needle
                .chars()
                .all(|value| value.is_ascii_alphanumeric() || value == '_')
            {
                contains_word(line, needle)
            } else {
                line.contains(**needle)
            }
        })
        .map(|needle| (*needle).to_string())
}

pub(crate) fn contains_word(line: &str, word: &str) -> bool {
    let mut offset = 0;
    while let Some(found) = line[offset..].find(word) {
        let start = offset + found;
        let end = start + word.len();
        let before = line[..start].chars().next_back();
        let after = line[end..].chars().next();
        let boundary = |value: Option<char>| {
            value.is_none_or(|value| !value.is_ascii_alphanumeric() && value != '_')
        };
        if boundary(before) && boundary(after) {
            return true;
        }
        offset = end;
        if offset >= line.len() {
            break;
        }
    }
    false
}

pub(crate) fn signal_evidence(pack: &FrameworkPack, output: &str, line: &str) -> Option<String> {
    pack.signals.iter().find_map(|signal| {
        let needle = signal_needle(signal);
        let lower = needle.to_ascii_lowercase();
        let belongs = match output {
            "COMPONENT" => [
                "component",
                "widget",
                "jsx",
                "qobject",
                "gtk",
                "qwidget",
                "uclass",
            ]
            .iter()
            .any(|token| signal_token_matches(&lower, token)),
            "RENDERS" => ["render", "widget", "page", "jsx", "component"]
                .iter()
                .any(|token| signal_token_matches(&lower, token)),
            "EVENT_HANDLER" => [
                "event", "signal", "callback", "connect", "handler", "listener", "on",
            ]
            .iter()
            .any(|token| signal_token_matches(&lower, token)),
            "SERVICE" => ["service", "controller", "endpoint", "api"]
                .iter()
                .any(|token| signal_token_matches(&lower, token)),
            "MIDDLEWARE" => ["middleware", "filter", "pipeline", "use", "scope"]
                .iter()
                .any(|token| signal_token_matches(&lower, token)),
            "DEPENDENCY" => ["inject", "depend", "singleton", "provide", "service"]
                .iter()
                .any(|token| signal_token_matches(&lower, token)),
            "ASYNC_CALLS" => ["async", "await", "spawn", "future", "event", "io_", "uv_"]
                .iter()
                .any(|token| signal_token_matches(&lower, token)),
            "RPC_ENDPOINT" => ["grpc", "rpc", "service", "endpoint", "register", "proto"]
                .iter()
                .any(|token| signal_token_matches(&lower, token)),
            "SERVER_ACTION" => ["action", "server"]
                .iter()
                .any(|token| signal_token_matches(&lower, token)),
            "SCHEMA" => ["schema", "model", "entity", "resource", "graphql", "proto"]
                .iter()
                .any(|token| signal_token_matches(&lower, token)),
            "SCHEDULED_JOB" => ["cron", "schedule", "job", "timer", "tokio"]
                .iter()
                .any(|token| signal_token_matches(&lower, token)),
            _ => false,
        };
        if belongs && line.contains(&needle) {
            Some(format!("pack_signal:{needle}"))
        } else {
            None
        }
    })
}

pub(crate) fn signal_token_matches(value: &str, token: &str) -> bool {
    if token.len() <= 2 {
        value == token || contains_word(value, token)
    } else {
        value.contains(token)
    }
}

pub(crate) fn signal_marker(pack: &FrameworkPack, line: &str, tokens: &[&str]) -> Option<String> {
    pack.signals.iter().find_map(|signal| {
        let needle = signal_needle(signal);
        if tokens
            .iter()
            .any(|token| needle.eq_ignore_ascii_case(token))
            && line.contains(&needle)
        {
            Some(needle)
        } else {
            None
        }
    })
}

pub(crate) fn service_name_marker(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    if lower.contains("service") && (line.contains("class ") || line.contains("struct ")) {
        Some("service_name".to_string())
    } else {
        None
    }
}

pub(crate) fn fact_target_name(output: &str, line: &str) -> Option<String> {
    match output {
        "MIDDLEWARE" => {
            if (line.trim_start().starts_with('@') && line.contains("middleware("))
                || (line.contains("def ") && line.contains("middleware("))
            {
                None
            } else {
                call_last_argument_handler(line, ".use(")
                    .or_else(|| generic_argument_after(line, &["UseMiddleware<"]))
                    .or_else(|| {
                        constructed_type_after(line, &[".addMiddleware(", "->addMiddleware("])
                    })
                    .or_else(|| {
                        argument_after(
                            line,
                            &[
                                ".use(",
                                "use(",
                                "use ",
                                "Use(",
                                "UseMiddleware(",
                                ".addMiddleware(",
                                ".addHook(",
                                "InsertFilter(",
                                "@UseGuards(",
                                "@UseInterceptors(",
                                "@UsePipes(",
                                "before_request(",
                                "after_request(",
                                "add_middleware(",
                                "middleware(",
                                ".layer(",
                                ".wrap(",
                                ".with(",
                                "filter(",
                            ],
                        )
                    })
            }
        }
        "EVENT_HANDLER" => {
            let markers = ["addEventListener(", "addListener(", ".on(", "subscribe("];
            event_call_last_argument(line)
                .or_else(|| {
                    markers.iter().find_map(|marker| {
                        let start = line.find(marker)? + marker.len();
                        registration_handler(&line[start..])
                    })
                })
                .or_else(|| event_property_target(line))
        }
        "ASYNC_CALLS" => quoted_argument_after(line, "invoke(")
            .or_else(|| argument_after(line, &["spawn(", "publish(", "subscribe(", ".then("])),
        "DEPENDENCY" => {
            generic_argument_after(line, &["AddScoped<", "AddSingleton<", "AddTransient<"])
                .or_else(|| {
                    argument_after(
                        line,
                        &[
                            "Depends(",
                            "inject(",
                            "Inject(",
                            "Provide(",
                            "AddScoped(",
                            "AddSingleton(",
                            "AddTransient(",
                        ],
                    )
                })
                .or_else(|| dependency_type_name(line))
        }
        "RPC_ENDPOINT" => argument_after(line, &["add_service(", "RegisterService("])
            .and_then(|name| registration_type_name(line, &name).or(Some(name)))
            .or_else(|| registration_type_name(line, ""))
            .or_else(|| identifier_after(line, "service "))
            .or_else(|| identifier_after(line, "class "))
            .or_else(|| identifier_after(line, "struct "))
            .or_else(|| identifier_after(line, "type "))
            .or_else(|| identifier_after(line, "fn ")),
        "SERVICE" | "COMPONENT" | "SCHEMA" => argument_after(line, &["BEGIN_MESSAGE_MAP("])
            .or_else(|| identifier_after(line, "class "))
            .or_else(|| identifier_after(line, "struct "))
            .or_else(|| identifier_after(line, "interface "))
            .or_else(|| identifier_after(line, "type "))
            .or_else(|| {
                matches!(output, "COMPONENT")
                    .then(|| assignment_target_before(line, "defineComponent("))
                    .flatten()
            }),
        "RENDERS" => assignment_target_before(line, "defineComponent("),
        "SCHEDULED_JOB" => identifier_after(line, "fn ")
            .or_else(|| identifier_after(line, "def "))
            .or_else(|| identifier_after(line, "void ")),
        _ => None,
    }
}

fn call_last_argument_handler(line: &str, marker: &str) -> Option<String> {
    let open = line.find(marker)? + marker.len() - 1;
    let close = matching_parenthesis(line, open)?;
    last_top_level_argument(&line[open + 1..close])
        .and_then(|(_, candidate)| handler_name_from_expression(candidate))
}

pub(crate) fn dependency_type_name(line: &str) -> Option<String> {
    for marker in ["@Autowired", "@Inject", "Dependency"] {
        let Some(start) = line.find(marker) else {
            continue;
        };
        let rest = &line[start + marker.len()..];
        if let Some(name) = rest
            .split_whitespace()
            .filter(|value| !value.starts_with('@'))
            .map(|value| value.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_'))
            .find(|value| {
                value
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_uppercase())
            })
        {
            return Some(name.to_string());
        }
    }
    None
}

pub(crate) fn event_property_target(line: &str) -> Option<String> {
    for marker in [
        "onClick",
        "onTap",
        "onPressed",
        "@onclick",
        "@click",
        "Clicked",
        "ON_COMMAND",
    ] {
        let Some(start) = line.find(marker) else {
            continue;
        };
        let rest = line[start + marker.len()..]
            .trim_start_matches([':', '=', '+', '>', '"', '\\'])
            .trim_start();
        let candidate = rest
            .trim_start_matches('{')
            .trim_matches(|value: char| matches!(value, '"' | '\'' | '}' | ',' | ';'))
            .split(|value: char| {
                matches!(
                    value,
                    ',' | ')' | '}' | ';' | ' ' | '"' | '\'' | '\\' | '<' | '>'
                )
            })
            .next()
            .unwrap_or_default()
            .trim_matches(|value: char| !value.is_ascii_alphanumeric() && value != '_');
        if !candidate.is_empty() {
            return Some(candidate.to_string());
        }
    }
    None
}

pub(crate) fn event_call_last_argument(line: &str) -> Option<String> {
    for marker in [
        "g_signal_connect(",
        "QObject::connect(",
        "uv_read_start(",
        "ON_COMMAND(",
    ] {
        let Some(start) = line.find(marker) else {
            continue;
        };
        let candidate = line[start + marker.len()..]
            .split([')', ';'])
            .next()
            .unwrap_or_default()
            .split(',')
            .map(|value| {
                value.trim().trim_matches(|value: char| {
                    matches!(value, '&' | '*' | '"' | '\'' | '(' | ')' | '}' | ';')
                })
            })
            .rfind(|value| !value.is_empty())
            .unwrap_or_default()
            .rsplit(|value: char| !value.is_ascii_alphanumeric() && value != '_')
            .next()
            .unwrap_or_default();
        if !candidate.is_empty() {
            return Some(candidate.to_string());
        }
    }
    None
}

pub(crate) fn fact_properties(output: &str, line: &str) -> BTreeMap<String, String> {
    let mut properties = BTreeMap::new();
    if let Some(target) = fact_target_name(output, line) {
        properties.insert("target".to_string(), target);
    }
    if output == "ASYNC_CALLS" && line.contains("invoke(") && !properties.contains_key("target") {
        properties.insert("resolution".to_string(), "dynamic".to_string());
    }
    match output {
        "MIDDLEWARE" if line.contains("Route::middleware(") => {
            properties.insert("resolution".to_string(), "framework_alias".to_string());
        }
        "EVENT_HANDLER" => {
            if let Some(event) = event_name(line) {
                properties.insert("event".to_string(), event);
            }
        }
        "RPC_ENDPOINT" => {
            if let Some(service) = identifier_after(line, "service ") {
                properties.insert("service".to_string(), service);
            }
            if let Some(method) = identifier_after(line, "rpc ") {
                properties.insert("method".to_string(), method);
            }
        }
        "SCHEDULED_JOB" => {
            if let Some(schedule) = first_quoted_value(line) {
                properties.insert("schedule".to_string(), schedule);
            }
        }
        "SCHEMA" => {
            if let Some(name) = identifier_after(line, "class ")
                .or_else(|| identifier_after(line, "struct "))
                .or_else(|| identifier_after(line, "model "))
            {
                properties.insert("name".to_string(), name);
            }
        }
        "RENDERS" => {
            if let Some(start) = line.find("return <") {
                let candidate = line[start + "return <".len()..]
                    .trim_start()
                    .split([' ', '>', '/', '{'])
                    .next()
                    .unwrap_or_default();
                if !candidate.is_empty() {
                    properties.insert("target".to_string(), candidate.to_string());
                }
            }
        }
        _ => {}
    }
    properties
}

pub(crate) fn event_name(line: &str) -> Option<String> {
    for marker in [
        "@onclick",
        "@click",
        "@onchange",
        "onClick",
        "onTap",
        "onPressed",
    ] {
        if line.contains(marker) {
            return Some(marker.trim_start_matches('@').to_ascii_lowercase());
        }
    }
    first_quoted_value(line)
}

pub(crate) fn first_quoted_value(line: &str) -> Option<String> {
    let bytes = line.as_bytes();
    for start in 0..bytes.len() {
        if bytes[start] != b'"' && bytes[start] != b'\'' {
            continue;
        }
        let quote = bytes[start];
        let end = bytes[start + 1..]
            .iter()
            .position(|value| *value == quote)
            .map(|offset| start + 1 + offset)?;
        return Some(line[start + 1..end].to_string());
    }
    None
}

pub(crate) fn argument_after(line: &str, markers: &[&str]) -> Option<String> {
    let marker = markers.iter().find(|marker| line.contains(**marker))?;
    let start = line.find(marker)? + marker.len();
    let rest = &line[start..];
    let candidate = if marker.ends_with("addHook(") {
        rest.rsplit(',').next()?.trim()
    } else {
        rest.trim_start()
            .trim_matches(|value: char| value == ')' || value == ';' || value == ',')
            .split([')', ';', ','])
            .next()?
            .trim()
    };
    let candidate = candidate
        .split_whitespace()
        .next()
        .unwrap_or(candidate)
        .trim_matches(|value: char| !value.is_ascii_alphanumeric() && value != '_');
    (!candidate.is_empty() && candidate != "self" && candidate != "this" && candidate != "async")
        .then(|| candidate.to_string())
}

pub(crate) fn quoted_argument_after(line: &str, marker: &str) -> Option<String> {
    let start = line.find(marker)? + marker.len();
    first_quoted_value(&line[start..])
}

pub(crate) fn constructed_type_after(line: &str, markers: &[&str]) -> Option<String> {
    let marker = markers.iter().find(|marker| line.contains(**marker))?;
    let start = line.find(marker)? + marker.len();
    let rest = line[start..].split([')', ';']).next()?;
    let start = rest.find("new ")? + "new ".len();
    let name: String = rest[start..]
        .chars()
        .take_while(|value| value.is_ascii_alphanumeric() || *value == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

pub(crate) fn registration_type_name(line: &str, requested_name: &str) -> Option<String> {
    let marker = ["RegisterService(", "register_service(", "add_service("]
        .iter()
        .find(|marker| line.contains(**marker))?;
    let start = line.find(marker)?;
    let variable = if requested_name.is_empty() {
        line[start + marker.len()..]
            .split([')', ',', ';'])
            .next()?
            .trim()
            .trim_start_matches(['&', '*'])
            .to_string()
    } else {
        requested_name.to_string()
    };
    if variable.is_empty() {
        return None;
    }

    let before = &line[..start];
    if let Some((_, after_ampersand)) = before.rsplit_once('&') {
        let name: String = after_ampersand
            .chars()
            .take_while(|value| value.is_ascii_alphanumeric() || *value == '_')
            .collect();
        if !name.is_empty() && name != variable {
            return Some(name);
        }
    }

    let tokens = before
        .rsplit([';', '{', '}'])
        .find(|segment| !segment.trim().is_empty())?
        .split_whitespace()
        .collect::<Vec<_>>();
    let position = tokens.iter().rposition(|value| {
        value.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_') == variable
    })?;
    let candidate = tokens.get(position.checked_sub(1)?)?;
    let candidate = candidate.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_');
    (!candidate.is_empty() && candidate != "=" && candidate != ":=").then(|| candidate.to_string())
}

pub(crate) fn generic_argument_after(line: &str, markers: &[&str]) -> Option<String> {
    let marker = markers.iter().find(|marker| line.contains(**marker))?;
    let start = line.find(marker)? + marker.len();
    let candidate = line[start..]
        .split(['>', ',', ')', ';'])
        .next()?
        .trim()
        .rsplit(|value: char| !value.is_ascii_alphanumeric() && value != '_')
        .next()?;
    (!candidate.is_empty()).then(|| candidate.to_string())
}

pub(crate) fn assignment_target_before(line: &str, marker: &str) -> Option<String> {
    let marker_start = line.find(marker)?;
    let left = line[..marker_start].rsplit_once('=')?.0.trim();
    let name = left
        .rsplit(|value: char| !value.is_ascii_alphanumeric() && value != '_')
        .next()?;
    (!name.is_empty()).then(|| name.to_string())
}

pub(crate) fn relation_kind_for_fact(kind: &str) -> Option<&'static str> {
    match kind {
        "COMPONENT" => Some("DECLARES_COMPONENT"),
        "RENDERS" => Some("RENDERS"),
        "EVENT_HANDLER" => Some("HANDLES_EVENT"),
        "SERVICE" => Some("DECLARES_SERVICE"),
        "MIDDLEWARE" => Some("USES_MIDDLEWARE"),
        "DEPENDENCY" => Some("INJECTS"),
        "ASYNC_CALLS" => Some("SPAWNS_ASYNC"),
        "RPC_ENDPOINT" => Some("EXPOSES_RPC"),
        "SERVER_ACTION" => Some("DECLARES_SERVER_ACTION"),
        "SCHEMA" => Some("DECLARES_SCHEMA"),
        "SCHEDULED_JOB" => Some("SCHEDULES_JOB"),
        _ => None,
    }
}

pub(crate) fn route_prefix(line: &str) -> Option<String> {
    if !(line.contains("@RequestMapping")
        || line.contains("@Controller(")
        || line.contains("@Path(")
        || line.trim_start().starts_with("[RoutePrefix(")
        || (line.trim_start().starts_with("[Route(") && !line.contains("#[Route(")))
    {
        return None;
    }
    java_route_paths(line)
        .into_iter()
        .next()
        .or_else(|| {
            annotation_route_paths(
                line,
                &[
                    "@RequestMapping",
                    "@Controller",
                    "@Path",
                    "[RoutePrefix",
                    "[Route",
                ],
            )
            .and_then(|(paths, _)| paths.into_iter().next())
        })
        .or_else(|| first_route_path(line).map(|(path, _)| path))
}

pub(crate) fn has_http_method_annotation(line: &str) -> bool {
    [
        "@Get",
        "@Post",
        "@Put",
        "@Patch",
        "@Delete",
        "@GET",
        "@POST",
        "@PUT",
        "@PATCH",
        "@DELETE",
        "@HEAD",
        "@OPTIONS",
        "[HttpGet",
        "[HttpPost",
        "[HttpPut",
        "[HttpPatch",
        "[HttpDelete",
        "[HttpHead",
        "[HttpOptions",
    ]
    .iter()
    .any(|marker| line.contains(marker))
}

pub(crate) fn combine_route_prefix(prefix: Option<&str>, route: &str) -> String {
    let route = normalize_route_path(route);
    let Some(prefix) = prefix.filter(|value| !value.is_empty() && *value != "/") else {
        return route;
    };
    let prefix = normalize_route_path(prefix);
    if route == "/" {
        return prefix;
    }
    format!("{}{}", prefix.trim_end_matches('/'), route)
}

fn normalize_route_path(path: &str) -> String {
    let path = path.trim();
    if path.is_empty() || path == "/" {
        "/".to_string()
    } else {
        format!("/{}", path.trim_start_matches('/'))
    }
}

pub(crate) fn route_method(line: &str) -> Option<&'static str> {
    if let Some(method) = configured_route_method(line) {
        return Some(method);
    }
    let methods: &[(&[&str], &str)] = &[
        (
            &[
                ".get(",
                "->get(",
                "::get(",
                "GET(",
                "MapGet(",
                "@GetMapping",
                "@Get(",
                "@get(",
                "@GET(",
                "@app.get(",
                "@router.get(",
                "@GET",
                "#[get(",
                "[Get(",
                "Get(",
                "[HttpGet",
                "HttpGet(",
            ],
            "GET",
        ),
        (
            &[
                ".post(",
                "->post(",
                "::post(",
                "POST(",
                "MapPost(",
                "@PostMapping",
                "@Post(",
                "@post(",
                "@POST(",
                "@app.post(",
                "@router.post(",
                "@POST",
                "#[post(",
                "[Post(",
                "Post(",
                "[HttpPost",
                "HttpPost(",
            ],
            "POST",
        ),
        (
            &[
                ".put(",
                "->put(",
                "::put(",
                "PUT(",
                "MapPut(",
                "@PutMapping",
                "@Put(",
                "@put(",
                "@PUT(",
                "@app.put(",
                "@router.put(",
                "@PUT",
                "#[put(",
                "[Put(",
                "Put(",
                "[HttpPut",
                "HttpPut(",
            ],
            "PUT",
        ),
        (
            &[
                ".patch(",
                "->patch(",
                "::patch(",
                "PATCH(",
                "MapPatch(",
                "@PatchMapping",
                "@Patch(",
                "@patch(",
                "@PATCH(",
                "@app.patch(",
                "@router.patch(",
                "@PATCH",
                "#[patch(",
                "[Patch(",
                "Patch(",
                "[HttpPatch",
                "HttpPatch(",
            ],
            "PATCH",
        ),
        (
            &[
                ".delete(",
                "->delete(",
                "::delete(",
                "DELETE(",
                "MapDelete(",
                "@DeleteMapping",
                "@Delete(",
                "@delete(",
                "@DELETE(",
                "@app.delete(",
                "@router.delete(",
                "@DELETE",
                "#[delete(",
                "[Delete(",
                "Delete(",
                "[HttpDelete",
                "HttpDelete(",
            ],
            "DELETE",
        ),
        (
            &[
                ".head(",
                "->head(",
                "::head(",
                "HEAD(",
                "@Head(",
                "@head(",
                "@HEAD(",
                "@app.head(",
                "@router.head(",
                "@HEAD",
                "#[head(",
                "[Head(",
                "Head(",
                "[HttpHead",
                "HttpHead(",
            ],
            "HEAD",
        ),
        (
            &[
                ".options(",
                "->options(",
                "::options(",
                "OPTIONS(",
                "@Options(",
                "@options(",
                "@OPTIONS(",
                "@app.options(",
                "@router.options(",
                "@OPTIONS",
                "#[options(",
                "[Options(",
                "Options(",
                "[HttpOptions",
                "HttpOptions(",
            ],
            "OPTIONS",
        ),
    ];
    for (patterns, method) in methods {
        if patterns.iter().any(|pattern| line.contains(pattern)) {
            return Some(method);
        }
    }
    if line.contains(".route(")
        || line.contains(".add_url_rule(")
        || line.contains(".add_api_route(")
        || line.contains(".addRoute(")
        || line.contains(".and_then(")
        || line.contains("Route(")
        || line.contains("Router(")
        || line.contains("Route::new")
        || line.contains(".at(")
        || line.contains("path(")
        || line.contains("CROW_ROUTE")
        || line.contains("ADD_METHOD_TO")
        || line.contains("@app.route")
        || line.contains("@router.route")
        || line.contains("@route(")
        || line.contains("#[Route")
        || (line.contains("[Route(") && !line.contains("#[Route("))
        || line.contains("HandleFunc(")
        || line.contains("scope(")
        || line.contains("r.on ")
        || line.contains("@page ")
    {
        if line.contains("@app.route") || line.contains("@router.route") || line.contains("@route(")
        {
            if line.contains("POST") || line.contains("post") {
                return Some("POST");
            }
            if line.contains("PUT") || line.contains("put") {
                return Some("PUT");
            }
            if line.contains("DELETE") || line.contains("delete") {
                return Some("DELETE");
            }
            return Some("GET");
        }
        if line.contains("GET") || line.contains("get(") {
            return Some("GET");
        }
        if line.contains("POST") || line.contains("post(") {
            return Some("POST");
        }
        return Some("ANY");
    }
    let trimmed = line.trim_start();
    if trimmed.starts_with("GET ")
        || trimmed.starts_with("get ")
        || trimmed.starts_with("map ")
        || trimmed.starts_with("@page ")
    {
        return Some("GET");
    }
    if trimmed.starts_with("POST ") || trimmed.starts_with("post ") {
        return Some("POST");
    }
    if line.contains("@RequestMapping") {
        return request_mapping_method(line).or(Some("ANY"));
    }
    if line.contains("#[route(") {
        return Some("ANY");
    }
    None
}

pub(crate) fn configured_route_method(line: &str) -> Option<&'static str> {
    let lower = line.to_ascii_lowercase();
    if !(lower.contains("methods")
        || lower.contains("method =")
        || lower.contains("method:")
        || lower.contains("via:")
        || lower.contains("acceptverbs"))
    {
        return None;
    }
    let quoted = quoted_values(line);
    let methods = [
        ("GET", "get"),
        ("POST", "post"),
        ("PUT", "put"),
        ("PATCH", "patch"),
        ("DELETE", "delete"),
        ("HEAD", "head"),
        ("OPTIONS", "options"),
    ];
    let mut found = methods.iter().filter_map(|(method, lower_method)| {
        (quoted
            .iter()
            .any(|value| value.eq_ignore_ascii_case(method))
            || lower.contains("via:") && lower.contains(&format!(":{lower_method}")))
        .then_some(*method)
    });
    let method = found.next()?;
    if found.next().is_some() {
        Some("ANY")
    } else {
        Some(method)
    }
}

pub(crate) fn request_mapping_method(line: &str) -> Option<&'static str> {
    let methods = [
        ("RequestMethod.GET", "GET"),
        ("RequestMethod.POST", "POST"),
        ("RequestMethod.PUT", "PUT"),
        ("RequestMethod.PATCH", "PATCH"),
        ("RequestMethod.DELETE", "DELETE"),
        ("RequestMethod.OPTIONS", "OPTIONS"),
        ("RequestMethod.HEAD", "HEAD"),
    ];
    methods
        .iter()
        .find_map(|(needle, method)| line.contains(needle).then_some(*method))
}

pub(crate) fn first_route_path(line: &str) -> Option<(String, usize)> {
    let bytes = line.as_bytes();
    let mut start = 0usize;
    while start < bytes.len() {
        let Some(quote_offset) = bytes[start..]
            .iter()
            .position(|value| matches!(value, b'"' | b'\''))
        else {
            break;
        };
        let quote_start = start + quote_offset;
        let quote = bytes[quote_start];
        let Some(end_offset) = bytes[quote_start + 1..]
            .iter()
            .position(|value| *value == quote)
        else {
            break;
        };
        let end = quote_start + 1 + end_offset;
        let value = &line[quote_start + 1..end];
        if value.starts_with('/') || value == "*" {
            return Some((value.to_string(), end + 1));
        }
        start = end + 1;
    }
    for method in [
        "GET ", "POST ", "PUT ", "PATCH ", "DELETE ", "HEAD ", "OPTIONS ", "get ", "post ", "put ",
        "patch ", "delete ", "head ", "options ",
    ] {
        let Some(rest) = line.trim_start().strip_prefix(method) else {
            continue;
        };
        let rest = rest.trim_start();
        let (path, consumed) = if matches!(rest.as_bytes().first(), Some(b'"' | b'\'')) {
            let quote = rest.as_bytes()[0];
            let offset = rest.as_bytes()[1..]
                .iter()
                .position(|value| *value == quote)?;
            (&rest[1..1 + offset], offset + 2)
        } else {
            let path = rest.split_whitespace().next()?;
            (path, path.len())
        };
        if !path.is_empty() {
            let rest_start = line.len() - rest.len();
            return Some((path.to_string(), rest_start + consumed));
        }
    }
    None
}

pub(crate) fn annotation_route_paths(line: &str, markers: &[&str]) -> Option<(Vec<String>, usize)> {
    let mut candidates = markers
        .iter()
        .flat_map(|marker| {
            line.match_indices(marker)
                .map(|(start, _)| (start, *marker))
        })
        .collect::<Vec<_>>();
    candidates.sort_unstable_by_key(|(start, _)| *start);

    for (marker_start, marker) in candidates {
        let marker_end = marker_start + marker.len();
        let tail = &line[marker_end..];
        let trimmed = tail.trim_start();
        if !trimmed.starts_with('(') {
            let bare_annotation = marker.starts_with('[') && trimmed.starts_with(']')
                || marker.starts_with('@')
                    && tail
                        .chars()
                        .next()
                        .is_none_or(|value| value.is_whitespace());
            if bare_annotation {
                return Some((vec![String::new()], marker_end));
            }
            continue;
        }
        let open = marker_end + tail.find('(')?;
        let close = matching_parenthesis(line, open)?;
        let argument = first_top_level_argument(&line[open + 1..close]).trim();
        if argument.is_empty() || top_level_assignment(argument) {
            return Some((vec![String::new()], close + 1));
        }
        let paths = quoted_values(argument);
        if !paths.is_empty() {
            return Some((paths, close + 1));
        }
    }
    None
}

pub(crate) fn matching_parenthesis(line: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for (offset, value) in line[open..].char_indices() {
        let index = open + offset;
        if let Some(current_quote) = quote {
            if escaped {
                escaped = false;
            } else if value == '\\' {
                escaped = true;
            } else if value == current_quote {
                quote = None;
            }
            continue;
        }
        match value {
            '"' | '\'' => quote = Some(value),
            '(' => depth += 1,
            ')' if depth == 1 => return Some(index),
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    None
}

fn first_top_level_argument(arguments: &str) -> &str {
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for (index, value) in arguments.char_indices() {
        if let Some(current_quote) = quote {
            if escaped {
                escaped = false;
            } else if value == '\\' {
                escaped = true;
            } else if value == current_quote {
                quote = None;
            }
            continue;
        }
        match value {
            '"' | '\'' => quote = Some(value),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return &arguments[..index],
            _ => {}
        }
    }
    arguments
}

fn top_level_assignment(argument: &str) -> bool {
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for value in argument.chars() {
        if let Some(current_quote) = quote {
            if escaped {
                escaped = false;
            } else if value == '\\' {
                escaped = true;
            } else if value == current_quote {
                quote = None;
            }
            continue;
        }
        match value {
            '"' | '\'' => quote = Some(value),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            '=' if depth == 0 => return true,
            _ => {}
        }
    }
    false
}

pub(crate) fn route_paths(line: &str) -> Vec<String> {
    quoted_values(line)
        .into_iter()
        .filter(|value| value.starts_with('/') || value == "*")
        .collect()
}

fn quoted_values(line: &str) -> Vec<String> {
    let bytes = line.as_bytes();
    let mut values = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let quote = bytes[index];
        if quote != b'"' && quote != b'\'' {
            index += 1;
            continue;
        }
        let Some(offset) = bytes[index + 1..].iter().position(|value| *value == quote) else {
            break;
        };
        let end = index + 1 + offset;
        let value = &line[index + 1..end];
        values.push(value.to_string());
        index = end + 1;
    }
    values
}

pub(crate) fn java_route_paths(line: &str) -> Vec<String> {
    let Some((annotation_start, _)) = [
        "@RequestMapping",
        "@GetMapping",
        "@PostMapping",
        "@PutMapping",
        "@PatchMapping",
        "@DeleteMapping",
        "@OptionsMapping",
        "@HeadMapping",
    ]
    .iter()
    .find_map(|name| line.find(name).map(|start| (start, *name))) else {
        return Vec::new();
    };
    let Some(open) = line[annotation_start..]
        .find('(')
        .map(|offset| offset + annotation_start)
    else {
        return Vec::new();
    };
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    let mut argument_start = open + 1;
    let mut arguments = Vec::new();
    for (offset, value) in line[open + 1..].char_indices() {
        let index = open + 1 + offset;
        if let Some(current_quote) = quote {
            if escaped {
                escaped = false;
            } else if value == '\\' {
                escaped = true;
            } else if value == current_quote {
                quote = None;
            }
            continue;
        }
        if value == '"' || value == '\'' {
            quote = Some(value);
        } else if value == '{' {
            depth += 1;
        } else if value == '}' {
            depth = depth.saturating_sub(1);
        } else if value == ',' && depth == 0 {
            arguments.push(&line[argument_start..index]);
            argument_start = index + 1;
        } else if value == ')' && depth == 0 {
            arguments.push(&line[argument_start..index]);
            break;
        }
    }
    let mut paths = Vec::new();
    for (index, argument) in arguments.iter().enumerate() {
        let trimmed = argument.trim_start();
        let selected = index == 0 && !trimmed.contains('=')
            || trimmed.starts_with("path") && trimmed[4..].trim_start().starts_with('=')
            || trimmed.starts_with("value") && trimmed[5..].trim_start().starts_with('=');
        if selected {
            for path in quoted_values(argument) {
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }
    }
    paths
}

pub(crate) fn registration_handler(rest: &str) -> Option<String> {
    for marker in [".and_then(", ".and(", ".map("] {
        if let Some(open) = rest.find(marker) {
            let candidate = rest[open + marker.len()..]
                .split([')', ',', ';'])
                .next()?
                .trim();
            let name = candidate
                .rsplit(|value: char| !value.is_ascii_alphanumeric() && value != '_')
                .find(|value| !value.is_empty())
                .unwrap_or(candidate);
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    if let Some(open) = rest.find(".to(") {
        let candidate = rest[open + 4..].split([')', ',', ';']).next()?.trim();
        if !candidate.is_empty() {
            return Some(candidate.to_string());
        }
    }
    if let Some((_, candidate)) = last_top_level_argument(rest) {
        if let Some(name) = handler_name_from_expression(candidate) {
            return Some(name);
        }
    }
    let open = rest.find(")(")? + 2;
    let candidate = rest[open..].split([')', ';', ',']).next()?.trim();
    handler_name_from_expression(candidate)
}

fn last_top_level_argument(input: &str) -> Option<(usize, &str)> {
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    let mut last = None;
    let mut stopped = false;
    for (index, value) in input.char_indices() {
        if let Some(current_quote) = quote {
            if escaped {
                escaped = false;
            } else if value == '\\' {
                escaped = true;
            } else if value == current_quote {
                quote = None;
            }
            continue;
        }
        match value {
            '"' | '\'' | '`' => quote = Some(value),
            '(' | '[' | '{' => depth += 1,
            ')' if depth == 0
                && input[index + value.len_utf8()..]
                    .trim_start()
                    .starts_with(',') => {}
            ')' if depth == 0 => {
                let candidate = input[start..index].trim();
                if !candidate.is_empty() {
                    let offset = start + input[start..index].find(candidate).unwrap_or_default();
                    last = Some((offset, candidate));
                }
                stopped = true;
                break;
            }
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                let candidate = input[start..index].trim();
                if !candidate.is_empty() {
                    let offset = start + input[start..index].find(candidate).unwrap_or_default();
                    last = Some((offset, candidate));
                }
                start = index + value.len_utf8();
            }
            ';' if depth == 0 => {
                stopped = true;
                break;
            }
            _ => {}
        }
    }
    if !stopped {
        let candidate = input[start..].trim().trim_end_matches([')', ';']);
        if !candidate.is_empty() {
            let offset = start + input[start..].find(candidate).unwrap_or_default();
            last = Some((offset, candidate));
        }
    }
    last
}

fn handler_name_from_expression(candidate: &str) -> Option<String> {
    let candidate = candidate
        .trim()
        .trim_matches(['"', '\'', '`'])
        .trim_start_matches(['&', '*']);
    let candidate = candidate.strip_prefix("async ").unwrap_or(candidate);
    let candidate = candidate.strip_prefix("function ").unwrap_or(candidate);
    if candidate.is_empty() || candidate.starts_with(['(', '{', '[']) || candidate.contains("=>") {
        return None;
    }
    if let Some(open) = candidate.find('(') {
        let callable = candidate[..open]
            .rsplit(|value: char| !value.is_ascii_alphanumeric() && value != '_')
            .find(|value| !value.is_empty())?;
        if matches!(
            callable.to_ascii_lowercase().as_str(),
            "get" | "post" | "put" | "patch" | "delete" | "head" | "options"
        ) {
            let close = matching_parenthesis(candidate, open)?;
            return last_top_level_argument(&candidate[open + 1..close])
                .and_then(|(_, value)| handler_name_from_expression(value));
        }
        return Some(callable.to_string());
    }
    let name = candidate
        .rsplit(|value: char| !value.is_ascii_alphanumeric() && value != '_')
        .find(|value| !value.is_empty())?;
    Some(name.to_string())
}

pub(crate) fn javascript_chained_route_calls(source: &str) -> Vec<(String, Option<String>, usize)> {
    let Some(route_open) = source.find(".route(").map(|start| start + ".route".len()) else {
        return Vec::new();
    };
    let Some(route_close) = matching_parenthesis(source, route_open) else {
        return Vec::new();
    };
    let methods = [
        (".get(", "GET"),
        (".post(", "POST"),
        (".put(", "PUT"),
        (".patch(", "PATCH"),
        (".delete(", "DELETE"),
        (".head(", "HEAD"),
        (".options(", "OPTIONS"),
    ];
    let mut cursor = route_close + 1;
    let mut calls = Vec::new();
    while cursor < source.len() {
        let Some((start, marker, method)) = methods
            .iter()
            .filter_map(|(marker, method)| {
                source[cursor..]
                    .find(marker)
                    .map(|offset| (cursor + offset, *marker, *method))
            })
            .min_by_key(|(start, _, _)| *start)
        else {
            break;
        };
        let open = start + marker.len() - 1;
        let Some(close) = matching_parenthesis(source, open) else {
            break;
        };
        let handler = last_top_level_argument(&source[open + 1..close])
            .and_then(|(_, value)| handler_name_from_expression(value));
        calls.push((
            method.to_string(),
            handler,
            source[..start].matches('\n').count(),
        ));
        cursor = close + 1;
    }
    calls
}

pub(crate) fn macro_registration_handler(line: &str) -> Option<String> {
    let start = line.find("ADD_METHOD_TO(")? + "ADD_METHOD_TO(".len();
    let candidate = line[start..].split(',').next()?.trim();
    let candidate = candidate.trim_start_matches(['&', '*']);
    let name = candidate
        .rsplit(|value: char| !value.is_ascii_alphanumeric() && value != '_')
        .next()
        .unwrap_or(candidate);
    (!name.is_empty()).then(|| name.to_string())
}

pub(crate) fn config_route_handler(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    if !["GET ", "POST ", "PUT ", "PATCH ", "DELETE "]
        .iter()
        .any(|method| trimmed.starts_with(method))
    {
        return None;
    }
    let path_start = trimmed.find('/')?;
    let rest = &trimmed[path_start..];
    let target = rest.split_whitespace().nth(1)?;
    let name = target
        .rsplit(|value: char| !value.is_ascii_alphanumeric() && value != '_')
        .next()?;
    (!name.is_empty()).then(|| name.to_string())
}

pub(crate) fn annotation_handler_name(line: &str) -> Option<String> {
    if !(line.contains('@') || line.contains('[') || line.contains("#[")) {
        return None;
    }
    if !(has_http_method_annotation(line)
        || line.contains("@RequestMapping")
        || line.contains("@Path(")
        || line.contains("#[route(")
        || line.contains("#[Route(")
        || line.contains("#[get(")
        || line.contains("#[Get(")
        || line.contains("#[post(")
        || line.contains("#[Post(")
        || line.contains("#[put(")
        || line.contains("#[Put(")
        || line.contains("#[patch(")
        || line.contains("#[Patch(")
        || line.contains("#[delete(")
        || line.contains("#[Delete(")
        || line.contains("@page "))
    {
        return None;
    }
    let open = line.rfind('(')?;
    let before = line[..open].trim_end();
    let name = before
        .rsplit(|value: char| !value.is_ascii_alphanumeric() && value != '_')
        .next()?;
    let lower = name.to_ascii_lowercase();
    if !name.is_empty()
        && !matches!(
            lower.as_str(),
            "get"
                | "post"
                | "put"
                | "patch"
                | "delete"
                | "route"
                | "httpget"
                | "httppost"
                | "httpput"
                | "httppatch"
                | "httpdelete"
                | "getmapping"
                | "postmapping"
                | "putmapping"
                | "patchmapping"
                | "deletemapping"
        )
    {
        return Some(name.to_string());
    }
    ["class ", "struct ", "interface ", "object "]
        .iter()
        .find_map(|keyword| identifier_after(line, keyword))
}

pub(crate) fn nearby_handler(lines: &[&str], index: usize) -> Option<String> {
    // Java annotations must bind to the first declaration below them. Scanning
    // broad keywords first can otherwise skip that method and bind to a later
    // `void` method in the same window.
    for line in lines.iter().skip(index).take(24) {
        let trimmed = line.trim_start();
        if trimmed.starts_with('@')
            || trimmed.starts_with('[')
            || trimmed.starts_with("#[")
            || !(trimmed.contains("public ")
                || trimmed.contains("protected ")
                || trimmed.contains("private "))
        {
            continue;
        }
        if let Some(open) = line.find('(') {
            let before = line[..open].trim_end();
            let name = before
                .rsplit(|value: char| !value.is_ascii_alphanumeric() && value != '_')
                .next()
                .unwrap_or_default();
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    for line in lines.iter().skip(index).take(24) {
        for keyword in [
            "function ",
            "fn ",
            "func ",
            "def ",
            "void ",
            "class ",
            "struct ",
            "interface ",
            "object ",
        ] {
            if let Some(name) = identifier_after(line, keyword) {
                return Some(name);
            }
        }
    }
    for line in lines.iter().skip(index).take(24) {
        let trimmed = line.trim_start();
        if trimmed.starts_with('@') || trimmed.starts_with('[') || trimmed.starts_with("#[") {
            continue;
        }
        if let Some(open) = line.find('(') {
            let before = line[..open].trim_end();
            let name = before
                .rsplit(|value: char| !value.is_ascii_alphanumeric() && value != '_')
                .next()
                .unwrap_or_default();
            let lower = name.to_ascii_lowercase();
            if !name.is_empty()
                && !matches!(
                    lower.as_str(),
                    "if" | "for" | "while" | "get" | "post" | "route"
                )
            {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// Returns the Java method that contains a functional route registration.
///
/// A `RouterFunction` route often has no named handler at all:
/// `andRoute(GET("/"), request -> ...)`. The method declaration is still an
/// exact source anchor for the registration, unlike guessing a call from the
/// lambda body. Only declaration-shaped lines are accepted so an expression
/// such as `RouterFunctions.route(...)` cannot become a false handler.
pub(crate) fn enclosing_java_method(lines: &[&str], index: usize) -> Option<String> {
    for line in lines[..index.min(lines.len())].iter().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with(['@', '/', '*'])
            || trimmed.contains('=')
            || trimmed.contains(" return ")
            || trimmed.starts_with("return ")
            || !trimmed.contains('{')
            || !trimmed.contains('(')
        {
            continue;
        }
        let open = trimmed.find('(')?;
        let before = trimmed[..open].trim_end();
        let name = before
            .rsplit(|value: char| !value.is_ascii_alphanumeric() && value != '_')
            .next()
            .unwrap_or_default();
        let lower = name.to_ascii_lowercase();
        if name.is_empty()
            || matches!(
                lower.as_str(),
                "if" | "for" | "while" | "switch" | "catch" | "route" | "get" | "post"
            )
        {
            continue;
        }
        return Some(name.to_string());
    }
    None
}

pub(crate) fn identifier_after(line: &str, keyword: &str) -> Option<String> {
    let start = line.find(keyword)? + keyword.len();
    let name: String = line[start..]
        .chars()
        .take_while(|value| value.is_ascii_alphanumeric() || *value == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
pub(crate) fn resolve_symbol(
    documents: &[DocumentOutput],
    path: &str,
    name: &str,
) -> Option<String> {
    let mut same_file_definitions = Vec::new();
    for document in documents.iter().filter(|document| document.path == path) {
        for occurrence in &document.occurrences {
            if occurrence.definition && symbol_matches_name(&occurrence.symbol, name) {
                same_file_definitions.push(occurrence.symbol.clone());
            }
        }
    }
    if let Some(symbol) = select_symbol(documents, same_file_definitions) {
        return Some(symbol);
    }

    // Provider references are stronger than a project-wide name match: they
    // already encode the import/module resolution performed by SCIP or LSP.
    let provider_targets = documents
        .iter()
        .filter(|document| document.path == path)
        .flat_map(|document| document.occurrences.iter())
        .filter(|occurrence| !occurrence.definition)
        .filter(|occurrence| symbol_matches_name(&occurrence.symbol, name))
        .map(|occurrence| occurrence.symbol.clone())
        .collect::<Vec<_>>();
    if let Some(symbol) = select_symbol(documents, provider_targets) {
        return Some(symbol);
    }

    let project_definitions = documents
        .iter()
        .flat_map(|document| document.occurrences.iter())
        .filter(|occurrence| occurrence.definition)
        .filter(|occurrence| symbol_matches_name(&occurrence.symbol, name))
        .map(|occurrence| occurrence.symbol.clone())
        .collect::<Vec<_>>();
    select_symbol(documents, project_definitions)
}

pub(crate) fn unique_symbols(mut symbols: Vec<String>) -> Vec<String> {
    symbols.sort();
    symbols.dedup();
    symbols
}

#[cfg(test)]
pub(crate) fn resolve_symbol_at(
    documents: &[DocumentOutput],
    path: &str,
    name: &str,
    source_line: usize,
) -> Option<String> {
    let definition_targets = documents
        .iter()
        .filter(|document| document.path == path)
        .flat_map(|document| document.occurrences.iter())
        .filter(|occurrence| {
            occurrence.definition
                && occurrence.range.first().copied() == Some(source_line as i32)
                && symbol_matches_name(&occurrence.symbol, name)
        })
        .map(|occurrence| occurrence.symbol.clone())
        .collect::<Vec<_>>();
    if let Some(symbol) = select_symbol(documents, definition_targets) {
        return Some(symbol);
    }

    let provider_targets = documents
        .iter()
        .filter(|document| document.path == path)
        .flat_map(|document| document.occurrences.iter())
        .filter(|occurrence| {
            !occurrence.definition
                && occurrence.range.first().copied() == Some(source_line as i32)
                && symbol_matches_name(&occurrence.symbol, name)
        })
        .map(|occurrence| occurrence.symbol.clone())
        .collect::<Vec<_>>();
    if let Some(symbol) = select_symbol(documents, provider_targets) {
        return Some(symbol);
    }
    resolve_symbol(documents, path, name)
}

#[cfg(test)]
pub(crate) fn select_symbol(documents: &[DocumentOutput], symbols: Vec<String>) -> Option<String> {
    let unique = unique_symbols(symbols);
    if unique.len() == 1 {
        return unique.into_iter().next();
    }

    // ponytail: clangd may expose a C/C++ declaration and its implementation
    // as different symbols; prefer the implementation file, but keep an
    // ambiguous result unresolved when no evidence distinguishes candidates.
    let mut ranked = unique
        .iter()
        .map(|symbol| {
            let score = documents
                .iter()
                .flat_map(|document| document.occurrences.iter())
                .filter(|occurrence| occurrence.definition && occurrence.symbol == *symbol)
                .map(|_| implementation_file_score(symbol))
                .max()
                .unwrap_or(0);
            (score, symbol)
        })
        .collect::<Vec<_>>();
    ranked.sort_by_key(|(score, symbol)| (*score, *symbol));
    let (best_score, best_symbol) = ranked.pop()?;
    if best_score == 0 || ranked.last().map(|(score, _)| *score) == Some(best_score) {
        return None;
    }
    Some(best_symbol.clone())
}
