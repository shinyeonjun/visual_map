//! Codex 컨텍스트 생성과 의미 보정 단계를 담당한다.

use crate::diagnostics::Diagnostic;
use crate::domain::DomainAnalysisOutput;
use crate::frameworks::registry::detector::FrameworkDetection;
use crate::model::AnalysisOptions;
use crate::pipeline::profile::PipelineProfiler;
use crate::semantic::context::{SemanticContext, SemanticContextArtifact, SemanticContextTimings};
use crate::semantic::proposal::merge_proposals;
use crate::semantic::validator::{apply, validate};
use crate::semantic::{CodexProvider, SemanticStatus};
use crate::views::overview::model::SemanticAnalysisSummary;
use crate::EngineError;
use std::path::Path;
use std::time::Instant;

use super::dev_artifacts;

pub(crate) fn run(
    options: &AnalysisOptions,
    root: &Path,
    started: Instant,
    domain_analysis: &mut DomainAnalysisOutput,
    framework_detections: &[FrameworkDetection],
    diagnostics: &mut Vec<Diagnostic>,
    profiler: &mut PipelineProfiler,
) -> Result<(SemanticStatus, SemanticAnalysisSummary), EngineError> {
    let semantic_policy = &options.config.semantic;
    let max_input_bytes = semantic_policy
        .codex_max_input_bytes
        .max(semantic_policy.minimum_context_bytes + semantic_policy.prompt_reserve_bytes);
    let max_context_bytes = max_input_bytes
        .saturating_sub(semantic_policy.prompt_reserve_bytes)
        .max(semantic_policy.minimum_context_bytes);

    // [DEV ONLY] Codex를 호출하지 않고도 전처리 결과를 검사할 수 있게 하는
    // 개발용 경로다. 제품 완성 단계에서는 진단 전용 명령으로 분리한다.
    let should_build_context =
        semantic_policy.codex_enabled || options.codex_context_output.is_some();
    let (chunks, context_timings) = if should_build_context {
        SemanticContext::chunks_with_timings(
            domain_analysis,
            framework_detections,
            max_context_bytes,
            semantic_policy,
        )
    } else {
        (Vec::new(), SemanticContextTimings::default())
    };
    if should_build_context {
        profiler.record_context_millis(
            "codex_context_domain_compaction",
            context_timings.domain_compaction_ms,
            format!("domains={}", domain_analysis.groups.len()),
        );
        profiler.record_context_millis(
            "codex_context_chunk_sizing",
            context_timings.chunk_sizing_ms,
            format!("chunks={}", chunks.len()),
        );
        profiler.record_context_millis(
            "codex_context_chunk_materialization",
            context_timings.chunk_materialization_ms,
            format!("chunks={}", chunks.len()),
        );
    }

    // [DEV ONLY] Codex 전달 직전의 중간 JSON 덤프다. 최종 제품에서는
    // 운영 파이프라인과 분리된 진단 명령에서만 저장한다.
    if let Some(output_path) = options.codex_context_output.as_deref() {
        let artifact = SemanticContextArtifact {
            schema_version: "codex-context.v1",
            max_input_bytes,
            max_context_bytes,
            chunk_count: chunks.len(),
            chunks: chunks.clone(),
        };
        let serialization_started = Instant::now();
        let json = dev_artifacts::serialize_artifact(&artifact)?;
        profiler.record_context_millis(
            "codex_context_json_serialization",
            serialization_started.elapsed().as_millis() as u64,
            format!("bytes={}", json.len()),
        );
        let write_started = Instant::now();
        dev_artifacts::write_artifact(output_path, &json)?;
        profiler.record_context_millis(
            "codex_context_file_write",
            write_started.elapsed().as_millis() as u64,
            format!("bytes={}", json.len()),
        );
        profiler.context_ready(started);
    } else if should_build_context {
        profiler.context_ready(started);
    }

    let result = if !semantic_policy.codex_enabled {
        (
            SemanticStatus::Disabled,
            SemanticAnalysisSummary {
                provider: "none".into(),
                max_input_bytes: semantic_policy.codex_max_input_bytes,
                ..SemanticAnalysisSummary::default()
            },
        )
    } else {
        review_with_codex(
            semantic_policy,
            max_input_bytes,
            root,
            &chunks,
            domain_analysis,
            diagnostics,
        )
    };

    if semantic_policy.codex_enabled {
        profiler.excluded("codex_semantic_review");
    } else {
        profiler.skipped("codex_semantic_review");
    }
    Ok(result)
}

fn review_with_codex(
    semantic_policy: &crate::config::SemanticPolicy,
    max_input_bytes: usize,
    root: &Path,
    chunks: &[crate::semantic::context::SemanticChunk],
    domain_analysis: &mut DomainAnalysisOutput,
    diagnostics: &mut Vec<Diagnostic>,
) -> (SemanticStatus, SemanticAnalysisSummary) {
    let provider = CodexProvider {
        executable: semantic_policy.codex_executable.clone(),
        timeout_ms: semantic_policy.codex_timeout_ms,
        max_input_bytes,
        command_prefix: Vec::new(),
    };

    let mut proposals = Vec::new();
    let mut failed_chunks = 0usize;
    for chunk in chunks {
        match provider.review_chunk(&chunk.context, root, chunk.index, chunk.count) {
            Ok(proposal) => proposals.push(proposal),
            Err(error) => {
                failed_chunks += 1;
                diagnostics.push(Diagnostic::warning(
                    "CODEX_CHUNK_FAILED",
                    format!(
                        "Codex 청크 {}/{} 분석에 실패했습니다: {}",
                        chunk.index + 1,
                        chunk.count,
                        error
                    ),
                    root,
                ));
            }
        }
    }

    let completed_chunks = proposals.len();
    let status = if completed_chunks == 0 {
        diagnostics.push(Diagnostic::warning(
            "CODEX_REVIEW_FAILED",
            format!(
                "Codex 청크를 하나도 완료하지 못했습니다: 총 {}개",
                chunks.len()
            ),
            root,
        ));
        SemanticStatus::Failed
    } else {
        let validated = validate(merge_proposals(proposals), domain_analysis, semantic_policy);
        diagnostics.extend(apply(domain_analysis, validated));
        if failed_chunks == 0 {
            SemanticStatus::Completed
        } else {
            SemanticStatus::Partial
        }
    };

    (
        status,
        SemanticAnalysisSummary {
            provider: "codex".into(),
            chunk_count: chunks.len(),
            completed_chunks,
            failed_chunks,
            max_input_bytes,
        },
    )
}
