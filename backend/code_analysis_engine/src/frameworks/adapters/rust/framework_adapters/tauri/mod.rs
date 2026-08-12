//! Tauri Rust command attribute를 외부 이벤트 진입점으로 보존한다.
use crate::facts::EntrypointKind;
use crate::facts::FactStore;
use crate::frameworks::common::decorators::{add_decorator_entrypoints, DecoratorEntrypointRule};
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    add_decorator_entrypoints(
        facts,
        detections,
        DecoratorEntrypointRule {
            framework_id: "rust.tauri",
            receiver: Some("tauri"),
            names: &["command"],
            kind: EntrypointKind::Event,
            method: "TAURI_COMMAND",
        },
    );
}
