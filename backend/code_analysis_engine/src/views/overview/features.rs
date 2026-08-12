//! 정적 Facts를 도메인과 실행 흐름 사이의 기능 그룹으로 변환한다.

use super::model::{FeatureConfidence, FeatureGroup, FeatureKind, FeatureStatus};
use super::reachability::ReachabilityIndex;
use crate::domain::DomainAnalysisOutput;
use crate::facts::{
    CodeUnitKind, Entrypoint, FactStore, Reference, ReferenceKind, ResolutionStatus, ResourceAccess,
};
use crate::flow::{ExecutionFlow, ExecutionFlowGraph};
use crate::languages::common::metadata::stable_id;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// entrypoint 중심으로 호출 그래프를 묶어 정적 기능 그룹을 만든다.
pub(super) fn build(
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
    let mut features = Vec::new();
    let mut covered_units = HashSet::new();

    for entrypoint in &facts.entrypoints {
        let units = reachability.reachable_from(&entrypoint.unit_id);
        covered_units.extend(units.iter().copied());
        features.push(context.build_feature(
            FeatureKind::Endpoint,
            FeatureStatus::Confirmed,
            Some(entrypoint),
            None,
            units,
        ));
    }

    // 진입점이 없는 CLI·배치·라이브러리 함수도 2단계에서 사라지지 않도록
    // 아직 어떤 기능에도 포함되지 않은 실행 흐름 단위를 operation으로 보존한다.
    //
    // 함수마다 operation을 만들면 대형 프로젝트에서 기능 수가 함수 수와
    // 거의 같아져 Overview가 불필요하게 폭발한다. 가장 가까운 비실행 구조
    // 유닛(클래스·모듈·파일)을 operation의 경계로 삼고, 그 경계 안의 모든
    // 실행 흐름과 호출 유닛은 하나의 후보 기능에 합친다. 상세 함수와 CFG는
    // 각각 `units`와 `executionFlows`에 그대로 남기므로 정보 손실은 없다.
    let file_units = file_units_by_file(facts);
    let mut operation_owners: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for flow in &flows.flows {
        let Some(owner_index) = reachability.unit_index(&flow.owner_unit_id) else {
            continue;
        };
        if covered_units.contains(&owner_index) {
            continue;
        }
        let operation_root = operation_root(&flow.owner_unit_id, facts, &file_units);
        operation_owners
            .entry(operation_root)
            .or_default()
            .insert(flow.owner_unit_id.clone());
    }
    for (operation_root, owners) in operation_owners {
        let mut units = reachability.reachable_from_many(owners.iter().map(String::as_str));
        if let Some(root_index) = reachability.unit_index(&operation_root) {
            if units.binary_search(&root_index).is_err() {
                units.push(root_index);
                units.sort_unstable();
            }
            covered_units.extend(units.iter().copied());
        }
        features.push(context.build_feature(
            FeatureKind::Operation,
            FeatureStatus::Candidate,
            None,
            Some(operation_root.as_str()),
            &units,
        ));
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

impl FeatureBuildContext<'_> {
    fn build_feature(
        &self,
        kind: FeatureKind,
        base_status: FeatureStatus,
        entrypoint: Option<&Entrypoint>,
        operation_root_id: Option<&str>,
        units: &[usize],
    ) -> FeatureGroup {
        let root_unit_id = operation_root_id
            .or_else(|| {
                entrypoint
                    .map(|value| value.unit_id.as_str())
                    .or_else(|| units.first().map(|index| self.reachability.unit_id(*index)))
            })
            .unwrap_or_default();
        let key = entrypoint
            .map(|value| format!("entrypoint:{}", value.id))
            .unwrap_or_else(|| format!("operation:{}", root_unit_id));
        let id = stable_id("feature", &key);
        let root_unit = self.facts.unit(root_unit_id);

        let mut domain_ids = BTreeSet::new();
        for &unit_index in units {
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
        if let Some(entrypoint) = entrypoint {
            evidence.extend(entrypoint.evidence.clone());
        }

        for &unit_index in units {
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

        let flow_ids = units
            .iter()
            .filter_map(|index| self.flows_by_owner.get(self.reachability.unit_id(*index)))
            .flatten()
            .map(|flow| flow.id.clone())
            .collect::<Vec<_>>();
        let resource_ids = units
            .iter()
            .filter_map(|index| {
                self.resources_by_unit
                    .get(self.reachability.unit_id(*index))
            })
            .flatten()
            .map(|resource| resource.id.clone())
            .collect::<Vec<_>>();
        let status = if dynamic_edge_count > 0 || unresolved_edge_count > 0 {
            if matches!(base_status, FeatureStatus::Confirmed) {
                FeatureStatus::Candidate
            } else {
                base_status
            }
        } else {
            base_status
        };

        let label = entrypoint
            .map(entrypoint_label)
            .or_else(|| root_unit.map(|unit| unit.qualified_name.clone()))
            .unwrap_or_else(|| key.clone());

        FeatureGroup {
            id,
            key,
            label,
            kind,
            status,
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
            unit_ids: units
                .iter()
                .map(|index| self.reachability.unit_id(*index).to_string())
                .collect(),
            entrypoint_ids: entrypoint
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
