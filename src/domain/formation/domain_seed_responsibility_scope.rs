//! Responsibility vs structural scope diagnostics for eligible anchors.

use super::domain_seed_anchor_eligibility::HypothesisContext;
use super::domain_seed_anchor_affinity::AnchorCapabilityEdge;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const SCOPE_CLASS_RESPONSIBILITY: &str = "responsibilityLike";
pub const SCOPE_CLASS_SCOPE: &str = "scopeLike";
pub const SCOPE_CLASS_MIXED: &str = "mixed/unknown";

pub const AMBIGUITY_RESPONSIBILITY_VS_RESPONSIBILITY: &str = "responsibility-vs-responsibility";
pub const AMBIGUITY_RESPONSIBILITY_VS_SCOPE: &str = "responsibility-vs-scope";
pub const AMBIGUITY_SCOPE_VS_SCOPE: &str = "scope-vs-scope";
pub const AMBIGUITY_SCOPE_UNKNOWN: &str = "unknown";

pub const SCOPE_SCORE_THRESHOLD: f64 = 0.52;
pub const RESPONSIBILITY_SCORE_THRESHOLD: f64 = 0.42;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ResponsibilityScopeDiagnostics {
    pub eligible_anchor_count: usize,
    pub scope_class_counts: Vec<ScopeClassCount>,
    pub anchor_scope_records: Vec<AnchorScopeRecord>,
    pub top_fanout_anchors: Vec<AnchorScopeRecord>,
    pub responsibility_ambiguity_class_counts: Vec<ScopeClassCount>,
    pub representative_ambiguous_scope_cases: Vec<AmbiguousScopeCaseRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeClassCount {
    pub scope_class: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AnchorScopeRecord {
    pub hypothesis_id: String,
    pub representative_root_concept: String,
    pub scope_class: String,
    pub fanout_capabilities: usize,
    pub fanout_ratio: f64,
    pub capability_dispersion: f64,
    pub entrypoint_dispersion: f64,
    pub owner_dispersion: f64,
    pub unit_dispersion: f64,
    pub contract_namespace_breadth: f64,
    pub entity_resource_concentration: f64,
    pub flow_neighborhood_concentration: f64,
    pub provenance_diversity: f64,
    pub scope_score: f64,
    pub responsibility_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AmbiguousScopeCaseRecord {
    pub capability_key: String,
    pub responsibility_scope_ambiguity_class: String,
    pub margin: f64,
    pub top1_root_concept: String,
    pub top1_scope_class: String,
    pub top2_root_concept: String,
    pub top2_scope_class: String,
}

#[derive(Debug, Clone)]
struct ScopeSignals {
    fanout_ratio: f64,
    capability_dispersion: f64,
    entrypoint_dispersion: f64,
    owner_dispersion: f64,
    unit_dispersion: f64,
    contract_namespace_breadth: f64,
    entity_resource_concentration: f64,
    flow_neighborhood_concentration: f64,
    provenance_diversity: f64,
    scope_score: f64,
    responsibility_score: f64,
}

pub fn build_responsibility_scope_diagnostics(
    hypothesis_contexts: &[HypothesisContext],
    edges: &[AnchorCapabilityEdge],
    capability_count: usize,
    ambiguous_scope_inputs: &[(String, f64, String, String, String, String)],
) -> ResponsibilityScopeDiagnostics {
    let fanout_by_hypothesis = hypothesis_fanout(edges);
    let scope_by_hypothesis = hypothesis_contexts
        .iter()
        .filter(|context| context.domain_anchor_eligible)
        .map(|context| {
            let fanout = fanout_by_hypothesis
                .get(&context.hypothesis_id)
                .copied()
                .unwrap_or(0);
            let record = diagnose_anchor_scope(context, capability_count, fanout);
            (context.hypothesis_id.clone(), record)
        })
        .collect::<BTreeMap<_, _>>();

    let mut scope_class_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut anchor_scope_records = scope_by_hypothesis.values().cloned().collect::<Vec<_>>();
    for record in &anchor_scope_records {
        *scope_class_counts
            .entry(record.scope_class.clone())
            .or_default() += 1;
    }
    anchor_scope_records.sort_by(|left, right| {
        right
            .fanout_ratio
            .partial_cmp(&left.fanout_ratio)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.representative_root_concept.cmp(&right.representative_root_concept))
    });
    let top_fanout_anchors = anchor_scope_records.iter().take(20).cloned().collect();

    let mut responsibility_ambiguity_class_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut representative_ambiguous_scope_cases = Vec::new();
    for (
        capability_key,
        margin,
        top1_id,
        top1_root,
        top2_id,
        top2_root,
    ) in ambiguous_scope_inputs
    {
        let top1_scope = scope_by_hypothesis
            .get(top1_id)
            .map(|record| record.scope_class.as_str())
            .unwrap_or(SCOPE_CLASS_MIXED);
        let top2_scope = scope_by_hypothesis
            .get(top2_id)
            .map(|record| record.scope_class.as_str())
            .unwrap_or(SCOPE_CLASS_MIXED);
        let ambiguity_class = classify_scope_ambiguity(top1_scope, top2_scope);
        *responsibility_ambiguity_class_counts
            .entry(ambiguity_class.clone())
            .or_default() += 1;
        representative_ambiguous_scope_cases.push(AmbiguousScopeCaseRecord {
            capability_key: capability_key.clone(),
            responsibility_scope_ambiguity_class: ambiguity_class,
            margin: *margin,
            top1_root_concept: top1_root.clone(),
            top1_scope_class: top1_scope.into(),
            top2_root_concept: top2_root.clone(),
            top2_scope_class: top2_scope.into(),
        });
    }
    representative_ambiguous_scope_cases.sort_by(|left, right| {
        left.margin
            .partial_cmp(&right.margin)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.capability_key.cmp(&right.capability_key))
    });

    ResponsibilityScopeDiagnostics {
        eligible_anchor_count: scope_by_hypothesis.len(),
        scope_class_counts: class_counts(scope_class_counts),
        anchor_scope_records,
        top_fanout_anchors,
        responsibility_ambiguity_class_counts: class_counts(responsibility_ambiguity_class_counts),
        representative_ambiguous_scope_cases: select_representative_ambiguous(
            representative_ambiguous_scope_cases,
        ),
    }
}

pub fn diagnose_anchor_scope(
    context: &HypothesisContext,
    capability_count: usize,
    fanout_capabilities: usize,
) -> AnchorScopeRecord {
    let signals = compute_scope_signals(context, capability_count, fanout_capabilities);
    let scope_class = classify_scope_class(&signals);
    AnchorScopeRecord {
        hypothesis_id: context.hypothesis_id.clone(),
        representative_root_concept: context.representative.root_concept.clone(),
        scope_class,
        fanout_capabilities,
        fanout_ratio: signals.fanout_ratio,
        capability_dispersion: signals.capability_dispersion,
        entrypoint_dispersion: signals.entrypoint_dispersion,
        owner_dispersion: signals.owner_dispersion,
        unit_dispersion: signals.unit_dispersion,
        contract_namespace_breadth: signals.contract_namespace_breadth,
        entity_resource_concentration: signals.entity_resource_concentration,
        flow_neighborhood_concentration: signals.flow_neighborhood_concentration,
        provenance_diversity: signals.provenance_diversity,
        scope_score: signals.scope_score,
        responsibility_score: signals.responsibility_score,
    }
}

pub fn classify_scope_ambiguity(left_scope: &str, right_scope: &str) -> String {
    match (left_scope, right_scope) {
        (SCOPE_CLASS_RESPONSIBILITY, SCOPE_CLASS_RESPONSIBILITY) => {
            AMBIGUITY_RESPONSIBILITY_VS_RESPONSIBILITY.into()
        }
        (SCOPE_CLASS_SCOPE, SCOPE_CLASS_SCOPE) => AMBIGUITY_SCOPE_VS_SCOPE.into(),
        (SCOPE_CLASS_RESPONSIBILITY, SCOPE_CLASS_SCOPE)
        | (SCOPE_CLASS_SCOPE, SCOPE_CLASS_RESPONSIBILITY) => AMBIGUITY_RESPONSIBILITY_VS_SCOPE.into(),
        _ => AMBIGUITY_SCOPE_UNKNOWN.into(),
    }
}

pub fn scope_node_id(signature_key: &str) -> String {
    if signature_key.is_empty() {
        "scope:unknown".into()
    } else {
        format!("scope:{signature_key}")
    }
}

fn classify_scope_class(signals: &ScopeSignals) -> String {
    if signals.scope_score >= SCOPE_SCORE_THRESHOLD
        && signals.responsibility_score < RESPONSIBILITY_SCORE_THRESHOLD
    {
        SCOPE_CLASS_SCOPE.into()
    } else if signals.responsibility_score >= RESPONSIBILITY_SCORE_THRESHOLD
        && signals.scope_score < SCOPE_SCORE_THRESHOLD
    {
        SCOPE_CLASS_RESPONSIBILITY.into()
    } else {
        SCOPE_CLASS_MIXED.into()
    }
}

fn compute_scope_signals(
    context: &HypothesisContext,
    capability_count: usize,
    fanout_capabilities: usize,
) -> ScopeSignals {
    let support = &context.merged_support;
    let capability_support = support.capability_keys.len().max(1);
    let project_capabilities = capability_count.max(1) as f64;

    let fanout_ratio = fanout_capabilities as f64 / project_capabilities;
    let capability_dispersion = support.capability_keys.len() as f64 / project_capabilities;
    let entrypoint_dispersion =
        normalized_dispersion(support.entrypoint_ids.len(), capability_support);
    let owner_dispersion = normalized_dispersion(support.owner_classes.len(), capability_support);
    let unit_dispersion = normalized_dispersion(support.unit_ids.len(), capability_support);

    let contract_paths = contract_paths(context);
    let contract_namespace_breadth = contract_namespace_breadth(&contract_paths);

    let resource_count = support.resource_entities.len();
    let flow_count = flow_ids(context).len();
    let entity_resource_concentration = concentration_ratio(resource_count, capability_support);
    let flow_neighborhood_concentration = concentration_ratio(flow_count, capability_support);
    let provenance_diversity = provenance_diversity(context);

    let scope_score = 0.34 * fanout_ratio
        + 0.24 * capability_dispersion
        + 0.14 * entrypoint_dispersion
        + 0.10 * owner_dispersion
        + 0.08 * unit_dispersion
        + 0.10 * contract_namespace_breadth;

    let responsibility_score = 0.28 * entity_resource_concentration
        + 0.22 * flow_neighborhood_concentration
        + 0.22 * provenance_diversity
        + 0.18 * owner_concentration(support.owner_classes.len(), capability_support)
        + 0.10 * (1.0 - fanout_ratio).max(0.0);

    ScopeSignals {
        fanout_ratio,
        capability_dispersion,
        entrypoint_dispersion,
        owner_dispersion,
        unit_dispersion,
        contract_namespace_breadth,
        entity_resource_concentration,
        flow_neighborhood_concentration,
        provenance_diversity,
        scope_score,
        responsibility_score,
    }
}

fn normalized_dispersion(distinct_count: usize, capability_support: usize) -> f64 {
    if distinct_count == 0 {
        return 0.0;
    }
    (distinct_count as f64 / capability_support.max(1) as f64).min(1.0)
}

fn concentration_ratio(distinct_count: usize, capability_support: usize) -> f64 {
    if distinct_count == 0 {
        return 0.0;
    }
    (1.0 - (distinct_count as f64 / capability_support.max(1) as f64)).clamp(0.0, 1.0)
}

fn owner_concentration(owner_count: usize, capability_support: usize) -> f64 {
    concentration_ratio(owner_count, capability_support)
}

fn contract_paths(context: &HypothesisContext) -> BTreeSet<String> {
    context
        .families
        .iter()
        .flat_map(|family| family.distinct_contract_paths.iter().cloned())
        .collect()
}

fn contract_namespace_breadth(contract_paths: &BTreeSet<String>) -> f64 {
    if contract_paths.is_empty() {
        return 0.0;
    }
    let namespaces = contract_paths
        .iter()
        .filter_map(|path| path.split('/').next().filter(|segment| !segment.is_empty()))
        .collect::<BTreeSet<_>>();
    (namespaces.len() as f64 / contract_paths.len() as f64).min(1.0)
}

fn flow_ids(context: &HypothesisContext) -> BTreeSet<String> {
    let mut flows = context.merged_support.flow_ids.clone();
    for family in &context.families {
        flows.extend(family.provenance.flow_ids.iter().cloned());
    }
    flows
}

fn provenance_diversity(context: &HypothesisContext) -> f64 {
    let mut evidence_sources = BTreeSet::new();
    let mut primitive_kinds = BTreeSet::new();
    for family in &context.families {
        for group in &family.independent_evidence_groups {
            evidence_sources.extend(group.evidence_sources.iter().cloned());
        }
        for group in &family.correlated_evidence_groups {
            evidence_sources.extend(group.evidence_sources.iter().cloned());
        }
        for observation in &family.provenance.primitive_observations {
            primitive_kinds.insert(observation.kind.clone());
        }
    }
    let source_component = (evidence_sources.len() as f64 / 6.0).min(1.0);
    let primitive_component = (primitive_kinds.len() as f64 / 5.0).min(1.0);
    (source_component + primitive_component) / 2.0
}

fn hypothesis_fanout(edges: &[AnchorCapabilityEdge]) -> BTreeMap<String, usize> {
    let mut fanout: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for edge in edges {
        fanout
            .entry(edge.hypothesis_id.clone())
            .or_default()
            .insert(edge.capability_key.clone());
    }
    fanout
        .into_iter()
        .map(|(hypothesis_id, capabilities)| (hypothesis_id, capabilities.len()))
        .collect()
}

fn class_counts(counts: BTreeMap<String, usize>) -> Vec<ScopeClassCount> {
    let mut items = counts
        .into_iter()
        .map(|(scope_class, count)| ScopeClassCount { scope_class, count })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.scope_class.cmp(&right.scope_class))
    });
    items
}

