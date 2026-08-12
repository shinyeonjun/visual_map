//! Tokio main attribute를 프로그램 메인 진입점으로 보존한다.
use crate::facts::EntrypointKind;
use crate::facts::FactStore;
use crate::frameworks::common::decorators::{add_decorator_entrypoints, DecoratorEntrypointRule};
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    add_decorator_entrypoints(
        facts,
        detections,
        DecoratorEntrypointRule {
            framework_id: "rust.tokio",
            receiver: Some("tokio"),
            names: &["main"],
            kind: EntrypointKind::Main,
            method: "MAIN",
        },
    );
}
