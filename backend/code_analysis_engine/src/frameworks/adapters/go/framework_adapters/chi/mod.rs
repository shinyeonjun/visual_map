//! Go Chi의 method route와 `Route`·`Group` scope를 추출한다.

use crate::facts::{EntrypointKind, FactStore};
use crate::frameworks::common::routes::{add_call_routes, CallRouteRule};
use crate::frameworks::registry::detector::FrameworkDetection;

const METHODS: &[&str] = &[
    "Get", "Post", "Put", "Patch", "Delete", "Options", "Head", "Method",
];

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    add_call_routes(
        facts,
        detections,
        CallRouteRule {
            framework_id: "go.chi",
            call_names: &[],
            receiver_names: &["r", "router", "route", "group"],
            receiver_constructors: &["NewRouter"],
            route_methods: METHODS,
            fixed_method: None,
            path_argument_index: 0,
            target_argument_index: Some(1),
            prefix_methods: &["Route", "Group"],
            kind: EntrypointKind::Http,
        },
    );
}
