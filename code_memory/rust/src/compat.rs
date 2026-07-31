use crate::{index_project, is_excluded_source_dir};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const COMPAT_TOOLS: &[&str] = &[
    "index_repository",
    "delete_project",
    "get_architecture",
    "query_graph",
    "search_code",
    "list_projects",
];

pub(crate) fn run_cli(args: &[String]) -> Result<(), String> {
    let tool = args.first().map(String::as_str).unwrap_or_default();
    if !COMPAT_TOOLS.contains(&tool) {
        return Err(format!("unknown cli tool '{tool}'"));
    }
    let args_file = required_arg(args, "--args-file")?;
    let payload: Value = serde_json::from_slice(
        &fs::read(&args_file).map_err(|e| format!("cannot read {}: {e}", args_file.display()))?,
    )
    .map_err(|e| format!("invalid cli args: {e}"))?;

    match tool {
        "index_repository" => index_repository(&payload),
        "delete_project" => delete_project(&payload),
        "get_architecture" => print_json(&read_architecture(&payload)?),
        "query_graph" => query_graph(&payload),
        "search_code" => search_code(&payload),
        "list_projects" => list_projects(),
        _ => unreachable!(),
    }
}

fn index_repository(payload: &Value) -> Result<(), String> {
    let root = required_string(payload, "repo_path")?;
    let project = payload
        .get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            Path::new(root)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("project")
        })
        .to_string();
    let (index_path, architecture_path) = project_paths(&project)?;
    let pack_root = framework_pack_root();
    let providers_root = optional_resource_root("CODE_MEMORY_PROVIDERS_ROOT", "providers");
    index_project(
        Path::new(root),
        &index_path,
        &architecture_path,
        &pack_root,
        providers_root.as_deref(),
    )?;
    print_json(&json!({ "project": project, "repo_path": root }))
}

fn delete_project(payload: &Value) -> Result<(), String> {
    let project = required_string(payload, "project")?;
    let (index_path, _) = project_paths(project)?;
    if let Some(parent) = index_path.parent() {
        let _ = fs::remove_dir_all(parent);
    }
    print_json(&json!({ "deleted": project }))
}

fn read_index(payload: &Value) -> Result<Value, String> {
    let project = required_string(payload, "project")?;
    let (index_path, _) = project_paths(project)?;
    let mut index: Value = serde_json::from_slice(
        &fs::read(&index_path)
            .map_err(|e| format!("cannot read project index {}: {e}", index_path.display()))?,
    )
    .map_err(|e| format!("invalid project index {}: {e}", index_path.display()))?;
    let (_, architecture_path) = project_paths(project)?;
    let architecture: Value =
        serde_json::from_slice(&fs::read(&architecture_path).map_err(|e| {
            format!(
                "cannot read project architecture {}: {e}",
                architecture_path.display()
            )
        })?)
        .map_err(|e| {
            format!(
                "invalid project architecture {}: {e}",
                architecture_path.display()
            )
        })?;
    if let Some(object) = index.as_object_mut() {
        object.insert(
            "__architecture_nodes".to_string(),
            architecture
                .get("nodes")
                .cloned()
                .unwrap_or_else(|| json!([])),
        );
        object.insert(
            "__architecture_edges".to_string(),
            architecture
                .get("edges")
                .cloned()
                .unwrap_or_else(|| json!([])),
        );
    }
    Ok(index)
}

fn read_architecture(payload: &Value) -> Result<Value, String> {
    let project = required_string(payload, "project")?;
    let (_, architecture_path) = project_paths(project)?;
    serde_json::from_slice(&fs::read(&architecture_path).map_err(|e| {
        format!(
            "cannot read project architecture {}: {e}",
            architecture_path.display()
        )
    })?)
    .map_err(|e| {
        format!(
            "invalid project architecture {}: {e}",
            architecture_path.display()
        )
    })
}

