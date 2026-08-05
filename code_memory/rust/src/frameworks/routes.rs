fn extract_routes(
    pack: &FrameworkPack,
    path: &str,
    source: &str,
    documents: &[DocumentOutput],
    fastapi_context: Option<&FastApiRouteContext>,
    facts: &mut Vec<FrameworkFact>,
) {
    let symbol_index = build_framework_symbol_index(documents);
    extract_routes_with_index(pack, path, source, fastapi_context, &symbol_index, facts);
}

fn extract_routes_with_index(
    pack: &FrameworkPack,
    path: &str,
    source: &str,
    fastapi_context: Option<&FastApiRouteContext>,
    symbol_index: &FrameworkSymbolIndex,
    facts: &mut Vec<FrameworkFact>,
) {
    if pack.id == "nextjs" {
        extract_file_system_route(pack, path, source, symbol_index, facts);
        return;
    }
    let lines: Vec<&str> = source.lines().collect();
    // Keep string templates (for example Vue's `@click` markup) visible, but
    // remove comments so prose such as "mail service" cannot become a fact.
    let code = source_without_comments(source, &pack.language);
    let code_lines: Vec<&str> = code.lines().collect();
    let mut annotation_prefix: Option<String> = None;
    let mut annotation_type: Option<String> = None;
    let mut pending_prefix: Option<String> = None;
    let mut skip_until = 0usize;
    for (index, line) in lines.iter().enumerate() {
        if index < skip_until {
            continue;
        }
        let code_line = code_lines.get(index).copied().unwrap_or_default();
        if pack.id == "fastapi"
            && (code_line.contains("APIRouter(") || code_line.contains(".include_router("))
        {
            continue;
        }
        if declares_type(code_line) {
            annotation_prefix = pending_prefix.take();
            annotation_type = declared_type_name(code_line);
        }
        if has_route_prefix_syntax(code_line) {
            let Some(prefix) = route_prefix(line) else {
                continue;
            };
            let request_mapping_has_method = code_line.contains("@RequestMapping")
                && request_mapping_method(code_line).is_some();
            if !has_http_method_annotation(code_line) && !request_mapping_has_method {
                pending_prefix = Some(prefix);
                continue;
            }
        }
        if matches!(
            (pack.language.as_str(), pack.adapter.as_str()),
            ("javascript" | "typescript", "registration-routing")
        ) && code_line.contains(".route(")
        {
            let mut chain = (*line).to_string();
            for next_line in lines.iter().skip(index + 1).take(31) {
                if chain.contains(';') {
                    break;
                }
                chain.push('\n');
                chain.push_str(next_line);
            }
            let chain_calls = javascript_chained_route_calls(&chain);
            if !chain_calls.is_empty() {
                let consumed = chain.lines().count();
                skip_until = index + consumed;
                let Some((route_path, _)) = first_route_path(&chain) else {
                    continue;
                };
                for (method, handler_name, line_offset) in chain_calls {
                    let source_line = index + line_offset + 1;
                    let handler = handler_name
                        .as_deref()
                        .and_then(|name| {
                            resolve_symbol_in_file_indexed(
                                symbol_index,
                                path,
                                name,
                                source_line.saturating_sub(1),
                            )
                        })
                        .and_then(|symbol| {
                            project_definition_for_symbol_indexed(symbol_index, &symbol)
                        });
                    let route_path = combine_route_prefix(None, &route_path);
                    facts.push(FrameworkFact {
                        id: format!(
                            "route:{}:{}:{}:{}:{}",
                            pack.id, path, source_line, method, route_path
                        ),
                        kind: "HTTP_ROUTE".to_string(),
                        framework: pack.id.clone(),
                        symbol: handler,
                        method: Some(method),
                        path: Some(route_path),
                        source_file: path.to_string(),
                        source_line,
                        source_end_line: source_line,
                        source_range: source_line_range(source, source_line),
                        evidence: vec!["http_route_chain_syntax".to_string()],
                        properties: BTreeMap::new(),
                    });
                }
                pending_prefix = None;
                continue;
            }
        }
        let mut route_line = (*line).to_string();
        let mut route_code = code_line.to_string();
        if joins_following_route_annotations(pack, code_line) {
            for (next, next_line) in lines
                .iter()
                .enumerate()
                .take((index + 8).min(lines.len()))
                .skip(index + 1)
            {
                let next_code = code_lines.get(next).copied().unwrap_or_default();
                let trimmed = next_code.trim_start();
                if !(trimmed.starts_with('@') || trimmed.starts_with('[')) {
                    break;
                }
                route_line.push('\n');
                route_line.push_str(next_line);
                route_code.push('\n');
                route_code.push_str(next_code);
            }
        }
        while route_code.contains('(')
            && route_code.matches('(').count() > route_code.matches(')').count()
            && route_line.lines().count() < 64
        {
            let next = index + route_line.lines().count();
            let Some(next_line) = lines.get(next) else {
                break;
            };
            let next_code_line = code_lines.get(next).copied().unwrap_or_default();
            route_line.push('\n');
            route_line.push_str(next_line);
            route_code.push('\n');
            route_code.push_str(next_code_line);
        }
        skip_until = index + route_line.lines().count();
        if pack.id == "drf" && drf_router_registration(&route_line).is_some() {
            extract_drf_router_routes(
                pack,
                path,
                source,
                index,
                &route_line,
                symbol_index,
                pending_prefix.as_deref().or(annotation_prefix.as_deref()),
                facts,
            );
            pending_prefix = None;
            continue;
        }
        if pack.id == "aspnet-mvc" && route_code.contains("MapControllerRoute(") {
            extract_aspnet_controller_route(
                pack,
                path,
                index,
                &route_line,
                symbol_index,
                facts,
            );
            pending_prefix = None;
            continue;
        }
        let Some(method) =
            configured_route_method(&route_line).or_else(|| route_method(&route_code))
        else {
            continue;
        };
        let Some((route_paths, end)) =
            framework_route_paths(pack, source, &route_line, &route_code)
        else {
            continue;
        };
        let functional_lambda = if pack.language != "java" {
            false
        } else if route_code.contains("->") || route_code.contains("=>") {
            true
        } else {
            let mut found = false;
            for line in code_lines.iter().skip(index).take(8) {
                if line.contains("->") || line.contains("=>") {
                    found = true;
                    break;
                }
                let trimmed = line.trim_start();
                if trimmed.contains("public ")
                    || trimmed.contains("protected ")
                    || trimmed.contains("private ")
                {
                    break;
                }
            }
            found
        };
        let handler_name = if functional_lambda {
            (pack.language == "java")
                .then(|| enclosing_java_method(&lines, index))
                .flatten()
        } else if pack.id == "nestjs" {
            nestjs_handler_name(&lines, index, &route_line)
        } else if pack.id == "fastapi" {
            fastapi_handler_name(&lines, index, &route_line[end..])
        } else if pack.id == "minimal-api" {
            minimal_api_route_call(&route_line).and_then(|(_, handler, _)| handler)
        } else {
            config_route_handler(&route_line)
                .or_else(|| macro_registration_handler(&route_line))
                .or_else(|| route_registration_handler(&pack.id, &route_line))
                .or_else(|| annotation_handler_name(&route_line))
                .or_else(|| {
                    let rest = &route_line[end..];
                    (!rest.contains("def ")
                        && !rest.contains("function ")
                        && !rest.contains("fn ")
                        && !rest.contains("func "))
                    .then(|| registration_handler(rest))
                    .flatten()
                })
                .or_else(|| nearby_handler(&lines, index))
        };
        let base_handler = handler_name
            .as_deref()
            .and_then(|name| {
                if matches!(
                    (pack.language.as_str(), pack.adapter.as_str()),
                    ("javascript" | "typescript", "registration-routing")
                ) {
                    if pack.id == "nestjs" {
                        resolve_symbol_indexed(symbol_index, path, name).or_else(|| {
                            resolve_symbol_in_file_indexed(symbol_index, path, name, index)
                        })
                    } else {
                        resolve_symbol_in_file_indexed(symbol_index, path, name, index)
                    }
                } else if pack.id == "minimal-api" {
                    resolve_symbol_indexed(symbol_index, path, name)
                        .or_else(|| resolve_symbol_at_indexed(symbol_index, path, name, index))
                } else if pack.language == "go" {
                    go_registration_receiver_type(source, index)
                        .and_then(|receiver_type| {
                            resolve_go_method_indexed(symbol_index, name, &receiver_type)
                        })
                        .or_else(|| resolve_symbol_at_indexed(symbol_index, path, name, index))
                } else {
                    resolve_symbol_at_indexed(symbol_index, path, name, index)
                        .or_else(|| resolve_symbol_indexed(symbol_index, path, name))
                }
            })
            .and_then(|symbol| project_definition_for_symbol_indexed(symbol_index, &symbol));
        let route_methods = if matches!(pack.id.as_str(), "django" | "drf") {
            let methods = python_http_methods(
                &route_line,
                source,
                handler_name.as_deref(),
                base_handler.as_deref(),
                symbol_index,
            );
            if methods.is_empty() {
                vec![method.to_string()]
            } else {
                methods
            }
        } else {
            vec![method.to_string()]
        };
        let multiple_methods = route_methods.len() > 1;
        let source_line = index + 1;
        for route_path in route_paths {
            let route_path = if pack.id == "fastapi" {
                combine_route_prefix(
                    fastapi_context.and_then(|context| context.prefix_for(path, line)),
                    &route_path,
                )
            } else if pack.id == "minimal-api" {
                combine_route_prefix(
                    fastapi_context.and_then(|context| context.minimal_prefix_for(path)),
                    &route_path,
                )
            } else {
                combine_route_prefix(
                    pending_prefix.as_deref().or(annotation_prefix.as_deref()),
                    &route_path,
                )
            };
            let route_path = if pack.language == "csharp" {
                csharp_route_tokens(
                    &route_path,
                    annotation_type.as_deref(),
                    handler_name.as_deref(),
                )
            } else {
                route_path
            };
            for route_method in &route_methods {
                let handler = if matches!(pack.id.as_str(), "django" | "drf")
                    && route_method != "ANY"
                {
                    base_handler
                        .as_deref()
                        .and_then(|owner| {
                            resolve_nested_definition_indexed(
                                symbol_index,
                                owner,
                                &route_method.to_ascii_lowercase(),
                            )
                        })
                        .or_else(|| base_handler.clone())
                } else {
                    base_handler.clone()
                };
                let id = if multiple_methods {
                    format!(
                        "route:{}:{}:{}:{}:{}",
                        pack.id, path, source_line, route_method, route_path
                    )
                } else {
                    format!("route:{}:{}:{}:{}", pack.id, path, source_line, route_path)
                };
                facts.push(FrameworkFact {
                    id,
                    kind: "HTTP_ROUTE".to_string(),
                    framework: pack.id.clone(),
                    symbol: handler,
                        method: Some(route_method.to_string()),
                    path: Some(route_path.clone()),
                    source_file: path.to_string(),
                    source_line,
                    source_end_line: source_line + route_line.lines().count().saturating_sub(1),
                    source_range: source_range_for_text(index, &route_line),
                    evidence: vec!["http_route_syntax".to_string()],
                    properties: BTreeMap::new(),
                });
            }
        }
        pending_prefix = None;
    }
    extract_file_system_route(pack, path, source, symbol_index, facts);
}

