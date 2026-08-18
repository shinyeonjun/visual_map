//! provenance-aware seed graph diagnostics.

use super::domain_seed_aggregation::RankedConceptFamily;
use super::domain_seed_diagnostics::{CapabilityDomainSeeds, DomainSeedRawEvidence};
use super::domain_seed_role_graph::{
    classify_relation, edge_evidence, family_id, family_matches_candidate, SeedGraphEdgeEvidence,
};
use super::key_decomposition::normalized_root_concept;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const STRONG_RELATIONAL_GROUPS: &[&str] = &["behavior", "stateResource"];
const TRANSPORT_CONTRACT_PREFIXES: &[&str] = &[
    "api", "v1", "v2", "v3", "rpc", "ws", "graphql", "public", "internal", "admin-api",
    "shop-api", "adminapi", "shopapi",
];
const STATE_RESOURCE_EVIDENCE_SOURCES: &[&str] = &[
    "entityVocabulary", "resourceOwnership",
];
const STATE_RESOURCE_UNIT_KINDS: &[&str] = &[
    "entity", "record", "struct", "class", "interface", "trait", "table", "collection",
];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FamilySupportSignature {
    pub capability_keys: Vec<String>,
    pub entrypoint_ids: Vec<String>,
    pub owner_classes: Vec<String>,
    pub unit_ids: Vec<String>,
    pub signature_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PrimitiveObservation {
    pub observation_id: String,
    pub kind: String,
    pub value: String,
    pub capability_key: Option<String>,
    pub entrypoint_id: Option<String>,
    pub unit_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FamilyProvenance {
    pub capability_keys: Vec<String>,
    pub entrypoint_ids: Vec<String>,
    pub unit_ids: Vec<String>,
    pub unit_paths: Vec<String>,
    pub owner_classes: Vec<String>,
    pub module_paths: Vec<String>,
    pub package_paths: Vec<String>,
    pub resource_entities: Vec<String>,
    pub flow_ids: Vec<String>,
    pub primitive_observations: Vec<PrimitiveObservation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NearIdenticalSignatureDiagnostic {
    pub other_group_id: String,
    pub jaccard_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedHypothesisGroup {
    pub group_id: String,
    pub signature_key: String,
    pub support_signature: FamilySupportSignature,
    pub competing_family_ids: Vec<String>,
    pub competing_root_concepts: Vec<String>,
    pub near_identical_groups: Vec<NearIdenticalSignatureDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoOriginEvidence {
    pub left_family_id: String,
    pub right_family_id: String,
    pub relation: String,
    pub shared_capability_keys: Vec<String>,
    pub shared_entrypoint_ids: Vec<String>,
    pub shared_primitive_observation_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceDependency {
    pub from_kind: String,
    pub to_kind: String,
    pub shared_primitive_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceGraphNode {
    pub hypothesis_id: String,
    pub group_id: String,
    pub competing_root_concepts: Vec<String>,
    pub competing_family_ids: Vec<String>,
    pub support_signature: FamilySupportSignature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceGraphEdge {
    pub from_hypothesis_id: String,
    pub to_hypothesis_id: String,
    pub relation: String,
    pub creation_reason: String,
    pub raw_channels: Vec<String>,
    pub independent_evidence_groups: Vec<String>,
    pub weak_evidence_groups: Vec<String>,
    pub provenance_independent_groups: Vec<String>,
    pub provenance_dependencies: Vec<ProvenanceDependency>,
    pub evidence: SeedGraphEdgeEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProvenancePairRejectionSummary {
    pub total_candidate_pairs: usize,
    pub rejected_pair_count: usize,
    pub accepted_pair_count: usize,
    pub rejection_reason_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PrimitiveRelationInventory {
    pub owner_relations: usize,
    pub entity_relations: usize,
    pub resource_relations: usize,
    pub call_relations: usize,
    pub flow_relations: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceSeedCandidateGraph {
    pub raw_family_count: usize,
    pub hypothesis_node_count: usize,
    pub provenance_independent_edge_count: usize,
    pub graph_density: f64,
    pub seed_hypothesis_groups: Vec<SeedHypothesisGroup>,
    pub co_origin_evidence: Vec<CoOriginEvidence>,
    pub pair_rejection_summary: ProvenancePairRejectionSummary,
    pub primitive_relation_inventory: PrimitiveRelationInventory,
    pub nodes: Vec<ProvenanceGraphNode>,
    pub edges: Vec<ProvenanceGraphEdge>,
}

#[derive(Debug, Clone)]
struct TaggedSignal {
    channel: String,
    group: String,
    observation_ids: BTreeSet<String>,
    capability_keys: BTreeSet<String>,
    entrypoint_ids: BTreeSet<String>,
}

#[derive(Debug, Clone)]
struct ProvenanceEdgeAnalysis {
    raw_channels: Vec<String>,
    independent_groups: Vec<String>,
    weak_groups: Vec<String>,
    provenance_independent_groups: Vec<String>,
    provenance_dependencies: Vec<ProvenanceDependency>,
    co_origin: Option<CoOriginEvidence>,
}

pub fn enrich_family_provenance(
    families: &mut [RankedConceptFamily],
    capability_seeds: &[CapabilityDomainSeeds],
) {
    for family in families.iter_mut() {
        family.provenance = build_family_provenance(family, capability_seeds);
        family.support_signature = build_support_signature(family, capability_seeds);
    }
}

pub fn build_provenance_seed_candidate_graph(
    families: &[RankedConceptFamily],
    capability_seeds: &[CapabilityDomainSeeds],
) -> ProvenanceSeedCandidateGraph {
    let raw_family_count = families.len();
    let eligible: Vec<_> = families
        .iter()
        .filter(|family| family.concept_role.role_class != "actionCrossCutting")
        .collect();

    let mut seed_hypothesis_groups = build_seed_hypothesis_groups(&eligible);
    annotate_near_identical_groups(&mut seed_hypothesis_groups);

    let nodes = seed_hypothesis_groups
        .iter()
        .map(|group| ProvenanceGraphNode {
            hypothesis_id: hypothesis_id(&group.group_id),
            group_id: group.group_id.clone(),
            competing_root_concepts: group.competing_root_concepts.clone(),
            competing_family_ids: group.competing_family_ids.clone(),
            support_signature: group.support_signature.clone(),
        })
        .collect::<Vec<_>>();

    let family_to_hypothesis = seed_hypothesis_groups
        .iter()
        .flat_map(|group| {
            group
                .competing_family_ids
                .iter()
                .map(|family_id| (family_id.clone(), hypothesis_id(&group.group_id)))
        })
        .collect::<BTreeMap<_, _>>();

    let mut co_origin_evidence = Vec::new();
    let mut edges = Vec::new();
    let mut rejection_reason_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut total_candidate_pairs = 0usize;
    let mut accepted_pair_count = 0usize;
    for left in &eligible {
        for right in &eligible {
            if left.root_concept == right.root_concept {
                continue;
            }
            total_candidate_pairs += 1;
            let evidence = edge_evidence(left, right);
            let analysis = analyze_provenance_edge(left, right, &evidence);
            if let Some(co_origin) = analysis.co_origin.clone() {
                co_origin_evidence.push(co_origin);
            }
            let left_hypothesis = family_to_hypothesis
                .get(&family_id(left))
                .cloned()
                .unwrap_or_else(|| family_id(left));
            let right_hypothesis = family_to_hypothesis
                .get(&family_id(right))
                .cloned()
                .unwrap_or_else(|| family_id(right));
            let same_hypothesis = left_hypothesis == right_hypothesis;
            let qualifies = provenance_edge_qualifies(&analysis);
            let relation = if qualifies {
                classify_relation(left, right, &evidence)
            } else {
                None
            };
            if qualifies && relation.is_some() && !same_hypothesis {
                accepted_pair_count += 1;
                let relation = relation.expect("relation checked");
                edges.push(ProvenanceGraphEdge {
                    from_hypothesis_id: left_hypothesis,
                    to_hypothesis_id: right_hypothesis,
                    relation,
                    creation_reason: format!(
                        "{} provenance-independent groups ({}) with {} dependencies recorded",
                        analysis.provenance_independent_groups.len(),
                        analysis.provenance_independent_groups.join(", "),
                        analysis.provenance_dependencies.len()
                    ),
                    raw_channels: analysis.raw_channels,
                    independent_evidence_groups: analysis.independent_groups,
                    weak_evidence_groups: analysis.weak_groups,
                    provenance_independent_groups: analysis.provenance_independent_groups,
                    provenance_dependencies: analysis.provenance_dependencies,
                    evidence,
                });
                continue;
            }
            let reason = classify_provenance_pair_rejection(
                &analysis,
                relation.as_deref(),
                same_hypothesis,
            );
            *rejection_reason_counts.entry(reason).or_default() += 1;
        }
    }

    edges.sort_by(|left, right| {
        left.from_hypothesis_id
            .cmp(&right.from_hypothesis_id)
            .then_with(|| left.to_hypothesis_id.cmp(&right.to_hypothesis_id))
    });
    co_origin_evidence.sort_by(|left, right| {
        left.left_family_id
            .cmp(&right.left_family_id)
            .then_with(|| left.right_family_id.cmp(&right.right_family_id))
    });

    let hypothesis_node_count = nodes.len();
    let provenance_independent_edge_count = edges.len();
    let graph_density = if hypothesis_node_count <= 1 {
        0.0
    } else {
        provenance_independent_edge_count as f64
            / (hypothesis_node_count as f64 * (hypothesis_node_count as f64 - 1.0))
    };

    let rejected_pair_count = total_candidate_pairs.saturating_sub(accepted_pair_count);
    let pair_rejection_summary = ProvenancePairRejectionSummary {
        total_candidate_pairs,
        rejected_pair_count,
        accepted_pair_count,
        rejection_reason_counts,
    };
    let primitive_relation_inventory = build_primitive_relation_inventory(capability_seeds);

    ProvenanceSeedCandidateGraph {
        raw_family_count,
        hypothesis_node_count,
        provenance_independent_edge_count,
        graph_density,
        seed_hypothesis_groups,
        co_origin_evidence,
        pair_rejection_summary,
        primitive_relation_inventory,
        nodes,
        edges,
    }
}

pub fn build_family_provenance(
    family: &RankedConceptFamily,
    capability_seeds: &[CapabilityDomainSeeds],
) -> FamilyProvenance {
    let mut capability_keys = BTreeSet::new();
    let mut entrypoint_ids = BTreeSet::new();
    let mut unit_ids = BTreeSet::new();
    let mut unit_paths = BTreeSet::new();
    let mut owner_classes = BTreeSet::new();
    let mut module_paths = BTreeSet::new();
    let mut package_paths = BTreeSet::new();
    let mut resource_entities = BTreeSet::new();
    let mut flow_ids = BTreeSet::new();
    let mut primitive_observations = Vec::new();

    for capability_key in &family.distinct_capability_keys {
        capability_keys.insert(capability_key.clone());
        push_observation(
            &mut primitive_observations,
            "capabilityKey",
            capability_key,
            Some(capability_key.clone()),
            None,
            None,
        );
    }
    for seed in capability_seeds {
        if !family.distinct_capability_keys.contains(&seed.capability_key) {
            continue;
        }
        for entrypoint_id in &seed.coverage.entrypoint_ids {
            entrypoint_ids.insert(entrypoint_id.clone());
            push_observation(
                &mut primitive_observations,
                "entrypointId",
                entrypoint_id,
                Some(seed.capability_key.clone()),
                Some(entrypoint_id.clone()),
                None,
            );
        }
        for unit_id in &seed.coverage.unit_ids {
            unit_ids.insert(unit_id.clone());
            push_observation(
                &mut primitive_observations,
                "unitId",
                unit_id,
                Some(seed.capability_key.clone()),
                None,
                Some(unit_id.clone()),
            );
        }
        for unit_path in &seed.coverage.unit_paths {
            unit_paths.insert(unit_path.clone());
            push_observation(
                &mut primitive_observations,
                "unitPath",
                unit_path,
                Some(seed.capability_key.clone()),
                None,
                None,
            );
        }
        for owner in &seed.coverage.owner_classes {
            owner_classes.insert(owner.clone());
            push_observation(
                &mut primitive_observations,
                "owner",
                owner,
                Some(seed.capability_key.clone()),
                seed.coverage.entrypoint_ids.first().cloned(),
                None,
            );
        }
        for module in &seed.coverage.module_paths {
            module_paths.insert(module.clone());
            push_observation(
                &mut primitive_observations,
                "module",
                module,
                Some(seed.capability_key.clone()),
                None,
                None,
            );
        }
        for package in &seed.coverage.package_paths {
            package_paths.insert(package.clone());
            push_observation(
                &mut primitive_observations,
                "package",
                package,
                Some(seed.capability_key.clone()),
                None,
                None,
            );
        }
        for resource in &seed.coverage.resource_entities {
            resource_entities.insert(resource.clone());
            push_observation(
                &mut primitive_observations,
                "resourceEntity",
                resource,
                Some(seed.capability_key.clone()),
                None,
                None,
            );
        }
        for flow_id in &seed.coverage.flow_ids {
            flow_ids.insert(flow_id.clone());
            push_observation(
                &mut primitive_observations,
                "flowRelation",
                flow_id,
                Some(seed.capability_key.clone()),
                None,
                None,
            );
        }
        for candidate in &seed.candidates {
            if !family_matches_candidate(family, &candidate.concept) {
                continue;
            }
            collect_candidate_observations(
                &mut primitive_observations,
                &seed.capability_key,
                seed.coverage.entrypoint_ids.first(),
                &candidate.raw_evidence,
                &candidate.evidence_source,
            );
        }
    }

    primitive_observations.sort_by(|left, right| left.observation_id.cmp(&right.observation_id));
    primitive_observations.dedup_by(|left, right| left.observation_id == right.observation_id);

    FamilyProvenance {
        capability_keys: capability_keys.into_iter().collect(),
        entrypoint_ids: entrypoint_ids.into_iter().collect(),
        unit_ids: unit_ids.into_iter().collect(),
        unit_paths: unit_paths.into_iter().collect(),
        owner_classes: owner_classes.into_iter().collect(),
        module_paths: module_paths.into_iter().collect(),
        package_paths: package_paths.into_iter().collect(),
        resource_entities: resource_entities.into_iter().collect(),
        flow_ids: flow_ids.into_iter().collect(),
        primitive_observations,
    }
}

pub fn build_support_signature(
    family: &RankedConceptFamily,
    capability_seeds: &[CapabilityDomainSeeds],
) -> FamilySupportSignature {
    let mut capability_keys = BTreeSet::new();
    let mut entrypoint_ids = BTreeSet::new();
    let mut owner_classes = BTreeSet::new();
    let mut unit_ids = BTreeSet::new();

    for key in &family.distinct_capability_keys {
        capability_keys.insert(key.clone());
    }
    for seed in capability_seeds {
        if !family.distinct_capability_keys.contains(&seed.capability_key) {
            continue;
        }
        entrypoint_ids.extend(seed.coverage.entrypoint_ids.iter().cloned());
        owner_classes.extend(seed.coverage.owner_classes.iter().cloned());
        unit_ids.extend(seed.coverage.unit_ids.iter().cloned());
    }

    let capability_keys = capability_keys.into_iter().collect::<Vec<_>>();
    let entrypoint_ids = entrypoint_ids.into_iter().collect::<Vec<_>>();
    let owner_classes = owner_classes.into_iter().collect::<Vec<_>>();
    let unit_ids = unit_ids.into_iter().collect::<Vec<_>>();
    let signature_key = format!(
        "cap:{}|ep:{}|owner:{}|unit:{}",
        capability_keys.join(","),
        entrypoint_ids.join(","),
        owner_classes.join(","),
        unit_ids.join(",")
    );

    FamilySupportSignature {
        capability_keys,
        entrypoint_ids,
        owner_classes,
        unit_ids,
        signature_key,
    }
}

fn build_seed_hypothesis_groups(families: &[&RankedConceptFamily]) -> Vec<SeedHypothesisGroup> {
    let mut by_signature: BTreeMap<String, Vec<&RankedConceptFamily>> = BTreeMap::new();
    for family in families {
        by_signature
            .entry(family.support_signature.signature_key.clone())
            .or_default()
            .push(*family);
    }

    by_signature
        .into_iter()
        .enumerate()
        .map(|(index, (signature_key, members))| {
            let support_signature = members[0].support_signature.clone();
            SeedHypothesisGroup {
                group_id: format!("hypothesis:{index}"),
                signature_key,
                support_signature,
                competing_family_ids: members.iter().map(|family| family_id(family)).collect(),
                competing_root_concepts: members
                    .iter()
                    .map(|family| family.root_concept.clone())
                    .collect(),
                near_identical_groups: Vec::new(),
            }
        })
        .collect()
}

fn annotate_near_identical_groups(groups: &mut [SeedHypothesisGroup]) {
    for index in 0..groups.len() {
        for other_index in (index + 1)..groups.len() {
            let score = signature_jaccard(&groups[index].support_signature, &groups[other_index].support_signature);
            if score >= 0.5 && score < 1.0 {
                groups[index].near_identical_groups.push(NearIdenticalSignatureDiagnostic {
                    other_group_id: groups[other_index].group_id.clone(),
                    jaccard_score: score,
                });
                groups[other_index].near_identical_groups.push(NearIdenticalSignatureDiagnostic {
                    other_group_id: groups[index].group_id.clone(),
                    jaccard_score: score,
                });
            }
        }
    }
}

fn signature_jaccard(left: &FamilySupportSignature, right: &FamilySupportSignature) -> f64 {
    let left_set = signature_component_set(left);
    let right_set = signature_component_set(right);
    let intersection = left_set.intersection(&right_set).count() as f64;
    let union = left_set.union(&right_set).count() as f64;
    if union <= 0.0 {
        0.0
    } else {
        intersection / union
    }
}

fn signature_component_set(signature: &FamilySupportSignature) -> BTreeSet<String> {
    let mut values = BTreeSet::new();
    for key in &signature.capability_keys {
        values.insert(format!("cap:{key}"));
    }
    for key in &signature.entrypoint_ids {
        values.insert(format!("ep:{key}"));
    }
    for key in &signature.owner_classes {
        values.insert(format!("owner:{key}"));
    }
    for key in &signature.unit_ids {
        values.insert(format!("unit:{key}"));
    }
    values
}

fn analyze_provenance_edge(
    left: &RankedConceptFamily,
    right: &RankedConceptFamily,
    evidence: &SeedGraphEdgeEvidence,
) -> ProvenanceEdgeAnalysis {
    let co_origin = build_co_origin_evidence(left, right);
    let mut signals = Vec::new();

    if !evidence.overlap_child_concepts.is_empty() {
        signals.push(tagged_signal(
            "childConceptOverlap",
            "lexical",
            &left.provenance,
            &right.provenance,
        ));
    }
    if lexical_path_containment(left, right) {
        signals.push(tagged_signal(
            "atomizedPathContainment",
            "lexical",
            &left.provenance,
            &right.provenance,
        ));
    }
    if business_root_containment_signal(left, right, evidence) {
        signals.push(tagged_signal(
            "businessRootContainment",
            "lexical",
            &left.provenance,
            &right.provenance,
        ));
    }
    if !evidence.shared_owners.is_empty() {
        signals.push(tagged_signal(
            "sharedOwnerStem",
            "ownership",
            &left.provenance,
            &right.provenance,
        ));
    }
    if !evidence.shared_modules.is_empty() {
        signals.push(tagged_signal(
            "sharedModule",
            "structural",
            &left.provenance,
            &right.provenance,
        ));
    }
    if !shared_state_resources(left, right).is_empty() {
        signals.push(tagged_signal(
            "sharedStateResource",
            "stateResource",
            &left.provenance,
            &right.provenance,
        ));
    }
    if cross_flow_behavior(left, right) {
        signals.push(tagged_signal(
            "crossFlowBehavior",
            "behavior",
            &left.provenance,
            &right.provenance,
        ));
    }
    let non_transport_contracts = evidence
        .shared_contract_prefixes
        .iter()
        .filter(|prefix| !is_transport_contract_prefix(prefix))
        .cloned()
        .collect::<Vec<_>>();
    if !non_transport_contracts.is_empty() {
        signals.push(tagged_signal(
            "sharedContractPrefix",
            "contract",
            &left.provenance,
            &right.provenance,
        ));
    }

    let (provenance_independent_groups, provenance_dependencies) =
        collapse_provenance_groups(&signals, left, right);

    let mut raw_channels = signals.iter().map(|signal| signal.channel.clone()).collect::<Vec<_>>();
    raw_channels.sort();
    raw_channels.dedup();

    let mut independent_groups = provenance_independent_groups.clone();
    independent_groups.sort();

    let weak_groups = signals
        .iter()
        .filter(|signal| signal.group == "contract")
        .map(|signal| signal.group.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    ProvenanceEdgeAnalysis {
        raw_channels,
        independent_groups,
        weak_groups,
        provenance_independent_groups,
        provenance_dependencies,
        co_origin,
    }
}

fn collapse_provenance_groups(
    signals: &[TaggedSignal],
    left: &RankedConceptFamily,
    right: &RankedConceptFamily,
) -> (Vec<String>, Vec<ProvenanceDependency>) {
    let mut dependencies = Vec::new();
    let mut group_to_origins: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for signal in signals {
        if signal.group == "contract" {
            continue;
        }
        let origins = origin_keys(signal);
        group_to_origins
            .entry(signal.group.clone())
            .or_default()
            .extend(origins);
    }

    if let (Some(left_contract), Some(_right_contract)) = (
        contract_observation(left),
        contract_observation(right),
    ) {
        if signals.iter().any(|signal| signal.group == "ownership") {
            dependencies.push(ProvenanceDependency {
                from_kind: "owner".into(),
                to_kind: "contractNamespace".into(),
                shared_primitive_id: left_contract.observation_id.clone(),
                reason: "owner stem and contract prefix derived from same entrypoint/capability"
                    .into(),
            });
            group_to_origins.remove("ownership");
        }
    }

    let mut independent = group_to_origins.keys().cloned().collect::<Vec<_>>();
    if shared_capabilities(left, right).next().is_some() && independent.len() <= 1 {
        independent.retain(|group| group != "lexical" && group != "ownership");
    }
    if shared_entrypoints(left, right).next().is_some() {
        independent.retain(|group| *group == "behavior" || *group == "stateResource");
    }
    independent.sort();
    independent.dedup();
    (independent, dependencies)
}

fn classify_provenance_pair_rejection(
    analysis: &ProvenanceEdgeAnalysis,
    relation: Option<&str>,
    same_hypothesis: bool,
) -> String {
    if same_hypothesis {
        return "coOriginOnly".into();
    }
    if analysis.raw_channels.is_empty() && analysis.co_origin.is_none() {
        return "zeroEvidence".into();
    }
    if analysis.co_origin.is_some() && analysis.provenance_independent_groups.is_empty() {
        return "coOriginOnly".into();
    }
    let groups = analysis
        .provenance_independent_groups
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if groups.len() >= 2 {
        if relation.is_none() {
            return "multiIndependent".into();
        }
        return "multiIndependent".into();
    }
    if groups == BTreeSet::from(["lexical".to_string()]) {
        return "lexicalOnly".into();
    }
    if groups == BTreeSet::from(["contract".to_string()]) {
        return "contractOnly".into();
    }
    if groups == BTreeSet::from(["ownership".to_string()]) {
        return "ownershipOnly".into();
    }
    if groups == BTreeSet::from(["structural".to_string()]) {
        return "structuralOnly".into();
    }
    if groups == BTreeSet::from(["stateResource".to_string()]) {
        return "stateResourceOnly".into();
    }
    if groups == BTreeSet::from(["behavior".to_string()]) {
        return "behaviorOnly".into();
    }
    if relation.is_none() && !analysis.provenance_independent_groups.is_empty() {
        return "multiIndependent".into();
    }
    "zeroEvidence".into()
}

pub(crate) fn build_primitive_relation_inventory(
    capability_seeds: &[CapabilityDomainSeeds],
) -> PrimitiveRelationInventory {
    let mut owners = BTreeSet::new();
    let mut entities = BTreeSet::new();
    let mut resources = BTreeSet::new();
    let mut calls = BTreeSet::new();
    let mut flows = BTreeSet::new();

    for seed in capability_seeds {
        owners.extend(seed.coverage.owner_classes.iter().cloned());
        resources.extend(seed.coverage.resource_entities.iter().cloned());
        flows.extend(seed.coverage.flow_ids.iter().cloned());
        calls.extend(seed.coverage.entrypoint_ids.iter().cloned());
        for candidate in &seed.candidates {
            match candidate.evidence_source.as_str() {
                "entityVocabulary" => {
                    if let Some(unit_name) = candidate.raw_evidence.unit_name.as_deref() {
                        if is_state_resource_unit(candidate.raw_evidence.unit_kind.as_deref()) {
                            entities.insert(unit_name.to_string());
                        }
                    }
                }
                "resourceOwnership" => {
                    if let Some(resource_name) = candidate.raw_evidence.resource_name.as_deref() {
                        resources.insert(resource_name.to_string());
                    }
                }
                _ => {}
            }
        }
    }

    PrimitiveRelationInventory {
        owner_relations: owners.len(),
        entity_relations: entities.len(),
        resource_relations: resources.len(),
        call_relations: calls.len(),
        flow_relations: flows.len(),
    }
}

fn provenance_edge_qualifies(analysis: &ProvenanceEdgeAnalysis) -> bool {
    if analysis.co_origin.is_some()
        && analysis.provenance_independent_groups.is_empty()
        && analysis.raw_channels.len() <= 1
    {
        return false;
    }
    if analysis
        .raw_channels
        .iter()
        .all(|channel| channel == "sharedContractPrefix")
    {
        return false;
    }
    if analysis.provenance_independent_groups.len() >= 2 {
        return analysis.provenance_independent_groups.iter().any(|group| {
            STRONG_RELATIONAL_GROUPS.contains(&group.as_str()) || *group != "lexical" && *group != "contract"
        });
    }
    let strong = analysis
        .provenance_independent_groups
        .iter()
        .filter(|group| STRONG_RELATIONAL_GROUPS.contains(&group.as_str()))
        .count();
    let non_contract = analysis
        .provenance_independent_groups
        .iter()
        .filter(|group| *group != "contract")
        .count();
    strong >= 1 && non_contract >= 2
}

fn build_co_origin_evidence(
    left: &RankedConceptFamily,
    right: &RankedConceptFamily,
) -> Option<CoOriginEvidence> {
    let shared_capability_keys = shared_capabilities(left, right).cloned().collect::<Vec<_>>();
    let shared_entrypoint_ids = shared_entrypoints(left, right).cloned().collect::<Vec<_>>();
    if shared_capability_keys.is_empty() && shared_entrypoint_ids.is_empty() {
        return None;
    }
    let shared_primitive_observation_ids = left
        .provenance
        .primitive_observations
        .iter()
        .chain(right.provenance.primitive_observations.iter())
        .filter(|observation| {
            shared_capability_keys.iter().any(|key| observation.capability_key.as_deref() == Some(key.as_str()))
                || shared_entrypoint_ids
                    .iter()
                    .any(|entrypoint| observation.entrypoint_id.as_deref() == Some(entrypoint.as_str()))
        })
        .map(|observation| observation.observation_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let relation = if shared_capability_keys.is_empty() {
        "supportOverlap".into()
    } else {
        "coOriginEvidence".into()
    };
    Some(CoOriginEvidence {
        left_family_id: family_id(left),
        right_family_id: family_id(right),
        relation,
        shared_capability_keys,
        shared_entrypoint_ids,
        shared_primitive_observation_ids,
    })
}

fn tagged_signal(
    channel: &str,
    group: &str,
    left: &FamilyProvenance,
    right: &FamilyProvenance,
) -> TaggedSignal {
    let mut observation_ids = BTreeSet::new();
    let mut capability_keys = BTreeSet::new();
    let mut entrypoint_ids = BTreeSet::new();
    for provenance in [left, right] {
        observation_ids.extend(
            provenance
                .primitive_observations
                .iter()
                .map(|observation| observation.observation_id.clone()),
        );
        capability_keys.extend(provenance.capability_keys.iter().cloned());
        entrypoint_ids.extend(provenance.entrypoint_ids.iter().cloned());
    }
    TaggedSignal {
        channel: channel.into(),
        group: group.into(),
        observation_ids,
        capability_keys,
        entrypoint_ids,
    }
}

fn origin_keys(signal: &TaggedSignal) -> BTreeSet<String> {
    let mut origins = signal.observation_ids.clone();
    for capability in &signal.capability_keys {
        origins.insert(format!("cap:{capability}"));
    }
    for entrypoint in &signal.entrypoint_ids {
        origins.insert(format!("ep:{entrypoint}"));
    }
    origins
}

fn shared_capabilities<'a>(
    left: &'a RankedConceptFamily,
    right: &'a RankedConceptFamily,
) -> impl Iterator<Item = &'a String> {
    let right_keys = right
        .provenance
        .capability_keys
        .iter()
        .collect::<BTreeSet<_>>();
    left.provenance
        .capability_keys
        .iter()
        .filter(move |key| right_keys.contains(*key))
}

fn shared_entrypoints<'a>(
    left: &'a RankedConceptFamily,
    right: &'a RankedConceptFamily,
) -> impl Iterator<Item = &'a String> {
    let right_values = right
        .provenance
        .entrypoint_ids
        .iter()
        .collect::<BTreeSet<_>>();
    left.provenance
        .entrypoint_ids
        .iter()
        .filter(move |value| right_values.contains(*value))
}

fn shared_state_resources(left: &RankedConceptFamily, right: &RankedConceptFamily) -> Vec<String> {
    let right_values = right
        .provenance
        .resource_entities
        .iter()
        .collect::<BTreeSet<_>>();
    left.provenance
        .resource_entities
        .iter()
        .filter(|value| right_values.contains(*value))
        .cloned()
        .collect()
}

fn cross_flow_behavior(left: &RankedConceptFamily, right: &RankedConceptFamily) -> bool {
    let left_flows = left.provenance.flow_ids.iter().collect::<BTreeSet<_>>();
    let right_flows = right.provenance.flow_ids.iter().collect::<BTreeSet<_>>();
    if left_flows.is_disjoint(&right_flows) {
        return false;
    }
    let left_entrypoints = left.provenance.entrypoint_ids.iter().collect::<BTreeSet<_>>();
    let right_entrypoints = right.provenance.entrypoint_ids.iter().collect::<BTreeSet<_>>();
    !left_entrypoints.is_disjoint(&right_entrypoints)
        && left_entrypoints != right_entrypoints
        && left.provenance.unit_ids != right.provenance.unit_ids
}

fn contract_observation(family: &RankedConceptFamily) -> Option<PrimitiveObservation> {
    family
        .provenance
        .primitive_observations
        .iter()
        .find(|observation| observation.kind == "contractNamespace")
        .cloned()
}

fn collect_candidate_observations(
    observations: &mut Vec<PrimitiveObservation>,
    capability_key: &str,
    entrypoint_id: Option<&String>,
    raw: &DomainSeedRawEvidence,
    evidence_source: &str,
) {
    if STATE_RESOURCE_EVIDENCE_SOURCES.contains(&evidence_source) {
        if let Some(unit_name) = raw.unit_name.as_deref() {
            if is_state_resource_unit(raw.unit_kind.as_deref()) {
                push_observation(
                    observations,
                    "resourceEntity",
                    unit_name,
                    Some(capability_key.into()),
                    entrypoint_id.cloned(),
                    None,
                );
            }
        }
        if let Some(resource_name) = raw.resource_name.as_deref() {
            push_observation(
                observations,
                "resourceEntity",
                resource_name,
                Some(capability_key.into()),
                entrypoint_id.cloned(),
                None,
            );
        }
    }
    if let Some(owner_class) = raw.owner_class.as_deref() {
        push_observation(
            observations,
            "owner",
            owner_class,
            Some(capability_key.into()),
            entrypoint_id.cloned(),
            None,
        );
    }
    if let Some(module) = raw.module.as_deref() {
        push_observation(
            observations,
            "module",
            module,
            Some(capability_key.into()),
            entrypoint_id.cloned(),
            None,
        );
    }
    if let Some(package) = raw.package.as_deref() {
        push_observation(
            observations,
            "package",
            package,
            Some(capability_key.into()),
            entrypoint_id.cloned(),
            None,
        );
    }
    if let Some(contract_path) = raw.contract_path.as_deref() {
        push_observation(
            observations,
            "contractNamespace",
            contract_path,
            Some(capability_key.into()),
            entrypoint_id.cloned(),
            None,
        );
    }
}

fn is_state_resource_unit(kind: Option<&str>) -> bool {
    kind.map(|value| {
        let lower = value.to_ascii_lowercase();
        STATE_RESOURCE_UNIT_KINDS
            .iter()
            .any(|candidate| lower.contains(candidate))
    })
    .unwrap_or(false)
}

fn push_observation(
    observations: &mut Vec<PrimitiveObservation>,
    kind: &str,
    value: &str,
    capability_key: Option<String>,
    entrypoint_id: Option<String>,
    unit_id: Option<String>,
) {
    let observation_id = format!(
        "{kind}:{value}:cap:{}:ep:{}:unit:{}",
        capability_key.as_deref().unwrap_or(""),
        entrypoint_id.as_deref().unwrap_or(""),
        unit_id.as_deref().unwrap_or("")
    );
    observations.push(PrimitiveObservation {
        observation_id,
        kind: kind.into(),
        value: value.into(),
        capability_key,
        entrypoint_id,
        unit_id,
    });
}

fn hypothesis_id(group_id: &str) -> String {
    format!("node:{group_id}")
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
        return normalized_root_concept(&right.root_concept).0
            .contains(&left.concept_role.normalized_root_concept)
            || normalized_root_concept(&left.root_concept).0
                .contains(&right.concept_role.normalized_root_concept);
    }
    lexical_path_containment(left, right)
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
        IdfPenaltyDiagnostic,
    };
    use crate::domain::formation::domain_seed_role_graph::ConceptRoleDiagnostic;

    fn sample_family(root: &str, role: &str, capability: &str, entrypoint: &str) -> RankedConceptFamily {
        RankedConceptFamily {
            rank: 1,
            root_concept: root.into(),
            child_concepts: vec![root.into()],
            atomized_path: root.into(),
            distinct_capabilities: 1,
            distinct_capability_keys: vec![capability.into()],
            distinct_entrypoints: 1,
            distinct_entrypoint_ids: vec![entrypoint.into()],
            distinct_contracts: 0,
            distinct_contract_paths: Vec::new(),
            distinct_owners: 1,
            distinct_owner_classes: vec!["OrderResolver".into()],
            distinct_modules: 1,
            distinct_module_paths: vec!["app.api.routes.orders".into()],
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
                role_class: role.into(),
                normalized_root_concept: root.into(),
                normalization_diagnostics: Vec::new(),
            },
            anchor_score_components: Default::default(),
            provenance: Default::default(),
            support_signature: Default::default(),
        }
    }

    #[test]
    fn 동일_entrypoint_family는_hypothesis_group으로_묶는다() {
        let mut families = [
            sample_family("order", "anchor", "create-order", "ep-1"),
            sample_family("orders", "anchor", "create-order", "ep-1"),
        ];
        enrich_family_provenance(&mut families, &[]);
        families[0].support_signature = build_support_signature(&families[0], &[]);
        families[1].support_signature = build_support_signature(&families[1], &[]);
        let graph = build_provenance_seed_candidate_graph(&families, &[]);
        assert_eq!(graph.raw_family_count, 2);
        assert_eq!(graph.hypothesis_node_count, 1);
        assert_eq!(graph.seed_hypothesis_groups[0].competing_root_concepts.len(), 2);
    }

    #[test]
    fn 동일_entrypoint_only_relation은_provenance_edge를_만들지_않는다() {
        let mut families = [
            sample_family("order", "anchor", "create-order", "ep-1"),
            sample_family("orders", "ambiguous", "create-order", "ep-1"),
        ];
        enrich_family_provenance(&mut families, &[]);
        families[0].support_signature = build_support_signature(&families[0], &[]);
        families[1].support_signature = build_support_signature(&families[1], &[]);
        let graph = build_provenance_seed_candidate_graph(&families, &[]);
        assert_eq!(graph.provenance_independent_edge_count, 0);
        assert!(!graph.co_origin_evidence.is_empty());
    }
}
