//! Rust framework adapter 경계다.
//!
//! Axum, Actix Web, Rocket, Warp, Poem, Tokio, Tonic, Tauri는 macro·builder·
//! trait 기반 규칙이 달라 독립 adapter로 확장한다.

mod framework_adapters;

use crate::facts::FactStore;
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    framework_adapters::enrich(facts, detections);
}
