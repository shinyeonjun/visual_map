//! Rust framework별 adapter 조정 계층.

mod actix_web;
mod axum;
mod poem;
mod rocket;
mod tauri;
mod tokio;
mod tonic;
mod warp;

use crate::facts::FactStore;
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    axum::enrich(facts, detections);
    actix_web::enrich(facts, detections);
    rocket::enrich(facts, detections);
    warp::enrich(facts, detections);
    poem::enrich(facts, detections);
    tokio::enrich(facts, detections);
    tonic::enrich(facts, detections);
    tauri::enrich(facts, detections);
}
