//! capability별 business-domain seed 후보를 static facts만으로 수집한다.
//! gold나 formation 결과는 사용하지 않는다.

use crate::config::{DomainPolicy, PathPolicy};
use crate::domain::capabilities::{build as build_capabilities, Capability};
use crate::domain::formation::key_decomposition::{
    decompose_capability_key, tokenize_capability_key,
};
use crate::domain::tfidf::{self, FeatureTerms};
use crate::facts::{CodeUnitKind, FactStore, ResourceAccess};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use super::capability_data::{extract_capability_data, CapabilityData};
use super::capability_evidence::collect_semantic_ownership;
use super::domain_seed_aggregation::{
    aggregate_project_domain_seeds, ProjectDomainSeedAggregation,
};

const PACKAGE_SKIP_SEGMENTS: &[&str] = &["src", "lib", "pkg", "internal", "test", "tests"];
const CONTRACT_SKIP_SEGMENTS: &[&str] = &[
    "api", "v1", "v2", "v3", "public", "internal", "backend", "ws", "rpc",
];
const GENERIC_CONCEPTS: &[&str] = &[
    "api", "app", "common", "core", "main", "util", "utils", "base", "shared", "index", "default",
    "handler", "route", "routes", "controller", "service", "module", "config", "health", "server",
    "client", "backend", "frontend", "http", "src", "lib", "packages", "modules", "resolver",
    "endpoint", "gateway",
];
const TRANSPORT_CONTEXT_PREFIXES: &[&str] = &[
    "admin", "shop", "public", "internal", "graphql", "web", "rpc", "ws",
];
const OWNER_ROLE_SUFFIXES: &[(&str, &str)] = &[
    ("controller", "controller"),
    ("service", "service"),
    ("resolver", "resolver"),
    ("handler", "handler"),
    ("repository", "repository"),
    ("endpoint", "endpoint"),
    ("gateway", "gateway"),
];
const ENTITY_KINDS: &[CodeUnitKind] = &[
    CodeUnitKind::Entity,
    CodeUnitKind::Record,
    CodeUnitKind::Struct,
    CodeUnitKind::Class,
    CodeUnitKind::Interface,
    CodeUnitKind::Trait,
];
const ENTITY_NAME_SUFFIXES: &[&str] = &["model", "entity", "dto", "record", "schema"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DomainSeedConfidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum DomainSeedEvidenceSource {
    SemanticModule,
    SemanticPackage,
    OwnerClass,
    EntityVocabulary,
    ResourceOwnership,
    ContractNamespace,
    CapabilityKey,
}

impl DomainSeedEvidenceSource {
    fn label(self) -> &'static str {
        match self {
            Self::SemanticModule => "semanticModule",
            Self::SemanticPackage => "semanticPackage",
            Self::OwnerClass => "ownerClass",
            Self::EntityVocabulary => "entityVocabulary",
            Self::ResourceOwnership => "resourceOwnership",
            Self::ContractNamespace => "contractNamespace",
            Self::CapabilityKey => "capabilityKey",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DomainSeedRawEvidence {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_segment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lexical_term: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainSeedCandidate {
    pub concept: String,
    pub evidence_source: String,
    pub confidence: DomainSeedConfidence,
    pub raw_evidence: DomainSeedRawEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitySeedCoverage {
    pub entrypoint_ids: Vec<String>,
    pub contract_paths: Vec<String>,
    pub owner_classes: Vec<String>,
    pub unit_ids: Vec<String>,
    pub unit_paths: Vec<String>,
    pub module_paths: Vec<String>,
    pub package_paths: Vec<String>,
    pub resource_ids: Vec<String>,
    pub resource_entities: Vec<String>,
    pub flow_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityDomainSeeds {
    pub capability_key: String,
    pub candidates: Vec<DomainSeedCandidate>,
    pub coverage: CapabilitySeedCoverage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainSeedDiagnostics {
    pub capabilities: usize,
    pub capability_seeds: Vec<CapabilityDomainSeeds>,
    pub aggregation: ProjectDomainSeedAggregation,
    pub knowledge_graph_ir: crate::graph::KnowledgeGraphIr,
}

pub fn analyze_domain_seeds(
    store: &FactStore,
    execution_flows: &crate::flow::ExecutionFlowGraph,
    domain_policy: &DomainPolicy,
    path_policy: &PathPolicy,
) -> DomainSeedDiagnostics {
    let capabilities = build_capabilities(store, path_policy);
    if capabilities.is_empty() {
        return DomainSeedDiagnostics {
            capabilities: 0,
            capability_seeds: Vec::new(),
            aggregation: ProjectDomainSeedAggregation {
                total_capabilities: 0,
                ranked_candidates: Vec::new(),
                ranked_concept_families: Vec::new(),
                seed_candidate_graph: crate::domain::formation::domain_seed_role_graph::SeedCandidateGraph {
                    nodes: Vec::new(),
                    edges: Vec::new(),
                },
                sparse_seed_candidate_graph:
                    crate::domain::formation::domain_seed_recovery::SparseSeedCandidateGraph {
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
                idf_policy: Default::default(),
            },
            knowledge_graph_ir: Default::default(),
        };
    }

    let capability_data = extract_capability_data(&capabilities, store, execution_flows);
    let (terms, _) = tfidf::extract_terms(&capability_data.unit_ids, store, domain_policy);
    let capability_seeds = capabilities
        .iter()
        .enumerate()
        .map(|(index, capability)| {
            let ownership = collect_semantic_ownership(capability, store);
            CapabilityDomainSeeds {
                capability_key: capability.key.clone(),
                coverage: CapabilitySeedCoverage {
                    entrypoint_ids: capability.entrypoint_ids.clone(),
                    contract_paths: capability.contract_paths.iter().cloned().collect(),
                    owner_classes: ownership.owner_classes,
                    unit_ids: capability.unit_ids.clone(),
                    unit_paths: capability_data.paths[index]
                        .iter()
                        .cloned()
                        .collect(),
                    module_paths: ownership
                        .modules
                        .iter()
                        .map(|module| module.replace('\\', "/"))
                        .collect(),
                    package_paths: ownership
                        .packages
                        .iter()
                        .map(|package| package.replace('\\', "/"))
                        .collect(),
                    resource_ids: capability.resource_ids.clone(),
                    resource_entities: capability
                        .resource_ids
                        .iter()
                        .filter_map(|resource_id| {
                            store.resources.iter().find(|resource| resource.id == *resource_id)
                        })
                        .map(|resource| resource.name.clone())
                        .collect(),
                    flow_ids: capability_data.flow_ids[index].clone(),
                },
                candidates: collect_seed_candidates(
                    capability,
                    &capability_data,
                    index,
                    &terms[index],
                    store,
                    domain_policy,
                ),
            }
        })
        .collect::<Vec<_>>();

    let aggregation = aggregate_project_domain_seeds(&capability_seeds);
    let diagnostics = DomainSeedDiagnostics {
        capabilities: capabilities.len(),
        capability_seeds,
        aggregation,
        knowledge_graph_ir: Default::default(),
    };
    DomainSeedDiagnostics {
        knowledge_graph_ir: super::domain_seed_knowledge_graph::build_domain_seed_knowledge_graph_ir(
            &diagnostics,
        ),
        ..diagnostics
    }
}

fn collect_seed_candidates(
    capability: &Capability,
    capability_data: &CapabilityData,
    index: usize,
    terms: &FeatureTerms,
    store: &FactStore,
    domain_policy: &DomainPolicy,
) -> Vec<DomainSeedCandidate> {
    let mut candidates = Vec::new();
    let ownership = collect_semantic_ownership(capability, store);

    for module in &ownership.modules {
        for concept in semantic_name_concepts(&module_segment_raw(module)) {
            push_candidate(
                &mut candidates,
                concept,
                DomainSeedEvidenceSource::SemanticModule,
                DomainSeedConfidence::Medium,
                DomainSeedRawEvidence {
                    module: Some(module.clone()),
                    ..Default::default()
                },
            );
        }
    }

    for package in &ownership.packages {
        for segment in package_segments(package) {
            push_candidate(
                &mut candidates,
                segment.clone(),
                DomainSeedEvidenceSource::SemanticPackage,
                DomainSeedConfidence::Medium,
                DomainSeedRawEvidence {
                    package: Some(package.clone()),
                    contract_segment: Some(segment),
                    ..Default::default()
                },
            );
        }
    }

    for owner_class in &ownership.owner_classes {
        if let Some((concept, role)) = owner_class_concept(owner_class) {
            push_candidate(
                &mut candidates,
                concept,
                DomainSeedEvidenceSource::OwnerClass,
                DomainSeedConfidence::High,
                DomainSeedRawEvidence {
                    owner_class: Some(owner_class.clone()),
                    owner_role: Some(role),
                    ..Default::default()
                },
            );
        }
    }

    for (unit_name, unit_kind) in entity_vocabulary(capability, store) {
        push_candidate(
            &mut candidates,
            normalize_concept(&unit_name),
            DomainSeedEvidenceSource::EntityVocabulary,
            DomainSeedConfidence::High,
            DomainSeedRawEvidence {
                unit_name: Some(unit_name),
                unit_kind: Some(unit_kind),
                ..Default::default()
            },
        );
    }

    if let Some(entity) = decompose_capability_key(&capability.key).entity {
        if is_meaningful_concept(&entity) {
            push_candidate(
                &mut candidates,
                normalize_concept(&entity),
                DomainSeedEvidenceSource::EntityVocabulary,
                DomainSeedConfidence::Medium,
                DomainSeedRawEvidence {
                    key_token: Some(entity),
                    ..Default::default()
                },
            );
        }
    }

    for resource in resources_for_capability(capability, store) {
        if let Some(concept) = resource_concept(&resource) {
            push_candidate(
                &mut candidates,
                concept,
                DomainSeedEvidenceSource::ResourceOwnership,
                resource_confidence(&resource),
                DomainSeedRawEvidence {
                    resource_name: Some(resource.name.clone()),
                    resource_kind: Some(format!("{:?}", resource.kind)),
                    ..Default::default()
                },
            );
        }
    }

    for contract_path in capability.contract_paths.iter() {
        for (concept, segment, confidence) in contract_namespace_candidates(contract_path) {
            push_candidate(
                &mut candidates,
                concept,
                DomainSeedEvidenceSource::ContractNamespace,
                confidence,
                DomainSeedRawEvidence {
                    contract_path: Some(contract_path.clone()),
                    contract_segment: Some(segment),
                    ..Default::default()
                },
            );
        }
    }

    for token in tokenize_capability_key(&capability.key) {
        if token.len() < 3 || domain_policy.is_generic(&token) {
            continue;
        }
        push_candidate(
            &mut candidates,
            normalize_concept(&token),
            DomainSeedEvidenceSource::CapabilityKey,
            DomainSeedConfidence::Medium,
            DomainSeedRawEvidence {
                key_token: Some(token),
                ..Default::default()
            },
        );
    }

    for (term, weight) in top_lexical_terms(terms, 8) {
        if weight < 0.05 || domain_policy.is_generic(&term) || term.len() < 3 {
            continue;
        }
        push_candidate(
            &mut candidates,
            normalize_concept(&term),
            DomainSeedEvidenceSource::CapabilityKey,
            DomainSeedConfidence::Low,
            DomainSeedRawEvidence {
                lexical_term: Some(term),
                ..Default::default()
            },
        );
    }

    let _ = capability_data.flow_ids[index].len();
    dedupe_candidates(candidates)
}

fn push_candidate(
    candidates: &mut Vec<DomainSeedCandidate>,
    concept: String,
    source: DomainSeedEvidenceSource,
    confidence: DomainSeedConfidence,
    raw_evidence: DomainSeedRawEvidence,
) {
    if !is_meaningful_concept(&concept) {
        return;
    }
    candidates.push(DomainSeedCandidate {
        concept,
        evidence_source: source.label().to_string(),
        confidence,
        raw_evidence,
    });
}

fn dedupe_candidates(mut candidates: Vec<DomainSeedCandidate>) -> Vec<DomainSeedCandidate> {
    let mut seen = BTreeSet::new();
    candidates.retain(|candidate| {
        let key = (
            candidate.concept.clone(),
            candidate.evidence_source.clone(),
        );
        if seen.contains(&key) {
            return false;
        }
        seen.insert(key);
        true
    });
    candidates.sort_by(|left, right| {
        right
            .confidence
            .cmp(&left.confidence)
            .then_with(|| left.concept.cmp(&right.concept))
            .then_with(|| left.evidence_source.cmp(&right.evidence_source))
    });
    candidates
}

fn module_segment_raw(module: &str) -> String {
    module
        .replace('\\', "/")
        .split('/')
        .filter(|segment| !segment.is_empty())
        .last()
        .map(|segment| segment.to_string())
        .unwrap_or_else(|| module.to_string())
}

fn semantic_name_concepts(name: &str) -> Vec<String> {
    let tokens = tokenize_class_stem(name);
    let mut concepts = BTreeSet::new();
    let full = normalize_concept(&tokens.join(""));
    if is_meaningful_concept(&full) {
        concepts.insert(full.clone());
    }
    if tokens.len() > 1 && TRANSPORT_CONTEXT_PREFIXES.contains(&tokens[0].as_str()) {
        let stripped = normalize_concept(&tokens[1..].join(""));
        if is_meaningful_concept(&stripped) {
            concepts.insert(stripped);
        }
    }
    for (suffix, _) in OWNER_ROLE_SUFFIXES {
        if let Some(stem) = full.strip_suffix(suffix) {
            if is_meaningful_concept(stem) {
                concepts.insert(stem.to_string());
            }
        }
    }
    concepts.into_iter().collect()
}

fn owner_class_concept(class_name: &str) -> Option<(String, String)> {
    let (stem, role) = strip_role_suffix(class_name)?;
    let tokens = tokenize_class_stem(&stem);
    if tokens.is_empty() {
        return None;
    }
    let concept = if TRANSPORT_CONTEXT_PREFIXES.contains(&tokens[0].as_str()) && tokens.len() > 1 {
        normalize_concept(&tokens[1..].join(""))
    } else {
        normalize_concept(&tokens.join(""))
    };
    if is_meaningful_concept(&concept) {
        Some((concept, role))
    } else {
        None
    }
}

fn strip_role_suffix(class_name: &str) -> Option<(String, String)> {
    let lower = class_name.to_ascii_lowercase();
    for (suffix, role) in OWNER_ROLE_SUFFIXES {
        if let Some(stem) = lower.strip_suffix(suffix) {
            if stem.len() >= 3 {
                let stem_original = &class_name[..class_name.len() - suffix.len()];
                return Some((stem_original.to_string(), (*role).to_string()));
            }
        }
    }
    None
}

fn tokenize_class_stem(stem: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in stem.chars() {
        if ch.is_ascii_uppercase() && !current.is_empty() {
            tokens.push(current.to_ascii_lowercase());
            current.clear();
        }
        current.push(ch);
    }
    if !current.is_empty() {
        tokens.push(current.to_ascii_lowercase());
    }
    if tokens.is_empty() {
        vec![stem.to_ascii_lowercase()]
    } else {
        tokens
    }
}

fn package_segments(package: &str) -> Vec<String> {
    package
        .split('/')
        .filter(|segment| !segment.is_empty())
        .filter(|segment| !PACKAGE_SKIP_SEGMENTS.contains(segment))
        .map(|segment| normalize_concept(segment))
        .filter(|segment| is_meaningful_concept(segment))
        .collect()
}

fn entity_vocabulary(capability: &Capability, store: &FactStore) -> Vec<(String, String)> {
    let mut values = BTreeSet::new();
    for unit_id in &capability.unit_ids {
        let Some(unit) = store.unit(unit_id) else {
            continue;
        };
        let kind_label = format!("{:?}", unit.kind);
        if ENTITY_KINDS.contains(&unit.kind) || entity_name_suffix(&unit.name) {
            values.insert((unit.name.clone(), kind_label));
        }
    }
    values.into_iter().collect()
}

fn entity_name_suffix(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    ENTITY_NAME_SUFFIXES
        .iter()
        .any(|suffix| lower.ends_with(suffix))
}

fn resources_for_capability<'a>(
    capability: &Capability,
    store: &'a FactStore,
) -> Vec<&'a ResourceAccess> {
    capability
        .resource_ids
        .iter()
        .filter_map(|resource_id| {
            store
                .resources
                .iter()
                .find(|resource| resource.id == *resource_id)
        })
        .collect()
}

fn resource_concept(resource: &ResourceAccess) -> Option<String> {
    let normalized = normalize_concept(&resource.name);
    if is_meaningful_concept(&normalized) {
        Some(normalized)
    } else {
        None
    }
}

fn resource_confidence(resource: &ResourceAccess) -> DomainSeedConfidence {
    match resource.kind {
        crate::facts::ResourceKind::Table | crate::facts::ResourceKind::Collection => {
            DomainSeedConfidence::High
        }
        crate::facts::ResourceKind::ExternalApi | crate::facts::ResourceKind::WebSocket => {
            DomainSeedConfidence::Medium
        }
        _ => DomainSeedConfidence::Low,
    }
}

fn contract_namespace_candidates(
    contract_path: &str,
) -> Vec<(String, String, DomainSeedConfidence)> {
    let segments: Vec<String> = contract_path
        .replace('\\', "/")
        .trim_start_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ":param")
        .map(|segment| segment.to_ascii_lowercase())
        .filter(|segment| !CONTRACT_SKIP_SEGMENTS.contains(&segment.as_str()))
        .collect();
    if segments.is_empty() {
        return Vec::new();
    }

    let primary = normalize_concept(&segments[0]);
    if !is_meaningful_concept(&primary) {
        return Vec::new();
    }
    vec![(
        primary,
        segments[0].clone(),
        DomainSeedConfidence::High,
    )]
}

fn top_lexical_terms(terms: &FeatureTerms, limit: usize) -> Vec<(String, f64)> {
    let mut ranked: Vec<_> = terms
        .term_frequencies
        .iter()
        .map(|(term, weight)| (term.clone(), *weight))
        .collect();
    ranked.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });
    ranked.into_iter().take(limit).collect()
}

fn normalize_concept(value: &str) -> String {
    value
        .replace(['-', '_', '.', ':', '/'], "")
        .to_ascii_lowercase()
}

fn is_meaningful_concept(value: &str) -> bool {
    let normalized = normalize_concept(value);
    normalized.len() >= 3 && !GENERIC_CONCEPTS.contains(&normalized.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{CodeUnit, CodeUnitVisibility, Entrypoint, EntrypointKind, SourceSpan};
    use crate::model::Language;

    fn unit(id: &str, name: &str, kind: CodeUnitKind, qualified: &str, path: &str) -> CodeUnit {
        CodeUnit {
            id: id.into(),
            kind,
            name: name.into(),
            qualified_name: qualified.into(),
            file_id: "file".into(),
            relative_path: path.into(),
            language: Language::Python,
            parent_id: None,
            span: SourceSpan::new("file", path, 1, 1, 2, 1),
            body_span: None,
            signature: None,
            parameters: Vec::new(),
            return_type: None,
            visibility: CodeUnitVisibility::default(),
            modifiers: Vec::new(),
            exported: true,
        }
    }

    #[test]
    fn owner_class에서_concept를_추출한다() {
        let (concept, role) = owner_class_concept("AdministratorResolver").expect("concept");
        assert_eq!(concept, "administrator");
        assert_eq!(role, "resolver");
    }

    #[test]
    fn shop_product_resolver는_product_concept를_뽑는다() {
        let (concept, role) = owner_class_concept("ShopProductResolver").expect("concept");
        assert_eq!(concept, "product");
        assert_eq!(role, "resolver");
    }

    #[test]
    fn contract_namespace는_첫_세그먼트를_뽑는다() {
        let values = contract_namespace_candidates("/api/v1/sessions/{session_id}/events");
        assert!(values.iter().any(|(concept, _, _)| concept == "sessions"));
    }

    #[test]
    fn generic_concept는_제외한다() {
        assert!(!is_meaningful_concept("api"));
        assert!(is_meaningful_concept("accounts"));
    }

    #[test]
    fn capability에서_seed_candidate를_수집한다() {
        let mut store = FactStore::default();
        store.units.insert(
            "unit-1".into(),
            unit(
                "unit-1",
                "AccountsController",
                CodeUnitKind::Class,
                "app.api.routes.accounts",
                "src/app/api/routes/accounts.py",
            ),
        );
        store.entrypoints.push(Entrypoint {
            id: "ep-1".into(),
            unit_id: "unit-1".into(),
            kind: EntrypointKind::Http,
            name: "list_accounts".into(),
            method: Some("GET".into()),
            path: Some("/api/v1/context/accounts".into()),
            framework_id: None,
            evidence: Vec::new(),
        });
        let capability = Capability {
            key: "accounts".into(),
            entrypoint_ids: vec!["ep-1".into()],
            resource_ids: Vec::new(),
            unit_ids: vec!["unit-1".into()],
            contract_paths: std::iter::once("/context/accounts".into()).collect(),
        };
        let capability_data = CapabilityData {
            unit_ids: vec![capability.unit_ids.clone()],
            resource_ids: vec![capability.resource_ids.clone()],
            flow_ids: vec![Vec::new()],
            paths: vec![std::iter::once("src/app/api/routes/accounts.py".into()).collect()],
            keys: vec![capability.key.clone()],
            contract_paths: vec![capability.contract_paths.clone()],
        };
        let terms = FeatureTerms {
            term_frequencies: std::collections::HashMap::new(),
        };
        let domain_policy = DomainPolicy::default();
        let candidates = collect_seed_candidates(
            &capability,
            &capability_data,
            0,
            &terms,
            &store,
            &domain_policy,
        );
        assert!(candidates.iter().any(|candidate| candidate.concept == "accounts"));
        assert!(candidates
            .iter()
            .any(|candidate| candidate.evidence_source == "ownerClass"));
    }
}
