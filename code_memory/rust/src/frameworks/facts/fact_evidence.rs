pub(crate) fn output_evidence(pack: &FrameworkPack, output: &str, line: &str) -> Option<String> {
    let line = line.trim();
    if output == "MIDDLEWARE"
        && matches!(pack.language.as_str(), "javascript" | "typescript")
    {
        match pack.id.as_str() {
            // These four packs share a repository-level candidate pool, but
            // their middleware syntax is not interchangeable. In particular,
            // Nest decorators must never become Express/Fastify facts merely
            // because those adapters are dependencies of the same monorepo.
            "nestjs" => {
                return first_marker(
                    line,
                    &[
                        "@UseGuards(",
                        "@UseInterceptors(",
                        "@UsePipes(",
                        "@UseFilters(",
                        "useGlobalGuards(",
                        "useGlobalInterceptors(",
                        "useGlobalPipes(",
                        "useGlobalFilters(",
                        "consumer.apply(",
                    ],
                );
            }
            "express" | "koa" => return first_marker(line, &[".use("]),
            "fastify" => return first_marker(line, &["addHook("]),
            _ => {}
        }
    }
    if pack.id == "react" && output == "COMPONENT" {
        return react_component_evidence(line);
    }
    if pack.id == "angular" && output == "SERVICE" {
        // Angular components are not services. Keep the service fact tied to
        // an actual service-shaped declaration so decorator-only lines cannot
        // publish unresolved SERVICE facts.
        return service_name_marker(line);
    }
    if pack.id == "vue" && output == "RENDERS" {
        // defineComponent declares a component; it is not render evidence
        // unless the line contains a template/render body.
        return first_marker(line, &["template:", "render(", "return <"]);
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
    if pack.id == "grpc" && pack.language == "go" && output == "RPC_ENDPOINT" {
        return (line.contains("RegisterService(")
            || line.contains("Register") && line.contains("Server("))
        .then(|| {
            if line.contains("RegisterService(") {
                "RegisterService".to_string()
            } else {
                "Register".to_string()
            }
        });
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
            } else if line.contains("addHook(") && line.contains("=>") {
                // The final identifier on an inline arrow signature is a
                // parameter (commonly `done`), not the hook implementation.
                // Keep the hook fact and abstain from inventing a target.
                None
            } else {
                call_last_argument_handler(line, ".use(")
                    .or_else(|| call_last_argument_handler(line, ".addHook("))
                    .or_else(|| generic_argument_after(line, &["UseMiddleware<"]))
                    .or_else(|| {
                        constructed_type_after(
                            line,
                            &[
                                ".addMiddleware(",
                                "->addMiddleware(",
                                "@UseGuards(",
                                "@UseInterceptors(",
                                "@UsePipes(",
                                "@UseFilters(",
                                "useGlobalGuards(",
                                "useGlobalInterceptors(",
                                "useGlobalPipes(",
                                "useGlobalFilters(",
                                "consumer.apply(",
                            ],
                        )
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
                                "@UseFilters(",
                                "useGlobalGuards(",
                                "useGlobalInterceptors(",
                                "useGlobalPipes(",
                                "useGlobalFilters(",
                                "consumer.apply(",
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
