use crate::paths::base_paths;
use crate::{engine, EngineRegistry};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
    time::Duration,
};

use super::model::{
    CodeCall, CodeHandle, CodeIndexResult, CodeInventory, CodeInventoryItem, CodeInventorySummary,
    FocusedCodeSearch, FocusedCodeSearchMatch, FocusedCodeSearchTotals, IndexCodeRequest,
    Workspace,
};
use super::store::{
    engine_json_value, object_string, read_workspace_by_id, timestamp, validate_workspace_id,
    value_items, workspace_code_cache_path, workspace_db_cache_dir, write_workspace,
};

const SEARCH_PAGE_SIZE: usize = 500;
const MAX_CODE_NODES: usize = 100_000;
const MAX_GRAPH_RELATIONSHIPS: usize = 100_000;
const MAX_FOCUSED_SEARCH_LIMIT: usize = 32;
const MAX_FOCUSED_SEARCH_TERM_BYTES: usize = 512;
const MAX_FOCUSED_PATH_FILTER_BYTES: usize = 512;
const SEARCH_CODE_GREP_LIMIT: usize = 500;
type CodeSourceLocation = (
    String,
    Option<String>,
    Option<u64>,
    Option<u64>,
    Option<u64>,
    Option<u64>,
);
const CODE_LABELS: &[&str] = &[
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
    "Union",
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
pub(crate) const CALLS_QUERY: &str = "MATCH (caller)-[:CALLS]->(callee) RETURN caller.qualified_name AS source, callee.qualified_name AS target LIMIT 100000";
pub(crate) const HANDLES_QUERY: &str = "MATCH (handler)-[:HANDLES]->(route) RETURN handler.qualified_name AS source, route.qualified_name AS target LIMIT 100000";
pub(crate) const SOURCE_LOCATIONS_QUERY: &str = "MATCH (node) RETURN node.qualified_name AS source, node.file_path AS path, node.start_line AS start_line, node.start_column AS start_column, node.end_line AS end_line, node.end_column AS end_column LIMIT 100000";

pub(crate) fn index_code_repository(
    app_data_dir: impl AsRef<Path>,
    registry: &EngineRegistry,
    request: IndexCodeRequest,
) -> Result<CodeIndexResult, String> {
    validate_workspace_id(&request.workspace_id)?;

    let paths = base_paths(app_data_dir);
    let mut workspace = read_workspace_by_id(&paths.workspaces_dir, &request.workspace_id)?;
    let code_cache_path = workspace_code_cache_path(&paths.workspaces_dir, &request.workspace_id);
    fs::create_dir_all(&code_cache_path).map_err(|error| error.to_string())?;
    workspace.engine_cache.code_cache_path = Some(code_cache_path.display().to_string());
    workspace.engine_cache.db_cache_dir = Some(
        workspace_db_cache_dir(&paths.workspaces_dir, &request.workspace_id)
            .display()
            .to_string(),
    );
    let code_engine = registry
        .engines
        .iter()
        .find(|engine| engine.id == "codebase-memory")
        .ok_or_else(|| "코드 읽기 도구가 등록되지 않았습니다".to_string())?;

    let payload = code_index_payload(&workspace, &code_cache_path);
    let args = engine::sidecar_args(["cli", "index_repository", payload.as_str()])?;
    let code_cache_env = code_cache_path.display().to_string();
    let run = engine::run_engine_command_with_env(
        code_engine,
        &args,
        Duration::from_secs(300),
        &[("CBM_CACHE_DIR", code_cache_env.as_str())],
    )?;

    if run.ok {
        workspace.code_project = Some(code_project_from_index_stdout(&run.stdout, &workspace.name));
        workspace.updated_at = timestamp();
        write_workspace(&paths.workspaces_dir, &workspace)?;
    }

    Ok(CodeIndexResult { workspace, run })
}

pub(crate) fn code_inventory(
    app_data_dir: impl AsRef<Path>,
    registry: &EngineRegistry,
    workspace_id: &str,
) -> Result<CodeInventory, String> {
    validate_workspace_id(workspace_id)?;

    let paths = base_paths(app_data_dir);
    let workspace = read_workspace_by_id(&paths.workspaces_dir, workspace_id)?;
    let code_cache_path = workspace_code_cache_path(&paths.workspaces_dir, workspace_id)
        .display()
        .to_string();
    let project = workspace
        .code_project
        .clone()
        .unwrap_or_else(|| workspace.name.clone());
    let code_engine = registry
        .engines
        .iter()
        .find(|engine| engine.id == "codebase-memory")
        .ok_or_else(|| "코드 읽기 도구가 등록되지 않았습니다".to_string())?;

    let architecture = run_code_query(
        code_engine,
        "get_architecture",
        serde_json::json!({ "project": project, "cache_path": code_cache_path, "cache_dir": code_cache_path }),
    )?;
    let mut remaining = MAX_CODE_NODES;
    let routes = run_code_label_query(code_engine, &project, "Route", &code_cache_path, remaining)?;
    remaining -= routes.len();

    let mut code_items = Vec::new();
    for label in CODE_LABELS {
        let items =
            run_code_label_query(code_engine, &project, label, &code_cache_path, remaining)?;
        remaining -= items.len();
        code_items.extend(items);
    }

    let files = run_code_label_query(code_engine, &project, "File", &code_cache_path, remaining)?;
    let calls = run_code_query(
        code_engine,
        "query_graph",
        serde_json::json!({
            "project": project,
            "codeProject": project,
            "query": CALLS_QUERY,
            "cache_path": code_cache_path,
            "cache_dir": code_cache_path
        }),
    )?;
    let handles = run_code_query(
        code_engine,
        "query_graph",
        serde_json::json!({
            "project": project,
            "codeProject": project,
            "query": HANDLES_QUERY,
            "cache_path": code_cache_path,
            "cache_dir": code_cache_path
        }),
    )?;
    let locations = run_code_query(
        code_engine,
        "query_graph",
        serde_json::json!({
            "project": project,
            "codeProject": project,
            "query": SOURCE_LOCATIONS_QUERY,
            "cache_path": code_cache_path,
            "cache_dir": code_cache_path
        }),
    )?;

    ensure_graph_result_below_limit(&calls, "CALLS")?;
    ensure_graph_result_below_limit(&handles, "HANDLES")?;
    ensure_graph_result_below_limit(&locations, "source locations")?;

    let routes = serde_json::json!({ "results": routes });
    let services = serde_json::json!({ "results": code_items });
    let files = serde_json::json!({ "results": files });
    let mut inventory =
        extract_code_inventory(project, Some(architecture), &routes, &services, &files)?;
    inventory.calls = extract_code_calls(&calls, &inventory);
    attach_code_handles(&handles, &mut inventory);
    enrich_code_locations(&locations, &mut inventory)?;
    downgrade_unverified_routes(&mut inventory);
    Ok(inventory)
}

pub(crate) fn focused_code_search(
    app_data_dir: impl AsRef<Path>,
    registry: &EngineRegistry,
    workspace_id: &str,
    identifier: &str,
    path_filter: Option<&str>,
    requested_limit: usize,
) -> Result<FocusedCodeSearch, String> {
    validate_workspace_id(workspace_id)?;
    let paths = base_paths(app_data_dir);
    let workspace = read_workspace_by_id(&paths.workspaces_dir, workspace_id)?;
    let code_cache_path = workspace_code_cache_path(&paths.workspaces_dir, workspace_id)
        .display()
        .to_string();
    let project = workspace
        .code_project
        .as_deref()
        .unwrap_or(workspace.name.as_str());
    let code_engine = registry
        .engines
        .iter()
        .find(|engine| engine.id == "codebase-memory")
        .ok_or_else(|| "코드 읽기 도구가 등록되지 않았습니다".to_string())?;
    let args = focused_code_search_args(project, identifier, path_filter, requested_limit)?;
    let run = engine::run_engine_command_with_env(
        code_engine,
        &args,
        Duration::from_secs(60),
        &[("CBM_CACHE_DIR", code_cache_path.as_str())],
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

pub(crate) fn focused_code_search_args(
    project: &str,
    identifier: &str,
    path_filter: Option<&str>,
    requested_limit: usize,
) -> Result<Vec<String>, String> {
    let pattern = focused_code_search_pattern(identifier)?;
    let limit = requested_limit.clamp(1, MAX_FOCUSED_SEARCH_LIMIT);
    if path_filter.is_some_and(|value| {
        value.len() > MAX_FOCUSED_PATH_FILTER_BYTES || value.chars().any(char::is_control)
    }) {
        return Err("코드 검색 경로 필터가 너무 길거나 올바르지 않습니다".to_string());
    }
    let path_filter = path_filter.map(str::trim).filter(|value| !value.is_empty());

    let mut payload = serde_json::json!({
        "project": project,
        "pattern": pattern,
        "regex": true,
        "mode": "compact",
        "context": 0,
        "limit": limit
    });
    if let Some(path_filter) = path_filter {
        payload["path_filter"] = serde_json::Value::String(path_filter.to_string());
    }

    let payload = payload.to_string();
    engine::sidecar_args(["cli", "search_code", payload.as_str()])
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
    // ASCII token classes behave the same in GNU grep ERE and Windows .NET regex.
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

fn run_code_label_query(
    code_engine: &engine::EngineAvailability,
    project: &str,
    label: &str,
    cache_dir: &str,
    max_results: usize,
) -> Result<Vec<serde_json::Value>, String> {
    let mut results = Vec::new();
    let mut offset = 0usize;

    loop {
        let page = run_code_query(
            code_engine,
            "search_graph",
            code_label_payload(project, label, cache_dir, offset),
        )?;
        let total = page
            .get("total")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| format!("코드 엔진 {label} 검색 응답에 total이 없습니다"))?
            as usize;
        if total > max_results {
            return Err(format!(
                "코드 노드가 안전 한도({MAX_CODE_NODES})를 초과했습니다: {label} {total}개"
            ));
        }

        let page_items = page
            .get("results")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| format!("코드 엔진 {label} 검색 응답에 results가 없습니다"))?;
        if results.len() + page_items.len() > max_results {
            return Err(format!(
                "코드 노드가 안전 한도({MAX_CODE_NODES})를 초과했습니다: {label}"
            ));
        }
        results.extend(page_items.iter().cloned());

        let has_more = page
            .get("has_more")
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| format!("코드 엔진 {label} 검색 응답에 has_more가 없습니다"))?;
        if !has_more {
            return Ok(results);
        }
        if page_items.is_empty() {
            return Err(format!(
                "코드 엔진 {label} 검색의 페이지 정보가 일관되지 않습니다"
            ));
        }
        offset = offset
            .checked_add(SEARCH_PAGE_SIZE)
            .ok_or_else(|| "코드 검색 offset이 범위를 벗어났습니다".to_string())?;
    }
}

pub(crate) fn code_label_payload(
    project: &str,
    label: &str,
    cache_dir: &str,
    offset: usize,
) -> serde_json::Value {
    serde_json::json!({
        "project": project,
        "label": label,
        "limit": SEARCH_PAGE_SIZE,
        "offset": offset,
        "cache_path": cache_dir,
        "cache_dir": cache_dir
    })
}

fn ensure_graph_result_below_limit(
    value: &serde_json::Value,
    relationship: &str,
) -> Result<(), String> {
    let total = value
        .get("total")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| format!("코드 엔진 {relationship} 응답에 total이 없습니다"))?;
    if total >= MAX_GRAPH_RELATIONSHIPS as u64 {
        Err(format!(
            "{relationship} 관계가 안전 한도({MAX_GRAPH_RELATIONSHIPS})에 도달해 결과가 잘렸을 수 있습니다"
        ))
    } else {
        Ok(())
    }
}