fn query_graph(payload: &Value) -> Result<(), String> {
    let query = required_string(payload, "query")?.to_ascii_uppercase();
    let index = read_index(payload)?;
    let relationship_kind = query_relationship_kind(&query);
    if relationship_kind == Some("HANDLES") {
        let endpoint_aliases = architecture_endpoint_aliases(&index);
        let rows = index
            .get("framework_relations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|relation| relation.get("kind").and_then(Value::as_str) == Some("HANDLES"))
            .map(|relation| {
                let from = relation.get("from").cloned().unwrap_or(Value::Null);
                let to = relation
                    .get("to")
                    .map(|value| normalize_endpoint(value, &endpoint_aliases))
                    .unwrap_or(Value::Null);
                json!([from, to,])
            })
            .collect::<Vec<_>>();
        let total = rows.len();
        return print_json(&json!({
            "columns": ["source", "target"],
            "rows": rows,
            "total": total
        }));
    }
    if relationship_kind == Some("CALLS") {
        let rows = index
            .get("relations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|relation| relation.get("kind").and_then(Value::as_str) == Some("CALLS"))
            .map(|relation| {
                json!([
                    relation.get("from").cloned().unwrap_or(Value::Null),
                    relation.get("to").cloned().unwrap_or(Value::Null),
                    relation.get("confidence").cloned().unwrap_or(Value::Null),
                    relation.get("strategy").cloned().unwrap_or(Value::Null),
                    Value::Null,
                    relation.get("path").cloned().unwrap_or(Value::Null),
                    relation.get("range").cloned().unwrap_or(Value::Null),
                ])
            })
            .collect::<Vec<_>>();
        let total = rows.len();
        return print_json(&json!({
            "columns": ["source", "target", "confidence", "strategy", "call_expression", "path", "range"],
            "rows": rows,
            "total": total
        }));
    }
    if let Some(kind) = relationship_kind {
        let rows = index
            .get("__architecture_edges")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|edge| edge.get("kind").and_then(Value::as_str) == Some(kind))
            .map(|edge| {
                json!([
                    edge.get("from").cloned().unwrap_or(Value::Null),
                    edge.get("to").cloned().unwrap_or(Value::Null),
                    edge.get("kind").cloned().unwrap_or(Value::Null),
                    edge.get("level").cloned().unwrap_or(Value::Null),
                    edge.get("properties").cloned().unwrap_or(Value::Null),
                    edge.get("evidence").cloned().unwrap_or(Value::Null),
                ])
            })
            .collect::<Vec<_>>();
        let total = rows.len();
        return print_json(&json!({
            "columns": ["source", "target", "kind", "level", "properties", "evidence"],
            "rows": rows,
            "total": total
        }));
    }

    let rows = inventory_rows(&index);
    let total = rows.len();
    print_json(&json!({
        "columns": inventory_columns(),
        "rows": rows,
        "total": total
    }))
}

fn query_relationship_kind(query: &str) -> Option<&str> {
    let relationship = query.split_once('[')?.1.split_once(']')?.0;
    let kind = relationship.split_once(':')?.1;
    let kind = kind
        .trim_start()
        .split(|character: char| !character.is_ascii_uppercase() && character != '_')
        .next()?;
    (!kind.is_empty()).then_some(kind)
}

fn architecture_endpoint_aliases(index: &Value) -> HashMap<String, String> {
    let mut aliases = HashMap::new();
    let Some(nodes) = index.get("__architecture_nodes").and_then(Value::as_array) else {
        return aliases;
    };

    for node in nodes {
        let Some(id) = node.get("id").and_then(Value::as_str) else {
            continue;
        };
        if node.get("kind").and_then(Value::as_str) != Some("ENDPOINT") {
            continue;
        }

        aliases.insert(id.to_string(), id.to_string());
        if let Some(suffix) = id.strip_prefix("entrypoint:") {
            aliases.insert(format!("route:{suffix}"), id.to_string());
            if let Some((framework, route_suffix)) = suffix.split_once(":route:") {
                let route_suffix = route_suffix
                    .strip_prefix(&format!("{framework}:"))
                    .unwrap_or(route_suffix);
                aliases.insert(format!("route:{framework}:{route_suffix}"), id.to_string());
            }
        }
    }

    aliases
}

fn normalize_endpoint(value: &Value, aliases: &HashMap<String, String>) -> Value {
    let Some(id) = value.as_str() else {
        return value.clone();
    };
    aliases
        .get(id)
        .cloned()
        .map(Value::String)
        .unwrap_or_else(|| value.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framework_route_alias_is_normalized_to_architecture_endpoint() {
        let index = json!({
            "__architecture_nodes": [
                {
                    "id": "entrypoint:fastapi:route:fastapi:src/routes.py:10:/items",
                    "kind": "ENDPOINT"
                }
            ]
        });
        let aliases = architecture_endpoint_aliases(&index);

        assert_eq!(
            normalize_endpoint(&json!("route:fastapi:src/routes.py:10:/items"), &aliases),
            json!("entrypoint:fastapi:route:fastapi:src/routes.py:10:/items")
        );
    }

    #[test]
    fn unknown_framework_endpoint_is_not_guessed() {
        let aliases = architecture_endpoint_aliases(&json!({
            "__architecture_nodes": []
        }));

        assert_eq!(
            normalize_endpoint(&json!("route:unknown"), &aliases),
            json!("route:unknown")
        );
    }

    #[test]
    fn focused_search_counts_hidden_files_without_substring_false_positives() {
        let files = vec![
            (
                "src/a.java".to_string(),
                "orders.find(); preorders.find(); orders_id = 1;".to_string(),
            ),
            (
                "src/b.java".to_string(),
                "return orders.save();".to_string(),
            ),
        ];
        let (results, matches, total) =
            collect_search_matches(&files, &HashMap::new(), "orders", None, 1);

        assert_eq!(results.len(), 1);
        assert_eq!(matches, 2);
        assert_eq!(total, 2);
    }

    #[test]
    fn generated_path_filter_is_exact_and_escaped() {
        let filter = r"^(src/a\.java|tests/b\.java)$";

        assert!(path_matches_filter("src/a.java", filter));
        assert!(path_matches_filter("tests/b.java", filter));
        assert!(!path_matches_filter("nested/src/a.java", filter));
        assert!(path_matches_filter("src/db/orders.sql", "^src/db/"));
        assert!(!path_matches_filter("src/db/orders.sql", r"src/.*"));
    }

    #[test]
    fn relationship_query_detection_does_not_confuse_node_labels() {
        assert_eq!(
            query_relationship_kind("MATCH (a)-[rel:IMPORTS]->(b) RETURN a, b"),
            Some("IMPORTS")
        );
        assert_eq!(
            query_relationship_kind("MATCH (node:ROUTE|FUNCTION) RETURN node"),
            None
        );
    }

    #[test]
    fn inventory_preserves_provider_enclosing_symbol() {
        let parent = "scip-dotnet nuget . . Contributors/Delete#";
        let method = "scip-dotnet nuget . . Contributors/Delete#Configure().";
        let documents = vec![json!({
            "path": "Contributors/Delete.cs",
            "symbols": [{
                "symbol": method,
                "kind": "Method",
                "enclosing_symbol": parent
            }],
            "occurrences": []
        })];
        let mut rows = Vec::new();
        add_document_symbols(&documents, None, &mut rows, &mut HashSet::new());

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][0][0], "Method");
        assert_eq!(rows[0][1], "Configure");
        assert_eq!(rows[0][10], parent);
    }

    #[test]
    fn inventory_expands_single_line_provider_definition_to_lexical_scope() {
        let root = std::env::temp_dir().join(format!(
            "code-memory-inventory-scope-{}",
            std::process::id()
        ));
        let path = root.join("Contributors/Delete.cs");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            "public class Delete : EndpointWithoutRequest\n{\n  public override void Configure()\n  {\n    Get(\"/items\");\n  }\n}\n",
        )
        .unwrap();
        let documents = vec![json!({
            "path": "Contributors/Delete.cs",
            "symbols": [{
                "symbol": "scip-dotnet nuget . . Contributors/Delete#Configure().",
                "kind": "Method"
            }],
            "occurrences": [{
                "symbol": "scip-dotnet nuget . . Contributors/Delete#Configure().",
                "range": [2, 23, 32],
                "definition": true
            }]
        })];
        let mut rows = Vec::new();
        add_document_symbols(&documents, root.to_str(), &mut rows, &mut HashSet::new());

        assert_eq!(rows[0][4], 3);
        assert_eq!(rows[0][6], 6);
        fs::remove_dir_all(root).unwrap();
    }
}

