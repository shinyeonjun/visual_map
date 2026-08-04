use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use super::{ImportUse, PackageInfo, PhpNamespaceIndex, SourcePathIndex};
use crate::source::is_managed_provider_root;
use crate::LANGUAGES;

pub(crate) fn load_packages(root: &Path) -> Vec<PackageInfo> {
    let mut paths = Vec::new();
    collect_metadata_files(root, &mut paths);
    let mut packages = Vec::new();
    for path in paths {
        let Some(relative) = relative_path(root, &path) else {
            continue;
        };
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        let file = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let directory = relative
            .rsplit_once('/')
            .map(|(value, _)| value)
            .unwrap_or("");
        let (ecosystem, name, version) = parse_package_metadata(file, &source);
        let Some(name) = name else {
            continue;
        };
        packages.push(PackageInfo {
            root: directory.to_string(),
            ecosystem,
            name,
            version,
        });
    }
    packages.sort_by(|left, right| left.root.cmp(&right.root));
    packages
}

pub(crate) fn collect_metadata_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    if is_managed_provider_root(dir) {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if !matches!(
                name.as_str(),
                ".git"
                    | "node_modules"
                    | "vendor"
                    | "target"
                    | "build"
                    | "dist"
                    | "obj"
                    | "bin"
                    | ".venv"
                    | "venv"
                    | "__pycache__"
                    | ".dart_tool"
                    | ".gradle"
                    | ".cache"
                    | ".code_memory"
            ) {
                collect_metadata_files(&path, files);
            }
        } else if file_type.is_file() && is_metadata_file(&name) {
            files.push(path);
        }
    }
}

pub(crate) fn is_metadata_file(name: &str) -> bool {
    matches!(
        name,
        "package.json"
            | "pyproject.toml"
            | "setup.cfg"
            | "setup.py"
            | "requirements.txt"
            | "Cargo.toml"
            | "go.mod"
            | "composer.json"
            | "Gemfile"
            | "pubspec.yaml"
            | "pom.xml"
            | "build.gradle"
            | "build.gradle.kts"
    ) || name.ends_with(".csproj")
}

pub(crate) fn parse_package_metadata(
    file: &str,
    source: &str,
) -> (String, Option<String>, Option<String>) {
    match file {
        "package.json" | "composer.json" => {
            let value: Value = serde_json::from_str(source).unwrap_or(Value::Null);
            let ecosystem = if file == "package.json" {
                "npm"
            } else {
                "composer"
            };
            (
                ecosystem.to_string(),
                value
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                value
                    .get("version")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            )
        }
        "Cargo.toml" => (
            "cargo".to_string(),
            toml_value(source, "name").or_else(|| Some("cargo-package".to_string())),
            toml_value(source, "version"),
        ),
        "pyproject.toml" => (
            "pypi".to_string(),
            toml_value(source, "name").or_else(|| Some("python-project".to_string())),
            toml_value(source, "version"),
        ),
        "setup.cfg" => (
            "pypi".to_string(),
            ini_value(source, "metadata", "name"),
            ini_value(source, "metadata", "version"),
        ),
        "setup.py" => (
            "pypi".to_string(),
            assignment_string(source, "name"),
            assignment_string(source, "version"),
        ),
        "go.mod" => (
            "go".to_string(),
            source
                .lines()
                .find_map(|line| line.trim().strip_prefix("module ").map(str::trim))
                .map(str::to_string),
            None,
        ),
        "pubspec.yaml" => (
            "pub".to_string(),
            yaml_value(source, "name"),
            yaml_value(source, "version"),
        ),
        "Gemfile" => ("gems".to_string(), Some("ruby-project".to_string()), None),
        "pom.xml" => (
            "maven".to_string(),
            xml_value(source, "artifactId").or_else(|| Some("java-project".to_string())),
            xml_value(source, "version"),
        ),
        "build.gradle" | "build.gradle.kts" => (
            "gradle".to_string(),
            assignment_string(source, "rootProject.name")
                .or_else(|| assignment_string(source, "project.name")),
            assignment_string(source, "version"),
        ),
        value if value.ends_with(".csproj") => (
            "nuget".to_string(),
            xml_value(source, "AssemblyName")
                .or_else(|| Some(value.trim_end_matches(".csproj").to_string())),
            xml_value(source, "Version"),
        ),
        "requirements.txt" => ("pypi".to_string(), Some("python-project".to_string()), None),
        _ => ("unknown".to_string(), None, None),
    }
}