fn run_code_query(
    code_engine: &engine::EngineAvailability,
    tool: &str,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let cache_dir = payload
        .get("cache_dir")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let payload = payload.to_string();
    let args = engine::sidecar_args(["cli", tool, payload.as_str()])?;
    let run = if let Some(cache_dir) = cache_dir.as_deref() {
        engine::run_engine_command_with_env(
            code_engine,
            &args,
            Duration::from_secs(60),
            &[("CBM_CACHE_DIR", cache_dir)],
        )?
    } else {
        engine::run_engine_command(code_engine, &args, Duration::from_secs(60))?
    };

    if run.ok {
        engine_json_value(&run.stdout)
            .ok_or_else(|| format!("코드 엔진 {tool} 응답이 올바른 JSON이 아닙니다"))
    } else {
        Err(run.stderr)
    }
}

pub(crate) fn extract_code_inventory(
    project: String,
    architecture: Option<serde_json::Value>,
    routes_json: &serde_json::Value,
    services_json: &serde_json::Value,
    files_json: &serde_json::Value,
) -> Result<CodeInventory, String> {
    let mut seen = HashMap::new();
    let routes = extract_labeled_items(&project, routes_json, &["Route"], &mut seen)?;
    let code_items = extract_labeled_items(&project, services_json, CODE_LABELS, &mut seen)?;
    let files = extract_labeled_items(&project, files_json, &["File"], &mut seen)?;
    let handlers = category_items(&code_items, "handler");
    let normalized_services = category_items(&code_items, "service");
    let repositories = category_items(&code_items, "repository");
    let functions = category_items(&code_items, "function");
    let classes = category_items(&code_items, "class");
    let modules = category_items(&code_items, "module");
    let unknown = code_items
        .iter()
        .filter(|item| code_category(item) == "code")
        .cloned()
        .collect::<Vec<_>>();
    let summary = CodeInventorySummary {
        routes: routes.len(),
        handlers: handlers.len(),
        services: normalized_services.len(),
        repositories: repositories.len(),
        functions: functions.len(),
        classes: classes.len(),
        modules: modules.len(),
        files: files.len(),
        unknown: unknown.len(),
    };

    Ok(CodeInventory {
        project,
        routes,
        services: normalized_services,
        files,
        handlers,
        repositories,
        functions,
        classes,
        modules,
        unknown,
        summary,
        architecture,
        calls: Vec::new(),
        handles: Vec::new(),
    })
}

