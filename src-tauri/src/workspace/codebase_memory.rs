use crate::{engine, EngineRegistry};
use serde_json::{json, Map, Value};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use super::model::{FocusedCodeSearch, FocusedCodeSearchMatch, FocusedCodeSearchTotals};
use super::store::engine_json_value;

const MAX_CODE_NODES: usize = 100_000;
const MAX_GRAPH_RELATIONSHIPS: usize = 100_000;
const MAX_FOCUSED_SEARCH_LIMIT: usize = 32;
const MAX_FOCUSED_SEARCH_TERM_BYTES: usize = 512;
const MAX_FOCUSED_PATH_FILTER_BYTES: usize = 512;
const SEARCH_CODE_GREP_LIMIT: usize = 500;
static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

// `Union` is an upstream Cypher keyword; supporting it would force an unbounded all-node scan.
pub(crate) const CODE_NODE_LABELS: &[&str] = &[
    "Function",
    "Method",
    "Class",
    "Struct",
    "Interface",
    "Trait",
    "Protocol",
    "Record",
    "Enum",
    "Type",
    "Constructor",
    "Subroutine",
    "Procedure",
    "Decorator",
    "Field",
    "Variable",
    "Module",
    "Namespace",
    "Package",
    "Resource",
];
pub(crate) const CALLS_QUERY: &str = "MATCH (caller)-[rel:CALLS]->(callee) RETURN caller.qualified_name AS source, callee.qualified_name AS target, rel.confidence AS confidence, rel.strategy AS strategy, rel.callee AS call_expression LIMIT 100000";
pub(crate) const HANDLES_QUERY: &str = "MATCH (handler)-[:HANDLES]->(route) RETURN handler.qualified_name AS source, route.qualified_name AS target LIMIT 100000";

#[derive(Debug)]
pub(crate) struct CodebaseMemoryInventory {
    pub architecture: Value,
    pub nodes: Value,
    pub calls: Value,
    pub handles: Value,
}

pub(crate) struct CodebaseMemoryAdapter<'a> {
    engine: &'a engine::EngineAvailability,
    cache_dir: PathBuf,
}

impl<'a> CodebaseMemoryAdapter<'a> {
    pub(crate) fn new(
        registry: &'a EngineRegistry,
        cache_dir: impl Into<PathBuf>,
    ) -> Result<Self, String> {
        let engine = registry
            .engines
            .iter()
            .find(|engine| engine.id == "codebase-memory")
            .ok_or_else(|| "코드 읽기 도구가 등록되지 않았습니다".to_string())?;

        Ok(Self {
            engine,
            cache_dir: cache_dir.into(),
        })
    }

    pub(crate) fn index_repository(
        &self,
        repo_path: &str,
        project_name: &str,
    ) -> Result<engine::EngineRunResult, String> {
        let payload = index_payload(repo_path, project_name);
        self.invoke(
            CodebaseMemoryTool::IndexRepository,
            &payload,
            Duration::from_secs(300),
            Some(repo_path),
        )
    }

    pub(crate) fn inventory(&self, project: &str) -> Result<CodebaseMemoryInventory, String> {
        let architecture = self.invoke_json(
            CodebaseMemoryTool::GetArchitecture,
            &json!({ "project": project }),
            Duration::from_secs(60),
        )?;
        let nodes =
            normalize_inventory_nodes(&self.query_graph(project, &inventory_nodes_query())?)?;
        ensure_result_below_limit(&nodes, "code nodes", MAX_CODE_NODES)?;

        let calls = self.query_graph(project, CALLS_QUERY)?;
        let handles = self.query_graph(project, HANDLES_QUERY)?;
        ensure_result_below_limit(&calls, "CALLS", MAX_GRAPH_RELATIONSHIPS)?;
        ensure_result_below_limit(&handles, "HANDLES", MAX_GRAPH_RELATIONSHIPS)?;

        Ok(CodebaseMemoryInventory {
            architecture,
            nodes,
            calls,
            handles,
        })
    }

    pub(crate) fn search_code(
        &self,
        project: &str,
        identifier: &str,
        path_filter: Option<&str>,
        requested_limit: usize,
    ) -> Result<FocusedCodeSearch, String> {
        let payload =
            focused_code_search_payload(project, identifier, path_filter, requested_limit)?;
        let run = self.invoke(
            CodebaseMemoryTool::SearchCode,
            &payload,
            Duration::from_secs(60),
            None,
        )?;

        if !run.ok {
            return Err(if run.stderr.trim().is_empty() {
                "코드 근거 검색에 실패했습니다".to_string()
            } else {
                run.stderr.trim().to_string()
            });
        }

        parse_focused_code_search_output(
            &run.stdout,
            &run.stderr,
            requested_limit.clamp(1, MAX_FOCUSED_SEARCH_LIMIT),
        )
    }

