use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::{combine_route_prefix, first_quoted_value, FastApiRouteContext};

pub(crate) fn build_fastapi_route_context(sources: &[(&str, &str)]) -> FastApiRouteContext {
    type RouterKey = (String, String);

    let source_paths = sources
        .iter()
        .filter(|(path, _)| path.ends_with(".py"))
        .map(|(path, _)| (*path).to_string())
        .collect::<HashSet<_>>();
    let mut local_prefixes = HashMap::<RouterKey, String>::new();
    for (path, source) in sources.iter().filter(|(path, _)| path.ends_with(".py")) {
        for (name, prefix) in fastapi_router_declarations(source) {
            local_prefixes.insert((path.to_string(), name), prefix);
        }
    }

    let mut imports = HashMap::<RouterKey, RouterKey>::new();
    for (path, source) in sources.iter().filter(|(path, _)| path.ends_with(".py")) {
        for (alias, module, symbol) in fastapi_imports(source) {
            let Some(target_path) = resolve_python_module_path(path, &module, &source_paths) else {
                continue;
            };
            imports.insert((path.to_string(), alias), (target_path, symbol));
        }
    }

    let mut edges = Vec::new();
    for (path, source) in sources.iter().filter(|(path, _)| path.ends_with(".py")) {
        for (caller, child, extra_prefix) in fastapi_include_edges(source) {
            let mut target = imports
                .get(&(path.to_string(), child.clone()))
                .cloned()
                .unwrap_or_else(|| (path.to_string(), child));
            for _ in 0..8 {
                if local_prefixes.contains_key(&target) {
                    break;
                }
                let Some(next) = imports.get(&target).cloned() else {
                    break;
                };
                target = next;
            }
            if !local_prefixes.contains_key(&target) {
                continue;
            }
            let parent = (caller != "app").then(|| (path.to_string(), caller));
            edges.push((parent, target, extra_prefix));
        }
    }

    let mut candidates = HashMap::<RouterKey, HashSet<String>>::new();
    for (key, prefix) in &local_prefixes {
        candidates
            .entry(key.clone())
            .or_default()
            .insert(prefix.clone());
    }
    let mounted_targets = edges
        .iter()
        .map(|(_, target, _)| target.clone())
        .collect::<HashSet<_>>();
    for _ in 0..edges.len().saturating_add(1) {
        let mut changed = false;
        for (parent, target, extra_prefix) in &edges {
            let parent_prefixes = parent
                .as_ref()
                .and_then(|key| candidates.get(key))
                .cloned()
                .unwrap_or_else(|| HashSet::from([String::new()]));
            for parent_prefix in parent_prefixes {
                let prefix = if extra_prefix.is_empty() {
                    parent_prefix.clone()
                } else {
                    combine_route_prefix(
                        (!parent_prefix.is_empty()).then_some(parent_prefix.as_str()),
                        extra_prefix,
                    )
                };
                changed |= candidates.entry(target.clone()).or_default().insert(prefix);
            }
        }
        if !changed {
            break;
        }
    }

    let mut prefixes = HashMap::new();
    for (key, values) in candidates {
        let selected = if values.len() == 1 {
            values.into_iter().next()
        } else if mounted_targets.contains(&key) {
            let non_empty = values
                .into_iter()
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>();
            (non_empty.len() == 1).then(|| non_empty[0].clone())
        } else {
            None
        };
        if let Some(prefix) = selected {
            prefixes.insert(key, prefix);
        }
    }
    FastApiRouteContext { prefixes }
}

pub(crate) fn fastapi_router_declarations(source: &str) -> Vec<(String, String)> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut output = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let Some(start) = line.find("APIRouter(") else {
            continue;
        };
        let mut block = line[start..].to_string();
        let mut balance = block.chars().filter(|value| *value == '(').count() as i32
            - block.chars().filter(|value| *value == ')').count() as i32;
        let mut next = index + 1;
        while balance > 0 && next < lines.len() {
            block.push('\n');
            block.push_str(lines[next]);
            balance += lines[next].chars().filter(|value| *value == '(').count() as i32;
            balance -= lines[next].chars().filter(|value| *value == ')').count() as i32;
            next += 1;
        }
        let Some((left, _)) = line[..start].rsplit_once('=') else {
            continue;
        };
        let Some(name) = left
            .rsplit(|value: char| !value.is_ascii_alphanumeric() && value != '_')
            .find(|value| !value.is_empty())
        else {
            continue;
        };
        let prefix = fastapi_named_quoted_value(&block, "prefix").unwrap_or_default();
        output.push((name.to_string(), prefix));
    }
    output
}

