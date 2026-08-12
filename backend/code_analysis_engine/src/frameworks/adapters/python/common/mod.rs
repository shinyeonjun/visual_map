//! Python 프레임워크 adapter가 공유하는 작은 변환 도구다.

mod arguments;
mod calls;
mod routes;
mod symbols;

pub(crate) use calls::{add_call_entrypoint, source_file_id, CallEntrypointSpec};
pub(crate) use routes::{add_routes, RoutePolicy};
pub(crate) use symbols::SymbolIndex;

use crate::frameworks::registry::detector::FrameworkDetection;
use std::collections::HashMap;

/// 감지 근거가 있는 파일마다 Python 프레임워크 ID를 연결한다.
pub(super) fn index_file_frameworks(
    detections: &[FrameworkDetection],
) -> HashMap<String, Vec<String>> {
    let mut result: HashMap<String, Vec<String>> = HashMap::new();
    for detection in detections {
        if !detection.id.starts_with("python.") {
            continue;
        }
        for evidence in &detection.evidence {
            let ids = result.entry(evidence.span.file_id.clone()).or_default();
            if !ids.contains(&detection.id) {
                ids.push(detection.id.clone());
            }
        }
    }
    result
}

pub(super) use arguments::{join_paths, methods_argument, route_path};
