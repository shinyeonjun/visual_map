//! C framework별 adapter 조정 계층.

mod gtk_glib;
mod libevent;
mod libuv;
mod qt;

use crate::facts::FactStore;
use crate::frameworks::registry::detector::FrameworkDetection;
use std::time::Instant;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    run_profiled("gtk_glib", facts, detections, gtk_glib::enrich);
    run_profiled("qt", facts, detections, qt::enrich);
    run_profiled("libuv", facts, detections, libuv::enrich);
    run_profiled("libevent", facts, detections, libevent::enrich);
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
            "[framework-profile-deep] family=c adapter={name} elapsed_ms={} entrypoints={}",
            started.elapsed().as_millis(),
            facts.entrypoints.len()
        );
    }
}
