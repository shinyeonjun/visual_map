fn parse_module(module: String, is_package: bool, source: &str) -> ModuleSource {
    let statements = logical_statements(source);
    let mut routers = HashMap::new();
    let mut applications = HashSet::new();
    let mut imports = HashMap::new();
    let mut includes = Vec::new();

    for statement in &statements {
        if let Some(imported) = parse_from_import(&module, is_package, &statement.text) {
            imports.extend(imported);
            continue;
        }
        if let Some((symbol, prefix)) = parse_router_definition(&statement.text) {
            routers.insert(symbol, prefix);
            continue;
        }
        applications.extend(parse_fastapi_applications(&statement.text));
        if let Some(include) = parse_router_include(&statement.text) {
            includes.push(include);
        }
    }

    ModuleSource {
        module,
        statements,
        routers,
        applications,
        imports,
        includes,
    }
}

fn parse_from_import(
    current_module: &str,
    is_package: bool,
    statement: &str,
) -> Option<Vec<(String, RouterKey)>> {
    let statement = statement.trim();
    let rest = statement.strip_prefix("from ")?;
    let (module_ref, imports) = rest.split_once(" import ")?;
    let resolved_module = resolve_import_module(current_module, is_package, module_ref.trim())?;
    let imports = imports
        .trim()
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'))
        .unwrap_or(imports.trim());

    let mut parsed = Vec::new();
    for imported in split_top_level(imports, ',') {
        let imported = imported.trim();
        if imported.is_empty() || imported == "*" {
            continue;
        }
        let (symbol, alias) = imported
            .split_once(" as ")
            .map_or((imported, imported), |(symbol, alias)| {
                (symbol.trim(), alias.trim())
            });
        if is_identifier(symbol) && is_identifier(alias) {
            parsed.push((
                alias.to_string(),
                RouterKey {
                    module: resolved_module.clone(),
                    symbol: symbol.to_string(),
                },
            ));
        }
    }

    Some(parsed)
}

fn parse_fastapi_applications(statement: &str) -> Vec<String> {
    if let Some((left, right)) = split_assignment(statement) {
        let call = right.trim();
        if call
            .find('(')
            .and_then(|open| call[..open].trim().rsplit('.').next())
            == Some("FastAPI")
        {
            let symbol = left.split(':').next().and_then(|left| {
                let symbol = left.split_whitespace().last()?;
                is_identifier(symbol).then(|| symbol.to_string())
            });
            return symbol.into_iter().collect();
        }
    }

    let statement = statement.trim_start();
    let definition = statement
        .strip_prefix("async def ")
        .or_else(|| statement.strip_prefix("def "));
    let Some(definition) = definition else {
        return Vec::new();
    };
    let Some(open) = definition.find('(') else {
        return Vec::new();
    };
    let Some(close) = definition.rfind(')') else {
        return Vec::new();
    };
    if open >= close {
        return Vec::new();
    }
    split_top_level(&definition[open + 1..close], ',')
        .into_iter()
        .filter_map(|parameter| {
            let (name, annotation) = parameter.split_once(':')?;
            let name = name.trim();
            let annotation = annotation
                .split_once('=')
                .map_or(annotation, |(annotation, _)| annotation)
                .trim();
            (is_identifier(name) && annotation.rsplit('.').next() == Some("FastAPI"))
                .then(|| name.to_string())
        })
        .collect()
}

fn parse_router_definition(statement: &str) -> Option<(String, StaticPath)> {
    let (left, right) = split_assignment(statement)?;
    let call = right.trim();
    let open = call.find('(')?;
    let callee = call[..open].trim();
    if callee.rsplit('.').next()? != "APIRouter" {
        return None;
    }
    let symbol = left.split(':').next()?.split_whitespace().last()?;
    if !is_identifier(symbol) {
        return None;
    }
    Some((
        symbol.to_string(),
        static_keyword_path(call_args(call)?, "prefix"),
    ))
}

fn parse_router_include(statement: &str) -> Option<RouterInclude> {
    let marker = ".include_router";
    let marker_index = statement.find(marker)?;
    let parent = statement[..marker_index].trim();
    if !is_dotted_identifier(parent) {
        return None;
    }
    let call = statement[marker_index + marker.len()..].trim();
    let args = call_args(call)?;
    let child = first_positional_argument(args)?;
    if !is_dotted_identifier(child) {
        return None;
    }
    Some(RouterInclude {
        parent: parent.to_string(),
        child: child.to_string(),
        prefix: static_keyword_path(args, "prefix"),
    })
}

