//! Build Knowledge Graph IR from domain seed diagnostics (expression layer only).

use super::domain_seed_diagnostics::DomainSeedDiagnostics;
use super::domain_seed_responsibility_equivalence::responsibility_domain_id;
use super::domain_seed_role_graph::family_id;
use crate::graph::knowledge_graph_ir::{
    KnowledgeEdge, KnowledgeEdgeKind, KnowledgeGraphIr, KnowledgeNode, KnowledgeNodeKind,
    KnowledgeObservationKind, KnowledgeSourceLocation,
};
use std::collections::BTreeMap;

pub fn build_domain_seed_knowledge_graph_ir(
    diagnostics: &DomainSeedDiagnostics,
) -> KnowledgeGraphIr {
    let mut graph = KnowledgeGraphIr::default();
    for seed in &diagnostics.capability_seeds {
        let capability_id = KnowledgeGraphIr::node_id(&KnowledgeNodeKind::Capability, &seed.capability_key);
        graph.upsert_node(KnowledgeNode {
            id: capability_id.clone(),
            kind: KnowledgeNodeKind::Capability,
            label: seed.capability_key.clone(),
            properties: capability_properties(seed),
            observation: KnowledgeObservationKind::Observed,
            confidence: None,
            state: None,
            provenance: Some("staticAnalysis".into()),
            source_location: None,
        });
        for entrypoint_id in &seed.coverage.entrypoint_ids {
            let entrypoint_node_id =
                KnowledgeGraphIr::node_id(&KnowledgeNodeKind::Entrypoint, entrypoint_id);
            graph.upsert_node(KnowledgeNode {
                id: entrypoint_node_id.clone(),
                kind: KnowledgeNodeKind::Entrypoint,
                label: entrypoint_id.clone(),
                properties: BTreeMap::new(),
                observation: KnowledgeObservationKind::Observed,
                confidence: None,
                state: None,
                provenance: Some("staticAnalysis".into()),
                source_location: Some(KnowledgeSourceLocation {
                    file_path: None,
                    unit_id: None,
                    entrypoint_id: Some(entrypoint_id.clone()),
                }),
            });
            graph.upsert_edge(KnowledgeEdge {
                id: KnowledgeGraphIr::edge_id(
                    &KnowledgeEdgeKind::HasEntrypoint,
                    &capability_id,
                    &entrypoint_node_id,
                ),
                kind: KnowledgeEdgeKind::HasEntrypoint,
                from_id: capability_id.clone(),
                to_id: entrypoint_node_id,
                observation: KnowledgeObservationKind::Observed,
                confidence: None,
                state: None,
                provenance: Some("staticAnalysis".into()),
                properties: BTreeMap::new(),
            });
        }
        for candidate in &seed.candidates {
            let evidence_id = format!(
                "evidence:{}:{}",
                seed.capability_key, candidate.evidence_source
            );
            graph.upsert_node(KnowledgeNode {
                id: evidence_id.clone(),
                kind: KnowledgeNodeKind::Evidence,
                label: candidate.concept.clone(),
                properties: evidence_properties(candidate),
                observation: KnowledgeObservationKind::Observed,
                confidence: None,
                state: None,
                provenance: Some(candidate.evidence_source.clone()),
                source_location: evidence_source_location(&candidate.raw_evidence),
            });
            graph.upsert_edge(KnowledgeEdge {
                id: KnowledgeGraphIr::edge_id(
                    &KnowledgeEdgeKind::SupportedBy,
                    &capability_id,
                    &evidence_id,
                ),
                kind: KnowledgeEdgeKind::SupportedBy,
                from_id: capability_id.clone(),
                to_id: evidence_id,
                observation: KnowledgeObservationKind::Observed,
                confidence: None,
                state: None,
                provenance: Some(candidate.evidence_source.clone()),
                properties: BTreeMap::new(),
            });
        }
    }

    let aggregation = &diagnostics.aggregation;
    for family in &aggregation.ranked_concept_families {
        let concept_id =
            KnowledgeGraphIr::node_id(&KnowledgeNodeKind::ConceptSeedHypothesis, &family.root_concept);
        graph.upsert_node(KnowledgeNode {
            id: concept_id.clone(),
            kind: KnowledgeNodeKind::ConceptSeedHypothesis,
            label: family.root_concept.clone(),
            properties: concept_properties(family),
            observation: KnowledgeObservationKind::Inferred,
            confidence: Some(family.final_seed_score),
            state: Some(family.concept_role.role_class.clone()),
            provenance: Some("domainSeedAggregation".into()),
            source_location: None,
        });
        let responsibility_id = KnowledgeGraphIr::node_id(
            &KnowledgeNodeKind::ResponsibilityDomain,
            &responsibility_domain_id(&family.support_signature.signature_key),
        );
        graph.upsert_node(KnowledgeNode {
            id: responsibility_id.clone(),
            kind: KnowledgeNodeKind::ResponsibilityDomain,
            label: responsibility_domain_id(&family.support_signature.signature_key),
            properties: responsibility_domain_properties(family),
            observation: KnowledgeObservationKind::Inferred,
            confidence: Some(family.coverage_score),
            state: None,
            provenance: Some("responsibilitySignature".into()),
            source_location: None,
        });
        graph.upsert_edge(KnowledgeEdge {
            id: KnowledgeGraphIr::edge_id(
                &KnowledgeEdgeKind::SemanticHintFor,
                &concept_id,
                &responsibility_id,
            ),
            kind: KnowledgeEdgeKind::SemanticHintFor,
            from_id: concept_id.clone(),
            to_id: responsibility_id,
            observation: KnowledgeObservationKind::Inferred,
            confidence: Some(family.final_seed_score),
            state: Some("semanticHint".into()),
            provenance: Some("conceptAsHint".into()),
            properties: BTreeMap::new(),
        });
        for capability_key in &family.distinct_capability_keys {
            let capability_id =
                KnowledgeGraphIr::node_id(&KnowledgeNodeKind::Capability, capability_key);
            graph.upsert_edge(KnowledgeEdge {
                id: KnowledgeGraphIr::edge_id(
                    &KnowledgeEdgeKind::BelongsTo,
                    &capability_id,
                    &concept_id,
                ),
                kind: KnowledgeEdgeKind::BelongsTo,
                from_id: capability_id,
                to_id: concept_id.clone(),
                observation: KnowledgeObservationKind::Inferred,
                confidence: Some(family.coverage_score),
                state: None,
                provenance: Some("domainSeedAggregation".into()),
                properties: BTreeMap::new(),
            });
        }
    }

    for edge in &aggregation.anchor_capability_graph.edges {
        let capability_id =
            KnowledgeGraphIr::node_id(&KnowledgeNodeKind::Capability, &edge.capability_key);
        let hypothesis_id = KnowledgeGraphIr::node_id(
            &KnowledgeNodeKind::ConceptSeedHypothesis,
            &edge.representative_root_concept,
        );
        let mut properties = BTreeMap::new();
        properties.insert(
            "symbolicAffinityScore".into(),
            serde_json::json!(edge.symbolic_affinity_score),
        );
        properties.insert(
            "retrievalChannels".into(),
            serde_json::json!(edge.retrieval_channels),
        );
        graph.upsert_edge(KnowledgeEdge {
            id: KnowledgeGraphIr::edge_id(
                &KnowledgeEdgeKind::CandidateFor,
                &hypothesis_id,
                &capability_id,
            ),
            kind: KnowledgeEdgeKind::CandidateFor,
            from_id: hypothesis_id,
            to_id: capability_id,
            observation: KnowledgeObservationKind::Inferred,
            confidence: Some(edge.symbolic_affinity_score),
            state: None,
            provenance: Some("anchorCapabilityAffinity".into()),
            properties,
        });
    }

    for edge in &diagnostics
        .aggregation
        .domain_anchor_eligibility
        .concept_hierarchy
        .parent_subconcept_edges
    {
        let parent_id = KnowledgeGraphIr::node_id(
            &KnowledgeNodeKind::ConceptSeedHypothesis,
            &edge.parent_root_concept,
        );
        let child_id = KnowledgeGraphIr::node_id(
            &KnowledgeNodeKind::ConceptSeedHypothesis,
            &edge.child_root_concept,
        );
        let mut properties = BTreeMap::new();
        properties.insert("signals".into(), serde_json::json!(edge.signals));
        properties.insert(
            "parentHypothesisId".into(),
            serde_json::json!(edge.parent_hypothesis_id),
        );
        properties.insert(
            "childHypothesisId".into(),
            serde_json::json!(edge.child_hypothesis_id),
        );
        graph.upsert_edge(KnowledgeEdge {
            id: KnowledgeGraphIr::edge_id(
                &KnowledgeEdgeKind::Contains,
                &parent_id,
                &child_id,
            ),
            kind: KnowledgeEdgeKind::Contains,
            from_id: parent_id,
            to_id: child_id,
            observation: KnowledgeObservationKind::Inferred,
            confidence: Some(edge.confidence),
            state: None,
            provenance: Some("conceptHierarchy".into()),
            properties,
        });
    }

    let equivalence = &diagnostics
        .aggregation
        .anchor_capability_graph
        .assignment_ambiguity
        .responsibility_equivalence;
    for record in &equivalence.representative_anchor_pairs {
        if record.equivalence_class
            != super::domain_seed_responsibility_equivalence::EQUIVALENCE_CLASS_EQUIVALENT
        {
            continue;
        }
        let left_id = KnowledgeGraphIr::node_id(
            &KnowledgeNodeKind::ConceptSeedHypothesis,
            &record.left_root_concept,
        );
        let right_id = KnowledgeGraphIr::node_id(
            &KnowledgeNodeKind::ConceptSeedHypothesis,
            &record.right_root_concept,
        );
        let mut properties = BTreeMap::new();
        properties.insert(
            "equivalenceClass".into(),
            serde_json::json!(record.equivalence_class),
        );
        properties.insert(
            "neighborhoodScore".into(),
            serde_json::json!(record.neighborhood_score),
        );
        graph.upsert_edge(KnowledgeEdge {
            id: KnowledgeGraphIr::edge_id(
                &KnowledgeEdgeKind::ResponsibilityEquivalent,
                &left_id,
                &right_id,
            ),
            kind: KnowledgeEdgeKind::ResponsibilityEquivalent,
            from_id: left_id,
            to_id: right_id,
            observation: KnowledgeObservationKind::Inferred,
            confidence: Some(record.neighborhood_score),
            state: None,
            provenance: Some("responsibilityEquivalence".into()),
            properties,
        });
    }

    let scope_diagnostics = &diagnostics
        .aggregation
        .anchor_capability_graph
        .assignment_ambiguity
        .responsibility_scope;
    for record in &scope_diagnostics.anchor_scope_records {
        let concept_id = KnowledgeGraphIr::node_id(
            &KnowledgeNodeKind::ConceptSeedHypothesis,
            &record.representative_root_concept,
        );
        let scope_label = super::domain_seed_responsibility_scope::scope_node_id(
            &record.hypothesis_id,
        );
        let scope_id =
            KnowledgeGraphIr::node_id(&KnowledgeNodeKind::Scope, &scope_label);
        let mut scope_properties = BTreeMap::new();
        scope_properties.insert("scopeClass".into(), serde_json::json!(record.scope_class));
        scope_properties.insert("fanoutRatio".into(), serde_json::json!(record.fanout_ratio));
        scope_properties.insert(
            "capabilityDispersion".into(),
            serde_json::json!(record.capability_dispersion),
        );
        graph.upsert_node(KnowledgeNode {
            id: scope_id.clone(),
            kind: KnowledgeNodeKind::Scope,
            label: scope_label,
            properties: scope_properties,
            observation: KnowledgeObservationKind::Inferred,
            confidence: Some(record.scope_score),
            state: Some(record.scope_class.clone()),
            provenance: Some("responsibilityScope".into()),
            source_location: None,
        });
        if record.scope_class == super::domain_seed_responsibility_scope::SCOPE_CLASS_SCOPE {
            graph.upsert_edge(KnowledgeEdge {
                id: KnowledgeGraphIr::edge_id(
                    &KnowledgeEdgeKind::HasStructuralScope,
                    &concept_id,
                    &scope_id,
                ),
                kind: KnowledgeEdgeKind::HasStructuralScope,
                from_id: concept_id,
                to_id: scope_id,
                observation: KnowledgeObservationKind::Inferred,
                confidence: Some(record.scope_score),
                state: None,
                provenance: Some("responsibilityScope".into()),
                properties: BTreeMap::new(),
            });
        }
    }

    graph
}