pub(crate) fn extract_code_calls(
    calls_json: &serde_json::Value,
    inventory: &CodeInventory,
) -> Vec<CodeCall> {
    let known_ids = inventory
        .routes
        .iter()
        .chain(inventory.handlers.iter())
        .chain(inventory.services.iter())
        .chain(inventory.repositories.iter())
        .chain(inventory.functions.iter())
        .chain(inventory.classes.iter())
        .chain(inventory.modules.iter())
        .chain(inventory.unknown.iter())
        .chain(inventory.files.iter())
        .map(|item| item.id.as_str())
        .collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let mut calls = graph_rows(calls_json)
        .into_iter()
        .filter_map(code_call)
        .filter(|call| {
            known_ids.contains(call.from.as_str()) && known_ids.contains(call.to.as_str())
        })
        .filter(|call| seen.insert((call.from.clone(), call.to.clone())))
        .collect::<Vec<_>>();
    calls.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| a.to.cmp(&b.to)));
    calls
}

pub(crate) fn extract_code_handles(
    handles_json: &serde_json::Value,
    inventory: &CodeInventory,
) -> Vec<CodeHandle> {
    let route_ids = inventory
        .routes
        .iter()
        .map(|item| item.id.as_str())
        .collect::<HashSet<_>>();
    let handler_ids = inventory
        .handlers
        .iter()
        .chain(inventory.services.iter())
        .chain(inventory.repositories.iter())
        .chain(inventory.functions.iter())
        .chain(inventory.classes.iter())
        .chain(inventory.modules.iter())
        .chain(inventory.unknown.iter())
        .map(|item| item.id.as_str())
        .collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let mut handles = graph_rows(handles_json)
        .into_iter()
        .filter_map(code_call)
        .filter_map(|edge| {
            (handler_ids.contains(edge.from.as_str()) && route_ids.contains(edge.to.as_str()))
                .then_some(CodeHandle {
                    handler: edge.from,
                    route: edge.to,
                })
        })
        .filter(|handle| seen.insert((handle.route.clone(), handle.handler.clone())))
        .collect::<Vec<_>>();
    handles.sort_by(|a, b| {
        a.route
            .cmp(&b.route)
            .then_with(|| a.handler.cmp(&b.handler))
    });
    handles
}