fn route_router_symbol(
    module: &ModuleSource,
    handler_line: u64,
    method: &str,
    local_path: &str,
) -> Option<(String, String)> {
    let definition = module
        .statements
        .iter()
        .position(|statement| {
            statement.start_line <= handler_line
                && handler_line <= statement.end_line
                && is_function_definition(&statement.text)
        })
        .or_else(|| {
            module.statements.iter().position(|statement| {
                statement.start_line >= handler_line
                    && statement.start_line <= handler_line.saturating_add(2)
                    && is_function_definition(&statement.text)
            })
        })?;

    let mut matches = BTreeSet::new();
    for statement in module.statements[..definition].iter().rev() {
        if !statement.text.trim_start().starts_with('@') {
            break;
        }
        if let Some(router) = parse_route_decorator(&statement.text, method, local_path) {
            matches.insert(router);
        }
    }
    (matches.len() == 1)
        .then(|| matches.into_iter().next())
        .flatten()
}

fn parse_route_decorator(
    statement: &str,
    method: &str,
    local_path: &str,
) -> Option<(String, String)> {
    let decorator = statement.trim().strip_prefix('@')?;
    let open = decorator.find('(')?;
    let callee = decorator[..open].trim();
    let (router, decorator_method) = callee.rsplit_once('.')?;
    if !HTTP_METHODS.contains(&decorator_method.to_ascii_lowercase().as_str())
        || !decorator_method.eq_ignore_ascii_case(method)
        || !is_dotted_identifier(router)
    {
        return None;
    }
    let path = first_positional_argument(call_args(&decorator[open..])?)?;
    let path = parse_static_string(path)?;
    route_paths_equivalent(&path, local_path).then(|| (router.to_string(), path))
}

fn route_paths_equivalent(source_path: &str, engine_path: &str) -> bool {
    source_path == engine_path
        || (source_path.is_empty() && engine_path == "/")
        || (source_path == "/" && engine_path.is_empty())
}

fn route_method(route: &CodeInventoryItem, handler: &CodeInventoryItem) -> Option<String> {
    detail_string(&handler.detail, &["routeMethod", "route_method"])
        .or_else(|| detail_string(&route.detail, &["routeMethod", "route_method", "method"]))
        .or_else(|| route_method_from_identity(&route.id))
        .or_else(|| {
            route
                .name
                .split_once(' ')
                .map(|(method, _)| method.to_string())
        })
        .filter(|method| {
            HTTP_METHODS
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(method))
        })
}

fn route_local_path(route: &CodeInventoryItem, handler: &CodeInventoryItem) -> String {
    detail_string(&handler.detail, &["routePath", "route_path"])
        .or_else(|| {
            detail_string(
                &route.detail,
                &["localRoutePath", "routePath", "route_path"],
            )
        })
        .unwrap_or_else(|| {
            route
                .name
                .split_once(' ')
                .map_or_else(|| route.name.clone(), |(_, path)| path.to_string())
        })
}

fn route_method_from_identity(identity: &str) -> Option<String> {
    let marker = "__route__";
    let tail = identity.split(marker).nth(1)?;
    let method = tail.split("__").next()?.trim();
    (!method.is_empty()).then(|| method.to_string())
}

fn detail_string(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn logical_statements(source: &str) -> Vec<LogicalStatement> {
    let mut statements = Vec::new();
    let mut buffer = String::new();
    let mut start_line = 0_u64;
    let mut depth = 0_i32;

    for (index, raw_line) in source.lines().enumerate() {
        let line_number = index as u64 + 1;
        let uncommented = strip_python_comment(raw_line);
        let line = uncommented.trim();
        if line.is_empty() && buffer.is_empty() {
            continue;
        }
        if buffer.is_empty() {
            start_line = line_number;
        } else if !line.is_empty() {
            buffer.push(' ');
        }
        buffer.push_str(line);
        depth += bracket_delta(line);

        if depth <= 0 && !line.ends_with('\\') && !buffer.trim().is_empty() {
            statements.push(LogicalStatement {
                start_line,
                end_line: line_number,
                text: buffer.trim().trim_end_matches('\\').trim().to_string(),
            });
            buffer.clear();
            depth = 0;
        }
    }

    if !buffer.trim().is_empty() {
        statements.push(LogicalStatement {
            start_line,
            end_line: source.lines().count() as u64,
            text: buffer.trim().to_string(),
        });
    }
    statements
}

fn strip_python_comment(line: &str) -> String {
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' && quote.is_some() {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if character == active {
                quote = None;
            }
            continue;
        }
        if character == '\'' || character == '"' {
            quote = Some(character);
        } else if character == '#' {
            return line[..index].to_string();
        }
    }
    line.to_string()
}

fn bracket_delta(value: &str) -> i32 {
    let mut quote = None;
    let mut escaped = false;
    let mut delta = 0;
    for character in value.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' && quote.is_some() {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if character == active {
                quote = None;
            }
            continue;
        }
        match character {
            '\'' | '"' => quote = Some(character),
            '(' | '[' | '{' => delta += 1,
            ')' | ']' | '}' => delta -= 1,
            _ => {}
        }
    }
    delta
}

