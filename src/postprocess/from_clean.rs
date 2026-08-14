//! 정적 분석 결과를 Clean bundle의 PreparedStaticOverview로 변환한다.

use crate::clean::{
    LoadedCleanBundle, PreparedDomain, PreparedEntrypoint, PreparedFeature, PreparedResource,
    PreparedStaticOverview, PreparedUnit,
};
use crate::domain::confidence::DomainConfidence;
use crate::domain::DomainGroup;
use crate::facts::{CodeUnit, CodeUnitVisibility, Entrypoint, Reference, ResourceAccess, SourceSpan};
use crate::frameworks::registry::capabilities::FrameworkKind;
use crate::frameworks::registry::detector::FrameworkDetection;
use crate::graph::StaticRelationGraph;
use crate::model::{AnalysisResult, ProjectSummary};
use crate::views::overview::model::FeatureConfidence;
use crate::views::overview::{FeatureGroup, OverviewResponse};
use std::collections::BTreeSet;

pub(crate) fn to_analysis_result(bundle: &LoadedCleanBundle) -> AnalysisResult {
    let overview = to_overview(&bundle.prepared);
    AnalysisResult {
        schema_version: bundle.manifest.source.source_schema_version.clone(),
        analysis_id: bundle.manifest.source.analysis_id.clone(),
        status: bundle.status.clone(),
        project: bundle.project.clone(),
        files: Vec::new(),
        summary: ProjectSummary::default(),
        diagnostics: Vec::new(),
        elapsed_ms: 0,
        overview: Some(overview),
    }
}

fn to_overview(prepared: &PreparedStaticOverview) -> OverviewResponse {
    OverviewResponse {
        schema_version: "domain-overview.v4".into(),
        domains: prepared.domains.iter().cloned().map(to_domain).collect(),
        features: prepared.features.iter().cloned().map(to_feature).collect(),
        relations: prepared
            .relations
            .iter()
            .cloned()
            .map(|relation| crate::domain::DomainRelation {
                source_domain_id: relation.source_domain_id,
                target_domain_id: relation.target_domain_id,
                kind: relation.kind,
                status: relation.status,
                weight: relation.weight,
                evidence: relation.evidence,
            })
            .collect(),
        static_graph: rebuild_static_graph(prepared),
        execution_flows: prepared.execution_flows.clone(),
        units: prepared.units.iter().cloned().map(to_unit).collect(),
        entrypoints: prepared.entrypoints.iter().cloned().map(to_entrypoint).collect(),
        resources: prepared.resources.iter().cloned().map(to_resource).collect(),
        unassigned_unit_ids: prepared.unassigned_unit_ids.clone(),
        dynamic_boundary_ids: prepared
            .dynamic_boundaries
            .iter()
            .map(|boundary| boundary.id.clone())
            .collect(),
        dynamic_boundaries: prepared.dynamic_boundaries.clone(),
        detected_frameworks: restore_frameworks(&prepared.frameworks),
        semantic_status: Default::default(),
        semantic_analysis: Default::default(),
        coverage: prepared.coverage.clone(),
    }
}

fn rebuild_static_graph(prepared: &PreparedStaticOverview) -> StaticRelationGraph {
    let node_ids = prepared.units.iter().map(|unit| unit.id.clone()).collect();
    let edges = prepared
        .references
        .iter()
        .map(|reference| Reference {
            id: reference.id.clone(),
            source_unit_id: reference.source_unit_id.clone(),
            target_unit_id: reference.target_unit_id.clone(),
            candidate_unit_ids: Vec::new(),
            target_name: String::new(),
            kind: reference.kind.clone(),
            status: reference.status.clone(),
            evidence: Vec::new(),
        })
        .collect();
    StaticRelationGraph {
        node_ids,
        edges,
        dynamic_edge_ids: prepared
            .dynamic_boundaries
            .iter()
            .map(|boundary| boundary.id.clone())
            .collect(),
        unresolved_edge_ids: Vec::new(),
    }
}

fn restore_frameworks(framework_ids: &[String]) -> Vec<FrameworkDetection> {
    framework_ids
        .iter()
        .map(|id| FrameworkDetection {
            id: id.clone(),
            display_name: id.clone(),
            kind: FrameworkKind::Library,
            capabilities: Vec::new(),
            parent: None,
            languages: Vec::new(),
            confidence: 1.0,
            evidence: Vec::new(),
        })
        .collect()
}

fn to_domain(domain: PreparedDomain) -> DomainGroup {
    DomainGroup {
        id: domain.id,
        key: domain.candidate_key,
        label: domain.label,
        kind: domain.kind,
        status: domain.status,
        confidence: DomainConfidence {
            level: domain.confidence_level,
            score: domain.confidence_score,
            signal_families: BTreeSet::new(),
            cohesion: 0.0,
            separation: 0.0,
            evidence_diversity: 0.0,
            overall: 0.0,
        },
        primary_unit_ids: domain.unit_ids,
        shared_unit_ids: Vec::new(),
        entrypoint_ids: domain.entrypoint_ids,
        feature_ids: domain.feature_ids,
        resource_ids: domain.resource_ids,
        evidence: domain.evidence,
        summary: None,
    }
}

fn to_feature(feature: PreparedFeature) -> FeatureGroup {
    FeatureGroup {
        id: feature.id,
        key: feature.candidate_key,
        label: feature.label,
        kind: feature.kind,
        status: feature.status,
        visibility: feature.visibility,
        confidence: FeatureConfidence {
            level: String::new(),
            resolved_edge_count: 0,
            unresolved_edge_count: 0,
            dynamic_edge_count: 0,
            evidence_count: 0,
        },
        domain_ids: feature.domain_ids,
        unit_ids: feature.unit_ids,
        reachable_unit_count: feature.reachable_unit_count,
        entrypoint_ids: feature.entrypoint_ids,
        flow_ids: feature.flow_ids,
        resource_ids: feature.resource_ids,
        dynamic_boundary_ids: feature.dynamic_boundary_ids,
        evidence: feature.evidence,
        summary: None,
    }
}

fn to_unit(unit: PreparedUnit) -> CodeUnit {
    let file_id = path_to_file_id(&unit.path);
    let path = unit.path.clone();
    CodeUnit {
        id: unit.id,
        kind: unit.kind,
        name: unit.name,
        qualified_name: unit.qualified_name,
        file_id: file_id.clone(),
        relative_path: path.clone(),
        language: unit.language,
        parent_id: unit.parent_id,
        span: SourceSpan::new(
            file_id, path, unit.start_line, unit.start_column, unit.end_line, unit.end_column,
        ),
        body_span: None,
        signature: unit.signature,
        parameters: Vec::new(),
        return_type: None,
        visibility: CodeUnitVisibility::default(),
        modifiers: Vec::new(),
        exported: false,
    }
}

fn to_entrypoint(entrypoint: PreparedEntrypoint) -> Entrypoint {
    Entrypoint {
        id: entrypoint.id,
        unit_id: entrypoint.unit_id,
        kind: entrypoint.kind,
        name: entrypoint.name,
        method: entrypoint.method,
        path: entrypoint.path,
        framework_id: entrypoint.framework_id,
        evidence: entrypoint.evidence,
    }
}

fn to_resource(resource: PreparedResource) -> ResourceAccess {
    ResourceAccess {
        id: resource.id,
        unit_id: resource.unit_id,
        kind: resource.kind,
        name: resource.name,
        mode: resource.mode,
        evidence: resource.evidence,
    }
}

fn path_to_file_id(path: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    format!("file_{:024x}", hasher.finish())
}
