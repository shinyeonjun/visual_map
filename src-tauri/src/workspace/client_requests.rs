use super::model::{ClientRequest, CodeInventory, CodeInventoryItem};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

const MAX_FILES: usize = 80_000;
const MAX_FILE_BYTES: u64 = 1_000_000;
const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Copy)]
struct Pattern {
    marker: &'static str,
    client: &'static str,
    method: Option<&'static str>,
    method_arg: Option<usize>,
    url_arg: usize,
}

#[derive(Debug)]
struct RawRequest {
    client: String,
    method: Option<String>,
    url_expression: String,
    evidence: String,
    line_offset: usize,
}

const PATTERNS: &[Pattern] = &[
    Pattern {
        marker: "fetch(",
        client: "fetch",
        method: None,
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "axios.get(",
        client: "axios",
        method: Some("GET"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "axios.post(",
        client: "axios",
        method: Some("POST"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "axios.put(",
        client: "axios",
        method: Some("PUT"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "axios.patch(",
        client: "axios",
        method: Some("PATCH"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "axios.delete(",
        client: "axios",
        method: Some("DELETE"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "axios.request(",
        client: "axios",
        method: None,
        method_arg: Some(0),
        url_arg: 1,
    },
    Pattern {
        marker: "requests.get(",
        client: "requests",
        method: Some("GET"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "requests.post(",
        client: "requests",
        method: Some("POST"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "requests.put(",
        client: "requests",
        method: Some("PUT"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "requests.delete(",
        client: "requests",
        method: Some("DELETE"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "httpx.get(",
        client: "httpx",
        method: Some("GET"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "httpx.post(",
        client: "httpx",
        method: Some("POST"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "http.get(",
        client: "http",
        method: Some("GET"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "http.post(",
        client: "http",
        method: Some("POST"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "client.get(",
        client: "http-client",
        method: Some("GET"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "client.post(",
        client: "http-client",
        method: Some("POST"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "client.put(",
        client: "http-client",
        method: Some("PUT"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "client.delete(",
        client: "http-client",
        method: Some("DELETE"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "session.get(",
        client: "http-session",
        method: Some("GET"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "session.post(",
        client: "http-session",
        method: Some("POST"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: ".GetAsync(",
        client: "dotnet-http-client",
        method: Some("GET"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: ".PostAsync(",
        client: "dotnet-http-client",
        method: Some("POST"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: ".PutAsync(",
        client: "dotnet-http-client",
        method: Some("PUT"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: ".DeleteAsync(",
        client: "dotnet-http-client",
        method: Some("DELETE"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "RestTemplate.getForObject(",
        client: "rest-template",
        method: Some("GET"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "RestTemplate.postForObject(",
        client: "rest-template",
        method: Some("POST"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "RestTemplate.exchange(",
        client: "rest-template",
        method: None,
        method_arg: Some(1),
        url_arg: 0,
    },
    Pattern {
        marker: "http.NewRequest(",
        client: "go-http",
        method: None,
        method_arg: Some(0),
        url_arg: 1,
    },
    Pattern {
        marker: "http.NewRequestWithContext(",
        client: "go-http",
        method: None,
        method_arg: Some(1),
        url_arg: 2,
    },
    Pattern {
        marker: "http.Get(",
        client: "go-http",
        method: Some("GET"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "http.Post(",
        client: "go-http",
        method: Some("POST"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "reqwest::get(",
        client: "reqwest",
        method: Some("GET"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "Http::get(",
        client: "laravel-http",
        method: Some("GET"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "Http::post(",
        client: "laravel-http",
        method: Some("POST"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "Faraday.get(",
        client: "faraday",
        method: Some("GET"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "Faraday.post(",
        client: "faraday",
        method: Some("POST"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "HTTParty.get(",
        client: "httparty",
        method: Some("GET"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "HTTParty.post(",
        client: "httparty",
        method: Some("POST"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "cpr::Get(",
        client: "cpr",
        method: Some("GET"),
        method_arg: None,
        url_arg: 0,
    },
    Pattern {
        marker: "cpr::Post(",
        client: "cpr",
        method: Some("POST"),
        method_arg: None,
        url_arg: 0,
    },
];

pub(crate) fn extract_client_requests(
    repo_path: &str,
    inventory: &CodeInventory,
) -> Result<Vec<ClientRequest>, String> {
    // ponytail: bounded source scan keeps this Phase 1 extractor predictable; move to provider
    // AST/request facts when a repository exceeds these limits or needs runtime resolution.
    let root = Path::new(repo_path);
    if !root.is_dir() {
        return Err(format!(
            "코드 요청 분석 루트가 디렉터리가 아닙니다: {repo_path}"
        ));
    }
    let mut files = Vec::new();
    collect_source_files(root, &mut files, 0)?;
    files.sort();

    let mut total_bytes = 0u64;
    let mut requests = Vec::new();
    let mut seen = HashSet::new();
    for file in files.into_iter().take(MAX_FILES) {
        let metadata = match fs::metadata(&file) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if metadata.len() > MAX_FILE_BYTES
            || total_bytes.saturating_add(metadata.len()) > MAX_TOTAL_BYTES
        {
            continue;
        }
        total_bytes = total_bytes.saturating_add(metadata.len());
        let Ok(source) = fs::read_to_string(&file) else {
            continue;
        };
        let relative = file
            .strip_prefix(root)
            .ok()
            .and_then(|path| path.to_str())
            .unwrap_or_default()
            .replace('\\', "/");
        if relative.is_empty() {
            continue;
        }
        let clean_lines = strip_comments(&source);
        let constants = collect_constants(&clean_lines);
        for (index, _) in clean_lines.iter().enumerate() {
            let window = clean_lines[index..clean_lines.len().min(index + 4)].join("\n");
            for raw in extract_raw_requests(&window) {
                let source_line = index + 1 + raw.line_offset;
                let Some(id) = request_id(&relative, source_line, &raw) else {
                    continue;
                };
                if !seen.insert(id.clone()) {
                    continue;
                }
                let resolved = resolve_expression(&raw.url_expression, &constants);
                let path = resolved.as_deref().and_then(normalize_url_path);
                let method = raw.method.clone().filter(|method| method != "REQUEST");
                let test_only = is_test_only_path(&relative);
                let resolution = if test_only {
                    "excluded"
                } else if path.is_some()
                    && method.is_some()
                    && can_confirm_client(&raw.client)
                    && resolved.as_deref().is_some_and(is_rooted_url)
                {
                    "static-confirmed"
                } else if path.is_some() {
                    "candidate"
                } else {
                    "unknown"
                };
                let mut evidence = vec![
                    format!("client:{}", raw.client),
                    format!("source:{}:{}", relative, source_line),
                    raw.evidence,
                ];
                if resolved.is_some() {
                    evidence.push("url-static-value".to_string());
                }
                if method.is_some() {
                    evidence.push("method-static-value".to_string());
                }
                if test_only {
                    evidence.push("excluded:test-only".to_string());
                }
                requests.push(ClientRequest {
                    id,
                    client: raw.client,
                    method,
                    raw_url: path.clone().unwrap_or_else(|| "<unresolved>".to_string()),
                    path,
                    source_file: relative.clone(),
                    line: source_line as u64,
                    end_line: source_line as u64,
                    caller_id: find_caller(inventory, &relative, source_line as u64),
                    resolution: resolution.to_string(),
                    confidence: match resolution {
                        "static-confirmed" => Some(95),
                        "candidate" => Some(45),
                        "excluded" => None,
                        _ => Some(0),
                    },
                    evidence,
                });
            }
        }
    }
    requests.sort_by(|left, right| {
        left.source_file
            .cmp(&right.source_file)
            .then(left.line.cmp(&right.line))
            .then(left.id.cmp(&right.id))
    });
    Ok(requests)
}

fn collect_source_files(
    path: &Path,
    output: &mut Vec<PathBuf>,
    depth: usize,
) -> Result<(), String> {
    if depth > 64 || output.len() >= MAX_FILES {
        return Ok(());
    }
    let entries = fs::read_dir(path)
        .map_err(|error| format!("코드 요청 분석 디렉터리를 읽지 못했습니다: {error}"))?;
    for entry in entries.flatten() {
        let entry_path = entry.path();
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if entry_path.is_dir() {
            if matches!(
                name.as_str(),
                ".git"
                    | "node_modules"
                    | "target"
                    | "dist"
                    | "build"
                    | "coverage"
                    | "vendor"
                    | ".venv"
                    | "venv"
                    | "__pycache__"
                    | "bin"
                    | "obj"
            ) {
                continue;
            }
            collect_source_files(&entry_path, output, depth + 1)?;
        } else if is_source_file(&entry_path) {
            output.push(entry_path);
        }
        if output.len() >= MAX_FILES {
            break;
        }
    }
    Ok(())
}

fn is_source_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "c" | "cc"
            | "cpp"
            | "cxx"
            | "h"
            | "hh"
            | "hpp"
            | "cs"
            | "dart"
            | "go"
            | "java"
            | "js"
            | "jsx"
            | "mjs"
            | "cjs"
            | "php"
            | "py"
            | "rb"
            | "rs"
            | "ts"
            | "tsx"
    )
}

fn strip_comments(source: &str) -> Vec<String> {
    let mut block = false;
    source
        .lines()
        .map(|line| {
            let mut output = String::new();
            let chars: Vec<char> = line.chars().collect();
            let mut index = 0;
            let mut quote = None;
            while index < chars.len() {
                if block {
                    if index + 1 < chars.len() && chars[index] == '*' && chars[index + 1] == '/' {
                        block = false;
                        index += 2;
                    } else {
                        index += 1;
                    }
                    continue;
                }
                if let Some(active) = quote {
                    output.push(chars[index]);
                    if chars[index] == '\\' && index + 1 < chars.len() {
                        output.push(chars[index + 1]);
                        index += 2;
                        continue;
                    }
                    if chars[index] == active {
                        quote = None;
                    }
                    index += 1;
                    continue;
                }
                if chars[index] == '"' || chars[index] == '\'' || chars[index] == '`' {
                    quote = Some(chars[index]);
                    output.push(chars[index]);
                    index += 1;
                    continue;
                }
                if index + 1 < chars.len() && chars[index] == '/' && chars[index + 1] == '*' {
                    block = true;
                    index += 2;
                    continue;
                }
                if index + 1 < chars.len() && chars[index] == '/' && chars[index + 1] == '/' {
                    break;
                }
                if (chars[index] == '#' && output.trim().is_empty())
                    || chars[index] == '#' && !output.contains('"') && !output.contains('\'')
                {
                    break;
                }
                output.push(chars[index]);
                index += 1;
            }
            output
        })
        .collect()
}

fn collect_constants(lines: &[String]) -> HashMap<String, String> {
    let mut constants = HashMap::new();
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
        if let Some(value) = parse_static_string(line[equal + 1..].trim().trim_end_matches(';')) {
            if normalize_url_path(&value).is_some() {
                constants.insert(name.to_string(), value);
            }
        }
    }
    constants
}

fn extract_raw_requests(source: &str) -> Vec<RawRequest> {
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
    output
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
    String::from_utf8(masked).expect("masking preserves UTF-8 bytes")
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
    if value.starts_with("http://") || value.starts_with("https://") {
        let authority_end = value[8..].find('/').map(|offset| offset + 8)?;
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

fn find_caller(inventory: &CodeInventory, path: &str, line: u64) -> Option<String> {
    let items = inventory
        .routes
        .iter()
        .chain(&inventory.handlers)
        .chain(&inventory.services)
        .chain(&inventory.repositories)
        .chain(&inventory.functions)
        .chain(&inventory.classes)
        .chain(&inventory.modules)
        .chain(&inventory.unknown)
        .chain(&inventory.files);
    let mut candidates = items
        .filter(|item| {
            item.file_path
                .as_deref()
                .is_some_and(|file| normalize_path(file) == normalize_path(path))
                && item
                    .line
                    .is_some_and(|start| start <= line && item.end_line.unwrap_or(start) >= line)
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|item| {
        (
            (item.end_line.unwrap_or(item.line.unwrap_or(line)) - item.line.unwrap_or(line)),
            item_priority(item),
        )
    });
    candidates.first().map(|item| item.id.clone())
}

fn item_priority(item: &CodeInventoryItem) -> u8 {
    match item.kind.to_ascii_lowercase().as_str() {
        "handler" => 0,
        "function" | "method" => 1,
        "service" | "repository" => 2,
        "class" => 3,
        "route" => 4,
        "file" => 9,
        _ => 5,
    }
}

fn normalize_path(value: &str) -> String {
    value
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_ascii_lowercase()
}

fn request_id(path: &str, line: usize, request: &RawRequest) -> Option<String> {
    if request.url_expression.trim().is_empty() {
        return None;
    }
    let mut digest = Sha256::new();
    digest.update(path.as_bytes());
    digest.update([0]);
    digest.update(line.to_string().as_bytes());
    digest.update([0]);
    digest.update(request.client.as_bytes());
    digest.update([0]);
    digest.update(request.method.as_deref().unwrap_or("ANY").as_bytes());
    digest.update([0]);
    digest.update(request.url_expression.trim().as_bytes());
    let digest = format!("{:x}", digest.finalize());
    Some(format!("client-request:{path}:{line}:{}", &digest[..12]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::model::{CodeInventoryItem, CodeInventorySummary};

    fn inventory(path: &str) -> CodeInventory {
        let item = CodeInventoryItem {
            id: "code:login".to_string(),
            kind: "function".to_string(),
            name: "login".to_string(),
            project: String::new(),
            qualified_name: "login".to_string(),
            engine_label: "Function".to_string(),
            file_path: Some(path.to_string()),
            line: Some(1),
            column: None,
            end_line: Some(20),
            end_column: None,
            detail: serde_json::json!({}),
        };
        CodeInventory {
            project: "test".to_string(),
            routes: Vec::new(),
            services: Vec::new(),
            files: Vec::new(),
            handlers: Vec::new(),
            repositories: Vec::new(),
            functions: vec![item],
            classes: Vec::new(),
            modules: Vec::new(),
            unknown: Vec::new(),
            summary: CodeInventorySummary {
                routes: 0,
                handlers: 0,
                services: 0,
                repositories: 0,
                functions: 1,
                classes: 0,
                modules: 0,
                files: 0,
                unknown: 0,
            },
            architecture: None,
            calls: Vec::new(),
            handles: Vec::new(),
            relation_gaps: Vec::new(),
            client_requests: Vec::new(),
            partial: false,
        }
    }

    #[test]
    fn extracts_static_requests_across_common_language_clients() {
        let root =
            std::env::temp_dir().join(format!("visual-map-client-request-{}", std::process::id()));
        let _ = fs::create_dir_all(&root);
        let source = r#"const API = "/api";
axios.post(API + "/token");
const example = "axios.post('/fake-string')";
const multiline = "axios.get(\n  '/fake-multiline')";
// axios.get("/fake")
requests.get("/health");
RestTemplate.getForObject("/owners", Owner.class);
client.GetAsync("/orders");
http.NewRequest("POST", "/transfers", body);
reqwest::get("/rust");
Http::post("/php");
Faraday.get("/ruby");
http.post("/dart");
"#;
        fs::write(root.join("main.ts"), source).unwrap();
        let inventory = inventory("main.ts");
        let requests = extract_client_requests(root.to_str().unwrap(), &inventory).unwrap();
        assert!(requests
            .iter()
            .any(|item| item.method.as_deref() == Some("POST")
                && item.path.as_deref() == Some("/api/token")));
        assert_eq!(
            requests
                .iter()
                .filter(|item| item.path.as_deref() == Some("/api/token"))
                .count(),
            1
        );
        assert_eq!(
            requests
                .iter()
                .find(|item| item.path.as_deref() == Some("/api/token"))
                .map(|item| item.line),
            Some(2)
        );
        assert!(requests
            .iter()
            .any(|item| item.client == "requests" && item.path.as_deref() == Some("/health")));
        assert!(requests
            .iter()
            .any(|item| item.client == "go-http" && item.path.as_deref() == Some("/transfers")));
        assert!(!requests
            .iter()
            .any(|item| item.path.as_deref() == Some("/fake")));
        assert!(!requests
            .iter()
            .any(|item| item.path.as_deref() == Some("/fake-string")));
        assert!(!requests
            .iter()
            .any(|item| item.path.as_deref() == Some("/fake-multiline")));
        fs::create_dir_all(root.join("tests")).unwrap();
        fs::write(
            root.join("tests/client.test.ts"),
            "axios.post(\"/test-only\")\n",
        )
        .unwrap();
        let requests = extract_client_requests(root.to_str().unwrap(), &inventory).unwrap();
        let test_only = requests
            .iter()
            .find(|item| item.path.as_deref() == Some("/test-only"))
            .expect("test-only request");
        assert_eq!(test_only.resolution, "excluded");
        assert!(test_only
            .evidence
            .iter()
            .any(|evidence| evidence == "excluded:test-only"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn keeps_one_common_request_contract_across_active_language_extensions() {
        let root = std::env::temp_dir().join(format!(
            "visual-map-client-request-languages-{}",
            std::process::id()
        ));
        let _ = fs::create_dir_all(&root);
        let fixtures = [
            (
                "main.c",
                "curl_easy_setopt(curl, CURLOPT_URL, \"/c\"); curl_easy_setopt(curl, CURLOPT_POST, 1L);",
                "POST",
                "/c",
                "candidate",
            ),
            ("main.cpp", "cpr::Get(\"/cpp\");", "GET", "/cpp", "static-confirmed"),
            ("main.cs", "client.GetAsync(\"/csharp\");", "GET", "/csharp", "candidate"),
            ("main.dart", "http.get(\"/dart\");", "GET", "/dart", "candidate"),
            (
                "main.go",
                "http.NewRequest(\"POST\", \"/go\", nil);",
                "POST",
                "/go",
                "static-confirmed",
            ),
            (
                "Main.java",
                "RestTemplate.getForObject(\"/java\", String.class);",
                "GET",
                "/java",
                "static-confirmed",
            ),
            ("main.js", "fetch(\"/javascript\");", "GET", "/javascript", "static-confirmed"),
            ("main.php", "Http::post(\"/php\");", "POST", "/php", "static-confirmed"),
            (
                "main.py",
                "requests.get(\"/python\");",
                "GET",
                "/python",
                "static-confirmed",
            ),
            ("main.rb", "Faraday.post(\"/ruby\");", "POST", "/ruby", "static-confirmed"),
            ("main.rs", "reqwest::get(\"/rust\");", "GET", "/rust", "static-confirmed"),
            (
                "main.ts",
                "axios.post(\"/typescript\");",
                "POST",
                "/typescript",
                "static-confirmed",
            ),
        ];
        for (file, source, _, _, _) in fixtures {
            fs::write(root.join(file), source).unwrap();
        }

        let requests =
            extract_client_requests(root.to_str().unwrap(), &inventory("main.js")).unwrap();
        assert_eq!(requests.len(), fixtures.len());
        for (_, _, method, path, resolution) in fixtures {
            let request = requests
                .iter()
                .find(|request| request.path.as_deref() == Some(path))
                .expect("language fixture request");
            assert_eq!(request.method.as_deref(), Some(method));
            assert_eq!(request.resolution, resolution);
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn leaves_dynamic_urls_unknown_instead_of_guessing() {
        let root = std::env::temp_dir().join(format!(
            "visual-map-client-request-dynamic-{}",
            std::process::id()
        ));
        let _ = fs::create_dir_all(&root);
        fs::write(
            root.join("main.py"),
            "requests.get(os.getenv(\"API_URL\") + path)\n",
        )
        .unwrap();
        let requests =
            extract_client_requests(root.to_str().unwrap(), &inventory("main.py")).unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].resolution, "unknown");
        assert_eq!(requests[0].path, None);
        let _ = fs::remove_dir_all(root);
    }
}
