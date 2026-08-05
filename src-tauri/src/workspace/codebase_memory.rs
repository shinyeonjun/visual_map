use crate::{engine, EngineRegistry};
use flate2::read::GzDecoder;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
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

fn runtime_cache_root(cache_dir: &Path) -> PathBuf {
    cache_dir.join("runtime")
}

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
#[derive(Debug)]
pub(crate) struct CodebaseMemoryInventory {
    pub architecture: Value,
    pub evidence: Value,
    pub nodes: Value,
    pub calls: Value,
    pub handles: Value,
}

#[derive(Debug, Deserialize)]
struct InventoryExport {
    schema: String,
    architecture: Value,
    evidence: Value,
    nodes: Value,
    calls: Value,
    handles: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeGenerationCounts {
    nodes: usize,
    calls: usize,
    handles: usize,
    architecture_nodes: usize,
    architecture_edges: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeGenerationReceipt {
    schema: String,
    generation_id: String,
    status: String,
    database_path: String,
    counts: CodeGenerationCounts,
}

#[derive(Debug, Deserialize)]
struct ProviderPlan {
    schema: String,
    languages: Vec<String>,
}

pub(crate) struct CodebaseMemoryAdapter<'a> {
    engine: &'a engine::EngineAvailability,
    cache_dir: PathBuf,
    provider_cache_dir: Option<PathBuf>,
    observer: Option<engine::EngineObserver>,
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
            provider_cache_dir: None,
            observer: None,
        })
    }

    pub(crate) fn new_with_provider_cache(
        registry: &'a EngineRegistry,
        cache_dir: impl Into<PathBuf>,
        provider_cache_dir: impl Into<PathBuf>,
    ) -> Result<Self, String> {
        let mut adapter = Self::new(registry, cache_dir)?;
        adapter.provider_cache_dir = Some(provider_cache_dir.into());
        Ok(adapter)
    }

    pub(crate) fn with_observer(mut self, observer: engine::EngineObserver) -> Self {
        self.observer = Some(observer);
        self
    }

    pub(crate) fn index_repository(
        &self,
        repo_path: &str,
        project_name: &str,
    ) -> Result<engine::EngineRunResult, String> {
        let payload = index_payload(repo_path, project_name);
        let plan = self.provider_plan(repo_path)?;
        self.invoke_with_operation(
            CodebaseMemoryTool::IndexRepository,
            &payload,
            Duration::from_secs(6 * 60 * 60),
            Some(repo_path),
            None,
            Some(&plan.languages),
        )
    }

    fn provider_plan(&self, repo_path: &str) -> Result<ProviderPlan, String> {
        let value = self.invoke_json(
            CodebaseMemoryTool::ProviderPlan,
            &json!({ "repo_path": repo_path }),
            Duration::from_secs(5 * 60),
        )?;
        let plan: ProviderPlan = serde_json::from_value(value)
            .map_err(|error| format!("provider plan 형식이 올바르지 않습니다: {error}"))?;
        if plan.schema != "code-memory.provider-plan.v1" {
            return Err(format!(
                "지원하지 않는 provider plan입니다: {}",
                plan.schema
            ));
        }
        Ok(plan)
    }

    pub(crate) fn delete_project(&self, project: &str) -> Result<(), String> {
        self.invoke_json(
            CodebaseMemoryTool::DeleteProject,
            &json!({ "project": project }),
            Duration::from_secs(30),
        )?;
        Ok(())
    }

    pub(crate) fn inventory(&self, project: &str) -> Result<CodebaseMemoryInventory, String> {
        if let Some(inventory) = self.generation_inventory(project)? {
            return Ok(inventory);
        }
        let export = self.invoke_json(
            CodebaseMemoryTool::ExportInventory,
            &json!({ "project": project }),
            Duration::from_secs(10 * 60),
        )?;
        let export: InventoryExport = serde_json::from_value(export)
            .map_err(|error| format!("코드 엔진 inventory export가 올바르지 않습니다: {error}"))?;
        if export.schema != "code-memory.inventory-export.v1" {
            return Err(format!(
                "지원하지 않는 코드 inventory export 계약입니다: {}",
                export.schema
            ));
        }
        let nodes = normalize_inventory_nodes(&export.nodes)?;
        ensure_result_below_limit(&nodes, "code nodes", MAX_CODE_NODES)?;
        ensure_result_below_limit(&export.calls, "CALLS", MAX_GRAPH_RELATIONSHIPS)?;
        ensure_result_below_limit(&export.handles, "HANDLES", MAX_GRAPH_RELATIONSHIPS)?;

        Ok(CodebaseMemoryInventory {
            architecture: export.architecture,
            evidence: export.evidence,
            nodes,
            calls: export.calls,
            handles: export.handles,
        })
    }

    fn generation_inventory(
        &self,
        project: &str,
    ) -> Result<Option<CodebaseMemoryInventory>, String> {
        let project_dir = self
            .cache_dir
            .join("compat-projects")
            .join(safe_project_name(project)?);
        let current = project_dir.join("current.json");
        if !current.is_file() {
            return Ok(None);
        }
        let receipt: CodeGenerationReceipt = serde_json::from_slice(
            &fs::read(&current)
                .map_err(|error| format!("코드 세대 receipt를 읽지 못했습니다: {error}"))?,
        )
        .map_err(|error| format!("코드 세대 receipt가 올바르지 않습니다: {error}"))?;
        if receipt.schema != "code-memory.generation-receipt.v1" || receipt.status != "complete" {
            return Err("완료되지 않은 코드 세대는 읽을 수 없습니다".to_string());
        }
        let expected = project_dir
            .join("generations")
            .join(&receipt.generation_id)
            .join("code-graph.sqlite");
        let expected = fs::canonicalize(&expected).map_err(|error| {
            format!(
                "코드 세대 데이터베이스를 찾을 수 없습니다({}): {error}",
                expected.display()
            )
        })?;
        let advertised = fs::canonicalize(&receipt.database_path)
            .map_err(|error| format!("receipt의 코드 데이터베이스 경로가 없습니다: {error}"))?;
        let cache_root = fs::canonicalize(&self.cache_dir)
            .map_err(|error| format!("코드 캐시 경로를 확인할 수 없습니다: {error}"))?;
        if advertised != expected || !expected.starts_with(&cache_root) {
            return Err(
                "receipt의 코드 데이터베이스 경로가 워크스페이스 캐시를 벗어났습니다".to_string(),
            );
        }
        let connection = Connection::open_with_flags(
            &expected,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|error| format!("코드 세대 데이터베이스를 열지 못했습니다: {error}"))?;
        connection
            .pragma_update(None, "query_only", true)
            .map_err(|error| format!("코드 세대를 읽기 전용으로 열지 못했습니다: {error}"))?;
        verify_generation_counts(&connection, &receipt.counts)?;
        let store_schema = generation_metadata(&connection, "schema")?;
        if !matches!(
            store_schema.as_str(),
            Some(
                "code-memory.graph-store.v1"
                    | "code-memory.graph-store.v2"
                    | "code-memory.graph-store.v3"
            )
        ) {
            return Err(format!(
                "지원하지 않는 코드 세대 저장 형식입니다: {store_schema}"
            ));
        }
        ensure_count_below_limit(receipt.counts.nodes, "code nodes", MAX_CODE_NODES)?;
        ensure_count_below_limit(receipt.counts.calls, "CALLS", MAX_GRAPH_RELATIONSHIPS)?;
        ensure_count_below_limit(receipt.counts.handles, "HANDLES", MAX_GRAPH_RELATIONSHIPS)?;

        let chunked = store_schema.as_str() == Some("code-memory.graph-store.v3");
        let (nodes, calls, handles) = if chunked {
            (
                generation_chunk_rows_result(&connection, "inventory_columns", "inventory")?,
                generation_chunk_rows_result(&connection, "calls_columns", "calls")?,
                generation_chunk_rows_result(&connection, "handles_columns", "handles")?,
            )
        } else {
            (
                generation_rows_result(
                    &connection,
                    "inventory_columns",
                    "SELECT row_json FROM inventory_nodes ORDER BY ordinal",
                    None,
                )?,
                generation_rows_result(
                    &connection,
                    "calls_columns",
                    "SELECT row_json FROM relationships WHERE kind = ?1 ORDER BY ordinal",
                    Some("CALLS"),
                )?,
                generation_rows_result(
                    &connection,
                    "handles_columns",
                    "SELECT row_json FROM relationships WHERE kind = ?1 ORDER BY ordinal",
                    Some("HANDLES"),
                )?,
            )
        };
        let nodes = normalize_inventory_nodes(&nodes)?;
        let mut architecture = generation_metadata(&connection, "architecture_header")?;
        let architecture_object = architecture
            .as_object_mut()
            .ok_or("코드 architecture header가 객체가 아닙니다")?;
        let (architecture_nodes, architecture_edges) = if chunked {
            (
                generation_chunk_rows(&connection, "architecture_nodes")?,
                generation_chunk_rows(&connection, "architecture_edges")?,
            )
        } else {
            (
                generation_json_rows(
                    &connection,
                    "SELECT node_json FROM architecture_nodes ORDER BY ordinal",
                    None,
                )?,
                generation_json_rows(
                    &connection,
                    "SELECT edge_json FROM architecture_edges ORDER BY ordinal",
                    None,
                )?,
            )
        };
        architecture_object.insert("nodes".to_string(), Value::Array(architecture_nodes));
        architecture_object.insert("edges".to_string(), Value::Array(architecture_edges));

        Ok(Some(CodebaseMemoryInventory {
            architecture,
            evidence: generation_metadata(&connection, "evidence")?,
            nodes,
            calls,
            handles,
        }))
    }

    pub(crate) fn search_code_with_operation(
        &self,
        project: &str,
        identifier: &str,
        path_filter: Option<&str>,
        requested_limit: usize,
        operation_id: Option<&str>,
    ) -> Result<FocusedCodeSearch, String> {
        let payload =
            focused_code_search_payload(project, identifier, path_filter, requested_limit)?;
        let run = self.invoke_with_operation(
            CodebaseMemoryTool::SearchCode,
            &payload,
            Duration::from_secs(60),
            None,
            operation_id,
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
        self.invoke_with_operation(tool, payload, timeout, allowed_root, None, None)
    }

    fn invoke_with_operation(
        &self,
        tool: CodebaseMemoryTool,
        payload: &Value,
        timeout: Duration,
        allowed_root: Option<&str>,
        operation_id: Option<&str>,
        required_languages: Option<&[String]>,
    ) -> Result<engine::EngineRunResult, String> {
        let request = ArgsFile::create(&self.cache_dir, payload)?;
        let request_path = request.path().display().to_string();
        let args =
            engine::sidecar_args(["cli", tool.as_str(), "--args-file", request_path.as_str()])?;
        let mut env_values = vec![
            (
                "CBM_CACHE_DIR".to_string(),
                self.cache_dir.display().to_string(),
            ),
            (
                "CODE_MEMORY_CACHE_ROOT".to_string(),
                runtime_cache_root(&self.cache_dir).display().to_string(),
            ),
        ];
        let engine_dir = Path::new(&self.engine.path).parent();
        if let Some(path) = engine_dir
            .map(|directory| directory.join("packs"))
            .filter(|path| path.is_dir())
            .or_else(|| {
                #[cfg(debug_assertions)]
                {
                    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../code_memory/packs");
                    source.is_dir().then_some(source)
                }
                #[cfg(not(debug_assertions))]
                {
                    None
                }
            })
        {
            env_values.push((
                "CODE_MEMORY_PACKS_ROOT".to_string(),
                path.display().to_string(),
            ));
        }
        let provider_dir = engine_dir
            .map(|directory| directory.join("providers"))
            .filter(|path| path.is_dir())
            .or_else(|| {
                #[cfg(debug_assertions)]
                {
                    let source_dir =
                        Path::new(env!("CARGO_MANIFEST_DIR")).join("../code_memory/providers");
                    source_dir.is_dir().then_some(source_dir)
                }
                #[cfg(not(debug_assertions))]
                {
                    None
                }
            });
        let provider_dir = match provider_dir {
            Some(path) => Some(path),
            None if tool.requires_providers() => {
                match (engine_dir, self.provider_cache_dir.as_deref()) {
                    (Some(engine_dir), Some(provider_cache_dir)) => {
                        super::provider_bundle::ensure_provider_root(
                            engine_dir,
                            provider_cache_dir,
                            required_languages.unwrap_or_default(),
                        )?
                    }
                    _ => None,
                }
            }
            None => None,
        };
        if let Some(path) = provider_dir {
            env_values.push((
                "CODE_MEMORY_PROVIDERS_ROOT".to_string(),
                path.display().to_string(),
            ));
        }
        if let Some(allowed_root) = allowed_root {
            env_values.push(("CBM_ALLOWED_ROOT".to_string(), allowed_root.to_string()));
        }
        if let Some(operation_id) = operation_id {
            env_values.push((
                "BACKEND_VISUAL_MAP_OPERATION_ID".to_string(),
                operation_id.to_string(),
            ));
        }
        let envs = env_values
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect::<Vec<_>>();

        let policy = if matches!(tool, CodebaseMemoryTool::IndexRepository) {
            engine::EngineRunPolicy {
                hard_timeout: timeout,
                idle_timeout: Duration::from_secs(5 * 60),
            }
        } else {
            engine::EngineRunPolicy::fixed(timeout)
        };
        engine::run_engine_command_with_env_observer(
            self.engine,
            &args,
            policy,
            &envs,
            self.observer.clone(),
        )
    }
}

