//! Java framework별 adapter 조정 계층.

mod jakarta_ee;
mod micronaut;
mod play;
mod quarkus;
mod spring;
mod spring_boot;
mod spring_mvc;
mod spring_webflux;

use crate::facts::FactStore;
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    spring::enrich(facts, detections);
    spring_boot::enrich(facts, detections);
    spring_mvc::enrich(facts, detections);
    spring_webflux::enrich(facts, detections);
    jakarta_ee::enrich(facts, detections);
    quarkus::enrich(facts, detections);
    micronaut::enrich(facts, detections);
    play::enrich(facts, detections);
}
