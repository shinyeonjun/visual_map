//! C#·.NET framework adapter 경계다.
//!
//! ASP.NET Controller/Minimal API, Blazor, .NET MAUI는 모두 C# 문법을 쓰지만
//! 서로 다른 진입점·컴포넌트 의미를 가지므로 이후 이 모듈 아래에서 분리한다.

mod framework_adapters;

use crate::facts::FactStore;
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    framework_adapters::enrich(facts, detections);
}
