use crate::config::PathPolicy;
use crate::domain::{DomainAnalysisOutput, DomainKind};
use crate::facts::{FactStore, ResolutionStatus};
use crate::flow::ExecutionFlowGraph;
use crate::frameworks::registry::detector::FrameworkDetection;
use crate::model::{FileEntry, ParseStatus};
use crate::semantic::SemanticStatus;
use crate::views::overview::model::{
    AnalysisCoverage, DynamicBoundary, LanguageCoverage, OverviewResponse, SemanticAnalysisSummary,
};
use std::collections::{BTreeMap, HashMap};

/// Feature-first 파이프라인에서 이미 만든 Feature를 받아 Overview를 구성한다.
pub fn project_with_features(
    analysis: &DomainAnalysisOutput,
    prebuilt_features: Vec<super::FeatureGroup>,
    facts: &FactStore,
    files: &[FileEntry],
    frameworks: Vec<FrameworkDetection>,
    execution_flows: ExecutionFlowGraph,
    semantic_status: SemanticStatus,
    semantic_analysis: SemanticAnalysisSummary,
) -> OverviewResponse {
    project_inner(
        analysis,
        Some(prebuilt_features),
        facts,
        files,
        frameworks,
        execution_flows,
        semantic_status,
        semantic_analysis,
    )
}

/// 내부 분석 결과를 프론트엔드 Overview 응답으로 변환한다.
pub fn project(
    analysis: &DomainAnalysisOutput,
    facts: &FactStore,
    files: &[FileEntry],
    frameworks: Vec<FrameworkDetection>,
    execution_flows: ExecutionFlowGraph,
    semantic_status: SemanticStatus,
    semantic_analysis: SemanticAnalysisSummary,
) -> OverviewResponse {
    project_inner(
        analysis,
        None,
        facts,
        files,
        frameworks,
        execution_flows,
        semantic_status,
        semantic_analysis,
    )
}

fn project_inner(
    analysis: &DomainAnalysisOutput,
    prebuilt_features: Option<Vec<super::FeatureGroup>>,
    facts: &FactStore,
    files: &[FileEntry],
    frameworks: Vec<FrameworkDetection>,
    execution_flows: ExecutionFlowGraph,
    semantic_status: SemanticStatus,
    semantic_analysis: SemanticAnalysisSummary,
) -> OverviewResponse {
    let units_by_file = index_units_by_file(facts);
    let references_by_file = index_references_by_file(facts);
    let mut languages: BTreeMap<String, LanguageCoverage> = BTreeMap::new();
    for file in files {
        let key = file.language.key().to_string();
        let coverage = languages.entry(key).or_default();
        coverage.detected_files += 1;
        let extracted_units = units_by_file
            .get(file.file_id.as_str())
            .copied()
            .unwrap_or(0);
        match file.parse_status {
            ParseStatus::Parsed => coverage.parsed_files += 1,
            ParseStatus::ParsedWithErrors => coverage.partial_files += 1,
            ParseStatus::Failed | ParseStatus::NotAnalyzed => coverage.failed_files += 1,
        }
        coverage.extracted_units += extracted_units;
        coverage.extracted_references += references_by_file
            .get(file.file_id.as_str())
            .copied()
            .unwrap_or(0);
    }

    let features = prebuilt_features.unwrap_or_else(|| {
        super::features::build(analysis, facts, &execution_flows, &PathPolicy::default())
    });
    let confirmed_reference_count = facts
        .references
        .iter()
        .filter(|reference| reference.status == ResolutionStatus::Confirmed)
        .count();
    let candidate_reference_count = facts
        .references
        .iter()
        .filter(|reference| reference.status == ResolutionStatus::Candidate)
        .count();
    let unknown_reference_count = facts
        .references
        .iter()
        .filter(|reference| reference.status == ResolutionStatus::Unknown)
        .count();
    let dynamic_reference_count = facts
        .references
        .iter()
        .filter(|reference| reference.status == ResolutionStatus::Dynamic)
        .count();
    let unresolved_reference_count = facts
        .references
        .iter()
        .filter(|reference| reference.target_unit_id.is_none())
        .count();
    let coverage = AnalysisCoverage {
        total_files: files.len(),
        parsed_files: languages.values().map(|value| value.parsed_files).sum(),
        partial_files: languages.values().map(|value| value.partial_files).sum(),
        failed_files: languages.values().map(|value| value.failed_files).sum(),
        languages,
        total_units: facts.units.len(),
        total_references: facts.references.len(),
        total_entrypoints: facts.entrypoints.len(),
        total_resources: facts.resources.len(),
        total_features: features.len(),
        total_execution_flows: execution_flows.flows.len(),
        total_confirmed_references: confirmed_reference_count,
        total_candidate_references: candidate_reference_count,
        total_unknown_references: unknown_reference_count,
        total_dynamic_references: dynamic_reference_count,
        total_dynamic_boundaries: dynamic_reference_count,
        total_unresolved_references: unresolved_reference_count,
    };

    let dynamic_boundaries = facts
        .references
        .iter()
        .filter(|reference| reference.status == ResolutionStatus::Dynamic)
        .map(|reference| DynamicBoundary {
            id: reference.id.clone(),
            source_unit_id: reference.source_unit_id.clone(),
            target_name: reference.target_name.clone(),
            kind: reference.kind.clone(),
            evidence: reference.evidence.clone(),
        })
        .collect();

    let mut domains = analysis
        .groups
        .iter()
        .filter(|group| group.kind == DomainKind::Business)
        .cloned()
        .collect::<Vec<_>>();
    let domain_indexes = domains
        .iter()
        .enumerate()
        .map(|(index, domain)| (domain.id.clone(), index))
        .collect::<HashMap<_, _>>();
    for feature in &features {
        for domain_id in &feature.domain_ids {
            if let Some(&index) = domain_indexes.get(domain_id.as_str()) {
                let domain = &mut domains[index];
                domain.feature_ids.push(feature.id.clone());
            }
        }
    }
    for domain in &mut domains {
        domain.feature_ids.sort();
        domain.feature_ids.dedup();
    }

    OverviewResponse {
        schema_version: "domain-overview.v4".into(),
        domains,
        features,
        relations: analysis.relations.clone(),
        static_graph: analysis.static_graph.clone(),
        execution_flows,
        units: facts.units.values().cloned().collect(),
        entrypoints: facts.entrypoints.clone(),
        resources: facts.resources.clone(),
        unassigned_unit_ids: analysis.unassigned_unit_ids.clone(),
        dynamic_boundary_ids: analysis.dynamic_reference_ids.clone(),
        dynamic_boundaries,
        detected_frameworks: frameworks,
        semantic_status,
        semantic_analysis,
        coverage,
        formation_diagnostics: analysis.formation_diagnostics.clone(),
    }
}

/// 파일별 코드 단위 개수를 한 번 순회해 인덱싱한다.
fn index_units_by_file(facts: &FactStore) -> HashMap<&str, usize> {
    let mut index = HashMap::new();
    for unit in facts.units.values() {
        *index.entry(unit.file_id.as_str()).or_insert(0) += 1;
    }
    index
}

/// 참조의 출발 코드 단위를 이용해 파일별 참조 개수를 한 번 집계한다.
fn index_references_by_file(facts: &FactStore) -> HashMap<&str, usize> {
    let mut index = HashMap::new();
    for reference in &facts.references {
        let Some(unit) = facts.unit(&reference.source_unit_id) else {
            continue;
        };
        *index.entry(unit.file_id.as_str()).or_insert(0) += 1;
    }
    index
}
