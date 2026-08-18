//! Scope classification failure decomposition diagnostics.
//!
//! Explains why eligible anchors were classified `scopeLike`, separates ratio from
//! absolute support, and flags likely false positives on small/sparse projects.

use super::domain_seed_anchor_eligibility::HypothesisContext;
use super::domain_seed_anchor_affinity::AnchorCapabilityEdge;
use super::domain_seed_diagnostics::CapabilityDomainSeeds;
use super::domain_seed_provenance::PrimitiveRelationInventory;
use super::domain_seed_responsibility_scope::{
    diagnose_anchor_scope, SCOPE_CLASS_SCOPE, SCOPE_SCORE_THRESHOLD, RESPONSIBILITY_SCORE_THRESHOLD,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const COMPARISON_ROOT_CONCEPTS: &[&str] = &[
    "resolvers",
    "adminapi",
    "admin",
    "auth",
    "endpoints",
    "serverpodauth",
];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ScopeClassificationDecompositionDiagnostics {
    pub project_context: ProjectStructuralContext,
    pub scope_like_anchor_count: usize,
    pub suspected_false_positive_count: usize,
    pub failure_mode_counts: Vec<FailureModeCount>,
    pub scope_like_decompositions: Vec<ScopeLikeAnchorDecomposition>,
    pub comparison_highlights: Vec<ScopeLikeAnchorDecomposition>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStructuralContext {
    pub capability_count: usize,
    pub eligible_anchor_count: usize,
    pub project_size_tier: String,
    pub independent_partition_counts: IndependentPartitionCounts,
    pub primitive_evidence_coverage: PrimitiveEvidenceCoverage,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct IndependentPartitionCounts {
    pub entrypoint_count: usize,
    pub module_path_count: usize,
    pub package_path_count: usize,
    pub owner_class_count: usize,
    pub unit_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PrimitiveEvidenceCoverage {
    pub owner_relations: usize,
    pub entity_relations: usize,
    pub resource_relations: usize,
    pub call_relations: usize,
    pub flow_relations: usize,
    pub entity_resource_sparse: bool,
    pub owner_sparse: bool,
    pub entity_resource_coverage: f64,
    pub owner_coverage: f64,
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FailureModeCount {
    pub failure_mode: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeLikeAnchorDecomposition {
    pub hypothesis_id: String,
    pub representative_root_concept: String,
    pub absolute_capability_support: usize,
    pub retrieval_fanout_capabilities: usize,
    pub fanout_ratio: f64,
    pub absolute_support_tier: String,
    pub partition_spans: PartitionSpanMetrics,
    pub neighborhood_diversity: NeighborhoodDiversityMetrics,
    pub provenance_diversity: f64,
    pub entity_resource_evidence: EvidenceCoverageMetric,
    pub owner_evidence: EvidenceCoverageMetric,
    pub scope_score: f64,
    pub responsibility_score: f64,
    pub classification_reasons: Vec<ScopeClassificationReason>,
    pub suspected_false_positive: bool,
    pub failure_modes: Vec<String>,
    pub comparison_highlight: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PartitionSpanMetrics {
    pub entrypoint_span: usize,
    pub module_span: usize,
    pub package_span: usize,
    pub owner_span: usize,
    pub unit_span: usize,
    pub project_entrypoint_share: f64,
    pub project_module_share: f64,
    pub project_owner_share: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NeighborhoodDiversityMetrics {
    pub flow_distinct_count: usize,
    pub flow_diversity: f64,
    pub flow_concentration: f64,
    pub call_relation_project_total: usize,
    pub flow_relation_project_total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceCoverageMetric {
    pub raw_value: f64,
    pub evidence_count: usize,
    pub project_evidence_total: usize,
    pub coverage: f64,
    pub confidence: String,
    pub usable_for_responsibility: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeClassificationReason {
    pub reason: String,
    pub value: f64,
    pub weighted_contribution: f64,
    pub detail: String,
}

pub fn build_scope_classification_decomposition(
    hypothesis_contexts: &[HypothesisContext],
    edges: &[AnchorCapabilityEdge],
    capability_seeds: &[CapabilityDomainSeeds],
    capability_count: usize,
    primitive_inventory: &PrimitiveRelationInventory,
) -> ScopeClassificationDecompositionDiagnostics {
    let project_context = build_project_context(
        hypothesis_contexts,
        capability_seeds,
        capability_count,
        primitive_inventory,
    );
    let fanout_by_hypothesis = hypothesis_fanout(edges);

    let mut scope_like_decompositions = Vec::new();
    for context in hypothesis_contexts.iter().filter(|context| context.domain_anchor_eligible) {
        let fanout = fanout_by_hypothesis
            .get(&context.hypothesis_id)
            .copied()
            .unwrap_or(0);
        let scope_record = diagnose_anchor_scope(context, capability_count, fanout);
        if scope_record.scope_class != SCOPE_CLASS_SCOPE {
            continue;
        }
        let decomposition = decompose_scope_like_anchor(
            context,
            &scope_record,
            fanout,
            capability_count,
            &project_context,
            primitive_inventory,
        );
        scope_like_decompositions.push(decomposition);
    }

    scope_like_decompositions.sort_by(|left, right| {
        right
            .fanout_ratio
            .partial_cmp(&left.fanout_ratio)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                right
                    .absolute_capability_support
                    .cmp(&left.absolute_capability_support)
            })
            .then_with(|| {
                left.representative_root_concept
                    .cmp(&right.representative_root_concept)
            })
    });

    let suspected_false_positive_count = scope_like_decompositions
        .iter()
        .filter(|record| record.suspected_false_positive)
        .count();

    let mut failure_mode_counts: BTreeMap<String, usize> = BTreeMap::new();
    for record in &scope_like_decompositions {
        for mode in &record.failure_modes {
            *failure_mode_counts.entry(mode.clone()).or_default() += 1;
        }
    }

    let comparison_highlights = scope_like_decompositions
        .iter()
        .filter(|record| {
            record.comparison_highlight || project_context.project_size_tier == "tiny"
        })
        .cloned()
        .collect();

    ScopeClassificationDecompositionDiagnostics {
        project_context,
        scope_like_anchor_count: scope_like_decompositions.len(),
        suspected_false_positive_count,
        failure_mode_counts: failure_mode_counts
            .into_iter()
            .map(|(failure_mode, count)| FailureModeCount { failure_mode, count })
            .collect(),
        scope_like_decompositions,
        comparison_highlights,
    }
}

fn decompose_scope_like_anchor(
    context: &HypothesisContext,
    scope_record: &super::domain_seed_responsibility_scope::AnchorScopeRecord,
    fanout: usize,
    capability_count: usize,
    project: &ProjectStructuralContext,
    primitive_inventory: &PrimitiveRelationInventory,
) -> ScopeLikeAnchorDecomposition {
    let support = &context.merged_support;
    let capability_support = support.capability_keys.len().max(1);

    let partition_spans = PartitionSpanMetrics {
        entrypoint_span: support.entrypoint_ids.len(),
        module_span: support.module_paths.len(),
        package_span: package_span_count(context),
        owner_span: support.owner_classes.len(),
        unit_span: support.unit_ids.len(),
        project_entrypoint_share: share(
            support.entrypoint_ids.len(),
            project.independent_partition_counts.entrypoint_count,
        ),
        project_module_share: share(
            support.module_paths.len(),
            project.independent_partition_counts.module_path_count,
        ),
        project_owner_share: share(
            support.owner_classes.len(),
            project.independent_partition_counts.owner_class_count,
        ),
    };

    let flow_count = flow_ids(context).len();
    let neighborhood_diversity = NeighborhoodDiversityMetrics {
        flow_distinct_count: flow_count,
        flow_diversity: 1.0 - scope_record.flow_neighborhood_concentration,
        flow_concentration: scope_record.flow_neighborhood_concentration,
        call_relation_project_total: primitive_inventory.call_relations,
        flow_relation_project_total: primitive_inventory.flow_relations,
    };

    let entity_resource_total =
        primitive_inventory.entity_relations + primitive_inventory.resource_relations;
    let entity_resource_evidence = evidence_metric(
        scope_record.entity_resource_concentration,
        support.resource_entities.len(),
        entity_resource_total,
        project.primitive_evidence_coverage.entity_resource_sparse,
    );
    let owner_evidence = evidence_metric(
        owner_concentration(support.owner_classes.len(), capability_support),
        support.owner_classes.len(),
        primitive_inventory.owner_relations,
        project.primitive_evidence_coverage.owner_sparse,
    );

    let classification_reasons =
        build_classification_reasons(scope_record, fanout, capability_count, project, &entity_resource_evidence);
    let failure_modes = detect_failure_modes(
        scope_record,
        fanout,
        project,
        &partition_spans,
        &entity_resource_evidence,
    );
    let suspected_false_positive = failure_modes.iter().any(|mode| {
        mode == "ratioInflatedBySmallProject" || mode == "lowAbsoluteFanoutDespiteHighRatio"
    });

    let root = context.representative.root_concept.clone();
    let comparison_highlight = COMPARISON_ROOT_CONCEPTS
        .iter()
        .any(|concept| root.eq_ignore_ascii_case(concept));

    ScopeLikeAnchorDecomposition {
        hypothesis_id: context.hypothesis_id.clone(),
        representative_root_concept: root,
        absolute_capability_support: capability_support,
        retrieval_fanout_capabilities: fanout,
        fanout_ratio: scope_record.fanout_ratio,
        absolute_support_tier: absolute_support_tier(capability_support.max(fanout)),
        partition_spans,
        neighborhood_diversity,
        provenance_diversity: scope_record.provenance_diversity,
        entity_resource_evidence,
        owner_evidence,
        scope_score: scope_record.scope_score,
        responsibility_score: scope_record.responsibility_score,
        classification_reasons,
        suspected_false_positive,
        failure_modes,
        comparison_highlight,
    }
}

fn build_project_context(
    hypothesis_contexts: &[HypothesisContext],
    capability_seeds: &[CapabilityDomainSeeds],
    capability_count: usize,
    primitive_inventory: &PrimitiveRelationInventory,
) -> ProjectStructuralContext {
    let mut entrypoints = BTreeSet::new();
    let mut modules = BTreeSet::new();
    let mut packages = BTreeSet::new();
    let mut owners = BTreeSet::new();
    let mut units = BTreeSet::new();

    for seed in capability_seeds {
        entrypoints.extend(seed.coverage.entrypoint_ids.iter().cloned());
        modules.extend(seed.coverage.module_paths.iter().cloned());
        packages.extend(seed.coverage.package_paths.iter().cloned());
        owners.extend(seed.coverage.owner_classes.iter().cloned());
    }

    for context in hypothesis_contexts {
        entrypoints.extend(context.merged_support.entrypoint_ids.iter().cloned());
        modules.extend(context.merged_support.module_paths.iter().cloned());
        owners.extend(context.merged_support.owner_classes.iter().cloned());
        units.extend(context.merged_support.unit_ids.iter().cloned());
    }

    let entity_resource_total =
        primitive_inventory.entity_relations + primitive_inventory.resource_relations;
    let entity_resource_sparse = entity_resource_total < 5;
    let owner_sparse = primitive_inventory.owner_relations < 5;

    let entity_resource_coverage = normalized_coverage(entity_resource_total, capability_count);
    let owner_coverage = normalized_coverage(primitive_inventory.owner_relations, capability_count);
    let confidence = project_evidence_confidence(entity_resource_sparse, owner_sparse);

    ProjectStructuralContext {
        capability_count,
        eligible_anchor_count: hypothesis_contexts
            .iter()
            .filter(|context| context.domain_anchor_eligible)
            .count(),
        project_size_tier: project_size_tier(capability_count),
        independent_partition_counts: IndependentPartitionCounts {
            entrypoint_count: entrypoints.len(),
            module_path_count: modules.len(),
            package_path_count: packages.len(),
            owner_class_count: owners.len(),
            unit_count: units.len(),
        },
        primitive_evidence_coverage: PrimitiveEvidenceCoverage {
            owner_relations: primitive_inventory.owner_relations,
            entity_relations: primitive_inventory.entity_relations,
            resource_relations: primitive_inventory.resource_relations,
            call_relations: primitive_inventory.call_relations,
            flow_relations: primitive_inventory.flow_relations,
            entity_resource_sparse,
            owner_sparse,
            entity_resource_coverage,
            owner_coverage,
            confidence,
        },
    }
}

fn build_classification_reasons(
    scope_record: &super::domain_seed_responsibility_scope::AnchorScopeRecord,
    fanout: usize,
    capability_count: usize,
    project: &ProjectStructuralContext,
    entity_resource_evidence: &EvidenceCoverageMetric,
) -> Vec<ScopeClassificationReason> {
    let mut reasons = vec![
        ScopeClassificationReason {
            reason: "scopeScoreThreshold".into(),
            value: scope_record.scope_score,
            weighted_contribution: scope_record.scope_score,
            detail: format!("scopeScore={:.3} >= {SCOPE_SCORE_THRESHOLD:.2}", scope_record.scope_score),
        },
        ScopeClassificationReason {
            reason: "responsibilityScoreBelowThreshold".into(),
            value: scope_record.responsibility_score,
            weighted_contribution: scope_record.responsibility_score,
            detail: format!(
                "responsibilityScore={:.3} < {RESPONSIBILITY_SCORE_THRESHOLD:.2}",
                scope_record.responsibility_score
            ),
        },
        ScopeClassificationReason {
            reason: "absoluteCapabilitySupport".into(),
            value: fanout.max(scope_record.fanout_capabilities) as f64,
            weighted_contribution: 0.34 * scope_record.fanout_ratio,
            detail: format!(
                "fanout={}/{} absoluteTier={} projectTier={}",
                fanout.max(scope_record.fanout_capabilities),
                capability_count,
                absolute_support_tier(fanout.max(scope_record.fanout_capabilities)),
                project.project_size_tier,
            ),
        },
        ScopeClassificationReason {
            reason: "fanoutRatio".into(),
            value: scope_record.fanout_ratio,
            weighted_contribution: 0.34 * scope_record.fanout_ratio,
            detail: format!(
                "ratio={:.3} on {}-capability project",
                scope_record.fanout_ratio, capability_count
            ),
        },
        ScopeClassificationReason {
            reason: "capabilityDispersion".into(),
            value: scope_record.capability_dispersion,
            weighted_contribution: 0.24 * scope_record.capability_dispersion,
            detail: format!("capDisp={:.3}", scope_record.capability_dispersion),
        },
        ScopeClassificationReason {
            reason: "entrypointPartitionSpan".into(),
            value: scope_record.entrypoint_dispersion,
            weighted_contribution: 0.14 * scope_record.entrypoint_dispersion,
            detail: format!("entryDisp={:.3}", scope_record.entrypoint_dispersion),
        },
        ScopeClassificationReason {
            reason: "ownerPartitionSpan".into(),
            value: scope_record.owner_dispersion,
            weighted_contribution: 0.10 * scope_record.owner_dispersion,
            detail: format!("ownerDisp={:.3}", scope_record.owner_dispersion),
        },
        ScopeClassificationReason {
            reason: "contractNamespaceBreadth".into(),
            value: scope_record.contract_namespace_breadth,
            weighted_contribution: 0.10 * scope_record.contract_namespace_breadth,
            detail: format!("contractBreadth={:.3}", scope_record.contract_namespace_breadth),
        },
        ScopeClassificationReason {
            reason: "flowNeighborhoodDiversity".into(),
            value: 1.0 - scope_record.flow_neighborhood_concentration,
            weighted_contribution: 0.22 * scope_record.flow_neighborhood_concentration,
            detail: format!(
                "flowConc={:.3} flowDiv={:.3}",
                scope_record.flow_neighborhood_concentration,
                1.0 - scope_record.flow_neighborhood_concentration
            ),
        },
        ScopeClassificationReason {
            reason: "provenanceDiversity".into(),
            value: scope_record.provenance_diversity,
            weighted_contribution: 0.22 * scope_record.provenance_diversity,
            detail: format!("provDiv={:.3}", scope_record.provenance_diversity),
        },
        ScopeClassificationReason {
            reason: "entityResourceEvidenceCoverage".into(),
            value: entity_resource_evidence.coverage,
            weighted_contribution: if entity_resource_evidence.usable_for_responsibility {
                0.28 * scope_record.entity_resource_concentration
            } else {
                0.0
            },
            detail: format!(
                "entityConc={:.3} evidenceCount={} projectTotal={} confidence={} usable={}",
                scope_record.entity_resource_concentration,
                entity_resource_evidence.evidence_count,
                entity_resource_evidence.project_evidence_total,
                entity_resource_evidence.confidence,
                entity_resource_evidence.usable_for_responsibility,
            ),
        },
    ];

    if scope_record.scope_class == SCOPE_CLASS_SCOPE
        && scope_record.responsibility_score < RESPONSIBILITY_SCORE_THRESHOLD
        && !entity_resource_evidence.usable_for_responsibility
        && scope_record.entity_resource_concentration > 0.0
    {
        reasons.push(ScopeClassificationReason {
            reason: "sparseEntityResourceConcentrationIgnored".into(),
            value: scope_record.entity_resource_concentration,
            weighted_contribution: 0.0,
            detail: "high concentration from sparse entity/resource evidence not treated as responsibility".into(),
        });
    }

    reasons
}

fn detect_failure_modes(
    scope_record: &super::domain_seed_responsibility_scope::AnchorScopeRecord,
    fanout: usize,
    project: &ProjectStructuralContext,
    partition_spans: &PartitionSpanMetrics,
    entity_resource_evidence: &EvidenceCoverageMetric,
) -> Vec<String> {
    let mut modes = Vec::new();
    let absolute = fanout.max(scope_record.fanout_capabilities);

    if matches!(project.project_size_tier.as_str(), "tiny" | "small")
        && scope_record.fanout_ratio >= 0.35
        && absolute < 15
    {
        modes.push("ratioInflatedBySmallProject".into());
    }
    if scope_record.fanout_ratio >= 0.35 && absolute < 15 {
        modes.push("lowAbsoluteFanoutDespiteHighRatio".into());
    }
    if absolute >= 50 && scope_record.fanout_ratio >= 0.30 {
        modes.push("genuineBroadAbsoluteFanout".into());
    }
    if project.primitive_evidence_coverage.entity_resource_sparse {
        modes.push("sparseEntityResourceProjectEvidence".into());
    }
    if !entity_resource_evidence.usable_for_responsibility
        && scope_record.entity_resource_concentration > 0.0
    {
        modes.push("unreliableEntityResourceConcentration".into());
    }
    if partition_spans.project_entrypoint_share < 0.20
        && partition_spans.project_module_share < 0.20
        && partition_spans.project_owner_share < 0.20
        && absolute < 50
    {
        modes.push("lowStructuralPartitionSpan".into());
    }
    if scope_record.responsibility_score < RESPONSIBILITY_SCORE_THRESHOLD
        && scope_record.scope_score >= SCOPE_SCORE_THRESHOLD
    {
        modes.push("scopeScoreDominatesResponsibility".into());
    }
    if modes.is_empty() {
        modes.push("scopeLikeWithoutFlaggedFailureMode".into());
    }
    modes
}

fn evidence_metric(
    raw_value: f64,
    evidence_count: usize,
    project_evidence_total: usize,
    project_sparse: bool,
) -> EvidenceCoverageMetric {
    let coverage = if project_evidence_total == 0 {
        0.0
    } else {
        (evidence_count as f64 / project_evidence_total as f64).min(1.0)
    };
    let confidence = if project_sparse || evidence_count == 0 {
        "low"
    } else if evidence_count < 3 {
        "medium"
    } else {
        "high"
    };
    let usable_for_responsibility =
        confidence == "high" || (confidence == "medium" && !project_sparse && raw_value > 0.0);
    EvidenceCoverageMetric {
        raw_value,
        evidence_count,
        project_evidence_total,
        coverage,
        confidence: confidence.into(),
        usable_for_responsibility,
    }
}

fn project_size_tier(capability_count: usize) -> String {
    match capability_count {
        0..=10 => "tiny".into(),
        11..=50 => "small".into(),
        51..=200 => "medium".into(),
        _ => "large".into(),
    }
}

fn absolute_support_tier(absolute: usize) -> String {
    match absolute {
        0..=14 => "narrow".into(),
        15..=49 => "moderate".into(),
        _ => "broad".into(),
    }
}

fn normalized_coverage(total: usize, capability_count: usize) -> f64 {
    if capability_count == 0 {
        0.0
    } else {
        (total as f64 / capability_count as f64).min(1.0)
    }
}

fn project_evidence_confidence(entity_resource_sparse: bool, owner_sparse: bool) -> String {
    if entity_resource_sparse && owner_sparse {
        "low".into()
    } else if entity_resource_sparse || owner_sparse {
        "medium".into()
    } else {
        "high".into()
    }
}

fn share(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        (numerator as f64 / denominator as f64).min(1.0)
    }
}

fn owner_concentration(owner_count: usize, capability_support: usize) -> f64 {
    if owner_count == 0 {
        0.0
    } else {
        (1.0 - (owner_count as f64 / capability_support.max(1) as f64)).clamp(0.0, 1.0)
    }
}

fn package_span_count(context: &HypothesisContext) -> usize {
    context
        .families
        .iter()
        .flat_map(|family| family.distinct_module_paths.iter())
        .collect::<BTreeSet<_>>()
        .len()
}

fn flow_ids(context: &HypothesisContext) -> BTreeSet<String> {
    let mut flows = context.merged_support.flow_ids.clone();
    for family in &context.families {
        flows.extend(family.provenance.flow_ids.iter().cloned());
    }
    flows
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::formation::domain_seed_responsibility_scope::SCOPE_CLASS_MIXED;

    #[test]
    fn narrow_absolute_support는_small_project에서_false_positive로_표시된다() {
        let project = ProjectStructuralContext {
            capability_count: 6,
            eligible_anchor_count: 8,
            project_size_tier: "tiny".into(),
            independent_partition_counts: IndependentPartitionCounts {
                entrypoint_count: 4,
                module_path_count: 6,
                package_path_count: 3,
                owner_class_count: 9,
                unit_count: 8,
            },
            primitive_evidence_coverage: PrimitiveEvidenceCoverage {
                entity_resource_sparse: true,
                owner_sparse: false,
                entity_resource_coverage: 0.33,
                owner_coverage: 1.5,
                confidence: "medium".into(),
                ..Default::default()
            },
        };
        let scope_record = super::super::domain_seed_responsibility_scope::AnchorScopeRecord {
            hypothesis_id: "hypothesis:test".into(),
            representative_root_concept: "files".into(),
            scope_class: SCOPE_CLASS_SCOPE.into(),
            fanout_capabilities: 3,
            fanout_ratio: 0.5,
            capability_dispersion: 0.5,
            entrypoint_dispersion: 0.33,
            owner_dispersion: 0.33,
            unit_dispersion: 0.33,
            contract_namespace_breadth: 0.5,
            entity_resource_concentration: 0.0,
            flow_neighborhood_concentration: 0.0,
            provenance_diversity: 0.9,
            scope_score: 0.55,
            responsibility_score: 0.25,
        };
        let modes = detect_failure_modes(
            &scope_record,
            3,
            &project,
            &PartitionSpanMetrics {
                entrypoint_span: 1,
                module_span: 2,
                owner_span: 2,
                project_entrypoint_share: 0.25,
                project_module_share: 0.33,
                project_owner_share: 0.22,
                ..Default::default()
            },
            &EvidenceCoverageMetric {
                raw_value: 0.0,
                evidence_count: 0,
                project_evidence_total: 2,
                coverage: 0.0,
                confidence: "low".into(),
                usable_for_responsibility: false,
            },
        );
        assert!(modes.contains(&"ratioInflatedBySmallProject".into()));
        assert!(modes.contains(&"lowAbsoluteFanoutDespiteHighRatio".into()));
    }

    #[test]
    fn broad_absolute_fanout는_genuine_broad_scope로_분류된다() {
        let project = ProjectStructuralContext {
            capability_count: 295,
            project_size_tier: "large".into(),
            ..Default::default()
        };
        let scope_record = super::super::domain_seed_responsibility_scope::AnchorScopeRecord {
            scope_class: SCOPE_CLASS_SCOPE.into(),
            fanout_capabilities: 291,
            fanout_ratio: 0.986,
            scope_score: 0.90,
            responsibility_score: 0.20,
            ..Default::default()
        };
        let modes = detect_failure_modes(
            &scope_record,
            291,
            &project,
            &PartitionSpanMetrics::default(),
            &EvidenceCoverageMetric {
                usable_for_responsibility: false,
                confidence: "low".into(),
                ..Default::default()
            },
        );
        assert!(modes.contains(&"genuineBroadAbsoluteFanout".into()));
    }

    #[test]
    fn sparse_entity_resource_concentration은_responsibility_evidence로_쓰이지_않는다() {
        let metric = evidence_metric(0.8, 1, 2, true);
        assert!(!metric.usable_for_responsibility);
        assert_eq!(metric.confidence, "low");
    }

    #[test]
    fn comparison_highlight는_관심_root_concept에_대해_설정된다() {
        assert!(COMPARISON_ROOT_CONCEPTS.contains(&"resolvers"));
        assert!(COMPARISON_ROOT_CONCEPTS.contains(&"serverpodauth"));
    }

    #[test]
    fn responsibility_like_anchor는_decomposition에_포함되지_않는다() {
        let record = super::super::domain_seed_responsibility_scope::AnchorScopeRecord {
            scope_class: super::super::domain_seed_responsibility_scope::SCOPE_CLASS_RESPONSIBILITY.into(),
            ..Default::default()
        };
        assert_ne!(record.scope_class, SCOPE_CLASS_SCOPE);
    }

    #[test]
    fn mixed_scope_class도_decomposition에서_제외된다() {
        let record = super::super::domain_seed_responsibility_scope::AnchorScopeRecord {
            scope_class: SCOPE_CLASS_MIXED.into(),
            ..Default::default()
        };
        assert_ne!(record.scope_class, SCOPE_CLASS_SCOPE);
    }
}
