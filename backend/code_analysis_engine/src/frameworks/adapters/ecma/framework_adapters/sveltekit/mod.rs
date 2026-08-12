//! SvelteKit `src/routes/**/+server` 파일 기반 route extractor.

use crate::facts::FactStore;
use crate::frameworks::common::file_routes::add_file_routes;
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    add_file_routes(facts, detections, "javascript.sveltekit");
}
