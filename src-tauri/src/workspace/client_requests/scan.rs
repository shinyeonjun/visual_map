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

#[derive(Debug, Default)]
pub(crate) struct ClientRequestScan {
    pub requests: Vec<ClientRequest>,
    pub skipped_files: usize,
    pub skipped_bytes: u64,
    pub truncated: bool,
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
) -> Result<ClientRequestScan, String> {
    // ponytail: bounded source scan keeps this Phase 1 extractor predictable; move to provider
    // AST/request facts when a repository exceeds these limits or needs runtime resolution.
    let root = Path::new(repo_path)
        .canonicalize()
        .map_err(|error| format!("코드 요청 분석 루트를 확인하지 못했습니다: {error}"))?;
    if !root.is_dir() {
        return Err(format!(
            "코드 요청 분석 루트가 디렉터리가 아닙니다: {repo_path}"
        ));
    }
    let mut files = Vec::new();
    let mut file_cap_reached = false;
    let mut skipped_files = 0;
    collect_source_files(
        &root,
        &mut files,
        0,
        &mut file_cap_reached,
        &mut skipped_files,
    )?;
    files.sort();

    let mut total_bytes = 0u64;
    let mut skipped_bytes = 0u64;
    let mut requests = Vec::new();
    let mut seen = HashSet::new();
    for file in files {
        let metadata = match fs::metadata(&file) {
            Ok(metadata) => metadata,
            Err(_) => {
                skipped_files += 1;
                continue;
            }
        };
        if metadata.len() > MAX_FILE_BYTES
            || total_bytes.saturating_add(metadata.len()) > MAX_TOTAL_BYTES
        {
            skipped_files += 1;
            skipped_bytes = skipped_bytes.saturating_add(metadata.len());
            continue;
        }
        total_bytes = total_bytes.saturating_add(metadata.len());
        let Ok(source) = fs::read_to_string(&file) else {
            skipped_files += 1;
            continue;
        };
        let relative = file
            .strip_prefix(&root)
            .ok()
            .and_then(|path| path.to_str())
            .unwrap_or_default()
            .replace('\\', "/");
        if relative.is_empty() {
            continue;
        }
        let clean_lines = strip_comments(&source);
        let constants = collect_constants(&clean_lines);
        let instance_clients = collect_http_instances(&clean_lines.join("\n"), &constants);
        for (index, _) in clean_lines.iter().enumerate() {
            let window = clean_lines[index..clean_lines.len().min(index + 4)].join("\n");
            for raw in extract_raw_requests(&window, &instance_clients) {
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
    Ok(ClientRequestScan {
        requests,
        skipped_files,
        skipped_bytes,
        truncated: file_cap_reached || skipped_files > 0,
    })
}

fn collect_source_files(
    path: &Path,
    output: &mut Vec<PathBuf>,
    depth: usize,
    file_cap_reached: &mut bool,
    skipped_files: &mut usize,
) -> Result<(), String> {
    if depth > 64 {
        return Ok(());
    }
    if output.len() >= MAX_FILES {
        *file_cap_reached = true;
        return Ok(());
    }
    let entries = fs::read_dir(path)
        .map_err(|error| format!("코드 요청 분석 디렉터리를 읽지 못했습니다: {error}"))?;
    let mut entries = entries.flatten().collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if output.len() >= MAX_FILES {
            *file_cap_reached = true;
            break;
        }
        let entry_path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            *skipped_files += 1;
            continue;
        };
        if file_type.is_symlink() {
            *skipped_files += 1;
            continue;
        }
        // Junctions/reparse points are not always reported as symlinks on
        // Windows. Canonicalizing before recursion keeps the scan inside the
        // selected repository in both cases.
        let Ok(canonical_entry) = entry_path.canonicalize() else {
            *skipped_files += 1;
            continue;
        };
        if !canonical_entry.starts_with(path) {
            *skipped_files += 1;
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if file_type.is_dir() {
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
            collect_source_files(
                &canonical_entry,
                output,
                depth + 1,
                file_cap_reached,
                skipped_files,
            )?;
        } else if is_source_file(&canonical_entry) {
            output.push(canonical_entry);
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
