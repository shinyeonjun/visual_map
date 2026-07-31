use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::{combine_route_prefix, first_quoted_value, first_route_path, registration_handler};

#[derive(Default)]
pub(crate) struct JavascriptRouteContext {
    prefixes: HashMap<String, String>,
}

impl JavascriptRouteContext {
    pub(crate) fn build(sources: &[(&str, &str)]) -> Self {
        let known = sources
            .iter()
            .map(|(path, _)| (*path).to_string())
            .collect::<HashSet<_>>();
        let source_by_path = sources.iter().copied().collect::<HashMap<_, _>>();
        let mut imports = HashMap::new();
        for (path, source) in sources {
            for (name, specifier) in module_bindings(source) {
                if let Some(target) = resolve_module(path, &specifier, &known) {
                    imports.insert(((*path).to_string(), name), target);
                }
            }
        }

        let router_modules = sources
            .iter()
            .filter(|(_, source)| is_exported_router(source))
            .map(|(path, _)| (*path).to_string())
            .collect::<HashSet<_>>();
        let mut edges = Vec::<(String, String, String)>::new();
        for (path, source) in sources {
            for line in source.lines().filter(|line| line.contains(".use(")) {
                let Some((prefix, end)) = first_route_path(line) else {
                    continue;
                };
                let Some(target_name) = registration_handler(&line[end..]) else {
                    continue;
                };
                let Some(target) = imports.get(&((*path).to_string(), target_name)) else {
                    continue;
                };
                if router_modules.contains(target) {
                    edges.push(((*path).to_string(), target.clone(), prefix));
                }
            }
            if source.contains(".use(route.path, route.route)") {
                for (prefix, target_name) in route_table_entries(source) {
                    let Some(target) = imports.get(&((*path).to_string(), target_name)) else {
                        continue;
                    };
                    if router_modules.contains(target) {
                        edges.push(((*path).to_string(), target.clone(), prefix));
                    }
                }
            }
        }
        edges.sort();
        edges.dedup();

        let mut memo = HashMap::<String, HashSet<String>>::new();
        let mut prefixes = HashMap::new();
        for path in router_modules {
            let resolved = prefixes_for(&path, &edges, &mut memo, &mut HashSet::new());
            if resolved.len() == 1 {
                prefixes.insert(path, resolved.into_iter().next().unwrap_or_default());
            }
        }

        // Keep only paths that still exist in the source snapshot. This also
        // makes the context insensitive to stale import candidates.
        prefixes.retain(|path, _| source_by_path.contains_key(path.as_str()));
        Self { prefixes }
    }

    pub(crate) fn mounted_path(&self, source_file: &str, local_path: &str) -> Option<String> {
        let prefix = self.prefixes.get(source_file)?;
        Some(combine_route_prefix(Some(prefix), local_path))
    }
}

fn module_bindings(source: &str) -> Vec<(String, String)> {
    source
        .lines()
        .filter_map(|line| {
            if let Some(require) = line.find("require(") {
                let name = line[..require]
                    .split('=')
                    .next()?
                    .split_whitespace()
                    .last()?
                    .trim();
                if name.is_empty() || name.starts_with(['{', '[']) {
                    return None;
                }
                return first_quoted_value(&line[require..])
                    .map(|specifier| (name.to_string(), specifier));
            }
            let trimmed = line.trim_start();
            if !trimmed.starts_with("import ") || !trimmed.contains(" from ") {
                return None;
            }
            let name = trimmed["import ".len()..].split_whitespace().next()?;
            if name.starts_with(['{', '*']) {
                return None;
            }
            first_quoted_value(trimmed).map(|specifier| (name.to_string(), specifier))
        })
        .collect()
}

fn resolve_module(parent: &str, specifier: &str, known: &HashSet<String>) -> Option<String> {
    if !specifier.starts_with('.') {
        return None;
    }
    let base = Path::new(parent).parent().unwrap_or_else(|| Path::new(""));
    let joined = normalize_path(base.join(specifier));
    let candidates = [
        joined.clone(),
        format!("{joined}.js"),
        format!("{joined}.ts"),
        format!("{joined}.jsx"),
        format!("{joined}.tsx"),
        format!("{joined}/index.js"),
        format!("{joined}/index.ts"),
    ];
    candidates
        .into_iter()
        .find(|candidate| known.contains(candidate))
}

fn normalize_path(path: PathBuf) -> String {
    let mut parts = Vec::new();
    let normalized = path.to_string_lossy().replace('\\', "/");
    for part in normalized.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            value => parts.push(value),
        }
    }
    parts.join("/")
}

fn is_exported_router(source: &str) -> bool {
    let exported = source.contains("module.exports")
        || source.contains("export default")
        || source.contains("export { ");
    exported
        && (source.contains(".Router(")
            || source.contains("new Router(")
            || source.contains("= express(")
            || source.contains("= fastify("))
}

fn route_table_entries(source: &str) -> Vec<(String, String)> {
    let mut path = None;
    let mut entries = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("path:") {
            path = first_quoted_value(trimmed);
        } else if let Some(route) = trimmed.strip_prefix("route:") {
            let target = route.trim().trim_end_matches(',').to_string();
            if let Some(prefix) = path.take().filter(|_| !target.is_empty()) {
                entries.push((prefix, target));
            }
        } else if trimmed.starts_with('}') {
            path = None;
        }
    }
    entries
}

fn prefixes_for(
    file: &str,
    edges: &[(String, String, String)],
    memo: &mut HashMap<String, HashSet<String>>,
    stack: &mut HashSet<String>,
) -> HashSet<String> {
    if let Some(prefixes) = memo.get(file) {
        return prefixes.clone();
    }
    if !stack.insert(file.to_string()) {
        return HashSet::new();
    }
    let incoming = edges
        .iter()
        .filter(|(_, child, _)| child == file)
        .collect::<Vec<_>>();
    let mut prefixes = HashSet::new();
    if incoming.is_empty() {
        prefixes.insert(String::new());
    } else {
        for (parent, _, mounted) in incoming {
            for prefix in prefixes_for(parent, edges, memo, stack) {
                prefixes.insert(combine_route_prefix(Some(&prefix), mounted));
            }
        }
    }
    stack.remove(file);
    memo.insert(file.to_string(), prefixes.clone());
    prefixes
}
