//! Rust Rocket의 attribute route를 HTTP 진입점으로 추출한다.
use crate::facts::FactStore;
use crate::frameworks::common::decorators::{add_decorator_routes, DecoratorRouteRule};
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    add_decorator_routes(
        facts,
        detections,
        DecoratorRouteRule {
            framework_id: "rust.rocket",
            controller_names: &[],
            route_names: &["get", "post", "put", "patch", "delete", "head", "options"],
            websocket_names: &["get_ws", "ws"],
            method_argument_index: None,
        },
    );
}
