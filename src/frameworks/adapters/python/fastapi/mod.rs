//! FastAPI adapter.

mod routes;

use super::common::{add_routes, RoutePolicy};
use crate::facts::FactStore;
use std::collections::HashMap;

pub(super) fn enrich(facts: &mut FactStore, file_frameworks: &HashMap<String, Vec<String>>) {
    routes::enrich(facts, file_frameworks);
}

pub(super) fn policy() -> RoutePolicy {
    RoutePolicy {
        framework_id: "python.fastapi",
        constructors: &["APIRouter"],
        registrations: &["include_router"],
        route_names: &[
            "get",
            "post",
            "put",
            "patch",
            "delete",
            "options",
            "head",
            "route",
            "api_route",
        ],
        websocket_names: &["websocket"],
    }
}
