fn file_system_route(
    pack: &FrameworkPack,
    path: &str,
    source: &str,
) -> Option<(String, String, Option<String>, usize)> {
    let normalized = path.replace('\\', "/");
    let relative = match pack.id.as_str() {
        "nextjs" if normalized.contains("/pages/") || normalized.starts_with("pages/") => {
            after_directory(&normalized, "pages")?
        }
        "nextjs" if normalized.contains("/app/") || normalized.starts_with("app/") => {
            after_directory(&normalized, "app")?
        }
        "nuxt" if normalized.contains("/server/api/") || normalized.starts_with("server/api/") => {
            after_directory(&normalized, "server/api")?
        }
        "nuxt" if normalized.contains("/pages/") || normalized.starts_with("pages/") => {
            after_directory(&normalized, "pages")?
        }
        "sveltekit"
            if normalized.contains("/src/routes/") || normalized.starts_with("src/routes/") =>
        {
            after_directory(&normalized, "src/routes")?
        }
        "dart-frog" if normalized.contains("/routes/") || normalized.starts_with("routes/") => {
            after_directory(&normalized, "routes")?
        }
        _ => return None,
    };
    let mut segments = relative.split('/').collect::<Vec<_>>();
    let file_name = segments.pop()?;
    let mut file = file_name
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(file_name);
    let file_method = if pack.id == "nuxt" {
        file.rsplit_once('.').and_then(|(stem, suffix)| {
            let method = filesystem_http_method(suffix)?;
            file = stem;
            Some(method)
        })
    } else {
        None
    };
    if matches!(file, "index" | "page" | "+page" | "+server" | "route") {
        // directory path already represents the route
    } else {
        segments.push(file);
    }
    let route = segments
        .into_iter()
        .filter(|segment| !segment.is_empty())
        .filter_map(|segment| filesystem_route_segment(pack.id.as_str(), segment))
        .collect::<Vec<_>>();
    let route_path = if route.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", route.join("/"))
    };
    let exported_method = exported_http_handler(source);
    let method = file_method
        .or_else(|| exported_method.map(|(method, _)| method))
        .unwrap_or("ANY")
        .to_string();
    let handler = if source.contains("onRequest") {
        Some("onRequest".to_string())
    } else if source.contains("defineEventHandler(handler") {
        Some("handler".to_string())
    } else if source.contains("defineEventHandler") {
        Some("default".to_string())
    } else if let Some((method, _)) = exported_method {
        Some(method.to_string())
    } else if source.contains("export default function ") {
        source
            .lines()
            .find_map(|line| identifier_after(line, "export default function "))
            .or_else(|| Some("default".to_string()))
    } else if pack.id == "sveltekit" {
        source
            .lines()
            .find_map(|line| assignment_target_before(line, "defineComponent("))
            .or_else(|| {
                source
                    .lines()
                    .find(|line| line.contains("export function load"))
                    .map(|_| "load".to_string())
            })
            .or_else(|| {
                ["GET", "POST", "PUT", "PATCH", "DELETE"]
                    .iter()
                    .find(|method| source.contains(&format!("function {method}")))
                    .map(|method| (*method).to_string())
            })
    } else {
        ["GET", "POST", "PUT", "PATCH", "DELETE"]
            .iter()
            .find(|method| source.contains(&format!("function {}", method)))
            .map(|method| (*method).to_string())
    };
    let source_line = exported_method
        .map(|(_, line)| line)
        .or_else(|| {
            source
                .lines()
                .position(|line| {
                    line.contains("function GET")
                        || line.contains("export default")
                        || line.contains("onRequest")
                })
                .map(|line| line + 1)
        })
        .unwrap_or(1);
    Some((route_path, method, handler, source_line))
}

fn filesystem_http_method(value: &str) -> Option<&'static str> {
    match value.to_ascii_lowercase().as_str() {
        "get" => Some("GET"),
        "post" => Some("POST"),
        "put" => Some("PUT"),
        "patch" => Some("PATCH"),
        "delete" => Some("DELETE"),
        "head" => Some("HEAD"),
        "options" => Some("OPTIONS"),
        _ => None,
    }
}

fn filesystem_route_segment(pack: &str, segment: &str) -> Option<String> {
    if matches!(pack, "nextjs" | "sveltekit")
        && ((segment.starts_with('(') && segment.ends_with(')')) || segment.starts_with('@'))
    {
        return None;
    }
    if segment.starts_with("[[...") && segment.ends_with("]]") {
        return Some(format!("*{}", &segment[5..segment.len() - 2]));
    }
    if segment.starts_with("[...") && segment.ends_with(']') {
        return Some(format!("*{}", &segment[4..segment.len() - 1]));
    }
    if segment.starts_with('[') && segment.ends_with(']') {
        return Some(format!(":{}", &segment[1..segment.len() - 1]));
    }
    Some(segment.to_string())
}

