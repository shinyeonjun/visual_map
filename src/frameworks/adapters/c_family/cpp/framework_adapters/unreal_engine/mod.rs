//! C++ Unreal Engine의 UObject/Actor 컴포넌트 경계를 보존한다.
use crate::facts::FactStore;
use crate::frameworks::common::components::{mark_components, ComponentMarkerRule};
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    mark_components(
        facts,
        detections,
        ComponentMarkerRule {
            framework_id: "cpp.unreal_engine",
            modifier: "framework:unreal-component",
            decorator_names: &[],
            call_names: &[],
            signature_tokens: &["UObject", "AActor", "APawn", "UCLASS"],
        },
    );
}