pub(crate) fn toml_value(source: &str, key: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let (left, right) = line.split_once('=')?;
        (left.trim() == key).then(|| {
            right
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string()
        })
    })
}

fn ini_value(source: &str, section: &str, key: &str) -> Option<String> {
    let mut current_section = "";
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            current_section = trimmed.trim_matches(['[', ']']).trim();
            continue;
        }
        if current_section != section {
            continue;
        }
        let Some((left, right)) = trimmed.split_once('=') else {
            continue;
        };
        if left.trim() == key {
            let value = right
                .split_once('#')
                .map(|(value, _)| value)
                .unwrap_or(right)
                .trim()
                .trim_matches(['"', '\'']);
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn assignment_string(source: &str, key: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let trimmed = line.trim();
        let position = trimmed.find(key)?;
        let remainder = &trimmed[position + key.len()..];
        if !remainder.trim_start().starts_with('=') {
            return None;
        }
        let value = remainder
            .trim_start_matches([' ', '\t', '='])
            .trim_start_matches(['"', '\''])
            .split(['"', '\'', ',', ')'])
            .next()
            .unwrap_or("")
            .trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

pub(crate) fn yaml_value(source: &str, key: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let (left, right) = line.split_once(':')?;
        (left.trim() == key).then(|| {
            right
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string()
        })
    })
}

pub(crate) fn xml_value(source: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = source.find(&open)? + open.len();
    let end = source[start..].find(&close)? + start;
    Some(source[start..end].trim().to_string())
}

