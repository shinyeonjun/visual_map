use crate::clean::PreparedStaticOverview;
use crate::domain::capability_keys::canonical_capability_key;
use crate::domain::contract_path::contract_identity;
use crate::domain::DomainKind;
use crate::domain::DomainSeedDiagnostics;
use crate::eval::gold_pairs::contract_to_capability_key;
use crate::flow::{ExecutionFlowGraph, FlowEdgeKind};
use crate::views::overview::OverviewResponse;
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Clone)]
pub struct EvalSnapshot {
    pub domains: Vec<SnapDomain>,
    pub features: Vec<SnapFeature>,
    pub flows: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct SnapDomain {
    pub id: String,
    pub key: String,
    pub kind: DomainKind,
}

#[derive(Debug, Clone)]
pub struct SnapFeature {
    pub id: String,
    pub key: String,
    pub domain_keys: Vec<String>,
    pub contracts: Vec<String>,
    pub flow_ids: Vec<String>,
}

pub fn snapshot_from_overview(overview: &OverviewResponse) -> EvalSnapshot {
    let domain_key = |id: &str| {
        overview
            .domains
            .iter()
            .find(|domain| domain.id == id)
            .map(|domain| domain.key.clone())
    };
    let entry_contract = |id: &str| {
        overview.entrypoints.iter().find(|entry| entry.id == id).and_then(|entry| {
            contract_identity(
                entry.method.as_deref(),
                entry.path.as_deref().unwrap_or(&entry.name),
            )
        })
    };
    snapshot(
        overview
            .domains
            .iter()
            .map(|domain| SnapDomain {
                id: domain.id.clone(),
                key: domain.key.clone(),
                kind: domain.kind,
            })
            .collect(),
        overview
            .features
            .iter()
            .map(|feature| {
                let mut contracts: Vec<String> = feature
                    .entrypoint_ids
                    .iter()
                    .filter_map(|id| entry_contract(id))
                    .collect();
                if let Some(from_key) = contract_from_feature_key(&feature.key) {
                    contracts.push(from_key);
                }
                contracts.sort();
                contracts.dedup();
                SnapFeature {
                    id: feature.id.clone(),
                    key: feature.key.clone(),
                    domain_keys: feature.domain_ids.iter().filter_map(|id| domain_key(id)).collect(),
                    contracts,
                    flow_ids: feature.flow_ids.clone(),
                }
            })
            .collect(),
        &overview.execution_flows,
    )
}

/// Competitive ownership assignment 결과로 latent domain snapshot을 만든다.
pub fn snapshot_from_domain_recovery(
    overview: &OverviewResponse,
    diagnostics: &DomainSeedDiagnostics,
) -> EvalSnapshot {
    let capability_domains = build_capability_domain_map(diagnostics);
    let domain_keys: BTreeSet<String> = capability_domains.values().cloned().collect();
    let domains = domain_keys
        .into_iter()
        .map(|key| SnapDomain {
            id: format!("recovery:{key}"),
            key: key.clone(),
            kind: DomainKind::Business,
        })
        .collect();

    let entry_contract = |id: &str| {
        overview.entrypoints.iter().find(|entry| entry.id == id).and_then(|entry| {
            contract_identity(
                entry.method.as_deref(),
                entry.path.as_deref().unwrap_or(&entry.name),
            )
        })
    };

    let features = overview
        .features
        .iter()
        .map(|feature| {
            let mut contracts: Vec<String> = feature
                .entrypoint_ids
                .iter()
                .filter_map(|id| entry_contract(id))
                .collect();
            if let Some(from_key) = contract_from_feature_key(&feature.key) {
                contracts.push(from_key);
            }
            contracts.sort();
            contracts.dedup();

            let mut domain_keys = BTreeSet::new();
            for contract in &contracts {
                if let Some(capability_key) = contract_to_capability_key(contract) {
                    if let Some(domain_key) = capability_domains.get(&capability_key) {
                        domain_keys.insert(domain_key.clone());
                        continue;
                    }
                }
                for (capability_key, domain_key) in &capability_domains {
                    if contracts_match_capability(contract, capability_key) {
                        domain_keys.insert(domain_key.clone());
                    }
                }
            }

            SnapFeature {
                id: feature.id.clone(),
                key: feature.key.clone(),
                domain_keys: domain_keys.into_iter().collect(),
                contracts,
                flow_ids: feature.flow_ids.clone(),
            }
        })
        .collect();

    snapshot(domains, features, &overview.execution_flows)
}

pub(crate) fn build_capability_domain_map(
    diagnostics: &DomainSeedDiagnostics,
) -> BTreeMap<String, String> {
    let assignments = &diagnostics
        .aggregation
        .anchor_capability_graph
        .capability_assignments;
    let mut map = BTreeMap::new();
    for assignment in assignments {
        if assignment.retrieved_candidate_count == 0 || assignment.assignment_state == "unassigned" {
            continue;
        }
        let Some(top) = assignment.top_candidates.first() else {
            continue;
        };
        let domain_key = canonical_capability_key(&top.representative_root_concept);
        map.insert(
            canonical_capability_key(&assignment.capability_key),
            domain_key,
        );
    }
    map
}

fn contracts_match_capability(contract: &str, capability_key: &str) -> bool {
    let Some(expected) = contract_to_capability_key(contract) else {
        return false;
    };
    canonical_capability_key(capability_key) == expected
        || canonical_capability_key(capability_key).ends_with(&expected)
        || expected.ends_with(&canonical_capability_key(capability_key))
}

pub fn snapshot_from_clean(prepared: &PreparedStaticOverview) -> EvalSnapshot {
    let domain_key = |id: &str| {
        prepared
            .domains
            .iter()
            .find(|domain| domain.id == id)
            .map(|domain| domain.candidate_key.clone())
    };
    let entry_contract = |id: &str| {
        prepared.entrypoints.iter().find(|entry| entry.id == id).and_then(|entry| {
            contract_identity(
                entry.method.as_deref(),
                entry.path.as_deref().unwrap_or(&entry.name),
            )
        })
    };
    snapshot(
        prepared
            .domains
            .iter()
            .map(|domain| SnapDomain {
                id: domain.id.clone(),
                key: domain.candidate_key.clone(),
                kind: domain.kind,
            })
            .collect(),
        prepared
            .features
            .iter()
            .map(|feature| {
                let mut contracts: Vec<String> = feature
                    .entrypoint_ids
                    .iter()
                    .filter_map(|id| entry_contract(id))
                    .collect();
                if let Some(from_key) = contract_from_feature_key(&feature.candidate_key) {
                    contracts.push(from_key);
                }
                contracts.sort();
                contracts.dedup();
                SnapFeature {
                    id: feature.id.clone(),
                    key: feature.candidate_key.clone(),
                    domain_keys: feature.domain_ids.iter().filter_map(|id| domain_key(id)).collect(),
                    contracts,
                    flow_ids: feature.flow_ids.clone(),
                }
            })
            .collect(),
        &prepared.execution_flows,
    )
}

fn snapshot(
    domains: Vec<SnapDomain>,
    features: Vec<SnapFeature>,
    flows: &ExecutionFlowGraph,
) -> EvalSnapshot {
    let flows = flows
        .flows
        .iter()
        .map(|flow| {
            let kinds = flow
                .edges
                .iter()
                .map(|edge| edge_kind_name(&edge.kind).to_string())
                .collect();
            (flow.id.clone(), kinds)
        })
        .collect();
    EvalSnapshot {
        domains,
        features,
        flows,
    }
}

fn contract_from_feature_key(key: &str) -> Option<String> {
    key.strip_prefix("contract:").map(str::to_string)
}

fn edge_kind_name(kind: &FlowEdgeKind) -> &'static str {
    match kind {
        FlowEdgeKind::Sequential => "sequential",
        FlowEdgeKind::TrueBranch => "trueBranch",
        FlowEdgeKind::FalseBranch => "falseBranch",
        FlowEdgeKind::LoopBody => "loopBody",
        FlowEdgeKind::LoopBack => "loopBack",
        FlowEdgeKind::Call => "call",
        FlowEdgeKind::Return => "return",
        FlowEdgeKind::Exception => "exception",
        FlowEdgeKind::Dynamic => "dynamic",
    }
}
