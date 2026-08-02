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
        let Some(method) =
            configured_route_method(&route_line).or_else(|| route_method(&route_code))
        else {
            continue;
        };
        let Some((route_paths, end)) = framework_route_paths(pack, &route_line, &route_code) else {
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
        let handler = handler_name
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
            let id = format!("route:{}:{}:{}:{}", pack.id, path, source_line, route_path);
            facts.push(FrameworkFact {
                id,
                kind: "HTTP_ROUTE".to_string(),
                framework: pack.id.clone(),
                symbol: handler.clone(),
                method: Some(method.to_string()),
                path: Some(route_path),
                source_file: path.to_string(),
                source_line,
                source_end_line: source_line + route_line.lines().count().saturating_sub(1),
                source_range: source_range_for_text(index, &route_line),
                evidence: vec!["http_route_syntax".to_string()],
                properties: BTreeMap::new(),
            });
        }
        pending_prefix = None;
    }
    if let Some((route_path, method, handler_name, source_line)) =
        file_system_route(pack, path, source)
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
    route_line: &str,
    route_code: &str,
) -> Option<(Vec<String>, usize)> {
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
    } else if pack.id == "django" {
        &["re_path", "path"]
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