fn search_code(payload: &Value) -> Result<(), String> {
    let index = read_index(payload)?;
    let root = index
        .get("project_root")
        .and_then(Value::as_str)
        .ok_or("project index has no project_root")?;
    let identifier = extract_identifier(required_string(payload, "pattern")?);
    if identifier.is_empty() {
        return Err("search pattern does not contain an identifier".to_string());
    }
    let path_filter = payload.get("path_filter").and_then(Value::as_str);
    let limit = payload
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(32)
        .clamp(1, 32) as usize;
    let documents = document_symbol_names(&index);
    let (results, total_matches, total_results) = collect_search_matches(
        &load_text_files(Path::new(root)),
        &documents,
        &identifier,
        path_filter,
        limit,
    );
    print_json(&json!({
        "results": results,
        "total_grep_matches": total_matches,
        "total_results": total_results,
        "raw_match_count": 0,
    }))
}

fn collect_search_matches(
    files: &[(String, String)],
    documents: &HashMap<String, Vec<String>>,
    identifier: &str,
    path_filter: Option<&str>,
    limit: usize,
) -> (Vec<Value>, usize, usize) {
    let mut results = Vec::new();
    let mut total_matches = 0usize;
    let mut total_results = 0usize;
    for (path, source) in files {
        if path_filter.is_some_and(|filter| !path_matches_filter(path, filter)) {
            continue;
        }
        let lines = source
            .lines()
            .enumerate()
            .filter_map(|(line, text)| {
                contains_identifier(text, identifier).then_some(line as u64 + 1)
            })
            .collect::<Vec<_>>();
        if lines.is_empty() {
            continue;
        }
        total_matches += lines.len();
        total_results += 1;
        if results.len() >= limit {
            continue;
        }
        let qualified_name = documents
            .get(path)
            .and_then(|names| names.iter().find(|name| name.contains(identifier)))
            .cloned()
            .unwrap_or_else(|| format!("{path}::{identifier}"));
        results.push(json!({
            "qualified_name": qualified_name,
            "label": "File",
            "file": path,
            "start_line": lines[0],
            "end_line": *lines.last().unwrap_or(&lines[0]),
            "match_lines": lines,
        }));
    }
    (results, total_matches, total_results)
}

