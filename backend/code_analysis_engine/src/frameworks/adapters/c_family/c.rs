//! C 프레임워크 adapter 확장 지점.
//!
//! C 고유 문법은 언어 공통 Facts로 보존하고, 프레임워크별 callback과
//! component 경계는 하위 adapter에서 공통 entrypoint/modifier로 보강한다.

mod framework_adapters;

use crate::facts::FactStore;
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    framework_adapters::enrich(facts, detections);
}
