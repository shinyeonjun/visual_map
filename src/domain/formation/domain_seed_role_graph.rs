//! concept role 및 seed candidate graph diagnostics.

use super::domain_seed_diagnostics::{CapabilityDomainSeeds, DomainSeedRawEvidence};
use super::domain_seed_aggregation::RankedConceptFamily;
use super::key_decomposition::{
    atomize_concept_label, atomize_concept_label_detailed, decompose_capability_key,
    normalized_root_concept, tokenize_capability_key, ConceptNormalizationDiagnostic,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const ACTION_POSITION_SOURCES: &[&str] = &["capabilityKey"];
const ENTITY_POSITION_SOURCES: &[&str] = &[
    "ownerClass",
    "semanticModule",
    "semanticPackage",
    "entityVocabulary",
    "resourceOwnership",
    "contractNamespace",
];
const ANCHOR_STEM_HINTS: &[&str] = &[
    "order", "product", "customer", "shipping", "payment", "session", "report", "files", "auth",
    "account", "administrator", "collection", "promotion", "invoice", "catalog", "inventory",
];
const OWNER_ROLE_SUFFIXES: &[&str] = &[
    "controller", "service", "resolver", "handler", "repository", "endpoint", "gateway",
];
const TRANSPORT_PREFIXES: &[&str] = &[
    "admin", "shop", "public", "internal", "graphql", "web", "rpc", "ws",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConceptRoleDiagnostic {
    pub position: String,
    pub actionness: f64,
    pub entityness: f64,
    pub leading_verb_hits: usize,
    pub trailing_entity_hits: usize,
    pub ownership_evidence_hits: usize,
    pub identifier_position_hits: usize,
    pub context_dispersion: f64,
    pub business_root_alignment: f64,
    pub effective_context_dispersion: f64,
    pub role_class: String,
    pub normalized_root_concept: String,
    pub normalization_diagnostics: Vec<ConceptNormalizationDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedCandidateGraph {
    pub nodes: Vec<SeedGraphNode>,
    pub edges: Vec<SeedGraphEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedGraphNode {
    pub family_id: String,
    pub root_concept: String,
    pub normalized_root_concept: String,
    pub rank: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedGraphEdge {
    pub from_family_id: String,
    pub to_family_id: String,
    pub relation: String,
    pub evidence: SeedGraphEdgeEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SeedGraphEdgeEvidence {
    pub containment: bool,
    pub overlap_child_concepts: Vec<String>,
    pub shared_owners: Vec<String>,
    pub shared_modules: Vec<String>,
    pub shared_contract_prefixes: Vec<String>,
    pub overlap_score: f64,
}

pub fn enrich_families_with_role_and_graph(
    families: &mut [RankedConceptFamily],
    capability_seeds: &[CapabilityDomainSeeds],
) -> super::domain_seed_recovery::FamilyGraphDiagnostics {
    for family in families.iter_mut() {
        family.concept_role = analyze_family_concept_role(family, capability_seeds);
    }
    super::domain_seed_recovery::enrich_bdr_recovery(families, capability_seeds);
    super::domain_seed_provenance::enrich_family_provenance(families, capability_seeds);
    let provenance_seed_candidate_graph =
        super::domain_seed_provenance::build_provenance_seed_candidate_graph(
            families,
            capability_seeds,
        );
    let (domain_anchor_eligibility, hypothesis_contexts) =
        super::domain_seed_anchor_eligibility::build_domain_anchor_eligibility(
            families,
            capability_seeds,
            &provenance_seed_candidate_graph,
        );
    let anchor_capability_graph =
        super::domain_seed_anchor_affinity::build_anchor_capability_graph(
            families,
            capability_seeds,
            &provenance_seed_candidate_graph,
            &domain_anchor_eligibility,
            &hypothesis_contexts,
        );
    super::domain_seed_recovery::FamilyGraphDiagnostics {
        seed_candidate_graph: build_seed_candidate_graph(families),
        sparse_seed_candidate_graph: super::domain_seed_recovery::build_sparse_seed_candidate_graph(
            families,
        ),
        provenance_seed_candidate_graph,
        domain_anchor_eligibility,
        anchor_capability_graph,
    }
}

pub fn analyze_family_concept_role(
    family: &RankedConceptFamily,
    capability_seeds: &[CapabilityDomainSeeds],
) -> ConceptRoleDiagnostic {
    let (normalized_root, root_diagnostics) = normalized_root_concept(&family.root_concept);
    let mut normalization_diagnostics = root_diagnostics;
    let mut leading_verb_hits = 0usize;
    let mut trailing_entity_hits = 0usize;
    let mut ownership_evidence_hits = 0usize;
    let mut identifier_position_hits = 0usize;
    let mut action_position_hits = 0usize;
    let mut entity_position_hits = 0usize;
    let mut context_counts: BTreeMap<String, usize> = BTreeMap::new();

    for capability in capability_seeds {
        if !family.distinct_capability_keys.contains(&capability.capability_key) {
            continue;
        }
        record_identifier_position(
            &family.root_concept,
            &normalized_root,
            &capability.capability_key,
            &mut leading_verb_hits,
            &mut trailing_entity_hits,
            &mut identifier_position_hits,
            &mut action_position_hits,
            &mut entity_position_hits,
        );

        for candidate in &capability.candidates {
            if !family_matches_candidate(family, &candidate.concept) {
                continue;
            }
            if ENTITY_POSITION_SOURCES.contains(&candidate.evidence_source.as_str()) {
                ownership_evidence_hits += 1;
                entity_position_hits += 1;
            }
            if ACTION_POSITION_SOURCES.contains(&candidate.evidence_source.as_str()) {
                action_position_hits += 1;
            }
            for context in business_contexts_from_evidence(&candidate.raw_evidence) {
                *context_counts.entry(context).or_default() += 1;
            }
            normalization_diagnostics.extend(atomize_concept_label_detailed(&candidate.concept).diagnostics);
        }
        for context in business_contexts_from_coverage(&capability.coverage.owner_classes) {
            *context_counts.entry(context).or_default() += 1;
        }
    }

    let context_dispersion = normalized_entropy(&context_counts);
    let actionness = compute_actionness(
        leading_verb_hits,
        trailing_entity_hits,
        action_position_hits,
        entity_position_hits,
        identifier_position_hits,
        context_dispersion,
        &normalized_root,
    );
    let entityness = compute_entityness(
        leading_verb_hits,
        trailing_entity_hits,
        ownership_evidence_hits,
        entity_position_hits,
        context_dispersion,
        &normalized_root,
        family,
    );
    let position = classify_position(actionness, entityness);
    let role_class = classify_role(actionness, entityness, context_dispersion, &normalized_root);

    normalization_diagnostics.sort_by(|left, right| left.original_token.cmp(&right.original_token));
    normalization_diagnostics
        .dedup_by(|left, right| left.original_token == right.original_token);

    ConceptRoleDiagnostic {
        position,
        actionness,
        entityness,
        leading_verb_hits,
        trailing_entity_hits,
        ownership_evidence_hits,
        identifier_position_hits,
        context_dispersion,
        business_root_alignment: 0.0,
        effective_context_dispersion: context_dispersion,
        role_class,
        normalized_root_concept: normalized_root,
        normalization_diagnostics,
    }
}

pub(crate) fn family_matches_candidate(family: &RankedConceptFamily, concept: &str) -> bool {
    if family.child_concepts.iter().any(|child| child == concept) {
        return true;
    }
    let (root, _) = normalized_root_concept(concept);
    root == family.root_concept || atomize_concept_label(concept).first() == Some(&family.root_concept)
}

fn record_identifier_position(
    root_concept: &str,
    normalized_root: &str,
    capability_key: &str,
    leading_verb_hits: &mut usize,
    trailing_entity_hits: &mut usize,
    identifier_position_hits: &mut usize,
    action_position_hits: &mut usize,
    entity_position_hits: &mut usize,
) {
    let tokens = tokenize_capability_key(capability_key);
    if tokens.is_empty() {
        return;
    }
    let decomposition = decompose_capability_key(capability_key);
    if let Some(action) = decomposition.action.as_deref() {
        if action == normalized_root || action.contains(normalized_root) {
            *leading_verb_hits += 1;
            *action_position_hits += 1;
            *identifier_position_hits += 1;
        }
    }
    if let Some(entity) = decomposition.entity.as_deref() {
        if entity.contains(normalized_root) || entity.contains(root_concept) {
            *trailing_entity_hits += 1;
            *entity_position_hits += 1;
            *identifier_position_hits += 1;
        }
    }
    if tokens.first() == Some(&normalized_root.to_string()) {
        *leading_verb_hits += 1;
        *action_position_hits += 1;
        *identifier_position_hits += 1;
    }
    if tokens.iter().skip(1).any(|token| token == normalized_root) {
        *trailing_entity_hits += 1;
        *entity_position_hits += 1;
        *identifier_position_hits += 1;
    }
}

fn business_contexts_from_evidence(raw: &DomainSeedRawEvidence) -> Vec<String> {
    let mut contexts = Vec::new();
    if let Some(owner_class) = raw.owner_class.as_deref() {
        contexts.push(format!("owner:{}", owner_business_stem(owner_class)));
    }
    if let Some(module) = raw.module.as_deref() {
        contexts.push(format!("module:{}", module_tail_segment(module)));
    }
    if let Some(package) = raw.package.as_deref() {
        contexts.push(format!("package:{}", package_tail_segment(package)));
    }
    if let Some(contract_path) = raw.contract_path.as_deref() {
        if let Some(prefix) = contract_prefix(contract_path) {
            contexts.push(format!("contract:{prefix}"));
        }
    }
    contexts
}

fn business_contexts_from_coverage(owner_classes: &[String]) -> Vec<String> {
    owner_classes
        .iter()
        .map(|owner| format!("owner:{}", owner_business_stem(owner)))
        .collect()
}

pub(crate) fn owner_business_stem(owner_class: &str) -> String {
    let mut lower = owner_class.to_ascii_lowercase();
    for suffix in OWNER_ROLE_SUFFIXES {
        if let Some(stem) = lower.strip_suffix(suffix) {
            if stem.len() >= 3 {
                lower = stem.to_string();
                break;
            }
        }
    }
    let tokens = tokenize_capability_key(&lower);
    if tokens.len() > 1 && TRANSPORT_PREFIXES.contains(&tokens[0].as_str()) {
        tokens[1..].join("")
    } else {
        tokens.join("")
    }
}

pub(crate) fn module_tail_segment(module: &str) -> String {
    module
        .replace('\\', "/")
        .split('/')
        .filter(|segment| !segment.is_empty())
        .last()
        .unwrap_or(module)
        .to_ascii_lowercase()
}

pub(crate) fn package_tail_segment(package: &str) -> String {
    package
        .replace('\\', "/")
        .split('/')
        .filter(|segment| !segment.is_empty())
        .last()
        .unwrap_or(package)
        .to_ascii_lowercase()
}

pub(crate) fn contract_prefix(contract_path: &str) -> Option<String> {
    contract_path
        .replace('\\', "/")
        .trim_start_matches('/')
        .split('/')
        .find(|segment| !segment.is_empty() && *segment != ":param")
        .map(|segment| segment.to_ascii_lowercase())
}

fn normalized_entropy(counts: &BTreeMap<String, usize>) -> f64 {
    if counts.len() <= 1 {
        return 0.0;
    }
    let total = counts.values().sum::<usize>() as f64;
    if total <= 0.0 {
        return 0.0;
    }
    let entropy: f64 = counts
        .values()
        .map(|count| {
            let probability = *count as f64 / total;
            -probability * probability.ln()
        })
        .sum();
    let max_entropy = (counts.len() as f64).ln();
    if max_entropy <= 0.0 {
        0.0
    } else {
        (entropy / max_entropy).clamp(0.0, 1.0)
    }
}

fn compute_actionness(
    leading_verb_hits: usize,
    trailing_entity_hits: usize,
    action_position_hits: usize,
    entity_position_hits: usize,
    identifier_position_hits: usize,
    context_dispersion: f64,
    normalized_root: &str,
) -> f64 {
    let leading_ratio = ratio(leading_verb_hits, leading_verb_hits + trailing_entity_hits);
    let position_ratio = ratio(action_position_hits, action_position_hits + entity_position_hits);
    let identifier_ratio = ratio(
        leading_verb_hits,
        identifier_position_hits.max(leading_verb_hits + trailing_entity_hits),
    );
    let verb_hint = if decompose_capability_key(normalized_root).action.is_some() {
        0.15
    } else {
        0.0
    };
    (leading_ratio * 0.30
        + position_ratio * 0.25
        + identifier_ratio * 0.20
        + context_dispersion * 0.20
        + verb_hint)
        .clamp(0.0, 1.0)
}

fn compute_entityness(
    leading_verb_hits: usize,
    trailing_entity_hits: usize,
    ownership_evidence_hits: usize,
    entity_position_hits: usize,
    context_dispersion: f64,
    normalized_root: &str,
    family: &RankedConceptFamily,
) -> f64 {
    let trailing_ratio = ratio(trailing_entity_hits, leading_verb_hits + trailing_entity_hits);
    let ownership_signal = (ownership_evidence_hits as f64 / (ownership_evidence_hits as f64 + 1.0))
        .min(1.0);
    let position_ratio = ratio(entity_position_hits, entity_position_hits + leading_verb_hits);
    let alignment = owner_alignment_score(normalized_root, family);
    let anchor_hint = if ANCHOR_STEM_HINTS.contains(&normalized_root) {
        0.15
    } else {
        0.0
    };
    let concentration = 1.0 - context_dispersion;
    (trailing_ratio * 0.25
        + ownership_signal * 0.25
        + position_ratio * 0.15
        + alignment * 0.15
        + concentration * 0.10
        + anchor_hint)
        .clamp(0.0, 1.0)
}

fn owner_alignment_score(normalized_root: &str, family: &RankedConceptFamily) -> f64 {
    if family.distinct_owner_classes.is_empty() {
        return 0.0;
    }
    let aligned = family
        .distinct_owner_classes
        .iter()
        .map(|owner| owner_business_stem(owner))
        .filter(|stem| stem.contains(normalized_root) || normalized_root.contains(stem))
        .count();
    (aligned as f64 / family.distinct_owner_classes.len() as f64).clamp(0.0, 1.0)
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn classify_position(actionness: f64, entityness: f64) -> String {
    if (actionness - entityness).abs() < 0.12 {
        "mixed".into()
    } else if actionness > entityness {
        "action".into()
    } else {
        "entity".into()
    }
}

fn classify_role(
    actionness: f64,
    entityness: f64,
    context_dispersion: f64,
    normalized_root: &str,
) -> String {
    let action_like = actionness >= 0.55 && context_dispersion >= 0.55;
    let anchor_like = entityness >= 0.55
        && context_dispersion <= 0.55
        && (ANCHOR_STEM_HINTS.contains(&normalized_root) || entityness > actionness + 0.10);
    if action_like && !anchor_like {
        "actionCrossCutting".into()
    } else if anchor_like {
        "anchor".into()
    } else {
        "ambiguous".into()
    }
}

pub fn build_seed_candidate_graph(families: &[RankedConceptFamily]) -> SeedCandidateGraph {
    let nodes = families
        .iter()
        .map(|family| SeedGraphNode {
            family_id: family_id(family),
            root_concept: family.root_concept.clone(),
            normalized_root_concept: family
                .concept_role
                .normalized_root_concept
                .clone(),
            rank: family.rank,
        })
        .collect::<Vec<_>>();

    let mut edges = Vec::new();
    for left in families {
        for right in families {
            if left.root_concept == right.root_concept {
                continue;
            }
            let evidence = edge_evidence(left, right);
            if let Some(relation) = classify_relation(left, right, &evidence) {
                edges.push(SeedGraphEdge {
                    from_family_id: family_id(left),
                    to_family_id: family_id(right),
                    relation,
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

    SeedCandidateGraph { nodes, edges }
}

pub(crate) fn family_id(family: &RankedConceptFamily) -> String {
    format!("family:{}", family.root_concept)
}

pub(crate) fn edge_evidence(left: &RankedConceptFamily, right: &RankedConceptFamily) -> SeedGraphEdgeEvidence {
    let left_children = left.child_concepts.iter().cloned().collect::<BTreeSet<_>>();
    let right_children = right.child_concepts.iter().cloned().collect::<BTreeSet<_>>();
    let overlap_child_concepts = left_children
        .intersection(&right_children)
        .cloned()
        .collect::<Vec<_>>();
    let containment = left_children.is_superset(&right_children)
        || atomized_prefix_match(&left.atomized_path, &right.atomized_path);
    let shared_owners = intersect_normalized(
        &left.distinct_owner_classes,
        &right.distinct_owner_classes,
        owner_business_stem,
    );
    let shared_modules = intersect_strings(
        &left.distinct_module_paths,
        &right.distinct_module_paths,
    );
    let left_prefixes = contract_prefixes(&left.distinct_contract_paths);
    let right_prefixes = contract_prefixes(&right.distinct_contract_paths);
    let shared_contract_prefixes = left_prefixes
        .intersection(&right_prefixes)
        .cloned()
        .collect::<Vec<_>>();
    let overlap_score = jaccard(
        &left_children,
        &right_children,
        &shared_owners,
        &shared_contract_prefixes,
    );
    SeedGraphEdgeEvidence {
        containment,
        overlap_child_concepts,
        shared_owners,
        shared_modules,
        shared_contract_prefixes,
        overlap_score,
    }
}

fn atomized_prefix_match(left_path: &str, right_path: &str) -> bool {
    if left_path.is_empty() || right_path.is_empty() {
        return false;
    }
    let left_tokens = left_path.split('/').collect::<Vec<_>>();
    let right_tokens = right_path.split('/').collect::<Vec<_>>();
    left_tokens.len() < right_tokens.len()
        && left_tokens
            .iter()
            .zip(right_tokens.iter())
            .all(|(left, right)| left == right)
}

fn contract_prefixes(paths: &[String]) -> BTreeSet<String> {
    paths
        .iter()
        .filter_map(|path| contract_prefix(path))
        .collect()
}

fn intersect_normalized(
    left: &[String],
    right: &[String],
    normalize: fn(&str) -> String,
) -> Vec<String> {
    let left_values = left.iter().map(|value| normalize(value)).collect::<BTreeSet<_>>();
    right
        .iter()
        .map(|value| normalize(value))
        .filter(|value| left_values.contains(value))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn intersect_strings(left: &[String], right: &[String]) -> Vec<String> {
    let left_values = left.iter().cloned().collect::<BTreeSet<_>>();
    right
        .iter()
        .filter(|value| left_values.contains(*value))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn jaccard(
    left_children: &BTreeSet<String>,
    right_children: &BTreeSet<String>,
    shared_owners: &[String],
    shared_contract_prefixes: &[String],
) -> f64 {
    let intersection = left_children.intersection(right_children).count() as f64
        + shared_owners.len() as f64 * 0.5
        + shared_contract_prefixes.len() as f64 * 0.5;
    let union = (left_children.len() + right_children.len()) as f64
        + shared_owners.len() as f64
        + shared_contract_prefixes.len() as f64;
    if union <= 0.0 {
        0.0
    } else {
        (intersection / union).clamp(0.0, 1.0)
    }
}

pub(crate) fn classify_relation(
    left: &RankedConceptFamily,
    right: &RankedConceptFamily,
    evidence: &SeedGraphEdgeEvidence,
) -> Option<String> {
    if evidence.containment {
        return Some("contains".into());
    }
    if !evidence.overlap_child_concepts.is_empty() || evidence.overlap_score >= 0.35 {
        return Some("overlaps".into());
    }
    if !evidence.shared_owners.is_empty()
        || !evidence.shared_contract_prefixes.is_empty()
        || evidence.overlap_score >= 0.18
    {
        let score_gap = (left.final_seed_score - right.final_seed_score).abs();
        if score_gap <= 0.12 {
            return Some("peer".into());
        }
        return Some("weakRelated".into());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::formation::domain_seed_aggregation::IdfPenaltyDiagnostic;
    use crate::domain::formation::domain_seed_diagnostics::{
        CapabilityDomainSeeds, CapabilitySeedCoverage, DomainSeedCandidate, DomainSeedConfidence,
    };
    use std::collections::BTreeMap;

    fn sample_family(
        root: &str,
        children: &[&str],
        owners: &[&str],
        capability_keys: &[&str],
    ) -> RankedConceptFamily {
        RankedConceptFamily {
            rank: 1,
            root_concept: root.into(),
            child_concepts: children.iter().map(|value| value.to_string()).collect(),
            atomized_path: root.into(),
            distinct_capabilities: capability_keys.len(),
            distinct_capability_keys: capability_keys.iter().map(|value| value.to_string()).collect(),
            distinct_entrypoints: 2,
            distinct_entrypoint_ids: vec!["ep-1".into(), "ep-2".into()],
            distinct_contracts: 1,
            distinct_contract_paths: vec!["/accounts".into()],
            distinct_owners: owners.len(),
            distinct_owner_classes: owners.iter().map(|value| value.to_string()).collect(),
            distinct_modules: 1,
            distinct_module_paths: vec!["app.api.routes.accounts".into()],
            distinct_units: 2,
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
                trailing_entity_hits: 2,
                ownership_evidence_hits: 2,
                identifier_position_hits: 2,
                context_dispersion: 0.2,
                business_root_alignment: 0.0,
                effective_context_dispersion: 0.2,
                role_class: "anchor".into(),
                normalized_root_concept: root.into(),
                normalization_diagnostics: Vec::new(),
            },
            anchor_score_components: crate::domain::formation::domain_seed_recovery::AnchorScoreComponents {
                symbolic_weights: BTreeMap::new(),
                components: BTreeMap::new(),
                symbolic_total: 0.0,
            },
            provenance: Default::default(),
            support_signature: Default::default(),
        }
    }

    #[test]
    fn action_concept는_cross_cutting으로_분류된다() {
        let mut family = sample_family(
            "update",
            &["update"],
            &["AccountsController", "OrderService"],
            &["update-accounts", "update-order"],
        );
        let capability_seeds = vec![
            CapabilityDomainSeeds {
                capability_key: "update-accounts".into(),
                coverage: CapabilitySeedCoverage {
                    owner_classes: vec!["AccountsController".into()],
                    ..Default::default()
                },
                candidates: vec![DomainSeedCandidate {
                    concept: "update".into(),
                    evidence_source: "capabilityKey".into(),
                    confidence: DomainSeedConfidence::Medium,
                    raw_evidence: DomainSeedRawEvidence {
                        key_token: Some("update".into()),
                        ..Default::default()
                    },
                }],
            },
            CapabilityDomainSeeds {
                capability_key: "update-order".into(),
                coverage: CapabilitySeedCoverage {
                    owner_classes: vec!["OrderService".into()],
                    ..Default::default()
                },
                candidates: vec![DomainSeedCandidate {
                    concept: "update".into(),
                    evidence_source: "capabilityKey".into(),
                    confidence: DomainSeedConfidence::Medium,
                    raw_evidence: DomainSeedRawEvidence {
                        key_token: Some("update".into()),
                        ..Default::default()
                    },
                }],
            },
        ];
        family.concept_role = analyze_family_concept_role(&family, &capability_seeds);
        assert!(family.concept_role.actionness > family.concept_role.entityness);
        assert_eq!(family.concept_role.role_class, "actionCrossCutting");
        assert!(family.concept_role.context_dispersion > 0.4);
    }

    #[test]
    fn seed_graph는_contains와_overlap을_기록한다() {
        let parent = sample_family(
            "order",
            &["order", "draftorder"],
            &["OrderService"],
            &["create-order", "draft-order"],
        );
        let child = sample_family(
            "draft",
            &["draftorder"],
            &["OrderService"],
            &["draft-order"],
        );
        let graph = build_seed_candidate_graph(&[parent, child]);
        assert!(graph.edges.iter().any(|edge| edge.relation == "contains"));
        assert!(graph.edges.iter().any(|edge| edge.relation == "overlaps"));
    }
}
