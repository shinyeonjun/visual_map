//! 프론트엔드·후속 의미 분석에 전달할 정적 분석 축약 결과.
//!
//! `OverviewResponse`는 감사·디버깅을 위해 전체 정적 그래프와 근거를
//! 보존한다. 반면 이 모델은 도메인 → 기능 → 실행 흐름 화면에 필요한
//! ID·이름 후보·소스 위치만 유지하고, 중복되는 raw graph와 테스트 코드를
//! 제거한다. 의미 있는 이름을 새로 만들지는 않는다.

use crate::facts::{CodeUnitKind, EntrypointKind, Evidence, ResourceKind};
use crate::flow::ExecutionFlowGraph;
use crate::model::{FileEntry, Language};
use crate::views::overview::model::DynamicBoundary;
use crate::views::overview::{AnalysisCoverage, FeatureGroup, OverviewResponse};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};

/// raw Overview에서 중복 그래프와 테스트 전용 사실을 걷어낸 정적 화면 계약이다.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PreparedStaticOverview {
    pub schema_version: String,
    pub domains: Vec<PreparedDomain>,
    pub features: Vec<PreparedFeature>,
    pub relations: Vec<PreparedRelation>,
    pub units: Vec<PreparedUnit>,
    pub entrypoints: Vec<PreparedEntrypoint>,
    pub resources: Vec<PreparedResource>,
    pub execution_flows: ExecutionFlowGraph,
    pub dynamic_boundaries: Vec<DynamicBoundary>,
    pub frameworks: Vec<String>,
    pub coverage: AnalysisCoverage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedDomain {
    pub id: String,
    pub candidate_key: String,
    pub label: String,
    pub kind: crate::domain::DomainKind,
    pub status: crate::domain::confidence::DomainStatus,
    pub confidence_level: String,
    pub confidence_score: u32,
    pub unit_ids: Vec<String>,
    pub feature_ids: Vec<String>,
    pub entrypoint_ids: Vec<String>,
    pub resource_ids: Vec<String>,
    pub symbols: Vec<String>,
    pub source_paths: Vec<String>,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedFeature {
    pub id: String,
    pub candidate_key: String,
    pub label: String,
    pub kind: crate::views::overview::FeatureKind,
    pub status: crate::views::overview::FeatureStatus,
    pub visibility: crate::views::overview::model::FeatureVisibility,
    pub domain_ids: Vec<String>,
    pub unit_ids: Vec<String>,
    pub reachable_unit_count: usize,
    pub entrypoint_ids: Vec<String>,
    pub flow_ids: Vec<String>,
    pub resource_ids: Vec<String>,
    pub dynamic_boundary_ids: Vec<String>,
    pub symbols: Vec<String>,
    pub source_paths: Vec<String>,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedRelation {
    pub source_domain_id: String,
    pub target_domain_id: String,
    pub kind: String,
    pub status: crate::facts::ResolutionStatus,
    pub weight: u32,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedUnit {
    pub id: String,
    pub kind: CodeUnitKind,
    pub name: String,
    pub qualified_name: String,
    pub language: Language,
    pub path: String,
    pub parent_id: Option<String>,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedEntrypoint {
    pub id: String,
    pub unit_id: String,
    pub kind: EntrypointKind,
    pub name: String,
    pub method: Option<String>,
    pub path: Option<String>,
    pub framework_id: Option<String>,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedResource {
    pub id: String,
    pub unit_id: String,
    pub kind: ResourceKind,
    pub name: String,
    pub mode: crate::facts::AccessMode,
    pub evidence: Vec<Evidence>,
}

struct PreparedVisibility<'a> {
    domain_ids: &'a HashSet<String>,
    unit_ids: &'a HashSet<String>,
    flow_ids: &'a HashSet<String>,
    entrypoint_ids: &'a HashSet<String>,
    resource_ids: &'a HashSet<String>,
    dynamic_ids: &'a HashSet<String>,
    units: &'a HashMap<&'a str, &'a crate::facts::CodeUnit>,
}

/// `files`의 `is_test` 정책을 사용해 화면·의미 분석 대상에서 테스트 코드를
/// 제거한다. raw 결과의 원본 사실은 변경하지 않는다.
pub fn prepare(overview: &OverviewResponse, files: &[FileEntry]) -> PreparedStaticOverview {
    let test_file_ids = files
        .iter()
        .filter(|file| file.is_test)
        .map(|file| file.file_id.as_str())
        .collect::<HashSet<_>>();
    let unit_by_id = overview
        .units
        .iter()
        .map(|unit| (unit.id.as_str(), unit))
        .collect::<HashMap<_, _>>();
    let visible_unit_ids = overview
        .units
        .iter()
        .filter(|unit| !test_file_ids.contains(unit.file_id.as_str()))
        .map(|unit| unit.id.clone())
        .collect::<HashSet<_>>();
    let visible_domain_ids = overview
        .domains
        .iter()
        .filter(|domain| {
            domain
                .primary_unit_ids
                .iter()
                .chain(domain.shared_unit_ids.iter())
                .any(|unit_id| visible_unit_ids.contains(unit_id))
        })
        .map(|domain| domain.id.clone())
        .collect::<HashSet<_>>();
    let visible_feature_ids = overview
        .features
        .iter()
        .filter(|feature| {
            feature
                .domain_ids
                .iter()
                .any(|domain_id| visible_domain_ids.contains(domain_id))
                && feature
                    .unit_ids
                    .iter()
                    .any(|unit_id| visible_unit_ids.contains(unit_id))
        })
        .map(|feature| feature.id.clone())
        .collect::<HashSet<_>>();
    let visible_flow_ids = overview
        .execution_flows
        .flows
        .iter()
        .filter(|flow| visible_unit_ids.contains(&flow.owner_unit_id))
        .map(|flow| flow.id.clone())
        .collect::<HashSet<_>>();
    let visible_entrypoint_ids = overview
        .entrypoints
        .iter()
        .filter(|entrypoint| visible_unit_ids.contains(&entrypoint.unit_id))
        .map(|entrypoint| entrypoint.id.clone())
        .collect::<HashSet<_>>();
    let visible_resource_ids = overview
        .resources
        .iter()
        .filter(|resource| visible_unit_ids.contains(&resource.unit_id))
        .map(|resource| resource.id.clone())
        .collect::<HashSet<_>>();
    let visible_dynamic_ids = overview
        .dynamic_boundaries
        .iter()
        .filter(|boundary| visible_unit_ids.contains(&boundary.source_unit_id))
        .map(|boundary| boundary.id.clone())
        .collect::<HashSet<_>>();

    let domains = overview
        .domains
        .iter()
        .filter(|domain| visible_domain_ids.contains(&domain.id))
        .map(|domain| {
            let unit_ids = visible_ids(
                domain
                    .primary_unit_ids
                    .iter()
                    .chain(domain.shared_unit_ids.iter()),
                &visible_unit_ids,
            );
            PreparedDomain {
                id: domain.id.clone(),
                candidate_key: domain.key.clone(),
                label: domain.label.clone(),
                kind: domain.kind,
                status: domain.status,
                confidence_level: domain.confidence.level.clone(),
                confidence_score: domain.confidence.score,
                unit_ids: unit_ids.clone(),
                feature_ids: visible_ids(domain.feature_ids.iter(), &visible_feature_ids),
                entrypoint_ids: visible_ids(domain.entrypoint_ids.iter(), &visible_entrypoint_ids),
                resource_ids: visible_ids(domain.resource_ids.iter(), &visible_resource_ids),
                symbols: symbols_for_units(&unit_ids, &unit_by_id),
                source_paths: paths_for_units(&unit_ids, &unit_by_id),
                evidence: domain.evidence.clone(),
            }
        })
        .collect::<Vec<_>>();

    let visibility = PreparedVisibility {
        domain_ids: &visible_domain_ids,
        unit_ids: &visible_unit_ids,
        flow_ids: &visible_flow_ids,
        entrypoint_ids: &visible_entrypoint_ids,
        resource_ids: &visible_resource_ids,
        dynamic_ids: &visible_dynamic_ids,
        units: &unit_by_id,
    };
    let features = overview
        .features
        .iter()
        .filter(|feature| visible_feature_ids.contains(&feature.id))
        .map(|feature| prepare_feature(feature, &visibility))
        .collect::<Vec<_>>();

    let execution_flows = prepared_flows(overview, &visible_flow_ids, &visible_dynamic_ids);
    let dynamic_boundaries = overview
        .dynamic_boundaries
        .iter()
        .filter(|boundary| visible_dynamic_ids.contains(&boundary.id))
        .cloned()
        .collect::<Vec<_>>();

    let units = overview
        .units
        .iter()
        .filter(|unit| visible_unit_ids.contains(&unit.id))
        .map(|unit| PreparedUnit {
            id: unit.id.clone(),
            kind: unit.kind.clone(),
            name: unit.name.clone(),
            qualified_name: unit.qualified_name.clone(),
            language: unit.language,
            path: unit.relative_path.clone(),
            parent_id: unit
                .parent_id
                .clone()
                .filter(|id| visible_unit_ids.contains(id)),
            start_line: unit.span.start_line,
            start_column: unit.span.start_column,
            end_line: unit.span.end_line,
            end_column: unit.span.end_column,
            signature: unit.signature.clone(),
        })
        .collect::<Vec<_>>();
    let entrypoints = overview
        .entrypoints
        .iter()
        .filter(|entrypoint| visible_entrypoint_ids.contains(&entrypoint.id))
        .map(|entrypoint| PreparedEntrypoint {
            id: entrypoint.id.clone(),
            unit_id: entrypoint.unit_id.clone(),
            kind: entrypoint.kind.clone(),
            name: entrypoint.name.clone(),
            method: entrypoint.method.clone(),
            path: entrypoint.path.clone(),
            framework_id: entrypoint.framework_id.clone(),
            evidence: entrypoint.evidence.clone(),
        })
        .collect::<Vec<_>>();
    let resources = overview
        .resources
        .iter()
        .filter(|resource| visible_resource_ids.contains(&resource.id))
        .map(|resource| PreparedResource {
            id: resource.id.clone(),
            unit_id: resource.unit_id.clone(),
            kind: resource.kind.clone(),
            name: resource.name.clone(),
            mode: resource.mode.clone(),
            evidence: resource.evidence.clone(),
        })
        .collect::<Vec<_>>();
    let relations = overview
        .relations
        .iter()
        .filter(|relation| {
            visible_domain_ids.contains(&relation.source_domain_id)
                && visible_domain_ids.contains(&relation.target_domain_id)
        })
        .map(|relation| PreparedRelation {
            source_domain_id: relation.source_domain_id.clone(),
            target_domain_id: relation.target_domain_id.clone(),
            kind: relation.kind.clone(),
            status: relation.status.clone(),
            weight: relation.weight,
            evidence: relation.evidence.clone(),
        })
        .collect::<Vec<_>>();

    let mut frameworks = overview
        .detected_frameworks
        .iter()
        .map(|framework| framework.id.clone())
        .collect::<Vec<_>>();
    frameworks.sort();
    frameworks.dedup();

    PreparedStaticOverview {
        schema_version: "prepared-static-overview.v1".into(),
        domains,
        features,
        relations,
        units,
        entrypoints,
        resources,
        execution_flows,
        dynamic_boundaries,
        frameworks,
        coverage: prepared_coverage(
            overview,
            &visible_unit_ids,
            &visible_feature_ids,
            &visible_flow_ids,
            &visible_entrypoint_ids,
            &visible_resource_ids,
            &visible_dynamic_ids,
        ),
    }
}

fn prepare_feature(feature: &FeatureGroup, visibility: &PreparedVisibility<'_>) -> PreparedFeature {
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

fn prepared_flows(
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

fn prepared_coverage(
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

fn visible_ids<'a>(
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

fn symbols_for_units(
    unit_ids: &[String],
    units: &HashMap<&str, &crate::facts::CodeUnit>,
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

fn paths_for_units(
    unit_ids: &[String],
    units: &HashMap<&str, &crate::facts::CodeUnit>,
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

#[cfg(test)]
mod tests {
    use super::prepare;
    use crate::model::FileEntry;
    use crate::views::overview::OverviewResponse;

    #[test]
    fn prepared_overview는_raw_graph를_복제하지_않고_결정적으로_직렬화된다() {
        let overview = OverviewResponse::default();
        let prepared = prepare(&overview, &Vec::<FileEntry>::new());
        let json = serde_json::to_string(&prepared).expect("prepared overview를 직렬화해야 한다");

        assert_eq!(prepared.schema_version, "prepared-static-overview.v1");
        assert!(!json.contains("staticGraph"));
        assert!(!json.contains("semanticAnalysis"));
    }
}
