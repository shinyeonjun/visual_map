//! Java Spring Boot adapter 경계.
use crate::facts::FactStore;
use crate::frameworks::common::components::{mark_components, ComponentMarkerRule};
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    mark_components(
        facts,
        detections,
        ComponentMarkerRule {
            framework_id: "java.spring_boot",
            modifier: "framework:spring-boot-application",
            decorator_names: &["SpringBootApplication"],
            call_names: &[],
            signature_tokens: &[],
        },
    );
}
