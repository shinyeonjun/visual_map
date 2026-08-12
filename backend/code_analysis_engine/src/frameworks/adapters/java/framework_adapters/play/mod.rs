//! Java Play의 확장자 없는 `conf/routes` DSL을 HTTP 진입점으로 연결한다.
use crate::facts::FactStore;
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    if !detections
        .iter()
        .any(|detection| detection.id == "java.play")
    {
        return;
    }
    let route_entrypoint_ids = facts
        .entrypoints
        .iter()
        .filter(|entrypoint| entrypoint.path.is_some())
        .filter(|entrypoint| {
            facts
                .unit(&entrypoint.unit_id)
                .is_some_and(|unit| unit.language == crate::model::Language::Unknown)
        })
        .map(|entrypoint| entrypoint.id.clone())
        .collect::<Vec<_>>();
    for entrypoint in &mut facts.entrypoints {
        if route_entrypoint_ids.iter().any(|id| id == &entrypoint.id) {
            entrypoint.framework_id = Some("java.play".to_string());
        }
    }
}
