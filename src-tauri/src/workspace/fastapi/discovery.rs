use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs,
    path::Path,
};

use super::model::{CodeInventory, CodeInventoryGap, CodeInventoryItem};

const MAX_PYTHON_SOURCE_BYTES: u64 = 2 * 1024 * 1024;
const HTTP_METHODS: &[&str] = &[
    "delete", "get", "head", "options", "patch", "post", "put", "trace",
];

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct RouterKey {
    module: String,
    symbol: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StaticPath {
    Known(String),
    Dynamic,
}

#[derive(Debug, Clone)]
struct LogicalStatement {
    start_line: u64,
    end_line: u64,
    text: String,
}

#[derive(Debug, Clone)]
struct ModuleSource {
    module: String,
    statements: Vec<LogicalStatement>,
    routers: HashMap<String, StaticPath>,
    applications: HashSet<String>,
    imports: HashMap<String, RouterKey>,
    includes: Vec<RouterInclude>,
}

#[derive(Debug, Clone)]
struct RouterInclude {
    parent: String,
    child: String,
    prefix: StaticPath,
}

#[derive(Debug, Clone)]
enum MountParent {
    Root,
    Router(RouterKey),
}

#[derive(Debug, Clone)]
struct MountEdge {
    parent: MountParent,
    prefix: StaticPath,
}

#[derive(Debug, Clone)]
struct FastApiGraph {
    modules: BTreeMap<String, ModuleSource>,
    module_by_path: HashMap<String, String>,
    incoming: HashMap<RouterKey, Vec<MountEdge>>,
}

#[derive(Debug, Default)]
struct MountResolution {
    prefixes: BTreeSet<String>,
    uncertain: bool,
    rooted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MountedRoutePath {
    local: String,
    mounted: String,
}

pub(super) fn enrich_fastapi_evidence(repo_path: &str, inventory: &mut CodeInventory) {
    let report = read_python_sources(repo_path, inventory);
    for skipped in report.skipped {
        inventory.relation_gaps.push(CodeInventoryGap::new(
            "provider-source-scan",
            "provider:fastapi",
            format!("file:{skipped}"),
            "FastAPI 보강 분석이 소스 파일을 읽지 못했습니다. 해당 파일에서 라우터 경로와 호출 관계를 확정하지 않았습니다.",
        ));
    }
    if report.sources.is_empty() {
        return;
    }

    let graph = FastApiGraph::from_sources(&report.sources);
    enrich_fastapi_route_paths_from_graph(&graph, inventory);
    enrich_fastapi_import_calls(&graph, inventory);
}

#[derive(Debug, Default)]
struct PythonSourceReport {
    sources: BTreeMap<String, String>,
    skipped: Vec<String>,
}

fn read_python_sources(repo_path: &str, inventory: &CodeInventory) -> PythonSourceReport {
    let Some(root) = Path::new(repo_path).canonicalize().ok() else {
        return PythonSourceReport::default();
    };
    let mut paths = BTreeSet::new();
    for item in inventory
        .routes
        .iter()
        .chain(inventory.handlers.iter())
        .chain(inventory.services.iter())
        .chain(inventory.repositories.iter())
        .chain(inventory.functions.iter())
        .chain(inventory.classes.iter())
        .chain(inventory.modules.iter())
        .chain(inventory.unknown.iter())
    {
        if let Some(path) = item
            .file_path
            .as_deref()
            .filter(|path| is_python_path(path))
        {
            paths.insert(path.to_string());
        }
    }
    for item in &inventory.files {
        let path = item
            .file_path
            .as_deref()
            .or_else(|| is_python_path(&item.name).then_some(item.name.as_str()));
        if let Some(path) = path.filter(|path| is_python_path(path)) {
            paths.insert(path.to_string());
        }
    }

    let mut report = PythonSourceReport::default();
    for path in paths {
        if path.starts_with('<') {
            continue;
        }
        let resolved = match crate::source::resolve_repo_source(repo_path, &path) {
            Ok(resolved) => resolved,
            Err(_) => {
                report.skipped.push(path);
                continue;
            }
        };
        let metadata = match resolved.metadata() {
            Ok(metadata) => metadata,
            Err(_) => {
                report.skipped.push(path);
                continue;
            }
        };
        if metadata.len() > MAX_PYTHON_SOURCE_BYTES {
            report.skipped.push(path);
            continue;
        }
        let source = match fs::read_to_string(&resolved) {
            Ok(source) => source,
            Err(_) => {
                report.skipped.push(path);
                continue;
            }
        };
        if !is_fastapi_source_candidate(&path, &source) {
            continue;
        }
        let Some(relative) = resolved
            .strip_prefix(&root)
            .ok()
            .map(|path| normalize_source_path(&path.to_string_lossy()))
        else {
            report.skipped.push(path);
            continue;
        };
        report.sources.insert(relative, source);
    }
    report
}

#[cfg(test)]
fn enrich_fastapi_route_paths_from_sources(
    sources: &BTreeMap<String, String>,
    inventory: &mut CodeInventory,
) {
    let graph = FastApiGraph::from_sources(sources);
    enrich_fastapi_route_paths_from_graph(&graph, inventory);
}

fn enrich_fastapi_route_paths_from_graph(graph: &FastApiGraph, inventory: &mut CodeInventory) {
    let handlers = inventory
        .handlers
        .iter()
        .map(|handler| (handler.id.as_str(), handler))
        .collect::<HashMap<_, _>>();
    let handler_by_route = inventory
        .handles
        .iter()
        .map(|handle| (handle.route.as_str(), handle.handler.as_str()))
        .collect::<HashMap<_, _>>();

    for route in &mut inventory.routes {
        let Some(handler) = handler_by_route
            .get(route.id.as_str())
            .and_then(|handler_id| handlers.get(handler_id).copied())
        else {
            continue;
        };
        let Some(path) = handler.file_path.as_deref() else {
            continue;
        };
        let Some(line) = handler.line else {
            continue;
        };
        let Some(method) = route_method(route, handler) else {
            continue;
        };
        let local_path = route_local_path(route, handler);
        let Some(resolved_path) =
            graph.mounted_route_path(path, line, &method, local_path.as_str())
        else {
            continue;
        };
        if resolved_path.mounted == local_path {
            continue;
        }

        if let Some(detail) = route.detail.as_object_mut() {
            detail.insert(
                "localRoutePath".to_string(),
                serde_json::Value::String(resolved_path.local),
            );
            detail.insert(
                "mountedRoutePath".to_string(),
                serde_json::Value::String(resolved_path.mounted.clone()),
            );
            detail.insert(
                "routePathSource".to_string(),
                serde_json::Value::String("fastapi-static-mount".to_string()),
            );
        }
        route.name = resolved_path.mounted;
    }
}

fn enrich_fastapi_import_calls(graph: &FastApiGraph, inventory: &mut CodeInventory) {
    let python_items = inventory
        .handlers
        .iter()
        .chain(inventory.services.iter())
        .chain(inventory.repositories.iter())
        .chain(inventory.functions.iter())
        .chain(inventory.classes.iter())
        .chain(inventory.modules.iter())
        .chain(inventory.unknown.iter())
        .filter_map(|item| {
            let path = item.file_path.as_deref()?;
            let (module, _) = python_module(path)?;
            Some((
                item.id.clone(),
                (path.to_string(), module, item.name.clone()),
            ))
        })
        .collect::<HashMap<_, _>>();

    for call in &mut inventory.calls {
        if call.confidence.is_some_and(|confidence| confidence >= 85)
            || call.strategy.as_deref() != Some("unique_name")
        {
            continue;
        }
        let Some((caller_path, _, _)) = python_items.get(&call.from) else {
            continue;
        };
        let Some((_, target_module, target_name)) = python_items.get(&call.to) else {
            continue;
        };
        let Some(source_module) = graph
            .module_for_path(caller_path)
            .and_then(|module| graph.modules.get(module))
        else {
            continue;
        };
        let Some((alias, imported, symbol)) = call
            .expression
            .as_deref()
            .and_then(|expression| imported_member_target(source_module, expression))
        else {
            continue;
        };
        let expected_module = format!("{}.{}", imported.module, imported.symbol);
        if alias_is_rebound(source_module, &alias)
            || target_name != &symbol
            || !module_matches(target_module, &expected_module)
        {
            continue;
        }

        call.confidence = Some(95);
        call.strategy = Some("python_static_import".to_string());
    }
}

fn imported_member_target(
    module: &ModuleSource,
    expression: &str,
) -> Option<(String, RouterKey, String)> {
    let (alias, symbol) = expression.trim().split_once('.')?;
    if !is_identifier(alias) || !is_identifier(symbol) {
        return None;
    }
    let imported = module.imports.get(alias)?;
    Some((alias.to_string(), imported.clone(), symbol.to_string()))
}

fn alias_is_rebound(module: &ModuleSource, alias: &str) -> bool {
    module
        .statements
        .iter()
        .filter(|statement| from_import_binds_alias(&statement.text, alias))
        .count()
        != 1
        || module.statements.iter().any(|statement| {
            let statement = statement.text.trim();
            if statement.starts_with("from ") {
                return false;
            }
            let assigned = split_assignment(statement).is_some_and(|(left, _)| {
                left.split(|character: char| character != '_' && !character.is_ascii_alphanumeric())
                    .any(|name| name == alias)
            });
            let parameter = is_function_definition(statement)
                && call_args(statement).is_some_and(|arguments| {
                    split_top_level(arguments, ',').into_iter().any(|argument| {
                        argument
                            .trim()
                            .trim_start_matches('*')
                            .split([':', '='])
                            .next()
                            .is_some_and(|name| name.trim() == alias)
                    })
                });
            assigned
                || parameter
                || statement.starts_with(&format!("for {alias} "))
                || statement.contains(&format!(" for {alias} "))
                || statement.contains(&format!("lambda {alias}"))
                || statement.contains(&format!(" as {alias}"))
                || statement == format!("global {alias}")
                || statement == format!("nonlocal {alias}")
                || statement == format!("del {alias}")
        })
}

fn from_import_binds_alias(statement: &str, alias: &str) -> bool {
    let Some((_, imports)) = statement
        .trim()
        .strip_prefix("from ")
        .and_then(|statement| statement.split_once(" import "))
    else {
        return false;
    };
    let imports = imports
        .trim()
        .strip_prefix('(')
        .and_then(|imports| imports.strip_suffix(')'))
        .unwrap_or(imports);
    split_top_level(imports, ',').into_iter().any(|import| {
        import
            .trim()
            .split_once(" as ")
            .map_or(import.trim(), |(_, alias)| alias.trim())
            == alias
    })
}

fn module_matches(actual: &str, expected: &str) -> bool {
    actual == expected
        || actual
            .strip_suffix(expected)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

