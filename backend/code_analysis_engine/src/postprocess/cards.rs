//! 도메인 계획과 청크 조립 모듈을 연결해 Codex context bundle을 만든다.

use super::context_chunk::{
    build_chunk, fit_context_budget, required_domain_size, serialized_bytes, ChunkBuildInput,
};
use super::context_metadata::build_shells;
use super::context_profile::{compact_global_summary, global_summary, project_profile};
use super::domains::build_plan;
use super::indexes::PostprocessIndexes;
use super::model::{
    CodexContextBundle, CodexContextManifest, ContextChunkDescriptor, ContextWarning,
    DomainCoverage,
};
use super::partition::partition_domains;
use super::PostprocessError;
use crate::config::AnalysisConfig;
use crate::model::AnalysisResult;
use std::collections::{HashMap, VecDeque};

pub(crate) fn build_bundle(
    result: &AnalysisResult,
    config: &AnalysisConfig,
) -> Result<CodexContextBundle, PostprocessError> {
    let overview = result
        .overview
        .as_ref()
        .ok_or(PostprocessError::MissingOverview)?;
    let indexes = PostprocessIndexes::build(overview, &result.files);
    let plan = build_plan(&indexes, &config.domains, &config.postprocess);
    let profile = project_profile(&indexes);
    let global_summary = compact_global_summary(
        global_summary(&indexes, &plan),
        config.postprocess.global_summary_reserve_bytes,
    );
    let shells = build_shells(&plan, &indexes, config);
    let required_sizes = shells
        .iter()
        .map(|domain| {
            (
                domain.domain_id.clone(),
                required_domain_size(domain, &plan, &indexes, config),
            )
        })
        .collect::<HashMap<_, _>>();
    let mut partitions = partition_domains(&shells, overview, &config.postprocess, &required_sizes);
    if partitions.is_empty() {
        partitions.push(Vec::new());
    }

    let mut pending_partitions = partitions.into_iter().collect::<VecDeque<_>>();
    let mut chunks = Vec::new();
    let mut coverage = Vec::new();
    let mut warnings = Vec::new();
    let mut chunk_index = 0;

    while let Some(partition) = pending_partitions.pop_front() {
        let chunk_id = format!("chunk-{chunk_index:04}");
        let mut context = build_chunk(ChunkBuildInput {
            chunk_id: &chunk_id,
            partition: &partition,
            shells: &shells,
            plan: &plan,
            indexes: &indexes,
            overview,
            result,
            config,
            profile: profile.clone(),
            global_summary: global_summary.clone(),
        });
        fit_context_budget(&mut context, config.postprocess.target_budget_bytes);

        if should_split_empty_content(&context, partition.len()) {
            let midpoint = partition.len() / 2;
            pending_partitions.push_front(partition[midpoint..].to_vec());
            pending_partitions.push_front(partition[..midpoint].to_vec());
            continue;
        }

        record_budget_warning(
            &mut warnings,
            &context,
            &partition,
            config.postprocess.target_budget_bytes,
        );
        coverage.extend(context.domains.iter().map(|domain| DomainCoverage {
            domain_id: domain.domain_id.clone(),
            representation: domain_representation(domain),
            chunk_id: Some(chunk_id.clone()),
        }));
        chunks.push(context);
        chunk_index += 1;
    }

    append_suppressed_coverage(&mut coverage, &plan.suppressed);
    append_alias_coverage(&mut coverage, &plan.aliases);
    coverage.sort_by(|left, right| left.domain_id.cmp(&right.domain_id));
    let descriptors = chunk_descriptors(&chunks, config);
    let manifest = CodexContextManifest {
        schema_version: "codex-context-manifest.v1",
        source_analysis_id: result.analysis_id.clone(),
        source_schema_version: result.schema_version.clone(),
        project_id: result.project.project_id.clone(),
        project_profile: profile,
        global_summary,
        chunks: descriptors,
        domain_coverage: coverage,
        warnings,
    };
    Ok(CodexContextBundle { manifest, chunks })
}

fn domain_representation(domain: &super::model::ContextDomain) -> String {
    if domain.omission.required_content_exceeds_budget
        || domain.omission.included_features < domain.omission.total_features
        || domain.omission.included_flows < domain.omission.total_flows
    {
        "partial".into()
    } else {
        "full".into()
    }
}

fn should_split_empty_content(
    context: &super::model::CodexSemanticContext,
    partition_len: usize,
) -> bool {
    context.summary.included_features == 0
        && context.summary.total_features > 0
        && partition_len > 1
}

fn record_budget_warning(
    warnings: &mut Vec<ContextWarning>,
    context: &super::model::CodexSemanticContext,
    partition: &[String],
    budget_bytes: usize,
) {
    if serialized_bytes(context) > budget_bytes {
        warnings.push(ContextWarning {
            code: "budget_unmet".into(),
            message: "도메인 shell만으로 Codex 예산을 초과해 degraded context를 생성했습니다."
                .into(),
            related_ids: partition.to_vec(),
        });
    }
}

fn append_suppressed_coverage(
    coverage: &mut Vec<DomainCoverage>,
    suppressed: &[super::model::SuppressedDomain],
) {
    coverage.extend(suppressed.iter().map(|domain| DomainCoverage {
        domain_id: domain.domain_id.clone(),
        representation: "suppressed".into(),
        chunk_id: None,
    }));
}

fn append_alias_coverage(
    coverage: &mut Vec<DomainCoverage>,
    aliases: &[super::model::DomainAlias],
) {
    for alias in aliases {
        let chunk_id = coverage
            .iter()
            .find(|entry| entry.domain_id == alias.to_domain_id)
            .and_then(|entry| entry.chunk_id.clone());
        coverage.push(DomainCoverage {
            domain_id: alias.from_domain_id.clone(),
            representation: "alias".into(),
            chunk_id,
        });
    }
}

fn chunk_descriptors(
    chunks: &[super::model::CodexSemanticContext],
    config: &AnalysisConfig,
) -> Vec<ContextChunkDescriptor> {
    chunks
        .iter()
        .enumerate()
        .map(|(index, chunk)| ContextChunkDescriptor {
            chunk_id: chunk.chunk_id.clone(),
            file_name: format!("partition-{index:04}.json"),
            domain_ids: chunk
                .domains
                .iter()
                .map(|domain| domain.domain_id.clone())
                .collect(),
            used_bytes: serialized_bytes(chunk),
            budget_bytes: config.postprocess.target_budget_bytes,
        })
        .collect()
}