fn contains_identifier(text: &str, identifier: &str) -> bool {
    text.match_indices(identifier).any(|(start, _)| {
        let end = start + identifier.len();
        let before = text[..start].chars().next_back();
        let after = text[end..].chars().next();
        !before.is_some_and(is_identifier_character) && !after.is_some_and(is_identifier_character)
    })
}

fn is_identifier_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

fn inventory_columns() -> [&'static str; 20] {
    [
        "labels",
        "name",
        "qualified_name",
        "file_path",
        "start_line",
        "start_column",
        "end_line",
        "end_column",
        "method",
        "source",
        "parent_qualified_name",
        "parent_class",
        "module",
        "namespace",
        "package",
        "route_path",
        "route_method",
        "signature",
        "return_type",
        "is_test",
    ]
}

fn inventory_rows(index: &Value) -> Vec<Value> {
    let mut nodes = Vec::new();
    let mut seen = HashSet::new();
    let evidence = entrypoint_evidence(index);
    if let Some(architecture_nodes) = index.get("__architecture_nodes").and_then(Value::as_array) {
        add_architecture_nodes(architecture_nodes, &evidence, &mut nodes, &mut seen);
    }
    if let Some(documents) = index.get("documents").and_then(Value::as_array) {
        add_document_symbols(
            documents,
            index.get("project_root").and_then(Value::as_str),
            &mut nodes,
            &mut seen,
        );
    }
    nodes.sort_by(|left, right| {
        left.get(2)
            .and_then(Value::as_str)
            .cmp(&right.get(2).and_then(Value::as_str))
    });
    nodes
}

