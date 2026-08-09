use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

use super::discovery::{find_files, read_descriptor, relative_path, stable_segment};
use super::model::{
    CollectedEvidence, CollectedFact, CollectedRelation, CollectionDiagnostic, CollectionMode,
    CollectionStatus, CollectorResult, TruthClass,
};

const ID: &str = "deployment";
const ROOT: &str = "deployment:root";

pub(crate) fn collect(root: &Path) -> CollectorResult {
    let mut result = CollectorResult::new(ID, "deployment-topology", CollectionMode::Passive);
    let files = find_files(root, is_deployment_file);
    if files.is_empty() {
        return result;
    }
    for file in files {
        let path = relative_path(root, &file);
        let source = match read_descriptor(&file) {
            Ok(source) => source,
            Err(message) => {
                result.diagnostics.push(diagnostic(path, message));
                continue;
            }
        };
        let before = result.facts.len();
        parse_file(&path, &source, &mut result);
        if result.facts.len() > before {
            result.summary.detected_by.push(path);
        }
    }
    if result.facts.is_empty() {
        result.summary.status = if result.diagnostics.is_empty() {
            CollectionStatus::NotDetected
        } else {
            CollectionStatus::Failed
        };
        return result;
    }
    result.facts.push(CollectedFact {
        stable_key: ROOT.to_string(),
        kind: "deployment-topology".to_string(),
        name: "Deployment".to_string(),
        path: None,
        properties: BTreeMap::new(),
    });
    result.summary.detected_by.sort();
    result.summary.detected_by.dedup();
    result.summary.status = if result.diagnostics.is_empty() {
        CollectionStatus::Collected
    } else {
        CollectionStatus::Partial
    };
    result
}

fn is_deployment_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name == "dockerfile"
        || name.starts_with("dockerfile.")
        || matches!(
            name.as_str(),
            "compose.yaml"
                | "compose.yml"
                | "docker-compose.yaml"
                | "docker-compose.yml"
                | "chart.yaml"
                | "terraform-plan.json"
                | "tfplan.json"
        )
        || name.ends_with(".tfplan.json")
    {
        return true;
    }
    if !matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("json" | "yaml" | "yml")
    ) {
        return false;
    }
    let normalized = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    ["/k8s/", "/kubernetes/", "/manifests/", "/deploy/"]
        .iter()
        .any(|segment| normalized.contains(segment))
        || [
            "deployment",
            "statefulset",
            "daemonset",
            "service",
            "ingress",
        ]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

fn parse_file(path: &str, source: &str, result: &mut CollectorResult) {
    let name = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name == "dockerfile" || name.starts_with("dockerfile.") {
        parse_dockerfile(path, source, result);
    } else if matches!(
        name.as_str(),
        "compose.yaml" | "compose.yml" | "docker-compose.yaml" | "docker-compose.yml"
    ) {
        parse_compose(path, source, result);
    } else if name == "chart.yaml" {
        parse_chart(path, source, result);
    } else if name == "terraform-plan.json"
        || name == "tfplan.json"
        || name.ends_with(".tfplan.json")
    {
        parse_terraform_plan(path, source, result);
    } else if name.ends_with(".json") {
        parse_kubernetes_json(path, source, result);
    } else {
        parse_kubernetes_yaml(path, source, result);
    }
}

fn parse_dockerfile(path: &str, source: &str, result: &mut CollectorResult) {
    for (index, line) in source.lines().enumerate() {
        let line = line.trim();
        let Some(rest) = line
            .strip_prefix("FROM ")
            .or_else(|| line.strip_prefix("from "))
        else {
            continue;
        };
        let image = rest
            .split_whitespace()
            .find(|value| !value.starts_with("--"))
            .unwrap_or("");
        if image.is_empty() {
            continue;
        }
        emit(
            path,
            index + 1,
            "container-image-reference",
            image,
            &[("format", "dockerfile")],
            result,
        );
    }
}

fn parse_compose(path: &str, source: &str, result: &mut CollectorResult) {
    let mut in_services = false;
    let mut service_indent = None;
    let mut service: Option<(String, Option<String>, usize)> = None;
    for (index, raw) in source.lines().enumerate() {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();
        if indent == 0 {
            if let Some(service) = service.take() {
                emit_compose(path, service, result);
            }
            in_services = trimmed == "services:";
            service_indent = None;
            continue;
        }
        if !in_services {
            continue;
        }
        if trimmed.ends_with(':') && !trimmed.contains(' ') {
            let expected = *service_indent.get_or_insert(indent);
            if indent == expected {
                if let Some(service) = service.take() {
                    emit_compose(path, service, result);
                }
                service = Some((trimmed.trim_end_matches(':').to_string(), None, index + 1));
            }
        } else if let (Some(value), Some((_, image, _))) =
            (trimmed.strip_prefix("image:"), service.as_mut())
        {
            *image = Some(yaml_scalar(value));
        }
    }
    if let Some(service) = service {
        emit_compose(path, service, result);
    }
}

