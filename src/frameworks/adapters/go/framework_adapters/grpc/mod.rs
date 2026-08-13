//! Go gRPC의 `Register*Server` 호출을 RPC 진입점으로 변환한다.
use crate::facts::FactStore;
use crate::frameworks::common::rpc::{add_rpc_registrations, RpcRegistrationRule};
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    add_rpc_registrations(
        facts,
        detections,
        RpcRegistrationRule {
            framework_id: "go.grpc",
            call_names: &[],
            call_prefix: Some("Register"),
            call_suffix: Some("Server"),
            service_argument_index: Some(1),
            service_name_from_callee: true,
        },
    );
}
