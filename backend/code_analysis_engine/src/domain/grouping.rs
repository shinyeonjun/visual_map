//! Facts를 도메인 후보와 그룹으로 변환하는 오케스트레이션 계층.

use crate::config::{DomainPolicy, PathPolicy};
use crate::domain::assignments::{
    assign_units, index_assignments_by_domain, index_domains_by_unit, index_entrypoints_by_domain,
    index_resources_by_domain, score_by_unit,
};
use crate::domain::candidates::build;
use crate::domain::confidence::calculate;
use crate::domain::membership::MembershipKind;
use crate::domain::naming::label;
use crate::domain::signals::collect;
use crate::facts::{FactStore, ResolutionStatus};
use crate::graph::StaticRelationGraph;
use sha2::{Digest, Sha256};

use super::aggregation::aggregate_relations;

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

    pub fn analyze(&self, store: &FactStore) -> DomainAnalysisOutput {
        let graph = crate::graph::build(store);
        self.analyze_with_graph(store, &graph)
    }

    pub fn analyze_with_graph(
        &self,
        store: &FactStore,
        graph: &StaticRelationGraph,
    ) -> DomainAnalysisOutput {
        let signals = collect(store, &self.domain_policy, &self.path_policy);
        let signal_count = signals.len();
        let candidates = build(&signals, self.domain_policy.maximum_candidate_evidence);
        let signal_scores = score_by_unit(&signals);
        let domain_ids: std::collections::HashMap<&str, String> = candidates
            .iter()
            .map(|candidate| (candidate.key.as_str(), stable_domain_id(&candidate.key)))
            .collect();
        let assignments = assign_units(store, &candidates, &signal_scores, &self.domain_policy);

        // 도메인별 멤버십을 미리 역색인해 두면 도메인마다 전체 멤버십을 다시
        // 순회하지 않아도 된다.
        let assignments_by_domain = index_assignments_by_domain(&assignments);
        let domains_by_unit = index_domains_by_unit(&assignments);
        let entrypoints_by_domain = index_entrypoints_by_domain(store, &domains_by_unit);
        let resources_by_domain = index_resources_by_domain(store, &domains_by_unit);

        let mut groups = Vec::new();
        for candidate in &candidates {
            let Some(domain_id) = domain_ids.get(candidate.key.as_str()) else {
                continue;
            };
            let candidate_units = assignments_by_domain
                .get(domain_id.as_str())
                .cloned()
                .unwrap_or_default();
            if candidate_units.is_empty() {
                continue;
            }
            let ambiguous = candidate_units
                .iter()
                .any(|assignment| assignment.kind == MembershipKind::Shared);
            let (status, confidence) = calculate(
                candidate.score,
                &candidate.signal_families,
                ambiguous,
                &self.domain_policy,
            );
            let primary_unit_ids = candidate_units
                .iter()
                .filter(|assignment| {
                    assignment.kind == MembershipKind::Primary
                        && assignment.domain_id.as_deref() == Some(domain_id)
                })
                .map(|assignment| assignment.unit_id.clone())
                .collect();
            let shared_unit_ids = candidate_units
                .iter()
                .filter(|assignment| {
                    assignment.kind == MembershipKind::Shared
                        || assignment.domain_id.as_deref() != Some(domain_id)
                })
                .map(|assignment| assignment.unit_id.clone())
                .collect();
            groups.push(DomainGroup {
                id: domain_id.clone(),
                key: candidate.key.clone(),
                label: label(&candidate.key),
                kind: domain_kind(&candidate.key, &self.domain_policy),
                status,
                confidence,
                primary_unit_ids,
                shared_unit_ids,
                entrypoint_ids: entrypoints_by_domain
                    .get(domain_id.as_str())
                    .cloned()
                    .unwrap_or_default(),
                feature_ids: Vec::new(),
                resource_ids: resources_by_domain
                    .get(domain_id.as_str())
                    .cloned()
                    .unwrap_or_default(),
                evidence: candidate.evidence.clone(),
                summary: None,
            });
        }
        groups.sort_by(|left, right| left.key.cmp(&right.key));

        let relations = aggregate_relations(store, graph, &assignments, &groups);
        let dynamic_reference_ids = graph
            .edges
            .iter()
            .filter(|reference| reference.status == ResolutionStatus::Dynamic)
            .map(|reference| reference.id.clone())
            .collect();
        let unassigned_unit_ids = assignments
            .iter()
            .filter(|assignment| assignment.domain_id.is_none())
            .map(|assignment| assignment.unit_id.clone())
            .collect();

        DomainAnalysisOutput {
            static_graph: graph.clone(),
            groups,
            relations,
            memberships: assignments,
            unassigned_unit_ids,
            dynamic_reference_ids,
            signal_count,
        }
    }
}

/// Codex 병합 이후 최종 도메인 관계를 다시 집계한다.
pub fn reaggregate_relations(store: &FactStore, analysis: &mut DomainAnalysisOutput) {
    analysis.relations = aggregate_relations(
        store,
        &analysis.static_graph,
        &analysis.memberships,
        &analysis.groups,
    );
}

fn domain_kind(key: &str, policy: &DomainPolicy) -> DomainKind {
    if policy.cross_cutting_keys.contains(key) {
        DomainKind::CrossCutting
    } else {
        DomainKind::Business
    }
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
