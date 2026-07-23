use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs,
    path::Path,
};

use super::model::{CodeInventory, CodeInventoryItem};

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
    let sources = read_python_sources(repo_path, inventory);
    if sources.is_empty() {
        return;
    }

    let graph = FastApiGraph::from_sources(&sources);
    enrich_fastapi_route_paths_from_graph(&graph, inventory);
    enrich_fastapi_import_calls(&graph, inventory);
}

fn read_python_sources(repo_path: &str, inventory: &CodeInventory) -> BTreeMap<String, String> {
    let Some(root) = Path::new(repo_path).canonicalize().ok() else {
        return BTreeMap::new();
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

    paths
        .into_iter()
        .filter_map(|path| {
            let resolved = crate::source::resolve_repo_source(repo_path, &path).ok()?;
            let metadata = resolved.metadata().ok()?;
            if metadata.len() > MAX_PYTHON_SOURCE_BYTES {
                return None;
            }
            let source = fs::read_to_string(&resolved).ok()?;
            if !is_fastapi_source_candidate(&path, &source) {
                return None;
            }
            let relative = resolved
                .strip_prefix(&root)
                .ok()?
                .to_string_lossy()
                .into_owned();
            Some((normalize_source_path(&relative), source))
        })
        .collect()
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

impl FastApiGraph {
    fn from_sources(sources: &BTreeMap<String, String>) -> Self {
        let mut modules = BTreeMap::new();
        let mut module_by_path = HashMap::new();

        for (path, source) in sources {
            let path = normalize_source_path(path);
            let Some((module, is_package)) = python_module(&path) else {
                continue;
            };
            let parsed = parse_module(module.clone(), is_package, source);
            module_by_path.insert(path, module.clone());
            modules.insert(module, parsed);
        }

        let mut graph = Self {
            modules,
            module_by_path,
            incoming: HashMap::new(),
        };
        graph.build_mount_edges();
        graph
    }

    fn build_mount_edges(&mut self) {
        let mut incoming = HashMap::<RouterKey, Vec<MountEdge>>::new();

        for module in self.modules.values() {
            for include in &module.includes {
                let Some(child) = self.resolve_symbol(&module.module, &include.child) else {
                    continue;
                };
                let parent =
                    if let Some(parent) = self.resolve_symbol(&module.module, &include.parent) {
                        MountParent::Router(parent)
                    } else if module.applications.contains(&include.parent) {
                        MountParent::Root
                    } else {
                        continue;
                    };
                incoming.entry(child).or_default().push(MountEdge {
                    parent,
                    prefix: include.prefix.clone(),
                });
            }
        }
        self.incoming = incoming;
    }

    fn mounted_route_path(
        &self,
        source_path: &str,
        handler_line: u64,
        method: &str,
        local_path: &str,
    ) -> Option<MountedRoutePath> {
        let path = normalize_source_path(source_path);
        let module_name = self.module_for_path(&path)?;
        let module = self.modules.get(module_name)?;
        let (router_symbol, source_local_path) =
            route_router_symbol(module, handler_line, method, local_path)?;
        let router = self.resolve_symbol(module_name, &router_symbol)?;
        let resolution = self.resolve_mount(&router, &mut HashSet::new());
        if resolution.uncertain || !resolution.rooted || resolution.prefixes.len() != 1 {
            return None;
        }
        let prefix = resolution.prefixes.iter().next()?;
        Some(MountedRoutePath {
            local: source_local_path.clone(),
            mounted: join_url_path(prefix, &source_local_path),
        })
    }

    fn module_for_path(&self, requested: &str) -> Option<&String> {
        if let Some(module) = self.module_by_path.get(requested) {
            return Some(module);
        }
        let suffix = format!("/{requested}");
        let mut matches = self
            .module_by_path
            .iter()
            .filter(|(path, _)| requested.ends_with(&format!("/{path}")) || path.ends_with(&suffix))
            .map(|(_, module)| module);
        let found = matches.next()?;
        matches.next().is_none().then_some(found)
    }

    fn resolve_symbol(&self, module_name: &str, expression: &str) -> Option<RouterKey> {
        let symbol = expression.trim();
        if !is_dotted_identifier(symbol) {
            return None;
        }
        let module_name = self.canonical_module(module_name)?;
        let module = self.modules.get(&module_name)?;

        if module.routers.contains_key(symbol) {
            return Some(RouterKey {
                module: module_name,
                symbol: symbol.to_string(),
            });
        }
        let imported = module.imports.get(symbol)?;
        self.resolve_imported(imported, &mut HashSet::new())
    }

    fn resolve_imported(
        &self,
        key: &RouterKey,
        seen: &mut HashSet<RouterKey>,
    ) -> Option<RouterKey> {
        let module_name = self.canonical_module(&key.module)?;
        let normalized = RouterKey {
            module: module_name.clone(),
            symbol: key.symbol.clone(),
        };
        if !seen.insert(normalized.clone()) {
            return None;
        }
        let module = self.modules.get(&module_name)?;
        if module.routers.contains_key(&key.symbol) {
            return Some(normalized);
        }
        let imported = module.imports.get(&key.symbol)?;
        self.resolve_imported(imported, seen)
    }

    fn canonical_module(&self, requested: &str) -> Option<String> {
        if self.modules.contains_key(requested) {
            return Some(requested.to_string());
        }
        let suffix = format!(".{requested}");
        let mut matches = self
            .modules
            .keys()
            .filter(|module| module.ends_with(&suffix));
        let found = matches.next()?.clone();
        matches.next().is_none().then_some(found)
    }

    fn resolve_mount(&self, router: &RouterKey, stack: &mut HashSet<RouterKey>) -> MountResolution {
        if !stack.insert(router.clone()) {
            return MountResolution {
                uncertain: true,
                ..MountResolution::default()
            };
        }

        let own_prefix = self
            .modules
            .get(&router.module)
            .and_then(|module| module.routers.get(&router.symbol));
        let Some(own_prefix) = own_prefix else {
            stack.remove(router);
            return MountResolution {
                uncertain: true,
                ..MountResolution::default()
            };
        };
        let StaticPath::Known(own_prefix) = own_prefix else {
            stack.remove(router);
            return MountResolution {
                uncertain: true,
                ..MountResolution::default()
            };
        };

        let mut result = MountResolution::default();
        let incoming = self.incoming.get(router);
        if incoming.is_none_or(Vec::is_empty) {
            result.prefixes.insert(normalize_url_prefix(own_prefix));
        } else if let Some(incoming) = incoming {
            for edge in incoming {
                let StaticPath::Known(include_prefix) = &edge.prefix else {
                    result.uncertain = true;
                    continue;
                };
                match &edge.parent {
                    MountParent::Root => {
                        result.rooted = true;
                        result.prefixes.insert(join_url_path(
                            &normalize_url_prefix(include_prefix),
                            own_prefix,
                        ));
                    }
                    MountParent::Router(parent) => {
                        let parent_resolution = self.resolve_mount(parent, stack);
                        result.uncertain |= parent_resolution.uncertain;
                        result.rooted |= parent_resolution.rooted;
                        for parent_prefix in parent_resolution.prefixes {
                            let mounted = join_url_path(&parent_prefix, include_prefix);
                            result.prefixes.insert(join_url_path(&mounted, own_prefix));
                        }
                    }
                }
            }
        }

        stack.remove(router);
        result
    }
}

fn parse_module(module: String, is_package: bool, source: &str) -> ModuleSource {
    let statements = logical_statements(source);
    let mut routers = HashMap::new();
    let mut applications = HashSet::new();
    let mut imports = HashMap::new();
    let mut includes = Vec::new();

    for statement in &statements {
        if let Some(imported) = parse_from_import(&module, is_package, &statement.text) {
            imports.extend(imported);
            continue;
        }
        if let Some((symbol, prefix)) = parse_router_definition(&statement.text) {
            routers.insert(symbol, prefix);
            continue;
        }
        applications.extend(parse_fastapi_applications(&statement.text));
        if let Some(include) = parse_router_include(&statement.text) {
            includes.push(include);
        }
    }

    ModuleSource {
        module,
        statements,
        routers,
        applications,
        imports,
        includes,
    }
}

fn parse_from_import(
    current_module: &str,
    is_package: bool,
    statement: &str,
) -> Option<Vec<(String, RouterKey)>> {
    let statement = statement.trim();
    let rest = statement.strip_prefix("from ")?;
    let (module_ref, imports) = rest.split_once(" import ")?;
    let resolved_module = resolve_import_module(current_module, is_package, module_ref.trim())?;
    let imports = imports
        .trim()
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'))
        .unwrap_or(imports.trim());

    let mut parsed = Vec::new();
    for imported in split_top_level(imports, ',') {
        let imported = imported.trim();
        if imported.is_empty() || imported == "*" {
            continue;
        }
        let (symbol, alias) = imported
            .split_once(" as ")
            .map_or((imported, imported), |(symbol, alias)| {
                (symbol.trim(), alias.trim())
            });
        if is_identifier(symbol) && is_identifier(alias) {
            parsed.push((
                alias.to_string(),
                RouterKey {
                    module: resolved_module.clone(),
                    symbol: symbol.to_string(),
                },
            ));
        }
    }

    Some(parsed)
}

fn parse_fastapi_applications(statement: &str) -> Vec<String> {
    if let Some((left, right)) = split_assignment(statement) {
        let call = right.trim();
        if call
            .find('(')
            .and_then(|open| call[..open].trim().rsplit('.').next())
            == Some("FastAPI")
        {
            let symbol = left.split(':').next().and_then(|left| {
                let symbol = left.split_whitespace().last()?;
                is_identifier(symbol).then(|| symbol.to_string())
            });
            return symbol.into_iter().collect();
        }
    }

    let statement = statement.trim_start();
    let definition = statement
        .strip_prefix("async def ")
        .or_else(|| statement.strip_prefix("def "));
    let Some(definition) = definition else {
        return Vec::new();
    };
    let Some(open) = definition.find('(') else {
        return Vec::new();
    };
    let Some(close) = definition.rfind(')') else {
        return Vec::new();
    };
    if open >= close {
        return Vec::new();
    }
    split_top_level(&definition[open + 1..close], ',')
        .into_iter()
        .filter_map(|parameter| {
            let (name, annotation) = parameter.split_once(':')?;
            let name = name.trim();
            let annotation = annotation
                .split_once('=')
                .map_or(annotation, |(annotation, _)| annotation)
                .trim();
            (is_identifier(name) && annotation.rsplit('.').next() == Some("FastAPI"))
                .then(|| name.to_string())
        })
        .collect()
}

fn parse_router_definition(statement: &str) -> Option<(String, StaticPath)> {
    let (left, right) = split_assignment(statement)?;
    let call = right.trim();
    let open = call.find('(')?;
    let callee = call[..open].trim();
    if callee.rsplit('.').next()? != "APIRouter" {
        return None;
    }
    let symbol = left.split(':').next()?.split_whitespace().last()?;
    if !is_identifier(symbol) {
        return None;
    }
    Some((
        symbol.to_string(),
        static_keyword_path(call_args(call)?, "prefix"),
    ))
}

fn parse_router_include(statement: &str) -> Option<RouterInclude> {
    let marker = ".include_router";
    let marker_index = statement.find(marker)?;
    let parent = statement[..marker_index].trim();
    if !is_dotted_identifier(parent) {
        return None;
    }
    let call = statement[marker_index + marker.len()..].trim();
    let args = call_args(call)?;
    let child = first_positional_argument(args)?;
    if !is_dotted_identifier(child) {
        return None;
    }
    Some(RouterInclude {
        parent: parent.to_string(),
        child: child.to_string(),
        prefix: static_keyword_path(args, "prefix"),
    })
}

fn route_router_symbol(
    module: &ModuleSource,
    handler_line: u64,
    method: &str,
    local_path: &str,
) -> Option<(String, String)> {
    let definition = module
        .statements
        .iter()
        .position(|statement| {
            statement.start_line <= handler_line
                && handler_line <= statement.end_line
                && is_function_definition(&statement.text)
        })
        .or_else(|| {
            module.statements.iter().position(|statement| {
                statement.start_line >= handler_line
                    && statement.start_line <= handler_line.saturating_add(2)
                    && is_function_definition(&statement.text)
            })
        })?;

    let mut matches = BTreeSet::new();
    for statement in module.statements[..definition].iter().rev() {
        if !statement.text.trim_start().starts_with('@') {
            break;
        }
        if let Some(router) = parse_route_decorator(&statement.text, method, local_path) {
            matches.insert(router);
        }
    }
    (matches.len() == 1)
        .then(|| matches.into_iter().next())
        .flatten()
}

fn parse_route_decorator(
    statement: &str,
    method: &str,
    local_path: &str,
) -> Option<(String, String)> {
    let decorator = statement.trim().strip_prefix('@')?;
    let open = decorator.find('(')?;
    let callee = decorator[..open].trim();
    let (router, decorator_method) = callee.rsplit_once('.')?;
    if !HTTP_METHODS.contains(&decorator_method.to_ascii_lowercase().as_str())
        || !decorator_method.eq_ignore_ascii_case(method)
        || !is_dotted_identifier(router)
    {
        return None;
    }
    let path = first_positional_argument(call_args(&decorator[open..])?)?;
    let path = parse_static_string(path)?;
    route_paths_equivalent(&path, local_path).then(|| (router.to_string(), path))
}

fn route_paths_equivalent(source_path: &str, engine_path: &str) -> bool {
    source_path == engine_path
        || (source_path.is_empty() && engine_path == "/")
        || (source_path == "/" && engine_path.is_empty())
}

fn route_method(route: &CodeInventoryItem, handler: &CodeInventoryItem) -> Option<String> {
    detail_string(&handler.detail, &["routeMethod", "route_method"])
        .or_else(|| detail_string(&route.detail, &["routeMethod", "route_method", "method"]))
        .or_else(|| route_method_from_identity(&route.id))
        .or_else(|| {
            route
                .name
                .split_once(' ')
                .map(|(method, _)| method.to_string())
        })
        .filter(|method| {
            HTTP_METHODS
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(method))
        })
}

fn route_local_path(route: &CodeInventoryItem, handler: &CodeInventoryItem) -> String {
    detail_string(&handler.detail, &["routePath", "route_path"])
        .or_else(|| {
            detail_string(
                &route.detail,
                &["localRoutePath", "routePath", "route_path"],
            )
        })
        .unwrap_or_else(|| {
            route
                .name
                .split_once(' ')
                .map_or_else(|| route.name.clone(), |(_, path)| path.to_string())
        })
}

fn route_method_from_identity(identity: &str) -> Option<String> {
    let marker = "__route__";
    let tail = identity.split(marker).nth(1)?;
    let method = tail.split("__").next()?.trim();
    (!method.is_empty()).then(|| method.to_string())
}

fn detail_string(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn logical_statements(source: &str) -> Vec<LogicalStatement> {
    let mut statements = Vec::new();
    let mut buffer = String::new();
    let mut start_line = 0_u64;
    let mut depth = 0_i32;

    for (index, raw_line) in source.lines().enumerate() {
        let line_number = index as u64 + 1;
        let uncommented = strip_python_comment(raw_line);
        let line = uncommented.trim();
        if line.is_empty() && buffer.is_empty() {
            continue;
        }
        if buffer.is_empty() {
            start_line = line_number;
        } else if !line.is_empty() {
            buffer.push(' ');
        }
        buffer.push_str(line);
        depth += bracket_delta(line);

        if depth <= 0 && !line.ends_with('\\') && !buffer.trim().is_empty() {
            statements.push(LogicalStatement {
                start_line,
                end_line: line_number,
                text: buffer.trim().trim_end_matches('\\').trim().to_string(),
            });
            buffer.clear();
            depth = 0;
        }
    }

    if !buffer.trim().is_empty() {
        statements.push(LogicalStatement {
            start_line,
            end_line: source.lines().count() as u64,
            text: buffer.trim().to_string(),
        });
    }
    statements
}

fn strip_python_comment(line: &str) -> String {
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' && quote.is_some() {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if character == active {
                quote = None;
            }
            continue;
        }
        if character == '\'' || character == '"' {
            quote = Some(character);
        } else if character == '#' {
            return line[..index].to_string();
        }
    }
    line.to_string()
}

