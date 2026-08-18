//! capability 쌍별 merge 탈락 사유와 유사도 breakdown.

use crate::config::{DomainClusteringMode, DomainPolicy, PathPolicy};
use crate::domain::capabilities::Capability;
use crate::domain::feature_graph::FeatureSimilarity;
use crate::domain::formation::capability_evidence::build_capability_evidence;
use crate::domain::formation::constraints::pair_forbidden_reason;
use crate::domain::formation::diagnostics::clustering_mode_label;
use crate::domain::merge_gate::{self, MergePairContext};
use crate::facts::FactStore;
use crate::flow::ExecutionFlowGraph;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::pair_context::build_capability_pair_context;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PairRejectionReason {
    SameKey,
    ForbiddenKey,
    ForbiddenTestProd,
    NoStructuralGate,
    BelowMergeThreshold,
    Eligible,
}

impl PairRejectionReason {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::SameKey => "sameKey",
            Self::ForbiddenKey => "forbiddenKey",
            Self::ForbiddenTestProd => "forbiddenTestProd",
            Self::NoStructuralGate => "noStructuralGate",
            Self::BelowMergeThreshold => "belowMergeThreshold",
            Self::Eligible => "eligible",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairSimilarityBreakdown {
    pub combined: f64,
    pub http_match: f64,
    pub call: f64,
    pub flow: f64,
    pub resource: f64,
    pub path: f64,
    pub lexical: f64,
}

impl From<&FeatureSimilarity> for PairSimilarityBreakdown {
    fn from(sim: &FeatureSimilarity) -> Self {
        Self {
            combined: sim.combined,
            http_match: sim.http_match,
            call: sim.call,
            flow: sim.flow,
            resource: sim.resource,
            path: sim.path,
            lexical: sim.lexical,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityPairCandidate {
    pub left_key: String,
    pub right_key: String,
    pub similarity: PairSimilarityBreakdown,
    pub rejection: String,
    pub merge_gate: Option<String>,
    pub left_evidence: Option<super::capability_evidence::CapabilityEvidence>,
    pub right_evidence: Option<super::capability_evidence::CapabilityEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityPairDiagnostics {
    pub clustering_mode: String,
    pub capabilities: usize,
    pub distinct_keys: usize,
    pub total_pairs: usize,
    pub cross_key_pairs: usize,
    pub merge_threshold: f64,
    pub rejected: BTreeMap<String, usize>,
    pub top_cross_key_candidates: Vec<CapabilityPairCandidate>,
}

pub fn analyze_capability_pairs(
    store: &FactStore,
    execution_flows: &ExecutionFlowGraph,
    domain_policy: &DomainPolicy,
    path_policy: &PathPolicy,
    mode: DomainClusteringMode,
    top_k: usize,
) -> CapabilityPairDiagnostics {
    let context = build_capability_pair_context(store, execution_flows, domain_policy, path_policy);
    analyze_capability_pairs_with_context(
        store,
        path_policy,
        &context,
        mode,
        top_k,
        true,
    )
}

pub(super) fn analyze_capability_pairs_with_context(
    store: &FactStore,
    path_policy: &PathPolicy,
    context: &super::pair_context::CapabilityPairContext,
    mode: DomainClusteringMode,
    top_k: usize,
    include_evidence: bool,
) -> CapabilityPairDiagnostics {
    let capabilities = &context.capabilities;
    let mut report = CapabilityPairDiagnostics {
        clustering_mode: clustering_mode_label(mode).into(),
        capabilities: capabilities.len(),
        distinct_keys: capabilities
            .iter()
            .map(|capability| capability.key.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        merge_threshold: context.merge_threshold,
        ..Default::default()
    };

    if capabilities.len() < 2 {
        return report;
    }

    report.total_pairs = pair_count(capabilities.len());
    let mut cross_key_candidates = Vec::new();

    for i in 0..capabilities.len() {
        for j in (i + 1)..capabilities.len() {
            let left = &capabilities[i];
            let right = &capabilities[j];
            let sim = context.matrix.get(i, j);
            let (reason, merge_gate_reason) =
                classify_pair(left, right, sim, store, path_policy, mode, context.merge_threshold);

            *report
                .rejected
                .entry(reason.label().to_string())
                .or_default() += 1;

            if left.key == right.key {
                continue;
            }
            report.cross_key_pairs += 1;
            cross_key_candidates.push(CapabilityPairCandidate {
                left_key: left.key.clone(),
                right_key: right.key.clone(),
                similarity: PairSimilarityBreakdown::from(sim),
                rejection: reason.label().to_string(),
                merge_gate: merge_gate_reason.map(str::to_string),
                left_evidence: None,
                right_evidence: None,
            });
        }
    }

    cross_key_candidates.sort_by(|left, right| {
        right
            .similarity
            .combined
            .partial_cmp(&left.similarity.combined)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.left_key.cmp(&right.left_key))
            .then_with(|| left.right_key.cmp(&right.right_key))
    });
    cross_key_candidates.truncate(top_k);

    if include_evidence {
        for candidate in &mut cross_key_candidates {
            let left_index = context
                .key_index
                .get(&candidate.left_key)
                .copied()
                .expect("candidate key must exist");
            let right_index = context
                .key_index
                .get(&candidate.right_key)
                .copied()
                .expect("candidate key must exist");
            candidate.left_evidence = Some(build_capability_evidence(
                &capabilities[left_index],
                &context.capability_data,
                left_index,
                &context.terms[left_index],
                store,
            ));
            candidate.right_evidence = Some(build_capability_evidence(
                &capabilities[right_index],
                &context.capability_data,
                right_index,
                &context.terms[right_index],
                store,
            ));
        }
    }

    report.top_cross_key_candidates = cross_key_candidates;
    report
}

pub(crate) fn classify_pair_with_context(
    left: &Capability,
    right: &Capability,
    sim: &FeatureSimilarity,
    store: &FactStore,
    path_policy: &PathPolicy,
    mode: DomainClusteringMode,
    merge_threshold: f64,
) -> (PairRejectionReason, Option<&'static str>) {
    classify_pair(left, right, sim, store, path_policy, mode, merge_threshold)
}

fn classify_pair(
    left: &Capability,
    right: &Capability,
    sim: &FeatureSimilarity,
    store: &FactStore,
    path_policy: &PathPolicy,
    mode: DomainClusteringMode,
    merge_threshold: f64,
) -> (PairRejectionReason, Option<&'static str>) {
    if left.key == right.key {
        return (PairRejectionReason::SameKey, None);
    }
    if let Some(reason) = pair_forbidden_reason(left, right, store, path_policy, mode) {
        return (
            match reason {
                "forbiddenTestProd" => PairRejectionReason::ForbiddenTestProd,
                _ => PairRejectionReason::ForbiddenKey,
            },
            None,
        );
    }
    let merge_gate_reason = merge_gate::merge_allowed(
        mode,
        sim,
        merge_pair_context(left, right, store, mode),
    );
    let Some(gate) = merge_gate_reason else {
        return (PairRejectionReason::NoStructuralGate, None);
    };
    if sim.combined < merge_threshold {
        return (PairRejectionReason::BelowMergeThreshold, Some(gate));
    }
    (PairRejectionReason::Eligible, Some(gate))
}

fn merge_pair_context<'a>(
    left: &'a Capability,
    right: &'a Capability,
    store: &'a FactStore,
    mode: DomainClusteringMode,
) -> Option<MergePairContext<'a>> {
    if mode == DomainClusteringMode::StructuralCrossKeyV2 {
        Some(MergePairContext { left, right, store })
    } else {
        None
    }
}

fn pair_count(n: usize) -> usize {
    n.saturating_mul(n.saturating_sub(1)) / 2
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PathPolicy;
    use std::collections::BTreeSet;

    fn capability(key: &str) -> Capability {
        Capability {
            key: key.to_string(),
            entrypoint_ids: Vec::new(),
            resource_ids: Vec::new(),
            unit_ids: Vec::new(),
            contract_paths: BTreeSet::new(),
        }
    }

    #[test]
    fn legacy_cross_key는_forbidden_key로_분류된다() {
        let (reason, _) = classify_pair(
            &capability("auth"),
            &capability("users"),
            &FeatureSimilarity {
                http_match: 0.5,
                call: 0.2,
                flow: 0.0,
                resource: 0.3,
                path: 0.0,
                lexical: 0.1,
                combined: 0.4,
            },
            &FactStore::default(),
            &PathPolicy::default(),
            DomainClusteringMode::LegacyStrictKey,
            0.08,
        );
        assert_eq!(reason, PairRejectionReason::ForbiddenKey);
    }

    #[test]
    fn structural_call_단독은_no_structural_gate다() {
        let (reason, gate) = classify_pair(
            &capability("auth"),
            &capability("users"),
            &FeatureSimilarity {
                http_match: 0.0,
                call: 0.5,
                flow: 0.0,
                resource: 0.0,
                path: 0.0,
                lexical: 0.0,
                combined: 0.2,
            },
            &FactStore::default(),
            &PathPolicy::default(),
            DomainClusteringMode::StructuralCrossKey,
            0.08,
        );
        assert_eq!(reason, PairRejectionReason::NoStructuralGate);
        assert!(gate.is_none());
    }
}