fn python_http_methods(
    route_line: &str,
    source: &str,
    handler_name: Option<&str>,
    base_handler: Option<&str>,
    symbol_index: &FrameworkSymbolIndex,
) -> Vec<String> {
    if let Some(name) = handler_name {
        let methods = api_view_methods(source, name);
        if !methods.is_empty() {
            return methods;
        }
    }
    let as_view_methods = quoted_values(
        route_line
            .split_once("as_view")
            .map(|(_, value)| value)
            .unwrap_or_default(),
    )
    .chunks(2)
    .filter_map(|pair| pair.first().and_then(|method| normalize_python_method(method)))
    .collect::<Vec<_>>();
    if !as_view_methods.is_empty() {
        return dedupe_strings(as_view_methods);
    }

    let mut methods = Vec::new();
    if let Some(owner) = base_handler {
        for symbol in nested_definition_symbols_indexed(symbol_index, owner) {
            let name = symbol_short_name(&symbol).to_ascii_lowercase();
            if normalize_python_method(&name).is_some() {
                methods.push(name.to_ascii_uppercase());
            }
        }
    }
    if let Some(name) = handler_name {
        let class_line = source.lines().find(|line| {
            line.trim_start()
                .starts_with(&format!("class {name}"))
        });
        if let Some(class_line) = class_line {
            methods.extend(inherited_python_methods(class_line));
        }
    }
    dedupe_strings(methods)
}

