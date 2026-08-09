use super::{broker, emit_progress, SemanticProgress};
use crate::provider::ResolvedProvider;
use codebase_semantic_compiler::{
    compile_global_reconciliation_prompt, compile_global_reconciliation_repair_prompt,
    parse_and_verify_global_reconciliation, CompiledBasePrompt, SemanticCompileError,
    VerifiedSemanticPartition,
};

const MAX_GLOBAL_REPAIR_ATTEMPTS: usize = 2;
const MAX_EXECUTION_RETRIES_PER_PROMPT: usize = 1;

pub(super) fn reconcile_globally(
    runtime: &ResolvedProvider,
    base: &CompiledBasePrompt,
    partitions: &[VerifiedSemanticPartition],
    operation_id: &str,
    progress: SemanticProgress<'_>,
    completed_map_jobs: u64,
    total_progress_steps: u64,
) -> Result<codebase_semantic_model::ApprovedSemanticRevision, String> {
    let compiled = compile_global_reconciliation_prompt(base, partitions)
        .map_err(|error| format!("AI 의미 전역 통합 입력을 만들지 못했습니다: {error}"))?;
    eprintln!(
        "@codebase-workspace-ai-global-reconciliation {}",
        serde_json::json!({
            "partitions": partitions.len(),
            "inputBytes": compiled.prompt.rendered_prompt().len(),
            "mode": "compact-short-aliases",
        })
    );
    emit_progress(
        progress,
        "검증된 지역 의미를 하나의 전체 지도로 통합하는 중",
        completed_map_jobs,
        total_progress_steps,
    );

    let mut prompt = compiled.prompt.clone();
    for repair_attempt in 0..=MAX_GLOBAL_REPAIR_ATTEMPTS {
        let raw = run_with_execution_retry(runtime, &prompt, operation_id)
            .map_err(|error| format!("AI 의미 전역 통합 실행에 실패했습니다: {error}"))?;
        match parse_and_verify_global_reconciliation(&compiled, &raw) {
            Ok(revision) => {
                emit_progress(
                    progress,
                    "전체 의미 지도 검증 완료",
                    total_progress_steps,
                    total_progress_steps,
                );
                return Ok(revision);
            }
            Err(error) if repair_attempt < MAX_GLOBAL_REPAIR_ATTEMPTS => {
                emit_progress(
                    progress,
                    &format!(
                        "전체 의미 지도 {}차 결과를 검증 오류 기준으로 교정하는 중",
                        repair_attempt + 1
                    ),
                    completed_map_jobs,
                    total_progress_steps,
                );
                prompt = compile_global_reconciliation_repair_prompt(&compiled, &raw, &error)
                    .map_err(|compile_error| {
                        format!("AI 의미 전역 통합 교정 입력을 만들지 못했습니다: {compile_error}")
                    })?;
            }
            Err(error) => return Err(global_validation_error(error)),
        }
    }
    Err("AI 의미 전역 통합이 검증 결과를 만들지 못했습니다".to_string())
}

fn run_with_execution_retry(
    runtime: &ResolvedProvider,
    prompt: &CompiledBasePrompt,
    operation_id: &str,
) -> Result<String, String> {
    let mut last_error = None;
    for _ in 0..=MAX_EXECUTION_RETRIES_PER_PROMPT {
        match broker::run_provider(runtime, prompt, operation_id) {
            Ok(raw) => return Ok(raw),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| "AI 공급자 결과가 없습니다".to_string()))
}

fn global_validation_error(error: SemanticCompileError) -> String {
    format!(
        "AI 의미 전역 통합 결과가 {}회 교정 후에도 검증되지 않았습니다: {error}",
        MAX_GLOBAL_REPAIR_ATTEMPTS
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_repair_and_execution_retry_are_bounded() {
        assert_eq!(MAX_GLOBAL_REPAIR_ATTEMPTS, 2);
        assert_eq!(MAX_EXECUTION_RETRIES_PER_PROMPT, 1);
    }
}
