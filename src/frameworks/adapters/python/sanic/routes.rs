//! Sanic decorator와 Blueprint 등록을 처리한다.

use super::super::common::{add_routes, RoutePolicy};
use crate::facts::FactStore;
use std::collections::HashMap;

pub(super) fn enrich(facts: &mut FactStore, file_frameworks: &HashMap<String, Vec<String>>) {
    add_routes(
        facts,
        file_frameworks,
        RoutePolicy {
            framework_id: "python.sanic",
            constructors: &["Blueprint"],
            registrations: &["register"],
            route_names: &[
                "get", "post", "put", "patch", "delete", "options", "head", "route",
            ],
            websocket_names: &["websocket"],
        },
    );
}
