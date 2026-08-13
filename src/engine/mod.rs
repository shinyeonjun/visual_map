//! 분석 실행 순서를 관리하는 경계다.
//!
//! 실제 분석 단계의 연결은 `crate::pipeline`이 소유한다. 이 모듈은 외부에서
//! 엔진을 실행할 때 사용하는 경계다.

pub use crate::analysis::{analyze, AnalysisEngine};
pub use crate::pipeline::DomainAnalysisPipeline;
