//! ASP.NET MVC의 attribute route와 controller 경계를 추출한다.
use crate::facts::FactStore;
use crate::frameworks::common::annotations::compose_class_route_prefixes;
use crate::frameworks::common::decorators::{add_decorator_routes, DecoratorRouteRule};
use crate::frameworks::registry::detector::FrameworkDetection;

const ROUTES: &[&str] = &[
    "HttpGet",
    "HttpPost",
    "HttpPut",
    "HttpPatch",
    "HttpDelete",
    "HttpHead",
    "Route",
];

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    add_decorator_routes(
        facts,
        detections,
        DecoratorRouteRule {
            framework_id: "csharp.aspnet_mvc",
            controller_names: &["RoutePrefix", "Route"],
            route_names: ROUTES,
            websocket_names: &[],
            method_argument_index: None,
        },
    );
    compose_class_route_prefixes(facts, &["csharp.aspnet_mvc"]);
}
