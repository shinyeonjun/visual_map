use super::model::PreparedFeature;
use crate::facts::CodeUnit;
use crate::flow::ExecutionFlowGraph;
use crate::views::overview::{AnalysisCoverage, FeatureGroup, OverviewResponse};
use std::collections::{BTreeSet, HashMap, HashSet};

/// Clean 변환 중 여러 dataset이 공유하는 가시성 인덱스다.
///
/// 각 builder가 raw 배열을 다시 순회하지 않도록 한 번 만든 ID 집합을
/// 전달한다. 이 구조체는 저장 계약이 아니라 변환 단계 전용이다.
pub(super) struct PreparedVisibility<'a> {
    pub(super) domain_ids: &'a HashSet<String>,
    pub(super) unit_ids: &'a HashSet<String>,
    pub(super) flow_ids: &'a HashSet<String>,
    pub(super) entrypoint_ids: &'a HashSet<String>,
    pub(super) resource_ids: &'a HashSet<String>,
    pub(super) dynamic_ids: &'a HashSet<String>,
    pub(super) units: &'a HashMap<&'a str, &'a CodeUnit>,
}

pub(super) fn prepare_feature(
    feature: &FeatureGroup,
    visibility: &PreparedVisibility<'_>,
) -> PreparedFeature {
    let unit_ids = visible_ids(feature.unit_ids.iter(), visibility.unit_ids);
    PreparedFeature {
        id: feature.id.clone(),
        candidate_key: feature.key.clone(),
        label: feature.label.clone(),
        kind: feature.kind.clone(),
        status: feature.status.clone(),
        visibility: feature.visibility.clone(),
        domain_ids: visible_ids(feature.domain_ids.iter(), visibility.domain_ids),
        unit_ids: unit_ids.clone(),
        reachable_unit_count: feature.reachable_unit_count,
        entrypoint_ids: visible_ids(feature.entrypoint_ids.iter(), visibility.entrypoint_ids),
        flow_ids: visible_ids(feature.flow_ids.iter(), visibility.flow_ids),
        resource_ids: visible_ids(feature.resource_ids.iter(), visibility.resource_ids),
        dynamic_boundary_ids: visible_ids(
            feature.dynamic_boundary_ids.iter(),
            visibility.dynamic_ids,
        ),
        symbols: symbols_for_units(&unit_ids, visibility.units),
        source_paths: paths_for_units(&unit_ids, visibility.units),
        evidence: feature.evidence.clone(),
    }
}

pub(super) fn prepared_flows(
    overview: &OverviewResponse,
    visible_flow_ids: &HashSet<String>,
    visible_dynamic_ids: &HashSet<String>,
) -> ExecutionFlowGraph {
    let mut flows = overview
        .execution_flows
        .flows
        .iter()
        .filter(|flow| visible_flow_ids.contains(&flow.id))
        .cloned()
        .collect::<Vec<_>>();
    for flow in &mut flows {
        flow.dynamic_boundary_ids
            .retain(|id| visible_dynamic_ids.contains(id));
    }
    flows.sort_by(|left, right| left.id.cmp(&right.id));
    let flow_ids = flows
        .iter()
        .map(|flow| flow.id.as_str())
        .collect::<HashSet<_>>();
    let links = overview
        .execution_flows
        .links
        .iter()
        .filter(|link| {
            flow_ids.contains(link.source_flow_id.as_str())
                && flow_ids.contains(link.target_flow_id.as_str())
        })
        .cloned()
        .collect();
    ExecutionFlowGraph { flows, links }
}

pub(super) fn prepared_coverage(
    overview: &OverviewResponse,
    unit_ids: &HashSet<String>,
    feature_ids: &HashSet<String>,
    flow_ids: &HashSet<String>,
    entrypoint_ids: &HashSet<String>,
    resource_ids: &HashSet<String>,
    dynamic_ids: &HashSet<String>,
) -> AnalysisCoverage {
    let mut coverage = overview.coverage.clone();
    coverage.total_units = unit_ids.len();
    coverage.total_features = feature_ids.len();
    coverage.total_execution_flows = flow_ids.len();
    coverage.total_entrypoints = entrypoint_ids.len();
    coverage.total_resources = resource_ids.len();
    coverage.total_dynamic_boundaries = dynamic_ids.len();
    coverage
}

pub(super) fn visible_ids<'a>(
    ids: impl Iterator<Item = &'a String>,
    visible: &HashSet<String>,
) -> Vec<String> {
    let mut values = ids
        .filter(|id| visible.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

pub(super) fn symbols_for_units(
    unit_ids: &[String],
    units: &HashMap<&str, &CodeUnit>,
) -> Vec<String> {
    let mut symbols = unit_ids
        .iter()
        .filter_map(|id| units.get(id.as_str()).map(|unit| unit.name.clone()))
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    symbols.sort();
    symbols.dedup();
    symbols.truncate(64);
    symbols
}

pub(super) fn paths_for_units(
    unit_ids: &[String],
    units: &HashMap<&str, &CodeUnit>,
) -> Vec<String> {
    let paths = unit_ids
        .iter()
        .filter_map(|id| {
            units
                .get(id.as_str())
                .map(|unit| unit.relative_path.clone())
        })
        .collect::<BTreeSet<_>>();
    paths.into_iter().collect()
}
