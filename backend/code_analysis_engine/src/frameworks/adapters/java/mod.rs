//! Java framework adapter 경계다.
//!
//! Spring 계열, Jakarta EE, Quarkus, Micronaut, Play는 annotation·route·DI
//! 규칙이 서로 다르므로 catalog 감지와 실제 의미 보강을 분리한다.

mod framework_adapters;

use crate::facts::FactStore;
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    framework_adapters::enrich(facts, detections);
}
