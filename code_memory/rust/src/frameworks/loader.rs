use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::{FrameworkFixture, FrameworkFixtureFile, FrameworkPack};
use crate::source::is_managed_provider_root;
use crate::LANGUAGES;

pub(crate) fn load_packs(root: &Path) -> Result<Vec<FrameworkPack>, String> {
    let catalog_path = root.join("packs").join("framework").join("catalog.json");
    let catalog = read_json(&catalog_path)?;
    if catalog.get("schema").and_then(Value::as_str)
        != Some("code-memory.framework-pack-catalog.v1")
    {
        return Err("invalid framework catalog schema".to_string());
    }
    let adapter_catalog_path = root.join("packs").join("framework").join("adapters.json");
    let adapter_catalog = read_json(&adapter_catalog_path)?;
    if adapter_catalog.get("schema").and_then(Value::as_str)
        != Some("code-memory.framework-adapter-catalog.v1")
    {
        return Err("invalid framework adapter catalog schema".to_string());
    }
    let adapters = adapter_catalog
        .get("adapters")
        .and_then(Value::as_object)
        .ok_or("framework adapter catalog has no adapters")?
        .iter()
        .filter_map(|(key, value)| value.as_str().map(|value| (key.clone(), value.to_string())))
        .collect::<HashMap<_, _>>();

    let mut packs = Vec::new();
    let mut seen = HashSet::new();
    for language in catalog
        .get("languages")
        .and_then(Value::as_array)
        .ok_or("framework catalog has no languages")?
    {
        let language_id = required_string(language, "id")?;
        if !LANGUAGES.iter().any(|item| item.id == language_id) {
            return Err(format!("unsupported framework language: {language_id}"));
        }
        let language_file = required_string(language, "file")?;
        let language_catalog = catalog_path
            .parent()
            .unwrap_or(Path::new("."))
            .join(language_file);
        let document = read_json(&language_catalog)?;
        if document.get("schema").and_then(Value::as_str)
            != Some("code-memory.framework-pack-catalog.v1")
            || document.get("language").and_then(Value::as_str) != Some(language_id.as_str())
        {
            return Err(format!(
                "invalid language framework catalog: {}",
                language_catalog.display()
            ));
        }
        for reference in document
            .get("packs")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                format!(
                    "framework catalog has no packs: {}",
                    language_catalog.display()
                )
            })?
        {
            let pack_id = required_string(reference, "id")?;
            let pack_file = required_string(reference, "path")?;
            let pack_path = language_catalog
                .parent()
                .unwrap_or(Path::new("."))
                .join(pack_file);
            let pack = read_json(&pack_path)?;
            let pack_language = required_string(&pack, "language")?;
            let pack_manifest_id = required_string(&pack, "id")?;
            let qualified = format!("{pack_language}/{pack_manifest_id}");
            if pack.get("schema").and_then(Value::as_str) != Some("code-memory.framework-pack.v1")
                || pack_language != language_id
                || pack_manifest_id != pack_id
                || !seen.insert(qualified.clone())
            {
                return Err(format!(
                    "invalid or duplicate framework pack: {}",
                    pack_path.display()
                ));
            }
            let outputs = string_array(&pack, "outputs")?;
            let name = required_string(&pack, "name")?;
            let kind = required_string(&pack, "kind")?;
            let signals = string_array(&pack, "signals")?;
            let rules = string_array(&pack, "rule_sets")?;
            let adapter = adapters
                .get(&qualified)
                .cloned()
                .ok_or_else(|| format!("framework pack has no adapter: {qualified}"))?;
            if !matches!(
                adapter.as_str(),
                "registration-routing"
                    | "annotation-routing"
                    | "filesystem-routing"
                    | "component-events"
                    | "rpc-service"
                    | "async-events"
                    | "event-or-declaration"
            ) {
                return Err(format!("framework pack has invalid adapter: {qualified}"));
            }
            if rules
                .iter()
                .any(|rule| !adapter_supports_rule(&adapter, rule))
            {
                return Err(format!(
                    "framework adapter {adapter} cannot execute a declared rule: {qualified}"
                ));
            }
            let fixture_path = pack_path
                .parent()
                .unwrap_or(Path::new("."))
                .join("fixture.json");
            let fixture_document = read_json(&fixture_path)?;
            if fixture_document.get("schema").and_then(Value::as_str)
                != Some("code-memory.framework-fixture.v1")
                || fixture_document.get("language").and_then(Value::as_str)
                    != Some(pack_language.as_str())
                || fixture_document.get("framework").and_then(Value::as_str)
                    != Some(pack_manifest_id.as_str())
            {
                return Err(format!(
                    "invalid framework fixture: {}",
                    fixture_path.display()
                ));
            }
            let files = fixture_document
                .get("files")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    format!("framework fixture has no files: {}", fixture_path.display())
                })?
                .iter()
                .map(|file| {
                    Ok(FrameworkFixtureFile {
                        path: required_string(file, "path")?,
                        source: required_string(file, "source")?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            if files.is_empty() {
                return Err(format!(
                    "framework fixture has no files: {}",
                    fixture_path.display()
                ));
            }
            let expected = fixture_document.get("expected").ok_or_else(|| {
                format!(
                    "framework fixture has no expected block: {}",
                    fixture_path.display()
                )
            })?;
            let expected_facts = string_array(expected, "facts")?;
            let expected_relations = optional_string_array(expected, "relations")?;
            if expected_facts != rules {
                return Err(format!(
                    "framework fixture facts do not match pack rules: {}",
                    fixture_path.display()
                ));
            }
            if expected_relations
                .iter()
                .any(|relation| !outputs.iter().any(|output| output == relation))
            {
                return Err(format!(
                    "framework fixture relations do not match pack outputs: {}",
                    fixture_path.display()
                ));
            }
            packs.push(FrameworkPack {
                id: pack_manifest_id,
                language: pack_language,
                name,
                kind: kind.clone(),
                signals,
                outputs,
                rules: rules.clone(),
                adapter,
                fixture: FrameworkFixture {
                    files,
                    expected_facts,
                    expected_relations,
                },
            });
        }
    }
    if adapters.len() != packs.len() {
        return Err(format!(
            "framework adapter count mismatch: adapters={}, packs={}",
            adapters.len(),
            packs.len()
        ));
    }
    Ok(packs)
}

pub(crate) fn adapter_supports_rule(adapter: &str, rule: &str) -> bool {
    match adapter {
        "registration-routing" => matches!(rule, "HTTP_ROUTE" | "MIDDLEWARE" | "SERVICE"),
        "annotation-routing" => matches!(
            rule,
            "HTTP_ROUTE" | "MIDDLEWARE" | "SERVICE" | "DEPENDENCY" | "ASYNC_CALLS" | "SCHEMA"
        ),
        "filesystem-routing" => {
            matches!(
                rule,
                "HTTP_ROUTE" | "COMPONENT" | "SERVER_ACTION" | "MIDDLEWARE"
            )
        }
        "component-events" => matches!(
            rule,
            "HTTP_ROUTE" | "SERVICE" | "COMPONENT" | "RENDERS" | "EVENT_HANDLER" | "ASYNC_CALLS"
        ),
        "rpc-service" => matches!(rule, "RPC_ENDPOINT" | "SERVICE" | "ASYNC_CALLS"),
        "async-events" => matches!(rule, "ASYNC_CALLS" | "EVENT_HANDLER" | "SCHEDULED_JOB"),
        "event-or-declaration" => matches!(
            rule,
            "COMPONENT"
                | "RENDERS"
                | "EVENT_HANDLER"
                | "SERVICE"
                | "DEPENDENCY"
                | "ASYNC_CALLS"
                | "RPC_ENDPOINT"
                | "SCHEMA"
                | "SCHEDULED_JOB"
        ),
        _ => false,
    }
}

pub(crate) fn read_json(path: &Path) -> Result<Value, String> {
    serde_json::from_slice(
        &fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?,
    )
    .map_err(|e| format!("invalid JSON {}: {e}", path.display()))
}

pub(crate) fn required_string(value: &Value, field: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("framework manifest has no {field}"))
}

