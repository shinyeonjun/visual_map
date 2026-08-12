//! NestJS `@Controller`·HTTP decorator adapter.

use crate::facts::FactStore;
use crate::frameworks::common::decorators::{add_decorator_routes, DecoratorRouteRule};
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    add_decorator_routes(
        facts,
        detections,
        DecoratorRouteRule {
            framework_id: "javascript.nestjs",
            controller_names: &["Controller"],
            route_names: &[
                "Get", "Post", "Put", "Patch", "Delete", "Options", "Head", "All",
            ],
            websocket_names: &["SubscribeMessage"],
            method_argument_index: None,
        },
    );
}
