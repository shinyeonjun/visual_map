//! Serverpod `Endpoint` 메서드를 RPC 진입점으로 변환한다.
use crate::facts::FactStore;
use crate::frameworks::common::rpc::add_endpoint_methods;
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    add_endpoint_methods(facts, detections, "dart.serverpod", "Endpoint");
}