fn bracket_delta(value: &str) -> i32 {
    let mut quote = None;
    let mut escaped = false;
    let mut delta = 0;
    for character in value.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' && quote.is_some() {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if character == active {
                quote = None;
            }
            continue;
        }
        match character {
            '\'' | '"' => quote = Some(character),
            '(' | '[' | '{' => delta += 1,
            ')' | ']' | '}' => delta -= 1,
            _ => {}
        }
    }
    delta
}

fn split_assignment(statement: &str) -> Option<(&str, &str)> {
    let mut quote = None;
    let mut depth = 0_i32;
    let characters = statement.char_indices().collect::<Vec<_>>();
    for (offset, (index, character)) in characters.iter().enumerate() {
        if let Some(active) = quote {
            if *character == active && (offset == 0 || characters[offset - 1].1 != '\\') {
                quote = None;
            }
            continue;
        }
        match character {
            '\'' | '"' => quote = Some(*character),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '=' if depth == 0 => {
                let previous = offset
                    .checked_sub(1)
                    .and_then(|previous| characters.get(previous))
                    .map(|(_, value)| *value);
                let next = characters.get(offset + 1).map(|(_, value)| *value);
                if previous != Some('=') && next != Some('=') {
                    return Some((&statement[..*index], &statement[index + 1..]));
                }
            }
            _ => {}
        }
    }
    None
}

