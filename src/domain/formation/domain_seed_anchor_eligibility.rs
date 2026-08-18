//! Domain anchor eligibility diagnostics (distinct from conceptRole=anchor).

use super::domain_seed_aggregation::RankedConceptFamily;
use super::domain_seed_diagnostics::CapabilityDomainSeeds;
use super::domain_seed_provenance::{
    PrimitiveRelationInventory, ProvenanceSeedCandidateGraph, SeedHypothesisGroup,
};
use super::domain_seed_role_graph::family_id;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const HIGH_SIGNED_AMBIGUOUS_ANCHOR_THRESHOLD: f64 = 2.0;
const SUPPORT_CONTAINMENT_THRESHOLD: f64 = 0.85;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DomainAnchorEligibilityDiagnostics {
    pub diagnostic_anchor_count: usize,
    pub hypothesis_node_count: usize,
    pub eligible_domain_anchor_count: usize,
    pub subconcept_candidate_count: usize,
    pub redundant_anchor_candidate_count: usize,
    pub evidence_coverage_gaps: Vec<String>,
    pub concept_hierarchy: super::domain_seed_concept_hierarchy::ConceptHierarchyDiagnostics,
    pub duplicate_diagnostic_family_groups: Vec<DuplicateDiagnosticFamilyGroup>,
    pub hypotheses: Vec<HypothesisEligibilityRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateDiagnosticFamilyInclusion {
    pub family_id: String,
    pub root_concept: String,
    pub inclusion_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateDiagnosticFamilyGroup {
    pub hypothesis_id: String,
    pub group_id: String,
    pub inclusions: Vec<DuplicateDiagnosticFamilyInclusion>,
    pub diagnostic_family_ids: Vec<String>,
    pub diagnostic_root_concepts: Vec<String>,
    pub diagnostic_inclusion_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HypothesisEligibilityRecord {
    pub hypothesis_id: String,
    pub group_id: String,
    pub representative_root_concept: String,
    pub alternative_root_concepts: Vec<String>,
    pub representative_selection_reason: String,
    pub representative_family_id: String,
    pub competing_family_ids: Vec<String>,
    pub concept_role_anchor_family_count: usize,
    pub domain_anchor_eligible: bool,
    pub eligibility_class: String,
    pub signed_anchor_score: f64,
    pub distinct_capability_count: usize,
    pub distinct_entrypoint_count: usize,
    pub owner_evidence_count: usize,
    pub entity_module_evidence_count: usize,
    pub support_containment_score: f64,
    pub contained_in_hypothesis_id: Option<String>,
    pub has_independent_ownership_state_behavior: bool,
    pub preserves_multi_entrypoint_owner_operations: bool,
    pub diagnostic_family_inclusions: Vec<DiagnosticFamilyInclusion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticFamilyInclusion {
    pub family_id: String,
    pub root_concept: String,
    pub concept_role: String,
    pub inclusion_reason: String,
}

#[derive(Debug, Clone)]
pub struct HypothesisContext {
    pub group: SeedHypothesisGroup,
    pub hypothesis_id: String,
    pub families: Vec<RankedConceptFamily>,
    pub representative: RankedConceptFamily,
    pub representative_selection_reason: String,
    pub diagnostic_inclusions: Vec<DiagnosticFamilyInclusion>,
    pub merged_support: MergedHypothesisSupport,
    pub max_signed_anchor_score: f64,
    pub eligibility_class: String,
    pub domain_anchor_eligible: bool,
    pub support_containment_score: f64,
    pub contained_in_hypothesis_id: Option<String>,
    pub has_independent_ownership_state_behavior: bool,
    pub preserves_multi_entrypoint_owner_operations: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MergedHypothesisSupport {
    pub capability_keys: BTreeSet<String>,
    pub entrypoint_ids: BTreeSet<String>,
    pub owner_classes: BTreeSet<String>,
    pub unit_ids: BTreeSet<String>,
    pub module_paths: BTreeSet<String>,
    pub resource_entities: BTreeSet<String>,
    pub flow_ids: BTreeSet<String>,
    pub root_concepts: BTreeSet<String>,
}

pub fn build_domain_anchor_eligibility(
    families: &[RankedConceptFamily],
    capability_seeds: &[CapabilityDomainSeeds],
    provenance_graph: &ProvenanceSeedCandidateGraph,
) -> (DomainAnchorEligibilityDiagnostics, Vec<HypothesisContext>) {
    let contexts = build_hypothesis_contexts(families, capability_seeds, provenance_graph);
    let classified = classify_hypothesis_eligibility(&contexts, capability_seeds);
    let concept_hierarchy =
        super::domain_seed_concept_hierarchy::infer_concept_hierarchy(&classified, provenance_graph);
    let evidence_coverage_gaps =
        detect_evidence_coverage_gaps(&provenance_graph.primitive_relation_inventory);

    let diagnostic_anchor_count = classified
        .iter()
        .map(|context| context.diagnostic_inclusions.len())
        .sum();
    let hypothesis_node_count = classified.len();
    let eligible_domain_anchor_count = classified
        .iter()
        .filter(|context| context.domain_anchor_eligible)
        .count();
    let subconcept_candidate_count = classified
        .iter()
        .filter(|context| context.eligibility_class == "subconceptCandidate")
        .count();
    let redundant_anchor_candidate_count = classified
        .iter()
        .filter(|context| context.eligibility_class == "redundantAnchorCandidate")
        .count();

    let duplicate_diagnostic_family_groups = classified
        .iter()
        .filter(|context| context.diagnostic_inclusions.len() > 1)
        .map(|context| {
            let inclusions = context
                .diagnostic_inclusions
                .iter()
                .map(|inclusion| DuplicateDiagnosticFamilyInclusion {
                    family_id: inclusion.family_id.clone(),
                    root_concept: inclusion.root_concept.clone(),
                    inclusion_reason: inclusion.inclusion_reason.clone(),
                })
                .collect::<Vec<_>>();
            DuplicateDiagnosticFamilyGroup {
                hypothesis_id: context.hypothesis_id.clone(),
                group_id: context.group.group_id.clone(),
                diagnostic_family_ids: inclusions
                    .iter()
                    .map(|inclusion| inclusion.family_id.clone())
                    .collect(),
                diagnostic_root_concepts: inclusions
                    .iter()
                    .map(|inclusion| inclusion.root_concept.clone())
                    .collect(),
                diagnostic_inclusion_reasons: inclusions
                    .iter()
                    .map(|inclusion| inclusion.inclusion_reason.clone())
                    .collect(),
                inclusions,
            }
        })
        .collect();

    let hypotheses = classified
        .iter()
        .map(|context| HypothesisEligibilityRecord {
            hypothesis_id: context.hypothesis_id.clone(),
            group_id: context.group.group_id.clone(),
            representative_root_concept: context.representative.root_concept.clone(),
            alternative_root_concepts: alternative_roots(context),
            representative_selection_reason: context.representative_selection_reason.clone(),
            representative_family_id: family_id(&context.representative),
            competing_family_ids: context.group.competing_family_ids.clone(),
            concept_role_anchor_family_count: context
                .families
                .iter()
                .filter(|family| family.concept_role.role_class == "anchor")
                .count(),
            domain_anchor_eligible: context.domain_anchor_eligible,
            eligibility_class: context.eligibility_class.clone(),
            signed_anchor_score: context.max_signed_anchor_score,
            distinct_capability_count: context.merged_support.capability_keys.len(),
            distinct_entrypoint_count: context.merged_support.entrypoint_ids.len(),
            owner_evidence_count: context.merged_support.owner_classes.len(),
            entity_module_evidence_count: context.merged_support.module_paths.len()
                + context.merged_support.resource_entities.len(),
            support_containment_score: context.support_containment_score,
            contained_in_hypothesis_id: context.contained_in_hypothesis_id.clone(),
            has_independent_ownership_state_behavior: context
                .has_independent_ownership_state_behavior,
            preserves_multi_entrypoint_owner_operations: context
                .preserves_multi_entrypoint_owner_operations,
            diagnostic_family_inclusions: context.diagnostic_inclusions.clone(),
        })
        .collect();

    let diagnostics = DomainAnchorEligibilityDiagnostics {
        diagnostic_anchor_count,
        hypothesis_node_count,
        eligible_domain_anchor_count,
        subconcept_candidate_count,
        redundant_anchor_candidate_count,
        evidence_coverage_gaps,
        concept_hierarchy,
        duplicate_diagnostic_family_groups,
        hypotheses,
    };
    (diagnostics, classified)
}

fn build_hypothesis_contexts(
    families: &[RankedConceptFamily],
    capability_seeds: &[CapabilityDomainSeeds],
    provenance_graph: &ProvenanceSeedCandidateGraph,
) -> Vec<HypothesisContext> {
    let family_map = families
        .iter()
        .map(|family| (family_id(family), family.clone()))
        .collect::<BTreeMap<_, _>>();

    provenance_graph
        .seed_hypothesis_groups
        .iter()
        .filter_map(|group| {
            let group_families = group
                .competing_family_ids
                .iter()
                .filter_map(|family_key| family_map.get(family_key).cloned())
                .filter(|family| family.concept_role.role_class != "actionCrossCutting")
                .collect::<Vec<_>>();
            if group_families.is_empty() {
                return None;
            }
            let diagnostic_inclusions = group_families
                .iter()
                .filter_map(diagnostic_inclusion_reason)
                .map(|(family, reason)| DiagnosticFamilyInclusion {
                    family_id: family_id(family),
                    root_concept: family.root_concept.clone(),
                    concept_role: family.concept_role.role_class.clone(),
                    inclusion_reason: reason.into(),
                })
                .collect::<Vec<_>>();
            let (representative, representative_selection_reason) =
                select_representative_family(&group_families);
            let merged_support = merge_support(&group_families);
            let max_signed_anchor_score = group_families
                .iter()
                .map(|family| family.anchor_score_components.symbolic_total)
                .fold(0.0_f64, f64::max);
            Some(HypothesisContext {
                hypothesis_id: hypothesis_id(&group.group_id),
                group: group.clone(),
                families: group_families,
                representative,
                representative_selection_reason,
                diagnostic_inclusions,
                merged_support,
                max_signed_anchor_score,
                eligibility_class: "pending".into(),
                domain_anchor_eligible: false,
                support_containment_score: 0.0,
                contained_in_hypothesis_id: None,
                has_independent_ownership_state_behavior: false,
                preserves_multi_entrypoint_owner_operations: false,
            })
        })
        .chain(solo_hypothesis_fallbacks(
            families,
            capability_seeds,
            provenance_graph,
        ))
        .collect()
}

fn solo_hypothesis_fallbacks<'a>(
    families: &'a [RankedConceptFamily],
    capability_seeds: &'a [CapabilityDomainSeeds],
    provenance_graph: &'a ProvenanceSeedCandidateGraph,
) -> impl Iterator<Item = HypothesisContext> + 'a {
    let grouped: BTreeSet<_> = provenance_graph
        .seed_hypothesis_groups
        .iter()
        .flat_map(|group| group.competing_family_ids.iter().cloned())
        .collect();
    families.iter().filter_map(move |family| {
        if family.concept_role.role_class == "actionCrossCutting" {
            return None;
        }
        let key = family_id(family);
        if grouped.contains(&key) {
            return None;
        }
        let signature = if family.support_signature.signature_key.is_empty() {
            super::domain_seed_provenance::build_support_signature(family, capability_seeds)
        } else {
            family.support_signature.clone()
        };
        let group = SeedHypothesisGroup {
            group_id: format!("solo:{}", family.root_concept),
            signature_key: signature.signature_key.clone(),
            support_signature: signature,
            competing_family_ids: vec![key.clone()],
            competing_root_concepts: vec![family.root_concept.clone()],
            near_identical_groups: Vec::new(),
        };
        let diagnostic_inclusions = diagnostic_inclusion_reason(family)
            .map(|(_, reason)| DiagnosticFamilyInclusion {
                family_id: key.clone(),
                root_concept: family.root_concept.clone(),
                concept_role: family.concept_role.role_class.clone(),
                inclusion_reason: reason.into(),
            })
            .into_iter()
            .collect::<Vec<_>>();
        let (representative, representative_selection_reason) =
            select_representative_family(&[family.clone()]);
        let merged_support = merge_support(&[family.clone()]);
        Some(HypothesisContext {
            hypothesis_id: hypothesis_id(&group.group_id),
            group,
            families: vec![family.clone()],
            representative,
            representative_selection_reason,
            diagnostic_inclusions,
            merged_support,
            max_signed_anchor_score: family.anchor_score_components.symbolic_total,
            eligibility_class: "pending".into(),
            domain_anchor_eligible: false,
            support_containment_score: 0.0,
            contained_in_hypothesis_id: None,
            has_independent_ownership_state_behavior: false,
            preserves_multi_entrypoint_owner_operations: false,
        })
    })
}

fn classify_hypothesis_eligibility(
    contexts: &[HypothesisContext],
    capability_seeds: &[CapabilityDomainSeeds],
) -> Vec<HypothesisContext> {
    contexts
        .iter()
        .cloned()
        .map(|mut context| {
            context.preserves_multi_entrypoint_owner_operations =
                preserves_multi_entrypoint_owner_operations(&context, capability_seeds);
            context.has_independent_ownership_state_behavior =
                has_independent_ownership_state_behavior(&context);

            if context.diagnostic_inclusions.is_empty() {
                context.eligibility_class = "notDiagnostic".into();
                context.domain_anchor_eligible = false;
                return context;
            }

            if context.preserves_multi_entrypoint_owner_operations {
                context.eligibility_class = "eligibleDomainAnchor".into();
                context.domain_anchor_eligible = true;
                return context;
            }

            let mut best_containment = 0.0_f64;
            let mut container_id = None;
            for stronger in contexts {
                if stronger.hypothesis_id == context.hypothesis_id {
                    continue;
                }
                if stronger.max_signed_anchor_score <= context.max_signed_anchor_score {
                    continue;
                }
                let containment = support_containment_ratio(
                    &context.merged_support,
                    &stronger.merged_support,
                );
                if containment > best_containment {
                    best_containment = containment;
                    container_id = Some(stronger.hypothesis_id.clone());
                }
            }
            context.support_containment_score = best_containment;
            context.contained_in_hypothesis_id = container_id.clone();

            if best_containment >= SUPPORT_CONTAINMENT_THRESHOLD {
                if context.has_independent_ownership_state_behavior {
                    context.eligibility_class = "eligibleDomainAnchor".into();
                    context.domain_anchor_eligible = true;
                    return context;
                }
                if is_subconcept_candidate(&context, contexts) {
                    context.eligibility_class = "subconceptCandidate".into();
                } else {
                    context.eligibility_class = "redundantAnchorCandidate".into();
                }
                context.domain_anchor_eligible = false;
                return context;
            }

            context.eligibility_class = "eligibleDomainAnchor".into();
            context.domain_anchor_eligible = true;
            context
        })
        .collect()
}

fn diagnostic_inclusion_reason(family: &RankedConceptFamily) -> Option<(&RankedConceptFamily, &'static str)> {
    match family.concept_role.role_class.as_str() {
        "anchor" => Some((family, "explicitAnchor")),
        "ambiguous" if family.anchor_score_components.symbolic_total
            >= HIGH_SIGNED_AMBIGUOUS_ANCHOR_THRESHOLD =>
        {
            Some((family, "highSignedAmbiguousAnchor"))
        }
        _ => None,
    }
}

fn select_representative_family(
    families: &[RankedConceptFamily],
) -> (RankedConceptFamily, String) {
    let mut ranked = families.to_vec();
    ranked.sort_by(|left, right| {
        right
            .anchor_score_components
            .symbolic_total
            .partial_cmp(&left.anchor_score_components.symbolic_total)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.rank.cmp(&right.rank))
            .then_with(|| {
                role_priority(&left.concept_role.role_class)
                    .cmp(&role_priority(&right.concept_role.role_class))
            })
            .then_with(|| left.root_concept.len().cmp(&right.root_concept.len()))
            .then_with(|| left.root_concept.cmp(&right.root_concept))
    });
    let representative = ranked
        .first()
        .cloned()
        .expect("non-empty hypothesis families");
    let reason = format!(
        "display heuristic: highest signedAnchorScore ({:.2}), then rank, conceptRole, shortest root",
        representative.anchor_score_components.symbolic_total
    );
    (representative, reason)
}

fn role_priority(role_class: &str) -> u8 {
    match role_class {
        "anchor" => 0,
        "ambiguous" => 1,
        _ => 2,
    }
}

fn merge_support(families: &[RankedConceptFamily]) -> MergedHypothesisSupport {
    let mut merged = MergedHypothesisSupport::default();
    for family in families {
        merged
            .capability_keys
            .extend(family.distinct_capability_keys.iter().cloned());
        merged
            .entrypoint_ids
            .extend(family.distinct_entrypoint_ids.iter().cloned());
        merged
            .owner_classes
            .extend(family.distinct_owner_classes.iter().cloned());
        merged
            .module_paths
            .extend(family.distinct_module_paths.iter().cloned());
        merged.root_concepts.insert(family.root_concept.clone());
        merged
            .resource_entities
            .extend(family.provenance.resource_entities.iter().cloned());
        merged
            .flow_ids
            .extend(family.provenance.flow_ids.iter().cloned());
        merged
            .unit_ids
            .extend(family.provenance.unit_ids.iter().cloned());
    }
    merged
}

fn support_containment_ratio(
    inner: &MergedHypothesisSupport,
    outer: &MergedHypothesisSupport,
) -> f64 {
    let sets = [
        (&inner.capability_keys, &outer.capability_keys),
        (&inner.entrypoint_ids, &outer.entrypoint_ids),
        (&inner.owner_classes, &outer.owner_classes),
        (&inner.unit_ids, &outer.unit_ids),
    ];
    let mut scores = Vec::new();
    for (inner_set, outer_set) in sets {
        if inner_set.is_empty() {
            continue;
        }
        let contained = inner_set.intersection(outer_set).count();
        scores.push(contained as f64 / inner_set.len() as f64);
    }
    if scores.is_empty() {
        0.0
    } else {
        scores.iter().sum::<f64>() / scores.len() as f64
    }
}

fn has_independent_ownership_state_behavior(context: &HypothesisContext) -> bool {
    !context.merged_support.owner_classes.is_empty()
        && (context.merged_support.resource_entities.len() >= 2
            || !context.merged_support.flow_ids.is_empty()
            || context.merged_support.entrypoint_ids.len() >= 2)
}

fn preserves_multi_entrypoint_owner_operations(
    context: &HypothesisContext,
    capability_seeds: &[CapabilityDomainSeeds],
) -> bool {
    if context.merged_support.capability_keys.len() != 1 {
        return false;
    }
    if context.merged_support.entrypoint_ids.len() < 2 {
        return false;
    }
    let capability_key = context
        .merged_support
        .capability_keys
        .iter()
        .next()
        .cloned()
        .unwrap_or_default();
    let Some(seed) = capability_seeds
        .iter()
        .find(|seed| seed.capability_key == capability_key)
    else {
        return false;
    };
    if seed.coverage.entrypoint_ids.len() < 2 || seed.coverage.owner_classes.len() != 1 {
        return false;
    }
    context.merged_support.owner_classes.len() <= 1
}

fn is_subconcept_candidate(
    context: &HypothesisContext,
    contexts: &[HypothesisContext],
) -> bool {
    let Some(container_id) = context.contained_in_hypothesis_id.as_deref() else {
        return false;
    };
    let Some(container) = contexts
        .iter()
        .find(|candidate| candidate.hypothesis_id == container_id)
    else {
        return false;
    };
    super::domain_seed_concept_hierarchy::concept_composition_link(context, container)
}

fn alternative_roots(context: &HypothesisContext) -> Vec<String> {
    context
        .group
        .competing_root_concepts
        .iter()
        .filter(|root| *root != &context.representative.root_concept)
        .cloned()
        .collect()
}

fn detect_evidence_coverage_gaps(inventory: &PrimitiveRelationInventory) -> Vec<String> {
    let mut gaps = Vec::new();
    if inventory.owner_relations == 0 {
        gaps.push("ownerEvidence".into());
    }
    if inventory.entity_relations == 0 {
        gaps.push("entityEvidence".into());
    }
    if inventory.resource_relations == 0 {
        gaps.push("resourceEvidence".into());
    }
    gaps
}

pub(crate) fn hypothesis_id(group_id: &str) -> String {
    format!("hypothesis:{group_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_coverage_gap은_0_primitive에서_기록된다() {
        let gaps = detect_evidence_coverage_gaps(&PrimitiveRelationInventory::default());
        assert!(gaps.contains(&"ownerEvidence".to_string()));
        assert!(gaps.contains(&"entityEvidence".to_string()));
        assert!(gaps.contains(&"resourceEvidence".to_string()));
    }

    #[test]
    fn duplicate_diagnostic_family_group는_동일_hypothesis의_복수_family를_기록한다() {
        let diagnostics = DomainAnchorEligibilityDiagnostics {
            diagnostic_anchor_count: 2,
            hypothesis_node_count: 1,
            duplicate_diagnostic_family_groups: vec![DuplicateDiagnosticFamilyGroup {
                hypothesis_id: "hypothesis:g1".into(),
                group_id: "g1".into(),
                inclusions: vec![
                    DuplicateDiagnosticFamilyInclusion {
                        family_id: "family:a".into(),
                        root_concept: "a".into(),
                        inclusion_reason: "explicitAnchor".into(),
                    },
                    DuplicateDiagnosticFamilyInclusion {
                        family_id: "family:b".into(),
                        root_concept: "b".into(),
                        inclusion_reason: "highSignedAmbiguousAnchor".into(),
                    },
                ],
                diagnostic_family_ids: vec!["family:a".into(), "family:b".into()],
                diagnostic_root_concepts: vec!["a".into(), "b".into()],
                diagnostic_inclusion_reasons: vec![
                    "explicitAnchor".into(),
                    "highSignedAmbiguousAnchor".into(),
                ],
            }],
            ..Default::default()
        };
        assert!(diagnostics.diagnostic_anchor_count > diagnostics.hypothesis_node_count);
        assert_eq!(diagnostics.duplicate_diagnostic_family_groups.len(), 1);
    }
}
