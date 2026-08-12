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

            let lower = source.to_ascii_lowercase();
            let Some(marker) = spec
                .markers
                .iter()
                .find(|marker| lower.contains(&marker.to_ascii_lowercase()))
            else {
                continue;
            };

            let line = source
                .lines()
                .position(|line| {
                    line.to_ascii_lowercase()
                        .contains(&marker.to_ascii_lowercase())
                })
                .map(|line| line as u32 + 1)
                .unwrap_or(1);
            let evidence = Evidence::new(
                "framework",
                marker.clone(),
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

fn is_internal_catalog_path(path: &str, policy: &FrameworkPolicy) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    policy
        .internal_catalog_markers
        .iter()
        .any(|marker| normalized.contains(&marker.to_ascii_lowercase()))
}