pub(crate) fn parse_imports(path: &str, language: &str, source: &str) -> Vec<ImportUse> {
    let mut imports = Vec::new();
    let mut go_import_block = false;
    for (index, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        let mut package = None;
        let mut alias = None;
        let mut member = None;
        match language {
            "javascript" | "typescript" => {
                package = quoted_after(trimmed, "from")
                    .or_else(|| quoted_after(trimmed, "require("))
                    .or_else(|| quoted_after(trimmed, "import("))
                    .or_else(|| {
                        trimmed
                            .starts_with("import ")
                            .then(|| first_quoted(trimmed).unwrap_or_default())
                    });
                if trimmed.starts_with("import ") {
                    alias = alias_before_from(trimmed);
                }
            }
            "python" => {
                if let Some(value) = trimmed.strip_prefix("from ") {
                    let (module, imported) = value
                        .split_once(" import ")
                        .map(|(module, imported)| {
                            (
                                module.trim(),
                                imported
                                    .split(',')
                                    .next()
                                    .unwrap_or(imported)
                                    .split_whitespace()
                                    .next()
                                    .unwrap_or(""),
                            )
                        })
                        .unwrap_or((value.split_whitespace().next().unwrap_or(""), ""));
                    package = (!module.is_empty()).then(|| module.to_string());
                    member = (!imported.is_empty()).then(|| imported.to_string());
                } else if let Some(value) = trimmed.strip_prefix("import ") {
                    let first = value.split(',').next().unwrap_or(value).trim();
                    let mut parts = first.split_whitespace();
                    package = parts.next().map(str::to_string);
                    if parts.next() == Some("as") {
                        alias = parts.next().map(str::to_string);
                    }
                }
            }
            "java" => {
                package = trimmed
                    .strip_prefix("import ")
                    .map(|value| value.trim_end_matches(';').trim().to_string());
            }
            "csharp" => {
                package = trimmed
                    .strip_prefix("using ")
                    .map(|value| value.trim_end_matches(';').trim().to_string());
            }
            "go" => {
                if go_import_block {
                    if trimmed == ")" {
                        go_import_block = false;
                    } else if !trimmed.starts_with("//") {
                        package = first_quoted(trimmed);
                    }
                } else if trimmed == "import (" {
                    go_import_block = true;
                } else if trimmed.starts_with("import ") {
                    package = first_quoted(trimmed);
                }
            }
            "rust" => {
                if let Some(value) = trimmed.strip_prefix("use ") {
                    let value = value
                        .trim_end_matches(';')
                        .split(" as ")
                        .next()
                        .unwrap_or(value)
                        .split('{')
                        .next()
                        .unwrap_or(value)
                        .trim();
                    package = (!value.is_empty()).then(|| value.to_string());
                } else if let Some(value) = trimmed.strip_prefix("extern crate ") {
                    package = Some(value.trim_end_matches(';').trim().to_string());
                } else if let Some(value) = trimmed
                    .strip_prefix("mod ")
                    .or_else(|| trimmed.strip_prefix("pub mod "))
                {
                    let value = value.trim();
                    if value.ends_with(';') {
                        package = Some(value.trim_end_matches(';').trim().to_string());
                    }
                }
            }
            "php" => {
                package = trimmed.strip_prefix("use ").map(|value| {
                    value
                        .split_whitespace()
                        .next()
                        .unwrap_or(value)
                        .trim_end_matches(';')
                        .to_string()
                });
            }
            "ruby" => package = quoted_after(trimmed, "require"),
            "dart" => {
                package = first_quoted(trimmed);
            }
            "c" | "cpp" => {
                package = include_target(trimmed);
            }
            _ => {}
        }
        let Some(package) = package.filter(|value| !value.is_empty()) else {
            continue;
        };
        imports.push(ImportUse {
            path: path.to_string(),
            language: language.to_string(),
            package,
            alias,
            member,
            line: index + 1,
        });
    }
    imports
}

pub(crate) fn quoted_after(line: &str, marker: &str) -> Option<String> {
    let start = line.find(marker)? + marker.len();
    first_quoted(&line[start..])
}

pub(crate) fn first_quoted(value: &str) -> Option<String> {
    let quote = value.find(['"', '\''])?;
    let delimiter = value.as_bytes()[quote] as char;
    let rest = &value[quote + 1..];
    let end = rest.find(delimiter)?;
    Some(rest[..end].to_string())
}

pub(crate) fn include_target(line: &str) -> Option<String> {
    let value = line.strip_prefix("#include")?.trim();
    let (open, close) = if value.starts_with('<') {
        ('<', '>')
    } else if value.starts_with('"') {
        ('"', '"')
    } else {
        return None;
    };
    let value = value.strip_prefix(open)?;
    let end = value.find(close)?;
    (!value[..end].is_empty()).then(|| value[..end].to_string())
}

pub(crate) fn alias_before_from(line: &str) -> Option<String> {
    let before = line.strip_prefix("import ")?.split(" from ").next()?.trim();
    if let Some(value) = before.strip_prefix("* as ") {
        return Some(value.trim().to_string());
    }
    if !before.starts_with('{') && !before.contains(',') {
        return Some(before.to_string());
    }
    None
}

pub(crate) fn external_library_label(package: &str) -> String {
    format!("{package} 라이브러리")
}