#[derive(Clone, Copy)]
enum CodebaseMemoryTool {
    ProviderPlan,
    IndexRepository,
    DeleteProject,
    ExportInventory,
    SearchCode,
}

impl CodebaseMemoryTool {
    fn as_str(self) -> &'static str {
        match self {
            Self::ProviderPlan => "provider_plan",
            Self::IndexRepository => "index_repository",
            Self::DeleteProject => "delete_project",
            Self::ExportInventory => "export_inventory",
            Self::SearchCode => "search_code",
        }
    }

    fn requires_providers(self) -> bool {
        matches!(self, Self::IndexRepository)
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

fn ensure_count_below_limit(total: usize, kind: &str, limit: usize) -> Result<(), String> {
    if total >= limit {
        Err(format!(
            "{kind} 결과가 안전 한도({limit})에 도달해 잘렸을 수 있습니다"
        ))
    } else {
        Ok(())
    }
}

fn safe_project_name(project: &str) -> Result<String, String> {
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
    if safe.is_empty() || matches!(safe.as_str(), "." | "..") {
        Err("코드 프로젝트 이름이 올바르지 않습니다".to_string())
    } else {
        Ok(safe)
    }
}

fn verify_generation_counts(
    connection: &Connection,
    expected: &CodeGenerationCounts,
) -> Result<(), String> {
    let actual = [
        (
            "nodes",
            "SELECT COUNT(*) FROM inventory_nodes",
            expected.nodes,
        ),
        (
            "calls",
            "SELECT COUNT(*) FROM relationships WHERE kind = 'CALLS'",
            expected.calls,
        ),
        (
            "handles",
            "SELECT COUNT(*) FROM relationships WHERE kind = 'HANDLES'",
            expected.handles,
        ),
        (
            "architecture nodes",
            "SELECT COUNT(*) FROM architecture_nodes",
            expected.architecture_nodes,
        ),
        (
            "architecture edges",
            "SELECT COUNT(*) FROM architecture_edges",
            expected.architecture_edges,
        ),
    ];
    for (label, sql, expected) in actual {
        let count = connection
            .query_row(sql, [], |row| row.get::<_, usize>(0))
            .map_err(|error| format!("코드 세대 {label} 수를 읽지 못했습니다: {error}"))?;
        if count != expected {
            return Err(format!(
                "코드 세대 {label} 수가 receipt와 다릅니다({expected} != {count})"
            ));
        }
    }
    Ok(())
}

fn generation_metadata(connection: &Connection, key: &str) -> Result<Value, String> {
    let value = connection
        .query_row(
            "SELECT value_json FROM metadata WHERE key = ?1",
            [key],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("코드 세대 메타데이터를 읽지 못했습니다: {error}"))?
        .ok_or_else(|| format!("코드 세대 메타데이터 '{key}'가 없습니다"))?;
    serde_json::from_str(&value)
        .map_err(|error| format!("코드 세대 메타데이터 '{key}'가 올바르지 않습니다: {error}"))
}

fn generation_rows_result(
    connection: &Connection,
    columns_key: &str,
    sql: &str,
    parameter: Option<&str>,
) -> Result<Value, String> {
    let rows = generation_json_rows(connection, sql, parameter)?;
    let total = rows.len();
    Ok(json!({
        "columns": generation_metadata(connection, columns_key)?,
        "rows": rows,
        "total": total
    }))
}

fn generation_chunk_rows_result(
    connection: &Connection,
    columns_key: &str,
    kind: &str,
) -> Result<Value, String> {
    let rows = generation_chunk_rows(connection, kind)?;
    let total = rows.len();
    Ok(json!({
        "columns": generation_metadata(connection, columns_key)?,
        "rows": rows,
        "total": total
    }))
}

fn generation_chunk_rows(connection: &Connection, kind: &str) -> Result<Vec<Value>, String> {
    let mut statement = connection
        .prepare("SELECT payload FROM chunks WHERE kind = ?1 ORDER BY chunk_index")
        .map_err(|error| format!("코드 세대 청크 질의를 준비하지 못했습니다: {error}"))?;
    let rows = statement
        .query_map([kind], |row| Ok(row.get_ref(0)?.as_bytes()?.to_vec()))
        .map_err(|error| format!("코드 세대 청크를 질의하지 못했습니다: {error}"))?;
    let mut values = Vec::new();
    for row in rows {
        let bytes = row.map_err(|error| format!("코드 세대 청크를 읽지 못했습니다: {error}"))?;
        let value = decode_generation_json(&bytes)?;
        let chunk = value.as_array().ok_or("코드 세대 청크가 배열이 아닙니다")?;
        values.extend(chunk.iter().cloned());
    }
    Ok(values)
}

fn generation_json_rows(
    connection: &Connection,
    sql: &str,
    parameter: Option<&str>,
) -> Result<Vec<Value>, String> {
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| format!("코드 세대 질의를 준비하지 못했습니다: {error}"))?;
    let mut rows = match parameter {
        Some(value) => statement.query([value]),
        None => statement.query([]),
    }
    .map_err(|error| format!("코드 세대를 질의하지 못했습니다: {error}"))?;
    let mut values = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|error| format!("코드 세대 행을 읽지 못했습니다: {error}"))?
    {
        let value = row
            .get_ref(0)
            .map_err(|error| format!("코드 세대 JSON을 읽지 못했습니다: {error}"))?;
        let value = value
            .as_bytes()
            .map_err(|error| format!("코드 세대 JSON 형식이 올바르지 않습니다: {error}"))?;
        values.push(decode_generation_json(value)?);
    }
    Ok(values)
}