fn capability_properties(
    seed: &super::domain_seed_diagnostics::CapabilityDomainSeeds,
) -> BTreeMap<String, serde_json::Value> {
    let mut properties = BTreeMap::new();
    properties.insert(
        "ownerClasses".into(),
        serde_json::json!(seed.coverage.owner_classes),
    );
    properties.insert(
        "modulePaths".into(),
        serde_json::json!(seed.coverage.module_paths),
    );
    properties.insert(
        "packagePaths".into(),
        serde_json::json!(seed.coverage.package_paths),
    );
    properties.insert(
        "contractPaths".into(),
        serde_json::json!(seed.coverage.contract_paths),
    );
    properties.insert(
        "entrypointIds".into(),
        serde_json::json!(seed.coverage.entrypoint_ids),
    );
    properties
}

fn evidence_properties(
    candidate: &super::domain_seed_diagnostics::DomainSeedCandidate,
) -> BTreeMap<String, serde_json::Value> {
    let mut properties = BTreeMap::new();
    properties.insert(
        "evidenceSource".into(),
        serde_json::json!(candidate.evidence_source),
    );
    properties.insert("concept".into(), serde_json::json!(candidate.concept));
    properties.insert(
        "confidence".into(),
        serde_json::json!(format!("{:?}", candidate.confidence)),
    );
    properties
}