    fn query_graph(&self, project: &str, query: &str) -> Result<Value, String> {
        self.invoke_json(
            CodebaseMemoryTool::QueryGraph,
            &json!({ "project": project, "query": query }),
            Duration::from_secs(60),
        )
    }

    fn invoke_json(
        &self,
        tool: CodebaseMemoryTool,
        payload: &Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let run = self.invoke(tool, payload, timeout, None)?;
        if !run.ok {
            return Err(if run.stderr.trim().is_empty() {
                format!("코드 엔진 {} 실행에 실패했습니다", tool.as_str())
            } else {
                run.stderr.trim().to_string()
            });
        }

        engine_json_value(&run.stdout)
            .ok_or_else(|| format!("코드 엔진 {} 응답이 올바른 JSON이 아닙니다", tool.as_str()))
    }

    fn invoke(
        &self,
        tool: CodebaseMemoryTool,
        payload: &Value,
        timeout: Duration,
        allowed_root: Option<&str>,
    ) -> Result<engine::EngineRunResult, String> {
        let request = ArgsFile::create(&self.cache_dir, payload)?;
        let request_path = request.path().display().to_string();
        let args =
            engine::sidecar_args(["cli", tool.as_str(), "--args-file", request_path.as_str()])?;
        let cache_dir = self.cache_dir.display().to_string();
        let mut envs = vec![("CBM_CACHE_DIR", cache_dir.as_str())];
        if let Some(allowed_root) = allowed_root {
            envs.push(("CBM_ALLOWED_ROOT", allowed_root));
        }

        engine::run_engine_command_with_env(self.engine, &args, timeout, &envs)
    }
}

#[derive(Clone, Copy)]
enum CodebaseMemoryTool {
    IndexRepository,
    GetArchitecture,
    QueryGraph,
    SearchCode,
}

impl CodebaseMemoryTool {
    fn as_str(self) -> &'static str {
        match self {
            Self::IndexRepository => "index_repository",
            Self::GetArchitecture => "get_architecture",
            Self::QueryGraph => "query_graph",
            Self::SearchCode => "search_code",
        }
    }
}

struct ArgsFile {
    path: PathBuf,
}

impl ArgsFile {
    fn create(cache_dir: &Path, payload: &Value) -> Result<Self, String> {
        let request_dir = cache_dir.join("requests");
        fs::create_dir_all(&request_dir).map_err(|error| error.to_string())?;
        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();

        for _ in 0..4 {
            let sequence = REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path =
                request_dir.join(format!("request-{}-{epoch}-{sequence}.json", process::id()));
            let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.to_string()),
            };
            let request = Self { path };
            let bytes = serde_json::to_vec(payload).map_err(|error| error.to_string())?;
            file.write_all(&bytes).map_err(|error| error.to_string())?;
            file.flush().map_err(|error| error.to_string())?;
            drop(file);
            return Ok(request);
        }

        Err("코드 엔진 요청 파일 이름을 만들지 못했습니다".to_string())
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ArgsFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub(crate) fn index_payload(repo_path: &str, project_name: &str) -> Value {
    json!({
        "repo_path": repo_path,
        "mode": "full",
        "name": project_name,
        "persistence": false
    })
}

pub(crate) fn inventory_nodes_query() -> String {
    let labels = std::iter::once("Route")
        .chain(CODE_NODE_LABELS.iter().copied())
        .chain(std::iter::once("File"))
        .collect::<Vec<_>>()
        .join("|");
    format!(
        "MATCH (node:{labels}) RETURN labels(node) AS labels, node.name AS name, node.qualified_name AS qualified_name, node.file_path AS file_path, node.start_line AS start_line, node.start_column AS start_column, node.end_line AS end_line, node.end_column AS end_column, node.method AS method, node.source AS source, node.parent_qualified_name AS parent_qualified_name, node.parent_class AS parent_class, node.module AS module, node.namespace AS namespace, node.package AS package, node.route_path AS route_path, node.route_method AS route_method, node.signature AS signature, node.return_type AS return_type, node.is_test AS is_test LIMIT {MAX_CODE_NODES}"
    )
}

