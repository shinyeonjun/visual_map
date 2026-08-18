//! Domain anchor → capability affinity diagnostics.

use super::domain_seed_anchor_eligibility::{
    DomainAnchorEligibilityDiagnostics, HypothesisContext,
};
use super::domain_seed_aggregation::RankedConceptFamily;
use super::domain_seed_diagnostics::{CapabilityDomainSeeds, DomainSeedRawEvidence};
use super::domain_seed_provenance::{
    FamilySupportSignature, ProvenanceSeedCandidateGraph,
};
use super::domain_seed_role_graph::{
    family_id, family_matches_candidate, module_tail_segment, owner_business_stem,
    package_tail_segment,
};
use super::key_decomposition::{atomize_concept_label, tokenize_capability_key};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const SYMBOLIC_COMPONENT_WEIGHT: f64 = 1.0;
const CONFIDENT_MIN_SCORE: f64 = 2.5;
const CONFIDENT_MIN_MARGIN: f64 = 0.75;
const AMBIGUOUS_MIN_SCORE: f64 = 1.0;
const WEAK_MIN_SCORE: f64 = 0.25;
const TOP_CANDIDATE_COUNT: usize = 3;

const STATE_RESOURCE_EVIDENCE_SOURCES: &[&str] =
    &["entityVocabulary", "resourceOwnership"];
