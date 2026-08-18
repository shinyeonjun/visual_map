//! Candidate retrieval channel ablation diagnostics.

use super::domain_seed_anchor_affinity::{AnchorCapabilityEdge, CapabilityAnchorAssignment};
use super::domain_seed_anchor_eligibility::HypothesisContext;
use super::domain_seed_diagnostics::CapabilityDomainSeeds;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const RETRIEVAL_CHANNELS: &[&str] = &[
    "lexical",
    "owner",
    "modulePackage",
    "entityResource",
    "behaviorCall",
    "behaviorFlow",
    "contract",
];

const STRONG_CHANNELS: &[&str] = &[
    "owner",
    "modulePackage",
    "entityResource",
    "behaviorCall",
    "behaviorFlow",
];
const INDEPENDENT_STRONG_CHANNELS: &[&str] = &[
    "modulePackage",
    "entityResource",
    "behaviorCall",
    "behaviorFlow",
];
const WEAK_CHANNELS: &[&str] = &["lexical", "contract"];
const GENERICNESS_WEAK_THRESHOLD: f64 = 0.45;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CandidateRetrievalAblationDiagnostics {
    pub baseline_candidate_edge_count: usize,
    pub suppressed_owner_only_edges: usize,
    pub channel_summaries: Vec<RetrievalChannelSummary>,
    pub channel_only_ablations: Vec<RetrievalAblationScenario>,
    pub leave_one_out_ablations: Vec<RetrievalAblationScenario>,
    pub weak_evidence_contributions: WeakEvidenceContributions,
    pub assignment_summary: RetrievalAssignmentSummary,
    pub top_hypothesis_fanout: Vec<HypothesisRetrievalFanoutRecord>,
    pub top_capability_candidate_load: Vec<CapabilityCandidateLoadRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalChannelSummary {
    pub channel: String,
    pub generated_candidate_edges: usize,
    pub unique_candidate_edges: usize,
    pub exclusive_edges: usize,
    pub multi_channel_edges: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalAblationScenario {
    pub scenario_kind: String,
    pub channel: String,
    pub candidate_edge_count: usize,
    pub capability_candidate_median: f64,
    pub capability_candidate_p95: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WeakEvidenceContributions {
    pub weak_lexical_edges: usize,
    pub generic_module_package_edges: usize,
    pub generic_owner_role_edges: usize,
    pub behavior_only_edges: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalAssignmentSummary {
    pub confident_assignments: usize,
    pub ambiguous_assignments: usize,
    pub weak_assignments: usize,
    pub unassigned_assignments: usize,
    pub no_candidate_assignments: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OwnerSuppressionVerification {
    pub suppressed_owner_only_edges: usize,
    pub owner_exclusive_pairs_before_gate: usize,
    pub suppressed_owner_only_pure: usize,
    pub suppressed_owner_with_weak_independent: usize,
    pub retained_owner_reinforced_edges: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HypothesisRetrievalFanoutRecord {
    pub hypothesis_id: String,
    pub representative_root_concept: String,
    pub representative_family_id: String,
    pub retrieval_fanout_capabilities: usize,
    pub retrieval_fanout_ratio: f64,
    pub dominant_retrieval_channels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityCandidateLoadRecord {
    pub capability_key: String,
    pub retrieved_candidate_count: usize,
    pub dominant_retrieval_channels: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RawRetrievalPair {
    pub hypothesis_id: String,
    pub capability_key: String,
    pub channels: BTreeSet<String>,
    pub weak_lexical: bool,
    pub weak_generic_module_package: bool,
    pub weak_generic_owner_role: bool,
    pub behavior_only: bool,
}

pub fn build_retrieval_ablation_diagnostics(
    raw_pairs: &[RawRetrievalPair],
    edges: &[AnchorCapabilityEdge],
    assignments: &[CapabilityAnchorAssignment],
    capability_seeds: &[CapabilityDomainSeeds],
    eligible_contexts: &[HypothesisContext],
) -> CandidateRetrievalAblationDiagnostics {
    let baseline_pairs: Vec<_> = raw_pairs
        .iter()
        .filter(|pair| retrieval_qualifies_channels(&pair.channels))
        .collect();

    let suppressed_owner_only_edges = raw_pairs
        .iter()
        .filter(|pair| is_suppressed_owner_only_retrieval(&pair.channels))
        .count();

    let channel_summaries = RETRIEVAL_CHANNELS
        .iter()
        .map(|channel| summarize_channel(channel, raw_pairs, edges))
        .collect();

    let channel_only_ablations = RETRIEVAL_CHANNELS
        .iter()
        .map(|channel| ablation_scenario("channelOnly", channel, raw_pairs, capability_seeds))
        .collect();

    let leave_one_out_ablations = RETRIEVAL_CHANNELS
        .iter()
        .map(|channel| {
            let filtered: Vec<_> = raw_pairs
                .iter()
                .filter(|pair| {
                    let remaining = pair
                        .channels
                        .iter()
                        .filter(|value| *value != channel)
                        .cloned()
                        .collect::<BTreeSet<_>>();
                    retrieval_qualifies_channels(&remaining)
                })
                .collect();
            ablation_from_pairs("leaveOneOut", channel, &filtered, capability_seeds)
        })
        .collect();

    let weak_evidence_contributions = summarize_weak_contributions(&baseline_pairs);
    let assignment_summary = summarize_assignments(assignments);
    let top_hypothesis_fanout = top_hypothesis_fanout(edges, eligible_contexts, capability_seeds.len());
    let top_capability_candidate_load = top_capability_candidate_load(assignments);

    CandidateRetrievalAblationDiagnostics {
        baseline_candidate_edge_count: baseline_pairs.len(),
        suppressed_owner_only_edges,
        channel_summaries,
        channel_only_ablations,
        leave_one_out_ablations,
        weak_evidence_contributions,
        assignment_summary,
        top_hypothesis_fanout,
        top_capability_candidate_load,
    }
}

pub fn independent_retrieval_channels(channels: &BTreeSet<String>) -> BTreeSet<String> {
    channels
        .iter()
        .filter(|channel| channel.as_str() != "owner")
        .cloned()
        .collect()
}

pub fn retrieval_qualifies_channels(channels: &BTreeSet<String>) -> bool {
    retrieval_qualifies_independent_channels(&independent_retrieval_channels(channels))
}

fn retrieval_qualifies_independent_channels(independent: &BTreeSet<String>) -> bool {
    if independent.is_empty() {
        return false;
    }
    if independent
        .iter()
        .any(|channel| INDEPENDENT_STRONG_CHANNELS.contains(&channel.as_str()))
    {
        return true;
    }
    let weak_only = independent
        .iter()
        .all(|channel| WEAK_CHANNELS.contains(&channel.as_str()));
    !weak_only && independent.len() >= 2
}

pub fn retrieval_qualifies_channels_legacy(channels: &BTreeSet<String>) -> bool {
    if channels.is_empty() {
        return false;
    }
    if channels
        .iter()
        .any(|channel| STRONG_CHANNELS.contains(&channel.as_str()))
    {
        return true;
    }
    let weak_only = channels
        .iter()
        .all(|channel| WEAK_CHANNELS.contains(&channel.as_str()));
    !weak_only && channels.len() >= 2
}

pub fn is_suppressed_owner_only_retrieval(channels: &BTreeSet<String>) -> bool {
    channels.contains("owner")
        && retrieval_qualifies_channels_legacy(channels)
        && !retrieval_qualifies_channels(channels)
}

pub fn edge_channel_metrics(channels: &BTreeSet<String>) -> EdgeChannelMetrics {
    let strong_structural_reason_count = channels
        .iter()
        .filter(|channel| {
            matches!(
                channel.as_str(),
                "owner" | "modulePackage" | "entityResource" | "behaviorCall" | "behaviorFlow"
            )
        })
        .count();
    let weak_reason_count = channels
        .iter()
        .filter(|channel| WEAK_CHANNELS.contains(&channel.as_str()))
        .count();
    EdgeChannelMetrics {
        strong_structural_reason_count,
        weak_reason_count,
        has_ownership_reason: channels.contains("owner"),
        has_entity_resource_reason: channels.contains("entityResource"),
        has_behavior_reason: channels.contains("behaviorCall") || channels.contains("behaviorFlow"),
        has_lexical_only_reason: channels == &BTreeSet::from(["lexical".to_string()]),
    }
}

#[derive(Debug, Clone, Default)]
pub struct EdgeChannelMetrics {
    pub strong_structural_reason_count: usize,
    pub weak_reason_count: usize,
    pub has_ownership_reason: bool,
    pub has_entity_resource_reason: bool,
    pub has_behavior_reason: bool,
    pub has_lexical_only_reason: bool,
}

pub fn classify_weak_evidence(
    channels: &BTreeSet<String>,
    genericness: f64,
    transportness: f64,
) -> (bool, bool, bool, bool) {
    let weak_lexical = channels.contains("lexical") && genericness >= GENERICNESS_WEAK_THRESHOLD;
    let weak_generic_module_package = channels.contains("modulePackage")
        && (genericness >= GENERICNESS_WEAK_THRESHOLD || transportness >= GENERICNESS_WEAK_THRESHOLD);
    let weak_generic_owner_role =
        channels.contains("owner") && genericness >= GENERICNESS_WEAK_THRESHOLD;
    let behavior_only = !channels.is_empty()
        && channels
            .iter()
            .all(|channel| channel == "behaviorCall" || channel == "behaviorFlow");
    (
        weak_lexical,
        weak_generic_module_package,
        weak_generic_owner_role,
        behavior_only,
    )
}

fn summarize_channel(
    channel: &str,
    raw_pairs: &[RawRetrievalPair],
    edges: &[AnchorCapabilityEdge],
) -> RetrievalChannelSummary {
    let qualified_pairs = raw_pairs
        .iter()
        .filter(|pair| {
            pair.channels.contains(channel)
                && retrieval_qualifies_channels(&pair.channels)
        })
        .collect::<Vec<_>>();
    let unique_candidate_edges = qualified_pairs
        .iter()
        .map(|pair| (pair.hypothesis_id.as_str(), pair.capability_key.as_str()))
        .collect::<BTreeSet<_>>()
        .len();
    let generated_candidate_edges = edges
        .iter()
        .filter(|edge| edge.retrieval_channels.iter().any(|value| value == channel))
        .count();
    let exclusive_edges = qualified_pairs
        .iter()
        .filter(|pair| pair.channels.len() == 1 && pair.channels.contains(channel))
        .count();
    let multi_channel_edges = qualified_pairs
        .iter()
        .filter(|pair| pair.channels.len() > 1 && pair.channels.contains(channel))
        .count();
    RetrievalChannelSummary {
        channel: channel.to_string(),
        generated_candidate_edges,
        unique_candidate_edges,
        exclusive_edges,
        multi_channel_edges,
    }
}

fn ablation_scenario(
    kind: &str,
    channel: &str,
    raw_pairs: &[RawRetrievalPair],
    capability_seeds: &[CapabilityDomainSeeds],
) -> RetrievalAblationScenario {
    let only_channel = BTreeSet::from([channel.to_string()]);
    let filtered: Vec<_> = raw_pairs
        .iter()
        .filter(|pair| {
            pair.channels.contains(channel) && retrieval_qualifies_channels(&only_channel)
        })
        .collect();
    ablation_from_pairs(kind, channel, &filtered, capability_seeds)
}

fn ablation_from_pairs(
    kind: &str,
    channel: &str,
    pairs: &[&RawRetrievalPair],
    capability_seeds: &[CapabilityDomainSeeds],
) -> RetrievalAblationScenario {
    let mut per_capability: BTreeMap<String, usize> = BTreeMap::new();
    for pair in pairs {
        *per_capability.entry(pair.capability_key.clone()).or_default() += 1;
    }
    let counts: Vec<usize> = capability_seeds
        .iter()
        .map(|seed| per_capability.get(&seed.capability_key).copied().unwrap_or(0))
        .collect();
    let (median, p95) = median_and_p95(&counts);
    RetrievalAblationScenario {
        scenario_kind: kind.into(),
        channel: channel.into(),
        candidate_edge_count: pairs.len(),
        capability_candidate_median: median,
        capability_candidate_p95: p95,
    }
}

fn median_and_p95(counts: &[usize]) -> (f64, usize) {
    if counts.is_empty() {
        return (0.0, 0);
    }
    let mut sorted = counts.to_vec();
    sorted.sort_unstable();
    let median = if sorted.len() % 2 == 0 {
        let mid = sorted.len() / 2;
        (sorted[mid - 1] + sorted[mid]) as f64 / 2.0
    } else {
        sorted[sorted.len() / 2] as f64
    };
    let p95_index = ((sorted.len() as f64 - 1.0) * 0.95).round() as usize;
    (median, sorted[p95_index.min(sorted.len() - 1)])
}

fn summarize_weak_contributions(pairs: &[&RawRetrievalPair]) -> WeakEvidenceContributions {
    WeakEvidenceContributions {
        weak_lexical_edges: pairs.iter().filter(|pair| pair.weak_lexical).count(),
        generic_module_package_edges: pairs
            .iter()
            .filter(|pair| pair.weak_generic_module_package)
            .count(),
        generic_owner_role_edges: pairs
            .iter()
            .filter(|pair| pair.weak_generic_owner_role)
            .count(),
        behavior_only_edges: pairs.iter().filter(|pair| pair.behavior_only).count(),
    }
}

fn summarize_assignments(assignments: &[CapabilityAnchorAssignment]) -> RetrievalAssignmentSummary {
    RetrievalAssignmentSummary {
        confident_assignments: assignments
            .iter()
            .filter(|assignment| assignment.assignment_state == "confident")
            .count(),
        ambiguous_assignments: assignments
            .iter()
            .filter(|assignment| assignment.assignment_state == "ambiguous")
            .count(),
        weak_assignments: assignments
            .iter()
            .filter(|assignment| assignment.assignment_state == "weak")
            .count(),
        unassigned_assignments: assignments
            .iter()
            .filter(|assignment| {
                assignment.retrieved_candidate_count > 0 && assignment.assignment_state == "unassigned"
            })
            .count(),
        no_candidate_assignments: assignments
            .iter()
            .filter(|assignment| assignment.retrieved_candidate_count == 0)
            .count(),
    }
}

fn top_hypothesis_fanout(
    edges: &[AnchorCapabilityEdge],
    eligible_contexts: &[HypothesisContext],
    capability_count: usize,
) -> Vec<HypothesisRetrievalFanoutRecord> {
    let mut records = eligible_contexts
        .iter()
        .filter(|context| context.domain_anchor_eligible)
        .map(|context| {
            let hypothesis_edges: Vec<_> = edges
                .iter()
                .filter(|edge| edge.hypothesis_id == context.hypothesis_id)
                .collect();
            let fanout = hypothesis_edges
                .iter()
                .map(|edge| edge.capability_key.as_str())
                .collect::<BTreeSet<_>>()
                .len();
            let dominant = dominant_channels(
                &hypothesis_edges
                    .iter()
                    .flat_map(|edge| edge.retrieval_channels.iter().cloned())
                    .collect::<Vec<_>>(),
            );
            HypothesisRetrievalFanoutRecord {
                hypothesis_id: context.hypothesis_id.clone(),
                representative_root_concept: context.representative.root_concept.clone(),
                representative_family_id: super::domain_seed_role_graph::family_id(
                    &context.representative,
                ),
                retrieval_fanout_capabilities: fanout,
                retrieval_fanout_ratio: if capability_count == 0 {
                    0.0
                } else {
                    fanout as f64 / capability_count as f64
                },
                dominant_retrieval_channels: dominant,
            }
        })
        .collect::<Vec<_>>();
    records.sort_by(|left, right| {
        right
            .retrieval_fanout_capabilities
            .cmp(&left.retrieval_fanout_capabilities)
            .then_with(|| left.hypothesis_id.cmp(&right.hypothesis_id))
    });
    records.truncate(20);
    records
}

fn top_capability_candidate_load(
    assignments: &[CapabilityAnchorAssignment],
) -> Vec<CapabilityCandidateLoadRecord> {
    let mut records = assignments
        .iter()
        .map(|assignment| {
            let dominant = dominant_channels(
                &assignment
                    .top_candidates
                    .iter()
                    .flat_map(|candidate| candidate.retrieval_channels.iter().cloned())
                    .collect::<Vec<_>>(),
            );
            CapabilityCandidateLoadRecord {
                capability_key: assignment.capability_key.clone(),
                retrieved_candidate_count: assignment.retrieved_candidate_count,
                dominant_retrieval_channels: dominant,
            }
        })
        .collect::<Vec<_>>();
    records.sort_by(|left, right| {
        right
            .retrieved_candidate_count
            .cmp(&left.retrieved_candidate_count)
            .then_with(|| left.capability_key.cmp(&right.capability_key))
    });
    records.truncate(20);
    records
}

fn dominant_channels(channels: &[String]) -> Vec<String> {
    let mut counts = BTreeMap::<String, usize>::new();
    for channel in channels {
        *counts.entry(channel.clone()).or_default() += 1;
    }
    let mut ranked = counts.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    ranked.into_iter().take(3).map(|(channel, _)| channel).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channels(values: &[&str]) -> BTreeSet<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn owner_only_retrieval은_후보를_생성하지_않는다() {
        let owner_only = channels(&["owner"]);
        assert!(!retrieval_qualifies_channels(&owner_only));
        assert!(is_suppressed_owner_only_retrieval(&owner_only));
    }

    #[test]
    fn owner는_behavior_call과_함께면_후보를_유지한다() {
        let with_behavior = channels(&["owner", "behaviorCall"]);
        assert!(retrieval_qualifies_channels(&with_behavior));
        assert!(!is_suppressed_owner_only_retrieval(&with_behavior));
    }

    #[test]
    fn owner는_behavior_flow와_함께면_후보를_유지한다() {
        let with_behavior = channels(&["owner", "behaviorFlow"]);
        assert!(retrieval_qualifies_channels(&with_behavior));
        assert!(!is_suppressed_owner_only_retrieval(&with_behavior));
    }

    #[test]
    fn owner는_entity_resource와_함께면_후보를_유지한다() {
        let with_entity = channels(&["owner", "entityResource"]);
        assert!(retrieval_qualifies_channels(&with_entity));
        assert!(!is_suppressed_owner_only_retrieval(&with_entity));
    }

    #[test]
    fn owner_lexical_only는_owner_gate로_suppress된다() {
        let owner_lexical = channels(&["owner", "lexical"]);
        assert!(!retrieval_qualifies_channels(&owner_lexical));
        assert!(is_suppressed_owner_only_retrieval(&owner_lexical));
    }
}
