//! C# framework별 adapter 조정 계층.

mod aspnet_core;
mod aspnet_mvc;
mod aspnet_web_api;
mod blazor;
mod conventional_mvc;
mod dotnet_maui;
mod minimal_api;

use crate::facts::FactStore;
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    aspnet_core::enrich(facts, detections);
    aspnet_mvc::enrich(facts, detections);
    aspnet_web_api::enrich(facts, detections);
    minimal_api::enrich(facts, detections);
    conventional_mvc::enrich(facts);
    blazor::enrich(facts, detections);
    dotnet_maui::enrich(facts, detections);
}
