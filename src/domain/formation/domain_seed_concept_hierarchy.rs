//! Concept hierarchy inference (diagnostic + KG expression only; does not change eligibility).

use super::domain_seed_anchor_eligibility::{HypothesisContext, MergedHypothesisSupport};
use super::domain_seed_aggregation::RankedConceptFamily;
use super::domain_seed_provenance::ProvenanceSeedCandidateGraph;
use super::key_decomposition::normalized_root_concept;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

const SUPPORT_CONTAINMENT_THRESHOLD: f64 = 0.85;
const SUPPORT_JACCARD_THRESHOLD: f64 = 0.85;
const PROVENANCE_OVERLAP_THRESHOLD: f64 = 0.75;
const CONCEPT_COMPOSITION_THRESHOLD: f64 = 0.70;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConceptHierarchyDiagnostics {
    pub parent_subconcept_edge_count: usize,
    pub would_demote_count: usize,
    pub parent_subconcept_edges: Vec<ParentSubconceptEdge>,
    pub would_demote_records: Vec<HierarchyDemotionRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParentSubconceptEdge {
    pub parent_hypothesis_id: String,
    pub parent_root_concept: String,
    pub child_hypothesis_id: String,
    pub child_root_concept: String,
    pub confidence: f64,
    pub signals: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyDemotionRecord {
    pub hypothesis_id: String,
    pub root_concept: String,
    pub parent_hypothesis_id: String,
    pub parent_root_concept: String,
    pub demotion_reason: String,
    pub signals: Vec<String>,
}

#[derive(Debug, Clone)]
struct HierarchyLink {
    parent_hypothesis_id: String,
    parent_root_concept: String,
    confidence: f64,
    signals: Vec<String>,
}

pub fn infer_concept_hierarchy(
    contexts: &[HypothesisContext],
    provenance_graph: &ProvenanceSeedCandidateGraph,
) -> ConceptHierarchyDiagnostics {
    let mut edges = Vec::new();
    let mut demotions = Vec::new();

    for child in contexts {
        if child.diagnostic_inclusions.is_empty() {
            continue;
        }
        let Some(link) = best_parent_link(child, contexts, provenance_graph) else {
            continue;
        };
        edges.push(ParentSubconceptEdge {
            parent_hypothesis_id: link.parent_hypothesis_id.clone(),
            parent_root_concept: link.parent_root_concept.clone(),
            child_hypothesis_id: child.hypothesis_id.clone(),
            child_root_concept: child.representative.root_concept.clone(),
            confidence: link.confidence,
            signals: link.signals.clone(),
        });
        if should_demote_subconcept(child, &link) {
            demotions.push(HierarchyDemotionRecord {
                hypothesis_id: child.hypothesis_id.clone(),
                root_concept: child.representative.root_concept.clone(),
                parent_hypothesis_id: link.parent_hypothesis_id,
                parent_root_concept: link.parent_root_concept,
                demotion_reason: "hierarchySubconceptWithoutIndependentOwnership".into(),
                signals: link.signals,
            });
        }
    }

    ConceptHierarchyDiagnostics {
        parent_subconcept_edge_count: edges.len(),
        would_demote_count: demotions.len(),
        parent_subconcept_edges: edges,
        would_demote_records: demotions,
    }
}

pub(crate) fn concept_composition_link(
    child: &HypothesisContext,
    parent: &HypothesisContext,
) -> bool {
    concept_composition_score(child, parent) >= CONCEPT_COMPOSITION_THRESHOLD
}

pub fn is_hierarchy_parent_subconcept_pair(
    left_hypothesis_id: &str,
    right_hypothesis_id: &str,
    hierarchy: &ConceptHierarchyDiagnostics,
) -> bool {
    hierarchy.parent_subconcept_edges.iter().any(|edge| {
        (edge.parent_hypothesis_id == left_hypothesis_id
            && edge.child_hypothesis_id == right_hypothesis_id)
            || (edge.parent_hypothesis_id == right_hypothesis_id
                && edge.child_hypothesis_id == left_hypothesis_id)
    })
}

fn best_parent_link(
    child: &HypothesisContext,
    contexts: &[HypothesisContext],
    provenance_graph: &ProvenanceSeedCandidateGraph,
) -> Option<HierarchyLink> {
    let mut best: Option<HierarchyLink> = None;
    for parent in contexts {
        if parent.hypothesis_id == child.hypothesis_id || parent.diagnostic_inclusions.is_empty() {
            continue;
        }
        let (confidence, signals) = hierarchy_signals(child, parent, provenance_graph);
        if confidence < CONCEPT_COMPOSITION_THRESHOLD {
            continue;
        }
        if !parent_is_preferred_over_child(child, parent) {
            continue;
        }
        if best
            .as_ref()
            .map(|current| confidence > current.confidence)
            .unwrap_or(true)
        {
            best = Some(HierarchyLink {
                parent_hypothesis_id: parent.hypothesis_id.clone(),
                parent_root_concept: parent.representative.root_concept.clone(),
                confidence,
                signals,
            });
        }
    }
    best
}

fn should_demote_subconcept(child: &HypothesisContext, link: &HierarchyLink) -> bool {
    if child.preserves_multi_entrypoint_owner_operations {
        return false;
    }
    if child.has_independent_ownership_state_behavior {
        return false;
    }
    if independent_business_ownership_evidence(child) {
        return false;
    }
    link.confidence >= CONCEPT_COMPOSITION_THRESHOLD
        && link.signals.iter().any(|signal| {
            matches!(
                signal.as_str(),
                "supportContainment"
                    | "nearIdenticalSupport"
                    | "conceptComposition"
                    | "provenanceOverlap"
            )
        })
}

fn hierarchy_signals(
    child: &HypothesisContext,
    parent: &HypothesisContext,
    provenance_graph: &ProvenanceSeedCandidateGraph,
) -> (f64, Vec<String>) {
    let mut signals = Vec::new();
    let mut scores = Vec::new();

    let containment = support_containment_ratio(&child.merged_support, &parent.merged_support);
    if containment >= SUPPORT_CONTAINMENT_THRESHOLD {
        signals.push("supportContainment".into());
        scores.push(containment);
    }

    let jaccard = support_jaccard(&child.merged_support, &parent.merged_support);
    if jaccard >= SUPPORT_JACCARD_THRESHOLD {
        signals.push("nearIdenticalSupport".into());
        scores.push(jaccard);
    }

    let composition = concept_composition_score(child, parent);
    if composition >= CONCEPT_COMPOSITION_THRESHOLD {
        signals.push("conceptComposition".into());
        scores.push(composition);
    }

    let provenance = provenance_overlap_ratio(child, parent);
    if provenance >= PROVENANCE_OVERLAP_THRESHOLD {
        signals.push("provenanceOverlap".into());
        scores.push(provenance);
    }

    if near_identical_group_link(child, parent, provenance_graph) {
        signals.push("nearIdenticalGroup".into());
        scores.push(0.9);
    }

    if signals.len() < 2 {
        return (0.0, signals);
    }

    let confidence = scores.iter().sum::<f64>() / scores.len() as f64;
    (confidence, signals)
}

fn parent_is_preferred_over_child(child: &HypothesisContext, parent: &HypothesisContext) -> bool {
    if parent.max_signed_anchor_score > child.max_signed_anchor_score {
        return true;
    }
    if parent.max_signed_anchor_score < child.max_signed_anchor_score {
        return false;
    }
    let parent_breadth = support_breadth(parent);
    let child_breadth = support_breadth(child);
    if parent_breadth > child_breadth {
        return true;
    }
    if parent_breadth < child_breadth {
        return false;
    }
    let parent_independent = independent_evidence_count(parent);
    let parent_specificity = specificity_score(parent);
    let child_independent = independent_evidence_count(child);
    let child_specificity = specificity_score(child);
    parent_independent > child_independent || parent_specificity >= child_specificity
}

fn support_breadth(context: &HypothesisContext) -> usize {
    context.merged_support.capability_keys.len()
        + context.merged_support.entrypoint_ids.len()
        + context.merged_support.owner_classes.len()
}

fn independent_evidence_count(context: &HypothesisContext) -> usize {
    context
        .families
        .iter()
        .map(|family| family.independent_evidence_groups.len())
        .sum()
}

fn specificity_score(context: &HypothesisContext) -> f64 {
    context
        .families
        .iter()
        .map(|family| family.specificity_score)
        .fold(0.0_f64, f64::max)
}

fn independent_business_ownership_evidence(context: &HypothesisContext) -> bool {
    context
        .families
        .iter()
        .any(|family| {
            family.distinct_owners >= 2
                && family.independent_evidence_groups.iter().any(|group| {
                    group.evidence_sources.iter().any(|source| {
                        matches!(
                            source.as_str(),
                            "ownerClass" | "entityVocabulary" | "resourceOwnership"
                        )
                    })
                })
        })
}

fn concept_composition_score(child: &HypothesisContext, parent: &HypothesisContext) -> f64 {
    let mut scores = Vec::new();
    for child_family in &child.families {
        for parent_family in &parent.families {
            if atomized_path_parent_child_relation(
                &parent_family.atomized_path,
                &child_family.atomized_path,
            ) {
                scores.push(1.0);
            }
            let child_root = normalized_root_concept(&child_family.root_concept).0;
            if parent_family.child_concepts.iter().any(|concept| {
                normalized_root_concept(concept).0 == child_root
            }) {
                scores.push(1.0);
            }
            let overlap = child_family
                .child_concepts
                .iter()
                .filter(|concept| parent_family.child_concepts.contains(concept))
                .count();
            if overlap > 0 {
                scores.push(0.8);
            }
            if independent_group_subset(child_family, parent_family) {
                scores.push(0.75);
            }
            if shared_correlated_groups(child_family, parent_family) {
                scores.push(0.7);
            }
        }
    }
    scores.into_iter().fold(0.0_f64, f64::max)
}

fn atomized_path_parent_child_relation(parent_path: &str, child_path: &str) -> bool {
    let parent_tokens = path_tokens(parent_path);
    let child_tokens = path_tokens(child_path);
    !parent_tokens.is_empty()
        && parent_tokens.is_subset(&child_tokens)
        && parent_tokens != child_tokens
}

fn path_tokens(path: &str) -> BTreeSet<String> {
    path.split('/')
        .filter(|token| !token.is_empty())
        .map(|token| normalized_root_concept(token).0)
        .collect()
}

fn independent_group_subset(child: &RankedConceptFamily, parent: &RankedConceptFamily) -> bool {
    let child_groups = child
        .independent_evidence_groups
        .iter()
        .map(|group| group.group_id.as_str())
        .collect::<BTreeSet<_>>();
    let parent_groups = parent
        .independent_evidence_groups
        .iter()
        .map(|group| group.group_id.as_str())
        .collect::<BTreeSet<_>>();
    !child_groups.is_empty() && child_groups.is_subset(&parent_groups) && child_groups != parent_groups
}

fn shared_correlated_groups(child: &RankedConceptFamily, parent: &RankedConceptFamily) -> bool {
    let child_groups = child
        .correlated_evidence_groups
        .iter()
        .map(|group| group.group_id.as_str())
        .collect::<BTreeSet<_>>();
    parent
        .correlated_evidence_groups
        .iter()
        .any(|group| child_groups.contains(group.group_id.as_str()))
}

fn provenance_overlap_ratio(child: &HypothesisContext, parent: &HypothesisContext) -> f64 {
    let child_observations = primitive_observation_ids(child);
    let parent_observations = primitive_observation_ids(parent);
    if child_observations.is_empty() {
        return 0.0;
    }
    let overlap = child_observations
        .intersection(&parent_observations)
        .count();
    overlap as f64 / child_observations.len() as f64
}

fn primitive_observation_ids(context: &HypothesisContext) -> BTreeSet<String> {
    context
        .families
        .iter()
        .flat_map(|family| {
            family
                .provenance
                .primitive_observations
                .iter()
                .map(|observation| observation.observation_id.clone())
        })
        .collect()
}

fn near_identical_group_link(
    child: &HypothesisContext,
    parent: &HypothesisContext,
    provenance_graph: &ProvenanceSeedCandidateGraph,
) -> bool {
    for group in &provenance_graph.seed_hypothesis_groups {
        if group.group_id == child.group.group_id {
            if group
                .near_identical_groups
                .iter()
                .any(|near| near.other_group_id == parent.group.group_id)
            {
                return true;
            }
        }
        if group.group_id == parent.group.group_id {
            if group
                .near_identical_groups
                .iter()
                .any(|near| near.other_group_id == child.group.group_id)
            {
                return true;
            }
        }
    }
    false
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

fn support_jaccard(left: &MergedHypothesisSupport, right: &MergedHypothesisSupport) -> f64 {
    jaccard(&left.capability_keys, &right.capability_keys)
}

fn jaccard<T: Ord + Clone>(left: &BTreeSet<T>, right: &BTreeSet<T>) -> f64 {
    if left.is_empty() && right.is_empty() {
        return 0.0;
    }
    let intersection = left.intersection(right).count();
    let union = left.union(right).count();
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::formation::domain_seed_anchor_eligibility::{
        DiagnosticFamilyInclusion, HypothesisContext,
    };
    use crate::domain::formation::domain_seed_aggregation::{
        EvidenceGroupDiagnostic, IdfPenaltyDiagnostic, RankedConceptFamily,
    };
    use crate::domain::formation::domain_seed_provenance::{
        FamilyProvenance, FamilySupportSignature, ProvenanceSeedCandidateGraph, SeedHypothesisGroup,
    };
    use crate::domain::formation::domain_seed_recovery::AnchorScoreComponents;
    use crate::domain::formation::domain_seed_role_graph::ConceptRoleDiagnostic;

    fn sample_family(
        root: &str,
        atomized_path: &str,
        child_concepts: &[&str],
        capabilities: &[&str],
    ) -> RankedConceptFamily {
        RankedConceptFamily {
            rank: 1,
            root_concept: root.into(),
            child_concepts: child_concepts.iter().map(|value| value.to_string()).collect(),
            atomized_path: atomized_path.into(),
            distinct_capabilities: capabilities.len(),
            distinct_capability_keys: capabilities.iter().map(|value| value.to_string()).collect(),
            distinct_entrypoints: 1,
            distinct_entrypoint_ids: vec!["ep-1".into()],
            distinct_contracts: 0,
            distinct_contract_paths: Vec::new(),
            distinct_owners: 1,
            distinct_owner_classes: vec![format!("{root}Controller")],
            distinct_modules: 1,
            distinct_module_paths: vec!["app.module".into()],
            distinct_units: 1,
            correlated_evidence_groups: Vec::new(),
            independent_evidence_groups: vec![EvidenceGroupDiagnostic {
                group_id: format!("group:{root}"),
                lexical_root: root.into(),
                evidence_sources: vec!["behaviorCall".into()],
                capability_keys: capabilities.iter().map(|value| value.to_string()).collect(),
            }],
            coverage_score: 0.7,
            coherence_score: 0.6,
            specificity_score: if atomized_path.contains('/') { 0.4 } else { 0.8 },
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
                role_class: "ambiguous".into(),
                normalized_root_concept: root.into(),
                normalization_diagnostics: Vec::new(),
            },
            anchor_score_components: AnchorScoreComponents {
                symbolic_total: if root == "order" { 4.0 } else { 3.0 },
                ..Default::default()
            },
            provenance: FamilyProvenance::default(),
            support_signature: FamilySupportSignature::default(),
        }
    }

    fn sample_context(
        id: &str,
        root: &str,
        atomized_path: &str,
        child_concepts: &[&str],
        capabilities: &[&str],
    ) -> HypothesisContext {
        let family = sample_family(root, atomized_path, child_concepts, capabilities);
        HypothesisContext {
            hypothesis_id: format!("hypothesis:{id}"),
            group: SeedHypothesisGroup {
                group_id: id.into(),
                signature_key: id.into(),
                support_signature: FamilySupportSignature::default(),
                competing_family_ids: vec![format!("family:{root}")],
                competing_root_concepts: vec![root.into()],
                near_identical_groups: Vec::new(),
            },
            families: vec![family.clone()],
            representative: family,
            representative_selection_reason: "test".into(),
            diagnostic_inclusions: vec![DiagnosticFamilyInclusion {
                family_id: format!("family:{root}"),
                root_concept: root.into(),
                concept_role: "ambiguous".into(),
                inclusion_reason: "highSignedAmbiguousAnchor".into(),
            }],
            merged_support: {
                let mut support = MergedHypothesisSupport::default();
                support
                    .capability_keys
                    .extend(capabilities.iter().map(|value| value.to_string()));
                support.entrypoint_ids.insert("ep-1".into());
                support.owner_classes.insert(format!("{root}Controller"));
                support.root_concepts.insert(root.into());
                support
            },
            max_signed_anchor_score: if root == "order" { 4.0 } else { 3.0 },
            eligibility_class: "eligibleDomainAnchor".into(),
            domain_anchor_eligible: true,
            support_containment_score: 0.0,
            contained_in_hypothesis_id: None,
            has_independent_ownership_state_behavior: false,
            preserves_multi_entrypoint_owner_operations: false,
        }
    }

    #[test]
    fn atomized_path_parent_child_relation은_문자열_prefix가_아닌_경로_토큰을_사용한다() {
        assert!(atomized_path_parent_child_relation("order", "order/draft"));
        assert!(!atomized_path_parent_child_relation("admin", "adminapi"));
    }

    #[test]
    fn hierarchy는_독립_ownership이_없는_subconcept를_would_demote로_기록한다() {
        let parent = sample_context("parent", "order", "order", &["order", "draftorder"], &["a", "b", "c"]);
        let mut child = sample_context("child", "draft", "order/draft", &["draft", "draftorder"], &["a"]);
        child.merged_support.owner_classes = parent.merged_support.owner_classes.clone();
        let contexts = vec![parent, child];
        let hierarchy = infer_concept_hierarchy(&contexts, &ProvenanceSeedCandidateGraph::default());
        assert!(
            !hierarchy.parent_subconcept_edges.is_empty(),
            "expected parent/subconcept edge, got {:?}",
            hierarchy.parent_subconcept_edges
        );
        assert!(hierarchy.would_demote_count >= 1);
        assert!(contexts.iter().all(|context| context.domain_anchor_eligible));
    }
}
