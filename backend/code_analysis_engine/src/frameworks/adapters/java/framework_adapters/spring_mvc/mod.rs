//! Java Spring MVC adapter 경계.
use crate::facts::FactStore;
use crate::frameworks::common::components::{mark_components, ComponentMarkerRule};
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    mark_components(
        facts,
        detections,
        ComponentMarkerRule {
            framework_id: "java.spring_mvc",
            modifier: "framework:spring-mvc-controller",
            decorator_names: &["Controller", "RestController", "RequestMapping"],
            call_names: &[],
            signature_tokens: &[],
        },
    );
}