pub(crate) fn string_array(value: &Value, field: &str) -> Result<Vec<String>, String> {
    let values = value
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("framework manifest has no {field}"))?;
    let result: Vec<String> = values
        .iter()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect();
    if result.is_empty() {
        return Err(format!("framework manifest has empty {field}"));
    }
    Ok(result)
}

pub(crate) fn optional_string_array(value: &Value, field: &str) -> Result<Vec<String>, String> {
    let values = value
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("framework manifest has no {field}"))?;
    Ok(values
        .iter()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect())
}

pub(crate) fn signal_needle(signal: &str) -> String {
    signal
        .split_once(':')
        .map(|(_, value)| value.to_string())
        .unwrap_or_else(|| signal.to_string())
}

pub(crate) fn framework_identity_confirmed(
    pack: &FrameworkPack,
    matched_signals: &HashSet<String>,
) -> bool {
    let identity_signals = pack.signals.iter().filter(|signal| {
        signal.split_once(':').is_some_and(|(prefix, _)| {
            matches!(
                prefix,
                "import" | "require" | "include" | "#include" | "package"
            )
        })
    });
    let mut declared = false;
    for signal in identity_signals {
        declared = true;
        if matched_signals.contains(signal) {
            return true;
        }
    }
    !declared
}

pub(crate) fn source_signal_matches(relative: &str, source: &str, signal: &str) -> bool {
    let needle = signal_needle(signal);
    let prefix = signal.split_once(':').map(|(prefix, _)| prefix);
    if prefix.is_none() && relative.contains(&needle) {
        return true;
    }
    match prefix {
        Some("import") => {
            source.contains(&format!("import {needle}"))
                || source.contains(&format!("from {needle}"))
                || source.contains(&format!("from \"{needle}"))
                || source.contains(&format!("from '{needle}"))
                || source.contains(&format!("import(\"{needle}"))
                || source.contains(&format!("import('{needle}"))
        }
        Some("require") => source.contains("require(") && source.contains(&needle),
        Some("include") => source.contains("#include") && source.contains(&needle),
        Some("package") => package_source_signal_matches(relative, source, &needle),
        _ => source.contains(&needle),
    }
}

