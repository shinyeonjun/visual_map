//! Go Gin의 method route와 `Group` prefix를 추출한다.

use crate::facts::{EntrypointKind, FactStore};
use crate::frameworks::common::routes::{add_call_routes, CallRouteRule};
use crate::frameworks::registry::detector::FrameworkDetection;

const METHODS: &[&str] = &[
    "GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "HEAD", "Any", "Match",
];

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    add_call_routes(
        facts,
        detections,
        CallRouteRule {
            framework_id: "go.gin",
            call_names: &[],
            receiver_names: &["router", "r", "group", "api", "v1"],
            receiver_constructors: &["Default", "New"],
            route_methods: METHODS,
            fixed_method: None,
            path_argument_index: 0,
            target_argument_index: Some(1),
            prefix_methods: &["Group"],
            kind: EntrypointKind::Http,
        },
    );
}
