//! 정적 Facts를 도메인과 실행 흐름 사이의 기능 그룹으로 변환한다.

use super::model::{
    FeatureConfidence, FeatureGroup, FeatureKind, FeatureStatus, FeatureVisibility,
};
use super::reachability::ReachabilityIndex;
use crate::domain::DomainAnalysisOutput;
use crate::facts::{
    CodeUnitKind, Entrypoint, FactStore, Reference, ReferenceKind, ResolutionStatus, ResourceAccess,
};
use crate::flow::{ExecutionFlow, ExecutionFlowGraph};
use crate::languages::common::metadata::stable_id;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// entrypoint 중심으로 호출 그래프를 묶어 정적 기능 그룹을 만든다.
pub(crate) fn build(
    analysis: &DomainAnalysisOutput,
    facts: &FactStore,
    flows: &ExecutionFlowGraph,
) -> Vec<FeatureGroup> {
    let domains_by_unit = domains_by_unit(analysis);
    let outgoing = outgoing_references(facts);
    let reachability = ReachabilityIndex::build(facts);
    let flows_by_owner = flows_by_owner(flows);
    let resources_by_unit = resources_by_unit(facts);
    let context = FeatureBuildContext {
        domains_by_unit: &domains_by_unit,
        outgoing: &outgoing,
        facts,
        flows_by_owner: &flows_by_owner,
        resources_by_unit: &resources_by_unit,
        reachability: &reachability,
    };
    let file_units = file_units_by_file(facts);
    let mut features = Vec::new();

    for entrypoint in &facts.entrypoints {
        let scope_units = reachability.reachable_from(&entrypoint.unit_id);
        let owner_units = owner_units_for_entrypoint(
            &entrypoint.unit_id,
            facts,
            &file_units,
            &reachability,
        );
        let feature = context.build_feature(FeatureBuildInput {
            kind: FeatureKind::Endpoint,
            base_status: FeatureStatus::Confirmed,
            visibility: FeatureVisibility::UserFacing,
            entrypoint: Some(entrypoint),
            operation_root_id: None,
            owner_units: &owner_units,
            scope_units: &scope_units,
            reachable_unit_count: scope_units.len(),
        });
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
        );
    } else {
        assign_orphan_flows_to_nearest_endpoint(&mut features, flows, facts, &reachability);
    }

    features.sort_by(|left, right| left.key.cmp(&right.key));
    features
}

struct FeatureBuildContext<'a> {
    domains_by_unit: &'a HashMap<String, Vec<String>>,
    outgoing: &'a HashMap<String, Vec<&'a Reference>>,
    facts: &'a FactStore,
    flows_by_owner: &'a HashMap<String, Vec<&'a ExecutionFlow>>,
    resources_by_unit: &'a HashMap<String, Vec<&'a ResourceAccess>>,
    reachability: &'a ReachabilityIndex,
}

struct FeatureBuildInput<'a> {
    kind: FeatureKind,
    base_status: FeatureStatus,
    visibility: FeatureVisibility,
    entrypoint: Option<&'a Entrypoint>,
    operation_root_id: Option<&'a str>,
    owner_units: &'a [usize],
    scope_units: &'a [usize],
    reachable_unit_count: usize,
}

impl FeatureBuildContext<'_> {
    fn build_feature(&self, input: FeatureBuildInput<'_>) -> FeatureGroup {
        let root_unit_id = input
            .operation_root_id
            .or_else(|| {
                input.entrypoint.map(|value| value.unit_id.as_str()).or_else(|| {
                    input
                        .owner_units
                        .first()
                        .map(|index| self.reachability.unit_id(*index))
                })
            })
            .unwrap_or_default();
        let key = input
            .entrypoint
            .map(|value| format!("entrypoint:{}", value.id))
            .unwrap_or_else(|| format!("operation:{}", root_unit_id));
        let id = stable_id("feature", &key);
        let root_unit = self.facts.unit(root_unit_id);

        let mut domain_ids = BTreeSet::new();
        for &unit_index in input.owner_units {
            let unit_id = self.reachability.unit_id(unit_index);
            if let Some(ids) = self.domains_by_unit.get(unit_id) {
                domain_ids.extend(ids.iter().cloned());
            }
        }

        let mut resolved_edge_count = 0;
        let mut unresolved_edge_count = 0;
        let mut dynamic_edge_count = 0;
        let mut dynamic_boundary_ids = Vec::new();
        let mut evidence = Vec::new();
        if let Some(unit) = root_unit {
            evidence.push(crate::facts::Evidence::new(
                "featureUnit",
                unit.qualified_name.clone(),
                unit.span.clone(),
            ));
        }
        if let Some(entrypoint) = input.entrypoint {
            evidence.extend(entrypoint.evidence.clone());
        }

        for &unit_index in input.scope_units {
            let unit_id = self.reachability.unit_id(unit_index);
            for reference in self.outgoing.get(unit_id).into_iter().flatten() {
                match reference.status {
                    ResolutionStatus::Confirmed => resolved_edge_count += 1,
                    ResolutionStatus::Candidate => unresolved_edge_count += 1,
                    ResolutionStatus::Unknown => unresolved_edge_count += 1,
                    ResolutionStatus::Dynamic => {
                        dynamic_edge_count += 1;
                        dynamic_boundary_ids.push(reference.id.clone());
                    }
                }
                evidence.extend(reference.evidence.clone());
            }
        }
        evidence.sort_by(|left, right| left.id.cmp(&right.id));
        evidence.dedup_by(|left, right| left.id == right.id);
        evidence.truncate(24);
        dynamic_boundary_ids.sort();
        dynamic_boundary_ids.dedup();

        let flow_ids = input
            .scope_units
            .iter()
            .filter_map(|index| self.flows_by_owner.get(self.reachability.unit_id(*index)))
            .flatten()
            .map(|flow| flow.id.clone())
            .collect::<Vec<_>>();
        let resource_ids = input
            .scope_units
            .iter()
            .filter_map(|index| {
                self.resources_by_unit
                    .get(self.reachability.unit_id(*index))
            })
            .flatten()
            .map(|resource| resource.id.clone())
            .collect::<Vec<_>>();
        let status = if dynamic_edge_count > 0 || unresolved_edge_count > 0 {
            if matches!(input.base_status, FeatureStatus::Confirmed) {
                FeatureStatus::Candidate
            } else {
                input.base_status.clone()
            }
        } else {
            input.base_status.clone()
        };

        let label = input
            .entrypoint
            .map(entrypoint_label)
            .or_else(|| root_unit.map(|unit| unit.qualified_name.clone()))
            .unwrap_or_else(|| key.clone());

        FeatureGroup {
            id,
            key,
            label,
            kind: input.kind,
            status,
            visibility: input.visibility,
            confidence: FeatureConfidence {
                level: confidence_level(
                    resolved_edge_count,
                    unresolved_edge_count,
                    dynamic_edge_count,
                ),
                resolved_edge_count,
                unresolved_edge_count,
                dynamic_edge_count,
                evidence_count: evidence.len(),
            },
            domain_ids: domain_ids.into_iter().collect(),
            unit_ids: input
                .owner_units
                .iter()
                .map(|index| self.reachability.unit_id(*index).to_string())
                .collect(),
            reachable_unit_count: input.reachable_unit_count,
            entrypoint_ids: input
                .entrypoint
                .map(|value| vec![value.id.clone()])
                .unwrap_or_default(),
            flow_ids,
            resource_ids,
            dynamic_boundary_ids,
            evidence,
            summary: None,
        }
    }
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