fn add_architecture_nodes(
    architecture_nodes: &[Value],
    evidence: &HashMap<String, (String, u64)>,
    rows: &mut Vec<Value>,
    seen: &mut HashSet<String>,
) {
    for node in architecture_nodes {
        let Some(id) = node.get("id").and_then(Value::as_str) else {
            continue;
        };
        if !seen.insert(id.to_string()) {
            continue;
        }
        let kind = node.get("kind").and_then(Value::as_str).unwrap_or("FILE");
        let label = match kind {
            "PROJECT" | "PACKAGE" => "Package",
            "MODULE" => "Module",
            "FILE" => "File",
            "ENDPOINT" => "Route",
            "EXTERNAL_LIBRARY" | "DYNAMIC_BOUNDARY" => "Resource",
            _ => "Resource",
        };
        let (path, line) = evidence
            .get(id)
            .cloned()
            .or_else(|| {
                node.get("path")
                    .and_then(Value::as_str)
                    .map(|path| (path.to_string(), 0))
            })
            .unwrap_or_default();
        let name = node.get("name").and_then(Value::as_str).unwrap_or(id);
        let properties = node.get("properties").cloned().unwrap_or_else(|| json!({}));
        rows.push(inventory_row(
            label,
            name,
            id,
            (!path.is_empty()).then_some(path),
            line,
            properties.get("method").and_then(Value::as_str),
            properties.get("routePath").and_then(Value::as_str),
            properties.get("routeMethod").and_then(Value::as_str),
            properties.get("signature").and_then(Value::as_str),
            properties.get("isTest").and_then(Value::as_bool),
            None,
            None,
        ));
    }
}

fn add_document_symbols(
    documents: &[Value],
    project_root: Option<&str>,
    rows: &mut Vec<Value>,
    seen: &mut HashSet<String>,
) {
    for document in documents {
        let path = document
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let source = project_root
            .filter(|_| !path.is_empty())
            .and_then(|root| fs::read_to_string(Path::new(root).join(path)).ok());
        let source_lines = source
            .as_deref()
            .map(|source| source.lines().collect::<Vec<_>>());
        let symbols = document
            .get("symbols")
            .and_then(Value::as_array)
            .into_iter()
            .flatten();
        let occurrences = document
            .get("occurrences")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|occurrence| {
                occurrence.get("definition").and_then(Value::as_bool) == Some(true)
            })
            .collect::<Vec<_>>();
        for symbol in symbols {
            let Some(id) = symbol.get("symbol").and_then(Value::as_str) else {
                continue;
            };
            if !seen.insert(id.to_string()) {
                continue;
            }
            let occurrence = occurrences
                .iter()
                .find(|occurrence| occurrence.get("symbol").and_then(Value::as_str) == Some(id))
                .copied();
            let provider_range = occurrence
                .and_then(|occurrence| occurrence.get("range"))
                .and_then(Value::as_array)
                .map(|range| {
                    range
                        .iter()
                        .filter_map(Value::as_i64)
                        .map(|value| value.clamp(0, i64::from(i32::MAX)) as i32)
                        .collect::<Vec<_>>()
                });
            let kind = symbol
                .get("kind")
                .and_then(Value::as_str)
                .map(normalize_symbol_kind)
                .unwrap_or("Variable");
            let source_range =
                inventory_symbol_range(provider_range.as_deref(), source_lines.as_deref(), kind);
            let name = symbol
                .get("display_name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| symbol_name(id));
            let signature = symbol.get("signature").and_then(Value::as_str);
            rows.push(inventory_row(
                kind,
                &name,
                id,
                (!path.is_empty()).then_some(path.to_string()),
                0,
                None,
                None,
                None,
                signature,
                None,
                Some(&source_range),
                symbol.get("enclosing_symbol").and_then(Value::as_str),
            ));
        }
    }
}

fn inventory_symbol_range(
    provider_range: Option<&[i32]>,
    source_lines: Option<&[&str]>,
    kind: &str,
) -> Vec<i32> {
    let range = provider_range.unwrap_or_default();
    let provider_spans_lines =
        crate::range_parts(range).is_some_and(|(start, _, end, _)| end > start);
    if provider_spans_lines
        || !matches!(
            kind,
            "Function" | "Method" | "Constructor" | "Class" | "Struct" | "Interface" | "Type"
        )
    {
        return range.to_vec();
    }
    source_lines
        .and_then(|lines| crate::source_scope_from_lines(lines, range))
        .unwrap_or_else(|| range.to_vec())
}

