//! Business Domain Recovery Algorithm v0.1 diagnostics.

use super::domain_seed_diagnostics::{CapabilityDomainSeeds, DomainSeedRawEvidence};
use super::domain_seed_aggregation::RankedConceptFamily;
use super::domain_seed_role_graph::{
    classify_relation, edge_evidence, family_id, family_matches_candidate, module_tail_segment,
    owner_business_stem, package_tail_segment, SeedCandidateGraph, SeedGraphEdgeEvidence,
};
use super::key_decomposition::{
    atomize_concept_label, decompose_capability_key, normalized_root_concept,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const POSITIVE_COMPONENTS: &[&str] = &[
    "coverage",
    "coherence",
    "entityness",
    "ownershipAlignment",
    "specificity",
    "independentEvidence",
];
const NEGATIVE_COMPONENTS: &[&str] = &[
    "actionness",
    "effectiveContextDispersion",
    "noise",
];
const STRONG_EVIDENCE_GROUPS: &[&str] = &[
    "structural",
    "ownership",
    "stateResource",
    "behavior",
];
const TRANSPORT_CONTRACT_PREFIXES: &[&str] = &[
    "api", "v1", "v2", "v3", "rpc", "ws", "graphql", "public", "internal", "admin-api",
    "shop-api", "adminapi", "shopapi",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FamilyGraphDiagnostics {
    pub seed_candidate_graph: SeedCandidateGraph,
    pub sparse_seed_candidate_graph: SparseSeedCandidateGraph,
    pub provenance_seed_candidate_graph: super::domain_seed_provenance::ProvenanceSeedCandidateGraph,
    pub domain_anchor_eligibility: super::domain_seed_anchor_eligibility::DomainAnchorEligibilityDiagnostics,
    pub anchor_capability_graph: super::domain_seed_anchor_affinity::AnchorCapabilityGraph,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorScoreComponent {
    pub value: f64,
    pub weight: f64,
    pub polarity: String,
    pub contribution: f64,
    pub signed_contribution: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AnchorScoreComponents {
    pub symbolic_weights: BTreeMap<String, f64>,
    pub components: BTreeMap<String, AnchorScoreComponent>,
    pub symbolic_total: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SparseSeedCandidateGraph {
    pub node_count: usize,
    pub edge_count: usize,
    pub graph_density: f64,
    pub anchor_node_count: usize,
    pub eligible_node_count: usize,
    pub explicit_anchor_count: usize,
    pub ambiguous_node_count: usize,
    pub excluded_action_cross_cutting_count: usize,
    pub excluded_action_cross_cutting: Vec<String>,
    pub nodes: Vec<SparseSeedGraphNode>,
    pub edges: Vec<SparseSeedGraphEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SparseSeedGraphNode {
    pub family_id: String,
    pub root_concept: String,
    pub normalized_root_concept: String,
    pub rank: usize,
    pub role_class: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SparseSeedGraphEdge {
    pub from_family_id: String,
    pub to_family_id: String,
    pub relation: String,
    pub creation_reason: String,
    pub independent_evidence_channels: Vec<String>,
    pub independent_evidence_groups: Vec<String>,
    pub weak_evidence_groups: Vec<String>,
    pub evidence: SeedGraphEdgeEvidence,
}

#[derive(Debug, Clone)]
struct SparseEdgeEvidenceAnalysis {
    raw_channels: Vec<String>,
    independent_evidence_groups: Vec<String>,
    weak_evidence_groups: Vec<String>,
}

pub fn enrich_bdr_recovery(
    families: &mut [RankedConceptFamily],
    capability_seeds: &[CapabilityDomainSeeds],
) {
    for family in families.iter_mut() {
        let alignment = compute_business_root_alignment(family, capability_seeds);
        let raw_dispersion = family.concept_role.context_dispersion;
        family.concept_role.business_root_alignment = alignment;
        family.concept_role.effective_context_dispersion =
            raw_dispersion * (1.0 - alignment).clamp(0.0, 1.0);
        family.anchor_score_components = build_anchor_score_components(family);
    }
}

pub fn build_sparse_seed_candidate_graph(
    families: &[RankedConceptFamily],
) -> SparseSeedCandidateGraph {
    let explicit_anchor_count = families
        .iter()
        .filter(|family| family.concept_role.role_class == "anchor")
        .count();
    let ambiguous_node_count = families
        .iter()
        .filter(|family| family.concept_role.role_class == "ambiguous")
        .count();
    let excluded_action_cross_cutting = families
        .iter()
        .filter(|family| family.concept_role.role_class == "actionCrossCutting")
        .map(|family| family.root_concept.clone())
        .collect::<Vec<_>>();
    let excluded_action_cross_cutting_count = excluded_action_cross_cutting.len();

    let eligible_families = families
        .iter()
        .filter(|family| family.concept_role.role_class != "actionCrossCutting")
        .collect::<Vec<_>>();
    let eligible_node_count = eligible_families.len();

    let nodes = eligible_families
        .iter()
        .map(|family| SparseSeedGraphNode {
            family_id: family_id(family),
            root_concept: family.root_concept.clone(),
            normalized_root_concept: family
                .concept_role
                .normalized_root_concept
                .clone(),
            rank: family.rank,
            role_class: family.concept_role.role_class.clone(),
        })
        .collect::<Vec<_>>();

    let mut edges = Vec::new();
    for left in &eligible_families {
        for right in &eligible_families {
            if left.root_concept == right.root_concept {
                continue;
            }
            let evidence = edge_evidence(left, right);
            if let Some((relation, reason, analysis)) =
                classify_sparse_relation(left, right, &evidence)
            {
                edges.push(SparseSeedGraphEdge {
                    from_family_id: family_id(left),
                    to_family_id: family_id(right),
                    relation,
                    creation_reason: reason,
                    independent_evidence_channels: analysis.raw_channels,
                    independent_evidence_groups: analysis.independent_evidence_groups,
                    weak_evidence_groups: analysis.weak_evidence_groups,
                    evidence,
                });
            }
        }
    }
    edges.sort_by(|left, right| {
        left.from_family_id
            .cmp(&right.from_family_id)
            .then_with(|| left.to_family_id.cmp(&right.to_family_id))
            .then_with(|| left.relation.cmp(&right.relation))
    });

    let node_count = nodes.len();
    let edge_count = edges.len();
    let graph_density = if node_count <= 1 {
        0.0
    } else {
        edge_count as f64 / (node_count as f64 * (node_count as f64 - 1.0))
    };

    SparseSeedCandidateGraph {
        node_count,
        edge_count,
        graph_density,
        anchor_node_count: eligible_node_count,
        eligible_node_count,
        explicit_anchor_count,
        ambiguous_node_count,
        excluded_action_cross_cutting_count,
        excluded_action_cross_cutting,
        nodes,
        edges,
    }
}

fn compute_business_root_alignment(
    family: &RankedConceptFamily,
    capability_seeds: &[CapabilityDomainSeeds],
) -> f64 {
    let root = &family.concept_role.normalized_root_concept;
    let mut aligned = 0usize;
    let mut total = 0usize;

    for owner in &family.distinct_owner_classes {
        total += 1;
        if stem_aligns_with_root(&owner_business_stem(owner), root) {
            aligned += 1;
        }
    }
    for module in &family.distinct_module_paths {
        total += 1;
        if stem_aligns_with_root(&module_tail_segment(module), root) {
            aligned += 1;
        }
    }

    for capability in capability_seeds {
        if !family.distinct_capability_keys.contains(&capability.capability_key) {
            continue;
        }
        let decomposition = decompose_capability_key(&capability.capability_key);
        if let Some(entity) = decomposition.entity.as_deref() {
            total += 1;
            if stem_aligns_with_root(entity, root) {
                aligned += 1;
            }
        }
        for candidate in &capability.candidates {
            if !family_matches_candidate(family, &candidate.concept) {
                continue;
            }
            for stem in business_stems_from_evidence(&candidate.raw_evidence) {
                total += 1;
                if stem_aligns_with_root(&stem, root) {
                    aligned += 1;
                }
            }
            let (concept_root, _) = normalized_root_concept(&candidate.concept);
            total += 1;
            if stem_aligns_with_root(&concept_root, root) {
                aligned += 1;
            }
        }
    }

    if total == 0 {
        0.0
    } else {
        (aligned as f64 / total as f64).clamp(0.0, 1.0)
    }
}

fn business_stems_from_evidence(raw: &DomainSeedRawEvidence) -> Vec<String> {
    let mut stems = Vec::new();
    if let Some(owner_class) = raw.owner_class.as_deref() {
        stems.push(owner_business_stem(owner_class));
    }
    if let Some(module) = raw.module.as_deref() {
        stems.push(module_tail_segment(module));
    }
    if let Some(package) = raw.package.as_deref() {
        stems.push(package_tail_segment(package));
    }
    if let Some(unit_name) = raw.unit_name.as_deref() {
        stems.push(owner_business_stem(unit_name));
    }
    stems
}

pub(crate) fn stem_aligns_with_root(stem: &str, root: &str) -> bool {
    let stem = stem.to_ascii_lowercase();
    let root = root.to_ascii_lowercase();
    if stem.is_empty() || root.is_empty() {
        return false;
    }
    if stem == root || stem.contains(&root) || root.contains(&stem) {
        return true;
    }
    let stem_tokens = atomize_concept_label(&stem);
    let root_tokens = atomize_concept_label(&root);
    stem_tokens.iter().any(|token| token == &root)
        || root_tokens.iter().any(|token| token == &stem)
        || stem_tokens.first() == root_tokens.first()
}

fn build_anchor_score_components(family: &RankedConceptFamily) -> AnchorScoreComponents {
    let mut symbolic_weights = BTreeMap::new();
    for name in POSITIVE_COMPONENTS
        .iter()
        .chain(NEGATIVE_COMPONENTS.iter())
        .copied()
    {
        symbolic_weights.insert(name.to_string(), 1.0);
    }

    let independent_evidence = independent_evidence_score(family);
    let values = BTreeMap::from([
        ("coverage".to_string(), family.coverage_score),
        ("coherence".to_string(), family.coherence_score),
        ("entityness".to_string(), family.concept_role.entityness),
        (
            "ownershipAlignment".to_string(),
            family.concept_role.business_root_alignment,
        ),
        ("specificity".to_string(), family.specificity_score),
        ("independentEvidence".to_string(), independent_evidence),
        ("actionness".to_string(), family.concept_role.actionness),
        (
            "effectiveContextDispersion".to_string(),
            family.concept_role.effective_context_dispersion,
        ),
        ("noise".to_string(), family.noise_penalty),
    ]);

    let mut components = BTreeMap::new();
    let mut symbolic_total = 0.0;
    for (name, value) in values {
        let weight = symbolic_weights.get(&name).copied().unwrap_or(1.0);
        let polarity = if POSITIVE_COMPONENTS.contains(&name.as_str()) {
            "positive"
        } else {
            "negative"
        };
        let contribution = value * weight;
        let signed_contribution = if polarity == "positive" {
            contribution
        } else {
            -contribution
        };
        symbolic_total += signed_contribution;
        components.insert(
            name,
            AnchorScoreComponent {
                value,
                weight,
                polarity: polarity.into(),
                contribution,
                signed_contribution,
            },
        );
    }

    AnchorScoreComponents {
        symbolic_weights,
        components,
        symbolic_total,
    }
}

fn independent_evidence_score(family: &RankedConceptFamily) -> f64 {
    let independent = family.independent_evidence_groups.len();
    let correlated = family.correlated_evidence_groups.len();
    if independent + correlated == 0 {
        0.0
    } else {
        independent as f64 / (independent + correlated) as f64
    }
}

fn classify_sparse_relation(
    left: &RankedConceptFamily,
    right: &RankedConceptFamily,
    evidence: &SeedGraphEdgeEvidence,
) -> Option<(String, String, SparseEdgeEvidenceAnalysis)> {
    let analysis = analyze_sparse_edge_evidence(left, right, evidence);
    if !sparse_edge_qualifies(&analysis) {
        return None;
    }

    let relation = classify_relation(left, right, evidence)?;
    let reason = format!(
        "{} independent evidence groups ({}) with {} raw channels; requires >=2 groups including one of {}",
        analysis.independent_evidence_groups.len(),
        analysis.independent_evidence_groups.join(", "),
        analysis.raw_channels.len(),
        STRONG_EVIDENCE_GROUPS.join(", ")
    );
    Some((relation, reason, analysis))
}

fn analyze_sparse_edge_evidence(
    left: &RankedConceptFamily,
    right: &RankedConceptFamily,
    evidence: &SeedGraphEdgeEvidence,
) -> SparseEdgeEvidenceAnalysis {
    let mut raw_channels = Vec::new();
    let mut independent_groups = BTreeSet::new();
    let mut weak_groups = BTreeSet::new();
    let mut lexical_conventions = BTreeSet::new();

    if !evidence.overlap_child_concepts.is_empty() {
        raw_channels.push("childConceptOverlap".into());
        independent_groups.insert("lexical".into());
        for concept in &evidence.overlap_child_concepts {
            lexical_conventions.insert(convention_key(concept));
        }
    }

    if lexical_path_containment(left, right) {
        raw_channels.push("atomizedPathContainment".into());
        independent_groups.insert("lexical".into());
        lexical_conventions.insert(left.concept_role.normalized_root_concept.clone());
        lexical_conventions.insert(right.concept_role.normalized_root_concept.clone());
    }

    if business_root_containment_signal(left, right, evidence) {
        raw_channels.push("businessRootContainment".into());
    }

    if ownership_group_independent(&evidence.shared_owners, &lexical_conventions) {
        raw_channels.push("sharedOwnerStem".into());
        independent_groups.insert("ownership".into());
    }

    if structural_group_independent(&evidence.shared_modules, &lexical_conventions) {
        raw_channels.push("sharedModule".into());
        independent_groups.insert("structural".into());
    }

    if state_resource_group_independent(left, right) {
        raw_channels.push("sharedStateResource".into());
        independent_groups.insert("stateResource".into());
    }

    if behavior_group_independent(left, right, &lexical_conventions) {
        raw_channels.push("sharedBehaviorPattern".into());
        independent_groups.insert("behavior".into());
    }

    let non_transport_contracts = evidence
        .shared_contract_prefixes
        .iter()
        .filter(|prefix| !is_transport_contract_prefix(prefix))
        .cloned()
        .collect::<Vec<_>>();
    if !non_transport_contracts.is_empty() {
        raw_channels.push("sharedContractPrefix".into());
        weak_groups.insert("contract".into());
    }

    SparseEdgeEvidenceAnalysis {
        raw_channels,
        independent_evidence_groups: independent_groups.into_iter().collect(),
        weak_evidence_groups: weak_groups.into_iter().collect(),
    }
}

fn sparse_edge_qualifies(analysis: &SparseEdgeEvidenceAnalysis) -> bool {
    if analysis.independent_evidence_groups.len() < 2 {
        return false;
    }
    let has_strong = analysis
        .independent_evidence_groups
        .iter()
        .any(|group| STRONG_EVIDENCE_GROUPS.contains(&group.as_str()));
    if !has_strong {
        return false;
    }

    if analysis
        .raw_channels
        .iter()
        .any(|channel| channel == "businessRootContainment")
    {
        let non_lexical_groups = analysis
            .independent_evidence_groups
            .iter()
            .filter(|group| *group != "lexical")
            .count();
        if non_lexical_groups < 1 {
            return false;
        }
    }

    true
}

fn convention_key(label: &str) -> String {
    normalized_root_concept(label).0
}

fn lexical_path_containment(left: &RankedConceptFamily, right: &RankedConceptFamily) -> bool {
    atomized_path_contains(&left.atomized_path, &right.concept_role.normalized_root_concept)
        || atomized_path_contains(&right.atomized_path, &left.concept_role.normalized_root_concept)
}

fn business_root_containment_signal(
    left: &RankedConceptFamily,
    right: &RankedConceptFamily,
    evidence: &SeedGraphEdgeEvidence,
) -> bool {
    if evidence.containment {
        return stem_aligns_with_root(
            &right.concept_role.normalized_root_concept,
            &left.concept_role.normalized_root_concept,
        ) || stem_aligns_with_root(
            &left.concept_role.normalized_root_concept,
            &right.concept_role.normalized_root_concept,
        );
    }
    lexical_path_containment(left, right)
}

fn ownership_group_independent(
    shared_owners: &[String],
    lexical_conventions: &BTreeSet<String>,
) -> bool {
    if shared_owners.is_empty() {
        return false;
    }
    shared_owners.iter().any(|owner| {
        let stem = owner_business_stem(owner);
        let convention = convention_key(&stem);
        !lexical_conventions.contains(&convention)
    })
}

fn structural_group_independent(
    shared_modules: &[String],
    lexical_conventions: &BTreeSet<String>,
) -> bool {
    if shared_modules.is_empty() {
        return false;
    }
    shared_modules.iter().any(|module| {
        let convention = convention_key(&module_tail_segment(module));
        !lexical_conventions.contains(&convention)
    })
}

fn state_resource_group_independent(
    left: &RankedConceptFamily,
    right: &RankedConceptFamily,
) -> bool {
    let right_resources = right
        .provenance
        .resource_entities
        .iter()
        .collect::<BTreeSet<_>>();
    !left
        .provenance
        .resource_entities
        .iter()
        .filter(|resource| right_resources.contains(*resource))
        .collect::<Vec<_>>()
        .is_empty()
}

fn behavior_group_independent(
    left: &RankedConceptFamily,
    right: &RankedConceptFamily,
    lexical_conventions: &BTreeSet<String>,
) -> bool {
    let left_actions = capability_actions(left);
    let right_actions = capability_actions(right);
    let shared_actions = left_actions
        .intersection(&right_actions)
        .cloned()
        .collect::<BTreeSet<_>>();
    if shared_actions.is_empty() {
        return false;
    }
    let left_entities = capability_entities(left);
    let right_entities = capability_entities(right);
    left_entities
        .iter()
        .chain(right_entities.iter())
        .any(|entity| !lexical_conventions.contains(&convention_key(entity)))
}

fn capability_actions(family: &RankedConceptFamily) -> BTreeSet<String> {
    family
        .distinct_capability_keys
        .iter()
        .filter_map(|key| decompose_capability_key(key).action)
        .collect()
}

fn capability_entities(family: &RankedConceptFamily) -> BTreeSet<String> {
    family
        .distinct_capability_keys
        .iter()
        .filter_map(|key| decompose_capability_key(key).entity)
        .collect()
}

fn atomized_path_contains(path: &str, root: &str) -> bool {
    path.split('/').any(|token| token == root)
}

fn is_transport_contract_prefix(prefix: &str) -> bool {
    let lower = prefix.to_ascii_lowercase();
    TRANSPORT_CONTRACT_PREFIXES.contains(&lower.as_str())
        || lower.ends_with("-api")
        || lower.ends_with("api")
        || lower == "rpc"
        || lower == "ws"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::formation::domain_seed_aggregation::{
        EvidenceGroupDiagnostic, IdfPenaltyDiagnostic,
    };
    use crate::domain::formation::domain_seed_role_graph::ConceptRoleDiagnostic;

    fn sample_family(
        root: &str,
        role_class: &str,
        owners: &[&str],
        alignment: f64,
        dispersion: f64,
    ) -> RankedConceptFamily {
        RankedConceptFamily {
            rank: 1,
            root_concept: root.into(),
            child_concepts: vec![root.into()],
            atomized_path: root.into(),
            distinct_capabilities: 2,
            distinct_capability_keys: vec!["create-order".into(), "draft-order".into()],
            distinct_entrypoints: 1,
            distinct_entrypoint_ids: vec!["ep-1".into()],
            distinct_contracts: 1,
            distinct_contract_paths: vec!["/orders".into()],
            distinct_owners: owners.len(),
            distinct_owner_classes: owners.iter().map(|value| value.to_string()).collect(),
            distinct_modules: 1,
            distinct_module_paths: vec!["app.api.routes.orders".into()],
            distinct_units: 1,
            correlated_evidence_groups: Vec::new(),
            independent_evidence_groups: vec![EvidenceGroupDiagnostic {
                group_id: "independent".into(),
                lexical_root: root.into(),
                evidence_sources: vec!["ownerClass".into()],
                capability_keys: vec!["create-order".into()],
            }],
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
                trailing_entity_hits: 2,
                ownership_evidence_hits: 2,
                identifier_position_hits: 2,
                context_dispersion: dispersion,
                business_root_alignment: alignment,
                effective_context_dispersion: dispersion * (1.0 - alignment),
                role_class: role_class.into(),
                normalized_root_concept: root.into(),
                normalization_diagnostics: Vec::new(),
            },
            anchor_score_components: AnchorScoreComponents {
                symbolic_weights: BTreeMap::new(),
                components: BTreeMap::new(),
                symbolic_total: 0.0,
            },
            provenance: Default::default(),
            support_signature: Default::default(),
        }
    }

    #[test]
    fn symbolic_total은_signed_score다() {
        let mut family = sample_family("order", "anchor", &["OrderResolver"], 0.8, 0.5);
        enrich_bdr_recovery(std::slice::from_mut(&mut family), &[]);
        let components = &family.anchor_score_components;
        let recomposed: f64 = components
            .components
            .values()
            .map(|component| component.signed_contribution)
            .sum();
        assert!((components.symbolic_total - recomposed).abs() < 0.0001);
        assert!(components.symbolic_total > 0.0);
        assert_eq!(
            components
                .components
                .get("actionness")
                .map(|component| component.polarity.as_str()),
            Some("negative")
        );
        assert!(
            components
                .components
                .get("actionness")
                .map(|component| component.signed_contribution)
                .unwrap_or(0.0)
                < 0.0
        );
    }

    #[test]
    fn sparse_graph는_contract_prefix_단독으로_edge를_만들지_않는다() {
        let left = sample_family("accounts", "anchor", &["AccountsController"], 0.9, 0.2);
        let mut right = sample_family("sessions", "anchor", &["SessionController"], 0.9, 0.2);
        right.distinct_contract_paths = vec!["/api/v1/sessions".into()];
        let evidence = edge_evidence(&left, &right);
        assert!(classify_sparse_relation(&left, &right, &evidence).is_none());
    }

    #[test]
    fn sparse_graph는_action_cross_cutting_node를_제외한다() {
        let anchor = sample_family("order", "anchor", &["OrderResolver"], 0.9, 0.2);
        let ambiguous = sample_family("payment", "ambiguous", &["PaymentService"], 0.5, 0.4);
        let action = sample_family("update", "actionCrossCutting", &[], 0.1, 0.8);
        let graph = build_sparse_seed_candidate_graph(&[anchor, ambiguous, action]);
        assert_eq!(graph.eligible_node_count, 2);
        assert_eq!(graph.anchor_node_count, 2);
        assert_eq!(graph.explicit_anchor_count, 1);
        assert_eq!(graph.ambiguous_node_count, 1);
        assert_eq!(graph.excluded_action_cross_cutting_count, 1);
    }

    #[test]
    fn lexical_containment만으로는_sparse_edge가_생성되지_않는다() {
        let parent = sample_family("order", "anchor", &["OrderService"], 0.9, 0.2);
        let mut child = sample_family("draft", "anchor", &["OrderService"], 0.9, 0.2);
        child.child_concepts = vec!["draftorder".into()];
        child.atomized_path = "order/draft".into();
        child.distinct_capability_keys = vec!["draft-order".into()];
        child.distinct_entrypoint_ids = vec!["ep-2".into()];
        child.distinct_module_paths = Vec::new();
        child.distinct_owner_classes = Vec::new();
        let evidence = edge_evidence(&parent, &child);
        assert!(classify_sparse_relation(&parent, &child, &evidence).is_none());
    }
}