pub(crate) fn dynamic_call_marker(language: &str, line: &str) -> Option<&'static str> {
    let markers: &[&str] = match language {
        "javascript" | "typescript" => {
            &["eval(", "Function(", "Reflect.", "globalThis[", "window["]
        }
        "python" => &[
            "eval(",
            "exec(",
            "getattr(",
            "__import__(",
            "importlib.import_module(",
        ],
        "java" => &[
            "Class.forName(",
            "Method.invoke(",
            "Proxy.newProxyInstance(",
        ],
        "csharp" => &["Type.GetType(", "MethodInfo.Invoke(", "Assembly.Load("],
        "go" => &["plugin.Open("],
        "rust" => &["libloading", "Library::new("],
        "c" | "cpp" => &["dlsym(", "GetProcAddress("],
        "php" => &["call_user_func(", "call_user_func_array("],
        "ruby" => &["send(", "public_send(", "const_get("],
        "dart" => &["dart:mirrors", "MirrorSystem"],
        _ => &[],
    };
    markers.iter().copied().find(|marker| line.contains(marker))
}

pub(crate) fn framework_fact_label(
    kind: &str,
    path: Option<&str>,
    method: Option<&str>,
    symbol: &Option<String>,
) -> Option<(&'static str, String)> {
    let label = match kind {
        "HTTP_ROUTE" => format!("{} {}", method.unwrap_or("ANY"), path.unwrap_or("/")),
        "RPC_ENDPOINT" => symbol
            .as_deref()
            .map(short_symbol)
            .unwrap_or_else(|| "RPC endpoint".to_string()),
        "COMPONENT" => symbol
            .as_deref()
            .map(short_symbol)
            .unwrap_or_else(|| "component".to_string()),
        "EVENT_HANDLER" => symbol
            .as_deref()
            .map(short_symbol)
            .unwrap_or_else(|| "event handler".to_string()),
        "SCHEDULED_JOB" => "scheduled job".to_string(),
        "SERVER_ACTION" => "server action".to_string(),
        "SERVICE" => symbol
            .as_deref()
            .map(short_symbol)
            .unwrap_or_else(|| "service".to_string()),
        "ASYNC_CALLS" => "async operation".to_string(),
        _ => return None,
    };
    let node_kind = match kind {
        "HTTP_ROUTE" | "RPC_ENDPOINT" => "ENDPOINT",
        "COMPONENT" => "COMPONENT",
        "SERVICE" => "SERVICE",
        "SCHEDULED_JOB" | "SERVER_ACTION" => "JOB",
        "EVENT_HANDLER" | "ASYNC_CALLS" => "EVENT",
        _ => "FLOW_NODE",
    };
    Some((node_kind, label))
}

pub(crate) fn short_symbol(symbol: &str) -> String {
    let symbol = symbol.trim_end_matches('.');
    symbol
        .rsplit(['/', '#', '.'])
        .next()
        .unwrap_or(symbol)
        .trim_end_matches("()")
        .to_string()
}

pub(crate) fn is_local_or_standard(
    import: &ImportUse,
    local_prefixes: &HashSet<String>,
    sources: &HashMap<String, String>,
    source_index: &SourcePathIndex,
) -> bool {
    let package = import.package.as_str();
    if package.starts_with('.')
        || package.starts_with("crate")
        || package.starts_with("self")
        || package.starts_with("super")
        || matches!(
            package,
            "std" | "core" | "alloc" | "node:fs" | "node:path" | "node:url"
        )
    {
        return true;
    }
    if local_prefixes.iter().any(|prefix| {
        package == prefix
            || package.starts_with(&format!("{prefix}/"))
            || package.starts_with(&format!("{prefix}."))
            || package.starts_with(&format!("{prefix}::"))
            || package.starts_with(&format!("{prefix}\\"))
    }) {
        return true;
    }
    if matches!(import.language.as_str(), "c" | "cpp")
        && local_include_exists(import, sources, source_index)
    {
        return true;
    }
    if import.language == "python" {
        let relative = package.replace('.', "/");
        if sources.contains_key(&format!("{relative}.py"))
            || sources.contains_key(&format!("{relative}/__init__.py"))
        {
            return true;
        }
    }
    if (import.language == "java" || import.language == "csharp" || import.language == "php")
        && local_prefixes
            .iter()
            .any(|prefix| !prefix.is_empty() && package.starts_with(prefix))
    {
        return true;
    }
    if import.language == "go" && !package.contains('.') {
        return true;
    }
    if import.language == "rust" && matches!(package, "std" | "core" | "alloc") {
        return true;
    }
    if import.language == "csharp" && package.starts_with("System") {
        return true;
    }
    false
}