fn call_args(call: &str) -> Option<&str> {
    let start = call.find('(')?;
    let end = call.rfind(')')?;
    (start < end).then_some(&call[start + 1..end])
}

fn first_positional_argument(args: &str) -> Option<&str> {
    split_top_level(args, ',')
        .into_iter()
        .map(str::trim)
        .find(|argument| !argument.is_empty() && split_assignment(argument).is_none())
}

fn static_keyword_path(args: &str, key: &str) -> StaticPath {
    for argument in split_top_level(args, ',') {
        let Some((name, value)) = split_assignment(argument) else {
            continue;
        };
        if name.trim() == key {
            return parse_static_string(value.trim())
                .map(StaticPath::Known)
                .unwrap_or(StaticPath::Dynamic);
        }
    }
    StaticPath::Known(String::new())
}

fn parse_static_string(value: &str) -> Option<String> {
    let value = value.trim();
    if value.len() < 2 {
        return None;
    }
    let quote = value.chars().next()?;
    if !matches!(quote, '\'' | '"') || value.chars().last()? != quote {
        return None;
    }
    let inner = &value[quote.len_utf8()..value.len() - quote.len_utf8()];
    (!inner.contains('\\')).then(|| inner.to_string())
}

fn split_top_level(value: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut quote = None;
    let mut escaped = false;
    let mut depth = 0_i32;

    for (index, character) in value.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' && quote.is_some() {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if character == active {
                quote = None;
            }
            continue;
        }
        match character {
            '\'' | '"' => quote = Some(character),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ if character == separator && depth == 0 => {
                parts.push(&value[start..index]);
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&value[start..]);
    parts
}

