fn collect_constants(lines: &[String]) -> HashMap<String, String> {
    let mut constants = HashMap::new();
    for _ in 0..2 {
        for line in lines {
            let Some(equal) = line.find('=') else {
                continue;
            };
            let left = line[..equal].trim().trim_start_matches("const ").trim();
            let Some(name) = left
                .rsplit(|value: char| !value.is_ascii_alphanumeric() && value != '_')
                .next()
            else {
                continue;
            };
            if name.is_empty()
                || !name
                    .chars()
                    .next()
                    .is_some_and(|value| value.is_ascii_alphabetic() || value == '_')
            {
                continue;
            }
            let expression = line[equal + 1..].trim().trim_end_matches(';');
            if let Some(value) = resolve_expression(expression, &constants) {
                if normalize_url_path(&value).is_some() {
                    constants.insert(name.to_string(), value);
                }
            }
        }
    }
    constants
}

fn extract_raw_requests(
    source: &str,
    instances: &HashMap<String, Option<String>>,
) -> Vec<RawRequest> {
    let mut output = Vec::new();
    let searchable = mask_strings(source);
    for pattern in PATTERNS {
        let mut cursor = 0;
        while let Some(relative) = searchable[cursor..].find(pattern.marker) {
            let start = cursor + relative;
            if (!pattern.marker.starts_with('.')
                && start > 0
                && searchable.as_bytes()[start - 1].is_ascii_alphanumeric())
                || searchable[..start].trim_end().ends_with("function")
            {
                cursor = start + pattern.marker.len();
                continue;
            }
            let Some(args) = call_arguments(&source[start + pattern.marker.len() - 1..]) else {
                cursor = start + pattern.marker.len();
                continue;
            };
            let method = pattern
                .method
                .map(str::to_string)
                .or_else(|| {
                    pattern.method_arg.and_then(|index| {
                        args.get(index)
                            .and_then(|value| parse_static_string(value))
                            .map(|value| value.to_ascii_uppercase())
                    })
                })
                .or_else(|| {
                    if pattern.marker == "fetch(" {
                        args.iter()
                            .find_map(|arg| {
                                arg.strip_prefix("method:")
                                    .and_then(parse_static_string)
                                    .map(|value| value.to_ascii_uppercase())
                            })
                            .or_else(|| (args.len() == 1).then_some("GET".to_string()))
                    } else {
                        None
                    }
                });
            let Some(url_expression) = args.get(pattern.url_arg).cloned() else {
                cursor = start + pattern.marker.len();
                continue;
            };
            output.push(RawRequest {
                client: pattern.client.to_string(),
                method,
                url_expression,
                evidence: format!("pattern:{}", pattern.marker.trim_end_matches('(')),
                line_offset: source[..start]
                    .bytes()
                    .filter(|byte| *byte == b'\n')
                    .count(),
            });
            cursor = start + pattern.marker.len();
        }
    }
    if let Some(url_start) = searchable.find("CURLOPT_URL") {
        let Some(url) = quoted_value_after_at(source, "CURLOPT_URL", url_start) else {
            return output;
        };
        let method = searchable
            .find("CURLOPT_CUSTOMREQUEST")
            .and_then(|start| quoted_value_after_at(source, "CURLOPT_CUSTOMREQUEST", start))
            .map(|value| value.to_ascii_uppercase())
            .or_else(|| {
                source
                    .contains("CURLOPT_POST")
                    .then_some("POST".to_string())
            })
            .or_else(|| {
                source
                    .contains("CURLOPT_HTTPGET")
                    .then_some("GET".to_string())
            });
        output.push(RawRequest {
            client: "libcurl".to_string(),
            method,
            url_expression: format!("\"{url}\""),
            evidence: "pattern:CURLOPT_URL".to_string(),
            line_offset: source[..url_start]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count(),
        });
    }
    output.extend(extract_instance_requests(source, instances));
    output
}

