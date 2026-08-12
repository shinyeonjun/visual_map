//! 1단계 전체 도메인 Overview 응답.

mod features;
pub mod model;
pub mod projector;
mod reachability;

pub use model::{
    AnalysisCoverage, FeatureConfidence, FeatureGroup, FeatureKind, FeatureStatus,
    LanguageCoverage, OverviewResponse,
};