pub(crate) fn resolve_project_import(
    import: &ImportUse,
    sources: &HashMap<String, String>,
    packages: &[PackageInfo],
    php_namespace_index: &PhpNamespaceIndex,
    source_index: &SourcePathIndex,
) -> Option<String> {
    if matches!(import.language.as_str(), "c" | "cpp")
        && local_include_exists(import, sources, source_index)
    {
        let package = import.package.replace('\\', "/");
        if sources.contains_key(&package) {
            return Some(package);
        }
        let directory = import
            .path
            .rsplit_once('/')
            .map(|(value, _)| value)
            .unwrap_or("");
        let candidate = join_path(directory, &package);
        return sources.contains_key(&candidate).then_some(candidate);
    }

    match import.language.as_str() {
        "python" => resolve_python_import(import, sources, source_index),
        "go" => resolve_go_import(import, sources, packages, source_index),
        "rust" => resolve_rust_import(import, sources, source_index),
        "php" => resolve_php_namespace_import(import, php_namespace_index)
            .or_else(|| resolve_namespace_import(import, sources, packages, source_index)),
        "java" | "csharp" => resolve_namespace_import(import, sources, packages, source_index),
        "ruby" => resolve_relative_import(import, sources, &["rb"], source_index),
        "dart" => resolve_dart_import(import, sources, packages, source_index),
        _ => None,
    }
}

fn resolve_php_namespace_import(import: &ImportUse, index: &PhpNamespaceIndex) -> Option<String> {
    let value = import.package.trim_matches(['\\', '/']);
    index.get(value).cloned()
}

pub(crate) fn build_php_namespace_index(sources: &HashMap<String, String>) -> PhpNamespaceIndex {
    let mut index = HashMap::new();
    let mut ambiguous = HashSet::new();
    for (path, source) in sources {
        if !path.ends_with(".php") {
            continue;
        }
        let namespace = source.lines().find_map(|line| {
            line.trim()
                .strip_prefix("namespace ")
                .and_then(|value| value.trim_end_matches(';').split_whitespace().next())
                .map(str::to_string)
        });
        let namespace = namespace.unwrap_or_default();
        for line in source.lines() {
            let trimmed = line.trim();
            let trimmed = trimmed
                .strip_prefix("abstract ")
                .or_else(|| trimmed.strip_prefix("final "))
                .or_else(|| trimmed.strip_prefix("readonly "))
                .unwrap_or(trimmed);
            let Some((kind, rest)) =
                ["class", "interface", "trait", "enum"]
                    .iter()
                    .find_map(|kind| {
                        trimmed
                            .strip_prefix(&format!("{kind} "))
                            .map(|rest| (*kind, rest))
                    })
            else {
                continue;
            };
            let _ = kind;
            let Some(name) = rest
                .split([' ', '{', ':'])
                .next()
                .filter(|name| !name.is_empty())
            else {
                continue;
            };
            let key = if namespace.is_empty() {
                name.to_string()
            } else {
                format!("{namespace}\\{name}")
            };
            if ambiguous.contains(&key) {
                continue;
            }
            if index.insert(key.clone(), path.clone()).is_some() {
                index.remove(&key);
                ambiguous.insert(key);
            }
        }
    }
    index
}

