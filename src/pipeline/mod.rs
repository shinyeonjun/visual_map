//! 분석 단계를 연결하는 파이프라인 경계다.
//!
//! 실제 실행은 `runner`가 담당한다. Codex 의미 리뷰는 정적 결과를 만든 뒤
//! `semantic review` 명령으로 별도 실행한다.

pub mod profile;
pub mod progress;
pub mod stages;

mod cache;
mod fact_bundle;
mod gold_pair_diagnose;
mod pair_diagnose;
mod runner;

use std::sync::Arc;

/// 도메인 분석 파이프라인의 실행 객체.
#[derive(Debug, Default)]
pub struct DomainAnalysisPipeline {
    pub(crate) fact_cache: Arc<cache::FactCache>,
}