const STATE_RESOURCE_UNIT_KINDS: &[&str] = &[
    "entity", "record", "struct", "class", "interface", "trait", "table", "collection",
];
const TRANSPORT_CONTRACT_PREFIXES: &[&str] = &[
    "api", "v1", "v2", "v3", "rpc", "ws", "graphql", "public", "internal", "admin-api",
    "shop-api", "adminapi", "shopapi",
];
const CORRELATED_NAMING_SOURCES: &[&str] =
    &["lexical", "entityVocabulary", "contractNamespace", "capabilityKey"];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AnchorCapabilityGraph {
    pub diagnostic_anchor_count: usize,
    pub hypothesis_node_count: usize,
    pub eligible_domain_anchor_count: usize,
    pub subconcept_candidate_count: usize,
    pub redundant_anchor_candidate_count: usize,
    pub explicit_anchor_count: usize,
    pub high_signed_ambiguous_anchor_count: usize,
    pub excluded_action_cross_cutting_count: usize,
    pub capability_count: usize,
    pub edge_count: usize,
    pub retrieval_summary: CandidateRetrievalSummary,
    pub hypothesis_nodes: Vec<HypothesisAnchorNode>,
    pub capability_nodes: Vec<CapabilityDiagnosticNode>,
    pub edges: Vec<AnchorCapabilityEdge>,
    pub capability_assignments: Vec<CapabilityAnchorAssignment>,
    pub retrieval_ablation: super::domain_seed_retrieval_ablation::CandidateRetrievalAblationDiagnostics,
    pub assignment_ambiguity: super::domain_seed_assignment_ambiguity::AssignmentAmbiguityDiagnostics,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CandidateRetrievalSummary {
    pub min_retrieved_candidate_count: usize,
    pub median_retrieved_candidate_count: f64,
    pub p95_retrieved_candidate_count: usize,
    pub max_retrieved_candidate_count: usize,
    pub total_affinity_edge_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HypothesisAnchorNode {
    pub hypothesis_id: String,
    pub group_id: String,
    pub representative_root_concept: String,
    pub alternative_root_concepts: Vec<String>,
    pub representative_family_id: String,
    pub eligibility_class: String,
    pub domain_anchor_eligible: bool,
    pub signed_anchor_score: f64,
    pub support_signature: FamilySupportSignature,
    pub diagnostic_family_inclusion_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityDiagnosticNode {
    pub capability_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AffinityComponentScores {
    pub lexical_alignment: f64,
    pub owner_alignment: f64,
    pub module_package_alignment: f64,
    pub entity_resource_alignment: f64,
    pub behavior_alignment: f64,
    pub contract_alignment: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceCorrelatedEvidenceGroup {
    pub group_id: String,
    pub primitive_observation_ids: Vec<String>,
    pub evidence_sources: Vec<String>,
    pub merged_independent_groups: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorCapabilityEdge {
    pub hypothesis_id: String,
    pub representative_family_id: String,
    pub representative_root_concept: String,
    pub capability_key: String,
    pub retrieval_channels: Vec<String>,
    pub retrieval_reasons: Vec<String>,
    pub strong_structural_reason_count: usize,
    pub weak_reason_count: usize,
    pub has_ownership_reason: bool,
    pub has_entity_resource_reason: bool,
    pub has_behavior_reason: bool,
    pub has_lexical_only_reason: bool,
    pub weak_lexical: bool,
    pub weak_generic_module_package: bool,
    pub weak_generic_owner_role: bool,
    pub behavior_only: bool,
    pub raw_evidence: Vec<String>,
    pub provenance_correlated_groups: Vec<ProvenanceCorrelatedEvidenceGroup>,
    pub independent_evidence_groups: Vec<String>,
    pub component_scores: AffinityComponentScores,
    pub symbolic_affinity_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorCandidateScore {
    pub hypothesis_id: String,
    pub representative_family_id: String,
    pub representative_root_concept: String,
    pub symbolic_affinity_score: f64,
    pub retrieval_channels: Vec<String>,
    pub rank: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityAnchorAssignment {
    pub capability_key: String,
    pub retrieved_candidate_count: usize,
    pub top_candidates: Vec<AnchorCandidateScore>,
    pub top1_score: f64,
    pub top2_score: f64,
    pub margin: f64,
    pub assignment_state: String,
    pub assignment_reason: String,
}

#[derive(Debug, Clone)]
struct PairEvidenceObservation {
    channel: String,
    independent_group: String,
    primitive_observation_id: String,
    capability_key: String,
    entrypoint_id: Option<String>,
}

#[derive(Debug, Clone)]
struct PairEvidenceAnalysis {
    raw_evidence: Vec<String>,
    provenance_correlated_groups: Vec<ProvenanceCorrelatedEvidenceGroup>,
    independent_evidence_groups: Vec<String>,
    component_scores: AffinityComponentScores,
    symbolic_affinity_score: f64,
}

pub fn build_anchor_capability_graph(
    families: &[RankedConceptFamily],
    capability_seeds: &[CapabilityDomainSeeds],
    _provenance_graph: &ProvenanceSeedCandidateGraph,
    eligibility: &DomainAnchorEligibilityDiagnostics,
    hypothesis_contexts: &[HypothesisContext],
) -> AnchorCapabilityGraph {
    let excluded_action_cross_cutting_count = families
        .iter()
        .filter(|family| family.concept_role.role_class == "actionCrossCutting")
        .count();
    let explicit_anchor_count = eligibility
        .hypotheses
        .iter()
        .flat_map(|hypothesis| hypothesis.diagnostic_family_inclusions.iter())
        .filter(|inclusion| inclusion.inclusion_reason == "explicitAnchor")
        .count();
    let high_signed_ambiguous_anchor_count = eligibility
        .hypotheses
        .iter()
        .flat_map(|hypothesis| hypothesis.diagnostic_family_inclusions.iter())
        .filter(|inclusion| inclusion.inclusion_reason == "highSignedAmbiguousAnchor")
        .count();

    let hypothesis_nodes = hypothesis_contexts
        .iter()
        .map(|context| HypothesisAnchorNode {
            hypothesis_id: context.hypothesis_id.clone(),
            group_id: context.group.group_id.clone(),
            representative_root_concept: context.representative.root_concept.clone(),
            alternative_root_concepts: context
                .group
                .competing_root_concepts
                .iter()
                .filter(|root| *root != &context.representative.root_concept)
                .cloned()
                .collect(),
            representative_family_id: family_id(&context.representative),
            eligibility_class: context.eligibility_class.clone(),
            domain_anchor_eligible: context.domain_anchor_eligible,
            signed_anchor_score: context.max_signed_anchor_score,
            support_signature: context.group.support_signature.clone(),
            diagnostic_family_inclusion_count: context.diagnostic_inclusions.len(),
        })
        .collect::<Vec<_>>();

    let capability_nodes = capability_seeds
        .iter()
        .map(|seed| CapabilityDiagnosticNode {
            capability_key: seed.capability_key.clone(),
        })
        .collect::<Vec<_>>();

    let eligible_contexts = hypothesis_contexts
        .iter()
        .filter(|context| context.domain_anchor_eligible)
        .cloned()
        .collect::<Vec<_>>();

    let mut edges = Vec::new();
    let mut raw_pairs = Vec::new();
    let mut retrieved_counts = Vec::new();
    for capability in capability_seeds {
        let mut capability_retrieved = 0usize;
        for context in eligible_contexts
            .iter()
            .filter(|context| context.domain_anchor_eligible)
        {
            let channels = evaluate_retrieval_channels(&context.representative, capability);
            let (weak_lexical, weak_generic_module_package, weak_generic_owner_role, behavior_only) =
                super::domain_seed_retrieval_ablation::classify_weak_evidence(
                    &channels,
                    context.representative.genericness,
                    context.representative.transportness,
                );
            raw_pairs.push(super::domain_seed_retrieval_ablation::RawRetrievalPair {
                hypothesis_id: context.hypothesis_id.clone(),
                capability_key: capability.capability_key.clone(),
                channels: channels.clone(),
                weak_lexical,
                weak_generic_module_package,
                weak_generic_owner_role,
                behavior_only,
            });
            if !super::domain_seed_retrieval_ablation::retrieval_qualifies_channels(&channels) {
                continue;
            }
            capability_retrieved += 1;
            let analysis =
                analyze_anchor_capability_pair(&context.representative, capability);
            if analysis.symbolic_affinity_score <= 0.0 && analysis.raw_evidence.is_empty() {
                continue;
            }
            let channel_vec = channels.iter().cloned().collect::<Vec<_>>();
            let metrics =
                super::domain_seed_retrieval_ablation::edge_channel_metrics(&channels);
            edges.push(AnchorCapabilityEdge {
                hypothesis_id: context.hypothesis_id.clone(),
                representative_family_id: family_id(&context.representative),
                representative_root_concept: context.representative.root_concept.clone(),
                capability_key: capability.capability_key.clone(),
                retrieval_channels: channel_vec.clone(),
                retrieval_reasons: channel_vec,
                strong_structural_reason_count: metrics.strong_structural_reason_count,
                weak_reason_count: metrics.weak_reason_count,
                has_ownership_reason: metrics.has_ownership_reason,
                has_entity_resource_reason: metrics.has_entity_resource_reason,
                has_behavior_reason: metrics.has_behavior_reason,
                has_lexical_only_reason: metrics.has_lexical_only_reason,
                weak_lexical,
                weak_generic_module_package,
                weak_generic_owner_role,
                behavior_only,
                raw_evidence: analysis.raw_evidence,
                provenance_correlated_groups: analysis.provenance_correlated_groups,
                independent_evidence_groups: analysis.independent_evidence_groups,
                component_scores: analysis.component_scores,
                symbolic_affinity_score: analysis.symbolic_affinity_score,
            });
        }
        retrieved_counts.push(capability_retrieved);
    }
    edges.sort_by(|left, right| {
        left.hypothesis_id
            .cmp(&right.hypothesis_id)
            .then_with(|| left.capability_key.cmp(&right.capability_key))
    });

    let capability_assignments = compute_capability_assignments(capability_seeds, &edges);
    let retrieval_summary = summarize_retrieval(&retrieved_counts, edges.len());
    let retrieval_ablation = super::domain_seed_retrieval_ablation::build_retrieval_ablation_diagnostics(
        &raw_pairs,
        &edges,
        &capability_assignments,
        capability_seeds,
        &eligible_contexts,
    );
    let assignment_ambiguity =
        super::domain_seed_assignment_ambiguity::build_assignment_ambiguity_diagnostics(
            &edges,
            &capability_assignments,
            families,
            capability_seeds,
            &eligible_contexts,
            &raw_pairs,
            &eligibility.concept_hierarchy,
            &_provenance_graph.primitive_relation_inventory,
        );

    AnchorCapabilityGraph {
        diagnostic_anchor_count: eligibility.diagnostic_anchor_count,
        hypothesis_node_count: eligibility.hypothesis_node_count,
        eligible_domain_anchor_count: eligibility.eligible_domain_anchor_count,
        subconcept_candidate_count: eligibility.subconcept_candidate_count,
        redundant_anchor_candidate_count: eligibility.redundant_anchor_candidate_count,
        explicit_anchor_count,
        high_signed_ambiguous_anchor_count,
        excluded_action_cross_cutting_count,
        capability_count: capability_nodes.len(),
        edge_count: edges.len(),
        retrieval_summary,
        hypothesis_nodes,
        capability_nodes,
        edges,
        capability_assignments,
        retrieval_ablation,
        assignment_ambiguity,
    }
}

fn evaluate_retrieval_channels(
    anchor: &RankedConceptFamily,
    capability: &CapabilityDomainSeeds,
) -> BTreeSet<String> {
    let normalized_root = anchor.concept_role.normalized_root_concept.clone();
    let mut channels = BTreeSet::new();
    if capability_matches_anchor_capability(anchor, &capability.capability_key, &normalized_root)
        || capability
            .candidates
            .iter()
            .any(|candidate| family_matches_candidate(anchor, &candidate.concept))
    {
        channels.insert("lexical".into());
    }
    if owner_alignment(anchor, capability, &[]) > 0.0 {
        channels.insert("owner".into());
    }
    if module_package_alignment(anchor, capability, &[]) > 0.0 {
        channels.insert("modulePackage".into());
    }
    if entity_resource_alignment(capability, &[]) > 0.0
        || capability.candidates.iter().any(|candidate| {
            STATE_RESOURCE_EVIDENCE_SOURCES.contains(&candidate.evidence_source.as_str())
                && family_matches_candidate(anchor, &candidate.concept)
        })
    {
        channels.insert("entityResource".into());
    }
    if behavior_flow_match(anchor, capability) {
        channels.insert("behaviorFlow".into());
    }
    if behavior_call_match(anchor, capability) {
        channels.insert("behaviorCall".into());
    }
    if contract_alignment(anchor, capability, &[]) > 0.0 {
        channels.insert("contract".into());
    }
    channels
}

fn summarize_retrieval(retrieved_counts: &[usize], total_edges: usize) -> CandidateRetrievalSummary {
    if retrieved_counts.is_empty() {
        return CandidateRetrievalSummary {
            total_affinity_edge_count: total_edges,
            ..Default::default()
        };
    }
    let mut sorted = retrieved_counts.to_vec();
    sorted.sort_unstable();
    let min = *sorted.first().unwrap_or(&0);
    let max = *sorted.last().unwrap_or(&0);
    let median = if sorted.len() % 2 == 0 {
        let mid = sorted.len() / 2;
        (sorted[mid - 1] + sorted[mid]) as f64 / 2.0
    } else {
        sorted[sorted.len() / 2] as f64
    };
    let p95_index = ((sorted.len() as f64 - 1.0) * 0.95).round() as usize;
    let p95 = sorted[p95_index.min(sorted.len() - 1)];
    CandidateRetrievalSummary {
        min_retrieved_candidate_count: min,
        median_retrieved_candidate_count: median,
        p95_retrieved_candidate_count: p95,
        max_retrieved_candidate_count: max,
        total_affinity_edge_count: total_edges,
    }
}

fn analyze_anchor_capability_pair(
    anchor: &RankedConceptFamily,
    capability: &CapabilityDomainSeeds,
) -> PairEvidenceAnalysis {
    let normalized_root = anchor.concept_role.normalized_root_concept.clone();
    let mut observations = Vec::new();

    if capability_matches_anchor_capability(anchor, &capability.capability_key, &normalized_root)
    {
        observations.push(observation(
            "capabilityKeyMatch",
            "lexical",
            format!("cap:{}:key", capability.capability_key),
            capability.capability_key.clone(),
            capability.coverage.entrypoint_ids.first().cloned(),
        ));
    }
    for candidate in &capability.candidates {
        if !family_matches_candidate(anchor, &candidate.concept) {
            continue;
        }
        let entrypoint_id = candidate_entrypoint(capability);
        observations.push(observation(
            &format!("conceptMatch:{}", candidate.evidence_source),
            evidence_group_for_source(&candidate.evidence_source),
            primitive_id(
                &capability.capability_key,
                entrypoint_id.as_deref(),
                &candidate.evidence_source,
                &candidate.concept,
            ),
            capability.capability_key.clone(),
            entrypoint_id,
        ));
    }

    let component_scores = AffinityComponentScores {
        lexical_alignment: lexical_alignment(anchor, capability, &normalized_root, &observations),
        owner_alignment: owner_alignment(anchor, capability, &observations),
        module_package_alignment: module_package_alignment(anchor, capability, &observations),
        entity_resource_alignment: entity_resource_alignment(capability, &observations),
        behavior_alignment: behavior_alignment(anchor, capability, &observations),
        contract_alignment: contract_alignment(anchor, capability, &observations),
    };
    let (provenance_correlated_groups, independent_evidence_groups) =
        collapse_pair_provenance(&observations);
    let raw_evidence = observations
        .iter()
        .map(|observation| observation.channel.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let symbolic_affinity_score = symbolic_affinity_score(&component_scores);

    PairEvidenceAnalysis {
        raw_evidence,
        provenance_correlated_groups,
        independent_evidence_groups,
        component_scores,
        symbolic_affinity_score,
    }
}

fn symbolic_affinity_score(scores: &AffinityComponentScores) -> f64 {
    [
        scores.lexical_alignment,
        scores.owner_alignment,
        scores.module_package_alignment,
        scores.entity_resource_alignment,
        scores.behavior_alignment,
        scores.contract_alignment,
    ]
    .into_iter()
    .map(|score| {
        if score > 0.0 {
            score * SYMBOLIC_COMPONENT_WEIGHT
        } else {
            0.0
        }
    })
    .sum()
}

fn compute_capability_assignments(
    capability_seeds: &[CapabilityDomainSeeds],
    edges: &[AnchorCapabilityEdge],
) -> Vec<CapabilityAnchorAssignment> {
    capability_seeds
        .iter()
        .map(|capability| {
            let capability_edges: Vec<_> = edges
                .iter()
                .filter(|edge| edge.capability_key == capability.capability_key)
                .collect();
            let retrieved_candidate_count = capability_edges
                .iter()
                .map(|edge| edge.hypothesis_id.as_str())
                .collect::<BTreeSet<_>>()
                .len();

            let mut candidates = BTreeMap::<String, &AnchorCapabilityEdge>::new();
            for edge in &capability_edges {
                candidates
                    .entry(edge.hypothesis_id.clone())
                    .and_modify(|existing| {
                        if edge.symbolic_affinity_score > existing.symbolic_affinity_score {
                            *existing = edge;
                        }
                    })
                    .or_insert(edge);
            }
            let mut candidates = candidates.into_values().collect::<Vec<_>>();

            candidates.sort_by(|left, right| {
                right
                    .symbolic_affinity_score
                    .partial_cmp(&left.symbolic_affinity_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| left.representative_family_id.cmp(&right.representative_family_id))
            });

            let top1_score = candidates.first().map(|edge| edge.symbolic_affinity_score).unwrap_or(0.0);
            let top2_score = candidates.get(1).map(|edge| edge.symbolic_affinity_score).unwrap_or(0.0);
            let margin = top1_score - top2_score;
            let top_edge = candidates.first().copied();
            let (assignment_state, assignment_reason) =
                classify_assignment(top1_score, margin, top_edge);

            let top_candidates = candidates
                .into_iter()
                .take(TOP_CANDIDATE_COUNT)
                .enumerate()
                .map(|(index, edge)| AnchorCandidateScore {
                    rank: index + 1,
                    hypothesis_id: edge.hypothesis_id.clone(),
                    representative_family_id: edge.representative_family_id.clone(),
                    representative_root_concept: edge.representative_root_concept.clone(),
                    symbolic_affinity_score: edge.symbolic_affinity_score,
                    retrieval_channels: edge.retrieval_channels.clone(),
                })
                .collect();

            CapabilityAnchorAssignment {
                capability_key: capability.capability_key.clone(),
                retrieved_candidate_count,
                top_candidates,
                top1_score,
                top2_score,
                margin,
                assignment_state,
                assignment_reason,
            }
        })
        .collect()
}

fn classify_assignment(
    top1_score: f64,
    margin: f64,
    top_edge: Option<&AnchorCapabilityEdge>,
) -> (String, String) {
    if top1_score < WEAK_MIN_SCORE {
        return (
            "unassigned".into(),
            "no anchor-capability affinity above weak threshold".into(),
        );
    }
    if lexical_or_contract_only(top_edge) {
        if top1_score >= CONFIDENT_MIN_SCORE {
            return (
                "ambiguous".into(),
                "lexical/contract-only evidence cannot produce confident assignment".into(),
            );
        }
        if top1_score >= AMBIGUOUS_MIN_SCORE {
            return (
                "weak".into(),
                "lexical/contract-only evidence capped at weak assignment".into(),
            );
        }
        return (
            "weak".into(),
            "lexical/contract-only evidence only".into(),
        );
    }
    if top1_score >= CONFIDENT_MIN_SCORE && margin >= CONFIDENT_MIN_MARGIN {
        return (
            "confident".into(),
            "top1 score and margin exceed experimental symbolic thresholds".into(),
        );
    }
    if top1_score >= AMBIGUOUS_MIN_SCORE {
        return (
            "ambiguous".into(),
            "top1 score present but margin or evidence mix is insufficient for confidence".into(),
        );
    }
    (
        "weak".into(),
        "affinity above zero but below ambiguous threshold".into(),
    )
}

fn lexical_or_contract_only(edge: Option<&AnchorCapabilityEdge>) -> bool {
    let Some(edge) = edge else {
        return true;
    };
    let mut groups = edge
        .independent_evidence_groups
        .iter()
        .map(|group| group.as_str())
        .collect::<Vec<_>>();
    groups.sort_unstable();
    if groups.is_empty() {
        let channels = edge.retrieval_channels.iter().cloned().collect::<BTreeSet<_>>();
        return channels.is_empty()
            || channels == BTreeSet::from(["contract".to_string()])
            || channels == BTreeSet::from(["lexical".to_string()])
            || channels == BTreeSet::from(["contract".to_string(), "lexical".to_string()]);
    }
    groups.is_empty()
        || groups == ["contract"]
        || groups == ["lexical"]
        || groups == ["contract", "lexical"]
}

fn collapse_pair_provenance(
    observations: &[PairEvidenceObservation],
) -> (Vec<ProvenanceCorrelatedEvidenceGroup>, Vec<String>) {
    let mut buckets: BTreeMap<(String, Option<String>), Vec<&PairEvidenceObservation>> =
        BTreeMap::new();
    for observation in observations {
        if CORRELATED_NAMING_SOURCES.contains(&observation.independent_group.as_str()) {
            buckets
                .entry((
                    observation.capability_key.clone(),
                    observation.entrypoint_id.clone(),
                ))
                .or_default()
                .push(observation);
        }
    }

    let mut correlated_groups = Vec::new();
    let mut absorbed = BTreeSet::new();
    for (index, ((capability_key, entrypoint_id), bucket)) in buckets.into_iter().enumerate() {
        let merged_groups = bucket
            .iter()
            .map(|observation| observation.independent_group.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if merged_groups.len() < 2 {
            continue;
        }
        for observation in &bucket {
            absorbed.insert(observation.primitive_observation_id.clone());
        }
        correlated_groups.push(ProvenanceCorrelatedEvidenceGroup {
            group_id: format!(
                "corr:{capability_key}:{}:{index}",
                entrypoint_id.as_deref().unwrap_or("none")
            ),
            primitive_observation_ids: bucket
                .iter()
                .map(|observation| observation.primitive_observation_id.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            evidence_sources: bucket
                .iter()
                .map(|observation| observation.channel.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            merged_independent_groups: merged_groups,
        });
    }

    let mut independent = observations
        .iter()
        .filter(|observation| !absorbed.contains(&observation.primitive_observation_id))
        .map(|observation| observation.independent_group.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    independent.sort();
    (correlated_groups, independent)
}

fn lexical_alignment(
    anchor: &RankedConceptFamily,
    capability: &CapabilityDomainSeeds,
    normalized_root: &str,
    observations: &[PairEvidenceObservation],
) -> f64 {
    let mut score = 0.0_f64;
    if observations
        .iter()
        .any(|observation| observation.independent_group == "lexical")
    {
        score += 0.6;
    }
    if capability_matches_anchor_capability(anchor, &capability.capability_key, normalized_root) {
        score += 0.4;
    }
    if anchor
        .distinct_capability_keys
        .contains(&capability.capability_key)
    {
        score = score.max(1.0);
    }
    score.clamp(0.0, 1.0)
}

fn owner_alignment(
    anchor: &RankedConceptFamily,
    capability: &CapabilityDomainSeeds,
    observations: &[PairEvidenceObservation],
) -> f64 {
    if observations
        .iter()
        .any(|observation| observation.independent_group == "ownership")
    {
        return 1.0;
    }
    let anchor_stems = anchor
        .distinct_owner_classes
        .iter()
        .map(|owner| owner_business_stem(owner))
        .collect::<BTreeSet<_>>();
    if anchor_stems.is_empty() {
        return 0.0;
    }
    let matches = capability
        .coverage
        .owner_classes
        .iter()
        .map(|owner| owner_business_stem(owner))
        .filter(|stem| {
            anchor_stems.iter().any(|anchor_stem| {
                stems_align(anchor_stem, stem)
                    || stems_align(
                        anchor_stem,
                        &anchor.concept_role.normalized_root_concept,
                    )
            })
        })
        .count();
    ratio(matches, capability.coverage.owner_classes.len().max(1))
}

fn module_package_alignment(
    anchor: &RankedConceptFamily,
    capability: &CapabilityDomainSeeds,
    observations: &[PairEvidenceObservation],
) -> f64 {
    if observations
        .iter()
        .any(|observation| observation.independent_group == "structural")
    {
        return 1.0;
    }
    let anchor_modules = anchor
        .distinct_module_paths
        .iter()
        .map(|module| module_tail_segment(module))
        .collect::<BTreeSet<_>>();
    let cap_modules = capability
        .coverage
        .module_paths
        .iter()
        .map(|module| module_tail_segment(module))
        .chain(
            capability
                .coverage
                .package_paths
                .iter()
                .map(|package| package_tail_segment(package)),
        )
        .collect::<BTreeSet<_>>();
    jaccard_sets(&anchor_modules, &cap_modules)
}

fn entity_resource_alignment(
    capability: &CapabilityDomainSeeds,
    observations: &[PairEvidenceObservation],
) -> f64 {
    if observations
        .iter()
        .any(|observation| observation.independent_group == "stateResource")
    {
        return 1.0;
    }
    let resource_entities = capability
        .candidates
        .iter()
        .filter(|candidate| STATE_RESOURCE_EVIDENCE_SOURCES.contains(&candidate.evidence_source.as_str()))
        .filter_map(|candidate| state_resource_name(&candidate.raw_evidence))
        .collect::<BTreeSet<_>>();
    if resource_entities.is_empty() {
        return 0.0;
    }
    (resource_entities.len() as f64 / resource_entities.len().max(1) as f64).min(1.0)
}

fn behavior_flow_match(
    anchor: &RankedConceptFamily,
    capability: &CapabilityDomainSeeds,
) -> bool {
    let anchor_flows = anchor.provenance.flow_ids.iter().collect::<BTreeSet<_>>();
    let capability_flows = capability.coverage.flow_ids.iter().collect::<BTreeSet<_>>();
    if anchor_flows.is_empty() || capability_flows.is_empty() {
        return false;
    }
    if anchor_flows.intersection(&capability_flows).count() == 0 {
        return false;
    }
    let anchor_entrypoints = anchor.provenance.entrypoint_ids.iter().collect::<BTreeSet<_>>();
    let capability_entrypoints = capability.coverage.entrypoint_ids.iter().collect::<BTreeSet<_>>();
    anchor_entrypoints != capability_entrypoints
}

fn behavior_call_match(
    anchor: &RankedConceptFamily,
    capability: &CapabilityDomainSeeds,
) -> bool {
    let anchor_entrypoints = anchor.provenance.entrypoint_ids.iter().collect::<BTreeSet<_>>();
    let capability_entrypoints = capability.coverage.entrypoint_ids.iter().collect::<BTreeSet<_>>();
    if anchor_entrypoints.is_disjoint(&capability_entrypoints) {
        return false;
    }
    anchor.provenance.unit_ids != capability.coverage.unit_ids
        || anchor.distinct_capability_keys.contains(&capability.capability_key)
}

fn behavior_alignment(
    anchor: &RankedConceptFamily,
    capability: &CapabilityDomainSeeds,
    observations: &[PairEvidenceObservation],
) -> f64 {
    if observations
        .iter()
        .any(|observation| observation.independent_group == "behavior")
    {
        return 1.0;
    }
    let anchor_flows = anchor.provenance.flow_ids.iter().collect::<BTreeSet<_>>();
    let capability_flows = capability.coverage.flow_ids.iter().collect::<BTreeSet<_>>();
    if anchor_flows.is_empty() || capability_flows.is_empty() {
        return 0.0;
    }
    let shared = anchor_flows.intersection(&capability_flows).count();
    if shared == 0 {
        return 0.0;
    }
    let anchor_entrypoints = anchor.provenance.entrypoint_ids.iter().collect::<BTreeSet<_>>();
    let capability_entrypoints = capability.coverage.entrypoint_ids.iter().collect::<BTreeSet<_>>();
    if anchor_entrypoints == capability_entrypoints {
        return 0.0;
    }
    ratio(shared, capability_flows.len().max(1))
}

fn contract_alignment(
    anchor: &RankedConceptFamily,
    capability: &CapabilityDomainSeeds,
    observations: &[PairEvidenceObservation],
) -> f64 {
    if observations
        .iter()
        .any(|observation| observation.independent_group == "contract")
    {
        return 1.0;
    }
    let anchor_prefixes = contract_prefixes(&anchor.distinct_contract_paths);
    let capability_prefixes = contract_prefixes(&capability.coverage.contract_paths);
    let overlap = anchor_prefixes
        .intersection(&capability_prefixes)
        .filter(|prefix| !is_transport_contract_prefix(prefix))
        .count();
    if overlap == 0 {
        return 0.0;
    }
    ratio(overlap, capability_prefixes.len().max(1))
}

fn capability_matches_anchor_capability(
    anchor: &RankedConceptFamily,
    capability_key: &str,
    normalized_root: &str,
) -> bool {
    if anchor.distinct_capability_keys.iter().any(|key| key == capability_key) {
        return true;
    }
    tokenize_capability_key(capability_key)
        .iter()
        .any(|token| token == normalized_root)
        || atomize_concept_label(capability_key)
            .iter()
            .any(|token| token == normalized_root)
}

fn evidence_group_for_source(source: &str) -> &str {
    match source {
        "ownerClass" => "ownership",
        "semanticModule" | "semanticPackage" => "structural",
        "entityVocabulary" | "resourceOwnership" => "stateResource",
        "contractNamespace" => "contract",
        "capabilityKey" => "lexical",
        _ => "lexical",
    }
}

fn observation(
    channel: &str,
    independent_group: &str,
    primitive_observation_id: String,
    capability_key: String,
    entrypoint_id: Option<String>,
) -> PairEvidenceObservation {
    PairEvidenceObservation {
        channel: channel.into(),
        independent_group: independent_group.into(),
        primitive_observation_id,
        capability_key,
        entrypoint_id,
    }
}

fn primitive_id(
    capability_key: &str,
    entrypoint_id: Option<&str>,
    source: &str,
    value: &str,
) -> String {
    format!(
        "{capability_key}|{}|{source}|{value}",
        entrypoint_id.unwrap_or("none")
    )
}

fn candidate_entrypoint(capability: &CapabilityDomainSeeds) -> Option<String> {
    capability.coverage.entrypoint_ids.first().cloned()
}

fn state_resource_name(raw: &DomainSeedRawEvidence) -> Option<String> {
    if let Some(unit_name) = raw.unit_name.as_deref() {
        if is_state_resource_unit(raw.unit_kind.as_deref()) {
            return Some(unit_name.to_string());
        }
    }
    raw.resource_name.clone()
}

fn is_state_resource_unit(kind: Option<&str>) -> bool {
    kind.is_some_and(|value| {
        STATE_RESOURCE_UNIT_KINDS
            .iter()
            .any(|allowed| allowed == &value.to_ascii_lowercase())
    })
}

fn contract_prefixes(paths: &[String]) -> BTreeSet<String> {
    paths
        .iter()
        .filter_map(|path| contract_prefix(path))
        .collect()
}

fn contract_prefix(path: &str) -> Option<String> {
    let normalized = path.trim_matches('/').to_ascii_lowercase();
    normalized
        .split('/')
        .find(|segment| !segment.is_empty() && !segment.starts_with('{'))
        .map(|segment| segment.to_string())
}

fn is_transport_contract_prefix(prefix: &str) -> bool {
    TRANSPORT_CONTRACT_PREFIXES
        .iter()
        .any(|candidate| candidate == &prefix.to_ascii_lowercase())
}

fn stems_align(left: &str, right: &str) -> bool {
    let left = left.to_ascii_lowercase();
    let right = right.to_ascii_lowercase();
    left == right || left.contains(&right) || right.contains(&left)
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        (numerator as f64 / denominator as f64).clamp(0.0, 1.0)
    }
}

fn jaccard_sets(left: &BTreeSet<String>, right: &BTreeSet<String>) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let intersection = left.intersection(right).count() as f64;
    let union = left.union(right).count() as f64;
    (intersection / union).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::formation::domain_seed_retrieval_ablation;
    use crate::domain::formation::domain_seed_aggregation::IdfPenaltyDiagnostic;
    use crate::domain::formation::domain_seed_diagnostics::{
        CapabilitySeedCoverage, DomainSeedCandidate, DomainSeedConfidence,
    };
    use crate::domain::formation::domain_seed_role_graph::ConceptRoleDiagnostic;

    fn sample_capability(key: &str, owner: &str) -> CapabilityDomainSeeds {
        CapabilityDomainSeeds {
            capability_key: key.into(),
            coverage: CapabilitySeedCoverage {
                owner_classes: vec![owner.into()],
                contract_paths: vec![format!("/{key}")],
                ..Default::default()
            },
            candidates: vec![DomainSeedCandidate {
                concept: key.into(),
                evidence_source: "ownerClass".into(),
                confidence: DomainSeedConfidence::High,
                raw_evidence: DomainSeedRawEvidence {
                    owner_class: Some(owner.into()),
                    ..Default::default()
                },
            }],
        }
    }

    fn sample_anchor(root: &str, role: &str, symbolic_total: f64) -> RankedConceptFamily {
        RankedConceptFamily {
            rank: 1,
            root_concept: root.into(),
            child_concepts: vec![root.into()],
            atomized_path: root.into(),
            distinct_capabilities: 1,
            distinct_capability_keys: vec![format!("create-{root}")],
            distinct_entrypoints: 0,
            distinct_entrypoint_ids: Vec::new(),
            distinct_contracts: 1,
            distinct_contract_paths: vec![format!("/{root}")],
            distinct_owners: 1,
            distinct_owner_classes: vec![format!("{root}Controller")],
            distinct_modules: 0,
            distinct_module_paths: Vec::new(),
            distinct_units: 0,
            correlated_evidence_groups: Vec::new(),
            independent_evidence_groups: Vec::new(),
            coverage_score: 0.7,
            coherence_score: 0.6,
            specificity_score: 0.8,
            noise_penalty: 0.1,
            genericness: 0.1,
            transportness: 0.1,
            idf_penalty: IdfPenaltyDiagnostic {
                formula: "test".into(),
                project_local_frequency: 0.2,
                total_capabilities: 10,
                document_frequency: 2.0,
                high_frequency_threshold: 0.45,
                below_threshold: true,
                result: 1.0,
            },
            final_seed_score: 0.7,
            concept_role: ConceptRoleDiagnostic {
                position: "entity".into(),
                actionness: 0.2,
                entityness: 0.8,
                leading_verb_hits: 0,
                trailing_entity_hits: 1,
                ownership_evidence_hits: 1,
                identifier_position_hits: 1,
                context_dispersion: 0.2,
                business_root_alignment: 0.8,
                effective_context_dispersion: 0.04,
                role_class: role.into(),
                normalized_root_concept: root.into(),
                normalization_diagnostics: Vec::new(),
            },
            anchor_score_components: crate::domain::formation::domain_seed_recovery::AnchorScoreComponents {
                symbolic_total,
                ..Default::default()
            },
            provenance: Default::default(),
            support_signature: Default::default(),
        }
    }

    fn sample_edge(
        anchor: &RankedConceptFamily,
        capability: &CapabilityDomainSeeds,
        channels: &[&str],
        analysis: &PairEvidenceAnalysis,
    ) -> AnchorCapabilityEdge {
        let channel_set = channels.iter().map(|value| value.to_string()).collect::<BTreeSet<_>>();
        let metrics = domain_seed_retrieval_ablation::edge_channel_metrics(&channel_set);
        let (weak_lexical, weak_generic_module_package, weak_generic_owner_role, behavior_only) =
            domain_seed_retrieval_ablation::classify_weak_evidence(
                &channel_set,
                anchor.genericness,
                anchor.transportness,
            );
        AnchorCapabilityEdge {
            hypothesis_id: "hypothesis:1".into(),
            representative_family_id: family_id(anchor),
            representative_root_concept: anchor.root_concept.clone(),
            capability_key: capability.capability_key.clone(),
            retrieval_channels: channels.iter().map(|value| value.to_string()).collect(),
            retrieval_reasons: channels.iter().map(|value| value.to_string()).collect(),
            strong_structural_reason_count: metrics.strong_structural_reason_count,
            weak_reason_count: metrics.weak_reason_count,
            has_ownership_reason: metrics.has_ownership_reason,
            has_entity_resource_reason: metrics.has_entity_resource_reason,
            has_behavior_reason: metrics.has_behavior_reason,
            has_lexical_only_reason: metrics.has_lexical_only_reason,
            weak_lexical,
            weak_generic_module_package,
            weak_generic_owner_role,
            behavior_only,
            raw_evidence: analysis.raw_evidence.clone(),
            provenance_correlated_groups: analysis.provenance_correlated_groups.clone(),
            independent_evidence_groups: analysis.independent_evidence_groups.clone(),
            component_scores: analysis.component_scores.clone(),
            symbolic_affinity_score: analysis.symbolic_affinity_score,
        }
    }

    #[test]
    fn lexical_contract_only는_confident_assignment를_막는다() {
        let anchor = sample_anchor("order", "anchor", 3.0);
        let capability = sample_capability("orders", "OrdersController");
        let analysis = analyze_anchor_capability_pair(&anchor, &capability);
        let (state, _) = classify_assignment(
            analysis.symbolic_affinity_score,
            1.0,
            Some(&sample_edge(&anchor, &capability, &["lexical", "contract"], &analysis)),
        );
        assert_ne!(state, "confident");
    }

    #[test]
    fn lexical_only_retrieval은_단독으로_후보를_생성하지_않는다() {
        let lexical_only = BTreeSet::from(["lexical".to_string()]);
        assert!(!domain_seed_retrieval_ablation::retrieval_qualifies_channels(
            &lexical_only
        ));
    }
}