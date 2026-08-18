//! Facts를 feature-first 도메인 분석으로 변환하는 오케스트레이션 계층.

use crate::config::{DomainPolicy, PathPolicy};
use crate::facts::FactStore;
use crate::flow::ExecutionFlowGraph;
use crate::graph::StaticRelationGraph;
use sha2::{Digest, Sha256};

use super::aggregation::aggregate_relations;

pub use super::formation::FeatureFirstResult;
pub use super::models::{DomainAnalysisOutput, DomainGroup, DomainKind, DomainRelation};

/// 공통 Facts에서 도메인을 생성한다.
#[derive(Debug, Clone)]
pub struct DomainAnalyzer {
    pub domain_policy: DomainPolicy,
    pub path_policy: PathPolicy,
}

impl Default for DomainAnalyzer {
    fn default() -> Self {
        let config = crate::config::AnalysisConfig::default();
        Self {
            domain_policy: config.domains,
            path_policy: config.paths,
        }
    }
}

impl DomainAnalyzer {
    pub fn new(domain_policy: DomainPolicy, path_policy: PathPolicy) -> Self {
        Self {
            domain_policy,
            path_policy,
        }
    }

    /// Feature를 먼저 만들고 Multi-view 유사도로 클러스터링해 도메인을 형성한다.
    pub fn analyze_feature_first(
        &self,
        store: &FactStore,
        graph: &StaticRelationGraph,
        execution_flows: &ExecutionFlowGraph,
    ) -> FeatureFirstResult {
        super::formation::analyze_feature_first(
            store,
            graph,
            execution_flows,
            &self.domain_policy,
            &self.path_policy,
        )
    }

    /// capability 쌍이 clustering merge 후보에서 탈락한 이유를 분석한다.
    pub fn diagnose_capability_pairs(
        &self,
        store: &FactStore,
        execution_flows: &ExecutionFlowGraph,
        mode: crate::config::DomainClusteringMode,
        top_k: usize,
    ) -> super::CapabilityPairDiagnostics {
        super::formation::pair_diagnostics::analyze_capability_pairs(
            store,
            execution_flows,
            &self.domain_policy,
            &self.path_policy,
            mode,
            top_k,
        )
    }
}

/// 의미 리뷰 병합 이후 최종 도메인 관계를 다시 집계한다.
pub fn reaggregate_relations(store: &FactStore, analysis: &mut DomainAnalysisOutput) {
    analysis.relations = aggregate_relations(
        store,
        &analysis.static_graph,
        &analysis.memberships,
        &analysis.groups,
    );
}

pub(super) fn stable_domain_id(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    let digest = hasher.finalize();
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("domain_{}", &hex[..24])
}
