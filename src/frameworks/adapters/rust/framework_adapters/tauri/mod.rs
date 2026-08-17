//! Tauri Rust command attribute를 프론트엔드에서 호출하는 RPC 진입점으로 보존한다.
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
            kind: EntrypointKind::Rpc,
            method: "TAURI_COMMAND",
        },
    );
}
