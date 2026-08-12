//! Vue `defineComponent` 경계를 CodeUnit marker로 보존한다.
use crate::facts::FactStore;
use crate::frameworks::common::components::{mark_components, ComponentMarkerRule};
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    mark_components(
        facts,
        detections,
        ComponentMarkerRule {
            framework_id: "javascript.vue",
            modifier: "framework:vue-component",
            decorator_names: &[],
            call_names: &["defineComponent"],
            signature_tokens: &[],
        },
    );
}