pub(crate) fn attach_code_handles(handles_json: &serde_json::Value, inventory: &mut CodeInventory) {
    let handles = extract_code_handles(handles_json, inventory);
    let handler_ids = handles
        .iter()
        .map(|handle| handle.handler.as_str())
        .collect::<HashSet<_>>();

    move_confirmed_handlers(
        &mut inventory.services,
        &mut inventory.handlers,
        &handler_ids,
    );
    move_confirmed_handlers(
        &mut inventory.repositories,
        &mut inventory.handlers,
        &handler_ids,
    );
    move_confirmed_handlers(
        &mut inventory.functions,
        &mut inventory.handlers,
        &handler_ids,
    );
    move_confirmed_handlers(
        &mut inventory.classes,
        &mut inventory.handlers,
        &handler_ids,
    );
    move_confirmed_handlers(
        &mut inventory.modules,
        &mut inventory.handlers,
        &handler_ids,
    );
    move_confirmed_handlers(
        &mut inventory.unknown,
        &mut inventory.handlers,
        &handler_ids,
    );
    let handled_routes = handles
        .iter()
        .map(|handle| handle.route.as_str())
        .collect::<HashSet<_>>();
    inventory.routes.sort_by(|left, right| {
        (!handled_routes.contains(left.id.as_str()), left.id.as_str()).cmp(&(
            !handled_routes.contains(right.id.as_str()),
            right.id.as_str(),
        ))
    });
    inventory.handlers.sort_by(|a, b| a.id.cmp(&b.id));
    inventory.handles = handles;
    inventory.summary = code_inventory_summary(inventory);
}

