//! Canonical Knowledge Graph IR for domain/feature/flow restoration.
//!
//! Expression layer only: does not participate in domain recovery scoring.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum KnowledgeNodeKind {
    Domain,
    Feature,
    Capability,
    Flow,
    FlowStep,
    Entrypoint,
    FunctionClass,
    ModulePackage,
    EntityResource,
    ConceptSeedHypothesis,
    ResponsibilityDomain,
    Scope,
    Evidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum KnowledgeEdgeKind {
    Contains,
    Calls,
    Reads,
    Writes,
    Owns,
    HasEntrypoint,
    SupportedBy,
    DerivedFrom,
    CandidateFor,
    BelongsTo,
    SemanticHintFor,
    ResponsibilityEquivalent,
    HasStructuralScope,
    HasFeature,
    HasFlow,
    Next,
    Branch,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum KnowledgeObservationKind {
    Observed,
    Inferred,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSourceLocation {
    pub file_path: Option<String>,
    pub unit_id: Option<String>,
    pub entrypoint_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeNode {
    pub id: String,
    pub kind: KnowledgeNodeKind,
    pub label: String,
    pub properties: BTreeMap<String, serde_json::Value>,
    pub observation: KnowledgeObservationKind,
    pub confidence: Option<f64>,
    pub state: Option<String>,
    pub provenance: Option<String>,
    pub source_location: Option<KnowledgeSourceLocation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeEdge {
    pub id: String,
    pub kind: KnowledgeEdgeKind,
    pub from_id: String,
    pub to_id: String,
    pub observation: KnowledgeObservationKind,
    pub confidence: Option<f64>,
    pub state: Option<String>,
    pub provenance: Option<String>,
    pub properties: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeGraphIr {
    pub nodes: Vec<KnowledgeNode>,
    pub edges: Vec<KnowledgeEdge>,
}

impl KnowledgeGraphIr {
    pub fn node_id(kind: &KnowledgeNodeKind, key: &str) -> String {
        format!("{kind:?}:{key}")
    }

    pub fn edge_id(kind: &KnowledgeEdgeKind, from_id: &str, to_id: &str) -> String {
        format!("{kind:?}:{from_id}->{to_id}")
    }

    pub fn upsert_node(&mut self, node: KnowledgeNode) {
        if self.nodes.iter().any(|existing| existing.id == node.id) {
            return;
        }
        self.nodes.push(node);
    }

    pub fn upsert_edge(&mut self, edge: KnowledgeEdge) {
        if self.edges.iter().any(|existing| existing.id == edge.id) {
            return;
        }
        self.edges.push(edge);
    }
}