fn emit_compose(
    path: &str,
    (name, image, line): (String, Option<String>, usize),
    result: &mut CollectorResult,
) {
    let mut values = vec![("format", "docker-compose")];
    if let Some(image) = image.as_deref() {
        values.push(("image", image));
    }
    emit(path, line, "deployment-service", &name, &values, result);
}

fn parse_chart(path: &str, source: &str, result: &mut CollectorResult) {
    let Some(name) = yaml_top_scalar(source, "name") else {
        return;
    };
    let version = yaml_top_scalar(source, "version");
    let mut values = vec![("format", "helm")];
    if let Some(version) = version.as_deref() {
        values.push(("version", version));
    }
    emit(path, 1, "helm-chart", &name, &values, result);
}

fn parse_kubernetes_yaml(path: &str, source: &str, result: &mut CollectorResult) {
    for document in source.split("\n---") {
        let Some(kind) = yaml_top_scalar(document, "kind") else {
            continue;
        };
        if !is_supported_kubernetes_boundary(&kind) {
            continue;
        }
        let Some(name) = yaml_nested_scalar(document, "metadata", "name") else {
            continue;
        };
        let namespace = yaml_nested_scalar(document, "metadata", "namespace");
        let mut values = vec![
            ("format", "kubernetes-yaml"),
            ("resource_kind", kind.as_str()),
        ];
        if let Some(namespace) = namespace.as_deref() {
            values.push(("namespace", namespace));
        }
        emit(path, 1, "deployment-resource", &name, &values, result);
    }
}

fn parse_kubernetes_json(path: &str, source: &str, result: &mut CollectorResult) {
    let Ok(value) = serde_json::from_str::<Value>(source) else {
        return;
    };
    if let Some(items) = value.get("items").and_then(Value::as_array) {
        for item in items {
            emit_kubernetes_json(path, item, result);
        }
    } else {
        emit_kubernetes_json(path, &value, result);
    }
}

fn emit_kubernetes_json(path: &str, value: &Value, result: &mut CollectorResult) {
    let Some(kind) = value.get("kind").and_then(Value::as_str) else {
        return;
    };
    if !is_supported_kubernetes_boundary(kind) {
        return;
    }
    let Some(name) = value.pointer("/metadata/name").and_then(Value::as_str) else {
        return;
    };
    emit(
        path,
        1,
        "deployment-resource",
        name,
        &[("format", "kubernetes-json"), ("resource_kind", kind)],
        result,
    );
}

fn parse_terraform_plan(path: &str, source: &str, result: &mut CollectorResult) {
    let Ok(value) = serde_json::from_str::<Value>(source) else {
        return;
    };
    if let Some(module) = value.pointer("/planned_values/root_module") {
        terraform_module(path, module, result);
    }
}

fn terraform_module(path: &str, module: &Value, result: &mut CollectorResult) {
    for resource in module
        .get("resources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(address) = resource.get("address").and_then(Value::as_str) else {
            continue;
        };
        let resource_type = resource.get("type").and_then(Value::as_str).unwrap_or("");
        if !is_supported_terraform_boundary(resource_type) {
            continue;
        }
        emit(
            path,
            1,
            "infrastructure-boundary",
            address,
            &[
                ("format", "terraform-plan"),
                ("resource_type", resource_type),
            ],
            result,
        );
    }
    for child in module
        .get("child_modules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        terraform_module(path, child, result);
    }
}

fn is_supported_kubernetes_boundary(kind: &str) -> bool {
    matches!(
        kind.to_ascii_lowercase().as_str(),
        "deployment"
            | "statefulset"
            | "daemonset"
            | "job"
            | "cronjob"
            | "service"
            | "ingress"
            | "gateway"
    )
}

