use super::capabilities::{FrameworkCapability, FrameworkKind};
use super::catalog::supported_frameworks;
use crate::config::FrameworkPolicy;
use crate::facts::{Evidence, SourceSpan};
use crate::model::{FileEntry, Language};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// 감지된 프레임워크와 그 근거다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkDetection {
    pub id: String,
    pub display_name: String,
    pub kind: FrameworkKind,
    pub capabilities: Vec<FrameworkCapability>,
    pub parent: Option<String>,
    pub languages: Vec<String>,
    pub confidence: f32,
    pub evidence: Vec<Evidence>,
}

/// 프로젝트 소스와 manifest에서 지원 catalog의 프레임워크를 감지한다.
pub fn detect(
    root: &Path,
    files: &[FileEntry],
    policy: &FrameworkPolicy,
) -> Vec<FrameworkDetection> {
    let mut contents: Vec<(FileEntry, String)> = Vec::new();
    for file in files {
        if is_internal_catalog_path(&file.relative_path, policy) {
            continue;
        }
        if let Ok(source) = fs::read_to_string(root.join(&file.relative_path)) {
            contents.push((file.clone(), source));
        }
    }

    for (manifest, path) in collect_manifest_sources(root, policy) {
        if let Ok(source) = fs::read_to_string(&path) {
            contents.push((
                FileEntry {
                    file_id: format!("manifest:{manifest}"),
                    relative_path: manifest.clone(),
                    language: Language::Unknown,
                    size_bytes: source.len() as u64,
                    line_count: source.lines().count() as u64,
                    modified_unix_ms: None,
                    content_hash: None,
                    is_test: false,
                    parse_status: crate::model::ParseStatus::NotAnalyzed,
                },
                source,
            ));
        }
    }

    let mut detections: BTreeMap<String, FrameworkDetection> = BTreeMap::new();
    for spec in supported_frameworks() {
        for (file, source) in &contents {
            if file.language != Language::Unknown
                && !framework_language_matches(file, &spec.languages)
            {
                continue;
            }

            let Some((marker, line)) = spec
                .markers
                .iter()
                .find_map(|marker| marker_line(file, source, marker))
            else {
                continue;
            };
            let evidence = Evidence::new(
                "framework",
                marker.to_string(),
                SourceSpan::new(
                    file.file_id.clone(),
                    file.relative_path.clone(),
                    line,
                    1,
                    line,
                    source
                        .lines()
                        .nth(line.saturating_sub(1) as usize)
                        .map(|v| v.len())
                        .unwrap_or(1) as u32
                        + 1,
                ),
            );
            let entry =
                detections
                    .entry(spec.id.to_string())
                    .or_insert_with(|| FrameworkDetection {
                        id: spec.id.to_string(),
                        display_name: spec.display_name.to_string(),
                        kind: spec.kind,
                        capabilities: spec.capabilities.to_vec(),
                        parent: spec.parent.clone(),
                        languages: spec
                            .languages
                            .iter()
                            .map(|value| (*value).to_string())
                            .collect(),
                        confidence: policy.initial_confidence,
                        evidence: Vec::new(),
                    });
            entry.evidence.push(evidence);
            entry.confidence =
                (entry.confidence + policy.confidence_increment).min(policy.maximum_confidence);
        }
    }

    detections.into_values().collect()
}

/// C/C++ 프로젝트는 `.h` 헤더를 양쪽 언어가 함께 사용한다. 스캐너가
/// 기본적으로 `.h`를 C로 분류하더라도 C++ 프레임워크의 명시적인 include나
/// DSL marker가 있는 헤더는 프레임워크 감지에서 제외하지 않는다.
fn framework_language_matches(file: &FileEntry, languages: &[String]) -> bool {
    if languages
        .iter()
        .any(|language| language == file.language.key())
    {
        return true;
    }
    let is_shared_c_header = file.language == Language::C
        && Path::new(&file.relative_path)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("h"));
    is_shared_c_header && languages.iter().any(|language| language == "cpp")
}

