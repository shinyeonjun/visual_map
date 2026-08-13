//! Starlette decorator 형태로 작성된 간단한 route를 처리한다.
//!
//! `Route(...)` 리스트와 `WebSocketRoute(...)` 생성자 형태의 URLConf는 아직
//! 별도 extractor 대상이다. 여기서는 Python 분석기가 decorator로 표현할 수
//! 있는 형태만 공통 route로 materialize한다.

use super::super::common::{
    add_call_entrypoint, add_routes, source_file_id, CallEntrypointSpec, RoutePolicy,
};
use crate::facts::EntrypointKind;
use crate::facts::FactStore;
use std::collections::HashMap;

pub(super) fn enrich(facts: &mut FactStore, file_frameworks: &HashMap<String, Vec<String>>) {
    // FastAPI 파일은 Starlette 의존성도 함께 감지될 수 있으므로 중복 진입점을
    // 만들지 않는다. FastAPI adapter가 이미 해당 파일을 소유한다.
    let starlette_only = file_frameworks
        .iter()
        .filter(|(_, ids)| ids.iter().any(|id| id == "python.starlette"))
        .filter(|(_, ids)| !ids.iter().any(|id| id == "python.fastapi"))
        .map(|(file, ids)| (file.clone(), ids.clone()))
        .collect::<HashMap<_, _>>();
    add_routes(
        facts,
        &starlette_only,
        RoutePolicy {
            framework_id: "python.starlette",
            constructors: &["Router"],
            registrations: &["mount"],
            route_names: &["route"],
            websocket_names: &["websocket"],
        },
    );

    // Starlette의 URLConf는 decorator가 아니라 `Route(path, endpoint)`와
    // `WebSocketRoute(path, endpoint)` 생성자로 선언되는 경우가 많다.
    let call_count = facts.call_sites.len();
    for index in 0..call_count {
        let Some(call) = facts.call_sites.get(index) else {
            continue;
        };
        let Some(file_id) = source_file_id(facts, &call.source_unit_id) else {
            continue;
        };
        if !starlette_only.contains_key(file_id) {
            continue;
        }
        let constructor = call.callee.rsplit('.').next().unwrap_or(&call.callee);
        let (kind, method) = match constructor {
            "Route" => (EntrypointKind::Http, "HTTP"),
            "WebSocketRoute" => (EntrypointKind::WebSocket, "WEBSOCKET"),
            _ => continue,
        };
        add_call_entrypoint(
            facts,
            index,
            CallEntrypointSpec {
                framework_id: "python.starlette",
                kind,
                method,
                path_argument_index: 0,
                target_argument_index: Some(1),
                evidence_kind: "starletteRoute",
            },
        );
    }
}
