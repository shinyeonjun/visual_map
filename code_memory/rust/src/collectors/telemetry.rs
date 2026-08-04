use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use super::discovery::{find_files, read_descriptor, relative_path, stable_segment};
use super::model::{
    properties, CollectedEvidence, CollectedFact, CollectedRelation, CollectionDiagnostic,
    CollectionMode, CollectionStatus, CollectorResult, TruthClass,
};

const ID: &str = "telemetry";

pub(crate) fn collect(root: &Path) -> CollectorResult {
    let mut result = CollectorResult::new(ID, "runtime-traces", CollectionMode::Passive);
    let files = find_files(root, is_otlp_trace_file);
    if files.is_empty() {
        return result;
    }
    let mut services = HashMap::<String, Service>::new();
    let mut operations = HashMap::<String, Operation>::new();
    let mut edges = HashMap::<(String, String), Edge>::new();
    for file in files {
        let path = relative_path(root, &file);
        let source = match read_descriptor(&file) {
            Ok(source) => source,
            Err(message) => {
                result.diagnostics.push(diagnostic(path, message));
                continue;
            }
        };
        match parse_otlp(&path, &source, &mut services, &mut operations, &mut edges) {
            Ok(true) => result.summary.detected_by.push(path),
            Ok(false) => result.diagnostics.push(diagnostic(
                path,
                "OTLP JSON has no resourceSpans".to_string(),
            )),
            Err(message) => result.diagnostics.push(diagnostic(path, message)),
        }
    }
    if operations.is_empty() {
        result.summary.status = if result.diagnostics.is_empty() {
            CollectionStatus::NotDetected
        } else {
            CollectionStatus::Failed
        };
        return result;
    }

    for service in services.values() {
        result.facts.push(CollectedFact {
            stable_key: service.key.clone(),
            kind: "runtime-service".to_string(),
            name: service.name.clone(),
            path: service.evidence.iter().next().cloned(),
            properties: properties(&[
                ("version", service.version.as_deref()),
                ("environment", service.environment.as_deref()),
            ]),
        });
    }
    for operation in operations.values() {
        result.facts.push(CollectedFact {
            stable_key: operation.key.clone(),
            kind: "runtime-operation".to_string(),
            name: operation.name.clone(),
            path: operation.evidence.iter().next().cloned(),
            properties: properties(&[
                ("service", Some(&operation.service)),
                ("observations", Some(&operation.count.to_string())),
                ("errors", Some(&operation.errors.to_string())),
                (
                    "average_duration_ms",
                    Some(&average_ms(operation.duration_ns, operation.count)),
                ),
            ]),
        });
        result.relations.push(CollectedRelation {
            from: service_key(&operation.service),
            to: operation.key.clone(),
            kind: "CONTAINS".to_string(),
            truth_class: TruthClass::Confirmed,
            evidence_type: "OTLP_TRACE".to_string(),
            evidence: evidences(&operation.evidence),
            properties: BTreeMap::new(),
        });
    }
    for edge in edges.values() {
        result.relations.push(CollectedRelation {
            from: edge.from.clone(),
            to: edge.to.clone(),
            kind: "OBSERVED_CALL".to_string(),
            truth_class: TruthClass::Confirmed,
            evidence_type: "OTLP_TRACE".to_string(),
            evidence: evidences(&edge.evidence),
            properties: properties(&[
                ("observations", Some(&edge.count.to_string())),
                ("errors", Some(&edge.errors.to_string())),
                (
                    "average_duration_ms",
                    Some(&average_ms(edge.duration_ns, edge.count)),
                ),
            ]),
        });
    }
    result.summary.detected_by.sort();
    result.summary.detected_by.dedup();
    result.summary.status = if result.diagnostics.is_empty() {
        CollectionStatus::Collected
    } else {
        CollectionStatus::Partial
    };
    result
}

fn is_otlp_trace_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    name.ends_with(".json")
        && (name.contains("otlp") || name.contains("otel"))
        && (name.contains("trace") || name.ends_with(".otlp.json"))
}