/// 프레임워크 이름이 주석·설명 문자열에 등장했다는 이유로 감지하지 않는다.
/// manifest는 의존성 이름 자체가 문자열이므로 원문 검색을 허용하고, 소스는
/// import/include·호출·attribute 문맥에서만 marker를 근거로 사용한다.
fn marker_line<'a>(file: &FileEntry, source: &str, marker: &'a str) -> Option<(&'a str, u32)> {
    if is_manifest_path(&file.relative_path) {
        return manifest_marker_line(file, source, marker);
    }
    if file.language == Language::Unknown {
        return source
            .to_ascii_lowercase()
            .lines()
            .enumerate()
            .find(|(_, line)| contains_marker(line, marker))
            .map(|(index, _)| (marker, index as u32 + 1));
    }

    let marker_lower = marker.to_ascii_lowercase();
    for (index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//")
            || (trimmed.starts_with('#')
                && !trimmed.starts_with("#include")
                && !trimmed.starts_with("#["))
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*')
        {
            continue;
        }
        let raw_lower = line.to_ascii_lowercase();
        let trimmed = line.trim_start();
        let is_import_context = trimmed.starts_with("import ")
            || trimmed.starts_with("from ")
            || trimmed.starts_with("use ")
            || trimmed.starts_with("#include")
            || trimmed.starts_with("using ");
        let is_file_marker = marker.starts_with('.')
            && file
                .relative_path
                .to_ascii_lowercase()
                .contains(&marker_lower);
        let code = code_without_literals_and_comments(line);
        let lower = code.to_ascii_lowercase();
        if !(contains_marker(&lower, &marker_lower)
            || is_import_context && contains_marker(&raw_lower, &marker_lower)
            || is_file_marker)
        {
            continue;
        }
        let is_attribute_context =
            trimmed.starts_with('@') || trimmed.starts_with("#[") || trimmed.starts_with('[');
        let is_call_context = lower.contains(&format!("{marker_lower}("))
            || lower.contains(&format!("{marker_lower}."))
            || lower.contains(&format!("{marker_lower}::"));
        let is_type_context = marker.chars().next().is_some_and(char::is_uppercase)
            && (contains_marker(&lower, &format!("new {marker_lower}"))
                || contains_marker(&lower, &format!("extends {marker_lower}"))
                || contains_marker(&lower, &format!("implements {marker_lower}"))
                || contains_marker(&lower, &format!(": {marker_lower}"))
                || contains_marker(&lower, &format!("<{marker_lower}")));
        if is_import_context
            || is_attribute_context
            || is_call_context
            || is_type_context
            || is_file_marker
        {
            return Some((marker, index as u32 + 1));
        }
    }
    None
}

fn collect_manifest_sources(root: &Path, policy: &FrameworkPolicy) -> Vec<(String, PathBuf)> {
    let names = policy
        .manifests
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<std::collections::HashSet<_>>();
    let mut pending = vec![root.to_path_buf()];
    let mut result = Vec::new();
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if !is_ignored_manifest_directory(&path) {
                    pending.push(path);
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if !names.contains(&name.to_ascii_lowercase()) {
                continue;
            }
            let Some(relative) = path
                .strip_prefix(root)
                .ok()
                .and_then(|value| value.to_str())
                .map(|value| value.replace('\\', "/"))
            else {
                continue;
            };
            result.push((relative, path));
        }
    }
    result.sort_by(|left, right| left.0.cmp(&right.0));
    result
}

fn is_ignored_manifest_directory(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                ".git"
                    | "node_modules"
                    | "target"
                    | "build"
                    | "dist"
                    | "out"
                    | "coverage"
                    | "vendor"
                    | ".venv"
                    | "venv"
                    | "__pycache__"
                    | ".dart_tool"
            )
        })
        .unwrap_or(false)
}

fn is_manifest_path(path: &str) -> bool {
    matches!(
        path.rsplit('/')
            .next()
            .unwrap_or(path)
            .to_ascii_lowercase()
            .as_str(),
        "package.json"
            | "cargo.toml"
            | "pom.xml"
            | "build.gradle"
            | "requirements.txt"
            | "pyproject.toml"
            | "pubspec.yaml"
            | "go.mod"
    )
}

