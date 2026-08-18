//! 정적 Facts를 도메인과 실행 흐름 사이의 기능 그룹으로 변환한다.

mod feature_build;
mod feature_contracts;
mod feature_flows;

use feature_build::{FeatureBuildContext, FeatureBuildInput};
use feature_contracts::{contract_feature_key, group_entrypoints_by_contract};
use feature_flows::FeatureFlowIndex;

use super::model::{FeatureGroup, FeatureKind, FeatureStatus, FeatureVisibility};
use super::reachability::ReachabilityIndex;
use crate::config::PathPolicy;
use crate::domain::contract_path::paths_match;
use crate::domain::DomainAnalysisOutput;
use crate::facts::{
    CodeUnitKind, Entrypoint, EntrypointKind, FactStore, Reference, ReferenceKind, ResourceAccess,
    ResourceKind,
};
use crate::flow::ExecutionFlowGraph;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// entrypoint 중심으로 호출 그래프를 묶어 정적 기능 그룹을 만든다.
pub(crate) fn build(
    analysis: &DomainAnalysisOutput,
    facts: &FactStore,
    flows: &ExecutionFlowGraph,
    path_policy: &PathPolicy,
) -> Vec<FeatureGroup> {
    let domains_by_unit = domains_by_unit(analysis);
    let outgoing = outgoing_references(facts);
    let reachability = ReachabilityIndex::build(facts);
    let resources_by_unit = resources_by_unit(facts);
    let context = FeatureBuildContext {
        domains_by_unit: &domains_by_unit,
        outgoing: &outgoing,
        facts,
        resources_by_unit: &resources_by_unit,
        reachability: &reachability,
    };
    let file_units = file_units_by_file(facts);
    let flow_index = FeatureFlowIndex::new(flows);
    let mut features = Vec::new();
    let mut eligible_entrypoints = Vec::new();

    for entrypoint in &facts.entrypoints {
        if !is_feature_entrypoint_kind(&entrypoint.kind) {
            continue;
        }
        let Some(unit) = facts.unit(&entrypoint.unit_id) else {
            continue;
        };
        if !path_policy.is_production_path(&unit.relative_path)
            && !path_policy.is_archived_path(&unit.relative_path)
        {
            continue;
        }
        eligible_entrypoints.push(entrypoint);
    }

    let contract_groups = group_entrypoints_by_contract(eligible_entrypoints.iter().copied());
    for mut group in contract_groups {
        sort_entrypoints_for_representative(&mut group.entrypoints, facts, path_policy);
        let representative = group.entrypoints[0];
        let feature_key = contract_feature_key(representative);
        let mut owner_set = BTreeSet::new();
        let mut scope_set = BTreeSet::new();
        for entrypoint in &group.entrypoints {
            owner_set.extend(owner_units_for_entrypoint(
                &entrypoint.unit_id,
                facts,
                &file_units,
                &reachability,
            ));
            scope_set.extend(reachability.reachable_from(&entrypoint.unit_id));
        }
        let owner_units: Vec<_> = owner_set.into_iter().collect();
        let scope_units: Vec<_> = scope_set.into_iter().collect();
        let entrypoint_unit_ids: Vec<&str> = group
            .entrypoints
            .iter()
            .map(|entrypoint| entrypoint.unit_id.as_str())
            .collect();
        let flow_ids = flow_index.collect_for_units(&entrypoint_unit_ids);
        let mut feature = context.build_feature(FeatureBuildInput {
            kind: FeatureKind::Endpoint,
            base_status: FeatureStatus::Confirmed,
            visibility: FeatureVisibility::UserFacing,
            entrypoint: Some(representative),
            feature_key: Some(&feature_key),
            operation_root_id: None,
            owner_units: &owner_units,
            scope_units: &scope_units,
            reachable_unit_count: scope_units.len(),
            flow_ids: &flow_ids,
        });
        feature.entrypoint_ids = group
            .entrypoints
            .iter()
            .map(|entrypoint| entrypoint.id.clone())
            .collect();
        for resource in &facts.resources {
            if !matches!(
                resource.kind,
                ResourceKind::ExternalApi | ResourceKind::WebSocket
            ) {
                continue;
            }
            let matches_contract = group.entrypoints.iter().any(|entrypoint| {
                let raw = entrypoint.path.as_deref().unwrap_or(&entrypoint.name);
                paths_match(&resource.name, raw)
            });
            if matches_contract && !feature.resource_ids.contains(&resource.id) {
                feature.resource_ids.push(resource.id.clone());
            }
        }
        features.push(feature);
    }

    if facts.entrypoints.is_empty() {
        append_operation_features(
            &mut features,
            flows,
            facts,
            &file_units,
            &reachability,
            &context,
            &flow_index,
        );
    }

    features.sort_by(|left, right| left.key.cmp(&right.key));
    features
}