fn is_supported_terraform_boundary(resource_type: &str) -> bool {
    let resource_type = resource_type.to_ascii_lowercase();
    [
        "_lambda_function",
        "_cloudfunctions_function",
        "_cloudfunctions2_function",
        "_cloud_run_service",
        "_ecs_service",
        "_container_app",
        "_app_service",
        "_web_app",
        "_function_app",
        "_compute_instance",
        "_kubernetes_cluster",
        "_api_gateway",
        "_apigateway",
        "_application_gateway",
        "_load_balancer",
        "_db_instance",
        "_rds_cluster",
        "_sql_database",
        "_postgresql_",
        "_mysql_",
        "_mssql_",
        "_cosmosdb_",
        "_storage_account",
        "_storage_bucket",
        "_s3_bucket",
        "_queue",
        "_topic",
        "_stream",
        "_redis_",
        "_elasticache_",
        "_opensearch_",
        "_servicebus_",
    ]
    .iter()
    .any(|marker| resource_type.contains(marker))
}

fn emit(
    path: &str,
    line: usize,
    kind: &str,
    name: &str,
    values: &[(&str, &str)],
    result: &mut CollectorResult,
) {
    let key = format!(
        "deployment:{}:{}:{}:{}",
        stable_segment(kind),
        stable_segment(path),
        line,
        stable_segment(name)
    );
    result.facts.push(CollectedFact {
        stable_key: key.clone(),
        kind: kind.to_string(),
        name: name.to_string(),
        path: Some(path.to_string()),
        properties: values
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect(),
    });
    result.relations.push(CollectedRelation {
        from: ROOT.to_string(),
        to: key,
        kind: "CONTAINS".to_string(),
        truth_class: TruthClass::Confirmed,
        evidence_type: "DEPLOYMENT_DESCRIPTOR".to_string(),
        evidence: vec![CollectedEvidence {
            path: path.to_string(),
            line: Some(line as u32),
            note: None,
        }],
        properties: BTreeMap::new(),
    });
}

fn yaml_top_scalar(source: &str, key: &str) -> Option<String> {
    source.lines().find_map(|line| {
        if line.starts_with(char::is_whitespace) {
            return None;
        }
        let (left, right) = line.split_once(':')?;
        (left.trim() == key).then(|| yaml_scalar(right))
    })
}

fn yaml_nested_scalar(source: &str, parent: &str, key: &str) -> Option<String> {
    let mut parent_indent = None;
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if parent_indent.is_none() {
            if indent == 0 && trimmed == format!("{parent}:") {
                parent_indent = Some(indent);
            }
            continue;
        }
        if indent == 0 {
            return None;
        }
        let Some((left, right)) = trimmed.split_once(':') else {
            continue;
        };
        if left.trim() == key {
            return Some(yaml_scalar(right));
        }
    }
    None
}

fn yaml_scalar(value: &str) -> String {
    value
        .trim()
        .trim_matches(['\'', '"'])
        .split(" #")
        .next()
        .unwrap_or("")
        .to_string()
}

fn diagnostic(path: String, message: String) -> CollectionDiagnostic {
    CollectionDiagnostic {
        collector: ID,
        level: "warning",
        code: "invalid-deployment-descriptor",
        message,
        path: Some(path),
    }
}

#[cfg(test)]
mod tests {
    use super::collect;

    #[test]
    fn imports_only_execution_and_external_deployment_boundaries() {
        let root =
            std::env::temp_dir().join(format!("code-memory-deployment-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("k8s")).unwrap();
        std::fs::write(
            root.join("compose.yaml"),
            "services:\n  api:\n    image: example/api:1\n  worker:\n    image: example/worker:1\n",
        )
        .unwrap();
        std::fs::write(
            root.join("k8s/deployment.yaml"),
            "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: api\n  namespace: prod\n---\napiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: api-settings\n",
        )
        .unwrap();
        std::fs::write(
            root.join("tfplan.json"),
            r#"{"planned_values":{"root_module":{"resources":[{"address":"aws_s3_bucket.assets","type":"aws_s3_bucket"},{"address":"aws_iam_role.runtime","type":"aws_iam_role"}]}}}"#,
        )
        .unwrap();

        let result = collect(&root);
        assert_eq!(
            result
                .facts
                .iter()
                .filter(|fact| fact.kind == "deployment-service")
                .count(),
            2
        );
        assert!(result
            .facts
            .iter()
            .any(|fact| fact.kind == "deployment-resource" && fact.name == "api"));
        assert!(!result.facts.iter().any(|fact| fact.name == "api-settings"));
        assert!(result.facts.iter().any(|fact| {
            fact.kind == "infrastructure-boundary" && fact.name == "aws_s3_bucket.assets"
        }));
        assert!(!result
            .facts
            .iter()
            .any(|fact| fact.name == "aws_iam_role.runtime"));
        let _ = std::fs::remove_dir_all(root);
    }
}
