//! C++ framework별 adapter 조정 계층.

mod boost_asio;
mod crow;
mod drogon;
mod grpc;
mod line_routes;
mod mfc;
mod poco;
mod qt;
mod unreal_engine;

use crate::facts::FactStore;
use crate::frameworks::registry::detector::FrameworkDetection;
use std::time::Instant;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    run_profiled("qt", facts, detections, qt::enrich);
    run_profiled("mfc", facts, detections, mfc::enrich);
    run_profiled("boost_asio", facts, detections, boost_asio::enrich);
    run_profiled("poco", facts, detections, poco::enrich);
    run_profiled("unreal_engine", facts, detections, unreal_engine::enrich);
    run_profiled("drogon", facts, detections, drogon::enrich);
    run_profiled("crow", facts, detections, crow::enrich);
    run_profiled("grpc", facts, detections, grpc::enrich);
}

fn run_profiled(
    name: &str,
    facts: &mut FactStore,
    detections: &[FrameworkDetection],
    enrich: fn(&mut FactStore, &[FrameworkDetection]),
) {
    let should_profile = std::env::var_os("VISUAL_MAP_FRAMEWORK_PROFILE_DEEP").is_some();
    let started = should_profile.then(Instant::now);
    enrich(facts, detections);
    if let Some(started) = started {
        eprintln!(
            "[framework-profile-deep] family=cpp adapter={name} elapsed_ms={} entrypoints={}",
            started.elapsed().as_millis(),
            facts.entrypoints.len()
        );
    }
}
