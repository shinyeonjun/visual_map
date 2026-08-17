//! 1단계 도메인 분석 구현 영역.
//!
//! 이 영역은 코드 사실을 business domain·cross-cutting·unknown 그룹으로
//! 묶고, 각 판단을 증거와 함께 반환한다.

pub mod confidence;
pub mod grouping;
pub mod membership;
pub mod naming;
pub mod signals;

mod aggregation;
pub(crate) mod capability_keys;
mod capabilities;
mod clustering;
pub(crate) mod contract_path;
mod feature_graph;
mod formation;
mod merge_gate;
mod models;
mod tfidf;

pub use formation::diagnostics::DomainFormationDiagnostics;
pub use grouping::{
    reaggregate_relations, DomainAnalysisOutput, DomainAnalyzer, DomainGroup, DomainKind,
    DomainRelation, FeatureFirstResult,
};