fn package_source_signal_matches(relative: &str, source: &str, needle: &str) -> bool {
    match Path::new(relative)
        .extension()
        .and_then(|value| value.to_str())
    {
        Some("dart") => source.lines().any(|line| {
            line.trim_start().starts_with("import ") && line.contains(&format!("package:{needle}"))
        }),
        Some("go") => source.lines().any(|line| {
            let line = line.trim();
            let import_line = line.starts_with("import ")
                || line.starts_with('"')
                || line
                    .split_whitespace()
                    .nth(1)
                    .is_some_and(|value| value.starts_with('"'));
            import_line && line.contains(&format!("\"{needle}"))
        }),
        Some("rs") => source.lines().any(|line| {
            let line = line.trim();
            line.strip_prefix("use ")
                .is_some_and(|value| value.starts_with(&format!("{needle}::")))
                || line
                    .strip_prefix("extern crate ")
                    .is_some_and(|value| value.trim_end_matches(';') == needle)
        }),
        _ => false,
    }
}

pub(crate) fn metadata_signal_matches(relative: &str, source: &str, signal: &str) -> bool {
    let needle = signal_needle(signal);
    let prefix = signal.split_once(':').map(|(prefix, _)| prefix);
    if matches!(prefix, Some("import" | "require"))
        && matches!(
            Path::new(relative)
                .file_name()
                .and_then(|value| value.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("package.json" | "package-lock.json")
        )
    {
        return serde_json::from_str::<serde_json::Value>(source)
            .ok()
            .is_some_and(|value| package_declares_dependency(&value, &needle));
    }
    source.contains(&needle)
}

fn package_declares_dependency(value: &serde_json::Value, needle: &str) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
        "packages",
    ]
    .iter()
    .any(|key| {
        object
            .get(*key)
            .and_then(serde_json::Value::as_object)
            .is_some_and(|dependencies| {
                dependencies
                    .keys()
                    .any(|name| name == needle || name.ends_with(&format!("/{needle}")))
            })
    })
}