fn resolve_python_import(
    import: &ImportUse,
    _sources: &HashMap<String, String>,
    source_index: &SourcePathIndex,
) -> Option<String> {
    let raw = import.package.as_str();
    let dots = raw.chars().take_while(|value| *value == '.').count();
    let body = raw.trim_start_matches('.').replace('.', "/");
    let mut directory = import
        .path
        .rsplit_once('/')
        .map(|(value, _)| value.to_string())
        .unwrap_or_default();
    for _ in 1..dots {
        directory = directory
            .rsplit_once('/')
            .map(|(value, _)| value.to_string())
            .unwrap_or_default();
    }
    let path = if dots == 0 {
        body
    } else {
        join_path(&directory, &body)
    };
    let mut candidates = vec![
        path.clone(),
        format!("{path}.py"),
        format!("{path}/__init__.py"),
    ];
    if let Some(member) = import.member.as_deref() {
        if !member.is_empty() && member != "*" {
            let member_path = join_path(&path, member);
            candidates.extend([
                format!("{member_path}.py"),
                format!("{member_path}/__init__.py"),
            ]);
        }
    }
    first_existing(source_index, &candidates)
}

fn resolve_go_import(
    import: &ImportUse,
    _sources: &HashMap<String, String>,
    packages: &[PackageInfo],
    source_index: &SourcePathIndex,
) -> Option<String> {
    let mut path = import.package.replace('\\', "/");
    if let Some(module) = packages
        .iter()
        .find(|package| package.ecosystem == "go" && path.starts_with(&package.name))
    {
        path = path
            .strip_prefix(&module.name)
            .unwrap_or(&path)
            .trim_start_matches('/')
            .to_string();
    }
    source_index.unique_go_module_file(&path)
}

fn resolve_rust_import(
    import: &ImportUse,
    _sources: &HashMap<String, String>,
    source_index: &SourcePathIndex,
) -> Option<String> {
    let mut path = import.package.as_str();
    let mut directory = import
        .path
        .rsplit_once('/')
        .map(|(value, _)| value.to_string())
        .unwrap_or_default();
    if path.starts_with("self::") {
        path = path.trim_start_matches("self::");
    } else if path.starts_with("super::") {
        path = path.trim_start_matches("super::");
        directory = directory
            .rsplit_once('/')
            .map(|(value, _)| value.to_string())
            .unwrap_or_default();
    } else if path.starts_with("crate::") {
        path = path.trim_start_matches("crate::");
        directory.clear();
    } else {
        // Rust 2018 permits a crate-root module in a use path without the
        // explicit crate:: prefix. Resolve it only when the corresponding
        // source module exists; an unresolved name remains external.
        let module = path.split("::").next().unwrap_or(path);
        let base = if directory.is_empty() {
            module.to_string()
        } else {
            join_path(&directory, module)
        };
        return first_existing(
            source_index,
            &[format!("{base}.rs"), format!("{base}/mod.rs")],
        );
    }
    let module = path.split("::").next().unwrap_or(path);
    let base = if directory.is_empty() {
        module.to_string()
    } else {
        join_path(&directory, module)
    };
    first_existing(
        source_index,
        &[format!("{base}.rs"), format!("{base}/mod.rs")],
    )
}

fn resolve_namespace_import(
    import: &ImportUse,
    _sources: &HashMap<String, String>,
    packages: &[PackageInfo],
    source_index: &SourcePathIndex,
) -> Option<String> {
    let mut path = import.package.replace(['\\', '.'], "/");
    for package in packages {
        if package.name.is_empty() {
            continue;
        }
        let prefix = package.name.replace(['\\', '.'], "/");
        if path == prefix || path.starts_with(&format!("{prefix}/")) {
            path = path
                .strip_prefix(&prefix)
                .unwrap_or(&path)
                .trim_start_matches('/')
                .to_string();
            break;
        }
    }
    let extensions = match import.language.as_str() {
        "java" => ["java", "", ""],
        "csharp" => ["cs", "", ""],
        "php" => ["php", "", ""],
        _ => ["", "", ""],
    };
    let candidates = extensions
        .iter()
        .filter(|extension| !extension.is_empty())
        .map(|extension| format!("{path}.{extension}"))
        .chain(std::iter::once(path.clone()))
        .collect::<Vec<_>>();
    first_existing(source_index, &candidates)
}

