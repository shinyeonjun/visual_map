//! Dart Frog의 파일 기반 `routes/` 진입점을 추출한다.
use crate::facts::FactStore;
use crate::frameworks::common::file_routes::add_file_routes;
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    add_file_routes(facts, detections, "dart.dart_frog");
}