fn inventory_row(
    label: &str,
    name: &str,
    qualified_name: &str,
    file_path: Option<String>,
    line: u64,
    method: Option<&str>,
    route_path: Option<&str>,
    route_method: Option<&str>,
    signature: Option<&str>,
    is_test: Option<bool>,
    source_range: Option<&[i32]>,
    parent_qualified_name: Option<&str>,
) -> Value {
    let source_location = source_range.and_then(crate::range_parts);
    let start_line = source_location
        .map(|(start, _, _, _)| start.max(0) as u64 + 1)
        .unwrap_or(line);
    let start_column = source_location.map(|(_, start, _, _)| start.max(0) as u64);
    let end_line = source_location
        .map(|(_, _, end, _)| end.max(0) as u64 + 1)
        .unwrap_or(line);
    let end_column = source_location.map(|(_, _, _, end)| end.max(0) as u64);
    json!([
        [label],
        name,
        qualified_name,
        file_path,
        (start_line > 0).then_some(start_line),
        start_column,
        (end_line > 0).then_some(end_line),
        end_column,
        method,
        Value::Null,
        parent_qualified_name,
        Value::Null,
        Value::Null,
        Value::Null,
        Value::Null,
        route_path,
        route_method,
        signature,
        Value::Null,
        is_test,
    ])
}

fn entrypoint_evidence(index: &Value) -> HashMap<String, (String, u64)> {
    let mut result = HashMap::new();
    let Some(architecture_nodes) = index.get("__architecture_edges").and_then(Value::as_array)
    else {
        return result;
    };
    for edge in architecture_nodes {
        if edge.get("kind").and_then(Value::as_str) != Some("ENTRYPOINT_TO") {
            continue;
        }
        let Some(from) = edge.get("from").and_then(Value::as_str) else {
            continue;
        };
        let Some(evidence) = edge
            .get("evidence")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
        else {
            continue;
        };
        let path = evidence
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let line = evidence
            .get("range")
            .and_then(Value::as_array)
            .and_then(|range| range.first())
            .and_then(Value::as_i64)
            .map(|line| line.max(0) as u64 + 1)
            .unwrap_or_default();
        result.insert(from.to_string(), (path.to_string(), line));
    }
    result
}

fn document_symbol_names(index: &Value) -> HashMap<String, Vec<String>> {
    let mut result = HashMap::<String, Vec<String>>::new();
    for document in index
        .get("documents")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(path) = document.get("path").and_then(Value::as_str) else {
            continue;
        };
        let names = document
            .get("symbols")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|symbol| symbol.get("symbol").and_then(Value::as_str))
            .map(str::to_string)
            .collect::<Vec<_>>();
        result.insert(path.to_string(), names);
    }
    result
}

fn normalize_symbol_kind(kind: &str) -> &'static str {
    match kind.to_ascii_lowercase().as_str() {
        "function" => "Function",
        "method" => "Method",
        "class" => "Class",
        "struct" => "Struct",
        "interface" => "Interface",
        "trait" => "Trait",
        "enum" => "Enum",
        "constructor" => "Constructor",
        "module" => "Module",
        "namespace" => "Namespace",
        "package" => "Package",
        "field" => "Field",
        "variable" => "Variable",
        "type" => "Type",
        _ => "Variable",
    }
}

fn symbol_name(symbol: &str) -> String {
    if let Ok(parsed) = scip::symbol::parse_symbol(symbol) {
        if let Some(descriptor) = parsed.descriptors.last() {
            return descriptor.name.clone();
        }
    }
    let value = symbol
        .rsplit_once('#')
        .map(|(_, value)| value)
        .unwrap_or(symbol);
    let value = value.split('@').next().unwrap_or(value);
    value
        .rsplit(['.', '/', ':'])
        .find(|part| !part.is_empty())
        .unwrap_or(value)
        .to_string()
}

fn extract_identifier(pattern: &str) -> String {
    let mut value = pattern.trim();
    value = value.strip_prefix("(^|[^A-Za-z0-9_])").unwrap_or(value);
    value = value.strip_suffix("([^A-Za-z0-9_]|$)").unwrap_or(value);
    let mut result = String::new();
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            result.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else {
            result.push(character);
        }
    }
    result
}

