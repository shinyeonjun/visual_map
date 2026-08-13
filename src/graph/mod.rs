//! 분석 사실을 관계 그래프로 관리하는 영역.
//!
//! 1단계에서는 얕은 정적 의존 관계와 도메인 간 집계만 사용하고, 2·3단계에서
//! 모듈 그래프와 실행 흐름 그래프로 확장한다.

pub mod aggregation;
pub mod dependency;

pub use dependency::{build, StaticRelationGraph};
