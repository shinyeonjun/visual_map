//! Rust Actix Web의 attribute route와 handler 경계를 추출한다.
use crate::facts::{EntrypointKind, FactStore};
use crate::frameworks::common::decorators::{
    add_decorator_entrypoints, add_decorator_routes, DecoratorEntrypointRule, DecoratorRouteRule,
};
use crate::frameworks::registry::detector::FrameworkDetection;

const HTTP_ROUTES: &[&str] = &[
    "get", "post", "put", "patch", "delete", "head", "options", "connect", "route",
];

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    add_decorator_routes(
        facts,
        detections,
        DecoratorRouteRule {
            framework_id: "rust.actix_web",
            controller_names: &[],
            route_names: HTTP_ROUTES,
            websocket_names: &[],
            method_argument_index: Some(1),
        },
    );
    add_decorator_entrypoints(
        facts,
        detections,
        DecoratorEntrypointRule {
            framework_id: "rust.actix_web",
            receiver: Some("actix_web"),
            names: &["main"],
            kind: EntrypointKind::Main,
            method: "MAIN",
        },
    );
}
