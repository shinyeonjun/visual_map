//! Rust Poem의 handler macro를 callback 경계로 보존한다.
use crate::facts::{EntrypointKind, FactStore};
use crate::frameworks::common::decorators::{add_decorator_entrypoints, DecoratorEntrypointRule};
use crate::frameworks::common::routes::{add_call_routes, CallRouteRule};
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    add_call_routes(
        facts,
        detections,
        CallRouteRule {
            framework_id: "rust.poem",
            call_names: &["at"],
            receiver_names: &[],
            receiver_constructors: &[],
            route_methods: &[],
            fixed_method: Some("HTTP"),
            path_argument_index: 0,
            target_argument_index: None,
            prefix_methods: &[],
            kind: EntrypointKind::Http,
        },
    );
    add_decorator_entrypoints(
        facts,
        detections,
        DecoratorEntrypointRule {
            framework_id: "rust.poem",
            receiver: None,
            names: &["handler"],
            kind: EntrypointKind::Callback,
            method: "POEM_HANDLER",
        },
    );
}
