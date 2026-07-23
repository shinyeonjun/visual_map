use crate::paths::base_paths;
use crate::EngineRegistry;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use super::codebase_memory::{CodebaseMemoryAdapter, CodebaseMemoryInventory, CODE_NODE_LABELS};
use super::model::{
    CodeCall, CodeHandle, CodeIndexResult, CodeInventory, CodeInventoryItem, CodeInventorySummary,
    FocusedCodeSearch, IndexCodeRequest,
};
use super::store::{
    engine_json_value, object_string, read_workspace_by_id, timestamp, validate_workspace_id,
    value_items, workspace_code_cache_path, workspace_db_cache_dir, write_workspace,
};

static NEXT_CODE_PROJECT_GENERATION: AtomicU64 = AtomicU64::new(0);

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
    let adapter = CodebaseMemoryAdapter::new(registry, &code_cache_path)?;
    let previous_project = workspace
        .code_project
        .clone()
        .unwrap_or_else(|| workspace.name.clone());
    let requested_project = next_code_project_generation();
    let mut run = adapter.index_repository(&workspace.repo_path, &requested_project)?;
    let mut inventory = None;
    let mut inventory_error = None;

    if run.ok {
        let project = code_project_from_index_stdout(&run.stdout, &requested_project);
        match code_inventory_from_adapter(&adapter, project.clone(), &workspace.repo_path) {
            Ok(indexed_inventory) => {
                workspace.code_project = Some(project.clone());
                workspace.updated_at = timestamp();
                if let Err(error) = write_workspace(&paths.workspaces_dir, &workspace) {
                    let _ = adapter.delete_project(&project);
                    return Err(error);
                }
                if previous_project != project {
                    let _ = adapter.delete_project(&previous_project);
                }
                inventory = Some(indexed_inventory);
            }
            Err(error) => {
                let _ = adapter.delete_project(&project);
                run.ok = false;
                run.stderr = format!("새 코드 인덱스를 검증하지 못했습니다: {error}");
                inventory_error = Some(error);
            }
        }
    } else {
        let project = code_project_from_index_stdout(&run.stdout, &requested_project);
        let _ = adapter.delete_project(&project);
    }

    Ok(CodeIndexResult {
        workspace,
        run,
        inventory,
        inventory_error,
    })
}

pub(crate) fn code_inventory(
    app_data_dir: impl AsRef<Path>,
    registry: &EngineRegistry,
    workspace_id: &str,
) -> Result<CodeInventory, String> {
    validate_workspace_id(workspace_id)?;

    let paths = base_paths(app_data_dir);
    let workspace = read_workspace_by_id(&paths.workspaces_dir, workspace_id)?;
    let code_cache_path = workspace_code_cache_path(&paths.workspaces_dir, workspace_id);
    let project = workspace
        .code_project
        .clone()
        .unwrap_or_else(|| workspace.name.clone());
    let adapter = CodebaseMemoryAdapter::new(registry, code_cache_path)?;
    code_inventory_from_adapter(&adapter, project, &workspace.repo_path)
}

fn code_inventory_from_adapter(
    adapter: &CodebaseMemoryAdapter<'_>,
    project: String,
    repo_path: &str,
) -> Result<CodeInventory, String> {
    let result: CodebaseMemoryInventory = adapter.inventory(&project)?;
    let (routes, services, files) = split_inventory_nodes(&result.nodes)?;
    let mut inventory = extract_code_inventory(
        project,
        Some(result.architecture),
        &routes,
        &services,
        &files,
    )?;
    inventory.calls = extract_code_calls(&result.calls, &inventory);
    attach_code_handles(&result.handles, &mut inventory);
    super::fastapi_routes::enrich_fastapi_route_paths(repo_path, &mut inventory);
    super::fastendpoints_routes::enrich_fastendpoints_routes(repo_path, &mut inventory);
    downgrade_unverified_routes(&mut inventory);
    Ok(inventory)
}

pub(crate) fn next_code_project_generation() -> String {
    let sequence = NEXT_CODE_PROJECT_GENERATION.fetch_add(1, Ordering::Relaxed);
    format!(
        "visual-map-{}-{}-{}",
        std::process::id(),
        timestamp(),
        sequence
    )
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
    let code_cache_path = workspace_code_cache_path(&paths.workspaces_dir, workspace_id);
    let project = workspace
        .code_project
        .as_deref()
        .unwrap_or(workspace.name.as_str());
    CodebaseMemoryAdapter::new(registry, code_cache_path)?.search_code(
        project,
        identifier,
        path_filter,
        requested_limit,
    )
}

pub(crate) fn split_inventory_nodes(
    nodes: &serde_json::Value,
) -> Result<(serde_json::Value, serde_json::Value, serde_json::Value), String> {
    let items = nodes
        .get("results")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "정규화된 코드 노드 응답에 results가 없습니다".to_string())?;
    let mut routes = Vec::new();
    let mut code = Vec::new();
    let mut files = Vec::new();

    for item in items {
        let label = object_string(item, &["label"])
            .ok_or_else(|| "정규화된 코드 노드에 label이 없습니다".to_string())?;
        match label.as_str() {
            "Route" => routes.push(item.clone()),
            "File" => files.push(item.clone()),
            label if CODE_NODE_LABELS.contains(&label) => code.push(item.clone()),
            _ => return Err(format!("허용되지 않은 코드 노드 label입니다: {label}")),
        }
    }

    Ok((
        serde_json::json!({ "total": routes.len(), "results": routes, "has_more": false }),
        serde_json::json!({ "total": code.len(), "results": code, "has_more": false }),
        serde_json::json!({ "total": files.len(), "results": files, "has_more": false }),
    ))
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
    let code_items = extract_labeled_items(&project, services_json, CODE_NODE_LABELS, &mut seen)?;
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
        partial: false,
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
    let mut calls_by_pair = HashMap::<(String, String), CodeCall>::new();
    for call in graph_rows(calls_json)
        .into_iter()
        .filter_map(code_call)
        .filter(|call| {
            known_ids.contains(call.from.as_str()) && known_ids.contains(call.to.as_str())
        })
    {
        let key = (call.from.clone(), call.to.clone());
        match calls_by_pair.entry(key) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(call);
            }
            std::collections::hash_map::Entry::Occupied(mut entry)
                if code_call_rank(&call) > code_call_rank(entry.get()) =>
            {
                entry.insert(call);
            }
            _ => {}
        }
    }
    let mut calls = calls_by_pair.into_values().collect::<Vec<_>>();
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
    attach_route_handles(handles, inventory);
}

