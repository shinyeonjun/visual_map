//! JavaScript·TypeScript 프레임워크 adapter 경계다.

mod framework_adapters;

use crate::facts::FactStore;
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    // framework ID가 javascript.*로 통합되어 있으므로 JavaScript와 TypeScript
    // adapter를 중복 실행하지 않고 계열 공통 registry를 한 번만 실행한다.
    framework_adapters::enrich(facts, detections);
}
