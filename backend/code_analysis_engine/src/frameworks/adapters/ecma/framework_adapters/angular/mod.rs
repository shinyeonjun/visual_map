//! Angular `@Component` 경계를 CodeUnit marker로 보존한다.
use crate::facts::FactStore;
use crate::frameworks::common::components::{mark_components, ComponentMarkerRule};
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    mark_components(
        facts,
        detections,
        ComponentMarkerRule {
            framework_id: "javascript.angular",
            modifier: "framework:angular-component",
            decorator_names: &["Component"],
            call_names: &[],
            signature_tokens: &[],
        },
    );
}