fn split_assignment(statement: &str) -> Option<(&str, &str)> {
    let mut quote = None;
    let mut depth = 0_i32;
    let characters = statement.char_indices().collect::<Vec<_>>();
    for (offset, (index, character)) in characters.iter().enumerate() {
        if let Some(active) = quote {
            if *character == active && (offset == 0 || characters[offset - 1].1 != '\\') {
                quote = None;
            }
            continue;
        }
        match character {
            '\'' | '"' => quote = Some(*character),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '=' if depth == 0 => {
                let previous = offset
                    .checked_sub(1)
                    .and_then(|previous| characters.get(previous))
                    .map(|(_, value)| *value);
                let next = characters.get(offset + 1).map(|(_, value)| *value);
                if previous != Some('=') && next != Some('=') {
                    return Some((&statement[..*index], &statement[index + 1..]));
                }
            }
            _ => {}
        }
    }
    None
}

fn call_args(call: &str) -> Option<&str> {
    let start = call.find('(')?;
    let end = call.rfind(')')?;
    (start < end).then_some(&call[start + 1..end])
}

fn first_positional_argument(args: &str) -> Option<&str> {
    split_top_level(args, ',')
        .into_iter()
        .map(str::trim)
        .find(|argument| !argument.is_empty() && split_assignment(argument).is_none())
}

fn static_keyword_path(args: &str, key: &str) -> StaticPath {
    for argument in split_top_level(args, ',') {
        let Some((name, value)) = split_assignment(argument) else {
            continue;
        };
        if name.trim() == key {
            return parse_static_string(value.trim())
                .map(StaticPath::Known)
                .unwrap_or(StaticPath::Dynamic);
        }
    }
    StaticPath::Known(String::new())
}

fn parse_static_string(value: &str) -> Option<String> {
    let value = value.trim();
    if value.len() < 2 {
        return None;
    }
    let quote = value.chars().next()?;
    if !matches!(quote, '\'' | '"') || value.chars().last()? != quote {
        return None;
    }
    let inner = &value[quote.len_utf8()..value.len() - quote.len_utf8()];
    (!inner.contains('\\')).then(|| inner.to_string())
}

fn split_top_level(value: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut quote = None;
    let mut escaped = false;
    let mut depth = 0_i32;

    for (index, character) in value.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' && quote.is_some() {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if character == active {
                quote = None;
            }
            continue;
        }
        match character {
            '\'' | '"' => quote = Some(character),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ if character == separator && depth == 0 => {
                parts.push(&value[start..index]);
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&value[start..]);
    parts
}

fn resolve_import_module(
    current_module: &str,
    is_package: bool,
    module_ref: &str,
) -> Option<String> {
    let level = module_ref
        .chars()
        .take_while(|character| *character == '.')
        .count();
    if level == 0 {
        return is_dotted_identifier(module_ref).then(|| module_ref.to_string());
    }

    let tail = module_ref[level..].trim_matches('.');
    let mut base = current_module.split('.').collect::<Vec<_>>();
    if !is_package {
        base.pop();
    }
    for _ in 1..level {
        base.pop()?;
    }
    if !tail.is_empty() {
        if !is_dotted_identifier(tail) {
            return None;
        }
        base.extend(tail.split('.'));
    }
    (!base.is_empty()).then(|| base.join("."))
}

fn python_module(path: &str) -> Option<(String, bool)> {
    if !is_python_path(path) {
        return None;
    }
    let path = normalize_source_path(path);
    let without_extension = path.strip_suffix(".py")?;
    let mut parts = without_extension
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>();
    let is_package = parts.last() == Some(&"__init__");
    if is_package {
        parts.pop();
    }
    (!parts.is_empty()).then(|| (parts.join("."), is_package))
}

fn normalize_source_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_string()
}

fn normalize_url_prefix(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return String::new();
    }
    format!("/{}", trimmed.trim_matches('/'))
}

fn join_url_path(prefix: &str, path: &str) -> String {
    let prefix = normalize_url_prefix(prefix);
    let path = path.trim();
    if path.is_empty() {
        return prefix;
    }
    let trailing_slash = path.ends_with('/');
    let body = path.trim_matches('/');
    let mut joined = match (prefix.is_empty(), body.is_empty()) {
        (true, true) => "/".to_string(),
        (true, false) => format!("/{body}"),
        (false, true) => prefix,
        (false, false) => format!("{prefix}/{body}"),
    };
    if trailing_slash && joined != "/" && !joined.ends_with('/') {
        joined.push('/');
    }
    joined
}

fn is_python_path(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".py")
}

fn is_fastapi_source_candidate(path: &str, source: &str) -> bool {
    normalize_source_path(path).ends_with("__init__.py")
        || source.contains("APIRouter")
        || source.contains("FastAPI")
        || source.contains("include_router")
        || HTTP_METHODS
            .iter()
            .any(|method| source.contains(&format!(".{method}(")))
}

fn is_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn is_dotted_identifier(value: &str) -> bool {
    !value.is_empty() && value.split('.').all(is_identifier)
}

fn is_function_definition(statement: &str) -> bool {
    let statement = statement.trim_start();
    statement.starts_with("def ") || statement.starts_with("async def ")
}

