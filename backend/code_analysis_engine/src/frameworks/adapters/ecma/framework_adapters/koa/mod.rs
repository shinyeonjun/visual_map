//! Koa router의 method route를 추출한다.

use crate::facts::{EntrypointKind, FactStore};
use crate::frameworks::common::routes::{add_call_routes, CallRouteRule};
use crate::frameworks::registry::detector::FrameworkDetection;

const METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "options", "head", "all",
];

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    add_call_routes(
        facts,
        detections,
        CallRouteRule {
            framework_id: "javascript.koa",
            call_names: &[],
            receiver_names: &["router", "api", "app"],
            receiver_constructors: &["Router", "Koa"],
            route_methods: METHODS,
            fixed_method: None,
            path_argument_index: 0,
            target_argument_index: Some(1),
            prefix_methods: &["prefix"],
            kind: EntrypointKind::Http,
        },
    );
}