fn exported_http_handler(source: &str) -> Option<(&'static str, usize)> {
    let methods = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];
    for (line, text) in source.lines().enumerate() {
        let trimmed = text.trim_start();
        for method in methods {
            let markers = [
                format!("export async function {method}"),
                format!("export function {method}"),
                format!("export const {method}"),
                format!("export let {method}"),
                format!("export var {method}"),
            ];
            if markers.iter().any(|marker| {
                trimmed.starts_with(marker)
                    && trimmed[marker.len()..]
                        .chars()
                        .next()
                        .is_none_or(|value| !value.is_ascii_alphanumeric() && value != '_')
            }) {
                return Some((method, line + 1));
            }
        }
    }
    None
}

fn after_directory<'a>(path: &'a str, directory: &str) -> Option<&'a str> {
    let prefix = format!("{directory}/");
    if let Some(relative) = path.strip_prefix(&prefix) {
        return Some(relative);
    }
    let marker = format!("/{directory}/");
    path.split_once(&marker).map(|(_, relative)| relative)
}

fn extract_generic_facts(
    pack: &FrameworkPack,
    path: &str,
    source: &str,
    documents: &[DocumentOutput],
    facts: &mut Vec<FrameworkFact>,
) {
    let symbol_index = build_framework_symbol_index(documents);
    extract_generic_facts_with_index(pack, path, source, &symbol_index, facts);
}

fn extract_generic_facts_with_index(
    pack: &FrameworkPack,
    path: &str,
    source: &str,
    symbol_index: &FrameworkSymbolIndex,
    facts: &mut Vec<FrameworkFact>,
) {
    let lines: Vec<&str> = source.lines().collect();
    let code = source_code_mask(source, &pack.language);
    let code_lines: Vec<&str> = code.lines().collect();
    let comment_free = source_without_comments(source, &pack.language);
    let comment_free_lines: Vec<&str> = comment_free.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        let code_line = code_lines.get(index).copied().unwrap_or_default();
        for output in &pack.rules {
            if matches!(output.as_str(), "HTTP_ROUTE" | "HANDLES") {
                continue;
            }
            if pack.language == "java"
                && output == "DEPENDENCY"
                && java_constructor_is_injection(&lines, index, &pack.id)
            {
                let dependencies = java_constructor_dependency_types(&lines, index);
                if !dependencies.is_empty() {
                    let handler = java_enclosing_type(&lines, index)
                        .as_deref()
                        .and_then(|name| resolve_java_type_indexed(symbol_index, path, name))
                        .filter(|symbol| project_symbol_is_defined_indexed(symbol_index, symbol));
                    for target in dependencies {
                        let mut properties = BTreeMap::new();
                        properties.insert("target".to_string(), target);
                        facts.push(FrameworkFact {
                            id: format!(
                                "fact:{}:{}:{}:{}:{}",
                                pack.id,
                                output,
                                path,
                                index + 1,
                                properties["target"]
                            ),
                            kind: output.clone(),
                            framework: pack.id.clone(),
                            symbol: handler.clone(),
                            method: None,
                            path: None,
                            source_file: path.to_string(),
                            source_line: index + 1,
                            source_end_line: index + 1,
                            source_range: line_source_range(index, line),
                            evidence: vec!["java_constructor_injection".to_string()],
                            properties,
                        });
                    }
                    continue;
                }
            }
            let evidence_line = if matches!(pack.id.as_str(), "vue" | "blazor" | "dotnet-maui")
                && output == "EVENT_HANDLER"
            {
                comment_free_lines.get(index).copied().unwrap_or_default()
            } else {
                code_line
            };
            let Some(evidence) = output_evidence(pack, output, evidence_line) else {
                continue;
            };
            let dependency_context = (pack.language == "java" && output == "DEPENDENCY")
                .then(|| java_dependency_annotation_context(&lines, index))
                .flatten();
            let fact_line = dependency_context.as_deref().unwrap_or(line);
            let handler_name =
                if pack.language == "java" && matches!(output.as_str(), "SERVICE" | "COMPONENT") {
                    fact_target_name(output, line).or_else(|| java_nearby_type(&lines, index))
                } else {
                    fact_target_name(output, fact_line).or_else(|| nearby_handler(&lines, index))
                };
            let symbol = handler_name
                .as_ref()
                .and_then(|name| {
                    if pack.language == "java" && matches!(output.as_str(), "SERVICE" | "COMPONENT")
                    {
                        resolve_java_type_indexed(symbol_index, path, name)
                    } else if matches!(pack.language.as_str(), "javascript" | "typescript")
                        && !matches!(output.as_str(), "COMPONENT" | "SERVICE")
                    {
                        resolve_symbol_on_line_indexed(symbol_index, path, name, index)
                    } else {
                        resolve_symbol_at_indexed(symbol_index, path, name, index)
                    }
                })
                .and_then(|symbol| project_definition_for_symbol_indexed(symbol_index, &symbol));
            facts.push(FrameworkFact {
                id: format!("fact:{}:{}:{}:{}", pack.id, output, path, index + 1),
                kind: output.clone(),
                framework: pack.id.clone(),
                symbol,
                method: None,
                path: None,
                source_file: path.to_string(),
                source_line: index + 1,
                source_end_line: index + 1,
                source_range: line_source_range(index, line),
                evidence: vec![evidence],
                properties: fact_properties(output, line),
            });
        }
    }
}

