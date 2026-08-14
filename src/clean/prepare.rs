use super::model::{
    PreparedDomain, PreparedEntrypoint, PreparedRelation, PreparedResource, PreparedStaticOverview,
    PreparedUnit,
};
use super::prepare_support::{
    paths_for_units, prepare_feature, prepared_coverage, prepared_flows, symbols_for_units,
    visible_ids, PreparedVisibility,
};
use crate::model::FileEntry;
use crate::views::overview::OverviewResponse;
use std::collections::{HashMap, HashSet};

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
    let unassigned_unit_ids = visible_ids(overview.unassigned_unit_ids.iter(), &visible_unit_ids);

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
    let references = overview
        .static_graph
        .edges
        .iter()
        .filter(|reference| {
            visible_unit_ids.contains(&reference.source_unit_id)
                && reference
                    .target_unit_id
                    .as_ref()
                    .map(|unit_id| visible_unit_ids.contains(unit_id))
                    .unwrap_or(false)
        })
        .map(|reference| super::model::PreparedReference {
            id: reference.id.clone(),
            source_unit_id: reference.source_unit_id.clone(),
            target_unit_id: reference.target_unit_id.clone(),
            kind: reference.kind.clone(),
            status: reference.status.clone(),
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
        references,
        units,
        entrypoints,
        resources,
        execution_flows,
        dynamic_boundaries,
        frameworks,
        unassigned_unit_ids,
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
