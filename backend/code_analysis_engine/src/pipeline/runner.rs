use crate::diagnostics::{Diagnostic, DiagnosticSeverity};
use crate::domain::{reaggregate_relations, DomainAnalyzer};
use crate::facts::FactStore;
use crate::flow::build as build_execution_flows;
use crate::frameworks::adapters::enrich as enrich_framework_facts;
use crate::frameworks::registry::detector::detect as detect_frameworks;
use crate::graph::build as build_static_graph;
use crate::languages::analyze_file;
use crate::model::{AnalysisRequest, AnalysisResult, AnalysisStatus};
use crate::project::ProjectScanner;
use crate::views::overview::projector::project;
use crate::EngineError;
use rayon::prelude::*;
use std::fs;
use std::path::Path;
use std::time::Instant;

use super::semantic_stage;
use super::{cache, profile, DomainAnalysisPipeline};
use std::sync::Arc;

impl DomainAnalysisPipeline {
    /// 파일 인벤토리부터 1단계 Overview까지 실행한다.
    pub fn run(&self, request: AnalysisRequest) -> Result<AnalysisResult, EngineError> {
        let started = Instant::now();
        let mut profiler = profile::PipelineProfiler::new(request.options.profile);

        let stage_started = Instant::now();
        let scan = ProjectScanner::new(request.options.clone()).scan(&request.root_path)?;
        profiler.record(
            "project_scan",
            stage_started,
            format!("files={}", scan.files.len()),
        );
        let root = Path::new(&scan.context.root_path);
        let mut facts = FactStore::default();
        let mut diagnostics = scan.diagnostics.clone();
        let config_fingerprint = cache::config_fingerprint(&request.options.config);
        let fact_cache = Arc::clone(&self.fact_cache);
        fact_cache
            .lock()
            .expect("Facts 캐시가 poisoned되지 않아야 한다")
            .ensure_capacity(request.options.config.scan.fact_cache_max_entries);

        let stage_started = Instant::now();
        let analyzed_files: Vec<_> = scan
            .files
            .par_iter()
            .map(|file| {
                let path = root.join(&file.relative_path);
                let cache_key = cache::FactCacheKey::new(
                    scan.context.project_id.as_str(),
                    file.file_id.as_str(),
                    file.language.key(),
                    file.content_hash.as_deref(),
                    &config_fingerprint,
                );
                if let Some(key) = cache_key.as_ref() {
                    if let Some(bundle) = fact_cache
                        .lock()
                        .expect("Facts 캐시가 poisoned되지 않아야 한다")
                        .get(key)
                    {
                        return Ok((file.file_id.clone(), bundle, true));
                    }
                }
                match fs::read_to_string(&path) {
                    Ok(source) => {
                        let bundle = analyze_file(file, &source, &request.options.config);
                        if let Some(key) = cache_key {
                            fact_cache
                                .lock()
                                .expect("Facts 캐시가 poisoned되지 않아야 한다")
                                .insert(key, bundle.clone());
                        }
                        Ok((file.file_id.clone(), bundle, false))
                    }
                    Err(error) => Err(Diagnostic::warning(
                        "SOURCE_READ_FAILED",
                        format!("AST 분석용 소스 파일을 읽지 못했습니다: {error}"),
                        Path::new(&file.relative_path),
                    )),
                }
            })
            .collect();
        let mut files = scan.files.clone();
        let cache_hits = analyzed_files
            .iter()
            .filter_map(|result| result.as_ref().ok().map(|(_, _, hit)| *hit))
            .filter(|hit| *hit)
            .count();
        let file_indexes = files
            .iter()
            .enumerate()
            .map(|(index, file)| (file.file_id.clone(), index))
            .collect::<std::collections::HashMap<_, _>>();
        // rayon의 indexed parallel iterator는 collect 결과의 원래 순서를 보존한다.
        // 따라서 Facts 병합과 진단 출력은 기존 순서와 동일하게 유지된다.
        for analyzed_file in analyzed_files {
            match analyzed_file {
                Ok((file_id, bundle, _cache_hit)) => {
                    if let Some(index) = file_indexes.get(file_id.as_str()) {
                        files[*index].parse_status = bundle.parse_status.clone();
                    }
                    let merge_stats =
                        facts.merge_with_limits(bundle, &request.options.config.limits);
                    if merge_stats.truncated {
                        let relative_path = files
                            .get(*file_indexes.get(file_id.as_str()).unwrap_or(&0))
                            .map(|file| file.relative_path.clone())
                            .unwrap_or_else(|| file_id.clone());
                        diagnostics.push(Diagnostic::warning(
                            "ANALYSIS_LIMIT_REACHED",
                            "파일 Facts가 프로젝트 분석 한도에 도달해 일부 사실을 생략했습니다.",
                            Path::new(&relative_path),
                        ));
                    }
                }
                Err(diagnostic) => diagnostics.push(diagnostic),
            }
        }
        profiler.record(
            "language_analysis",
            stage_started,
            format!(
                "files={} units={} references={} entrypoints={} resources={} cache_hits={}",
                files.len(),
                facts.units.len(),
                facts.references.len(),
                facts.entrypoints.len(),
                facts.resources.len(),
                cache_hits
            ),
        );
        facts.repair_integrity();

        let stage_started = Instant::now();
        facts.resolve_references();
        diagnostics.extend(facts.diagnostics.clone());
        profiler.record(
            "reference_resolution",
            stage_started,
            format!(
                "references={} confirmed={} candidate={} unknown={} dynamic={}",
                facts.references.len(),
                facts
                    .references
                    .iter()
                    .filter(
                        |reference| reference.status == crate::facts::ResolutionStatus::Confirmed
                    )
                    .count(),
                facts
                    .references
                    .iter()
                    .filter(
                        |reference| reference.status == crate::facts::ResolutionStatus::Candidate
                    )
                    .count(),
                facts
                    .references
                    .iter()
                    .filter(|reference| reference.status == crate::facts::ResolutionStatus::Unknown)
                    .count(),
                facts
                    .references
                    .iter()
                    .filter(|reference| reference.status == crate::facts::ResolutionStatus::Dynamic)
                    .count()
            ),
        );

        let stage_started = Instant::now();
        let framework_detections =
            detect_frameworks(root, &files, &request.options.config.frameworks);
        profiler.record(
            "framework_detection",
            stage_started,
            format!(
                "frameworks={} ids={}",
                framework_detections.len(),
                framework_detections
                    .iter()
                    .map(|framework| framework.id.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        );

        let stage_started = Instant::now();
        enrich_framework_facts(&mut facts, &framework_detections);
        profiler.record(
            "framework_fact_enrichment",
            stage_started,
            format!("framework_facts={}", facts.frameworks.len()),
        );

        let stage_started = Instant::now();
        let static_graph = build_static_graph(&facts);
        profiler.record(
            "static_relation_graph",
            stage_started,
            format!(
                "nodes={} edges={} dynamic_edges={} unresolved_edges={}",
                static_graph.node_ids.len(),
                static_graph.edges.len(),
                static_graph.dynamic_edge_ids.len(),
                static_graph.unresolved_edge_ids.len()
            ),
        );

        let stage_started = Instant::now();
        let (execution_flows, flow_limited) =
            build_execution_flows(&facts, &request.options.config.limits);
        if flow_limited {
            diagnostics.push(Diagnostic::warning(
                "ANALYSIS_LIMIT_REACHED",
                "실행 흐름 그래프가 프로젝트 분석 한도에 도달해 일부 흐름을 생략했습니다.",
                Path::new("."),
            ));
        }
        profiler.record(
            "execution_flow_graph",
            stage_started,
            format!("flows={}", execution_flows.flows.len()),
        );

        let stage_started = Instant::now();
        let domain_analyzer = DomainAnalyzer::new(
            request.options.config.domains.clone(),
            request.options.config.paths.clone(),
        );
        let mut domain_analysis = domain_analyzer.analyze_with_graph(&facts, &static_graph);
        profiler.record(
            "domain_grouping",
            stage_started,
            format!(
                "domains={} memberships={} relations={} signals={}",
                domain_analysis.groups.len(),
                domain_analysis.memberships.len(),
                domain_analysis.relations.len(),
                domain_analysis.signal_count
            ),
        );

        let (semantic_status, semantic_analysis) = semantic_stage::run(
            &request.options,
            root,
            started,
            &mut domain_analysis,
            &framework_detections,
            &mut diagnostics,
            &mut profiler,
        )?;

        // 관계 재집계는 AI 이름 보정과 무관한 정적 단계다. Codex를 끈
        // 실행에서도 프론트가 사용할 도메인 관계를 항상 만든다.
        let stage_started = Instant::now();
        reaggregate_relations(&facts, &mut domain_analysis);
        profiler.record(
            "domain_relation_aggregation",
            stage_started,
            format!("relations={}", domain_analysis.relations.len()),
        );

        let stage_started = Instant::now();
        let overview = project(
            &domain_analysis,
            &facts,
            &files,
            framework_detections,
            execution_flows,
            semantic_status,
            semantic_analysis,
        );
        profiler.record(
            "overview_projection",
            stage_started,
            format!(
                "domains={} features={} flows={} units={} entrypoints={} resources={}",
                overview.domains.len(),
                overview.features.len(),
                overview.execution_flows.flows.len(),
                overview.units.len(),
                overview.entrypoints.len(),
                overview.resources.len()
            ),
        );
        profiler.finish();
        let status = status_from_diagnostics(&diagnostics);

        Ok(AnalysisResult {
            schema_version: "analysis-result.v1".into(),
            analysis_id: scan.analysis_id,
            status,
            project: scan.context,
            files,
            summary: scan.summary,
            diagnostics,
            elapsed_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
            overview: Some(overview),
        })
    }
}

fn status_from_diagnostics(diagnostics: &[Diagnostic]) -> AnalysisStatus {
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    {
        AnalysisStatus::Failed
    } else if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Warning)
    {
        AnalysisStatus::Partial
    } else {
        AnalysisStatus::Ready
    }
}
