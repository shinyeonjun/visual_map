//! 프로젝트 Facts와 실행 흐름 그래프를 구성한다.

use crate::diagnostics::Diagnostic;
use crate::facts::FactStore;
use crate::flow::{build as build_execution_flows, ExecutionFlowGraph};
use crate::frameworks::adapters::enrich as enrich_framework_facts;
use crate::frameworks::registry::detector::detect as detect_frameworks;
use crate::languages::analyze_file;
use crate::model::{AnalysisRequest, FileEntry};
use crate::project::ProjectScanner;
use crate::EngineError;
use rayon::prelude::*;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use super::cache;
use super::DomainAnalysisPipeline;

pub(crate) struct FactBundle {
    pub facts: FactStore,
    pub execution_flows: ExecutionFlowGraph,
    pub _files: Vec<FileEntry>,
    pub _diagnostics: Vec<Diagnostic>,
}

impl DomainAnalysisPipeline {
    pub(crate) fn build_fact_bundle(
        &self,
        request: &AnalysisRequest,
    ) -> Result<FactBundle, EngineError> {
        let scan = ProjectScanner::new(request.options.clone()).scan(&request.root_path)?;
        let root = Path::new(&scan.context.root_path);
        let mut facts = FactStore::default();
        let mut diagnostics = scan.diagnostics.clone();
        let use_fact_cache = request.options.use_fact_cache
            && request.options.config.scan.fact_cache_max_entries > 0;
        let config_fingerprint = if use_fact_cache {
            cache::config_fingerprint(&request.options.config)
        } else {
            String::new()
        };
        let fact_cache = Arc::clone(&self.fact_cache);
        if use_fact_cache {
            fact_cache.ensure_capacity(request.options.config.scan.fact_cache_max_entries);
        }

        let analyzed_files: Vec<_> = scan
            .files
            .par_iter()
            .map(|file| {
                let path = root.join(&file.relative_path);
                let cache_key = use_fact_cache.then(|| {
                    cache::FactCacheKey::new(
                        scan.context.project_id.as_str(),
                        file.file_id.as_str(),
                        file.language.key(),
                        file.content_hash.as_deref(),
                        &config_fingerprint,
                    )
                });
                if let Some(Some(key)) = cache_key.as_ref() {
                    if let Some(bundle) = fact_cache.get(key) {
                        return Ok((file.file_id.clone(), bundle, true));
                    }
                }
                match fs::read_to_string(&path) {
                    Ok(source) => {
                        let bundle = analyze_file(file, &source, &request.options.config);
                        if let Some(Some(key)) = cache_key {
                            fact_cache.insert(key, bundle.clone());
                        }
                        Ok((file.file_id.clone(), bundle, false))
                    }
                    Err(error) => Err(crate::diagnostics::Diagnostic::warning(
                        "SOURCE_READ_FAILED",
                        format!("AST 분석용 소스 파일을 읽지 못했습니다: {error}"),
                        Path::new(&file.relative_path),
                    )),
                }
            })
            .collect();
        let mut files = scan.files.clone();
        let file_indexes = files
            .iter()
            .enumerate()
            .map(|(index, file)| (file.file_id.clone(), index))
            .collect::<std::collections::HashMap<_, _>>();
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
                        diagnostics.push(crate::diagnostics::Diagnostic::warning(
                            "ANALYSIS_LIMIT_REACHED",
                            "파일 Facts가 프로젝트 분석 한도에 도달해 일부 사실을 생략했습니다.",
                            Path::new(&relative_path),
                        ));
                    }
                }
                Err(diagnostic) => diagnostics.push(diagnostic),
            }
        }
        facts.repair_integrity();
        facts.resolve_references();
        diagnostics.extend(facts.diagnostics.clone());

        let framework_detections =
            detect_frameworks(root, &files, &request.options.config.frameworks);
        enrich_framework_facts(&mut facts, &framework_detections);
        let (execution_flows, flow_limited) =
            build_execution_flows(&facts, &request.options.config.limits);
        if flow_limited {
            diagnostics.push(crate::diagnostics::Diagnostic::warning(
                "ANALYSIS_LIMIT_REACHED",
                "실행 흐름 그래프가 프로젝트 분석 한도에 도달해 일부 흐름을 생략했습니다.",
                Path::new("."),
            ));
        }

        Ok(FactBundle {
            facts,
            execution_flows,
            _files: files,
            _diagnostics: diagnostics,
        })
    }
}