fn evidence_source_location(
    raw: &super::domain_seed_diagnostics::DomainSeedRawEvidence,
) -> Option<KnowledgeSourceLocation> {
    Some(KnowledgeSourceLocation {
        file_path: raw.module.clone().or(raw.package.clone()),
        unit_id: raw.unit_name.clone(),
        entrypoint_id: None,
    })
}

fn responsibility_domain_properties(
    family: &super::domain_seed_aggregation::RankedConceptFamily,
) -> BTreeMap<String, serde_json::Value> {
    let mut properties = BTreeMap::new();
    properties.insert(
        "responsibilityId".into(),
        serde_json::json!(responsibility_domain_id(
            &family.support_signature.signature_key
        )),
    );
    properties.insert(
        "signatureKey".into(),
        serde_json::json!(family.support_signature.signature_key),
    );
    properties.insert(
        "capabilityKeys".into(),
        serde_json::json!(family.distinct_capability_keys),
    );
    properties.insert(
        "entrypointIds".into(),
        serde_json::json!(family.distinct_entrypoint_ids),
    );
    properties.insert(
        "ownerClasses".into(),
        serde_json::json!(family.distinct_owner_classes),
    );
    properties
}

fn concept_properties(
    family: &super::domain_seed_aggregation::RankedConceptFamily,
) -> BTreeMap<String, serde_json::Value> {
    let mut properties = BTreeMap::new();
    properties.insert("familyId".into(), serde_json::json!(family_id(family)));
    properties.insert(
        "semanticHintOnly".into(),
        serde_json::json!(true),
    );
    properties.insert(
        "atomizedPath".into(),
        serde_json::json!(family.atomized_path),
    );
    properties.insert("genericness".into(), serde_json::json!(family.genericness));
    properties.insert(
        "transportness".into(),
        serde_json::json!(family.transportness),
    );
    properties.insert(
        "roleClass".into(),
        serde_json::json!(family.concept_role.role_class),
    );
    properties
}