pub(crate) fn focused_code_search_payload(
    project: &str,
    identifier: &str,
    path_filter: Option<&str>,
    requested_limit: usize,
) -> Result<Value, String> {
    let pattern = focused_code_search_pattern(identifier)?;
    let limit = requested_limit.clamp(1, MAX_FOCUSED_SEARCH_LIMIT);
    if path_filter.is_some_and(|value| {
        value.len() > MAX_FOCUSED_PATH_FILTER_BYTES || value.chars().any(char::is_control)
    }) {
        return Err("코드 검색 경로 필터가 너무 길거나 올바르지 않습니다".to_string());
    }
    let path_filter = path_filter.map(str::trim).filter(|value| !value.is_empty());

    let mut payload = json!({
        "project": project,
        "pattern": pattern,
        "regex": true,
        "mode": "compact",
        "context": 0,
        "limit": limit
    });
    if let Some(path_filter) = path_filter {
        payload["path_filter"] = Value::String(path_filter.to_string());
    }
    Ok(payload)
}

pub(crate) fn focused_code_search_pattern(identifier: &str) -> Result<String, String> {
    let identifier = identifier.trim();
    if identifier.is_empty() {
        return Err("코드에서 찾을 테이블 또는 컬럼 이름이 필요합니다".to_string());
    }
    if identifier.len() > MAX_FOCUSED_SEARCH_TERM_BYTES || identifier.chars().any(char::is_control)
    {
        return Err("코드 검색 이름이 너무 길거나 올바르지 않습니다".to_string());
    }

    let mut escaped = String::with_capacity(identifier.len());
    for character in identifier.chars() {
        if matches!(
            character,
            '\\' | '^' | '$' | '.' | '|' | '?' | '*' | '+' | '(' | ')' | '[' | ']' | '{' | '}'
        ) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    Ok(format!("(^|[^A-Za-z0-9_]){escaped}([^A-Za-z0-9_]|$)"))
}

#[derive(serde::Deserialize)]
struct RawFocusedCodeSearch {
    results: Vec<RawFocusedCodeSearchMatch>,
    total_grep_matches: usize,
    total_results: usize,
    raw_match_count: usize,
}

#[derive(serde::Deserialize)]
struct RawFocusedCodeSearchMatch {
    qualified_name: String,
    label: String,
    file: String,
    start_line: u64,
    end_line: u64,
    match_lines: Vec<u64>,
}

pub(crate) fn parse_focused_code_search_output(
    stdout: &str,
    stderr: &str,
    applied_limit: usize,
) -> Result<FocusedCodeSearch, String> {
    let raw = serde_json::from_str::<RawFocusedCodeSearch>(stdout.trim())
        .ok()
        .or_else(|| {
            stdout.lines().find_map(|line| {
                let line = line.trim();
                line.starts_with('{')
                    .then(|| serde_json::from_str::<RawFocusedCodeSearch>(line).ok())
                    .flatten()
            })
        })
        .ok_or_else(|| "코드 엔진 search_code 응답이 올바른 JSON이 아닙니다".to_string())?;
    let applied_limit = applied_limit.clamp(1, MAX_FOCUSED_SEARCH_LIMIT);
    if raw.results.len() > applied_limit || raw.results.len() > raw.total_results {
        return Err("코드 엔진 search_code 결과 합계가 일관되지 않습니다".to_string());
    }

    let matches = raw
        .results
        .into_iter()
        .map(|item| FocusedCodeSearchMatch {
            qualified_name: item.qualified_name,
            label: item.label,
            file: item.file,
            start_line: item.start_line,
            end_line: item.end_line,
            match_lines: item.match_lines,
        })
        .collect::<Vec<_>>();
    let totals = FocusedCodeSearchTotals {
        returned: matches.len(),
        total_results: raw.total_results,
        total_grep_matches: raw.total_grep_matches,
        raw_match_count: raw.raw_match_count,
    };
    let mut partial_reasons = Vec::new();
    if stderr.lines().any(|line| {
        let line = line.trim();
        !line.is_empty() && !line.starts_with("level=")
    }) {
        partial_reasons.push("engine-stderr".to_string());
    }
    if totals.returned < totals.total_results {
        partial_reasons.push("result-limit".to_string());
    }
    if totals.total_grep_matches >= SEARCH_CODE_GREP_LIMIT {
        partial_reasons.push("grep-limit".to_string());
    }
    if totals.raw_match_count > 0 {
        partial_reasons.push("unmapped-raw-matches".to_string());
    }

    Ok(FocusedCodeSearch {
        matches,
        totals,
        partial: !partial_reasons.is_empty(),
        partial_reasons,
    })
}

pub(crate) fn normalize_inventory_nodes(value: &Value) -> Result<Value, String> {
    let columns = value
        .get("columns")
        .and_then(Value::as_array)
        .ok_or_else(|| "코드 엔진 노드 응답에 columns가 없습니다".to_string())?
        .iter()
        .map(|column| {
            column
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| "코드 엔진 노드 column 이름이 문자열이 아닙니다".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let rows = value
        .get("rows")
        .and_then(Value::as_array)
        .ok_or_else(|| "코드 엔진 노드 응답에 rows가 없습니다".to_string())?;
    let total = value
        .get("total")
        .and_then(Value::as_u64)
        .ok_or_else(|| "코드 엔진 노드 응답에 total이 없습니다".to_string())?;
    if total != rows.len() as u64 {
        return Err("코드 엔진 노드 결과 합계가 일관되지 않습니다".to_string());
    }

    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        let values = row
            .as_array()
            .ok_or_else(|| "코드 엔진 노드 row가 배열이 아닙니다".to_string())?;
        if values.len() != columns.len() {
            return Err("코드 엔진 노드 column과 row 길이가 다릅니다".to_string());
        }

        let mut object = columns
            .iter()
            .cloned()
            .zip(values.iter().cloned())
            .collect::<Map<_, _>>();
        let label = object
            .remove("labels")
            .as_ref()
            .and_then(single_graph_label)
            .ok_or_else(|| "코드 엔진 노드 label이 없거나 올바르지 않습니다".to_string())?;
        if label != "Route" && label != "File" && !CODE_NODE_LABELS.contains(&label.as_str()) {
            return Err(format!("허용되지 않은 코드 엔진 노드 label입니다: {label}"));
        }
        object.insert("label".to_string(), Value::String(label));
        normalize_line_fields(&mut object);
        results.push(Value::Object(object));
    }

    Ok(json!({ "total": total, "results": results, "has_more": false }))
}

fn single_graph_label(value: &Value) -> Option<String> {
    if let Some(items) = value.as_array() {
        return items.first()?.as_str().map(str::to_string);
    }
    let value = value.as_str()?;
    serde_json::from_str::<Vec<String>>(value)
        .ok()
        .and_then(|items| items.into_iter().next())
        .or_else(|| (!value.trim().is_empty()).then(|| value.to_string()))
}

fn normalize_line_fields(object: &mut Map<String, Value>) {
    for key in ["start_line", "start_column", "end_line", "end_column"] {
        let Some(value) = object.get_mut(key) else {
            continue;
        };
        if let Some(parsed) = value.as_str().and_then(|value| value.parse::<u64>().ok()) {
            *value = Value::Number(parsed.into());
        }
    }
}

fn ensure_result_below_limit(value: &Value, kind: &str, limit: usize) -> Result<(), String> {
    let total = value
        .get("total")
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("코드 엔진 {kind} 응답에 total이 없습니다"))?;
    if total >= limit as u64 {
        Err(format!(
            "{kind} 결과가 안전 한도({limit})에 도달해 잘렸을 수 있습니다"
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_exposes_only_product_tools() {
        let tools = [
            CodebaseMemoryTool::IndexRepository,
            CodebaseMemoryTool::GetArchitecture,
            CodebaseMemoryTool::QueryGraph,
            CodebaseMemoryTool::SearchCode,
        ]
        .map(CodebaseMemoryTool::as_str);

        assert_eq!(
            tools,
            [
                "index_repository",
                "get_architecture",
                "query_graph",
                "search_code"
            ]
        );
        assert!(!tools.contains(&"semantic_query"));
        assert!(!tools.contains(&"manage_adr"));
    }

    #[test]
    fn args_file_is_deleted_when_request_guard_drops() {
        let root = std::env::temp_dir().join(format!(
            "backend-map-code-adapter-{}-{}",
            process::id(),
            REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let request = ArgsFile::create(&root, &json!({ "project": "shop" })).unwrap();
        let path = request.path().to_path_buf();
        assert!(path.is_file());
        drop(request);
        assert!(!path.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn query_rows_become_stable_inventory_objects() {
        let normalized = normalize_inventory_nodes(&json!({
            "columns": [
                "labels", "name", "qualified_name", "file_path",
                "start_line", "start_column", "end_line", "end_column"
            ],
            "rows": [[
                "[\"Method\"]", "create", "shop.OrderService.create", "src/OrderService.java",
                "12", "3", "20", ""
            ]],
            "total": 1
        }))
        .unwrap();
        let item = &normalized["results"][0];

        assert_eq!(item["label"], "Method");
        assert_eq!(item["start_line"], 12);
        assert_eq!(item["start_column"], 3);
        assert_eq!(item["end_line"], 20);
    }
}