fn extract_instance_requests(
    source: &str,
    instances: &HashMap<String, Option<String>>,
) -> Vec<RawRequest> {
    let searchable = mask_strings(source);
    let mut output = Vec::new();
    for (receiver, base_url) in instances {
        for method in ["get", "post", "put", "patch", "delete"] {
            let marker = format!("{receiver}.{method}(");
            let mut cursor = 0;
            while let Some(relative) = searchable[cursor..].find(&marker) {
                let start = cursor + relative;
                let Some(args) = call_arguments(&source[start + marker.len() - 1..]) else {
                    cursor = start + marker.len();
                    continue;
                };
                let Some(url) = args.first() else {
                    cursor = start + marker.len();
                    continue;
                };
                let url_expression = base_url
                    .as_deref()
                    .map(|base| format!("{base:?} + {url}"))
                    .unwrap_or_else(|| url.clone());
                output.push(RawRequest {
                    client: "http-instance".to_string(),
                    method: Some(method.to_ascii_uppercase()),
                    url_expression,
                    evidence: format!("pattern:{receiver}.{method}"),
                    line_offset: source[..start]
                        .bytes()
                        .filter(|byte| *byte == b'\n')
                        .count(),
                });
                cursor = start + marker.len();
            }
        }
    }
    output
}

fn collect_http_instances(
    source: &str,
    constants: &HashMap<String, String>,
) -> HashMap<String, Option<String>> {
    let searchable = mask_strings(source);
    let mut instances = HashMap::new();
    for marker in ["axios.create(", "createApiClient(", "createClient("] {
        let mut cursor = 0;
        while let Some(relative) = searchable[cursor..].find(marker) {
            let start = cursor + relative;
            let line_start = source[..start].rfind('\n').map_or(0, |index| index + 1);
            let line = &source[line_start..start];
            let Some(equal) = line.rfind('=') else {
                cursor = start + marker.len();
                continue;
            };
            let receiver = line[..equal]
                .trim()
                .trim_start_matches("const ")
                .trim_start_matches("let ")
                .trim_start_matches("var ")
                .trim();
            if receiver.is_empty()
                || !receiver
                    .chars()
                    .all(|value| value.is_ascii_alphanumeric() || value == '_')
            {
                cursor = start + marker.len();
                continue;
            }
            let args = call_arguments(&source[start + marker.len() - 1..]).unwrap_or_default();
            let base_expression = if marker == "axios.create(" {
                args.first().and_then(|arg| {
                    let start = arg.find("baseURL")?;
                    let value = arg[start + "baseURL".len()..]
                        .trim_start()
                        .strip_prefix(':')?
                        .split([',', '}'])
                        .next()?
                        .trim();
                    Some(value.to_string())
                })
            } else {
                args.first().cloned()
            };
            let base_url = base_expression.and_then(|expression| {
                resolve_expression(&expression, constants).and_then(|value| {
                    normalize_url_path(&value).or(Some(value))
                })
            });
            instances.insert(receiver.to_string(), base_url);
            cursor = start + marker.len();
        }
    }
    instances
}

fn quoted_value_after_at(source: &str, marker: &str, start: usize) -> Option<String> {
    let start = start + marker.len();
    let tail = &source[start..];
    let quote = tail.find(['\'', '"'])?;
    let value = &tail[quote + 1..];
    let end = value.find(tail.as_bytes()[quote] as char)?;
    Some(value[..end].to_string())
}

fn mask_strings(source: &str) -> String {
    let mut masked = source.as_bytes().to_vec();
    let mut quote = None;
    let mut escaped = false;
    for (index, byte) in source.as_bytes().iter().copied().enumerate() {
        if let Some(active) = quote {
            if escaped {
                if byte < 0x80 && byte != b'\n' && byte != b'\r' {
                    masked[index] = b' ';
                }
                escaped = false;
            } else if byte == b'\\' {
                masked[index] = b' ';
                escaped = true;
            } else if byte == active {
                masked[index] = b' ';
                quote = None;
            } else if byte < 0x80 && byte != b'\n' && byte != b'\r' {
                masked[index] = b' ';
            }
        } else if matches!(byte, b'\'' | b'"' | 96) {
            masked[index] = b' ';
            quote = Some(byte);
        }
    }
    String::from_utf8_lossy(&masked).into_owned()
}

fn is_test_only_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    let parts = normalized.split('/').collect::<Vec<_>>();
    if parts.iter().any(|part| {
        matches!(
            *part,
            "test" | "tests" | "__tests__" | "spec" | "specs" | "integration-tests"
        )
    }) {
        return true;
    }
    let file = parts.last().copied().unwrap_or_default();
    file.starts_with("test_")
        || file.contains("_test.")
        || file.contains(".test.")
        || file.contains(".spec.")
        || file.contains("_spec.")
        || file.ends_with("test.java")
}