fn resolve_relative_import(
    import: &ImportUse,
    _sources: &HashMap<String, String>,
    extensions: &[&str],
    source_index: &SourcePathIndex,
) -> Option<String> {
    let raw = import.package.trim_start_matches("./");
    let directory = import
        .path
        .rsplit_once('/')
        .map(|(value, _)| value)
        .unwrap_or("");
    let base = if import.package.starts_with('.') {
        join_path(directory, raw)
    } else {
        raw.to_string()
    };
    let mut candidates = vec![base.clone()];
    candidates.extend(
        extensions
            .iter()
            .map(|extension| format!("{base}.{extension}")),
    );
    if extensions.len() == 1 && extensions[0] == "dart" {
        return exact_existing(source_index, &candidates);
    }
    first_existing(source_index, &candidates)
}

fn resolve_dart_import(
    import: &ImportUse,
    sources: &HashMap<String, String>,
    packages: &[PackageInfo],
    source_index: &SourcePathIndex,
) -> Option<String> {
    let raw = import.package.as_str();
    if raw.starts_with("package:") {
        let value = raw.trim_start_matches("package:");
        let (package_name, relative) = value.split_once('/').unwrap_or((value, ""));
        let package_root = packages
            .iter()
            .find(|package| package.ecosystem == "pub" && package.name == package_name)
            .map(|package| package.root.as_str())
            .unwrap_or("");
        let path = join_path(package_root, &format!("lib/{relative}"));
        // package: imports already carry a package root. Do not fall back to
        // a whole-project suffix scan: an unavailable external package must
        // stay external, never become an ambiguous internal edge.
        return exact_existing(source_index, &[path.clone(), format!("{path}.dart")]);
    }
    if raw.starts_with("dart:") {
        return None;
    }
    resolve_relative_import(import, sources, &["dart"], source_index)
}

fn join_path(directory: &str, value: &str) -> String {
    let value = value.replace('\\', "/").trim_matches('/').to_string();
    if directory.is_empty() {
        value
    } else if value.is_empty() {
        directory.replace('\\', "/")
    } else {
        format!(
            "{}/{}",
            directory.replace('\\', "/").trim_end_matches('/'),
            value
        )
    }
}

fn first_existing(source_index: &SourcePathIndex, candidates: &[String]) -> Option<String> {
    if let Some(exact) = candidates
        .iter()
        .find_map(|candidate| source_index.exact(candidate))
    {
        return Some(exact);
    }
    for candidate in candidates {
        if let Some(unique) = source_index.unique_suffix(candidate) {
            return Some(unique);
        }
    }
    None
}

fn exact_existing(source_index: &SourcePathIndex, candidates: &[String]) -> Option<String> {
    candidates
        .iter()
        .find_map(|candidate| source_index.exact(candidate))
}

pub(crate) fn local_include_exists(
    import: &ImportUse,
    sources: &HashMap<String, String>,
    source_index: &SourcePathIndex,
) -> bool {
    let package = import.package.replace('\\', "/");
    if sources.contains_key(&package) {
        return true;
    }
    if import
        .path
        .rsplit_once('/')
        .map(|(parent, _)| format!("{parent}/{package}"))
        .is_some_and(|candidate| sources.contains_key(&candidate))
    {
        return true;
    }

    // Include paths from the compiler command are not available to this
    // lexical layer. A unique project-relative suffix is safe to resolve;
    // ambiguity stays external so the graph never invents a target.
    source_index.unique_suffix(&package).is_some()
}

