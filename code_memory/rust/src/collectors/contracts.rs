use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

use super::discovery::{find_files, read_descriptor, relative_path, stable_segment};
use super::model::{
    properties, CollectedEvidence, CollectedFact, CollectedRelation, CollectionDiagnostic,
    CollectionMode, CollectionStatus, CollectorResult, TruthClass,
};

const ID: &str = "contracts";
const HTTP_METHODS: &[&str] = &[
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

pub(crate) fn collect(root: &Path) -> CollectorResult {
    let mut result = CollectorResult::new(ID, "api-contracts", CollectionMode::Passive);
    let files = find_files(root, is_contract_file);
    if files.is_empty() {
        return result;
    }

    for file in files {
        let path = relative_path(root, &file);
        result.summary.detected_by.push(path.clone());
        let source = match read_descriptor(&file) {
            Ok(source) => source,
            Err(message) => {
                result.diagnostics.push(diagnostic(path, message));
                continue;
            }
        };
        let before = result.facts.len();
        result.facts.push(document_fact(&path));
        let parsed = match file
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref()
        {
            Some("proto") => parse_proto(&path, &source, &mut result),
            Some("graphql" | "gql") => parse_graphql(&path, &source, &mut result),
            Some("json") => parse_json_contract(&path, &source, &mut result),
            Some("yaml" | "yml") => parse_yaml_contract(&path, &source, &mut result),
            _ => false,
        };
        if !parsed {
            result.facts.truncate(before);
            result.diagnostics.push(diagnostic(
                path,
                "contract format was detected by name but no supported declaration was found"
                    .to_string(),
            ));
        }
    }
    result.summary.detected_by.sort();
    result.summary.detected_by.dedup();
    result.summary.status = if result.facts.is_empty() {
        CollectionStatus::Failed
    } else if result.diagnostics.is_empty() {
        CollectionStatus::Collected
    } else {
        CollectionStatus::Partial
    };
    result
}

fn is_contract_file(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase());
    if matches!(extension.as_deref(), Some("proto" | "graphql" | "gql")) {
        return true;
    }
    if !matches!(extension.as_deref(), Some("json" | "yaml" | "yml")) {
        return false;
    }
    let name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    name.contains("openapi")
        || name.contains("swagger")
        || name.contains("asyncapi")
        || matches!(name.as_str(), "api" | "schema")
}

fn parse_json_contract(path: &str, source: &str, result: &mut CollectorResult) -> bool {
    let value: Value = match serde_json::from_str(source) {
        Ok(value) => value,
        Err(error) => {
            result.diagnostics.push(diagnostic(
                path.to_string(),
                format!("invalid contract JSON: {error}"),
            ));
            return false;
        }
    };
    if value.get("openapi").is_some() || value.get("swagger").is_some() {
        parse_openapi_json(path, &value, result);
        return true;
    }
    if value.get("asyncapi").is_some() {
        parse_asyncapi_json(path, &value, result);
        return true;
    }
    false
}

fn parse_openapi_json(path: &str, value: &Value, result: &mut CollectorResult) {
    let document = document_key(path);
    let Some(paths) = value.get("paths").and_then(Value::as_object) else {
        return;
    };
    for (route, item) in paths {
        let Some(item) = item.as_object() else {
            continue;
        };
        for method in HTTP_METHODS {
            let Some(operation) = item.get(*method).and_then(Value::as_object) else {
                continue;
            };
            let operation_id = operation.get("operationId").and_then(Value::as_str);
            let key = endpoint_key(path, method, route);
            result.facts.push(CollectedFact {
                stable_key: key.clone(),
                kind: "http-endpoint".to_string(),
                name: operation_id.unwrap_or(route).to_string(),
                path: Some(path.to_string()),
                properties: properties(&[
                    ("format", Some("openapi")),
                    ("method", Some(&method.to_ascii_uppercase())),
                    ("route", Some(route)),
                    ("operation_id", operation_id),
                ]),
            });
            result.relations.push(declares(&document, &key, path, None));
        }
    }
}

