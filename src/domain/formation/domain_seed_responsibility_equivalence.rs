//! Responsibility equivalence diagnostics for anchor hypothesis pairs.
//!
//! Structural responsibility overlap/containment only — concept strings are hints, not identity.

use super::domain_seed_anchor_eligibility::HypothesisContext;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const EQUIVALENCE_CLASS_EQUIVALENT: &str = "equivalent/alias-like";
pub const EQUIVALENCE_CLASS_DISTINCT: &str = "distinct/nested-responsibility";
pub const EQUIVALENCE_CLASS_UNKNOWN: &str = "unknown";

const NEIGHBORHOOD_OVERLAP_THRESHOLD: f64 = 0.85;
const SUPPORT_CONTAINMENT_THRESHOLD: f64 = 0.85;
const MIN_OBSERVED_DIMENSIONS: usize = 2;

const INDEPENDENT_EVIDENCE_SOURCES: &[&str] = &[
    "ownerClass",
    "entityVocabulary",
    "resourceOwnership",
];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ResponsibilityEquivalenceDiagnostics {
    pub analyzed_anchor_pair_count: usize,
    pub ambiguous_top_pair_count: usize,
    pub anchor_pair_class_counts: Vec<EquivalenceClassCount>,
    pub ambiguous_pair_class_counts: Vec<EquivalenceClassCount>,
    pub representative_anchor_pairs: Vec<AnchorPairEquivalenceRecord>,
    pub representative_ambiguous_pairs: Vec<AmbiguousPairEquivalenceRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EquivalenceClassCount {
    pub equivalence_class: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorPairEquivalenceRecord {
    pub left_hypothesis_id: String,
    pub left_root_concept: String,
    pub right_hypothesis_id: String,
    pub right_root_concept: String,
    pub equivalence_class: String,
    pub neighborhood_score: f64,
    pub left_supports_right: f64,
    pub right_supports_left: f64,
    pub left_independent_signals: Vec<String>,
    pub right_independent_signals: Vec<String>,
    pub overlap_signals: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AmbiguousPairEquivalenceRecord {
    pub capability_key: String,
    pub ambiguity_class: String,
    pub margin: f64,
    pub top1_root_concept: String,
    pub top2_root_concept: String,
    pub equivalence_class: String,
    pub neighborhood_score: f64,
    pub left_independent_signals: Vec<String>,
    pub right_independent_signals: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct NeighborhoodMetrics {
    neighborhood_score: f64,
    left_supports_right: f64,
    right_supports_left: f64,
    overlap_signals: Vec<String>,
    dimensions_observed: usize,
}

pub fn build_responsibility_equivalence_diagnostics(
    hypothesis_contexts: &[HypothesisContext],
    ambiguous_pairs: &[(String, String, String, f64, String, String)],
) -> ResponsibilityEquivalenceDiagnostics {
    let context_by_id = hypothesis_contexts
        .iter()
        .map(|context| (context.hypothesis_id.as_str(), context))
        .collect::<BTreeMap<_, _>>();

    let eligible: Vec<_> = hypothesis_contexts
        .iter()
        .filter(|context| context.domain_anchor_eligible)
        .collect();

    let mut anchor_pair_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut anchor_pair_records = Vec::new();
    for left_index in 0..eligible.len() {
        for right_index in (left_index + 1)..eligible.len() {
            let left = eligible[left_index];
            let right = eligible[right_index];
            let record = classify_anchor_pair(left, right);
            *anchor_pair_counts
                .entry(record.equivalence_class.clone())
                .or_default() += 1;
            anchor_pair_records.push(record);
        }
    }

    let mut ambiguous_pair_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut ambiguous_pair_records = Vec::new();
    for (capability_key, ambiguity_class, top1_id, margin, top2_id, top2_root) in ambiguous_pairs {
        let Some(top1) = context_by_id.get(top1_id.as_str()) else {
            continue;
        };
        let Some(top2) = context_by_id.get(top2_id.as_str()) else {
            continue;
        };
        let record = classify_anchor_pair(top1, top2);
        *ambiguous_pair_counts
            .entry(record.equivalence_class.clone())
            .or_default() += 1;
        ambiguous_pair_records.push(AmbiguousPairEquivalenceRecord {
            capability_key: capability_key.clone(),
            ambiguity_class: ambiguity_class.clone(),
            margin: *margin,
            top1_root_concept: top1.representative.root_concept.clone(),
            top2_root_concept: top2_root.clone(),
            equivalence_class: record.equivalence_class.clone(),
            neighborhood_score: record.neighborhood_score,
            left_independent_signals: record.left_independent_signals,
            right_independent_signals: record.right_independent_signals,
        });
    }

    ResponsibilityEquivalenceDiagnostics {
        analyzed_anchor_pair_count: anchor_pair_records.len(),
        ambiguous_top_pair_count: ambiguous_pair_records.len(),
        anchor_pair_class_counts: class_counts(anchor_pair_counts),
        ambiguous_pair_class_counts: class_counts(ambiguous_pair_counts),
        representative_anchor_pairs: select_representative_pairs(anchor_pair_records),
        representative_ambiguous_pairs: select_representative_ambiguous(ambiguous_pair_records),
    }
}

pub fn classify_anchor_pair(
    left: &HypothesisContext,
    right: &HypothesisContext,
) -> AnchorPairEquivalenceRecord {
    let metrics = compute_neighborhood_metrics(left, right);
    let left_independent = independent_responsibility_signals(left, right);
    let right_independent = independent_responsibility_signals(right, left);
    let equivalence_class = classify_equivalence(
        &metrics,
        &left_independent,
        &right_independent,
        &left.merged_support.capability_keys,
        &right.merged_support.capability_keys,
    );
    AnchorPairEquivalenceRecord {
        left_hypothesis_id: left.hypothesis_id.clone(),
        left_root_concept: left.representative.root_concept.clone(),
        right_hypothesis_id: right.hypothesis_id.clone(),
        right_root_concept: right.representative.root_concept.clone(),
        equivalence_class,
        neighborhood_score: metrics.neighborhood_score,
        left_supports_right: metrics.left_supports_right,
        right_supports_left: metrics.right_supports_left,
        left_independent_signals: left_independent,
        right_independent_signals: right_independent,
        overlap_signals: metrics.overlap_signals,
    }
}

pub fn responsibility_domain_id(signature_key: &str) -> String {
    if signature_key.is_empty() {
        "D_unknown".into()
    } else {
        format!("D_{signature_key}")
    }
}

fn classify_equivalence(
    metrics: &NeighborhoodMetrics,
    left_independent: &[String],
    right_independent: &[String],
    left_capabilities: &BTreeSet<String>,
    right_capabilities: &BTreeSet<String>,
) -> String {
    if metrics.dimensions_observed < MIN_OBSERVED_DIMENSIONS {
        return EQUIVALENCE_CLASS_UNKNOWN.into();
    }

    let nested_containment = metrics.left_supports_right >= SUPPORT_CONTAINMENT_THRESHOLD
        || metrics.right_supports_left >= SUPPORT_CONTAINMENT_THRESHOLD;
    let capability_nested = capability_subset(left_capabilities, right_capabilities)
        || capability_subset(right_capabilities, left_capabilities);
    let has_independent = !left_independent.is_empty() || !right_independent.is_empty();

    if metrics.neighborhood_score >= NEIGHBORHOOD_OVERLAP_THRESHOLD
        && !has_independent
    {
        return EQUIVALENCE_CLASS_EQUIVALENT.into();
    }

    if (nested_containment || capability_nested) && has_independent {
        return EQUIVALENCE_CLASS_DISTINCT.into();
    }

    if metrics.neighborhood_score >= NEIGHBORHOOD_OVERLAP_THRESHOLD && has_independent {
        return EQUIVALENCE_CLASS_DISTINCT.into();
    }

    EQUIVALENCE_CLASS_UNKNOWN.into()
}

fn capability_subset(inner: &BTreeSet<String>, outer: &BTreeSet<String>) -> bool {
    !inner.is_empty() && inner.is_subset(outer) && inner != outer
}

fn compute_neighborhood_metrics(
    left: &HypothesisContext,
    right: &HypothesisContext,
) -> NeighborhoodMetrics {
    let left_support = enriched_support(left);
    let right_support = enriched_support(right);

    let mut dimension_scores = Vec::new();
    let mut overlap_signals = Vec::new();

    record_dimension(
        "capabilitySupport",
        jaccard(&left_support.capability_keys, &right_support.capability_keys),
        &mut dimension_scores,
        &mut overlap_signals,
        NEIGHBORHOOD_OVERLAP_THRESHOLD,
    );
    record_dimension(
        "entrypoint",
        jaccard(&left_support.entrypoint_ids, &right_support.entrypoint_ids),
        &mut dimension_scores,
        &mut overlap_signals,
        NEIGHBORHOOD_OVERLAP_THRESHOLD,
    );
    record_dimension(
        "owner",
        jaccard(&left_support.owner_classes, &right_support.owner_classes),
        &mut dimension_scores,
        &mut overlap_signals,
        NEIGHBORHOOD_OVERLAP_THRESHOLD,
    );
    record_dimension(
        "unit",
        jaccard(&left_support.unit_ids, &right_support.unit_ids),
        &mut dimension_scores,
        &mut overlap_signals,
        NEIGHBORHOOD_OVERLAP_THRESHOLD,
    );
    record_dimension(
        "module",
        jaccard(&left_support.module_paths, &right_support.module_paths),
        &mut dimension_scores,
        &mut overlap_signals,
        NEIGHBORHOOD_OVERLAP_THRESHOLD,
    );
    record_dimension(
        "entityResource",
        jaccard(
            &left_support.resource_entities,
            &right_support.resource_entities,
        ),
        &mut dimension_scores,
        &mut overlap_signals,
        NEIGHBORHOOD_OVERLAP_THRESHOLD,
    );
    record_dimension(
        "flowNeighborhood",
        jaccard(&left_support.flow_ids, &right_support.flow_ids),
        &mut dimension_scores,
        &mut overlap_signals,
        NEIGHBORHOOD_OVERLAP_THRESHOLD,
    );
    record_dimension(
        "provenance",
        jaccard(
            &left_support.provenance_observation_ids,
            &right_support.provenance_observation_ids,
        ),
        &mut dimension_scores,
        &mut overlap_signals,
        PROVENANCE_OVERLAP_THRESHOLD,
    );

    let left_supports_right = support_containment_ratio(&right_support, &left_support);
    let right_supports_left = support_containment_ratio(&left_support, &right_support);
    if left_supports_right >= SUPPORT_CONTAINMENT_THRESHOLD {
        overlap_signals.push("leftContainsRight".into());
    }
    if right_supports_left >= SUPPORT_CONTAINMENT_THRESHOLD {
        overlap_signals.push("rightContainsLeft".into());
    }

    let neighborhood_score = if dimension_scores.is_empty() {
        0.0
    } else {
        dimension_scores.iter().sum::<f64>() / dimension_scores.len() as f64
    };

    NeighborhoodMetrics {
        neighborhood_score,
        left_supports_right,
        right_supports_left,
        overlap_signals,
        dimensions_observed: dimension_scores.len(),
    }
}

const PROVENANCE_OVERLAP_THRESHOLD: f64 = 0.75;

#[derive(Debug, Clone, Default)]
struct EnrichedSupport {
    capability_keys: BTreeSet<String>,
    entrypoint_ids: BTreeSet<String>,
    owner_classes: BTreeSet<String>,
    unit_ids: BTreeSet<String>,
    module_paths: BTreeSet<String>,
    resource_entities: BTreeSet<String>,
    flow_ids: BTreeSet<String>,
    provenance_observation_ids: BTreeSet<String>,
}

fn enriched_support(context: &HypothesisContext) -> EnrichedSupport {
    let mut support = EnrichedSupport {
        capability_keys: context.merged_support.capability_keys.clone(),
        entrypoint_ids: context.merged_support.entrypoint_ids.clone(),
        owner_classes: context.merged_support.owner_classes.clone(),
        unit_ids: context.merged_support.unit_ids.clone(),
        module_paths: context.merged_support.module_paths.clone(),
        resource_entities: context.merged_support.resource_entities.clone(),
        flow_ids: context.merged_support.flow_ids.clone(),
        provenance_observation_ids: BTreeSet::new(),
    };
    for family in &context.families {
        support
            .flow_ids
            .extend(family.provenance.flow_ids.iter().cloned());
        for observation in &family.provenance.primitive_observations {
            support
                .provenance_observation_ids
                .insert(observation.observation_id.clone());
        }
    }
    support
}

fn independent_responsibility_signals(
    self_ctx: &HypothesisContext,
    other_ctx: &HypothesisContext,
) -> Vec<String> {
    let mut signals = Vec::new();
    let self_support = enriched_support(self_ctx);
    let other_support = enriched_support(other_ctx);

    if !difference(&self_support.owner_classes, &other_support.owner_classes).is_empty() {
        signals.push("distinctOwner".into());
    }
    if !difference(&self_support.entrypoint_ids, &other_support.entrypoint_ids).is_empty() {
        signals.push("distinctEntrypoint".into());
    }
    if !difference(&self_support.flow_ids, &other_support.flow_ids).is_empty() {
        signals.push("distinctFlow".into());
    }
    if !difference(
        &self_support.resource_entities,
        &other_support.resource_entities,
    )
    .is_empty()
    {
        signals.push("distinctResource".into());
    }
    if !difference(&self_support.unit_ids, &other_support.unit_ids).is_empty() {
        signals.push("distinctUnit".into());
    }
    if independent_evidence_groups(self_ctx, other_ctx) {
        signals.push("independentEvidenceGroup".into());
    }
    signals
}

fn independent_evidence_groups(
    self_ctx: &HypothesisContext,
    other_ctx: &HypothesisContext,
) -> bool {
    let other_group_ids = other_ctx
        .families
        .iter()
        .flat_map(|family| family.independent_evidence_groups.iter())
        .map(|group| group.group_id.as_str())
        .collect::<BTreeSet<_>>();
    self_ctx.families.iter().any(|family| {
        family.independent_evidence_groups.iter().any(|group| {
            !other_group_ids.contains(group.group_id.as_str())
                && group.evidence_sources.iter().any(|source| {
                    INDEPENDENT_EVIDENCE_SOURCES.contains(&source.as_str())
                })
        })
    })
}

fn record_dimension(
    label: &str,
    score: f64,
    dimension_scores: &mut Vec<f64>,
    overlap_signals: &mut Vec<String>,
    threshold: f64,
) {
    if score <= 0.0 {
        return;
    }
    dimension_scores.push(score);
    if score >= threshold {
        overlap_signals.push(label.into());
    }
}

fn support_containment_ratio(inner: &EnrichedSupport, outer: &EnrichedSupport) -> f64 {
    let sets = [
        (&inner.capability_keys, &outer.capability_keys),
        (&inner.entrypoint_ids, &outer.entrypoint_ids),
        (&inner.owner_classes, &outer.owner_classes),
        (&inner.unit_ids, &outer.unit_ids),
        (&inner.module_paths, &outer.module_paths),
        (&inner.resource_entities, &outer.resource_entities),
        (&inner.flow_ids, &outer.flow_ids),
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

fn difference<T: Ord + Clone>(left: &BTreeSet<T>, right: &BTreeSet<T>) -> BTreeSet<T> {
    left.difference(right).cloned().collect()
}

fn class_counts(counts: BTreeMap<String, usize>) -> Vec<EquivalenceClassCount> {
    let mut items = counts
        .into_iter()
        .map(|(equivalence_class, count)| EquivalenceClassCount {
            equivalence_class,
            count,
        })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.equivalence_class.cmp(&right.equivalence_class))
    });
    items
}

fn select_representative_pairs(
    mut records: Vec<AnchorPairEquivalenceRecord>,
) -> Vec<AnchorPairEquivalenceRecord> {
    records.sort_by(|left, right| {
        right
            .neighborhood_score
            .partial_cmp(&left.neighborhood_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.left_root_concept.cmp(&right.left_root_concept))
    });
    let mut selected = Vec::new();
    let mut seen_classes = BTreeSet::new();
    for record in records {
        if seen_classes.insert(record.equivalence_class.clone()) {
            selected.push(record);
        }
        if selected.len() >= 12 {
            break;
        }
    }
    selected
}

fn select_representative_ambiguous(
    mut records: Vec<AmbiguousPairEquivalenceRecord>,
) -> Vec<AmbiguousPairEquivalenceRecord> {
    records.sort_by(|left, right| {
        left.margin
            .partial_cmp(&right.margin)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.capability_key.cmp(&right.capability_key))
    });
    let mut selected = Vec::new();
    let mut seen_classes = BTreeSet::new();
    for record in records {
        if seen_classes.insert(record.equivalence_class.clone()) {
            selected.push(record);
        }
        if selected.len() >= 12 {
            break;
        }
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::formation::domain_seed_anchor_eligibility::{
        DiagnosticFamilyInclusion, HypothesisContext, MergedHypothesisSupport,
    };
    use crate::domain::formation::domain_seed_aggregation::{
        IdfPenaltyDiagnostic, RankedConceptFamily,
    };
    use crate::domain::formation::domain_seed_provenance::{
        FamilyProvenance, FamilySupportSignature, SeedHypothesisGroup,
    };
    use crate::domain::formation::domain_seed_recovery::AnchorScoreComponents;
    use crate::domain::formation::domain_seed_role_graph::ConceptRoleDiagnostic;

    fn sample_context(
        id: &str,
        root: &str,
        capabilities: &[&str],
        entrypoints: &[&str],
        owners: &[&str],
    ) -> HypothesisContext {
        let family = RankedConceptFamily {
            rank: 1,
            root_concept: root.into(),
            child_concepts: vec![root.into()],
            atomized_path: root.into(),
            distinct_capabilities: capabilities.len(),
            distinct_capability_keys: capabilities.iter().map(|value| value.to_string()).collect(),
            distinct_entrypoints: entrypoints.len(),
            distinct_entrypoint_ids: entrypoints.iter().map(|value| value.to_string()).collect(),
            distinct_contracts: 0,
            distinct_contract_paths: Vec::new(),
            distinct_owners: owners.len(),
            distinct_owner_classes: owners.iter().map(|value| value.to_string()).collect(),
            distinct_modules: 1,
            distinct_module_paths: vec!["app.module".into()],
            distinct_units: 1,
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
                role_class: "ambiguous".into(),
                normalized_root_concept: root.into(),
                normalization_diagnostics: Vec::new(),
            },
            anchor_score_components: AnchorScoreComponents {
                symbolic_total: 3.0,
                ..Default::default()
            },
            provenance: FamilyProvenance::default(),
            support_signature: FamilySupportSignature {
                signature_key: format!("sig:{root}"),
                ..Default::default()
            },
        };
        let mut merged = MergedHypothesisSupport::default();
        merged
            .capability_keys
            .extend(capabilities.iter().map(|value| value.to_string()));
        merged
            .entrypoint_ids
            .extend(entrypoints.iter().map(|value| value.to_string()));
        merged
            .owner_classes
            .extend(owners.iter().map(|value| value.to_string()));
        merged.root_concepts.insert(root.into());
        HypothesisContext {
            hypothesis_id: format!("hypothesis:{id}"),
            group: SeedHypothesisGroup {
                group_id: id.into(),
                signature_key: format!("sig:{root}"),
                support_signature: FamilySupportSignature {
                    signature_key: format!("sig:{root}"),
                    ..Default::default()
                },
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
            merged_support: merged,
            max_signed_anchor_score: 3.0,
            eligibility_class: "eligibleDomainAnchor".into(),
            domain_anchor_eligible: true,
            support_containment_score: 0.0,
            contained_in_hypothesis_id: None,
            has_independent_ownership_state_behavior: false,
            preserves_multi_entrypoint_owner_operations: false,
        }
    }

    #[test]
    fn 거의_동일한_support는_equivalent로_분류된다() {
        let left = sample_context("a", "order", &["a", "b", "c"], &["ep-1"], &["OrderController"]);
        let right = sample_context("b", "orders", &["a", "b", "c"], &["ep-1"], &["OrderController"]);
        let record = classify_anchor_pair(&left, &right);
        assert_eq!(record.equivalence_class, EQUIVALENCE_CLASS_EQUIVALENT);
    }

    #[test]
    fn subset이면서_독립_entrypoint가_있으면_distinct로_분류된다() {
        let left = sample_context("a", "order", &["a", "b", "c"], &["ep-1", "ep-2"], &["OrderController"]);
        let mut right = sample_context("b", "draft", &["a"], &["ep-1"], &["OrderController"]);
        right.merged_support.entrypoint_ids.insert("ep-draft".into());
        let record = classify_anchor_pair(&left, &right);
        assert_eq!(record.equivalence_class, EQUIVALENCE_CLASS_DISTINCT);
    }

    #[test]
    fn responsibility_domain_id는_D_prefix를_사용한다() {
        assert_eq!(responsibility_domain_id("sig:order"), "D_sig:order");
    }
}
