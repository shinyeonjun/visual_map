//! Assignment ambiguity decomposition diagnostics.

use super::domain_seed_anchor_affinity::{
    AffinityComponentScores, AnchorCapabilityEdge, CapabilityAnchorAssignment,
};
use super::domain_seed_anchor_eligibility::HypothesisContext;
use super::domain_seed_concept_hierarchy::ConceptHierarchyDiagnostics;
use super::domain_seed_responsibility_equivalence::ResponsibilityEquivalenceDiagnostics;
use super::domain_seed_responsibility_scope::ResponsibilityScopeDiagnostics;
use super::domain_seed_scope_classification_decomposition::ScopeClassificationDecompositionDiagnostics;
use super::domain_seed_scope_role_diagnostics::ScopeRoleDiagnostics;
use super::domain_seed_aggregation::RankedConceptFamily;
use super::domain_seed_diagnostics::CapabilityDomainSeeds;
use super::domain_seed_retrieval_ablation::{
    is_suppressed_owner_only_retrieval, OwnerSuppressionVerification,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const GENERICNESS_THRESHOLD: f64 = 0.45;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AssignmentAmbiguityDiagnostics {
    pub ambiguous_assignment_count: usize,
    pub weak_assignment_count: usize,
    pub ambiguity_class_counts: Vec<AmbiguityClassCount>,
    pub representative_cases: Vec<AmbiguousAssignmentRecord>,
    pub high_candidate_load_cases: Vec<HighCandidateLoadCase>,
    pub owner_suppression_verification: OwnerSuppressionVerification,
    pub responsibility_equivalence: ResponsibilityEquivalenceDiagnostics,
    pub responsibility_scope: ResponsibilityScopeDiagnostics,
    pub scope_classification_decomposition: ScopeClassificationDecompositionDiagnostics,
    pub scope_role: ScopeRoleDiagnostics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AmbiguityClassCount {
    pub ambiguity_class: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AmbiguousAssignmentRecord {
    pub capability_key: String,
    pub ambiguity_class: String,
    pub margin: f64,
    pub assignment_reason: String,
    pub top1: AnchorAssignmentSide,
    pub top2: Option<AnchorAssignmentSide>,
    pub responsibility_equivalence_class: String,
    pub responsibility_scope_ambiguity_class: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorAssignmentSide {
    pub hypothesis_id: String,
    pub family_id: String,
    pub root_concept: String,
    pub symbolic_affinity_score: f64,
    pub signed_anchor_score: f64,
    pub concept_role: String,
    pub retrieval_fanout_capabilities: usize,
    pub support_capability_count: usize,
    pub genericness: f64,
    pub transportness: f64,
    pub scope_class: String,
    pub component_scores: AffinityComponentScores,
    pub retrieval_channels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HighCandidateLoadCase {
    pub capability_key: String,
    pub retrieved_candidate_count: usize,
    pub dominant_retrieval_channels: Vec<String>,
    pub evidence_sources: Vec<String>,
    pub entrypoint_ids: Vec<String>,
    pub contract_paths: Vec<String>,
    pub owner_classes: Vec<String>,
    pub module_paths: Vec<String>,
    pub package_paths: Vec<String>,
    pub matched_seed_concepts: Vec<String>,
    pub upstream_diagnosis: String,
}

pub fn build_assignment_ambiguity_diagnostics(
    edges: &[AnchorCapabilityEdge],
    assignments: &[CapabilityAnchorAssignment],
    families: &[RankedConceptFamily],
    capability_seeds: &[CapabilityDomainSeeds],
    hypothesis_contexts: &[HypothesisContext],
    raw_pairs: &[super::domain_seed_retrieval_ablation::RawRetrievalPair],
    concept_hierarchy: &ConceptHierarchyDiagnostics,
    primitive_inventory: &super::domain_seed_provenance::PrimitiveRelationInventory,
) -> AssignmentAmbiguityDiagnostics {
    let family_by_root = families
        .iter()
        .map(|family| (family.root_concept.as_str(), family))
        .collect::<BTreeMap<_, _>>();
    let edge_by_pair = edges
        .iter()
        .map(|edge| {
            (
                (
                    edge.capability_key.as_str(),
                    edge.hypothesis_id.as_str(),
                ),
                edge,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let fanout_by_hypothesis = hypothesis_fanout(edges);
    let signed_score_by_hypothesis = hypothesis_contexts
        .iter()
        .map(|context| (context.hypothesis_id.as_str(), context.max_signed_anchor_score))
        .collect::<BTreeMap<_, _>>();
    let support_by_hypothesis = hypothesis_contexts
        .iter()
        .map(|context| {
            (
                context.hypothesis_id.as_str(),
                context.merged_support.capability_keys.len(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let context_by_id = hypothesis_contexts
        .iter()
        .map(|context| (context.hypothesis_id.as_str(), context))
        .collect::<BTreeMap<_, _>>();

    let mut class_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut representative_cases = Vec::new();
    let mut weak_assignment_count = 0usize;
    let mut ambiguous_pair_inputs = Vec::new();
    let mut ambiguous_scope_inputs = Vec::new();

    let responsibility_scope_by_hypothesis = hypothesis_contexts
        .iter()
        .filter(|context| context.domain_anchor_eligible)
        .map(|context| {
            let fanout = fanout_by_hypothesis
                .get(&context.hypothesis_id)
                .copied()
                .unwrap_or(0);
            let record = super::domain_seed_responsibility_scope::diagnose_anchor_scope(
                context,
                capability_seeds.len(),
                fanout,
            );
            (context.hypothesis_id.clone(), record.scope_class)
        })
        .collect::<BTreeMap<_, _>>();

    for assignment in assignments {
        if assignment.assignment_state == "weak" {
            weak_assignment_count += 1;
        }
        if assignment.assignment_state != "ambiguous" {
            continue;
        }
        let top1 = assignment.top_candidates.first();
        let top2 = assignment.top_candidates.get(1);
        let (Some(top1_candidate), Some(top1_edge)) = (
            top1,
            top1.and_then(|candidate| {
                edge_by_pair.get(&(
                    assignment.capability_key.as_str(),
                    candidate.hypothesis_id.as_str(),
                ))
            }),
        ) else {
            continue;
        };
        let top1_side = build_side(
            top1_edge,
            top1_candidate.symbolic_affinity_score,
            &family_by_root,
            &fanout_by_hypothesis,
            &signed_score_by_hypothesis,
            &support_by_hypothesis,
            &responsibility_scope_by_hypothesis,
        );
        let top2_side = top2.and_then(|candidate| {
            edge_by_pair
                .get(&(
                    assignment.capability_key.as_str(),
                    candidate.hypothesis_id.as_str(),
                ))
                .map(|edge| {
                    build_side(
                        edge,
                        candidate.symbolic_affinity_score,
                        &family_by_root,
                        &fanout_by_hypothesis,
                        &signed_score_by_hypothesis,
                        &support_by_hypothesis,
                        &responsibility_scope_by_hypothesis,
                    )
                })
        });
        let ambiguity_class = classify_ambiguity(&top1_side, top2_side.as_ref(), concept_hierarchy);
        let responsibility_equivalence_class = top2_side.as_ref().map(|top2| {
            let top1_context = context_by_id
                .get(top1_side.hypothesis_id.as_str())
                .copied()
                .expect("top1 hypothesis context");
            let top2_context = context_by_id
                .get(top2.hypothesis_id.as_str())
                .copied()
                .expect("top2 hypothesis context");
            super::domain_seed_responsibility_equivalence::classify_anchor_pair(
                top1_context,
                top2_context,
            )
            .equivalence_class
        }).unwrap_or_else(|| super::domain_seed_responsibility_equivalence::EQUIVALENCE_CLASS_UNKNOWN.into());
        let responsibility_scope_ambiguity_class = top2_side.as_ref().map(|top2| {
            super::domain_seed_responsibility_scope::classify_scope_ambiguity(
                top1_side.scope_class.as_str(),
                top2.scope_class.as_str(),
            )
        }).unwrap_or_else(|| super::domain_seed_responsibility_scope::AMBIGUITY_SCOPE_UNKNOWN.into());
        if let Some(top2) = top2_side.as_ref() {
            ambiguous_pair_inputs.push((
                assignment.capability_key.clone(),
                ambiguity_class.clone(),
                top1_side.hypothesis_id.clone(),
                assignment.margin,
                top2.hypothesis_id.clone(),
                top2.root_concept.clone(),
            ));
            ambiguous_scope_inputs.push((
                assignment.capability_key.clone(),
                assignment.margin,
                top1_side.hypothesis_id.clone(),
                top1_side.root_concept.clone(),
                top2.hypothesis_id.clone(),
                top2.root_concept.clone(),
            ));
        }
        *class_counts.entry(ambiguity_class.clone()).or_default() += 1;
        representative_cases.push(AmbiguousAssignmentRecord {
            capability_key: assignment.capability_key.clone(),
            ambiguity_class,
            margin: assignment.margin,
            assignment_reason: assignment.assignment_reason.clone(),
            top1: top1_side,
            top2: top2_side,
            responsibility_equivalence_class,
            responsibility_scope_ambiguity_class,
        });
    }

    representative_cases.sort_by(|left, right| {
        left.margin
            .partial_cmp(&right.margin)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.capability_key.cmp(&right.capability_key))
    });

    let mut ambiguity_class_counts = class_counts
        .into_iter()
        .map(|(ambiguity_class, count)| AmbiguityClassCount {
            ambiguity_class,
            count,
        })
        .collect::<Vec<_>>();
    ambiguity_class_counts.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.ambiguity_class.cmp(&right.ambiguity_class))
    });

    let representative_cases = select_representative_cases(representative_cases);
    let high_candidate_load_cases = build_high_candidate_load_cases(
        assignments,
        capability_seeds,
        edges,
        5,
    );
    let owner_suppression_verification = build_owner_suppression_verification(raw_pairs, edges);
    let responsibility_equivalence =
        super::domain_seed_responsibility_equivalence::build_responsibility_equivalence_diagnostics(
            hypothesis_contexts,
            &ambiguous_pair_inputs,
        );
    let responsibility_scope =
        super::domain_seed_responsibility_scope::build_responsibility_scope_diagnostics(
            hypothesis_contexts,
            edges,
            capability_seeds.len(),
            &ambiguous_scope_inputs,
        );
    let scope_classification_decomposition =
        super::domain_seed_scope_classification_decomposition::build_scope_classification_decomposition(
            hypothesis_contexts,
            edges,
            capability_seeds,
            capability_seeds.len(),
            primitive_inventory,
        );
    let scope_role = super::domain_seed_scope_role_diagnostics::build_scope_role_diagnostics(
        hypothesis_contexts,
        edges,
        capability_seeds,
        &scope_classification_decomposition,
        primitive_inventory,
    );

    AssignmentAmbiguityDiagnostics {
        ambiguous_assignment_count: assignments
            .iter()
            .filter(|assignment| assignment.assignment_state == "ambiguous")
            .count(),
        weak_assignment_count,
        ambiguity_class_counts,
        representative_cases,
        high_candidate_load_cases,
        owner_suppression_verification,
        responsibility_equivalence,
        responsibility_scope,
        scope_classification_decomposition,
        scope_role,
    }
}

fn build_side(
    edge: &AnchorCapabilityEdge,
    symbolic_affinity_score: f64,
    family_by_root: &BTreeMap<&str, &RankedConceptFamily>,
    fanout_by_hypothesis: &BTreeMap<String, usize>,
    signed_score_by_hypothesis: &BTreeMap<&str, f64>,
    support_by_hypothesis: &BTreeMap<&str, usize>,
    scope_class_by_hypothesis: &BTreeMap<String, String>,
) -> AnchorAssignmentSide {
    let family = family_by_root.get(edge.representative_root_concept.as_str());
    AnchorAssignmentSide {
        hypothesis_id: edge.hypothesis_id.clone(),
        family_id: edge.representative_family_id.clone(),
        root_concept: edge.representative_root_concept.clone(),
        symbolic_affinity_score,
        signed_anchor_score: signed_score_by_hypothesis
            .get(edge.hypothesis_id.as_str())
            .copied()
            .unwrap_or(0.0),
        concept_role: family
            .map(|family| family.concept_role.role_class.clone())
            .unwrap_or_else(|| "unknown".into()),
        retrieval_fanout_capabilities: fanout_by_hypothesis
            .get(&edge.hypothesis_id)
            .copied()
            .unwrap_or(0),
        support_capability_count: support_by_hypothesis
            .get(edge.hypothesis_id.as_str())
            .copied()
            .unwrap_or(0),
        genericness: family.map(|family| family.genericness).unwrap_or(0.0),
        transportness: family.map(|family| family.transportness).unwrap_or(0.0),
        scope_class: scope_class_by_hypothesis
            .get(&edge.hypothesis_id)
            .cloned()
            .unwrap_or_else(|| super::domain_seed_responsibility_scope::SCOPE_CLASS_MIXED.into()),
        component_scores: edge.component_scores.clone(),
        retrieval_channels: edge.retrieval_channels.clone(),
    }
}

fn classify_ambiguity(
    top1: &AnchorAssignmentSide,
    top2: Option<&AnchorAssignmentSide>,
    concept_hierarchy: &ConceptHierarchyDiagnostics,
) -> String {
    let Some(top2) = top2 else {
        return "single-candidate-ambiguous".into();
    };
    if top1.concept_role == "actionCrossCutting" || top2.concept_role == "actionCrossCutting" {
        return "action-vs-business".into();
    }
    if is_parent_subconcept(top1, top2, concept_hierarchy) {
        return "parent-vs-subconcept".into();
    }
    if is_generic_or_structural(top1) || is_generic_or_structural(top2) {
        return "business-vs-generic/structural".into();
    }
    "business-vs-business".into()
}

fn is_parent_subconcept(
    left: &AnchorAssignmentSide,
    right: &AnchorAssignmentSide,
    concept_hierarchy: &ConceptHierarchyDiagnostics,
) -> bool {
    super::domain_seed_concept_hierarchy::is_hierarchy_parent_subconcept_pair(
        &left.hypothesis_id,
        &right.hypothesis_id,
        concept_hierarchy,
    )
}

fn is_generic_or_structural(side: &AnchorAssignmentSide) -> bool {
    side.genericness >= GENERICNESS_THRESHOLD
        || side.transportness >= GENERICNESS_THRESHOLD
        || (side.concept_role == "ambiguous"
            && side.signed_anchor_score > 0.0
            && side.support_capability_count > side.retrieval_fanout_capabilities.max(1))
}

fn select_representative_cases(
    cases: Vec<AmbiguousAssignmentRecord>,
) -> Vec<AmbiguousAssignmentRecord> {
    let mut selected = Vec::new();
    let mut seen_classes = BTreeSet::new();
    for case in &cases {
        if seen_classes.insert(case.ambiguity_class.clone()) {
            selected.push(case.clone());
        }
        if selected.len() >= 12 {
            break;
        }
    }
    if selected.len() < 12 {
        for case in cases.into_iter().take(12) {
            if selected.iter().any(|existing| {
                existing.capability_key == case.capability_key
                    && existing.ambiguity_class == case.ambiguity_class
            }) {
                continue;
            }
            selected.push(case);
            if selected.len() >= 12 {
                break;
            }
        }
    }
    selected
}

fn build_high_candidate_load_cases(
    assignments: &[CapabilityAnchorAssignment],
    capability_seeds: &[CapabilityDomainSeeds],
    edges: &[AnchorCapabilityEdge],
    limit: usize,
) -> Vec<HighCandidateLoadCase> {
    let seed_by_key = capability_seeds
        .iter()
        .map(|seed| (seed.capability_key.as_str(), seed))
        .collect::<BTreeMap<_, _>>();
    let mut cases = assignments
        .iter()
        .filter(|assignment| assignment.retrieved_candidate_count > 0)
        .map(|assignment| {
            let seed = seed_by_key.get(assignment.capability_key.as_str());
            let capability_edges: Vec<_> = edges
                .iter()
                .filter(|edge| edge.capability_key == assignment.capability_key)
                .collect();
            let dominant_channels = dominant_channels(
                &capability_edges
                    .iter()
                    .flat_map(|edge| edge.retrieval_channels.iter().cloned())
                    .collect::<Vec<_>>(),
            );
            let evidence_sources = seed
                .map(|seed| {
                    seed.candidates
                        .iter()
                        .map(|candidate| candidate.evidence_source.clone())
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let matched_seed_concepts = seed
                .map(|seed| {
                    seed.candidates
                        .iter()
                        .map(|candidate| candidate.concept.clone())
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .take(8)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let upstream_diagnosis = diagnose_high_candidate_load(
                assignment,
                seed.copied(),
                &dominant_channels,
                capability_edges.len(),
            );
            HighCandidateLoadCase {
                capability_key: assignment.capability_key.clone(),
                retrieved_candidate_count: assignment.retrieved_candidate_count,
                dominant_retrieval_channels: dominant_channels,
                evidence_sources,
                entrypoint_ids: seed
                    .map(|seed| seed.coverage.entrypoint_ids.clone())
                    .unwrap_or_default(),
                contract_paths: seed
                    .map(|seed| seed.coverage.contract_paths.clone())
                    .unwrap_or_default(),
                owner_classes: seed
                    .map(|seed| seed.coverage.owner_classes.clone())
                    .unwrap_or_default(),
                module_paths: seed
                    .map(|seed| seed.coverage.module_paths.clone())
                    .unwrap_or_default(),
                package_paths: seed
                    .map(|seed| seed.coverage.package_paths.clone())
                    .unwrap_or_default(),
                matched_seed_concepts,
                upstream_diagnosis,
            }
        })
        .collect::<Vec<_>>();
    cases.sort_by(|left, right| {
        right
            .retrieved_candidate_count
            .cmp(&left.retrieved_candidate_count)
            .then_with(|| left.capability_key.cmp(&right.capability_key))
    });
    cases.truncate(limit);
    cases
}

fn diagnose_high_candidate_load(
    assignment: &CapabilityAnchorAssignment,
    seed: Option<&CapabilityDomainSeeds>,
    dominant_channels: &[String],
    edge_count: usize,
) -> String {
    let Some(seed) = seed else {
        return "capability seed evidence unavailable".into();
    };
    let mut reasons = Vec::new();
    if seed.coverage.entrypoint_ids.is_empty() {
        reasons.push("no entrypoint ids");
    } else if seed.coverage.entrypoint_ids.len() == 1 {
        reasons.push("single broad entrypoint");
    }
    if seed.coverage.contract_paths.is_empty() {
        reasons.push("no contract namespace");
    }
    if seed.candidates.is_empty() {
        reasons.push("no seed candidates");
    } else if seed.candidates.len() == 1 {
        reasons.push("single generic seed candidate");
    }
    if !dominant_channels.is_empty() {
        reasons.push("dominant retrieval channels");
    }
    if assignment.retrieved_candidate_count > 50 {
        reasons.push("near-universal eligible anchor fanout");
    }
    if reasons.is_empty() {
        format!(
            "retrieved {} hypotheses across {} edges via {:?}",
            assignment.retrieved_candidate_count, edge_count, dominant_channels
        )
    } else {
        format!(
            "retrieved {} hypotheses: {}",
            assignment.retrieved_candidate_count,
            reasons.join(", ")
        )
    }
}

fn hypothesis_fanout(edges: &[AnchorCapabilityEdge]) -> BTreeMap<String, usize> {
    let mut per_hypothesis: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for edge in edges {
        per_hypothesis
            .entry(edge.hypothesis_id.clone())
            .or_default()
            .insert(edge.capability_key.clone());
    }
    per_hypothesis
        .into_iter()
        .map(|(hypothesis_id, capabilities)| (hypothesis_id, capabilities.len()))
        .collect()
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

pub fn build_owner_suppression_verification(
    raw_pairs: &[super::domain_seed_retrieval_ablation::RawRetrievalPair],
    edges: &[AnchorCapabilityEdge],
) -> OwnerSuppressionVerification {
    let mut owner_exclusive_pairs_before_gate = 0usize;
    let mut suppressed_owner_only_pure = 0usize;
    let mut suppressed_owner_with_weak_independent = 0usize;
    let mut suppressed_owner_only_edges = 0usize;

    for pair in raw_pairs {
        if !pair.channels.contains("owner") {
            continue;
        }
        let owner_exclusive = pair.channels.len() == 1 && pair.channels.contains("owner");
        if owner_exclusive {
            owner_exclusive_pairs_before_gate += 1;
        }
        if is_suppressed_owner_only_retrieval(&pair.channels) {
            suppressed_owner_only_edges += 1;
            if owner_exclusive {
                suppressed_owner_only_pure += 1;
            } else {
                suppressed_owner_with_weak_independent += 1;
            }
        }
    }

    let retained_owner_reinforced_edges = edges
        .iter()
        .filter(|edge| edge.retrieval_channels.iter().any(|channel| channel == "owner"))
        .count();

    OwnerSuppressionVerification {
        suppressed_owner_only_edges,
        owner_exclusive_pairs_before_gate,
        suppressed_owner_only_pure,
        suppressed_owner_with_weak_independent,
        retained_owner_reinforced_edges,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::domain_seed_concept_hierarchy::{
        ConceptHierarchyDiagnostics, ParentSubconceptEdge,
    };
    use super::super::domain_seed_retrieval_ablation::RawRetrievalPair;

    fn side(root: &str, role: &str, genericness: f64) -> AnchorAssignmentSide {
        AnchorAssignmentSide {
            hypothesis_id: format!("hypothesis:{root}"),
            family_id: format!("family:{root}"),
            root_concept: root.into(),
            symbolic_affinity_score: 2.0,
            signed_anchor_score: 2.5,
            concept_role: role.into(),
            retrieval_fanout_capabilities: 10,
            support_capability_count: 5,
            genericness,
            transportness: 0.0,
            scope_class: super::super::domain_seed_responsibility_scope::SCOPE_CLASS_MIXED.into(),
            component_scores: AffinityComponentScores {
                lexical_alignment: 0.0,
                owner_alignment: 0.0,
                module_package_alignment: 0.0,
                entity_resource_alignment: 0.0,
                behavior_alignment: 1.0,
                contract_alignment: 0.0,
            },
            retrieval_channels: vec!["behaviorCall".into()],
        }
    }

    #[test]
    fn action_cross_cutting은_action_vs_business로_분류된다() {
        let top1 = side("payment", "anchor", 0.1);
        let top2 = side("process", "actionCrossCutting", 0.1);
        assert_eq!(
            classify_ambiguity(&top1, Some(&top2), &ConceptHierarchyDiagnostics::default()),
            "action-vs-business"
        );
    }

    #[test]
    fn parent_subconcept는_hierarchy_edge로_분류된다() {
        let top1 = side("order", "anchor", 0.1);
        let top2 = side("draft", "anchor", 0.1);
        let hierarchy = ConceptHierarchyDiagnostics {
            parent_subconcept_edge_count: 1,
            parent_subconcept_edges: vec![ParentSubconceptEdge {
                parent_hypothesis_id: top1.hypothesis_id.clone(),
                parent_root_concept: top1.root_concept.clone(),
                child_hypothesis_id: top2.hypothesis_id.clone(),
                child_root_concept: top2.root_concept.clone(),
                confidence: 0.9,
                signals: vec!["supportContainment".into(), "conceptComposition".into()],
            }],
            ..Default::default()
        };
        assert_eq!(
            classify_ambiguity(&top1, Some(&top2), &hierarchy),
            "parent-vs-subconcept"
        );
    }

    #[test]
    fn owner_suppression_검증은_pure와_weak를_분리한다() {
        let pairs = vec![
            RawRetrievalPair {
                hypothesis_id: "h1".into(),
                capability_key: "cap".into(),
                channels: BTreeSet::from(["owner".to_string()]),
                weak_lexical: false,
                weak_generic_module_package: false,
                weak_generic_owner_role: false,
                behavior_only: false,
            },
            RawRetrievalPair {
                hypothesis_id: "h2".into(),
                capability_key: "cap".into(),
                channels: BTreeSet::from(["owner".to_string(), "lexical".to_string()]),
                weak_lexical: true,
                weak_generic_module_package: false,
                weak_generic_owner_role: false,
                behavior_only: false,
            },
        ];
        let verification = build_owner_suppression_verification(&pairs, &[]);
        assert_eq!(verification.owner_exclusive_pairs_before_gate, 1);
        assert_eq!(verification.suppressed_owner_only_pure, 1);
        assert_eq!(verification.suppressed_owner_with_weak_independent, 1);
        assert_eq!(verification.suppressed_owner_only_edges, 2);
    }
}
