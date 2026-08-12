//! Axum의 `Router::route`·`nest` builder를 추출한다.

use crate::facts::{EntrypointKind, FactStore};
use crate::frameworks::common::routes::{add_call_routes, CallRouteRule};
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    add_call_routes(
        facts,
        detections,
        CallRouteRule {
            framework_id: "rust.axum",
            call_names: &["route"],
            receiver_names: &["app", "router", "api"],
            receiver_constructors: &["new", "Router"],
            route_methods: &[],
            fixed_method: Some("HTTP"),
            path_argument_index: 0,
            target_argument_index: None,
            prefix_methods: &["nest"],
            kind: EntrypointKind::Http,
        },
    );
}
