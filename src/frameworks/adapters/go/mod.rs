//! Go framework adapter 경계다.
//!
//! net/http, Gin, Echo, Fiber, Chi, Beego, gRPC의 route·middleware·RPC 규칙은
//! Go 공통 분석기와 분리해서 이 모듈 아래에 추가한다.

mod framework_adapters;

use crate::facts::FactStore;
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    framework_adapters::enrich(facts, detections);
}