fn manifest_marker_line<'a>(
    file: &FileEntry,
    source: &str,
    marker: &'a str,
) -> Option<(&'a str, u32)> {
    let name = file.relative_path.rsplit('/').next().unwrap_or_default();
    let matched = match name.to_ascii_lowercase().as_str() {
        "package.json" => json_dependency_contains(source, marker),
        "cargo.toml" | "pyproject.toml" => toml_dependency_contains(source, marker),
        "requirements.txt" => source.lines().enumerate().find_map(|(index, line)| {
            let dependency = line.split('#').next()?.trim();
            let dependency = dependency
                .split(['=', '<', '>', '!', '~', '['])
                .next()?
                .trim();
            contains_package_name(dependency, marker).then_some(index as u32 + 1)
        }),
        _ => source
            .lines()
            .enumerate()
            .find(|(_, line)| contains_marker(line, marker))
            .map(|(index, _)| index as u32 + 1),
    }?;
    Some((marker, matched))
}

fn json_dependency_contains(source: &str, marker: &str) -> Option<u32> {
    let value = serde_json::from_str::<serde_json::Value>(source).ok()?;
    for section in [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
        "bundledDependencies",
    ] {
        let Some(values) = value.get(section) else {
            continue;
        };
        let names = values
            .as_object()
            .map(|object| object.keys().map(String::as_str).collect::<Vec<_>>())
            .or_else(|| {
                values
                    .as_array()
                    .map(|items| items.iter().filter_map(|item| item.as_str()).collect())
            })
            .unwrap_or_default();
        if names.iter().any(|name| contains_package_name(name, marker)) {
            return source
                .lines()
                .enumerate()
                .find(|(_, line)| contains_marker(line, marker))
                .map(|(index, _)| index as u32 + 1)
                .or(Some(1));
        }
    }
    None
}

fn toml_dependency_contains(source: &str, marker: &str) -> Option<u32> {
    let value = toml::from_str::<toml::Value>(source).ok()?;
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        let Some(table) = value.get(section).and_then(toml::Value::as_table) else {
            continue;
        };
        if table.keys().any(|name| contains_package_name(name, marker)) {
            return source
                .lines()
                .enumerate()
                .find(|(_, line)| contains_marker(line, marker))
                .map(|(index, _)| index as u32 + 1)
                .or(Some(1));
        }
    }
    None
}

fn contains_package_name(name: &str, marker: &str) -> bool {
    let name = name.trim().to_ascii_lowercase();
    let marker = marker.trim().trim_end_matches('(').trim_end_matches('/');
    name == marker.to_ascii_lowercase()
        || (marker.contains('/') && name.starts_with(&marker.to_ascii_lowercase()))
        || (marker.contains('-') && name.starts_with(&marker.to_ascii_lowercase()))
}

fn contains_marker(value: &str, marker: &str) -> bool {
    if marker.is_empty() {
        return false;
    }
    let value = value.to_ascii_lowercase();
    let marker = marker.to_ascii_lowercase();
    let mut offset = 0;
    while let Some(relative) = value[offset..].find(&marker) {
        let start = offset + relative;
        let end = start + marker.len();
        let before = value[..start].chars().next_back();
        let after = value[end..].chars().next();
        let marker_is_identifier = marker
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_');
        if !marker_is_identifier
            || !before
                .is_some_and(|character| character.is_ascii_alphanumeric() || character == '_')
                && !after
                    .is_some_and(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            return true;
        }
        offset = end;
    }
    false
}

/// 소스 코드에서 문자열·주석 안의 marker는 framework 근거로 사용하지 않는다.
/// 여러 언어의 문자열 표기가 달라도 한 줄 단위 감지에서는 동일한 보수적
/// 상태 기계로 처리할 수 있다.
fn code_without_literals_and_comments(line: &str) -> String {
    let trimmed = line.trim_start();
    let preserve_hash = trimmed.starts_with("#include") || trimmed.starts_with("#[");
    let mut result = String::with_capacity(line.len());
    let mut quote = None;
    let mut escaped = false;
    let mut chars = line.chars().peekable();

    while let Some(character) = chars.next() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == active_quote {
                quote = None;
            }
            result.push(' ');
            continue;
        }

        if matches!(character, '"' | '\'' | '`') {
            quote = Some(character);
            result.push(' ');
            continue;
        }
        if character == '/' && chars.peek() == Some(&'/') {
            break;
        }
        if character == '#' && !preserve_hash {
            break;
        }
        result.push(character);
    }
    result
}

fn is_internal_catalog_path(path: &str, policy: &FrameworkPolicy) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    policy
        .internal_catalog_markers
        .iter()
        .any(|marker| normalized.contains(&marker.to_ascii_lowercase()))
}