fn flows_by_owner(flows: &ExecutionFlowGraph) -> HashMap<String, Vec<&ExecutionFlow>> {
    let mut result = HashMap::new();
    for flow in &flows.flows {
        result
            .entry(flow.owner_unit_id.clone())
            .or_insert_with(Vec::new)
            .push(flow);
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
        features.push(context.build_feature(FeatureBuildInput {
            kind: FeatureKind::Operation,
            base_status: FeatureStatus::Candidate,
            visibility: FeatureVisibility::Internal,
            entrypoint: None,
            operation_root_id: Some(operation_root.as_str()),
            owner_units: &owner_units,
            scope_units: &scope_units,
            reachable_unit_count: scope_units.len(),
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

fn assign_orphan_flows_to_nearest_endpoint(
    features: &mut [FeatureGroup],
    flows: &ExecutionFlowGraph,
    facts: &FactStore,
    reachability: &ReachabilityIndex,
) {
    if features.is_empty() {
        return;
    }
    let mut assigned_flow_ids: HashSet<String> = features
        .iter()
        .flat_map(|feature| feature.flow_ids.iter().cloned())
        .collect();
    for flow in &flows.flows {
        if assigned_flow_ids.contains(&flow.id) {
            continue;
        }
        let Some(target) = nearest_endpoint_feature_index(flow, features, facts, reachability) else {
            continue;
        };
        features[target].flow_ids.push(flow.id.clone());
        assigned_flow_ids.insert(flow.id.clone());
    }
    for feature in features {
        feature.flow_ids.sort();
        feature.flow_ids.dedup();
    }
}

fn nearest_endpoint_feature_index(
    flow: &ExecutionFlow,
    features: &[FeatureGroup],
    facts: &FactStore,
    _reachability: &ReachabilityIndex,
) -> Option<usize> {
    let flow_owner = facts.unit(&flow.owner_unit_id)?;
    let flow_file_id = &flow_owner.file_id;
    for (index, feature) in features.iter().enumerate() {
        if !matches!(feature.kind, FeatureKind::Endpoint) {
            continue;
        }
        let shares_file = feature.unit_ids.iter().any(|unit_id| {
            facts
                .unit(unit_id)
                .is_some_and(|unit| unit.file_id == *flow_file_id)
        });
        if shares_file {
            return Some(index);
        }
    }
    features
        .iter()
        .position(|feature| matches!(feature.kind, FeatureKind::Endpoint))
}

fn file_units_by_file(facts: &FactStore) -> HashMap<String, String> {
    facts
        .units
        .values()
        .filter(|unit| unit.kind == CodeUnitKind::File)
        .map(|unit| (unit.file_id.clone(), unit.id.clone()))
        .collect()
}

/// 실행 흐름을 가장 가까운 구조적 컨테이너에 귀속시킨다.
///
/// 메서드는 클래스, 자유 함수는 파일, 중첩 함수는 가장 가까운 파일 또는
/// 모듈에 묶는다. 부모 유닛이 누락된 불완전한 파싱 결과에서는 파일 유닛을
/// fallback으로 사용해 고아 operation을 만들지 않는다.
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

fn entrypoint_label(entrypoint: &Entrypoint) -> String {
    match (&entrypoint.method, &entrypoint.path) {
        (Some(method), Some(path)) => format!("{method} {path}"),
        (_, Some(path)) => path.clone(),
        _ => entrypoint.name.clone(),
    }
}

fn confidence_level(resolved: usize, unresolved: usize, dynamic: usize) -> String {
    if unresolved == 0 && dynamic == 0 {
        "high".into()
    } else if resolved > 0 {
        "medium".into()
    } else {
        "low".into()
    }
}
