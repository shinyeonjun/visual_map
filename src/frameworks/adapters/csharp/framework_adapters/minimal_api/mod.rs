//! ASP.NET Minimal API의 `MapGet` 계열 route를 추출한다.

use crate::facts::{EntrypointKind, FactStore};
use crate::frameworks::common::routes::{add_call_routes, CallRouteRule};
use crate::frameworks::registry::detector::FrameworkDetection;

const METHODS: &[&str] = &[
    "MapGet",
    "MapPost",
    "MapPut",
    "MapPatch",
    "MapDelete",
    "MapMethods",
];

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    add_call_routes(
        facts,
        detections,
        CallRouteRule {
            framework_id: "csharp.minimal_api",
            call_names: METHODS,
            receiver_names: &["app", "group", "webApp", "groupBuilder"],
            receiver_constructors: &["CreateBuilder", "Build"],
            route_methods: &["Get", "Post", "Put", "Patch", "Delete", "Methods"],
            fixed_method: None,
            path_argument_index: 0,
            target_argument_index: Some(1),
            prefix_methods: &["MapGroup"],
            kind: EntrypointKind::Http,
        },
    );
}
