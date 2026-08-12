//! .NET MAUI ContentPage/View 상속 경계를 CodeUnit marker로 보존한다.
use crate::facts::FactStore;
use crate::frameworks::common::components::{mark_components, ComponentMarkerRule};
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    mark_components(
        facts,
        detections,
        ComponentMarkerRule {
            framework_id: "csharp.dotnet_maui",
            modifier: "framework:maui-component",
            decorator_names: &[],
            call_names: &[],
            signature_tokens: &["ContentPage", "ContentView", "Window"],
        },
    );
}
