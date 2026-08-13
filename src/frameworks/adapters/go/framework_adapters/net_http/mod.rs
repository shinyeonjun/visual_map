//! Go net/http의 `Handle`·`HandleFunc` 진입점을 추출한다.

use crate::facts::{EntrypointKind, FactStore};
use crate::frameworks::common::routes::{add_call_routes, CallRouteRule};
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    add_call_routes(
        facts,
        detections,
        CallRouteRule {
            framework_id: "go.net_http",
            call_names: &["Handle", "HandleFunc"],
            receiver_names: &["http", "DefaultServeMux"],
            receiver_constructors: &[],
            route_methods: &[],
            fixed_method: Some("HTTP"),
            path_argument_index: 0,
            target_argument_index: Some(1),
            prefix_methods: &[],
            kind: EntrypointKind::Http,
        },
    );
}
