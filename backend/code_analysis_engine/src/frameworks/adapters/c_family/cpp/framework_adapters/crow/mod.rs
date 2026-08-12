//! C++ Crow adapter 경계. CROW_ROUTE 규칙을 이 모듈에 구현한다.
use crate::facts::FactStore;
use crate::frameworks::adapters::c_family::cpp::framework_adapters::line_routes;
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    line_routes::refine_crow_routes(facts, detections);
}
