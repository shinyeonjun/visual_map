//! 분석 단계를 연결하는 파이프라인 경계다.
//!
//! 실제 실행은 `runner`, Codex 의미 보정은 `semantic_stage`가 담당한다.

pub mod profile;
pub mod progress;
pub mod stages;

mod cache;
mod dev_artifacts;
mod runner;
mod semantic_stage;

/// 도메인 분석 파이프라인의 실행 객체.
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
pub struct DomainAnalysisPipeline {
    pub(crate) fact_cache: Arc<Mutex<cache::FactCache>>,
}