pub(crate) fn metadata_matches_language(relative: &str, language: &str) -> bool {
    let file_name = Path::new(relative)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match language {
        "javascript" | "typescript" => {
            matches!(file_name.as_str(), "package.json" | "package-lock.json")
        }
        "python" => matches!(file_name.as_str(), "pyproject.toml" | "requirements.txt"),
        "java" | "kotlin" => matches!(
            file_name.as_str(),
            "pom.xml" | "build.gradle" | "build.gradle.kts"
        ),
        "c" | "cpp" => matches!(
            file_name.as_str(),
            "cmakelists.txt" | "vcpkg.json" | "conanfile.txt"
        ),
        "go" => file_name == "go.mod",
        "rust" => file_name == "cargo.toml",
        "dart" => file_name == "pubspec.yaml",
        "csharp" => file_name.ends_with(".csproj") || file_name.ends_with(".sln"),
        _ => false,
    }
}

pub(crate) fn metadata_scope(relative: &str) -> String {
    Path::new(relative)
        .parent()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
}

pub(crate) fn path_is_in_scope(path: &str, root: &str) -> bool {
    root.is_empty() || path == root || path.starts_with(&format!("{root}/"))
}

pub(crate) fn collect_metadata_sources(root: &Path) -> Vec<(String, String)> {
    let mut paths = Vec::new();
    collect_metadata_paths(root, &mut paths);
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| {
            let text = fs::read_to_string(&path).ok()?;
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            Some((relative, text))
        })
        .collect()
}

pub(crate) fn path_matches_language(path: &str, extensions: &[&str]) -> bool {
    extensions
        .iter()
        .any(|extension| path.ends_with(&format!(".{extension}")))
}

pub(crate) fn cached_source_signal_match(
    cache: &mut HashMap<(String, String), bool>,
    relative: &str,
    text: &str,
    signal: &str,
) -> bool {
    let key = (relative.to_string(), signal.to_string());
    if let Some(value) = cache.get(&key) {
        return *value;
    }
    let value = source_signal_matches(relative, text, signal);
    cache.insert(key, value);
    value
}

pub(crate) fn cached_metadata_signal_match(
    cache: &mut HashMap<(String, String), bool>,
    relative: &str,
    text: &str,
    signal: &str,
) -> bool {
    let key = (relative.to_string(), signal.to_string());
    if let Some(value) = cache.get(&key) {
        return *value;
    }
    let value = metadata_signal_matches(relative, text, signal);
    cache.insert(key, value);
    value
}

pub(crate) fn collect_metadata_paths(dir: &Path, paths: &mut Vec<PathBuf>) {
    if is_managed_provider_root(dir) {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !matches!(
                name.as_str(),
                ".git"
                    | "node_modules"
                    | "target"
                    | "build"
                    | "dist"
                    | "vendor"
                    | ".dart_tool"
                    | ".gradle"
                    | "packages"
                    | ".code_memory"
            ) {
                collect_metadata_paths(&path, paths);
            }
        } else {
            let file_name = entry.file_name().to_string_lossy().to_string();
            let is_metadata = matches!(
                file_name.as_str(),
                "package.json"
                    | "package-lock.json"
                    | "pom.xml"
                    | "build.gradle"
                    | "build.gradle.kts"
                    | "pyproject.toml"
                    | "requirements.txt"
                    | "go.mod"
                    | "Cargo.toml"
                    | "pubspec.yaml"
                    | "CMakeLists.txt"
                    | "vcpkg.json"
                    | "conanfile.txt"
                    | "Package.swift"
            ) || file_name.ends_with(".csproj")
                || file_name.ends_with(".sln");
            if is_metadata {
                paths.push(path);
            }
        }
    }
}