fn java_constructor_dependency_types(lines: &[&str], index: usize) -> Vec<String> {
    if !lines.get(index).is_some_and(|line| line.contains('(')) {
        return Vec::new();
    }
    let Some(signature) = lines
        .iter()
        .skip(index)
        .take(16)
        .scan(String::new(), |buffer, line| {
            if !buffer.is_empty() {
                buffer.push(' ');
            }
            buffer.push_str(line.trim());
            Some(buffer.clone())
        })
        .find(|value| value.contains(')'))
    else {
        return Vec::new();
    };
    let Some(open) = signature.find('(') else {
        return Vec::new();
    };
    let before_open = signature[..open].trim();
    let Some(constructor) = before_open
        .rsplit(|value: char| !value.is_ascii_alphanumeric() && value != '_')
        .next()
    else {
        return Vec::new();
    };
    if constructor.is_empty()
        || !constructor
            .chars()
            .next()
            .is_some_and(|value| value.is_ascii_uppercase())
        || java_enclosing_type(lines, index).as_deref() != Some(constructor)
        || !(before_open.contains("public ")
            || before_open.contains("protected ")
            || before_open.contains("private ")
            || before_open
                .split_whitespace()
                .filter(|token| *token != constructor)
                .all(|token| token.starts_with('@') || token.starts_with('<')))
    {
        return Vec::new();
    }
    let Some(close) = signature[open + 1..]
        .find(')')
        .map(|value| value + open + 1)
    else {
        return Vec::new();
    };
    signature[open + 1..close]
        .split(',')
        .filter_map(|parameter| {
            parameter
                .split_whitespace()
                .filter(|token| !token.starts_with('@'))
                .map(|token| {
                    token.trim_matches(|value: char| {
                        !value.is_ascii_alphanumeric() && value != '.' && value != '_'
                    })
                })
                .find(|token| {
                    token
                        .chars()
                        .next()
                        .is_some_and(|value| value.is_ascii_uppercase())
                })
                .map(|token| token.split('<').next().unwrap_or(token).to_string())
        })
        .collect()
}

fn java_constructor_is_injection(lines: &[&str], index: usize, framework: &str) -> bool {
    for (offset, line) in lines.iter().take(index + 1).rev().take(5).enumerate() {
        if line.contains("@Autowired") || line.contains("@Inject") {
            return true;
        }
        if offset > 0 && !line.trim().is_empty() {
            break;
        }
    }
    if !matches!(
        framework,
        "spring" | "spring-boot" | "spring-mvc" | "spring-webflux"
    ) {
        return false;
    }
    let Some(type_name) = java_enclosing_type(lines, index) else {
        return false;
    };
    let Some(type_index) =
        lines
            .iter()
            .enumerate()
            .take(index + 1)
            .rev()
            .find_map(|(line_index, line)| {
                (identifier_after(line, "class ").as_deref() == Some(type_name.as_str())
                    || identifier_after(line, "record ").as_deref() == Some(type_name.as_str()))
                .then_some(line_index)
            })
    else {
        return false;
    };
    for line in lines.iter().take(type_index).rev().take(8) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if [
            "@Component",
            "@Service",
            "@Repository",
            "@Controller",
            "@RestController",
            "@Configuration",
        ]
        .iter()
        .any(|annotation| line.contains(annotation))
        {
            return true;
        }
        if !trimmed.starts_with('@') {
            break;
        }
    }
    false
}

fn java_dependency_annotation_context(lines: &[&str], index: usize) -> Option<String> {
    let line = *lines.get(index)?;
    if !(line.contains("@Autowired") || line.contains("@Inject")) || line.contains('(') {
        return None;
    }
    let mut context = line.trim().to_string();
    for next in lines.iter().skip(index + 1).take(4) {
        let trimmed = next.trim();
        if next.contains('(') && !trimmed.starts_with('@') {
            return None;
        }
        context.push(' ');
        context.push_str(trimmed);
        if next.contains(';') {
            return Some(context);
        }
    }
    None
}

fn java_nearby_type(lines: &[&str], index: usize) -> Option<String> {
    lines.iter().skip(index).take(8).find_map(|line| {
        ["class ", "record ", "interface ", "enum "]
            .iter()
            .find_map(|keyword| identifier_after(line, keyword))
    })
}

fn java_enclosing_type(lines: &[&str], index: usize) -> Option<String> {
    lines
        .iter()
        .take(index + 1)
        .rev()
        .take(200)
        .find_map(|line| {
            identifier_after(line, "class ").or_else(|| identifier_after(line, "record "))
        })
}

