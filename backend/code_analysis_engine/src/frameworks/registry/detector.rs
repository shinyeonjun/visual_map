use super::capabilities::{FrameworkCapability, FrameworkKind};
use super::catalog::supported_frameworks;
use crate::config::FrameworkPolicy;
use crate::facts::{Evidence, SourceSpan};
use crate::model::{FileEntry, Language};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

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

    for manifest in &policy.manifests {
        let path = root.join(manifest);
        if let Ok(source) = fs::read_to_string(&path) {
            contents.push((
                FileEntry {
                    file_id: format!("manifest:{manifest}"),
                    relative_path: manifest.to_string(),
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
    if file.language == Language::Unknown {
        return source
            .to_ascii_lowercase()
            .contains(&marker.to_ascii_lowercase())
            .then_some((marker, 1));
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
        if !(lower.contains(&marker_lower)
            || is_import_context && raw_lower.contains(&marker_lower)
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
            && (lower.contains(&format!("new {marker_lower}"))
                || lower.contains(&format!("extends {marker_lower}"))
                || lower.contains(&format!("implements {marker_lower}"))
                || lower.contains(&format!(": {marker_lower}"))
                || lower.contains(&format!("<{marker_lower}")));
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