pub(super) fn attach_route_handles(mut handles: Vec<CodeHandle>, inventory: &mut CodeInventory) {
    handles.sort_by(|left, right| {
        left.route
            .cmp(&right.route)
            .then_with(|| left.handler.cmp(&right.handler))
    });
    handles.dedup();
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
    inventory.handlers.sort_by(|a, b| a.id.cmp(&b.id));
    let handles = normalize_route_bindings(&mut inventory.routes, &inventory.handlers, &handles);
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
    inventory.handles = handles;
    inventory.summary = code_inventory_summary(inventory);
}

fn normalize_route_bindings(
    routes: &mut Vec<CodeInventoryItem>,
    handlers: &[CodeInventoryItem],
    handles: &[CodeHandle],
) -> Vec<CodeHandle> {
    let handler_by_id = handlers
        .iter()
        .map(|handler| (handler.id.as_str(), handler))
        .collect::<HashMap<_, _>>();
    let mut handlers_by_route = HashMap::<&str, Vec<&str>>::new();
    for handle in handles {
        handlers_by_route
            .entry(handle.route.as_str())
            .or_default()
            .push(handle.handler.as_str());
    }
    for handler_ids in handlers_by_route.values_mut() {
        handler_ids.sort_unstable();
        handler_ids.dedup();
    }

    let mut normalized_routes = Vec::with_capacity(routes.len().max(handles.len()));
    let mut normalized_handles = Vec::with_capacity(handles.len());
    for mut route in std::mem::take(routes) {
        let Some(handler_ids) = handlers_by_route.get(route.id.as_str()) else {
            normalized_routes.push(route);
            continue;
        };

        if handler_ids.len() == 1 {
            let handler_id = handler_ids[0];
            hydrate_route_binding(&mut route, handler_by_id.get(handler_id).copied(), false);
            normalized_handles.push(CodeHandle {
                handler: handler_id.to_string(),
                route: route.id.clone(),
            });
            normalized_routes.push(route);
            continue;
        }

        for handler_id in handler_ids {
            let mut binding = route.clone();
            binding.id = route_binding_id(&route.id, handler_id);
            binding.qualified_name = binding.id.clone();
            hydrate_route_binding(&mut binding, handler_by_id.get(handler_id).copied(), true);
            normalized_handles.push(CodeHandle {
                handler: (*handler_id).to_string(),
                route: binding.id.clone(),
            });
            normalized_routes.push(binding);
        }
    }
    normalized_routes.sort_by(|left, right| left.id.cmp(&right.id));
    normalized_handles.sort_by(|left, right| {
        left.route
            .cmp(&right.route)
            .then_with(|| left.handler.cmp(&right.handler))
    });
    *routes = normalized_routes;
    normalized_handles
}

fn hydrate_route_binding(
    route: &mut CodeInventoryItem,
    handler: Option<&CodeInventoryItem>,
    prefer_handler_location: bool,
) {
    let Some(handler) = handler else {
        return;
    };
    if let Some(path) = object_string(&handler.detail, &["routePath", "route_path"])
        .filter(|value| !value.trim().is_empty())
    {
        route.name = path;
    }
    if prefer_handler_location || route.file_path.is_none() {
        route.file_path = handler.file_path.clone();
        route.line = handler.line;
        route.column = handler.column;
        route.end_line = handler.end_line;
        route.end_column = handler.end_column;
    }
}

pub(crate) fn route_binding_id(route_id: &str, handler_id: &str) -> String {
    format!("{route_id}#handler={handler_id}")
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
            confidence: items.get(2).and_then(code_call_confidence),
            strategy: items.get(3).and_then(optional_json_string),
            expression: items.get(4).and_then(optional_json_string),
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

    Some(CodeCall {
        from,
        to,
        confidence: value.get("confidence").and_then(code_call_confidence),
        strategy: value.get("strategy").and_then(optional_json_string),
        expression: value
            .get("call_expression")
            .or_else(|| value.get("callExpression"))
            .and_then(optional_json_string),
    })
}

fn code_call_rank(call: &CodeCall) -> (u8, &str, &str) {
    (
        call.confidence.unwrap_or(0),
        call.strategy.as_deref().unwrap_or_default(),
        call.expression.as_deref().unwrap_or_default(),
    )
}

fn code_call_confidence(value: &serde_json::Value) -> Option<u8> {
    let value = value
        .as_f64()
        .or_else(|| value.as_str()?.parse::<f64>().ok())?;
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    let percent = if value <= 1.0 { value * 100.0 } else { value };
    (percent <= 100.0).then(|| percent.round() as u8)
}

fn optional_json_string(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
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
        .find_map(|key| value.get(key).and_then(positive_json_line))
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

pub(crate) fn code_project_from_index_stdout(stdout: &str, fallback: &str) -> String {
    engine_json_value(stdout)
        .and_then(|value| object_string(&value, &["project"]))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| fallback.to_string())
}