fn select_representative_ambiguous(
    records: Vec<AmbiguousScopeCaseRecord>,
) -> Vec<AmbiguousScopeCaseRecord> {
    let mut selected = Vec::new();
    let mut seen_classes = BTreeSet::new();
    for record in records.iter() {
        if seen_classes.insert(record.responsibility_scope_ambiguity_class.clone()) {
            selected.push(record.clone());
        }
        if selected.len() >= 12 {
            return selected;
        }
    }
    for record in records.into_iter().take(12) {
        if selected.iter().any(|existing| {
            existing.capability_key == record.capability_key
                && existing.responsibility_scope_ambiguity_class
                    == record.responsibility_scope_ambiguity_class
        }) {
            continue;
        }
        selected.push(record);
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

    fn sample_context(capability_count: usize, fanout: usize) -> HypothesisContext {
        let mut capabilities = BTreeSet::new();
        for index in 0..capability_count {
            capabilities.insert(format!("cap-{index}"));
        }
        let family = RankedConceptFamily {
            rank: 1,
            root_concept: "ignored-for-classification".into(),
            child_concepts: Vec::new(),
            atomized_path: "ignored".into(),
            distinct_capabilities: capability_count,
            distinct_capability_keys: capabilities.iter().cloned().collect(),
            distinct_entrypoints: 1,
            distinct_entrypoint_ids: vec!["ep-1".into()],
            distinct_contracts: 0,
            distinct_contract_paths: Vec::new(),
            distinct_owners: 1,
            distinct_owner_classes: vec!["Owner".into()],
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
                normalized_root_concept: "ignored".into(),
                normalization_diagnostics: Vec::new(),
            },
            anchor_score_components: AnchorScoreComponents::default(),
            provenance: FamilyProvenance::default(),
            support_signature: FamilySupportSignature::default(),
        };
        HypothesisContext {
            hypothesis_id: "hypothesis:test".into(),
            group: SeedHypothesisGroup {
                group_id: "test".into(),
                signature_key: "sig:test".into(),
                support_signature: FamilySupportSignature::default(),
                competing_family_ids: vec!["family:test".into()],
                competing_root_concepts: vec!["ignored".into()],
                near_identical_groups: Vec::new(),
            },
            families: vec![family.clone()],
            representative: family,
            representative_selection_reason: "test".into(),
            diagnostic_inclusions: vec![DiagnosticFamilyInclusion {
                family_id: "family:test".into(),
                root_concept: "ignored".into(),
                concept_role: "ambiguous".into(),
                inclusion_reason: "highSignedAmbiguousAnchor".into(),
            }],
            merged_support: MergedHypothesisSupport {
                capability_keys: capabilities,
                entrypoint_ids: BTreeSet::from(["ep-1".into()]),
                owner_classes: BTreeSet::from(["Owner".into()]),
                unit_ids: BTreeSet::from(["unit-1".into()]),
                module_paths: BTreeSet::from(["app.module".into()]),
                resource_entities: BTreeSet::new(),
                flow_ids: BTreeSet::new(),
                root_concepts: BTreeSet::new(),
            },
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
    fn 높은_fanout은_scopeLike로_분류된다() {
        let context = sample_context(250, 0);
        let record = diagnose_anchor_scope(&context, 295, 291);
        assert_eq!(record.scope_class, SCOPE_CLASS_SCOPE);
    }

    #[test]
    fn 좁은_support는_responsibilityLike로_분류된다() {
        let context = sample_context(12, 0);
        let mut support = context.merged_support.clone();
        support.resource_entities.insert("Order".into());
        support.flow_ids.insert("flow-1".into());
        let mut context = context;
        context.merged_support = support;
        let record = diagnose_anchor_scope(&context, 295, 10);
        assert_eq!(record.scope_class, SCOPE_CLASS_RESPONSIBILITY);
    }

    #[test]
    fn scope_ambiguity_class는_쌍_성격을_집계한다() {
        assert_eq!(
            classify_scope_ambiguity(SCOPE_CLASS_SCOPE, SCOPE_CLASS_SCOPE),
            AMBIGUITY_SCOPE_VS_SCOPE
        );
        assert_eq!(
            classify_scope_ambiguity(SCOPE_CLASS_RESPONSIBILITY, SCOPE_CLASS_SCOPE),
            AMBIGUITY_RESPONSIBILITY_VS_SCOPE
        );
    }
}
