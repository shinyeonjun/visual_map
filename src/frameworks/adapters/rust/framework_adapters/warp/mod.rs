//! Rust Warp의 명시적인 `warp::path(...)` filter 경계를 추출한다.
use crate::facts::{EntrypointKind, FactStore};
use crate::frameworks::common::routes::{add_call_routes, CallRouteRule};
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    add_call_routes(
        facts,
        detections,
        CallRouteRule {
            framework_id: "rust.warp",
            call_names: &["path"],
            receiver_names: &["warp"],
            receiver_constructors: &[],
            route_methods: &[],
            fixed_method: Some("HTTP_FILTER"),
            path_argument_index: 0,
            target_argument_index: None,
            prefix_methods: &[],
            kind: EntrypointKind::Http,
        },
    );
}