fn parse_asyncapi_json(path: &str, value: &Value, result: &mut CollectorResult) {
    let document = document_key(path);
    if let Some(channels) = value.get("channels").and_then(Value::as_object) {
        for (channel_id, channel) in channels {
            let address = channel
                .get("address")
                .and_then(Value::as_str)
                .unwrap_or(channel_id);
            let key = channel_key(path, channel_id);
            result.facts.push(CollectedFact {
                stable_key: key.clone(),
                kind: "message-channel".to_string(),
                name: channel_id.clone(),
                path: Some(path.to_string()),
                properties: properties(&[("format", Some("asyncapi")), ("address", Some(address))]),
            });
            result.relations.push(declares(&document, &key, path, None));
        }
    }
    if let Some(operations) = value.get("operations").and_then(Value::as_object) {
        for (operation_id, operation) in operations {
            let action = operation.get("action").and_then(Value::as_str);
            let key = format!(
                "contract:asyncapi:operation:{}:{}",
                stable_segment(path),
                stable_segment(operation_id)
            );
            result.facts.push(CollectedFact {
                stable_key: key.clone(),
                kind: "message-operation".to_string(),
                name: operation_id.clone(),
                path: Some(path.to_string()),
                properties: properties(&[("format", Some("asyncapi")), ("action", action)]),
            });
            result.relations.push(declares(&document, &key, path, None));
            let channel = operation
                .get("channel")
                .and_then(|channel| channel.get("$ref"))
                .and_then(Value::as_str)
                .and_then(|reference| reference.rsplit('/').next());
            if let Some(channel) = channel {
                result.relations.push(CollectedRelation {
                    from: key,
                    to: channel_key(path, channel),
                    kind: "USES_CHANNEL".to_string(),
                    truth_class: TruthClass::Confirmed,
                    evidence_type: "CONTRACT_DECLARATION".to_string(),
                    evidence: vec![evidence(path, None)],
                    properties: BTreeMap::new(),
                });
            }
        }
    }
}

fn parse_yaml_contract(path: &str, source: &str, result: &mut CollectorResult) -> bool {
    let is_openapi = source.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with("openapi:") || line.starts_with("swagger:")
    });
    let is_asyncapi = source
        .lines()
        .any(|line| line.trim_start().starts_with("asyncapi:"));
    if is_openapi {
        parse_openapi_yaml(path, source, result);
    } else if is_asyncapi {
        parse_asyncapi_yaml(path, source, result);
    }
    is_openapi || is_asyncapi
}

fn parse_openapi_yaml(path: &str, source: &str, result: &mut CollectorResult) {
    let document = document_key(path);
    let mut paths_indent = None;
    let mut current_route: Option<(String, usize)> = None;
    for (index, raw) in source.lines().enumerate() {
        let indent = raw.len() - raw.trim_start().len();
        let line = raw.trim();
        if line == "paths:" {
            paths_indent = Some(indent);
            current_route = None;
            continue;
        }
        let Some(base_indent) = paths_indent else {
            continue;
        };
        if !line.is_empty() && !line.starts_with('#') && indent <= base_indent {
            paths_indent = None;
            current_route = None;
            continue;
        }
        if indent > base_indent && line.starts_with('/') && line.ends_with(':') {
            current_route = Some((line.trim_end_matches(':').to_string(), indent));
            continue;
        }
        let Some((route, route_indent)) = &current_route else {
            continue;
        };
        let method = line.trim_end_matches(':').to_ascii_lowercase();
        if indent > *route_indent && HTTP_METHODS.contains(&method.as_str()) {
            let key = endpoint_key(path, &method, route);
            result.facts.push(CollectedFact {
                stable_key: key.clone(),
                kind: "http-endpoint".to_string(),
                name: route.clone(),
                path: Some(path.to_string()),
                properties: properties(&[
                    ("format", Some("openapi")),
                    ("method", Some(&method.to_ascii_uppercase())),
                    ("route", Some(route)),
                ]),
            });
            result
                .relations
                .push(declares(&document, &key, path, Some(index as u32 + 1)));
        }
    }
}

