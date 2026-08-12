//! Go Beego의 전역 method route를 추출한다.

use crate::facts::{EntrypointKind, FactStore};
use crate::frameworks::common::routes::{add_call_routes, CallRouteRule};
use crate::frameworks::registry::detector::FrameworkDetection;

const METHODS: &[&str] = &["Get", "Post", "Put", "Patch", "Delete", "Head", "Options"];

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    add_call_routes(
        facts,
        detections,
        CallRouteRule {
            framework_id: "go.beego",
            call_names: &[],
            receiver_names: &["beego", "app", "router"],
            receiver_constructors: &[],
            route_methods: METHODS,
            fixed_method: None,
            path_argument_index: 0,
            target_argument_index: Some(1),
            prefix_methods: &["Namespace"],
            kind: EntrypointKind::Http,
        },
    );
}