pub(crate) fn enrich_code_locations(
    locations_json: &serde_json::Value,
    inventory: &mut CodeInventory,
) -> Result<(), String> {
    let mut locations = HashMap::<
        String,
        (
            Option<String>,
            Option<u64>,
            Option<u64>,
            Option<u64>,
            Option<u64>,
        ),
    >::new();
    for row in graph_rows(locations_json) {
        let Some((qualified_name, path, line, column, end_line, end_column)) =
            source_location_row(row)
        else {
            continue;
        };
        let location = (path, line, column, end_line, end_column);
        if locations
            .insert(qualified_name.clone(), location.clone())
            .is_some_and(|existing| existing != location)
        {
            return Err(format!(
                "동일 qualified name에 서로 다른 소스 위치가 있습니다: {qualified_name}"
            ));
        }
    }

    for bucket in [
        &mut inventory.routes,
        &mut inventory.handlers,
        &mut inventory.services,
        &mut inventory.repositories,
        &mut inventory.functions,
        &mut inventory.classes,
        &mut inventory.modules,
        &mut inventory.unknown,
        &mut inventory.files,
    ] {
        for item in bucket {
            let Some((path, line, column, end_line, end_column)) =
                locations.get(&item.qualified_name)
            else {
                continue;
            };
            if item.file_path.is_none() {
                item.file_path = path.clone();
            }
            item.line = item.line.or(*line);
            item.column = item.column.or(*column);
            item.end_line = item.end_line.or(*end_line);
            item.end_column = item.end_column.or(*end_column);
        }
    }
    Ok(())
}

pub(crate) fn downgrade_unverified_routes(inventory: &mut CodeInventory) {
    let handled_routes = inventory
        .handles
        .iter()
        .map(|handle| handle.route.as_str())
        .collect::<HashSet<_>>();
    let (routes, mut unverified): (Vec<_>, Vec<_>) = std::mem::take(&mut inventory.routes)
        .into_iter()
        .partition(|route| route.file_path.is_some() || handled_routes.contains(route.id.as_str()));

    for route in &mut unverified {
        route.kind = "unknown".to_string();
    }
    inventory.routes = routes;
    inventory.unknown.append(&mut unverified);
    inventory
        .unknown
        .sort_by(|left, right| left.id.cmp(&right.id));
    inventory.summary = code_inventory_summary(inventory);
}