fn inherited_python_methods(class_line: &str) -> Vec<String> {
    let mut methods = Vec::new();
    let lower = class_line.to_ascii_lowercase();
    if lower.contains("listcreateapiview") || lower.contains("listmixin") {
        methods.push("GET".to_string());
    }
    if lower.contains("listcreateapiview") || lower.contains("createmixin") {
        methods.push("POST".to_string());
    }
    if lower.contains("retrieveupdatedestroyapiview") {
        methods.extend(["GET", "PUT", "PATCH", "DELETE"].map(str::to_string));
    }
    if lower.contains("retrieveapiview") || lower.contains("retrievemixin") {
        methods.push("GET".to_string());
    }
    if lower.contains("updateapiview") || lower.contains("updatemixin") {
        methods.extend(["PUT", "PATCH"].map(str::to_string));
    }
    if lower.contains("destroyapiview") || lower.contains("destroymixin") {
        methods.push("DELETE".to_string());
    }
    dedupe_strings(methods)
}

fn dedupe_strings(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup();
    values
}

#[allow(clippy::too_many_arguments)]
fn extract_drf_router_routes(
    pack: &FrameworkPack,
    path: &str,
    source: &str,
    line_index: usize,
    route_line: &str,
    symbol_index: &FrameworkSymbolIndex,
    prefix: Option<&str>,
    facts: &mut Vec<FrameworkFact>,
) {
    let Some((registered_prefix, viewset_name, _)) = drf_router_registration(route_line) else {
        return;
    };
    let base_handler = resolve_symbol_indexed(symbol_index, path, &viewset_name)
        .or_else(|| resolve_symbol_at_indexed(symbol_index, path, &viewset_name, line_index))
        .and_then(|symbol| project_definition_for_symbol_indexed(symbol_index, &symbol));
    let action_names = base_handler
        .as_deref()
        .map(|owner| {
            nested_definition_symbols_indexed(symbol_index, owner)
                .into_iter()
                .map(|symbol| symbol_short_name(&symbol).to_ascii_lowercase())
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default();
    let inherited = source
        .lines()
        .find(|line| line.contains(&format!("class {viewset_name}")))
        .map(inherited_viewset_actions)
        .unwrap_or_default();
    let standard = [
        ("list", "GET", false),
        ("create", "POST", false),
        ("retrieve", "GET", true),
        ("update", "PUT", true),
        ("partial_update", "PATCH", true),
        ("destroy", "DELETE", true),
    ];
    let route_prefix = combine_route_prefix(prefix, &registered_prefix);
    let source_line = line_index + 1;
    for (action, method, detail) in standard {
        if !action_names.contains(action) && !inherited.contains(action) {
            continue;
        }
        let handler = base_handler.as_deref().and_then(|owner| {
            resolve_nested_definition_indexed(symbol_index, owner, action)
                .or_else(|| Some(owner.to_string()))
        });
        push_drf_route_fact(
            pack,
            path,
            source,
            source_line,
            route_line,
            route_prefix.clone(),
            method,
            handler,
            &format!("drf-router-register:{action}"),
            detail,
            facts,
        );
    }
    for (methods, detail, action, url_path) in drf_custom_actions(source, &viewset_name) {
        for method in methods {
            let action_path = if detail {
                format!(
                    "{}/{{pk}}/{url_path}/",
                    route_prefix.trim_end_matches('/')
                )
                .replace("//", "/")
            } else {
                format!("{}/{url_path}/", route_prefix.trim_end_matches('/')).replace("//", "/")
            };
            let handler = base_handler.as_deref().and_then(|owner| {
                resolve_nested_definition_indexed(symbol_index, owner, &action)
                    .or_else(|| Some(owner.to_string()))
            });
            push_drf_route_fact(
                pack,
                path,
                source,
                source_line,
                route_line,
                action_path,
                &method,
                handler,
                "drf-router-register:action",
                false,
                facts,
            );
        }
    }
}

fn inherited_viewset_actions(line: &str) -> HashSet<String> {
    let lower = line.to_ascii_lowercase();
    let mut actions = HashSet::new();
    if lower.contains("modelviewset") {
        actions.extend(["list", "create", "retrieve", "update", "partial_update", "destroy"].map(str::to_string));
    } else if lower.contains("readonlymodelviewset") {
        actions.extend(["list", "retrieve"].map(str::to_string));
    }
    actions.extend(
        inherited_python_methods(line)
            .into_iter()
            .map(|method| method.to_ascii_lowercase()),
    );
    actions
}

fn drf_custom_actions(source: &str, owner: &str) -> Vec<(Vec<String>, bool, String, String)> {
    let lines = source.lines().collect::<Vec<_>>();
    let class_start = lines
        .iter()
        .position(|line| line.contains(&format!("class {owner}")))
        .unwrap_or(0);
    let mut actions = Vec::new();
    for index in class_start..lines.len() {
        let line = lines[index].trim();
        if !line.starts_with("@action") {
            continue;
        }
        let methods = quoted_values(line)
            .into_iter()
            .filter_map(|value| normalize_python_method(&value))
            .collect::<Vec<_>>();
        let methods = if methods.is_empty() {
            vec!["GET".to_string()]
        } else {
            methods
        };
        let detail = line.contains("detail=True");
        let url_path = quoted_values(line)
            .into_iter()
            .last()
            .filter(|value| !matches!(value.to_ascii_uppercase().as_str(), "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS"))
            .unwrap_or_else(|| {
                lines
                    .iter()
                    .skip(index + 1)
                    .find_map(|candidate| candidate.trim_start().strip_prefix("def "))
                    .and_then(|value| value.split('(').next())
                    .unwrap_or("action")
                    .to_string()
            });
        let action = lines
            .iter()
            .skip(index + 1)
            .find_map(|candidate| candidate.trim_start().strip_prefix("def "))
            .and_then(|value| value.split('(').next())
            .unwrap_or("action")
            .to_string();
        actions.push((methods, detail, action, url_path));
    }
    actions
}

#[allow(clippy::too_many_arguments)]
fn push_drf_route_fact(
    pack: &FrameworkPack,
    path: &str,
    _source: &str,
    source_line: usize,
    route_line: &str,
    route_path: String,
    method: &str,
    handler: Option<String>,
    evidence: &str,
    _detail: bool,
    facts: &mut Vec<FrameworkFact>,
) {
    let route_path = if _detail {
        format!("{}/{{pk}}/", route_path.trim_end_matches('/'))
    } else if route_path.is_empty() {
        "/".to_string()
    } else if route_path.starts_with('/') {
        if route_path.ends_with('/') {
            route_path
        } else {
            format!("{route_path}/")
        }
    } else {
        format!("/{route_path}/")
    };
    let id = format!("route:{}:{}:{}:{}:{}", pack.id, path, source_line, method, route_path);
    if facts.iter().any(|fact| fact.id == id) {
        return;
    }
    facts.push(FrameworkFact {
        id,
        kind: "HTTP_ROUTE".to_string(),
        framework: pack.id.clone(),
        symbol: handler,
        method: Some(method.to_string()),
        path: Some(route_path),
        source_file: path.to_string(),
        source_line,
        source_end_line: source_line + route_line.lines().count().saturating_sub(1),
        source_range: source_range_for_text(source_line - 1, route_line),
        evidence: vec![evidence.to_string(), "router_registration_expansion".to_string()],
        properties: BTreeMap::new(),
    });
}

fn extract_file_system_route(
    pack: &FrameworkPack,
    path: &str,
    source: &str,
    symbol_index: &FrameworkSymbolIndex,
    facts: &mut Vec<FrameworkFact>,
) {
    if let Some((route_path, method, handler_name, source_line)) = file_system_route(pack, path, source)
    {
        let handler = handler_name
            .and_then(|name| {
                resolve_symbol_at_indexed(symbol_index, path, &name, source_line.saturating_sub(1))
            })
            .and_then(|symbol| project_definition_for_symbol_indexed(symbol_index, &symbol));
        let id = format!("route:{}:{}:{}:{}", pack.id, path, source_line, route_path);
        if !facts.iter().any(|fact| fact.id == id) {
            facts.push(FrameworkFact {
                id,
                kind: "HTTP_ROUTE".to_string(),
                framework: pack.id.clone(),
                symbol: handler,
                method: Some(method),
                path: Some(route_path),
                source_file: path.to_string(),
                source_line,
                source_end_line: source_line,
                source_range: source_line_range(source, source_line),
                evidence: vec!["filesystem_route_convention".to_string()],
                properties: BTreeMap::new(),
            });
        }
    }
}

fn extract_aspnet_controller_route(
    pack: &FrameworkPack,
    path: &str,
    line_index: usize,
    route_line: &str,
    symbol_index: &FrameworkSymbolIndex,
    facts: &mut Vec<FrameworkFact>,
) {
    let Some((route_path, controller, action, end)) = aspnet_controller_route(route_line) else {
        return;
    };
    let route_path = combine_route_prefix(None, &route_path);
    let controller = format!("{}Controller", controller.trim_end_matches("Controller"));
    let handler =
        resolve_method_in_type_near_path_indexed(symbol_index, path, &controller, &action);
    let source_line = line_index + 1;
    facts.push(FrameworkFact {
        id: format!(
            "route:{}:{}:{}:ANY:{}",
            pack.id, path, source_line, route_path
        ),
        kind: "HTTP_ROUTE".to_string(),
        framework: pack.id.clone(),
        symbol: handler,
        method: Some("ANY".to_string()),
        path: Some(route_path),
        source_file: path.to_string(),
        source_line,
        source_end_line: source_line + route_line.lines().count().saturating_sub(1),
        source_range: source_range_for_text(line_index, &route_line[..end]),
        evidence: vec!["aspnet_controller_route_registration".to_string()],
        properties: BTreeMap::new(),
    });
}

fn aspnet_controller_route(route_line: &str) -> Option<(String, String, String, usize)> {
    let marker = "MapControllerRoute(";
    let open = route_line.find(marker)? + marker.len() - 1;
    let close = matching_parenthesis(route_line, open)?;
    let arguments = &route_line[open + 1..close];
    let pattern = named_top_level_argument(arguments, "pattern")
        .or_else(|| top_level_argument(arguments, 1).map(|(_, value)| value))
        .and_then(csharp_static_string)?;
    let defaults = named_top_level_argument(arguments, "defaults")
        .or_else(|| top_level_argument(arguments, 2).map(|(_, value)| value))?;
    let controller = csharp_named_string_assignment(defaults, "controller")?;
    let action = csharp_named_string_assignment(defaults, "action")?;
    Some((pattern, controller, action, close + 1))
}

fn named_top_level_argument<'a>(arguments: &'a str, name: &str) -> Option<&'a str> {
    (0..16).find_map(|index| {
        let (_, argument) = top_level_argument(arguments, index)?;
        let (candidate, value) = argument.split_once(':')?;
        (candidate.trim() == name).then(|| value.trim())
    })
}

