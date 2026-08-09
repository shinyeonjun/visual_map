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

fn django_handler_name(candidate: &str) -> Option<String> {
    let receiver = candidate.split(".as_view").next()?.trim();
    if receiver != candidate {
        return receiver
            .rsplit(|value: char| !value.is_ascii_alphanumeric() && value != '_')
            .find(|value| !value.is_empty())
            .map(str::to_string);
    }
    handler_name_from_expression(candidate)
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

pub(crate) fn nestjs_handler_name(
    lines: &[&str],
    index: usize,
    route_line: &str,
) -> Option<String> {
    // Nest decorators can be stacked above a method, or written inline as
    // `@Get() handler() {}`. Resolve the declaration immediately below the
    // decorator block; searching the whole file can select a service method
    // with the same short name instead.
    for line in route_line.lines() {
        let candidate = line.trim();
        if let Some(close) = candidate.find(')') {
            if let Some(name) = nestjs_method_name(&candidate[close + 1..]) {
                return Some(name);
            }
        }
    }
    for line in lines.iter().skip(index + 1).take(32) {
        let candidate = line.trim();
        if nestjs_route_annotation(candidate) {
            break;
        }
        if candidate.is_empty()
            || candidate.starts_with('@')
            || candidate.starts_with("//")
            || candidate.starts_with("/*")
            || candidate.starts_with('*')
        {
            continue;
        }
        if let Some(name) = nestjs_method_name(candidate) {
            return Some(name);
        }
    }
    None
}

fn nestjs_route_annotation(line: &str) -> bool {
    [
        "@Get", "@Post", "@Put", "@Patch", "@Delete", "@Head", "@Options",
    ]
    .iter()
    .any(|marker| line.starts_with(marker))
}

fn nestjs_method_name(line: &str) -> Option<String> {
    let open = line.find('(')?;
    let before = line[..open].trim_end();
    let name = before
        .rsplit(|value: char| !value.is_ascii_alphanumeric() && value != '_')
        .next()
        .unwrap_or_default();
    if name.is_empty()
        || matches!(
            name,
            "async" | "public" | "private" | "protected" | "static" | "get" | "set"
        )
    {
        return None;
    }
    Some(name.to_string())
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
    // Decorators and annotations bind to the first declaration below them.
    // Keep the scan source-ordered: a former three-pass implementation could
    // skip `export class CatsController` and select its later
    // `constructor(private ...)` merely because the parameter contained an
    // access modifier. That attached framework facts to a constructor (and,
    // after a global fallback, sometimes to another project entirely).
    for line in lines.iter().skip(index).take(24) {
        let trimmed = line.trim_start();
        if trimmed.is_empty()
            || trimmed.starts_with('@')
            || trimmed.starts_with('[')
            || trimmed.starts_with("#[")
            || trimmed.starts_with("//")
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*')
        {
            continue;
        }
        for keyword in [
            "class ",
            "struct ",
            "interface ",
            "object ",
            "record ",
            "enum ",
            "trait ",
            "function ",
            "fn ",
            "func ",
            "def ",
            "void ",
        ] {
            if let Some(name) = identifier_after(line, keyword) {
                return Some(name);
            }
        }
        if let Some(open) = line.find('(') {
            let before = line[..open].trim_end();
            if before.contains('=') {
                continue;
            }
            let name = before
                .rsplit(|value: char| !value.is_ascii_alphanumeric() && value != '_')
                .next()
                .unwrap_or_default();
            let lower = name.to_ascii_lowercase();
            if !name.is_empty()
                && !matches!(
                    lower.as_str(),
                    "if"
                        | "for"
                        | "while"
                        | "switch"
                        | "catch"
                        | "get"
                        | "post"
                        | "put"
                        | "patch"
                        | "delete"
                        | "route"
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
