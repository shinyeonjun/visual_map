//! Express의 `app.get(...)`·`router.post(...)` route를 추출한다.

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
            framework_id: "javascript.express",
            call_names: &[],
            receiver_names: &["app", "router", "route", "api", "server"],
            receiver_constructors: &["express", "Router"],
            route_methods: METHODS,
            fixed_method: None,
            path_argument_index: 0,
            target_argument_index: Some(1),
            prefix_methods: &["use", "route"],
            kind: EntrypointKind::Http,
        },
    );
    // express-ws와 유사한 확장은 기존 HTTP route와 별개의 외부 경계다.
    // `ws`/`websocket` 메서드가 실제로 보일 때만 WebSocket으로 분류한다.
    add_call_routes(
        facts,
        detections,
        CallRouteRule {
            framework_id: "javascript.express",
            call_names: &[],
            receiver_names: &["app", "router", "route", "api", "server"],
            receiver_constructors: &[],
            route_methods: &["ws", "websocket"],
            fixed_method: None,
            path_argument_index: 0,
            target_argument_index: Some(1),
            prefix_methods: &["use"],
            kind: EntrypointKind::WebSocket,
        },
    );
}
