//! 1단계 도메인 분석 구현 영역.
//!
//! 이 영역은 코드 사실을 business domain·cross-cutting·unknown 그룹으로
//! 묶고, 각 판단을 증거와 함께 반환한다.

pub mod candidates;
pub mod confidence;
pub mod grouping;
pub mod membership;
pub mod naming;
pub mod signals;

mod aggregation;
mod assignments;
mod models;
mod reference_signals;

pub use grouping::{
    reaggregate_relations, DomainAnalysisOutput, DomainAnalyzer, DomainGroup, DomainKind,
    DomainRelation,
};