fn parse_asyncapi_yaml(path: &str, source: &str, result: &mut CollectorResult) {
    let document = document_key(path);
    let mut channels_indent = None;
    for (index, raw) in source.lines().enumerate() {
        let indent = raw.len() - raw.trim_start().len();
        let line = raw.trim();
        if line == "channels:" {
            channels_indent = Some(indent);
            continue;
        }
        let Some(base_indent) = channels_indent else {
            continue;
        };
        if !line.is_empty() && !line.starts_with('#') && indent <= base_indent {
            channels_indent = None;
            continue;
        }
        if indent > base_indent && line.ends_with(':') && !line.starts_with('$') {
            let channel = line.trim_end_matches(':');
            let key = channel_key(path, channel);
            result.facts.push(CollectedFact {
                stable_key: key.clone(),
                kind: "message-channel".to_string(),
                name: channel.to_string(),
                path: Some(path.to_string()),
                properties: properties(&[("format", Some("asyncapi"))]),
            });
            result
                .relations
                .push(declares(&document, &key, path, Some(index as u32 + 1)));
        }
    }
}

fn parse_proto(path: &str, source: &str, result: &mut CollectorResult) -> bool {
    let document = document_key(path);
    let package = source.lines().find_map(|line| {
        line.trim()
            .strip_prefix("package ")
            .map(|value| value.trim_end_matches(';').trim().to_string())
    });
    let mut current_service: Option<String> = None;
    let mut found = false;
    for (index, raw) in source.lines().enumerate() {
        let line = raw.trim();
        if let Some(name) = declaration_name(line, "service") {
            found = true;
            let key = format!(
                "contract:proto:service:{}:{}",
                stable_segment(path),
                stable_segment(&name)
            );
            result.facts.push(CollectedFact {
                stable_key: key.clone(),
                kind: "rpc-service".to_string(),
                name: name.clone(),
                path: Some(path.to_string()),
                properties: properties(&[
                    ("format", Some("protobuf")),
                    ("package", package.as_deref()),
                ]),
            });
            result
                .relations
                .push(declares(&document, &key, path, Some(index as u32 + 1)));
            current_service = Some(name);
            continue;
        }
        if let Some(name) = declaration_name(line, "message") {
            found = true;
            let key = format!(
                "contract:proto:message:{}:{}",
                stable_segment(path),
                stable_segment(&name)
            );
            result.facts.push(CollectedFact {
                stable_key: key.clone(),
                kind: "message-schema".to_string(),
                name,
                path: Some(path.to_string()),
                properties: properties(&[
                    ("format", Some("protobuf")),
                    ("package", package.as_deref()),
                ]),
            });
            result
                .relations
                .push(declares(&document, &key, path, Some(index as u32 + 1)));
            continue;
        }
        if let Some(service) = &current_service {
            if let Some(name) = declaration_name(line, "rpc") {
                found = true;
                let service_key = format!(
                    "contract:proto:service:{}:{}",
                    stable_segment(path),
                    stable_segment(service)
                );
                let key = format!(
                    "contract:proto:rpc:{}:{}:{}",
                    stable_segment(path),
                    stable_segment(service),
                    stable_segment(&name)
                );
                result.facts.push(CollectedFact {
                    stable_key: key.clone(),
                    kind: "rpc-endpoint".to_string(),
                    name,
                    path: Some(path.to_string()),
                    properties: properties(&[("format", Some("protobuf"))]),
                });
                result.relations.push(CollectedRelation {
                    from: service_key,
                    to: key,
                    kind: "DECLARES".to_string(),
                    truth_class: TruthClass::Confirmed,
                    evidence_type: "CONTRACT_DECLARATION".to_string(),
                    evidence: vec![evidence(path, Some(index as u32 + 1))],
                    properties: BTreeMap::new(),
                });
            }
            if line.starts_with('}') {
                current_service = None;
            }
        }
    }
    found
}