#[derive(Default)]
struct Service {
    key: String,
    name: String,
    version: Option<String>,
    environment: Option<String>,
    evidence: BTreeSet<String>,
}

#[derive(Default)]
struct Operation {
    key: String,
    service: String,
    name: String,
    count: usize,
    errors: usize,
    duration_ns: u128,
    evidence: BTreeSet<String>,
}

#[derive(Default)]
struct Edge {
    from: String,
    to: String,
    count: usize,
    errors: usize,
    duration_ns: u128,
    evidence: BTreeSet<String>,
}

struct Span {
    trace_id: String,
    span_id: String,
    parent_span_id: String,
    operation: String,
    error: bool,
    duration_ns: u128,
}

fn parse_otlp(
    path: &str,
    source: &str,
    services: &mut HashMap<String, Service>,
    operations: &mut HashMap<String, Operation>,
    edges: &mut HashMap<(String, String), Edge>,
) -> Result<bool, String> {
    let value: Value =
        serde_json::from_str(source).map_err(|error| format!("invalid OTLP JSON: {error}"))?;
    let Some(resource_spans) = value.get("resourceSpans").and_then(Value::as_array) else {
        return Ok(false);
    };
    let mut spans = Vec::new();
    for resource_span in resource_spans {
        let attributes = resource_attributes(resource_span.pointer("/resource/attributes"));
        let service = attributes
            .get("service.name")
            .cloned()
            .unwrap_or_else(|| "unknown-service".to_string());
        let service_entry = services.entry(service.clone()).or_default();
        service_entry.key = service_key(&service);
        service_entry.name = service.clone();
        service_entry.version = service_entry
            .version
            .take()
            .or_else(|| attributes.get("service.version").cloned());
        service_entry.environment = service_entry.environment.take().or_else(|| {
            attributes
                .get("deployment.environment.name")
                .or_else(|| attributes.get("deployment.environment"))
                .cloned()
        });
        service_entry.evidence.insert(path.to_string());
        for scope in resource_span
            .get("scopeSpans")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            for span in scope
                .get("spans")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let name = span.get("name").and_then(Value::as_str).unwrap_or("");
                let span_id = span.get("spanId").and_then(Value::as_str).unwrap_or("");
                if name.is_empty() || span_id.is_empty() {
                    continue;
                }
                let operation_key = operation_key(&service, name);
                let duration_ns = span_duration_ns(span);
                let error = span_is_error(span);
                let operation = operations.entry(operation_key.clone()).or_default();
                operation.key = operation_key.clone();
                operation.service = service.clone();
                operation.name = name.to_string();
                operation.count += 1;
                operation.errors += usize::from(error);
                operation.duration_ns = operation.duration_ns.saturating_add(duration_ns);
                operation.evidence.insert(path.to_string());
                spans.push(Span {
                    trace_id: span
                        .get("traceId")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    span_id: span_id.to_string(),
                    parent_span_id: span
                        .get("parentSpanId")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    operation: operation_key,
                    error,
                    duration_ns,
                });
            }
        }
    }
    let parents: HashMap<(&str, &str), &str> = spans
        .iter()
        .filter(|span| !span.trace_id.is_empty())
        .map(|span| {
            (
                (span.trace_id.as_str(), span.span_id.as_str()),
                span.operation.as_str(),
            )
        })
        .collect();
    for span in &spans {
        if span.trace_id.is_empty() || span.parent_span_id.is_empty() {
            continue;
        }
        let Some(parent) = parents.get(&(span.trace_id.as_str(), span.parent_span_id.as_str()))
        else {
            continue;
        };
        let key = ((*parent).to_string(), span.operation.clone());
        let edge = edges.entry(key.clone()).or_default();
        edge.from = key.0;
        edge.to = key.1;
        edge.count += 1;
        edge.errors += usize::from(span.error);
        edge.duration_ns = edge.duration_ns.saturating_add(span.duration_ns);
        edge.evidence.insert(path.to_string());
    }
    Ok(true)
}