fn csharp_named_string_assignment(input: &str, name: &str) -> Option<String> {
    input.match_indices(name).find_map(|(start, _)| {
        let before = input[..start].chars().next_back();
        let after = input[start + name.len()..].chars().next();
        if before.is_some_and(|value| value.is_ascii_alphanumeric() || value == '_')
            || after.is_some_and(|value| value.is_ascii_alphanumeric() || value == '_')
        {
            return None;
        }
        let value = input[start + name.len()..].trim_start().strip_prefix('=')?;
        csharp_static_string(value.trim_start())
    })
}

fn csharp_static_string(value: &str) -> Option<String> {
    let value = value.trim();
    let quote = value.find('"')?;
    let prefix = &value[..quote];
    if !prefix.chars().all(|character| matches!(character, '$' | '@')) {
        return None;
    }
    let end = value[quote + 1..].find('"')? + quote + 1;
    let body = &value[quote + 1..end];
    if !prefix.contains('$') {
        return Some(body.to_string());
    }

    let mut literal = String::with_capacity(body.len());
    let mut characters = body.chars().peekable();
    while let Some(character) = characters.next() {
        if matches!(character, '{' | '}') {
            if characters.peek() != Some(&character) {
                return None;
            }
            characters.next();
        }
        literal.push(character);
    }
    Some(literal)
}