fn resolve_import_module(
    current_module: &str,
    is_package: bool,
    module_ref: &str,
) -> Option<String> {
    let level = module_ref
        .chars()
        .take_while(|character| *character == '.')
        .count();
    if level == 0 {
        return is_dotted_identifier(module_ref).then(|| module_ref.to_string());
    }

    let tail = module_ref[level..].trim_matches('.');
    let mut base = current_module.split('.').collect::<Vec<_>>();
    if !is_package {
        base.pop();
    }
    for _ in 1..level {
        base.pop()?;
    }
    if !tail.is_empty() {
        if !is_dotted_identifier(tail) {
            return None;
        }
        base.extend(tail.split('.'));
    }
    (!base.is_empty()).then(|| base.join("."))
}

fn python_module(path: &str) -> Option<(String, bool)> {
    if !is_python_path(path) {
        return None;
    }
    let path = normalize_source_path(path);
    let without_extension = path.strip_suffix(".py")?;
    let mut parts = without_extension
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>();
    let is_package = parts.last() == Some(&"__init__");
    if is_package {
        parts.pop();
    }
    (!parts.is_empty()).then(|| (parts.join("."), is_package))
}

fn normalize_source_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_string()
}

fn normalize_url_prefix(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return String::new();
    }
    format!("/{}", trimmed.trim_matches('/'))
}

