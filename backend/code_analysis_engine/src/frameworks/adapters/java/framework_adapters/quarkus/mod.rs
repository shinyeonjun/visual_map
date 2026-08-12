//! Quarkus controller-level `@Path` prefix adapter.
use crate::facts::FactStore;
use crate::frameworks::common::annotations::compose_class_route_prefixes;
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, _detections: &[FrameworkDetection]) {
    compose_class_route_prefixes(facts, &["java.quarkus"]);
}