fn java_mapping_annotation(source: &str) -> bool {
    [
        "@RequestMapping",
        "@GetMapping",
        "@PostMapping",
        "@PutMapping",
        "@PatchMapping",
        "@DeleteMapping",
    ]
    .iter()
    .any(|annotation| source.contains(annotation))
}

fn joins_following_route_annotations(pack: &FrameworkPack, line: &str) -> bool {
    (pack.language == "java"
        && [
            "@GET", "@POST", "@PUT", "@PATCH", "@DELETE", "@HEAD", "@OPTIONS",
        ]
        .iter()
        .any(|marker| line.trim_start().starts_with(marker)))
        || (pack.language == "csharp" && line.trim_start().starts_with("[Http"))
}

fn framework_route_paths(
    pack: &FrameworkPack,
    source: &str,
    route_line: &str,
    route_code: &str,
) -> Option<(Vec<String>, usize)> {
    if !pack_owns_route_registration(pack, source, route_code) {
        return None;
    }
    if pack.language == "python" {
        match pack.id.as_str() {
            "django" => return annotation_route_paths(route_line, &["re_path", "path"]),
            "drf" if drf_router_registration(route_line).is_some() => {
                let (prefix, _, end) = drf_router_registration(route_line)?;
                return Some((vec![prefix], end));
            }
            "drf" => return annotation_route_paths(route_line, &["re_path", "path"]),
            "starlette" => {
                return annotation_route_paths(route_line, &["WebSocketRoute", "Route"])
            }
            "fastapi" if !python_route_registration(route_code, "add_api_route") => return None,
            "flask" if !python_route_registration(route_code, "add_url_rule") => return None,
            "sanic" if !python_route_registration(route_code, "add_route") => return None,
            _ => {}
        }
    }
    if pack.language == "java" && java_mapping_annotation(route_code) {
        let paths = java_route_paths(route_line);
        let paths = if paths.is_empty() {
            vec![String::new()]
        } else {
            paths
        };
        let end = first_route_path(route_line)
            .map(|(_, end)| end)
            .or_else(|| route_line.find(')').map(|end| end + 1))
            .unwrap_or(route_line.len());
        return Some((paths, end));
    }

    if pack.language == "csharp" {
        if pack.id == "minimal-api" {
            if let Some((path, _, end)) = minimal_api_route_call(route_line) {
                return Some((vec![path], end));
            }
        }
        let method_paths = annotation_route_paths(
            route_line,
            &[
                "[HttpGet",
                "[HttpPost",
                "[HttpPut",
                "[HttpPatch",
                "[HttpDelete",
                "[HttpHead",
                "[HttpOptions",
            ],
        );
        if let Some((paths, end)) = method_paths {
            if paths.iter().any(|path| !path.is_empty()) {
                return Some((paths, end));
            }
            if let Some(route) = annotation_route_paths(route_line, &["[Route"]) {
                return Some(route);
            }
            return Some((paths, end));
        }
    }

    let annotation_markers: &[&str] = if pack.id == "nestjs" {
        &[
            "@Get", "@Post", "@Put", "@Patch", "@Delete", "@Head", "@Options",
        ]
    } else if pack.language == "java" && matches!(pack.id.as_str(), "jakarta-ee" | "quarkus") {
        if let Some(result) = annotation_route_paths(route_line, &["@Path"]) {
            return Some(result);
        }
        return Some((vec![String::new()], route_line.len()));
    } else if pack.language == "java" && pack.id == "micronaut" {
        &[
            "@Get", "@Post", "@Put", "@Patch", "@Delete", "@Head", "@Options",
        ]
    } else {
        &[]
    };
    if !annotation_markers.is_empty() {
        if let Some(paths) = annotation_route_paths(route_line, annotation_markers) {
            return Some(paths);
        }
    }

    if matches!(
        (pack.language.as_str(), pack.adapter.as_str()),
        ("javascript" | "typescript", "registration-routing")
    ) {
        if let Some((path, end)) = javascript_registration_route_path(route_line) {
            return Some((vec![path], end));
        }
        if javascript_registration_marker(route_line) {
            return None;
        }
    }

    let (route_path, end) = first_route_path(route_line)?;
    let paths = route_paths(route_line);
    Some((
        if paths.is_empty() {
            vec![route_path]
        } else {
            paths
        },
        end,
    ))
}