fn parse_graphql(path: &str, source: &str, result: &mut CollectorResult) -> bool {
    let document = document_key(path);
    let mut root_type: Option<String> = None;
    let mut found = false;
    for (index, raw) in source.lines().enumerate() {
        let line = raw.trim();
        if let Some(name) = declaration_name(line, "type") {
            root_type =
                matches!(name.as_str(), "Query" | "Mutation" | "Subscription").then_some(name);
            continue;
        }
        let Some(root_kind) = &root_type else {
            continue;
        };
        if line.starts_with('}') {
            root_type = None;
            continue;
        }
        let field = line
            .split(['(', ':', ' ', '\t'])
            .next()
            .filter(|field| !field.is_empty() && !field.starts_with('#'));
        let Some(field) = field else {
            continue;
        };
        found = true;
        let key = format!(
            "contract:graphql:operation:{}:{}:{}",
            stable_segment(path),
            root_kind.to_ascii_lowercase(),
            stable_segment(field)
        );
        result.facts.push(CollectedFact {
            stable_key: key.clone(),
            kind: "graphql-operation".to_string(),
            name: field.to_string(),
            path: Some(path.to_string()),
            properties: properties(&[
                ("format", Some("graphql")),
                ("operation_type", Some(&root_kind.to_ascii_lowercase())),
            ]),
        });
        result
            .relations
            .push(declares(&document, &key, path, Some(index as u32 + 1)));
    }
    found
}

fn declaration_name(line: &str, keyword: &str) -> Option<String> {
    line.strip_prefix(keyword)?
        .trim_start()
        .split(|character: char| character.is_whitespace() || matches!(character, '{' | '('))
        .next()
        .map(str::to_string)
        .filter(|value| !value.is_empty())
}

fn document_fact(path: &str) -> CollectedFact {
    CollectedFact {
        stable_key: document_key(path),
        kind: "contract-document".to_string(),
        name: Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path)
            .to_string(),
        path: Some(path.to_string()),
        properties: BTreeMap::new(),
    }
}

fn document_key(path: &str) -> String {
    format!("contract:document:{}", stable_segment(path))
}

fn endpoint_key(path: &str, method: &str, route: &str) -> String {
    format!(
        "contract:http:endpoint:{}:{}:{}",
        stable_segment(path),
        method.to_ascii_lowercase(),
        stable_segment(route)
    )
}

fn channel_key(path: &str, channel: &str) -> String {
    format!(
        "contract:asyncapi:channel:{}:{}",
        stable_segment(path),
        stable_segment(channel)
    )
}

fn declares(document: &str, target: &str, path: &str, line: Option<u32>) -> CollectedRelation {
    CollectedRelation {
        from: document.to_string(),
        to: target.to_string(),
        kind: "DECLARES".to_string(),
        truth_class: TruthClass::Confirmed,
        evidence_type: "CONTRACT_DECLARATION".to_string(),
        evidence: vec![evidence(path, line)],
        properties: BTreeMap::new(),
    }
}

fn evidence(path: &str, line: Option<u32>) -> CollectedEvidence {
    CollectedEvidence {
        path: path.to_string(),
        line,
        note: None,
    }
}

fn diagnostic(path: String, message: String) -> CollectionDiagnostic {
    CollectionDiagnostic {
        collector: ID,
        level: "warning",
        code: "invalid-contract",
        message,
        path: Some(path),
    }
}

#[cfg(test)]
mod tests {
    use super::collect;

    #[test]
    fn openapi_and_proto_declarations_become_contract_facts() {
        let root = std::env::temp_dir().join(format!(
            "code-memory-contract-collector-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("openapi.json"),
            r#"{"openapi":"3.1.0","paths":{"/users":{"get":{"operationId":"listUsers"}}}}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("users.proto"),
            "syntax = \"proto3\";\nservice Users {\n rpc GetUser (Request) returns (Reply);\n}\nmessage Request {}\n",
        )
        .unwrap();

        let result = collect(&root);
        assert!(result
            .facts
            .iter()
            .any(|fact| fact.kind == "http-endpoint" && fact.name == "listUsers"));
        assert!(result
            .facts
            .iter()
            .any(|fact| fact.kind == "rpc-endpoint" && fact.name == "GetUser"));
        let _ = std::fs::remove_dir_all(root);
    }
}
