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