fn pack_owns_route_registration(pack: &FrameworkPack, source: &str, line: &str) -> bool {
    let trimmed = line.trim_start();
    match (pack.language.as_str(), pack.id.as_str()) {
        ("python", "django") => contains_call(line, &["path", "re_path"]),
        ("python", "drf") => {
            drf_router_registration(line).is_some() || contains_call(line, &["path", "re_path"])
        }
        ("python", "starlette") => contains_call(line, &["Route", "WebSocketRoute"]),
        ("python", "fastapi") => python_route_registration(line, "add_api_route"),
        ("python", "flask") => python_route_registration(line, "add_url_rule"),
        ("python", "sanic") => python_route_registration(line, "add_route"),

        ("java", "spring-mvc") => java_mapping_annotation(line),
        ("java", "spring-webflux") => {
            java_mapping_annotation(line)
                || line.contains("RouterFunctions")
                || line.contains("RequestPredicates.")
                || line.contains(".andRoute(")
        }
        ("java", "jakarta-ee" | "quarkus") => {
            line.contains("@Path")
                || ["@GET", "@POST", "@PUT", "@PATCH", "@DELETE", "@HEAD", "@OPTIONS"]
                    .iter()
                    .any(|marker| line.contains(marker))
        }
        ("java", "micronaut") => [
            "@Get", "@Post", "@Put", "@Patch", "@Delete", "@Head", "@Options",
        ]
        .iter()
        .any(|marker| line.contains(marker)),
        ("java", "play") => starts_with_http_verb(trimmed),

        ("csharp", "minimal-api") => minimal_api_route_call(line).is_some(),
        ("csharp", "aspnet-mvc" | "aspnet-web-api") => [
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
            || pack.id == "aspnet-mvc" && line.contains("MapControllerRoute("),

        ("javascript" | "typescript", "nestjs") => [
            "@Get", "@Post", "@Put", "@Patch", "@Delete", "@Head", "@Options",
        ]
        .iter()
        .any(|marker| line.contains(marker)),
        ("javascript" | "typescript", "angular") => line.contains("Route("),
        ("javascript" | "typescript", "express" | "fastify" | "koa") => {
            javascript_server_route_receiver(pack.id.as_str(), source, line)
        }
        ("javascript" | "typescript", "nextjs" | "nuxt" | "sveltekit") => false,

        ("cpp", "crow") => line.contains("CROW_ROUTE("),
        ("cpp", "drogon") => {
            line.contains("ADD_METHOD_TO(") || line.contains("METHOD_ADD(")
        }
        ("cpp", "poco") => line.contains("Route("),

        ("go", "beego") => line.contains("beego.Router("),
        ("go", "echo" | "gin") => registration_with_handler(
            line,
            &[".GET(", ".POST(", ".PUT(", ".PATCH(", ".DELETE(", ".HEAD(", ".OPTIONS("],
        ),
        ("go", "chi" | "fiber") => registration_with_handler(
            line,
            &[".Get(", ".Post(", ".Put(", ".Patch(", ".Delete(", ".Head(", ".Options("],
        ),
        ("go", "net-http") => {
            line.contains("http.HandleFunc(") || line.contains("http.Handle(")
        }

        ("rust", "rocket") => [
            "#[get(", "#[post(", "#[put(", "#[patch(", "#[delete(", "#[head(",
            "#[options(", "#[route(",
        ]
        .iter()
        .any(|marker| line.contains(marker)),
        ("rust", "actix-web") => {
            line.contains(".route(")
                || ["#[get(", "#[post(", "#[put(", "#[patch(", "#[delete("]
                    .iter()
                    .any(|marker| line.contains(marker))
        }
        ("rust", "axum") => line.contains(".route(") || line.contains("Router::new()"),
        ("rust", "poem") => line.contains(".at(") || line.contains("Route::new"),
        ("rust", "warp") => line.contains("warp::path"),

        ("php", "api-platform") => line.contains("#[Get(") || line.contains("#[ApiResource"),
        ("php", "symfony") => line.contains("#[Route"),
        ("php", "laravel") => line.contains("Route::"),
        ("php", "cakephp") => line.contains("Router::"),
        ("php", "codeigniter") => line.contains("$routes->"),
        ("php", "laminas") => line.contains("->addRoute("),
        ("php", "slim") => line.contains("$app->") || line.contains("$group->"),

        ("ruby", "rack") => trimmed.starts_with("map "),
        ("ruby", "roda") => trimmed.starts_with("r.on ") || trimmed.starts_with("r.get "),
        ("ruby", "grape" | "hanami" | "rails" | "sinatra") => {
            starts_with_http_verb(trimmed)
        }

        ("dart", "shelf") => registration_with_handler(
            line,
            &[".get(", ".post(", ".put(", ".patch(", ".delete(", ".head(", ".options("],
        ),
        ("dart", "dart-frog") => false,
        _ => false,
    }
}

fn contains_call(line: &str, names: &[&str]) -> bool {
    names.iter().any(|name| {
        line.match_indices(&format!("{name}("))
            .any(|(start, _)| {
                start == 0
                    || line[..start]
                        .chars()
                        .next_back()
                        .is_none_or(|value| !value.is_ascii_alphanumeric() && value != '_')
            })
    })
}

fn starts_with_http_verb(line: &str) -> bool {
    [
        "GET ", "POST ", "PUT ", "PATCH ", "DELETE ", "HEAD ", "OPTIONS ", "get ",
        "post ", "put ", "patch ", "delete ", "head ", "options ",
    ]
    .iter()
    .any(|method| line.starts_with(method))
}

fn registration_with_handler(line: &str, markers: &[&str]) -> bool {
    markers.iter().any(|marker| {
        let Some(open) = line.find(marker).map(|start| start + marker.len() - 1) else {
            return false;
        };
        let Some(close) = matching_parenthesis(line, open) else {
            return false;
        };
        top_level_argument(&line[open + 1..close], 1).is_some()
    })
}

fn javascript_server_route_receiver(pack: &str, source: &str, line: &str) -> bool {
    let markers = [
        ".get(", ".GET(", ".post(", ".POST(", ".put(", ".PUT(", ".patch(",
        ".PATCH(", ".delete(", ".DELETE(", ".head(", ".HEAD(", ".options(",
        ".OPTIONS(", ".route(",
    ];
    markers.iter().any(|marker| {
        let Some(start) = line.find(marker) else {
            return false;
        };
        let receiver = line[..start]
            .rsplit(|value: char| !value.is_ascii_alphanumeric() && value != '_')
            .find(|value| !value.is_empty())
            .unwrap_or_default();
        if receiver.is_empty() {
            return false;
        }
        let lower = receiver.to_ascii_lowercase();
        let conventional = matches!(lower.as_str(), "app" | "router" | "server" | "fastify")
            || lower.ends_with("router")
            || lower.ends_with("server");
        conventional || javascript_receiver_is_framework_instance(pack, source, receiver)
    })
}

fn javascript_receiver_is_framework_instance(pack: &str, source: &str, receiver: &str) -> bool {
    source.lines().any(|line| {
        let Some((left, right)) = line.split_once('=') else {
            return false;
        };
        let binding = left
            .rsplit(|value: char| !value.is_ascii_alphanumeric() && value != '_')
            .find(|value| !value.is_empty());
        if binding != Some(receiver) {
            return false;
        }
        match pack {
            "express" => {
                right.contains("express(")
                    || right.contains("express.Router(")
                    || right.contains("Router(")
            }
            "fastify" => right.contains("fastify("),
            "koa" => right.contains("new Router(") || right.contains("Router("),
            _ => false,
        }
    })
}

fn python_route_registration(line: &str, registration_method: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with('@')
        && [
            ".get(",
            ".post(",
            ".put(",
            ".patch(",
            ".delete(",
            ".head(",
            ".options(",
            ".route(",
            ".api_route(",
            ".websocket(",
        ]
        .iter()
        .any(|marker| trimmed.contains(marker))
    {
        return true;
    }
    line.contains(&format!(".{registration_method}("))
}

fn javascript_registration_route_path(line: &str) -> Option<(String, usize)> {
    let code = source_code_mask(line, "javascript");
    for marker in [
        "route(",
        ".get(",
        ".GET(",
        ".post(",
        ".POST(",
        ".put(",
        ".PUT(",
        ".patch(",
        ".PATCH(",
        ".delete(",
        ".DELETE(",
        ".head(",
        ".HEAD(",
        ".options(",
        ".OPTIONS(",
        "MapGet(",
        "MapPost(",
        "MapPut(",
        "MapPatch(",
        "MapDelete(",
        "Route(",
        "GET(",
        "POST(",
        "PUT(",
        "PATCH(",
        "DELETE(",
        "HEAD(",
        "OPTIONS(",
    ] {
        let mut search_start = 0usize;
        while let Some(offset) = code[search_start..].find(marker) {
            let open = search_start + offset + marker.len() - 1;
            let close = matching_parenthesis(line, open)?;
            let arguments = &line[open + 1..close];
            let first_argument = javascript_first_argument(arguments);
            if let Some((path, consumed)) = first_route_path(first_argument) {
                return Some((path, open + 1 + consumed));
            }
            search_start = close + 1;
        }
    }
    None
}

fn javascript_registration_marker(line: &str) -> bool {
    [
        "route(",
        ".get(",
        ".GET(",
        ".post(",
        ".POST(",
        ".put(",
        ".PUT(",
        ".patch(",
        ".PATCH(",
        ".delete(",
        ".DELETE(",
        ".head(",
        ".HEAD(",
        ".options(",
        ".OPTIONS(",
        "MapGet(",
        "MapPost(",
        "MapPut(",
        "MapPatch(",
        "MapDelete(",
    ]
    .iter()
    .any(|marker| line.contains(marker))
}

fn javascript_first_argument(arguments: &str) -> &str {
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for (index, value) in arguments.char_indices() {
        if let Some(current) = quote {
            if escaped {
                escaped = false;
            } else if value == '\\' {
                escaped = true;
            } else if value == current {
                quote = None;
            }
            continue;
        }
        match value {
            '\'' | '"' => quote = Some(value),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return &arguments[..index],
            _ => {}
        }
    }
    arguments
}

fn declared_type_name(line: &str) -> Option<String> {
    ["class ", "record ", "struct ", "interface "]
        .iter()
        .find_map(|keyword| identifier_after(line, keyword))
}

fn csharp_route_tokens(path: &str, type_name: Option<&str>, action: Option<&str>) -> String {
    let mut path = path.to_string();
    if let Some(controller) = type_name {
        let controller = controller.strip_suffix("Controller").unwrap_or(controller);
        path = path.replace("[controller]", controller);
    }
    if let Some(action) = action {
        path = path.replace("[action]", action);
    }
    path
}

fn fastapi_handler_name(lines: &[&str], index: usize, trailing: &str) -> Option<String> {
    identifier_after(trailing, "def ").or_else(|| {
        lines
            .iter()
            .skip(index + 1)
            .take(12)
            .find_map(|line| identifier_after(line.trim_start(), "def "))
    })
}

fn line_source_range(line_number: usize, line: &str) -> Vec<i32> {
    vec![
        line_number as i32,
        0,
        line_number as i32,
        line.chars().count() as i32,
    ]
}

fn source_range_for_text(start_line: usize, text: &str) -> Vec<i32> {
    let lines = text.lines().collect::<Vec<_>>();
    let end_line = start_line + lines.len().saturating_sub(1);
    vec![
        start_line as i32,
        0,
        end_line as i32,
        lines
            .last()
            .map(|line| line.chars().count())
            .unwrap_or_default() as i32,
    ]
}

fn source_line_range(source: &str, source_line: usize) -> Vec<i32> {
    source
        .lines()
        .nth(source_line.saturating_sub(1))
        .map(|line| line_source_range(source_line.saturating_sub(1), line))
        .unwrap_or_else(|| line_source_range(source_line.saturating_sub(1), ""))
}
