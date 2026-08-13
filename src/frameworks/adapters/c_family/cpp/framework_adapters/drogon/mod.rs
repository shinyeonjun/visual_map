//! C++ Drogon adapter 경계. HTTP route macro·controller 규칙을 이 모듈에 구현한다.
use crate::facts::FactStore;
use crate::frameworks::adapters::c_family::cpp::framework_adapters::line_routes;
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    line_routes::refine_drogon_routes(facts, detections);
}