fn source_location_row(value: &serde_json::Value) -> Option<CodeSourceLocation> {
    if let Some(items) = value.as_array() {
        let (column, end_line, end_column) = if items.len() >= 6 {
            (
                items.get(3).and_then(positive_json_line),
                items.get(4).and_then(positive_json_line),
                items.get(5).and_then(positive_json_line),
            )
        } else {
            (None, items.get(3).and_then(positive_json_line), None)
        };
        return Some((
            items.first()?.as_str()?.to_string(),
            items
                .get(1)
                .and_then(serde_json::Value::as_str)
                .filter(|path| !path.is_empty())
                .map(str::to_string),
            items.get(2).and_then(positive_json_line),
            column,
            end_line,
            end_column,
        ));
    }

    let qualified_name = endpoint_string(value, &["source", "qualified_name", "qualifiedName"])?;
    let path =
        object_string(value, &["path", "file_path", "filePath"]).filter(|path| !path.is_empty());
    let line = ["start_line", "startLine", "line"]
        .iter()
        .find_map(|key| value.get(key).and_then(positive_json_line));
    let column = ["start_column", "startColumn", "column"]
        .iter()
        .find_map(|key| value.get(key).and_then(positive_json_line));
    let end_line = ["end_line", "endLine"]
        .iter()
        .find_map(|key| value.get(key).and_then(positive_json_line));
    let end_column = ["end_column", "endColumn"]
        .iter()
        .find_map(|key| value.get(key).and_then(positive_json_line));
    Some((qualified_name, path, line, column, end_line, end_column))
}

fn positive_json_line(value: &serde_json::Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|line| line.parse().ok()))
        .filter(|line| *line > 0)
}

fn move_confirmed_handlers(
    source: &mut Vec<CodeInventoryItem>,
    handlers: &mut Vec<CodeInventoryItem>,
    handler_ids: &HashSet<&str>,
) {
    let mut remaining = Vec::with_capacity(source.len());
    for item in std::mem::take(source) {
        if handler_ids.contains(item.id.as_str()) {
            handlers.push(item);
        } else {
            remaining.push(item);
        }
    }
    *source = remaining;
}

fn code_inventory_summary(inventory: &CodeInventory) -> CodeInventorySummary {
    CodeInventorySummary {
        routes: inventory.routes.len(),
        handlers: inventory.handlers.len(),
        services: inventory.services.len(),
        repositories: inventory.repositories.len(),
        functions: inventory.functions.len(),
        classes: inventory.classes.len(),
        modules: inventory.modules.len(),
        files: inventory.files.len(),
        unknown: inventory.unknown.len(),
    }
}

fn graph_rows(value: &serde_json::Value) -> Vec<&serde_json::Value> {
    if let Some(items) = value.as_array() {
        return items.iter().collect();
    }
    for key in ["items", "results", "rows", "data", "records"] {
        if let Some(items) = value.get(key).and_then(serde_json::Value::as_array) {
            return items.iter().collect();
        }
    }

    Vec::new()
}

fn code_call(value: &serde_json::Value) -> Option<CodeCall> {
    if let Some(items) = value.as_array() {
        let from = items.first().and_then(serde_json::Value::as_str)?;
        let to = items.get(1).and_then(serde_json::Value::as_str)?;
        return Some(CodeCall {
            from: from.to_string(),
            to: to.to_string(),
        });
    }

    let from = endpoint_string(
        value,
        &[
            "from",
            "caller",
            "source",
            "sourceQualifiedName",
            "source_qualified_name",
            "caller.qualified_name",
            "caller.qualifiedName",
            "a.qualified_name",
            "a.qualifiedName",
        ],
    )?;
    let to = endpoint_string(
        value,
        &[
            "to",
            "callee",
            "target",
            "targetQualifiedName",
            "target_qualified_name",
            "callee.qualified_name",
            "callee.qualifiedName",
            "b.qualified_name",
            "b.qualifiedName",
        ],
    )?;

    Some(CodeCall { from, to })
}

fn endpoint_string(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        let candidate = value.get(key)?;
        candidate.as_str().map(str::to_string).or_else(|| {
            object_string(
                candidate,
                &["qualifiedName", "qualified_name", "id", "name"],
            )
        })
    })
}

