//! 프레임워크 감지 결과를 공통 Facts에 연결하는 보조 기능.

pub mod annotations;
pub mod callbacks;
pub mod components;
pub mod decorators;
pub mod events;
pub mod file_routes;
pub mod graphql;
pub mod inherited;
pub mod route_path;
pub mod routes;
pub mod rpc;

use crate::facts::{FactStore, FrameworkFact};
use crate::frameworks::registry::detector::FrameworkDetection;
use std::collections::{HashMap, HashSet};

/// 프레임워크 감지 결과를 유닛 적용 여부 조회용으로 역색인한다.
///
/// 유닛 하나를 검사할 때마다 모든 detection evidence를 다시 순회하지
/// 않도록 파일·언어별 적용 대상을 한 번만 계산한다.
#[derive(Debug, Clone)]
pub struct FrameworkApplicabilityIndex {
    by_id: HashMap<String, Vec<String>>,
    by_file: HashMap<String, HashSet<String>>,
    by_language: HashMap<String, Vec<String>>,
}

impl FrameworkApplicabilityIndex {
    pub fn new(detections: &[FrameworkDetection]) -> Self {
        let mut index = Self {
            by_id: HashMap::new(),
            by_file: HashMap::new(),
            by_language: HashMap::new(),
        };
        for detection in detections {
            index
                .by_id
                .insert(detection.id.clone(), detection.languages.clone());
            for language in &detection.languages {
                index
                    .by_language
                    .entry(language.clone())
                    .or_default()
                    .push(detection.id.clone());
            }
            for evidence in &detection.evidence {
                index
                    .by_file
                    .entry(evidence.span.file_id.clone())
                    .or_default()
                    .insert(detection.id.clone());
            }
        }
        for detection_ids in index.by_language.values_mut() {
            detection_ids.sort();
            detection_ids.dedup();
        }
        index
    }

    pub fn applies(&self, facts: &FactStore, unit_id: &str, framework_id: &str) -> bool {
        let Some(unit) = facts.unit(unit_id) else {
            return false;
        };
        let Some(languages) = self.by_id.get(framework_id) else {
            return false;
        };
        if !unit_language_matches_framework(unit, languages) {
            return false;
        }
        if let Some(local_detections) = self.by_file.get(&unit.file_id) {
            return local_detections.contains(framework_id);
        }
        self.by_language
            .get(unit.language.key())
            .is_some_and(|detection_ids| detection_ids.len() == 1)
    }
}

fn unit_language_matches_framework(unit: &crate::facts::CodeUnit, languages: &[String]) -> bool {
    if languages
        .iter()
        .any(|language| language == unit.language.key())
    {
        return true;
    }
    let is_shared_c_header = unit.language == crate::model::Language::C
        && std::path::Path::new(&unit.relative_path)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("h"));
    is_shared_c_header && languages.iter().any(|language| language == "cpp")
}

/// 프레임워크 adapter를 특정 코드 유닛에 적용할 수 있는지 판정한다.
///
/// 같은 프로젝트에 여러 프레임워크가 섞일 수 있으므로, 소스 파일에 남은
/// detection 근거가 있으면 그 파일의 근거만 사용한다. 해당 파일에 근거가
/// 없을 때는 같은 언어의 감지가 하나뿐인 경우에만 전역 적용한다. 모호한
/// 경우에는 프레임워크를 추측하지 않고 generic 정적 사실로 남긴다.
pub fn framework_applies_to_unit(
    facts: &FactStore,
    unit_id: &str,
    detections: &[FrameworkDetection],
    framework_id: &str,
) -> bool {
    FrameworkApplicabilityIndex::new(detections).applies(facts, unit_id, framework_id)
}

/// 프레임워크 감지 근거와 같은 파일의 진입점에 프레임워크 ID를 연결한다.
///
/// route/annotation의 문법 추출은 언어 분석기가 담당하고, 이 함수는 감지 결과와
/// 이미 추출된 공통 사실을 연결하는 역할만 담당한다.
pub fn attach_framework_ids(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    facts.frameworks = detections
        .iter()
        .map(|detection| FrameworkFact {
            id: detection.id.clone(),
            display_name: detection.display_name.clone(),
            kind: format!("{:?}", detection.kind).to_ascii_lowercase(),
            capabilities: detection
                .capabilities
                .iter()
                .map(|capability| format!("{capability:?}").to_ascii_lowercase())
                .collect(),
            parent: detection.parent.clone(),
            confidence: detection.confidence,
            evidence: detection.evidence.clone(),
        })
        .collect();

    for entrypoint in &mut facts.entrypoints {
        // adapter가 구문과 프레임워크를 함께 확정한 entrypoint는 보존한다.
        // 같은 파일에 여러 프레임워크가 섞인 경우 일반적인 파일 근접 추정이
        // 이미 확정된 의미를 덮어쓰면 안 된다.
        if entrypoint.framework_id.is_some() {
            continue;
        }
        let Some(unit) = facts.units.get(&entrypoint.unit_id) else {
            continue;
        };
        let Some(detection) = best_detection_for_entrypoint(entrypoint, &unit.file_id, detections)
        else {
            continue;
        };
        entrypoint.framework_id = Some(detection.id.clone());
    }
}

/// 같은 파일에 감지된 프레임워크가 여러 개일 때 진입점 근거와 가장 가까운
/// marker를 선택한다. 목록 순서에 의존하면 한 파일의 모든 entrypoint가
/// 임의의 프레임워크로 분류되는 문제가 생긴다.
fn best_detection_for_entrypoint<'a>(
    entrypoint: &crate::facts::Entrypoint,
    file_id: &str,
    detections: &'a [FrameworkDetection],
) -> Option<&'a FrameworkDetection> {
    let entry_line = entrypoint
        .evidence
        .first()
        .map(|evidence| evidence.span.start_line);
    detections
        .iter()
        .filter(|detection| {
            detection
                .evidence
                .iter()
                .any(|evidence| evidence.span.file_id == file_id)
        })
        .min_by_key(|detection| {
            detection
                .evidence
                .iter()
                .filter(|evidence| evidence.span.file_id == file_id)
                .map(|evidence| {
                    entry_line
                        .map(|line| line.abs_diff(evidence.span.start_line))
                        .unwrap_or(0)
                })
                .min()
                .unwrap_or(u32::MAX)
        })
}