fn decode_generation_json(value: &[u8]) -> Result<Value, String> {
    let json = if value.starts_with(&[0x1f, 0x8b]) {
        let mut decoder = GzDecoder::new(value);
        let mut json = Vec::new();
        decoder
            .read_to_end(&mut json)
            .map_err(|error| format!("코드 세대 JSON 압축을 풀지 못했습니다: {error}"))?;
        json
    } else {
        value.to_vec()
    };
    serde_json::from_slice(&json)
        .map_err(|error| format!("코드 세대 JSON이 올바르지 않습니다: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_exposes_only_product_tools() {
        let tools = [
            CodebaseMemoryTool::IndexRepository,
            CodebaseMemoryTool::DeleteProject,
            CodebaseMemoryTool::ExportInventory,
            CodebaseMemoryTool::SearchCode,
        ]
        .map(CodebaseMemoryTool::as_str);

        assert_eq!(
            tools,
            [
                "index_repository",
                "delete_project",
                "export_inventory",
                "search_code"
            ]
        );
        assert!(!tools.contains(&"semantic_query"));
        assert!(!tools.contains(&"manage_adr"));
    }

    #[test]
    fn inventory_export_contract_deserializes_all_sections() {
        let export: InventoryExport = serde_json::from_value(json!({
            "schema": "code-memory.inventory-export.v1",
            "architecture": {"schema": "architecture"},
            "evidence": {"schema": "evidence"},
            "nodes": {"columns": [], "rows": [], "total": 0},
            "calls": {"columns": [], "rows": [], "total": 0},
            "handles": {"columns": [], "rows": [], "total": 0}
        }))
        .unwrap();

        assert_eq!(export.schema, "code-memory.inventory-export.v1");
        assert_eq!(export.architecture["schema"], "architecture");
        assert_eq!(export.evidence["schema"], "evidence");
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
    fn code_engine_runtime_cache_stays_inside_workspace_cache() {
        let cache = Path::new("workspaces")
            .join("workspace-1")
            .join("engines")
            .join("codebase-memory")
            .join("cache");

        let runtime = runtime_cache_root(&cache);
        assert_eq!(runtime, cache.join("runtime"));
        assert!(runtime.starts_with(&cache));
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