fn extract_labeled_items(
    project: &str,
    value: &serde_json::Value,
    allowed_labels: &[&str],
    seen: &mut HashMap<String, String>,
) -> Result<Vec<CodeInventoryItem>, String> {
    let mut items = Vec::new();
    for value in value_items(value) {
        let item = code_item(project, value)?;
        if !allowed_labels.contains(&item.engine_label.as_str()) {
            return Err(format!(
                "코드 엔진 label 계약이 일치하지 않습니다: expected {}, got {} ({})",
                allowed_labels.join("|"),
                item.engine_label,
                item.qualified_name
            ));
        }
        if is_obvious_inventory_noise(&item) {
            continue;
        }

        match seen.get(&item.qualified_name) {
            Some(label) if label == &item.engine_label => continue,
            Some(label) => {
                return Err(format!(
                    "동일 qualified name에 서로 다른 label이 있습니다: {} ({label}, {})",
                    item.qualified_name, item.engine_label
                ));
            }
            None => {
                seen.insert(item.qualified_name.clone(), item.engine_label.clone());
                items.push(item);
            }
        }
    }
    items.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(items)
}

fn is_obvious_inventory_noise(item: &CodeInventoryItem) -> bool {
    (item.engine_label == "Route" && item.name.contains("://"))
        || (item.engine_label == "Decorator"
            && item.name.starts_with("#[")
            && !item.name.ends_with(']'))
}

fn code_item(project: &str, value: &serde_json::Value) -> Result<CodeInventoryItem, String> {
    let qualified_name = object_string(value, &["qualifiedName", "qualified_name", "id"])
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "코드 엔진 노드에 qualified_name이 없습니다".to_string())?;
    let engine_label = object_string(value, &["label", "kind"])
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("코드 엔진 노드에 label이 없습니다: {qualified_name}"))?;
    let name = object_string(value, &["name"])
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| qualified_name.clone());
    let file_path = object_string(value, &["filePath", "file_path", "path"])
        .filter(|value| !value.trim().is_empty());

    Ok(CodeInventoryItem {
        id: qualified_name.clone(),
        kind: engine_label.clone(),
        name,
        project: project.to_string(),
        qualified_name,
        engine_label,
        file_path,
        line: positive_line(value, &["startLine", "start_line", "line"]),
        column: positive_line(value, &["startColumn", "start_column", "column"]),
        end_line: positive_line(value, &["endLine", "end_line"]),
        end_column: positive_line(value, &["endColumn", "end_column"]),
        detail: value.clone(),
    })
}

fn positive_line(value: &serde_json::Value, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| value.get(key).and_then(serde_json::Value::as_u64))
        .filter(|line| *line > 0)
}

fn category_items(items: &[CodeInventoryItem], category: &str) -> Vec<CodeInventoryItem> {
    items
        .iter()
        .filter(|item| code_category(item) == category)
        .cloned()
        .collect()
}

fn code_category(item: &CodeInventoryItem) -> &'static str {
    let kind = item.engine_label.to_ascii_lowercase();
    if matches!(
        kind.as_str(),
        "function" | "method" | "constructor" | "subroutine" | "procedure"
    ) {
        "function"
    } else if matches!(
        kind.as_str(),
        "class"
            | "struct"
            | "interface"
            | "trait"
            | "protocol"
            | "record"
            | "enum"
            | "type"
            | "union"
    ) {
        class_role(&item.name).unwrap_or("class")
    } else if matches!(kind.as_str(), "module" | "package" | "namespace") {
        "module"
    } else {
        "code"
    }
}

fn class_role(name: &str) -> Option<&'static str> {
    let compact = name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase();
    let role_name = compact.strip_suffix("impl").unwrap_or(&compact);

    if role_name.ends_with("handler") || role_name.ends_with("controller") {
        Some("handler")
    } else if role_name.ends_with("repository")
        || role_name.ends_with("repo")
        || role_name.ends_with("dao")
    {
        Some("repository")
    } else if role_name.ends_with("service") {
        Some("service")
    } else {
        None
    }
}

pub(crate) fn code_index_payload(workspace: &Workspace, code_cache_path: &Path) -> String {
    let code_cache_path = code_cache_path.display().to_string();
    serde_json::json!({
        "path": workspace.repo_path,
        "repo_path": workspace.repo_path,
        "project": workspace.name,
        "project_name": workspace.name,
        "cache_path": code_cache_path,
        "cache_dir": code_cache_path,
        "workspace_cache_dir": code_cache_path
    })
    .to_string()
}

pub(crate) fn code_project_from_index_stdout(stdout: &str, fallback: &str) -> String {
    engine_json_value(stdout)
        .and_then(|value| object_string(&value, &["project"]))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| fallback.to_string())
}