fn sort_entrypoints_for_representative(
    entrypoints: &mut [&Entrypoint],
    facts: &FactStore,
    path_policy: &PathPolicy,
) {
    entrypoints.sort_by(|left, right| {
        let left_production = facts
            .unit(&left.unit_id)
            .is_some_and(|unit| path_policy.is_production_path(&unit.relative_path));
        let right_production = facts
            .unit(&right.unit_id)
            .is_some_and(|unit| path_policy.is_production_path(&unit.relative_path));
        let left_method = left
            .method
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
        let right_method = right
            .method
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
        right_production
            .cmp(&left_production)
            .then_with(|| right_method.cmp(&left_method))
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn domains_by_unit(analysis: &DomainAnalysisOutput) -> HashMap<String, Vec<String>> {
    let mut result: HashMap<String, BTreeSet<String>> = HashMap::new();
    for membership in &analysis.memberships {
        let domains = result.entry(membership.unit_id.clone()).or_default();
        if let Some(domain_id) = &membership.domain_id {
            domains.insert(domain_id.clone());
        }
        domains.extend(membership.domain_ids.iter().cloned());
    }
    result
        .into_iter()
        .map(|(unit_id, domain_ids)| (unit_id, domain_ids.into_iter().collect()))
        .collect()
}

fn outgoing_references(facts: &FactStore) -> HashMap<String, Vec<&Reference>> {
    let mut result: HashMap<String, Vec<&Reference>> = HashMap::new();
    for reference in &facts.references {
        if !matches!(
            reference.kind,
            ReferenceKind::Call | ReferenceKind::Constructs
        ) {
            continue;
        }
        result
            .entry(reference.source_unit_id.clone())
            .or_default()
            .push(reference);
    }
    result
}

fn resources_by_unit(facts: &FactStore) -> HashMap<String, Vec<&ResourceAccess>> {
    let mut result = HashMap::new();
    for resource in &facts.resources {
        result
            .entry(resource.unit_id.clone())
            .or_insert_with(Vec::new)
            .push(resource);
    }
    result
}

fn append_operation_features(
    features: &mut Vec<FeatureGroup>,
    flows: &ExecutionFlowGraph,
    facts: &FactStore,
    file_units: &HashMap<String, String>,
    reachability: &ReachabilityIndex,
    context: &FeatureBuildContext<'_>,
    flow_index: &FeatureFlowIndex<'_>,
) {
    let mut covered_units = HashSet::new();
    let mut operation_owners: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for flow in &flows.flows {
        let Some(owner_index) = reachability.unit_index(&flow.owner_unit_id) else {
            continue;
        };
        if covered_units.contains(&owner_index) {
            continue;
        }
        let operation_root = operation_root(&flow.owner_unit_id, facts, file_units);
        operation_owners
            .entry(operation_root)
            .or_default()
            .insert(flow.owner_unit_id.clone());
    }
    for (operation_root, owners) in operation_owners {
        let scope_units = reachability.reachable_from_many(owners.iter().map(String::as_str));
        let mut owner_units = owners
            .iter()
            .filter_map(|owner| reachability.unit_index(owner))
            .collect::<Vec<_>>();
        owner_units.sort_unstable();
        owner_units.dedup();
        if let Some(root_index) = reachability.unit_index(&operation_root) {
            if owner_units.binary_search(&root_index).is_err() {
                owner_units.push(root_index);
                owner_units.sort_unstable();
            }
            covered_units.extend(owner_units.iter().copied());
        }
        let owner_unit_ids: Vec<&str> = owners.iter().map(String::as_str).collect();
        let flow_ids = flow_index.collect_for_units(&owner_unit_ids);
        features.push(context.build_feature(FeatureBuildInput {
            kind: FeatureKind::Operation,
            base_status: FeatureStatus::Candidate,
            visibility: FeatureVisibility::Internal,
            entrypoint: None,
            feature_key: None,
            operation_root_id: Some(operation_root.as_str()),
            owner_units: &owner_units,
            scope_units: &scope_units,
            reachable_unit_count: scope_units.len(),
            flow_ids: &flow_ids,
        }));
    }
}

fn owner_units_for_entrypoint(
    entrypoint_unit_id: &str,
    facts: &FactStore,
    file_units: &HashMap<String, String>,
    reachability: &ReachabilityIndex,
) -> Vec<usize> {
    let mut owner_units = Vec::new();
    if let Some(index) = reachability.unit_index(entrypoint_unit_id) {
        owner_units.push(index);
    }
    let operation_root = operation_root(entrypoint_unit_id, facts, file_units);
    if let Some(index) = reachability.unit_index(&operation_root) {
        if !owner_units.contains(&index) {
            owner_units.push(index);
        }
    }
    owner_units.sort_unstable();
    owner_units
}

fn is_feature_entrypoint_kind(kind: &EntrypointKind) -> bool {
    matches!(
        kind,
        EntrypointKind::Http
            | EntrypointKind::WebSocket
            | EntrypointKind::Rpc
            | EntrypointKind::Job
    )
}

fn file_units_by_file(facts: &FactStore) -> HashMap<String, String> {
    facts
        .units
        .values()
        .filter(|unit| unit.kind == CodeUnitKind::File)
        .map(|unit| (unit.file_id.clone(), unit.id.clone()))
        .collect()
}

fn operation_root(
    unit_id: &str,
    facts: &FactStore,
    file_units: &HashMap<String, String>,
) -> String {
    let Some(unit) = facts.unit(unit_id) else {
        return unit_id.to_string();
    };
    let mut current = unit;
    while let Some(parent_id) = current.parent_id.as_deref() {
        let Some(parent) = facts.unit(parent_id) else {
            break;
        };
        if !is_flow_unit(&parent.kind) {
            return parent.id.clone();
        }
        current = parent;
    }

    file_units
        .get(&unit.file_id)
        .cloned()
        .unwrap_or_else(|| unit.id.clone())
}

fn is_flow_unit(kind: &CodeUnitKind) -> bool {
    matches!(
        kind,
        CodeUnitKind::Function
            | CodeUnitKind::Method
            | CodeUnitKind::Constructor
            | CodeUnitKind::Lambda
    )
}
