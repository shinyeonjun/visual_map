//! Flask route와 Blueprint prefix를 처리한다.

use super::super::common::{add_routes, RoutePolicy};
use crate::facts::FactStore;
use std::collections::HashMap;

pub(super) fn enrich(facts: &mut FactStore, file_frameworks: &HashMap<String, Vec<String>>) {
    add_routes(
        facts,
        file_frameworks,
        RoutePolicy {
            framework_id: "python.flask",
            constructors: &["Blueprint"],
            registrations: &["register_blueprint"],
            route_names: &["route"],
            websocket_names: &[],
        },
    );
}
