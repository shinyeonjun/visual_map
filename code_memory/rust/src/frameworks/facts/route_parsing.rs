pub(crate) fn route_method(line: &str) -> Option<&'static str> {
    if let Some(method) = configured_route_method(line) {
        return Some(method);
    }
    if [
        ".websocket(",
        "@app.websocket(",
        "@router.websocket(",
        ".add_api_websocket_route(",
    ]
    .iter()
    .any(|marker| line.contains(marker))
    {
        return Some("WEBSOCKET");
    }
    if line.contains("router.register(") {
        return Some("ANY");
    }
    if let Some(method) = cpp_macro_route_method(line) {
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
        || line.contains(".add_api_websocket_route(")
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

fn cpp_macro_route_method(line: &str) -> Option<&'static str> {
    let marker = if line.contains("ADD_METHOD_TO(") {
        "ADD_METHOD_TO("
    } else if line.contains("METHOD_ADD(") {
        "METHOD_ADD("
    } else {
        return None;
    };
    let open = line.find(marker)? + marker.len() - 1;
    let close = matching_parenthesis(line, open)?;
    let method = top_level_argument(&line[open + 1..close], 2)?.1.trim();
    match method.to_ascii_uppercase().as_str() {
        "GET" => Some("GET"),
        "POST" => Some("POST"),
        "PUT" => Some("PUT"),
        "PATCH" => Some("PATCH"),
        "DELETE" => Some("DELETE"),
        "HEAD" => Some("HEAD"),
        "OPTIONS" => Some("OPTIONS"),
        _ => None,
    }
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

pub(crate) fn drf_router_registration(line: &str) -> Option<(String, String, usize)> {
    let marker = "router.register(";
    let open = line.find(marker)? + marker.len() - 1;
    let close = matching_parenthesis(line, open)?;
    let arguments = &line[open + 1..close];
    let prefix = top_level_argument(arguments, 0)
        .and_then(|(_, value)| quoted_values(value).into_iter().next())?;
    let viewset = top_level_argument(arguments, 1)
        .and_then(|(_, value)| handler_name_from_expression(value))?;
    Some((prefix, viewset, close + 1))
}

pub(crate) fn api_view_methods(source: &str, handler: &str) -> Vec<String> {
    let lines = source.lines().collect::<Vec<_>>();
    for (index, line) in lines.iter().enumerate() {
        let Some(function) = line.trim_start().strip_prefix("def ") else {
            continue;
        };
        let name = function
            .split(['(', ':', ' '])
            .next()
            .unwrap_or_default();
        if name != handler {
            continue;
        }
        let decorators = lines[..index].iter().rev().take(6).collect::<Vec<_>>();
        for decorator in decorators {
            if !decorator.trim_start().starts_with("@api_view") {
                continue;
            }
            let methods = quoted_values(decorator)
                .into_iter()
                .filter_map(|value| normalize_python_method(&value))
                .collect::<Vec<_>>();
            if !methods.is_empty() {
                return methods;
            }
        }
    }
    Vec::new()
}

pub(crate) fn normalize_python_method(value: &str) -> Option<String> {
    match value.trim().to_ascii_uppercase().as_str() {
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS" => {
            Some(value.trim().to_ascii_uppercase())
        }
        _ => None,
    }
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

pub(crate) fn minimal_api_route_call(line: &str) -> Option<(String, Option<String>, usize)> {
    for method in ["MapGet(", "MapPost(", "MapPut(", "MapPatch(", "MapDelete("] {
        let Some(open) = line.find(method).map(|start| start + method.len() - 1) else {
            continue;
        };
        let close = matching_parenthesis(line, open)?;
        let arguments = &line[open + 1..close];
        let first = top_level_argument(arguments, 0)?.1.trim();
        let first_is_path = first.starts_with(['\"', '\'']);
        let second = top_level_argument(arguments, 1);
        if !first_is_path
            && second.is_some_and(|(_, value)| !value.trim().starts_with(['\"', '\'']))
        {
            return None;
        }
        let (path, handler) = if first_is_path {
            (
                first.trim_matches(['\"', '\'']).to_string(),
                second.and_then(|(_, value)| handler_name_from_expression(value)),
            )
        } else {
            (
                second
                    .filter(|(_, value)| value.trim().starts_with(['\"', '\'']))
                    .map(|(_, value)| value.trim().trim_matches(['\"', '\'']).to_string())
                    .unwrap_or_default(),
                handler_name_from_expression(first),
            )
        };
        return Some((path, handler, close + 1));
    }
    None
}

pub(crate) fn annotation_route_paths(line: &str, markers: &[&str]) -> Option<(Vec<String>, usize)> {
    let mut candidates = markers
        .iter()
        .flat_map(|marker| {
            line.match_indices(marker)
                .filter(|(start, _)| {
                    *start == 0
                        || line[..*start]
                            .chars()
                            .next_back()
                            .is_none_or(|value| !value.is_ascii_alphanumeric() && value != '_')
                })
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

pub(crate) fn quoted_values(line: &str) -> Vec<String> {
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

pub(crate) fn route_registration_handler(pack_id: &str, line: &str) -> Option<String> {
    let markers: &[&str] = match pack_id {
        "django" => &["path(", "re_path("],
        "drf" => &["path(", "re_path("],
        "starlette" => &["Route("],
        _ => return None,
    };
    for marker in markers {
        let open = line.find(marker)? + marker.len() - 1;
        let close = matching_parenthesis(line, open)?;
        let arguments = &line[open + 1..close];
        if let Some((_, candidate)) = top_level_argument(arguments, 1) {
            let handler = if matches!(pack_id, "django" | "drf") {
                django_handler_name(candidate)
            } else {
                handler_name_from_expression(candidate)
            };
            if let Some(handler) = handler {
                return Some(handler);
            }
        }
    }
    None
}

pub(crate) fn go_registration_receiver_type(source: &str, line_index: usize) -> Option<String> {
    let lines = source.lines().collect::<Vec<_>>();
    let line = lines.get(line_index)?;
    let method_markers = [
        ".GET(",
        ".POST(",
        ".PUT(",
        ".PATCH(",
        ".DELETE(",
        ".HEAD(",
        ".OPTIONS(",
    ];
    let (open, close) = method_markers.iter().find_map(|marker| {
        let open = line.find(marker)? + marker.len() - 1;
        Some((open, matching_parenthesis(line, open)?))
    })?;
    let (_, candidate) = last_top_level_argument(&line[open + 1..close])?;
    let dot = candidate.rfind('.')?;
    let receiver = candidate[..dot]
        .rsplit(|value: char| !value.is_ascii_alphanumeric() && value != '_')
        .find(|value| !value.is_empty())?;
    for declaration in lines[..=line_index].iter().rev() {
        let Some(open) = declaration.find("func (") else {
            continue;
        };
        let open = open + "func ".len();
        let Some(close) = matching_parenthesis(declaration, open) else {
            continue;
        };
        let receiver_declaration = declaration[open + 1..close].split_whitespace();
        let mut tokens = receiver_declaration;
        if tokens.next() != Some(receiver) {
            continue;
        }
        if let Some(receiver_type) = tokens.last().map(|value| value.trim_start_matches('*')) {
            if !receiver_type.is_empty() {
                return Some(receiver_type.to_string());
            }
        }
    }
    None
}

pub(crate) fn top_level_argument(input: &str, wanted: usize) -> Option<(usize, &str)> {
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    let mut argument = 0usize;
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
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                if argument == wanted {
                    let candidate = input[start..index].trim();
                    return (!candidate.is_empty()).then_some((start, candidate));
                }
                argument += 1;
                start = index + value.len_utf8();
            }
            _ => {}
        }
    }
    if argument == wanted {
        let candidate = input[start..].trim();
        return (!candidate.is_empty()).then_some((start, candidate));
    }
    None
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