fn can_confirm_client(client: &str) -> bool {
    matches!(
        client,
        "fetch"
            | "axios"
            | "requests"
            | "httpx"
            | "rest-template"
            | "go-http"
            | "reqwest"
            | "laravel-http"
            | "faraday"
            | "httparty"
            | "cpr"
            | "http-instance"
    )
}

fn call_arguments(source: &str) -> Option<Vec<String>> {
    let mut depth = 0i32;
    let mut quote = None;
    let mut start = 1usize;
    let mut args = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    for (index, value) in chars.iter().copied().enumerate().skip(1) {
        if let Some(active) = quote {
            if value == '\\' {
                continue;
            }
            if value == active {
                quote = None;
            }
            continue;
        }
        if value == '"' || value == '\'' || value == '`' {
            quote = Some(value);
            continue;
        }
        match value {
            '(' | '[' | '{' => depth += 1,
            ')' => {
                if depth == 0 {
                    args.push(
                        source
                            .chars()
                            .skip(start)
                            .take(index.saturating_sub(start))
                            .collect::<String>()
                            .trim()
                            .to_string(),
                    );
                    return Some(args.into_iter().filter(|arg| !arg.is_empty()).collect());
                }
                depth -= 1;
            }
            ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                args.push(
                    source
                        .chars()
                        .skip(start)
                        .take(index.saturating_sub(start))
                        .collect::<String>()
                        .trim()
                        .to_string(),
                );
                start = index + 1;
            }
            _ => {}
        }
    }
    None
}

fn resolve_expression(expression: &str, constants: &HashMap<String, String>) -> Option<String> {
    if let Some(value) = parse_static_string(expression.trim()) {
        return Some(value);
    }
    if let Some(value) = resolve_template(expression.trim(), constants) {
        return Some(value);
    }
    let terms = expression.split('+').map(str::trim).collect::<Vec<_>>();
    if terms.len() < 2 {
        return constants.get(expression.trim()).cloned();
    }
    let mut value = String::new();
    for term in terms {
        if let Some(literal) = parse_static_string(term) {
            value.push_str(&literal);
        } else if let Some(constant) = constants.get(term) {
            value.push_str(constant);
        } else {
            return None;
        }
    }
    Some(value)
}

fn resolve_template(expression: &str, constants: &HashMap<String, String>) -> Option<String> {
    let body = expression
        .strip_prefix('`')?
        .strip_suffix('`')?;
    let mut remaining = body;
    let mut output = String::new();
    while let Some(start) = remaining.find("${") {
        output.push_str(&remaining[..start]);
        let end = remaining[start + 2..].find('}')? + start + 2;
        let name = remaining[start + 2..end].trim();
        output.push_str(constants.get(name)?);
        remaining = &remaining[end + 1..];
    }
    output.push_str(remaining);
    Some(output)
}

fn parse_static_string(value: &str) -> Option<String> {
    let value = value.trim().trim_end_matches(',').trim();
    let bytes = value.as_bytes();
    if bytes.len() < 2
        || !matches!(bytes[0], b'\'' | b'"' | b'`')
        || bytes[bytes.len() - 1] != bytes[0]
    {
        return None;
    }
    if value.contains("${") {
        return None;
    }
    let body = &value[1..value.len() - 1];
    Some(
        body.replace("\\\\", "\\")
            .replace("\\'", "'")
            .replace("\\\"", "\""),
    )
}

fn normalize_url_path(value: &str) -> Option<String> {
    let mut value = value.trim().to_string();
    let authority_start = value
        .strip_prefix("https://")
        .map(|_| 8)
        .or_else(|| value.strip_prefix("http://").map(|_| 7));
    if let Some(authority_start) = authority_start {
        let authority_end = value[authority_start..]
            .find('/')
            .map(|offset| offset + authority_start)?;
        value = value[authority_end..].to_string();
    }
    if !value.starts_with('/') {
        if value.starts_with("./") {
            value = value.trim_start_matches("./").to_string();
        }
        if value.is_empty() || value.chars().any(char::is_whitespace) || value.contains(':') {
            return None;
        }
    }
    if let Some(index) = value.find(['?', '#']) {
        value.truncate(index);
    }
    let value = value
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("/");
    Some(if value.is_empty() {
        "/".to_string()
    } else {
        format!("/{value}")
    })
}

fn is_rooted_url(value: &str) -> bool {
    value.starts_with('/') || value.starts_with("http://") || value.starts_with("https://")
}
