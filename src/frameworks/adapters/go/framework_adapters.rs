//! Go framework별 adapter 조정 계층.

mod beego;
mod chi;
mod echo;
mod fiber;
mod gin;
mod grpc;
mod net_http;

use crate::facts::FactStore;
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    net_http::enrich(facts, detections);
    gin::enrich(facts, detections);
    echo::enrich(facts, detections);
    fiber::enrich(facts, detections);
    chi::enrich(facts, detections);
    beego::enrich(facts, detections);
    grpc::enrich(facts, detections);
}