pub(crate) fn fastapi_imports(source: &str) -> Vec<(String, String, String)> {
    let mut output = Vec::new();
    let lines = source.lines().collect::<Vec<_>>();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("from ") else {
            index += 1;
            continue;
        };
        let Some((module, initial_names)) = rest.split_once(" import ") else {
            index += 1;
            continue;
        };
        let mut names = initial_names.trim().to_string();
        while names.starts_with('(') && !names.contains(')') && index + 1 < lines.len() {
            index += 1;
            names.push(' ');
            names.push_str(lines[index].trim());
        }
        for item in names.trim_matches(['(', ')']).split(',') {
            let parts = item.trim().split(" as ").collect::<Vec<_>>();
            let symbol = parts.first().copied().unwrap_or_default().trim();
            let alias = parts.last().copied().unwrap_or_default().trim();
            if !symbol.is_empty() && !alias.is_empty() {
                output.push((
                    alias.to_string(),
                    module.trim().to_string(),
                    symbol.to_string(),
                ));
            }
        }
        index += 1;
    }
    output
}

pub(crate) fn fastapi_include_edges(source: &str) -> Vec<(String, String, String)> {
    let mut output = Vec::new();
    for line in source.lines() {
        let Some(start) = line.find(".include_router(") else {
            continue;
        };
        let caller = line[..start]
            .rsplit(|value: char| !value.is_ascii_alphanumeric() && value != '_')
            .find(|value| !value.is_empty())
            .unwrap_or_default()
            .to_string();
        let rest = &line[start + ".include_router(".len()..];
        let child = rest
            .split([',', ')', ' '])
            .find(|value| !value.is_empty())
            .unwrap_or_default()
            .trim_matches(|value: char| !value.is_ascii_alphanumeric() && value != '_')
            .to_string();
        if caller.is_empty() || child.is_empty() {
            continue;
        }
        let prefix = fastapi_named_quoted_value(rest, "prefix").unwrap_or_default();
        output.push((caller, child, prefix));
    }
    output
}

pub(crate) fn fastapi_named_quoted_value(source: &str, name: &str) -> Option<String> {
    let start = source.find(name)? + name.len();
    first_quoted_value(&source[start..])
}

pub(crate) fn resolve_python_module_path(
    source_path: &str,
    module: &str,
    source_paths: &HashSet<String>,
) -> Option<String> {
    let module = module.trim();
    let dots = module.chars().take_while(|value| *value == '.').count();
    let module = module.trim_start_matches('.').replace('.', "/");
    let mut candidates = Vec::new();
    if dots > 0 {
        let mut base = Path::new(source_path)
            .parent()
            .unwrap_or(Path::new(""))
            .to_path_buf();
        for _ in 1..dots {
            base.pop();
        }
        candidates.push(base.join(format!("{module}.py")));
        candidates.push(base.join(&module).join("__init__.py"));
    } else {
        candidates.push(PathBuf::from(format!("{module}.py")));
        candidates.push(PathBuf::from(&module).join("__init__.py"));
        for marker in ["routes/", "app/"] {
            if let Some((_, suffix)) = module.split_once(marker) {
                candidates.push(PathBuf::from(format!("{suffix}.py")));
                candidates.push(PathBuf::from(suffix).join("__init__.py"));
            }
        }
    }
    let candidates = candidates
        .into_iter()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect::<Vec<_>>();
    source_paths
        .iter()
        .find(|path| candidates.iter().any(|candidate| *path == candidate))
        .cloned()
        .or_else(|| {
            source_paths
                .iter()
                .find(|path| {
                    candidates
                        .iter()
                        .any(|candidate| path.ends_with(&format!("/{candidate}")))
                })
                .cloned()
        })
}
