//! Tauri JavaScript/TypeScript의 `invoke` 이벤트 경계를 추출한다.
use crate::facts::FactStore;
use crate::frameworks::common::events::{add_call_events, CallEventRule};
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    add_call_events(
        facts,
        detections,
        CallEventRule {
            framework_id: "javascript.tauri",
            call_names: &["invoke"],
            event_name_argument_index: 0,
            method: "TAURI_INVOKE",
        },
    );
}