fn resource_attributes(value: Option<&Value>) -> HashMap<String, String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|attribute| {
            let key = attribute.get("key")?.as_str()?;
            let value = attribute.get("value")?;
            let scalar = ["stringValue", "intValue", "doubleValue", "boolValue"]
                .iter()
                .find_map(|field| value.get(*field))?;
            let scalar = scalar
                .as_str()
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| scalar.to_string());
            Some((key.to_string(), scalar))
        })
        .collect()
}

fn span_duration_ns(span: &Value) -> u128 {
    let parse = |field: &str| {
        span.get(field).and_then(|value| {
            value
                .as_str()
                .and_then(|value| value.parse::<u128>().ok())
                .or_else(|| value.as_u64().map(u128::from))
        })
    };
    parse("endTimeUnixNano")
        .zip(parse("startTimeUnixNano"))
        .map(|(end, start)| end.saturating_sub(start))
        .unwrap_or(0)
}

fn span_is_error(span: &Value) -> bool {
    span.pointer("/status/code").is_some_and(|code| {
        code.as_u64() == Some(2)
            || code.as_str().is_some_and(|code| {
                code.eq_ignore_ascii_case("STATUS_CODE_ERROR") || code.eq_ignore_ascii_case("ERROR")
            })
    })
}

fn service_key(service: &str) -> String {
    format!("runtime-service:{}", stable_segment(service))
}

fn operation_key(service: &str, operation: &str) -> String {
    format!(
        "runtime-operation:{}:{}",
        stable_segment(service),
        stable_segment(operation)
    )
}

fn average_ms(duration_ns: u128, count: usize) -> String {
    if count == 0 {
        return "0".to_string();
    }
    format!("{:.3}", duration_ns as f64 / count as f64 / 1_000_000.0)
}

fn evidences(paths: &BTreeSet<String>) -> Vec<CollectedEvidence> {
    paths
        .iter()
        .map(|path| CollectedEvidence {
            path: path.clone(),
            line: None,
            note: None,
        })
        .collect()
}

fn diagnostic(path: String, message: String) -> CollectionDiagnostic {
    CollectionDiagnostic {
        collector: ID,
        level: "warning",
        code: "invalid-otlp-trace",
        message,
        path: Some(path),
    }
}

#[cfg(test)]
mod tests {
    use super::collect;

    #[test]
    fn aggregates_otlp_spans_into_operations_and_edges() {
        let root = std::env::temp_dir().join(format!("code-memory-otlp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let trace = serde_json::json!({
            "resourceSpans": [{
                "resource": {"attributes": [{"key": "service.name", "value": {"stringValue": "api"}}]},
                "scopeSpans": [{"spans": [
                    {"traceId": "t1", "spanId": "a1", "name": "POST /orders", "startTimeUnixNano": "0", "endTimeUnixNano": "2000000"},
                    {"traceId": "t1", "spanId": "b1", "parentSpanId": "a1", "name": "db.insert", "startTimeUnixNano": "0", "endTimeUnixNano": "1000000"},
                    {"traceId": "t2", "spanId": "a2", "name": "POST /orders", "startTimeUnixNano": "0", "endTimeUnixNano": "4000000"},
                    {"traceId": "t2", "spanId": "b2", "parentSpanId": "a2", "name": "db.insert", "startTimeUnixNano": "0", "endTimeUnixNano": "2000000", "status": {"code": 2}}
                ]}]
            }]
        });
        std::fs::write(
            root.join("traces.otlp.json"),
            serde_json::to_vec(&trace).unwrap(),
        )
        .unwrap();

        let result = collect(&root);
        assert_eq!(
            result
                .facts
                .iter()
                .filter(|fact| fact.kind == "runtime-operation")
                .count(),
            2
        );
        let edge = result
            .relations
            .iter()
            .find(|relation| relation.kind == "OBSERVED_CALL")
            .unwrap();
        assert_eq!(
            edge.properties.get("observations").map(String::as_str),
            Some("2")
        );
        assert_eq!(edge.properties.get("errors").map(String::as_str), Some("1"));
        let _ = std::fs::remove_dir_all(root);
    }
}
