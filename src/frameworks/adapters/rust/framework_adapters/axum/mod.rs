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
            // Tree-sitter는 `Router::new().route(...)`의 fluent receiver를
            // 별도 이름으로 보존하지 않고 마지막 `route` callee만 준다.
            // Axum 감지 결과가 있는 파일 안에서만 route 호출을 허용한다.
            receiver_names: &[],
            receiver_constructors: &[],
            route_methods: &[],
            fixed_method: Some("HTTP"),
            path_argument_index: 0,
            target_argument_index: None,
            prefix_methods: &["nest"],
            kind: EntrypointKind::Http,
        },
    );
}