pub(crate) fn normalize_external_package(package: &str, language: &str) -> String {
    let package = package.trim().trim_matches(['"', '\'']);
    match language {
        "javascript" | "typescript" => {
            if package.starts_with('@') {
                package.split('/').take(2).collect::<Vec<_>>().join("/")
            } else {
                package.split('/').next().unwrap_or(package).to_string()
            }
        }
        "python" => package.split('.').next().unwrap_or(package).to_string(),
        "java" => package.split('.').take(2).collect::<Vec<_>>().join("."),
        "rust" => package.split("::").next().unwrap_or(package).to_string(),
        "dart" => package
            .trim_start_matches("package:")
            .split('/')
            .next()
            .unwrap_or(package)
            .to_string(),
        "php" => package
            .split(['\\', '/'])
            .take(2)
            .collect::<Vec<_>>()
            .join("\\"),
        _ => package.to_string(),
    }
}

pub(crate) fn ecosystem_for_language(language: &str) -> &'static str {
    match language {
        "javascript" | "typescript" => "npm",
        "python" => "pypi",
        "java" => "maven",
        "csharp" => "nuget",
        "rust" => "cargo",
        "go" => "go",
        "php" => "composer",
        "ruby" => "gems",
        "dart" => "pub",
        _ => "system",
    }
}

pub(crate) fn language_for_path(path: &str) -> Option<&'static str> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".vue") {
        return Some("typescript");
    }
    LANGUAGES.iter().find_map(|language| {
        language
            .extensions
            .iter()
            .any(|extension| lower.ends_with(&format!(".{extension}")))
            .then_some(language.id)
    })
}

pub(crate) fn relative_path(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|value| value.to_string_lossy().replace('\\', "/"))
}

pub(crate) fn package_id(package: &PackageInfo) -> String {
    format!("package:{}:{}", package.ecosystem, package.root)
}

pub(crate) fn nearest_package<'a>(
    path: &str,
    packages: &'a [PackageInfo],
) -> Option<&'a PackageInfo> {
    packages
        .iter()
        .filter(|package| {
            package.root.is_empty()
                || path == package.root
                || path.starts_with(&format!("{}/", package.root))
        })
        .max_by_key(|package| package.root.len())
}

pub(crate) fn contains_any(value: &str, markers: &[&str]) -> bool {
    markers.iter().any(|marker| value.contains(marker))
}

pub(crate) fn contains_any_ascii_case_insensitive(value: &str, markers: &[&str]) -> bool {
    markers.iter().any(|marker| {
        let marker = marker.as_bytes();
        !marker.is_empty()
            && value
                .as_bytes()
                .windows(marker.len())
                .any(|window| window.eq_ignore_ascii_case(marker))
    })
}

pub(crate) fn static_database_operation(line: &str) -> Option<&'static str> {
    let trimmed = line.trim_start();
    if trimmed.is_empty()
        || trimmed.starts_with('#')
        || trimmed.starts_with("//")
        || trimmed.starts_with("--")
        || trimmed.contains('+')
        || trimmed.contains("${")
    {
        return None;
    }
    let database_call = contains_any(
        trimmed,
        &[
            ".execute(",
            ".executemany(",
            ".query(",
            ".raw(",
            "createQuery(",
        ],
    ) && !contains_any(
        trimmed,
        &["logger.query(", "logger.execute(", "logger.raw("],
    );
    let assigned_sql =
        assignment_name(trimmed).is_some_and(|name| matches!(name, "sql" | "query" | "statement"));
    if !database_call && !assigned_sql {
        return None;
    }
    if contains_any_ascii_case_insensitive(trimmed, &["SELECT ", "SELECT\t"]) {
        Some("READ")
    } else if contains_any_ascii_case_insensitive(
        trimmed,
        &["INSERT ", "UPDATE ", "DELETE ", "UPSERT "],
    ) {
        Some("WRITE")
    } else if database_call {
        Some("DB_CALL")
    } else {
        None
    }
}

fn assignment_name(line: &str) -> Option<&str> {
    let left = line.split_once('=')?.0.trim();
    let name = left
        .split_whitespace()
        .last()?
        .trim_matches(|character: char| !character.is_ascii_alphanumeric() && character != '_');
    (!name.is_empty()).then_some(name)
}
