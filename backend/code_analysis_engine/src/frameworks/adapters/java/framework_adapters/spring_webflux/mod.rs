//! Java Spring WebFlux route와 reactive controller 경계를 보강한다.
use crate::facts::{EntrypointKind, FactStore};
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    let Some(detection) = detections
        .iter()
        .find(|detection| detection.id == "java.spring_webflux")
    else {
        return;
    };
    let webflux_files = detection
        .evidence
        .iter()
        .map(|evidence| evidence.span.file_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let file_by_unit = facts
        .units
        .values()
        .map(|unit| (unit.id.as_str(), unit.file_id.as_str()))
        .collect::<std::collections::HashMap<_, _>>();
    for entrypoint in &mut facts.entrypoints {
        if entrypoint.kind != EntrypointKind::Http {
            continue;
        }
        let Some(file_id) = file_by_unit.get(entrypoint.unit_id.as_str()) else {
            continue;
        };
        if webflux_files.contains(file_id) {
            entrypoint.framework_id = Some("java.spring_webflux".to_string());
        }
    }
}