fn path_matches_filter(path: &str, filter: &str) -> bool {
    let filter = filter.trim();
    if let Some(exact) = filter
        .strip_prefix("^(")
        .and_then(|value| value.strip_suffix(")$"))
    {
        for part in split_unescaped(exact, '|') {
            let Some(candidate) = regex_literal(part) else {
                return false;
            };
            if candidate == path {
                return true;
            }
        }
        return false;
    }
    let anchored_start = filter.starts_with('^');
    let anchored_end = filter.ends_with('$');
    let value = filter
        .strip_prefix('^')
        .unwrap_or(filter)
        .strip_suffix('$')
        .unwrap_or_else(|| filter.strip_prefix('^').unwrap_or(filter));
    let Some(value) = regex_literal(value) else {
        return false;
    };
    match (anchored_start, anchored_end) {
        (true, true) => path == value,
        (true, false) => path.starts_with(&value),
        (false, true) => path.ends_with(&value),
        (false, false) => path.contains(&value),
    }
}

fn split_unescaped(value: &str, separator: char) -> Vec<&str> {
    let mut output = Vec::new();
    let mut start = 0;
    let mut escaped = false;
    for (index, character) in value.char_indices() {
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == separator {
            output.push(&value[start..index]);
            start = index + character.len_utf8();
        }
    }
    output.push(&value[start..]);
    output
}

fn regex_literal(value: &str) -> Option<String> {
    let mut output = String::new();
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            output.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if matches!(
            character,
            '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '^' | '$'
        ) {
            return None;
        } else {
            output.push(character);
        }
    }
    (!escaped).then_some(output)
}

fn load_text_files(root: &Path) -> Vec<(String, String)> {
    let mut files = Vec::new();
    collect_text_files(root, root, &mut files);
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files
}

fn collect_text_files(root: &Path, directory: &Path, files: &mut Vec<(String, String)>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if !is_excluded_source_dir(&entry.file_name().to_string_lossy()) {
                collect_text_files(root, &path, files);
            }
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.len() > 8 * 1024 * 1024 {
            continue;
        }
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        files.push((relative, source));
    }
}

fn list_projects() -> Result<(), String> {
    let root = env::var_os("CBM_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| env::temp_dir().join("visual-map-code-memory"))
        .join("compat-projects");
    let mut projects = fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().join("language-index.json").is_file())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            let index = fs::read(entry.path().join("language-index.json")).ok()?;
            let value: Value = serde_json::from_slice(&index).ok()?;
            Some(json!({
                "name": name,
                "project_root": value.get("project_root").cloned().unwrap_or(Value::Null)
            }))
        })
        .collect::<Vec<_>>();
    projects.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));
    print_json(&json!({ "projects": projects }))
}

fn project_paths(project: &str) -> Result<(PathBuf, PathBuf), String> {
    let safe = project
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if safe.is_empty() || safe == "." || safe == ".." {
        return Err("invalid project name".to_string());
    }
    let root = env::var_os("CBM_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| env::temp_dir().join("visual-map-code-memory"));
    let directory = root.join("compat-projects").join(safe);
    Ok((
        directory.join("language-index.json"),
        directory.join("architecture.json"),
    ))
}

fn resource_root(variable: &str, name: &str) -> PathBuf {
    optional_resource_root(variable, name)
        .unwrap_or_else(|| env::current_dir().unwrap_or_default().join(name))
}

fn framework_pack_root() -> PathBuf {
    let candidate = resource_root("CODE_MEMORY_PACKS_ROOT", "packs");
    if candidate.join("packs").join("framework").is_dir() {
        candidate
    } else if candidate.join("framework").is_dir() {
        candidate.parent().unwrap_or(&candidate).to_path_buf()
    } else {
        candidate
    }
}

fn optional_resource_root(variable: &str, name: &str) -> Option<PathBuf> {
    env::var_os(variable)
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .or_else(|| {
            env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(|parent| parent.join(name)))
                .filter(|path| path.is_dir())
        })
}

fn required_arg(args: &[String], name: &str) -> Result<PathBuf, String> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing {name} <path>"))
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("missing {key}"))
}

fn print_json(value: &Value) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string(value).map_err(|e| format!("cannot serialize cli response: {e}"))?
    );
    Ok(())
}
