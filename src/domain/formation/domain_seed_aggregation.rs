//! capability별 domain seed 후보를 project-level concept family로 집계한다.

use super::domain_seed_diagnostics::{
    CapabilityDomainSeeds, DomainSeedConfidence, DomainSeedRawEvidence,
};
use super::key_decomposition::{atomize_concept_label, normalized_root_concept};
use super::domain_seed_role_graph::enrich_families_with_role_and_graph;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const STRUCTURAL_SALIENCE_TOKENS: &[&str] = &[
    "server", "client", "backend", "frontend", "api", "http", "routes", "src", "lib", "packages",
    "modules", "controller", "resolver", "endpoint", "service", "handler", "gateway", "module",
    "route", "util", "utils", "common", "core", "main", "config", "app", "index", "default",
];
const TRANSPORT_TOKENS: &[&str] = &[
    "admin", "shop", "public", "internal", "graphql", "shopapi", "adminapi", "rpc", "ws",
    "websocket", "transport", "gateway", "http", "api",
];
const CORRELATED_EVIDENCE_SOURCES: &[&str] = &[
    "capabilityKey",
    "entityVocabulary",
    "contractNamespace",
];
const HIGH_FREQUENCY_THRESHOLD: f64 = 0.45;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDomainSeedAggregation {
    pub total_capabilities: usize,
    pub ranked_candidates: Vec<AggregatedDomainSeedCandidate>,
    pub ranked_concept_families: Vec<RankedConceptFamily>,
    pub seed_candidate_graph: super::domain_seed_role_graph::SeedCandidateGraph,
    pub sparse_seed_candidate_graph: super::domain_seed_recovery::SparseSeedCandidateGraph,
    pub provenance_seed_candidate_graph: super::domain_seed_provenance::ProvenanceSeedCandidateGraph,
    pub domain_anchor_eligibility: super::domain_seed_anchor_eligibility::DomainAnchorEligibilityDiagnostics,
    pub anchor_capability_graph: super::domain_seed_anchor_affinity::AnchorCapabilityGraph,
    pub idf_policy: IdfPenaltyPolicyDiagnostic,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct IdfPenaltyPolicyDiagnostic {
    pub formula: String,
    pub high_frequency_threshold: f64,
    pub below_threshold_penalty: f64,
    pub min_penalty: f64,
    pub max_penalty: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdfPenaltyDiagnostic {
    pub formula: String,
    pub project_local_frequency: f64,
    pub total_capabilities: usize,
    pub document_frequency: f64,
    pub high_frequency_threshold: f64,
    pub below_threshold: bool,
    pub result: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceGroupDiagnostic {
    pub group_id: String,
    pub lexical_root: String,
    pub evidence_sources: Vec<String>,
    pub capability_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedConceptFamily {
    pub rank: usize,
    pub root_concept: String,
    pub child_concepts: Vec<String>,
    pub atomized_path: String,
    pub distinct_capabilities: usize,
    pub distinct_capability_keys: Vec<String>,
    pub distinct_entrypoints: usize,
    pub distinct_entrypoint_ids: Vec<String>,
    pub distinct_contracts: usize,
    pub distinct_contract_paths: Vec<String>,
    pub distinct_owners: usize,
    pub distinct_owner_classes: Vec<String>,
    pub distinct_modules: usize,
    pub distinct_module_paths: Vec<String>,
    pub distinct_units: usize,
    pub correlated_evidence_groups: Vec<EvidenceGroupDiagnostic>,
    pub independent_evidence_groups: Vec<EvidenceGroupDiagnostic>,
    pub coverage_score: f64,
    pub coherence_score: f64,
    pub specificity_score: f64,
    pub noise_penalty: f64,
    pub genericness: f64,
    pub transportness: f64,
    pub idf_penalty: IdfPenaltyDiagnostic,
    pub final_seed_score: f64,
    pub concept_role: super::domain_seed_role_graph::ConceptRoleDiagnostic,
    pub anchor_score_components: super::domain_seed_recovery::AnchorScoreComponents,
    pub provenance: super::domain_seed_provenance::FamilyProvenance,
    pub support_signature: super::domain_seed_provenance::FamilySupportSignature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregatedDomainSeedCandidate {
    pub rank: usize,
    pub concept: String,
    pub support_capabilities: usize,
    pub support_capability_keys: Vec<String>,
    pub evidence_sources: Vec<String>,
    pub evidence_source_diversity: usize,
    pub normalized_modules: Vec<String>,
    pub normalized_packages: Vec<String>,
    pub normalized_owner_classes: Vec<String>,
    pub project_local_frequency: f64,
    pub genericness: f64,
    pub transportness: f64,
    pub extraction_confidence: f64,
    pub idf_penalty: f64,
    pub idf_penalty_diagnostic: IdfPenaltyDiagnostic,
    pub business_domain_salience: f64,
    pub raw_evidence: Vec<DomainSeedRawEvidence>,
}

#[derive(Default)]
struct ConceptAccumulator {
    capability_keys: BTreeSet<String>,
    evidence_sources: BTreeSet<String>,
    modules: BTreeSet<String>,
    packages: BTreeSet<String>,
    owner_classes: BTreeSet<String>,
    raw_evidence: Vec<DomainSeedRawEvidence>,
    extraction_scores: Vec<f64>,
}

#[derive(Debug, Clone)]
struct CapabilityEvidenceHit {
    evidence_source: String,
    lexical_root: String,
}

#[derive(Default)]
struct FamilyAccumulator {
    child_concepts: BTreeSet<String>,
    atomized_tokens: BTreeSet<String>,
    capability_keys: BTreeSet<String>,
    entrypoint_ids: BTreeSet<String>,
    contract_paths: BTreeSet<String>,
    owner_classes: BTreeSet<String>,
    unit_ids: BTreeSet<String>,
    evidence_hits: Vec<(String, CapabilityEvidenceHit)>,
    extraction_scores: Vec<f64>,
    modules: BTreeSet<String>,
    packages: BTreeSet<String>,
}

pub fn aggregate_project_domain_seeds(
    capability_seeds: &[CapabilityDomainSeeds],
) -> ProjectDomainSeedAggregation {
    let total_capabilities = capability_seeds.len();
    let idf_policy = default_idf_policy();
    if total_capabilities == 0 {
        return ProjectDomainSeedAggregation {
            total_capabilities: 0,
            ranked_candidates: Vec::new(),
            ranked_concept_families: Vec::new(),
            seed_candidate_graph: super::domain_seed_role_graph::SeedCandidateGraph {
                nodes: Vec::new(),
                edges: Vec::new(),
            },
            sparse_seed_candidate_graph: super::domain_seed_recovery::SparseSeedCandidateGraph {
                node_count: 0,
                edge_count: 0,
                graph_density: 0.0,
                anchor_node_count: 0,
                eligible_node_count: 0,
                explicit_anchor_count: 0,
                ambiguous_node_count: 0,
                excluded_action_cross_cutting_count: 0,
                excluded_action_cross_cutting: Vec::new(),
                nodes: Vec::new(),
                edges: Vec::new(),
            },
            provenance_seed_candidate_graph: Default::default(),
            domain_anchor_eligibility: Default::default(),
            anchor_capability_graph: Default::default(),
            idf_policy,
        };
    }

    let project_totals = project_coverage_totals(capability_seeds);
    let mut by_concept: BTreeMap<String, ConceptAccumulator> = BTreeMap::new();
    for capability in capability_seeds {
        for candidate in &capability.candidates {
            let entry = by_concept.entry(candidate.concept.clone()).or_default();
            entry
                .capability_keys
                .insert(capability.capability_key.clone());
            entry
                .evidence_sources
                .insert(candidate.evidence_source.clone());
            entry
                .extraction_scores
                .push(confidence_score(candidate.confidence));
            merge_raw_evidence(entry, &candidate.raw_evidence);
        }
    }

    let mut ranked: Vec<AggregatedDomainSeedCandidate> = by_concept
        .into_iter()
        .map(|(concept, accumulator)| {
            build_aggregated_candidate(concept, accumulator, total_capabilities, &idf_policy)
        })
        .collect();

    ranked.sort_by(|left, right| {
        right
            .business_domain_salience
            .partial_cmp(&left.business_domain_salience)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| right.support_capabilities.cmp(&left.support_capabilities))
            .then_with(|| left.concept.cmp(&right.concept))
    });
    for (index, candidate) in ranked.iter_mut().enumerate() {
        candidate.rank = index + 1;
    }

    let mut families = build_concept_families(
        &ranked,
        capability_seeds,
        total_capabilities,
        &project_totals,
        &idf_policy,
    );
    families.sort_by(|left, right| {
        right
            .final_seed_score
            .partial_cmp(&left.final_seed_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| right.distinct_capabilities.cmp(&left.distinct_capabilities))
            .then_with(|| left.root_concept.cmp(&right.root_concept))
    });
    for (index, family) in families.iter_mut().enumerate() {
        family.rank = index + 1;
    }

    let graph_diagnostics = enrich_families_with_role_and_graph(&mut families, capability_seeds);

    ProjectDomainSeedAggregation {
        total_capabilities,
        ranked_candidates: ranked,
        ranked_concept_families: families,
        seed_candidate_graph: graph_diagnostics.seed_candidate_graph,
        sparse_seed_candidate_graph: graph_diagnostics.sparse_seed_candidate_graph,
        provenance_seed_candidate_graph: graph_diagnostics.provenance_seed_candidate_graph,
        domain_anchor_eligibility: graph_diagnostics.domain_anchor_eligibility,
        anchor_capability_graph: graph_diagnostics.anchor_capability_graph,
        idf_policy,
    }
}

fn build_concept_families(
    ranked: &[AggregatedDomainSeedCandidate],
    capability_seeds: &[CapabilityDomainSeeds],
    total_capabilities: usize,
    project_totals: &ProjectCoverageTotals,
    idf_policy: &IdfPenaltyPolicyDiagnostic,
) -> Vec<RankedConceptFamily> {
    let coverage_by_capability = capability_seeds
        .iter()
        .map(|seed| (seed.capability_key.clone(), seed.coverage.clone()))
        .collect::<BTreeMap<_, _>>();

    let mut by_root: BTreeMap<String, FamilyAccumulator> = BTreeMap::new();
    for candidate in ranked {
        let tokens = atomize_concept_label(&candidate.concept);
        let (root, _) = normalized_root_concept(&candidate.concept);
        let root = if tokens.first() == Some(&root) {
            root
        } else {
            tokens
                .first()
                .cloned()
                .unwrap_or_else(|| candidate.concept.clone())
        };
        let family = by_root.entry(root.clone()).or_default();
        family.child_concepts.insert(candidate.concept.clone());
        family
            .atomized_tokens
            .extend(tokens.iter().cloned());
        family
            .extraction_scores
            .extend(std::iter::repeat_n(
                candidate.extraction_confidence,
                candidate.support_capabilities.max(1),
            ));
        family.modules.extend(candidate.normalized_modules.iter().cloned());
        family
            .packages
            .extend(candidate.normalized_packages.iter().cloned());
        family
            .owner_classes
            .extend(candidate.normalized_owner_classes.iter().cloned());

        for capability_key in &candidate.support_capability_keys {
            family.capability_keys.insert(capability_key.clone());
            if let Some(coverage) = coverage_by_capability.get(capability_key) {
                family
                    .entrypoint_ids
                    .extend(coverage.entrypoint_ids.iter().cloned());
                family
                    .contract_paths
                    .extend(coverage.contract_paths.iter().cloned());
                family
                    .owner_classes
                    .extend(coverage.owner_classes.iter().cloned());
                family.unit_ids.extend(coverage.unit_ids.iter().cloned());
            }
        }
    }

    for (root, family) in &mut by_root {
        for capability in capability_seeds {
            for candidate in &capability.candidates {
                let tokens = atomize_concept_label(&candidate.concept);
                let candidate_root = tokens
                    .first()
                    .cloned()
                    .unwrap_or_else(|| candidate.concept.clone());
                if candidate_root != *root || !family.child_concepts.contains(&candidate.concept) {
                    continue;
                }
                family.evidence_hits.push((
                    capability.capability_key.clone(),
                    CapabilityEvidenceHit {
                        evidence_source: candidate.evidence_source.clone(),
                        lexical_root: lexical_root_for_concept(&candidate.concept),
                    },
                ));
            }
        }
    }

    by_root
        .into_iter()
        .map(|(root_concept, accumulator)| {
            build_ranked_family(
                root_concept,
                accumulator,
                total_capabilities,
                project_totals,
                idf_policy,
            )
        })
        .collect()
}

fn build_ranked_family(
    root_concept: String,
    accumulator: FamilyAccumulator,
    total_capabilities: usize,
    project_totals: &ProjectCoverageTotals,
    idf_policy: &IdfPenaltyPolicyDiagnostic,
) -> RankedConceptFamily {
    let distinct_capabilities = accumulator.capability_keys.len();
    let project_local_frequency = distinct_capabilities as f64 / total_capabilities as f64;
    let genericness = genericness_score(&root_concept);
    let transportness = family_transportness(&root_concept, &accumulator);
    let specificity_score = (1.0 - genericness * 0.9).clamp(0.0, 1.0);
    let noise_penalty = (genericness * 0.55 + transportness * 0.45).clamp(0.0, 1.0);
    let idf_penalty =
        build_idf_diagnostic(project_local_frequency, total_capabilities, idf_policy);

    let coverage_score = family_coverage_score(&accumulator, total_capabilities, project_totals);
    let coherence_score = family_coherence_score(
        distinct_capabilities,
        accumulator.child_concepts.len(),
        &accumulator.evidence_hits,
    );
    let convergence_bonus = cross_capability_convergence(distinct_capabilities, &accumulator);
    let final_seed_score = ((coverage_score * 0.38
        + coherence_score * 0.34
        + specificity_score * 0.18
        + convergence_bonus * 0.10)
        * (1.0 - noise_penalty * 0.85)
        * idf_penalty.result)
        .clamp(0.0, 1.0);

    let mut atomized_path = accumulator.atomized_tokens.iter().cloned().collect::<Vec<_>>();
    atomized_path.sort();
    atomized_path.dedup();
    let atomized_path = if atomized_path.is_empty() {
        root_concept.clone()
    } else {
        atomized_path.join("/")
    };

    let (correlated_evidence_groups, independent_evidence_groups) =
        partition_evidence_groups(&accumulator.evidence_hits);
    let (normalized_root_concept, _) = normalized_root_concept(&root_concept);

    RankedConceptFamily {
        rank: 0,
        root_concept,
        child_concepts: accumulator.child_concepts.into_iter().collect(),
        atomized_path,
        distinct_capabilities,
        distinct_capability_keys: accumulator.capability_keys.into_iter().collect(),
        distinct_entrypoints: accumulator.entrypoint_ids.len(),
        distinct_entrypoint_ids: accumulator.entrypoint_ids.into_iter().collect(),
        distinct_contracts: accumulator.contract_paths.len(),
        distinct_contract_paths: accumulator.contract_paths.into_iter().collect(),
        distinct_owners: accumulator.owner_classes.len(),
        distinct_owner_classes: accumulator.owner_classes.iter().cloned().collect(),
        distinct_modules: accumulator.modules.len(),
        distinct_module_paths: accumulator.modules.into_iter().collect(),
        distinct_units: accumulator.unit_ids.len(),
        correlated_evidence_groups,
        independent_evidence_groups,
        coverage_score,
        coherence_score,
        specificity_score,
        noise_penalty,
        genericness,
        transportness,
        idf_penalty,
        final_seed_score,
        concept_role: super::domain_seed_role_graph::ConceptRoleDiagnostic {
            position: "unknown".into(),
            actionness: 0.0,
            entityness: 0.0,
            leading_verb_hits: 0,
            trailing_entity_hits: 0,
            ownership_evidence_hits: 0,
            identifier_position_hits: 0,
            context_dispersion: 0.0,
            business_root_alignment: 0.0,
            effective_context_dispersion: 0.0,
            role_class: "ambiguous".into(),
            normalized_root_concept,
            normalization_diagnostics: Vec::new(),
        },
            anchor_score_components: super::domain_seed_recovery::AnchorScoreComponents {
                symbolic_weights: BTreeMap::new(),
                components: BTreeMap::new(),
                symbolic_total: 0.0,
            },
            provenance: Default::default(),
            support_signature: Default::default(),
        }
}

fn build_aggregated_candidate(
    concept: String,
    accumulator: ConceptAccumulator,
    total_capabilities: usize,
    idf_policy: &IdfPenaltyPolicyDiagnostic,
) -> AggregatedDomainSeedCandidate {
    let support_capabilities = accumulator.capability_keys.len();
    let project_local_frequency = support_capabilities as f64 / total_capabilities as f64;
    let genericness = genericness_score(&concept);
    let transportness = transportness_score(&concept, &accumulator);
    let extraction_confidence = mean(&accumulator.extraction_scores);
    let evidence_source_diversity = accumulator.evidence_sources.len();
    let idf_penalty_diagnostic =
        build_idf_diagnostic(project_local_frequency, total_capabilities, idf_policy);
    let cross_capability_factor = if support_capabilities >= 2 {
        1.0
    } else if support_capabilities == 1 && evidence_source_diversity <= 1 {
        0.55
    } else {
        0.75
    };
    let business_domain_salience = (extraction_confidence
        * cross_capability_factor
        * (1.0 - genericness * 0.85)
        * (1.0 - transportness * 0.85)
        * idf_penalty_diagnostic.result)
        .clamp(0.0, 1.0);

    AggregatedDomainSeedCandidate {
        rank: 0,
        concept,
        support_capabilities,
        support_capability_keys: accumulator.capability_keys.into_iter().collect(),
        evidence_sources: accumulator.evidence_sources.into_iter().collect(),
        evidence_source_diversity,
        normalized_modules: accumulator.modules.into_iter().collect(),
        normalized_packages: accumulator.packages.into_iter().collect(),
        normalized_owner_classes: accumulator.owner_classes.into_iter().collect(),
        project_local_frequency,
        genericness,
        transportness,
        extraction_confidence,
        idf_penalty: idf_penalty_diagnostic.result,
        idf_penalty_diagnostic,
        business_domain_salience,
        raw_evidence: dedupe_raw_evidence(accumulator.raw_evidence),
    }
}

struct ProjectCoverageTotals {
    entrypoints: usize,
    contracts: usize,
    owners: usize,
    units: usize,
}

fn project_coverage_totals(capability_seeds: &[CapabilityDomainSeeds]) -> ProjectCoverageTotals {
    let mut entrypoints = BTreeSet::new();
    let mut contracts = BTreeSet::new();
    let mut owners = BTreeSet::new();
    let mut units = BTreeSet::new();
    for seed in capability_seeds {
        entrypoints.extend(seed.coverage.entrypoint_ids.iter().cloned());
        contracts.extend(seed.coverage.contract_paths.iter().cloned());
        owners.extend(seed.coverage.owner_classes.iter().cloned());
        units.extend(seed.coverage.unit_ids.iter().cloned());
    }
    ProjectCoverageTotals {
        entrypoints: entrypoints.len().max(1),
        contracts: contracts.len().max(1),
        owners: owners.len().max(1),
        units: units.len().max(1),
    }
}

fn family_coverage_score(
    accumulator: &FamilyAccumulator,
    total_capabilities: usize,
    totals: &ProjectCoverageTotals,
) -> f64 {
    let capability_coverage = accumulator.capability_keys.len() as f64 / total_capabilities as f64;
    let entrypoint_coverage =
        accumulator.entrypoint_ids.len() as f64 / totals.entrypoints as f64;
    let contract_coverage = accumulator.contract_paths.len() as f64 / totals.contracts as f64;
    let owner_coverage = accumulator.owner_classes.len() as f64 / totals.owners as f64;
    let unit_coverage = accumulator.unit_ids.len() as f64 / totals.units as f64;
    (capability_coverage * 0.35
        + entrypoint_coverage * 0.20
        + contract_coverage * 0.15
        + owner_coverage * 0.15
        + unit_coverage * 0.15)
        .clamp(0.0, 1.0)
}

fn family_coherence_score(
    distinct_capabilities: usize,
    child_concept_count: usize,
    evidence_hits: &[(String, CapabilityEvidenceHit)],
) -> f64 {
    if distinct_capabilities == 0 {
        return 0.0;
    }
    let independent_roots = evidence_hits
        .iter()
        .map(|(_, hit)| hit.lexical_root.clone())
        .collect::<BTreeSet<_>>()
        .len();
    let mut score: f64 = 0.2;
    if distinct_capabilities >= 2 {
        score += 0.35;
    }
    if child_concept_count >= 2 {
        score += 0.20;
    }
    if independent_roots >= 2 {
        score += 0.15;
    }
    if distinct_capabilities == 1 && child_concept_count <= 1 {
        score *= 0.45;
    }
    score.clamp(0.0, 1.0)
}

fn cross_capability_convergence(
    distinct_capabilities: usize,
    accumulator: &FamilyAccumulator,
) -> f64 {
    if distinct_capabilities < 2 {
        return 0.0;
    }
    let mut capability_roots: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (capability_key, hit) in &accumulator.evidence_hits {
        capability_roots
            .entry(capability_key.clone())
            .or_default()
            .insert(hit.lexical_root.clone());
    }
    let shared_roots = capability_roots
        .values()
        .fold(None, |shared: Option<BTreeSet<String>>, roots| {
            Some(match shared {
                None => roots.clone(),
                Some(current) => current.intersection(roots).cloned().collect(),
            })
        })
        .unwrap_or_default();
    let base = (distinct_capabilities as f64 / (distinct_capabilities as f64 + 1.0)).clamp(0.0, 1.0);
    if shared_roots.is_empty() {
        base * 0.65
    } else {
        (base + 0.2).min(1.0)
    }
}

fn partition_evidence_groups(
    evidence_hits: &[(String, CapabilityEvidenceHit)],
) -> (Vec<EvidenceGroupDiagnostic>, Vec<EvidenceGroupDiagnostic>) {
    let mut grouped: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    for (capability_key, hit) in evidence_hits {
        grouped
            .entry((capability_key.clone(), hit.lexical_root.clone()))
            .or_default()
            .insert(hit.evidence_source.clone());
    }

    let mut correlated = Vec::new();
    let mut independent = Vec::new();
    for ((capability_key, lexical_root), sources) in grouped {
        let correlated_sources = sources
            .iter()
            .filter(|source| CORRELATED_EVIDENCE_SOURCES.contains(&source.as_str()))
            .cloned()
            .collect::<BTreeSet<_>>();
        let group = EvidenceGroupDiagnostic {
            group_id: format!("{capability_key}:{lexical_root}"),
            lexical_root: lexical_root.clone(),
            evidence_sources: sources.into_iter().collect(),
            capability_keys: vec![capability_key.clone()],
        };
        if correlated_sources.len() >= 2 {
            correlated.push(group);
        } else {
            independent.push(group);
        }
    }
    (correlated, independent)
}

fn lexical_root_for_concept(concept: &str) -> String {
    atomize_concept_label(concept)
        .into_iter()
        .next()
        .unwrap_or_else(|| concept.to_string())
}

fn family_transportness(root_concept: &str, accumulator: &FamilyAccumulator) -> f64 {
    let mut score = transportness_score(
        root_concept,
        &ConceptAccumulator {
            capability_keys: accumulator.capability_keys.clone(),
            evidence_sources: BTreeSet::new(),
            modules: accumulator.modules.clone(),
            packages: accumulator.packages.clone(),
            owner_classes: accumulator.owner_classes.clone(),
            raw_evidence: Vec::new(),
            extraction_scores: Vec::new(),
        },
    );
    if TRANSPORT_TOKENS.iter().any(|token| root_concept.starts_with(token)) {
        score = (score + 0.25).min(1.0);
    }
    score
}

fn default_idf_policy() -> IdfPenaltyPolicyDiagnostic {
    IdfPenaltyPolicyDiagnostic {
        formula: "if frequency <= threshold => 1.0; else ln_1p((N+1)/(df+1)) clamped to [min,max]"
            .into(),
        high_frequency_threshold: HIGH_FREQUENCY_THRESHOLD,
        below_threshold_penalty: 1.0,
        min_penalty: 0.15,
        max_penalty: 1.0,
    }
}

fn build_idf_diagnostic(
    project_local_frequency: f64,
    total_capabilities: usize,
    policy: &IdfPenaltyPolicyDiagnostic,
) -> IdfPenaltyDiagnostic {
    let document_frequency = project_local_frequency * total_capabilities as f64;
    let below_threshold = project_local_frequency <= policy.high_frequency_threshold
        || total_capabilities <= 1;
    let result = if below_threshold {
        policy.below_threshold_penalty
    } else {
        let idf = ((total_capabilities as f64 + 1.0) / (document_frequency + 1.0)).ln_1p();
        idf.clamp(policy.min_penalty, policy.max_penalty)
    };
    IdfPenaltyDiagnostic {
        formula: policy.formula.clone(),
        project_local_frequency,
        total_capabilities,
        document_frequency,
        high_frequency_threshold: policy.high_frequency_threshold,
        below_threshold,
        result,
    }
}

fn merge_raw_evidence(accumulator: &mut ConceptAccumulator, raw: &DomainSeedRawEvidence) {
    if let Some(module) = raw.module.as_deref() {
        accumulator.modules.insert(normalize_module_label(module));
    }
    if let Some(package) = raw.package.as_deref() {
        accumulator.packages.insert(normalize_package_label(package));
    }
    if let Some(owner_class) = raw.owner_class.as_deref() {
        accumulator.owner_classes.insert(owner_class.to_string());
    }
    accumulator.raw_evidence.push(raw.clone());
}

fn normalize_module_label(module: &str) -> String {
    module.replace('\\', "/")
}

fn normalize_package_label(package: &str) -> String {
    package.replace('\\', "/")
}

fn confidence_score(confidence: DomainSeedConfidence) -> f64 {
    match confidence {
        DomainSeedConfidence::High => 1.0,
        DomainSeedConfidence::Medium => 0.65,
        DomainSeedConfidence::Low => 0.35,
    }
}

fn genericness_score(concept: &str) -> f64 {
    if STRUCTURAL_SALIENCE_TOKENS.contains(&concept) {
        return 1.0;
    }
    let mut score: f64 = 0.0;
    for token in STRUCTURAL_SALIENCE_TOKENS {
        if concept.contains(token) {
            score += 0.35;
        }
    }
    score.clamp(0.0, 1.0)
}

fn transportness_score(concept: &str, accumulator: &ConceptAccumulator) -> f64 {
    let mut score: f64 = 0.0;
    if TRANSPORT_TOKENS
        .iter()
        .any(|token| concept.starts_with(token))
    {
        score += 0.7;
    }
    if TRANSPORT_TOKENS.iter().any(|token| *token == concept) {
        score += 0.5;
    }
    let transport_sources = accumulator
        .evidence_sources
        .iter()
        .filter(|source| {
            matches!(
                source.as_str(),
                "contractNamespace" | "resourceOwnership" | "capabilityKey"
            )
        })
        .count();
    if transport_sources > 0 && accumulator.evidence_sources.len() <= transport_sources {
        score += 0.25;
    }
    score.clamp(0.0, 1.0)
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

fn dedupe_raw_evidence(mut evidence: Vec<DomainSeedRawEvidence>) -> Vec<DomainSeedRawEvidence> {
    let mut seen = BTreeSet::new();
    evidence.retain(|item| {
        let key = format!("{item:?}");
        if seen.contains(&key) {
            return false;
        }
        seen.insert(key);
        true
    });
    evidence.truncate(12);
    evidence
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::formation::domain_seed_diagnostics::{
        CapabilitySeedCoverage, DomainSeedCandidate,
    };

    fn sample_capability_seeds() -> Vec<CapabilityDomainSeeds> {
        vec![
            CapabilityDomainSeeds {
                capability_key: "accounts".into(),
                coverage: CapabilitySeedCoverage {
                    entrypoint_ids: vec!["ep-1".into()],
                    contract_paths: vec!["/accounts".into()],
                    owner_classes: vec!["AccountsController".into()],
                    unit_ids: vec!["unit-1".into()],
                    ..Default::default()
                },
                candidates: vec![DomainSeedCandidate {
                    concept: "accounts".into(),
                    evidence_source: "ownerClass".into(),
                    confidence: DomainSeedConfidence::High,
                    raw_evidence: DomainSeedRawEvidence {
                        owner_class: Some("AccountsController".into()),
                        owner_role: Some("controller".into()),
                        ..Default::default()
                    },
                }],
            },
            CapabilityDomainSeeds {
                capability_key: "contacts".into(),
                coverage: CapabilitySeedCoverage {
                    entrypoint_ids: vec!["ep-2".into()],
                    contract_paths: vec!["/contacts".into()],
                    owner_classes: vec!["AccountsService".into()],
                    unit_ids: vec!["unit-2".into()],
                    ..Default::default()
                },
                candidates: vec![DomainSeedCandidate {
                    concept: "accounts".into(),
                    evidence_source: "semanticModule".into(),
                    confidence: DomainSeedConfidence::Medium,
                    raw_evidence: DomainSeedRawEvidence {
                        module: Some("app.api.routes.accounts".into()),
                        ..Default::default()
                    },
                }],
            },
        ]
    }

    #[test]
    fn concept별로_support와_salience를_집계한다() {
        let aggregation = aggregate_project_domain_seeds(&sample_capability_seeds());
        assert_eq!(aggregation.ranked_candidates.len(), 1);
        let top = &aggregation.ranked_candidates[0];
        assert_eq!(top.support_capabilities, 2);
        assert_eq!(top.evidence_source_diversity, 2);
        assert!(top.business_domain_salience > 0.0);
        assert!(!top.idf_penalty_diagnostic.below_threshold);
        assert!(top.idf_penalty_diagnostic.result < 1.0);
    }

    #[test]
    fn family_level_ranking을_생성한다() {
        let aggregation = aggregate_project_domain_seeds(&sample_capability_seeds());
        assert_eq!(aggregation.ranked_concept_families.len(), 1);
        let family = &aggregation.ranked_concept_families[0];
        assert_eq!(family.root_concept, "accounts");
        assert_eq!(family.distinct_capabilities, 2);
        assert!(family.final_seed_score > 0.0);
        assert!(family.coverage_score > 0.0);
    }

    #[test]
    fn structural_concept는_salience가_낮다() {
        let capability_seeds = vec![CapabilityDomainSeeds {
            capability_key: "health".into(),
            coverage: CapabilitySeedCoverage::default(),
            candidates: vec![DomainSeedCandidate {
                concept: "controller".into(),
                evidence_source: "ownerClass".into(),
                confidence: DomainSeedConfidence::High,
                raw_evidence: DomainSeedRawEvidence {
                    owner_class: Some("HealthController".into()),
                    owner_role: Some("controller".into()),
                    ..Default::default()
                },
            }],
        }];
        let aggregation = aggregate_project_domain_seeds(&capability_seeds);
        let top = &aggregation.ranked_candidates[0];
        assert!(top.genericness >= 0.9);
        assert!(top.business_domain_salience < 0.2);
        let family = &aggregation.ranked_concept_families[0];
        assert!(family.noise_penalty > 0.5);
    }

    #[test]
    fn idf_policy_diagnostic을_노출한다() {
        let aggregation = aggregate_project_domain_seeds(&sample_capability_seeds());
        assert_eq!(
            aggregation.idf_policy.high_frequency_threshold,
            HIGH_FREQUENCY_THRESHOLD
        );
        assert!(aggregation.idf_policy.formula.contains("threshold"));
    }
}