fn join_url_path(prefix: &str, path: &str) -> String {
    let prefix = normalize_url_prefix(prefix);
    let path = path.trim();
    if path.is_empty() {
        return prefix;
    }
    let trailing_slash = path.ends_with('/');
    let body = path.trim_matches('/');
    let mut joined = match (prefix.is_empty(), body.is_empty()) {
        (true, true) => "/".to_string(),
        (true, false) => format!("/{body}"),
        (false, true) => prefix,
        (false, false) => format!("{prefix}/{body}"),
    };
    if trailing_slash && joined != "/" && !joined.ends_with('/') {
        joined.push('/');
    }
    joined
}

fn is_python_path(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".py")
}

fn is_fastapi_source_candidate(path: &str, source: &str) -> bool {
    normalize_source_path(path).ends_with("__init__.py")
        || source.contains("APIRouter")
        || source.contains("FastAPI")
        || source.contains("include_router")
        || HTTP_METHODS
            .iter()
            .any(|method| source.contains(&format!(".{method}(")))
}

fn is_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn is_dotted_identifier(value: &str) -> bool {
    !value.is_empty() && value.split('.').all(is_identifier)
}

fn is_function_definition(statement: &str) -> bool {
    let statement = statement.trim_start();
    statement.starts_with("def ") || statement.starts_with("async def ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::model::{CodeCall, CodeHandle, CodeInventorySummary};

    fn sources(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
        entries
            .iter()
            .map(|(path, source)| ((*path).to_string(), (*source).to_string()))
            .collect()
    }

    fn inventory_for_route(path: &str, line: u64) -> CodeInventory {
        let handler_id = "app.routes.session.lifecycle.list_sessions".to_string();
        let route_id = "__route__GET__/#handler=list_sessions".to_string();
        let handler = CodeInventoryItem {
            id: handler_id.clone(),
            kind: "Function".to_string(),
            name: "list_sessions".to_string(),
            project: "test".to_string(),
            qualified_name: handler_id.clone(),
            engine_label: "Function".to_string(),
            file_path: Some(path.to_string()),
            line: Some(line),
            column: None,
            end_line: None,
            end_column: None,
            detail: serde_json::json!({
                "route_path": "/",
                "route_method": "GET"
            }),
        };
        let route = CodeInventoryItem {
            id: route_id.clone(),
            kind: "Route".to_string(),
            name: "/".to_string(),
            project: "test".to_string(),
            qualified_name: route_id.clone(),
            engine_label: "Route".to_string(),
            file_path: Some(path.to_string()),
            line: Some(line),
            column: None,
            end_line: None,
            end_column: None,
            detail: serde_json::json!({}),
        };
        CodeInventory {
            project: "test".to_string(),
            routes: vec![route],
            services: Vec::new(),
            files: Vec::new(),
            handlers: vec![handler],
            repositories: Vec::new(),
            functions: Vec::new(),
            classes: Vec::new(),
            modules: Vec::new(),
            unknown: Vec::new(),
            summary: CodeInventorySummary {
                routes: 1,
                handlers: 1,
                services: 0,
                repositories: 0,
                functions: 0,
                classes: 0,
                modules: 0,
                files: 0,
                unknown: 0,
            },
            architecture: None,
            calls: Vec::new(),
            handles: vec![CodeHandle {
                handler: handler_id,
                route: route_id,
            }],
            partial: false,
        }
    }

    fn file_item(path: &str) -> CodeInventoryItem {
        CodeInventoryItem {
            id: path.to_string(),
            kind: "File".to_string(),
            name: path.to_string(),
            project: "test".to_string(),
            qualified_name: path.to_string(),
            engine_label: "File".to_string(),
            file_path: Some(path.to_string()),
            line: None,
            column: None,
            end_line: None,
            end_column: None,
            detail: serde_json::json!({}),
        }
    }

    fn function_item(id: &str, name: &str, path: &str) -> CodeInventoryItem {
        CodeInventoryItem {
            id: id.to_string(),
            kind: "Function".to_string(),
            name: name.to_string(),
            project: "test".to_string(),
            qualified_name: id.to_string(),
            engine_label: "Function".to_string(),
            file_path: Some(path.to_string()),
            line: Some(1),
            column: None,
            end_line: Some(2),
            end_column: None,
            detail: serde_json::json!({}),
        }
    }

    #[test]
    fn resolves_nested_fastapi_router_prefixes_and_reexports() {
        let sources = sources(&[
            (
                "src/app/routes/session/lifecycle.py",
                r#"
from fastapi import APIRouter

router = APIRouter()

@router.get("/")
def list_sessions():
    pass
"#,
            ),
            (
                "src/app/routes/session/__init__.py",
                "from .lifecycle import router\n",
            ),
            (
                "src/app/routes/root.py",
                r#"
from fastapi import APIRouter
from .session import (
    router as session_router,
)

router = APIRouter(prefix="/api/v1")
router.include_router(session_router, prefix="/sessions")
"#,
            ),
            (
                "src/app/main.py",
                r#"
from fastapi import FastAPI
from .routes.root import router as root_router
app = FastAPI()
app.include_router(root_router)
"#,
            ),
        ]);
        let graph = FastApiGraph::from_sources(&sources);

        assert_eq!(
            graph.mounted_route_path("src/app/routes/session/lifecycle.py", 7, "GET", "/"),
            Some(MountedRoutePath {
                local: "/".to_string(),
                mounted: "/api/v1/sessions/".to_string(),
            })
        );
    }

    #[test]
    fn enriches_only_a_uniquely_proven_mounted_route() {
        let sources = sources(&[
            (
                "app/routes/session/lifecycle.py",
                r#"
from fastapi import APIRouter
router = APIRouter()

@router.get("/")
def list_sessions():
    pass
"#,
            ),
            (
                "app/routes/session/router.py",
                r#"
from fastapi import APIRouter
from app.routes.session.lifecycle import router as lifecycle_router
router = APIRouter(prefix="/api/v1/sessions")
router.include_router(lifecycle_router)
"#,
            ),
            (
                "app/route_groups/control.py",
                r#"
from fastapi import FastAPI
from app.routes.session.router import router as session_router
def include_control_routes(app: FastAPI):
    app.include_router(session_router)
"#,
            ),
        ]);
        let mut inventory = inventory_for_route("app/routes/session/lifecycle.py", 6);

        enrich_fastapi_route_paths_from_sources(&sources, &mut inventory);

        let route = &inventory.routes[0];
        assert_eq!(route.name, "/api/v1/sessions/");
        assert_eq!(route.detail["localRoutePath"], "/");
        assert_eq!(route.detail["mountedRoutePath"], "/api/v1/sessions/");
        assert_eq!(route.detail["routePathSource"], "fastapi-static-mount");
    }

    #[test]
    fn confirms_only_unambiguous_unshadowed_fastapi_import_calls() {
        let route_path = "backend/app/api/routes/login.py";
        let sources = sources(&[(
            route_path,
            r#"
from fastapi import APIRouter
from app import crud, shadowed
from app.core import security
router = APIRouter()

@router.post("/login")
def list_sessions(shadowed):
    user = crud.authenticate()
    shadowed.run()
    return security.create_access_token(user.id)
"#,
        )]);
        let graph = FastApiGraph::from_sources(&sources);
        let mut inventory = inventory_for_route(route_path, 8);
        let caller = inventory.handlers[0].id.clone();
        inventory.functions = vec![
            function_item(
                "backend.app.crud.authenticate",
                "authenticate",
                "backend/app/crud.py",
            ),
            function_item(
                "backend.app.core.security.create_access_token",
                "create_access_token",
                "backend/app/core/security.py",
            ),
            function_item("backend.app.shadowed.run", "run", "backend/app/shadowed.py"),
        ];
        inventory.calls = vec![
            CodeCall {
                from: caller.clone(),
                to: "backend.app.crud.authenticate".to_string(),
                confidence: Some(38),
                strategy: Some("unique_name".to_string()),
                expression: Some("crud.authenticate".to_string()),
            },
            CodeCall {
                from: caller.clone(),
                to: "backend.app.core.security.create_access_token".to_string(),
                confidence: Some(38),
                strategy: Some("unique_name".to_string()),
                expression: Some("security.create_access_token".to_string()),
            },
            CodeCall {
                from: caller.clone(),
                to: "backend.app.shadowed.run".to_string(),
                confidence: Some(38),
                strategy: Some("unique_name".to_string()),
                expression: Some("shadowed.run".to_string()),
            },
            CodeCall {
                from: caller,
                to: "backend.app.crud.authenticate".to_string(),
                confidence: Some(38),
                strategy: Some("unique_name".to_string()),
                expression: Some("crud.users.authenticate".to_string()),
            },
        ];

        enrich_fastapi_import_calls(&graph, &mut inventory);

        assert_eq!(
            inventory
                .calls
                .iter()
                .map(|call| (
                    call.expression.as_deref().unwrap(),
                    call.confidence,
                    call.strategy.as_deref().unwrap()
                ))
                .collect::<Vec<_>>(),
            vec![
                ("crud.authenticate", Some(95), "python_static_import"),
                (
                    "security.create_access_token",
                    Some(95),
                    "python_static_import"
                ),
                ("shadowed.run", Some(38), "unique_name"),
                ("crud.users.authenticate", Some(38), "unique_name"),
            ]
        );
    }

    #[test]
    fn preserves_an_empty_decorator_path_after_engine_root_normalization() {
        let sources = sources(&[
            (
                "app/routes/sessions.py",
                r#"
from fastapi import APIRouter
router = APIRouter(prefix="/api/v1/sessions")

@router.post("")
def create_session():
    pass
"#,
            ),
            (
                "app/main.py",
                r#"
from fastapi import FastAPI
from app.routes.sessions import router as session_router
app = FastAPI()
app.include_router(session_router)
"#,
            ),
        ]);
        let mut inventory = inventory_for_route("app/routes/sessions.py", 6);
        inventory.handlers[0].detail["route_method"] = serde_json::json!("POST");

        enrich_fastapi_route_paths_from_sources(&sources, &mut inventory);

        let route = &inventory.routes[0];
        assert_eq!(route.name, "/api/v1/sessions");
        assert_eq!(route.detail["localRoutePath"], "");
    }

    #[test]
    fn reads_indexed_python_files_within_the_repository_boundary() {
        let root = std::env::temp_dir().join(format!(
            "backend-map-fastapi-routes-{}-{}",
            std::process::id(),
            crate::workspace::store::timestamp()
        ));
        let lifecycle = root.join("app/routes/session/lifecycle.py");
        let router = root.join("app/routes/session/router.py");
        let control = root.join("app/route_groups/control.py");
        for parent in [
            lifecycle.parent().unwrap(),
            router.parent().unwrap(),
            control.parent().unwrap(),
        ] {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(
            &lifecycle,
            "from fastapi import APIRouter\nrouter = APIRouter()\n\n@router.get(\"/\")\ndef list_sessions():\n    pass\n",
        )
        .unwrap();
        fs::write(
            &router,
            "from fastapi import APIRouter\nfrom app.routes.session.lifecycle import router as lifecycle_router\nrouter = APIRouter(prefix=\"/api/v1/sessions\")\nrouter.include_router(lifecycle_router)\n",
        )
        .unwrap();
        fs::write(
            &control,
            "from fastapi import FastAPI\nfrom app.routes.session.router import router as session_router\ndef include_control_routes(app: FastAPI):\n    app.include_router(session_router)\n",
        )
        .unwrap();

        let lifecycle_path = lifecycle.display().to_string();
        let mut inventory = inventory_for_route(&lifecycle_path, 5);
        inventory.files = vec![
            file_item(&lifecycle_path),
            file_item("app/routes/session/router.py"),
            file_item("app/route_groups/control.py"),
        ];

        enrich_fastapi_evidence(root.to_str().unwrap(), &mut inventory);

        assert_eq!(inventory.routes[0].name, "/api/v1/sessions/");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn does_not_present_an_unmounted_router_prefix_as_a_full_route() {
        let sources = sources(&[(
            "app/routes.py",
            r#"
from fastapi import APIRouter
router = APIRouter(prefix="/internal")
@router.get("/items")
def items():
    pass
"#,
        )]);
        let graph = FastApiGraph::from_sources(&sources);

        assert_eq!(
            graph.mounted_route_path("app/routes.py", 5, "GET", "/items"),
            None
        );
    }

    #[test]
    fn leaves_multiply_mounted_routes_unresolved() {
        let sources = sources(&[
            (
                "app/child.py",
                r#"
from fastapi import APIRouter
router = APIRouter()
@router.get("/items")
def items():
    pass
"#,
            ),
            (
                "app/parents.py",
                r#"
from fastapi import APIRouter
from app.child import router as child_router
v1 = APIRouter(prefix="/v1")
v2 = APIRouter(prefix="/v2")
v1.include_router(child_router)
v2.include_router(child_router)
"#,
            ),
            (
                "app/main.py",
                r#"
from fastapi import FastAPI
from app.parents import v1, v2
app = FastAPI()
app.include_router(v1)
app.include_router(v2)
"#,
            ),
        ]);
        let graph = FastApiGraph::from_sources(&sources);

        assert_eq!(
            graph.mounted_route_path("app/child.py", 5, "GET", "/items"),
            None
        );
    }

    #[test]
    fn leaves_dynamic_prefixes_unresolved() {
        let sources = sources(&[
            (
                "app/child.py",
                r#"
from fastapi import APIRouter
router = APIRouter()
@router.get("/items")
def items():
    pass
"#,
            ),
            (
                "app/root.py",
                r#"
from fastapi import APIRouter
from app.child import router as child_router
router = APIRouter(prefix=API_PREFIX)
router.include_router(child_router)
"#,
            ),
            (
                "app/main.py",
                r#"
from fastapi import FastAPI
from app.root import router
app = FastAPI()
app.include_router(router)
"#,
            ),
        ]);
        let graph = FastApiGraph::from_sources(&sources);

        assert_eq!(
            graph.mounted_route_path("app/child.py", 5, "GET", "/items"),
            None
        );
    }

    #[test]
    fn joins_paths_without_losing_a_route_trailing_slash() {
        assert_eq!(join_url_path("/api/v1/sessions", "/"), "/api/v1/sessions/");
        assert_eq!(
            join_url_path("/api/v1/sessions/", "/{session_id}"),
            "/api/v1/sessions/{session_id}"
        );
    }
}
