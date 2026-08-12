//! C++ gRPC의 `ServerBuilder::RegisterService` 호출을 RPC 진입점으로 변환한다.
use crate::facts::FactStore;
use crate::frameworks::common::rpc::{add_rpc_registrations, RpcRegistrationRule};
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    add_rpc_registrations(
        facts,
        detections,
        RpcRegistrationRule {
            framework_id: "cpp.grpc",
            call_names: &["RegisterService"],
            call_prefix: None,
            call_suffix: None,
            service_argument_index: Some(0),
            service_name_from_callee: false,
        },
    );
}
