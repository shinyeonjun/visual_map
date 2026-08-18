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
mod merge_evidence_v2;
mod models;
mod tfidf;

pub use formation::{
    aggregate_project_domain_seeds, analyze_domain_seeds, AggregatedDomainSeedCandidate,
    AnchorScoreComponent, AnchorScoreComponents, CapabilityDomainSeeds, CapabilitySeedCoverage,
    ConceptRoleDiagnostic, DomainSeedCandidate, DomainSeedConfidence, DomainSeedDiagnostics,
    DomainSeedEvidenceSource, DomainSeedRawEvidence, EvidenceGroupDiagnostic, IdfPenaltyDiagnostic,
    IdfPenaltyPolicyDiagnostic, ProjectDomainSeedAggregation, RankedConceptFamily,
    SeedCandidateGraph, SeedGraphEdge, SeedGraphEdgeEvidence, SeedGraphNode,
    SparseSeedCandidateGraph, SparseSeedGraphEdge, SparseSeedGraphNode,
};
pub use formation::capability_evidence::CapabilityEvidence;
pub use formation::diagnostics::DomainFormationDiagnostics;
pub use formation::key_decomposition::keys_share_entity;
pub(crate) use formation::gold_pair_signals::{
    analyze_gold_pair_signals, GoldPairLabelInput, GoldPairSignalModeReport,
    GoldLabeledPairSignals,
};
pub(crate) use formation::pair_context::build_capability_pair_context;
pub use formation::pair_diagnostics::{
    analyze_capability_pairs, CapabilityPairCandidate, CapabilityPairDiagnostics,
    PairRejectionReason, PairSimilarityBreakdown,
};
pub use grouping::{
    reaggregate_relations, DomainAnalysisOutput, DomainAnalyzer, DomainGroup, DomainKind,
    DomainRelation, FeatureFirstResult,
};
