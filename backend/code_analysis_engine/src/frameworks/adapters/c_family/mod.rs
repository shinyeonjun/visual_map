//! C 계열(C/C++) 프레임워크 adapter 경계다.
//!
//! C와 C++는 include·매크로·클래스 계층을 공유하지만, Qt·MFC·Boost.Asio
//! 같은 프레임워크 문법은 서로 다르므로 하위 adapter를 분리한다.

mod c;
mod cpp;

use crate::facts::FactStore;
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    c::enrich(facts, detections);
    cpp::enrich(facts, detections);
}
