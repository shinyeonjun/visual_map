//! Dart framework별 adapter 조정 계층.

mod dart_frog;
mod flutter;
mod serverpod;
mod shelf;

use crate::facts::FactStore;
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    flutter::enrich(facts, detections);
    shelf::enrich(facts, detections);
    serverpod::enrich(facts, detections);
    dart_frog::enrich(facts, detections);
}
