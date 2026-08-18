//! Structural scope vs cross-cutting responsibility diagnostics for broad anchors.

use super::domain_seed_anchor_eligibility::{HypothesisContext, MergedHypothesisSupport};
use super::domain_seed_anchor_affinity::AnchorCapabilityEdge;
use super::domain_seed_diagnostics::CapabilityDomainSeeds;
use super::domain_seed_provenance::PrimitiveRelationInventory;
use super::domain_seed_responsibility_scope::{diagnose_anchor_scope, SCOPE_CLASS_SCOPE};
use super::domain_seed_scope_classification_decomposition::{
    ProjectStructuralContext, ScopeClassificationDecompositionDiagnostics,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const ROLE_STRUCTURAL_SCOPE: &str = "structuralScope";
pub const ROLE_CROSS_CUTTING_RESPONSIBILITY: &str = "crossCuttingResponsibility";
pub const ROLE_UNKNOWN: &str = "unknown";

const COMPARISON_ROOT_CONCEPTS: &[&str] = &[
    "resolvers",
    "adminapi",
    "admin",
    "auth",
    "endpoints",
    "serverpodauth",
];

const STRUCTURAL_ROLE_THRESHOLD: f64 = 0.55;
const RESPONSIBILITY_ROLE_THRESHOLD: f64 = 0.45;
const BROAD_ABSOLUTE_FANOUT_MIN: usize = 15;
const BROAD_FANOUT_RATIO_MIN: f64 = 0.25;
const NARROW_ABSOLUTE_SUPPORT_MAX: usize = 14;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ScopeRoleDiagnostics {
    pub broad_anchor_count: usize,
    pub role_class_counts: Vec<ScopeRoleClassCount>,
    pub anchor_role_records: Vec<BroadAnchorRoleRecord>,
    pub comparison_highlights: Vec<BroadAnchorRoleRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeRoleClassCount {
    pub scope_role_class: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadAnchorRoleRecord {
    pub hypothesis_id: String,
    pub representative_root_concept: String,
    pub scope_class: String,
    pub scope_role_class: String,
    pub retrieval_fanout_capabilities: usize,
    pub fanout_ratio: f64,
    pub absolute_support_tier: String,
    pub suspected_false_positive: bool,
    pub mediation_metrics: GraphMediationMetrics,
    pub classification_reasons: Vec<ScopeRoleReason>,
    pub comparison_highlight: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GraphMediationMetrics {
    pub partition_traversal: f64,
    pub capability_partition_traversal: f64,
    pub entrypoint_partition_traversal: f64,
    pub owner_partition_traversal: f64,
    pub module_partition_traversal: f64,
    pub neighborhood_cohesion: f64,
    pub flow_neighborhood_cohesion: f64,
    pub independent_evidence_score: f64,
    pub independent_evidence_group_count: usize,
    pub has_independent_ownership_state_behavior: bool,
    pub contract_evidence_count: usize,
    pub entity_resource_evidence_count: usize,
    pub common_container_overlap: f64,
    pub shared_anchor_count: usize,
    pub structural_role_score: f64,
    pub responsibility_role_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeRoleReason {
    pub reason: String,
    pub value: f64,
    pub detail: String,
}

#[derive(Debug, Clone, Default)]
struct CapabilityPartitions {
    by_capability: BTreeMap<String, CapabilityPartitionSlice>,
    project_contract_path_count: usize,
}

#[derive(Debug, Clone, Default)]
struct CapabilityPartitionSlice {
    entrypoints: BTreeSet<String>,
    owners: BTreeSet<String>,
    modules: BTreeSet<String>,
    contract_paths: BTreeSet<String>,
}

pub fn build_scope_role_diagnostics(
    hypothesis_contexts: &[HypothesisContext],
    edges: &[AnchorCapabilityEdge],
    capability_seeds: &[CapabilityDomainSeeds],
    scope_decomposition: &ScopeClassificationDecompositionDiagnostics,
    primitive_inventory: &PrimitiveRelationInventory,
) -> ScopeRoleDiagnostics {
    let project = &scope_decomposition.project_context;
    let capability_count = project.capability_count;
    let capability_partitions = build_capability_partitions(capability_seeds);
    let fanout_by_hypothesis = hypothesis_fanout(edges);
    let fanout_capabilities_by_hypothesis = hypothesis_fanout_capabilities(edges);
    let decomposition_by_hypothesis = scope_decomposition
        .scope_like_decompositions
        .iter()
        .map(|record| (record.hypothesis_id.as_str(), record))
        .collect::<BTreeMap<_, _>>();

    let eligible_contexts = hypothesis_contexts
        .iter()
        .filter(|context| context.domain_anchor_eligible)
        .collect::<Vec<_>>();

    let mut anchor_role_records = Vec::new();
    for context in &eligible_contexts {
        let fanout = fanout_by_hypothesis
            .get(&context.hypothesis_id)
            .copied()
            .unwrap_or(0);
        let scope_record = diagnose_anchor_scope(context, capability_count, fanout);
        let decomposition = decomposition_by_hypothesis.get(context.hypothesis_id.as_str());
        let suspected_false_positive = decomposition
            .map(|record| record.suspected_false_positive)
            .unwrap_or(false);

        if !is_broad_anchor(&scope_record, fanout) {
            continue;
        }

        let fanout_capabilities = fanout_capabilities_by_hypothesis
            .get(&context.hypothesis_id)
            .cloned()
            .unwrap_or_default();
        let mediation_metrics = compute_mediation_metrics(
            context,
            &scope_record,
            &fanout_capabilities,
            &capability_partitions,
            project,
            primitive_inventory,
            &eligible_contexts,
        );
        let scope_role_class = classify_scope_role(
            context,
            fanout,
            project,
            suspected_false_positive,
            &mediation_metrics,
        );
        let classification_reasons = build_role_reasons(
            &scope_role_class,
            fanout,
            capability_count,
            suspected_false_positive,
            project,
            &mediation_metrics,
        );
        let root = context.representative.root_concept.clone();
        let comparison_highlight = COMPARISON_ROOT_CONCEPTS
            .iter()
            .any(|concept| root.eq_ignore_ascii_case(concept))
            || (project.project_size_tier == "tiny" && suspected_false_positive);

        anchor_role_records.push(BroadAnchorRoleRecord {
            hypothesis_id: context.hypothesis_id.clone(),
            representative_root_concept: root,
            scope_class: scope_record.scope_class,
            scope_role_class,
            retrieval_fanout_capabilities: fanout,
            fanout_ratio: scope_record.fanout_ratio,
            absolute_support_tier: absolute_support_tier(fanout.max(
                context.merged_support.capability_keys.len(),
            )),
            suspected_false_positive,
            mediation_metrics,
            classification_reasons,
            comparison_highlight,
        });
    }

    anchor_role_records.sort_by(|left, right| {
        right
            .fanout_ratio
            .partial_cmp(&left.fanout_ratio)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                left.representative_root_concept
                    .cmp(&right.representative_root_concept)
            })
    });

    let mut role_class_counts: BTreeMap<String, usize> = BTreeMap::new();
    for record in &anchor_role_records {
        *role_class_counts
            .entry(record.scope_role_class.clone())
            .or_default() += 1;
    }

    let comparison_highlights = anchor_role_records
        .iter()
        .filter(|record| record.comparison_highlight)
        .cloned()
        .collect();

    ScopeRoleDiagnostics {
        broad_anchor_count: anchor_role_records.len(),
        role_class_counts: role_class_counts
            .into_iter()
            .map(|(scope_role_class, count)| ScopeRoleClassCount {
                scope_role_class,
                count,
            })
            .collect(),
        anchor_role_records,
        comparison_highlights,
    }
}

fn is_broad_anchor(
    scope_record: &super::domain_seed_responsibility_scope::AnchorScopeRecord,
    fanout: usize,
) -> bool {
    scope_record.scope_class == SCOPE_CLASS_SCOPE
        || (fanout >= BROAD_ABSOLUTE_FANOUT_MIN && scope_record.fanout_ratio >= BROAD_FANOUT_RATIO_MIN)
}

fn compute_mediation_metrics(
    context: &HypothesisContext,
    scope_record: &super::domain_seed_responsibility_scope::AnchorScopeRecord,
    fanout_capabilities: &BTreeSet<String>,
    capability_partitions: &CapabilityPartitions,
    project: &ProjectStructuralContext,
    primitive_inventory: &PrimitiveRelationInventory,
    eligible_contexts: &[&HypothesisContext],
) -> GraphMediationMetrics {
    let capability_partition_traversal =
        partition_traversal_from_capabilities(fanout_capabilities, capability_partitions, project);
    let entrypoint_partition_traversal = share(
        context.merged_support.entrypoint_ids.len(),
        project.independent_partition_counts.entrypoint_count,
    );
    let owner_partition_traversal = share(
        context.merged_support.owner_classes.len(),
        project.independent_partition_counts.owner_class_count,
    );
    let module_partition_traversal = share(
        context.merged_support.module_paths.len(),
        project.independent_partition_counts.module_path_count,
    );
    let partition_traversal = average(&[
        capability_partition_traversal,
        entrypoint_partition_traversal,
        owner_partition_traversal,
        module_partition_traversal,
    ]);

    let flow_neighborhood_cohesion = scope_record.flow_neighborhood_concentration;
    let independent_evidence_group_count = independent_evidence_group_count(context);
    let flow_density = if primitive_inventory.flow_relations == 0 {
        0.0
    } else {
        (flow_ids(context).len() as f64 / primitive_inventory.flow_relations as f64).min(1.0)
    };
    let ownership_component = if context.has_independent_ownership_state_behavior {
        0.25
    } else {
        0.0
    };
    let neighborhood_cohesion = (0.45 * flow_neighborhood_cohesion
        + 0.30 * (independent_evidence_group_count as f64 / 4.0).min(1.0)
        + 0.15 * flow_density
        + ownership_component)
        .min(1.0);

    let contract_evidence_count = contract_paths(context).len();
    let entity_resource_evidence_count = context.merged_support.resource_entities.len();
    let contract_specificity = if contract_evidence_count == 0 {
        0.0
    } else {
        1.0 - scope_record.contract_namespace_breadth
    };
    let entity_component = if entity_resource_evidence_count == 0 {
        0.0
    } else {
        scope_record.entity_resource_concentration
    };
    let independent_evidence_score = (ownership_component
        + (independent_evidence_group_count as f64 / 5.0).min(0.30)
        + contract_specificity * 0.20
        + entity_component * 0.25
        + scope_record.provenance_diversity * 0.20)
        .min(1.0);

    let (common_container_overlap, shared_anchor_count) =
        common_container_metrics(context, eligible_contexts);

    let structural_role_score = (0.35 * partition_traversal
        + 0.25 * common_container_overlap
        + 0.15 * scope_record.contract_namespace_breadth
        + 0.15 * (1.0 - neighborhood_cohesion)
        + 0.10 * (1.0 - independent_evidence_score))
        .clamp(0.0, 1.0);
    let responsibility_role_score = (0.35 * neighborhood_cohesion
        + 0.30 * independent_evidence_score
        + 0.20 * scope_record.responsibility_score
        + 0.15 * (1.0 - common_container_overlap))
        .clamp(0.0, 1.0);

    GraphMediationMetrics {
        partition_traversal,
        capability_partition_traversal,
        entrypoint_partition_traversal,
        owner_partition_traversal,
        module_partition_traversal,
        neighborhood_cohesion,
        flow_neighborhood_cohesion,
        independent_evidence_score,
        independent_evidence_group_count,
        has_independent_ownership_state_behavior: context.has_independent_ownership_state_behavior,
        contract_evidence_count,
        entity_resource_evidence_count,
        common_container_overlap,
        shared_anchor_count,
        structural_role_score,
        responsibility_role_score,
    }
}

fn classify_scope_role(
    context: &HypothesisContext,
    fanout: usize,
    project: &ProjectStructuralContext,
    suspected_false_positive: bool,
    metrics: &GraphMediationMetrics,
) -> String {
    let absolute = fanout.max(context.merged_support.capability_keys.len());
    if suspected_false_positive
        || (matches!(project.project_size_tier.as_str(), "tiny" | "small")
            && absolute <= NARROW_ABSOLUTE_SUPPORT_MAX)
    {
        return ROLE_UNKNOWN.into();
    }

    if metrics.structural_role_score >= STRUCTURAL_ROLE_THRESHOLD
        && metrics.responsibility_role_score < RESPONSIBILITY_ROLE_THRESHOLD
        && absolute > NARROW_ABSOLUTE_SUPPORT_MAX
    {
        ROLE_STRUCTURAL_SCOPE.into()
    } else if metrics.responsibility_role_score >= RESPONSIBILITY_ROLE_THRESHOLD
        && metrics.structural_role_score < STRUCTURAL_ROLE_THRESHOLD
    {
        ROLE_CROSS_CUTTING_RESPONSIBILITY.into()
    } else {
        ROLE_UNKNOWN.into()
    }
}

fn build_role_reasons(
    scope_role_class: &str,
    fanout: usize,
    capability_count: usize,
    suspected_false_positive: bool,
    project: &ProjectStructuralContext,
    metrics: &GraphMediationMetrics,
) -> Vec<ScopeRoleReason> {
    let mut reasons = vec![
        ScopeRoleReason {
            reason: "partitionTraversal".into(),
            value: metrics.partition_traversal,
            detail: format!(
                "cap={:.3} entry={:.3} owner={:.3} module={:.3}",
                metrics.capability_partition_traversal,
                metrics.entrypoint_partition_traversal,
                metrics.owner_partition_traversal,
                metrics.module_partition_traversal,
            ),
        },
        ScopeRoleReason {
            reason: "neighborhoodCohesion".into(),
            value: metrics.neighborhood_cohesion,
            detail: format!(
                "flowCohesion={:.3} independentGroups={}",
                metrics.flow_neighborhood_cohesion, metrics.independent_evidence_group_count
            ),
        },
        ScopeRoleReason {
            reason: "independentEvidence".into(),
            value: metrics.independent_evidence_score,
            detail: format!(
                "ownershipBehavior={} contracts={} entities/resources={}",
                metrics.has_independent_ownership_state_behavior,
                metrics.contract_evidence_count,
                metrics.entity_resource_evidence_count,
            ),
        },
        ScopeRoleReason {
            reason: "commonContainerOverlap".into(),
            value: metrics.common_container_overlap,
            detail: format!("sharedAnchors={}", metrics.shared_anchor_count),
        },
        ScopeRoleReason {
            reason: "structuralRoleScore".into(),
            value: metrics.structural_role_score,
            detail: format!("threshold={STRUCTURAL_ROLE_THRESHOLD:.2}"),
        },
        ScopeRoleReason {
            reason: "responsibilityRoleScore".into(),
            value: metrics.responsibility_role_score,
            detail: format!("threshold={RESPONSIBILITY_ROLE_THRESHOLD:.2}"),
        },
        ScopeRoleReason {
            reason: "absoluteFanout".into(),
            value: fanout as f64,
            detail: format!("fanout={fanout}/{capability_count} tier={}", project.project_size_tier),
        },
    ];

    if suspected_false_positive {
        reasons.push(ScopeRoleReason {
            reason: "suspectedFalsePositive".into(),
            value: 1.0,
            detail: "ratio inflation on small project; structuralScope not assigned".into(),
        });
    }

    reasons.push(ScopeRoleReason {
        reason: "assignedScopeRole".into(),
        value: 1.0,
        detail: scope_role_class.into(),
    });

    reasons
}

fn build_capability_partitions(capability_seeds: &[CapabilityDomainSeeds]) -> CapabilityPartitions {
    let mut by_capability = BTreeMap::new();
    let mut project_contract_paths = BTreeSet::new();
    for seed in capability_seeds {
        by_capability.insert(
            seed.capability_key.clone(),
            CapabilityPartitionSlice {
                entrypoints: seed.coverage.entrypoint_ids.iter().cloned().collect(),
                owners: seed.coverage.owner_classes.iter().cloned().collect(),
                modules: seed.coverage.module_paths.iter().cloned().collect(),
                contract_paths: seed.coverage.contract_paths.iter().cloned().collect(),
            },
        );
        project_contract_paths.extend(seed.coverage.contract_paths.iter().cloned());
    }
    CapabilityPartitions {
        by_capability,
        project_contract_path_count: project_contract_paths.len(),
    }
}

fn partition_traversal_from_capabilities(
    fanout_capabilities: &BTreeSet<String>,
    capability_partitions: &CapabilityPartitions,
    project: &ProjectStructuralContext,
) -> f64 {
    let mut entrypoints = BTreeSet::new();
    let mut owners = BTreeSet::new();
    let mut modules = BTreeSet::new();
    let mut contracts = BTreeSet::new();
    for capability in fanout_capabilities {
        if let Some(slice) = capability_partitions.by_capability.get(capability) {
            entrypoints.extend(slice.entrypoints.iter().cloned());
            owners.extend(slice.owners.iter().cloned());
            modules.extend(slice.modules.iter().cloned());
            contracts.extend(slice.contract_paths.iter().cloned());
        }
    }
    average(&[
        share(entrypoints.len(), project.independent_partition_counts.entrypoint_count),
        share(owners.len(), project.independent_partition_counts.owner_class_count),
        share(modules.len(), project.independent_partition_counts.module_path_count),
        share(contracts.len(), capability_partitions.project_contract_path_count.max(1)),
    ])
}

fn common_container_metrics(
    context: &HypothesisContext,
    eligible_contexts: &[&HypothesisContext],
) -> (f64, usize) {
    let mut overlaps = Vec::new();
    let mut shared_anchor_count = 0usize;
    for other in eligible_contexts {
        if other.hypothesis_id == context.hypothesis_id {
            continue;
        }
        let overlap = partition_overlap(&context.merged_support, &other.merged_support);
        overlaps.push(overlap);
        if overlap >= 0.20 {
            shared_anchor_count += 1;
        }
    }
    let common_container_overlap = if overlaps.is_empty() {
        0.0
    } else {
        overlaps.iter().sum::<f64>() / overlaps.len() as f64
    };
    (common_container_overlap, shared_anchor_count)
}

fn partition_overlap(left: &MergedHypothesisSupport, right: &MergedHypothesisSupport) -> f64 {
    average(&[
        jaccard(&left.entrypoint_ids, &right.entrypoint_ids),
        jaccard(&left.module_paths, &right.module_paths),
        jaccard(&left.owner_classes, &right.owner_classes),
        jaccard(&left.unit_ids, &right.unit_ids),
    ])
}

fn jaccard<T: Ord + Clone>(left: &BTreeSet<T>, right: &BTreeSet<T>) -> f64 {
    if left.is_empty() && right.is_empty() {
        return 0.0;
    }
    let intersection = left.intersection(right).count();
    let union = left.union(right).count().max(1);
    intersection as f64 / union as f64
}

fn independent_evidence_group_count(context: &HypothesisContext) -> usize {
    context
        .families
        .iter()
        .flat_map(|family| family.independent_evidence_groups.iter())
        .map(|group| group.group_id.as_str())
        .collect::<BTreeSet<_>>()
        .len()
}

fn contract_paths(context: &HypothesisContext) -> BTreeSet<String> {
    context
        .families
        .iter()
        .flat_map(|family| family.distinct_contract_paths.iter().cloned())
        .collect()
}

fn flow_ids(context: &HypothesisContext) -> BTreeSet<String> {
    let mut flows = context.merged_support.flow_ids.clone();
    for family in &context.families {
        flows.extend(family.provenance.flow_ids.iter().cloned());
    }
    flows
}

fn hypothesis_fanout(edges: &[AnchorCapabilityEdge]) -> BTreeMap<String, usize> {
    hypothesis_fanout_capabilities(edges)
        .into_iter()
        .map(|(hypothesis_id, capabilities)| (hypothesis_id, capabilities.len()))
        .collect()
}

fn hypothesis_fanout_capabilities(
    edges: &[AnchorCapabilityEdge],
) -> BTreeMap<String, BTreeSet<String>> {
    let mut fanout: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for edge in edges {
        fanout
            .entry(edge.hypothesis_id.clone())
            .or_default()
            .insert(edge.capability_key.clone());
    }
    fanout
}

fn absolute_support_tier(absolute: usize) -> String {
    match absolute {
        0..=14 => "narrow".into(),
        15..=49 => "moderate".into(),
        _ => "broad".into(),
    }
}

fn share(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        (numerator as f64 / denominator as f64).min(1.0)
    }
}

fn average(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::formation::domain_seed_anchor_eligibility::{
        DiagnosticFamilyInclusion, MergedHypothesisSupport,
    };
    use crate::domain::formation::domain_seed_aggregation::{
        IdfPenaltyDiagnostic, RankedConceptFamily,
    };
    use crate::domain::formation::domain_seed_provenance::{
        FamilyProvenance, FamilySupportSignature, SeedHypothesisGroup,
    };
    use crate::domain::formation::domain_seed_recovery::AnchorScoreComponents;
    use crate::domain::formation::domain_seed_role_graph::ConceptRoleDiagnostic;
    use crate::domain::formation::domain_seed_scope_classification_decomposition::{
        IndependentPartitionCounts, PrimitiveEvidenceCoverage, ProjectStructuralContext,
    };

    fn sample_context(root: &str, capability_keys: &[&str]) -> HypothesisContext {
        let capabilities = capability_keys
            .iter()
            .map(|cap| (*cap).to_string())
            .collect::<BTreeSet<_>>();
        let family = RankedConceptFamily {
            rank: 1,
            root_concept: root.into(),
            child_concepts: Vec::new(),
            atomized_path: root.into(),
            distinct_capabilities: capabilities.len(),
            distinct_capability_keys: capabilities.iter().cloned().collect(),
            distinct_entrypoints: 1,
            distinct_entrypoint_ids: vec!["ep-1".into()],
            distinct_contracts: 1,
            distinct_contract_paths: vec!["/api".into()],
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
                normalized_root_concept: root.into(),
                normalization_diagnostics: Vec::new(),
            },
            anchor_score_components: AnchorScoreComponents::default(),
            provenance: FamilyProvenance::default(),
            support_signature: FamilySupportSignature::default(),
        };
        HypothesisContext {
            hypothesis_id: format!("hypothesis:{root}"),
            group: SeedHypothesisGroup {
                group_id: root.into(),
                signature_key: format!("sig:{root}"),
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
            merged_support: MergedHypothesisSupport {
                capability_keys: capabilities,
                entrypoint_ids: BTreeSet::from(["ep-1".into()]),
                owner_classes: BTreeSet::from(["Owner".into()]),
                unit_ids: BTreeSet::from(["unit-1".into()]),
                module_paths: BTreeSet::from(["app.module".into()]),
                resource_entities: BTreeSet::new(),
                flow_ids: BTreeSet::new(),
                root_concepts: BTreeSet::from([root.into()]),
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

    fn tiny_project() -> ProjectStructuralContext {
        ProjectStructuralContext {
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
                confidence: "medium".into(),
                ..Default::default()
            },
        }
    }

    #[test]
    fn narrow_small_project_anchor는_structuralScope로_확정되지_않는다() {
        let metrics = GraphMediationMetrics {
            structural_role_score: 0.80,
            responsibility_role_score: 0.20,
            ..Default::default()
        };
        let role = classify_scope_role(
            &sample_context("files", &["cap-1", "cap-2", "cap-3"]),
            3,
            &tiny_project(),
            true,
            &metrics,
        );
        assert_eq!(role, ROLE_UNKNOWN);
    }

    #[test]
    fn genuine_broad_fanout는_structuralScope가_될_수_있다() {
        let metrics = GraphMediationMetrics {
            partition_traversal: 0.90,
            common_container_overlap: 0.75,
            neighborhood_cohesion: 0.10,
            independent_evidence_score: 0.10,
            structural_role_score: 0.72,
            responsibility_role_score: 0.28,
            ..Default::default()
        };
        let role = classify_scope_role(
            &sample_context("resolvers", &["cap-1"]),
            291,
            &ProjectStructuralContext {
                capability_count: 295,
                project_size_tier: "large".into(),
                ..Default::default()
            },
            false,
            &metrics,
        );
        assert_eq!(role, ROLE_STRUCTURAL_SCOPE);
    }

    #[test]
    fn cohesive_independent_evidence는_crossCuttingResponsibility가_될_수_있다() {
        let mut context = sample_context("auth", &["cap-1", "cap-2", "cap-3"]);
        context.has_independent_ownership_state_behavior = true;
        context.merged_support.flow_ids.insert("flow-1".into());
        let metrics = GraphMediationMetrics {
            partition_traversal: 0.35,
            common_container_overlap: 0.20,
            neighborhood_cohesion: 0.70,
            independent_evidence_score: 0.65,
            structural_role_score: 0.30,
            responsibility_role_score: 0.68,
            ..Default::default()
        };
        let role = classify_scope_role(
            &context,
            19,
            &ProjectStructuralContext {
                capability_count: 46,
                project_size_tier: "small".into(),
                ..Default::default()
            },
            false,
            &metrics,
        );
        assert_eq!(role, ROLE_CROSS_CUTTING_RESPONSIBILITY);
    }

    #[test]
    fn broad_anchor는_scopeLike_또는_높은_absolute_fanout이다() {
        let scope_record = super::super::domain_seed_responsibility_scope::AnchorScopeRecord {
            scope_class: SCOPE_CLASS_SCOPE.into(),
            fanout_ratio: 0.5,
            ..Default::default()
        };
        assert!(is_broad_anchor(&scope_record, 3));

        let mixed = super::super::domain_seed_responsibility_scope::AnchorScopeRecord {
            scope_class: "mixed/unknown".into(),
            fanout_ratio: 0.87,
            ..Default::default()
        };
        assert!(is_broad_anchor(&mixed, 257));
    }
}
