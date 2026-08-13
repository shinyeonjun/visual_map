//! C++ 프레임워크 adapter 확장 지점.
//!
//! Qt component, MFC view, Boost.Asio callback, Unreal reflection 등은
//! C 공통 adapter와 섞지 않고 하위 adapter에서 해석한다.

mod framework_adapters;

use crate::facts::FactStore;
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    framework_adapters::enrich(facts, detections);
}
