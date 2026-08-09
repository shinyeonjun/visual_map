use super::broker;
use crate::provider::ResolvedProvider;
use codebase_semantic_compiler::{
    compile_base_repair_prompt, parse_and_verify_base_response, CompiledBasePrompt,
    SemanticCompileError,
};

pub(super) enum ProviderAttemptFailure {
    Execution(String),
    Rejected {
        raw: String,
        error: SemanticCompileError,
    },
}

impl ProviderAttemptFailure {
    pub(super) fn describe(&self, attempt: &str) -> String {
        match self {
            Self::Execution(error) => format!("{attempt} 실행 실패: {error}"),
            Self::Rejected { error, .. } => {
                format!("{attempt} 결과 검증 실패: {error}")
            }
        }
    }
}

#[derive(Clone, Copy)]
enum RecoveryKind {
    Repair,
    ExecutionRetry,
}

impl RecoveryKind {
    const fn attempt_label(self) -> &'static str {
        match self {
            Self::Repair => "AI 교정",
            Self::ExecutionRetry => "1회 재시도",
        }
    }
}

pub(super) struct RecoveryRequest {
    pub(super) partition_index: usize,
    pub(super) first_error: String,
    pub(super) prompt: CompiledBasePrompt,
    kind: RecoveryKind,
}

impl RecoveryRequest {
    pub(super) const fn attempt_label(&self) -> &'static str {
        self.kind.attempt_label()
    }
}

pub(super) fn verify_provider_result(
    prompt: &CompiledBasePrompt,
    run: Result<String, String>,
) -> Result<codebase_semantic_model::ApprovedSemanticRevision, ProviderAttemptFailure> {
    let raw = run.map_err(ProviderAttemptFailure::Execution)?;
    parse_and_verify_base_response(prompt, &raw)
        .map_err(|error| ProviderAttemptFailure::Rejected { raw, error })
}

pub(super) fn compile_recovery_prompt(
    partition_index: usize,
    original: &CompiledBasePrompt,
    failure: ProviderAttemptFailure,
) -> Result<RecoveryRequest, String> {
    let first_error = failure.describe("첫");
    let (prompt, kind) = match failure {
        ProviderAttemptFailure::Execution(_) => (original.clone(), RecoveryKind::ExecutionRetry),
        ProviderAttemptFailure::Rejected { raw, error } => (
            compile_base_repair_prompt(original, &raw, &error).map_err(|compile_error| {
                format!("{first_error}; AI 교정 입력 생성 실패: {compile_error}")
            })?,
            RecoveryKind::Repair,
        ),
    };
    Ok(RecoveryRequest {
        partition_index,
        first_error,
        prompt,
        kind,
    })
}

pub(super) fn run_provider_with_repair(
    runtime: &ResolvedProvider,
    original: &CompiledBasePrompt,
    operation_id: &str,
    context: &str,
) -> Result<codebase_semantic_model::ApprovedSemanticRevision, String> {
    let initial = broker::run_provider(runtime, original, operation_id);
    let failure = match verify_provider_result(original, initial) {
        Ok(revision) => return Ok(revision),
        Err(failure) => failure,
    };
    let (raw, error) = match failure {
        ProviderAttemptFailure::Execution(error) => {
            return Err(format!("{context} 첫 실행 실패: {error}"));
        }
        ProviderAttemptFailure::Rejected { raw, error } => (raw, error),
    };
    let repair = compile_base_repair_prompt(original, &raw, &error)
        .map_err(|compile_error| format!("{context} AI 교정 입력 생성 실패: {compile_error}"))?;
    match verify_provider_result(
        &repair,
        broker::run_provider(runtime, &repair, operation_id),
    ) {
        Ok(revision) => Ok(revision),
        Err(repair_failure) => Err(format!("{context} {}", repair_failure.describe("AI 교정"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codebase_fact_model::identity::{Sha256Digest, SnapshotId, WorkspaceId};
    use codebase_semantic_compiler::base_semantic_output_schema;
    use codebase_semantic_model::{
        AiProviderDescriptor, AiProviderKind, BaseSemanticInput, BaseSemanticPacket,
        OutputLanguage, ProjectSemanticContext, ReasoningEffort, ScopeReceipt, SemanticTask,
        BASE_SEMANTIC_SCHEMA_VERSION,
    };

    #[test]
    fn rejected_output_becomes_a_repair_but_execution_failure_retries_the_original() {
        let original = prompt_fixture();
        let rejected = verify_provider_result(&original, Ok("{}".to_string())).unwrap_err();
        let repair = compile_recovery_prompt(7, &original, rejected).unwrap();

        assert_eq!(repair.partition_index, 7);
        assert_eq!(repair.attempt_label(), "AI 교정");
        assert!(repair.prompt.task_prompt.contains("REPAIR_PAYLOAD_JSON"));
        assert_ne!(repair.prompt.task_prompt, original.task_prompt);

        let retry = compile_recovery_prompt(
            3,
            &original,
            ProviderAttemptFailure::Execution("provider unavailable".to_string()),
        )
        .unwrap();
        assert_eq!(retry.partition_index, 3);
        assert_eq!(retry.attempt_label(), "1회 재시도");
        assert_eq!(retry.prompt.rendered_prompt(), original.rendered_prompt());
    }

    fn prompt_fixture() -> CompiledBasePrompt {
        CompiledBasePrompt {
            packet: BaseSemanticPacket {
                schema_version: BASE_SEMANTIC_SCHEMA_VERSION,
                task: SemanticTask::Base,
                workspace_id: WorkspaceId::parse("ws-0123456789abcdef").unwrap(),
                snapshot_id: SnapshotId::from_components(&["repair-test"]).unwrap(),
                semantic_input_digest: Sha256Digest::of_bytes(b"repair-test"),
                provider: AiProviderDescriptor {
                    kind: AiProviderKind::Codex,
                    model: "gpt-5.6-sol".to_string(),
                    effort: ReasoningEffort::High,
                },
                output_language: OutputLanguage::Korean,
                packet_compiler_version: "test".to_string(),
                prompt_policy_version: "test".to_string(),
                scope_receipt: ScopeReceipt {
                    included: 0,
                    total: 0,
                    truncated: false,
                    reason: None,
                },
                input: BaseSemanticInput {
                    repository: ProjectSemanticContext {
                        fact_id: codebase_fact_model::identity::FactNodeId::from_components(&[
                            "repair-test",
                        ])
                        .unwrap(),
                        name: "fixture".to_string(),
                        languages: vec![],
                        framework_fact_ids: vec![],
                        root_region_ids: vec![],
                    },
                    regions: vec![],
                    anchors: vec![],
                    boundary_relations: vec![],
                    representative_traces: vec![],
                    excerpts: vec![],
                    previous_revision: None,
                },
            },
            system_policy: "test policy".to_string(),
            task_prompt: "test task".to_string(),
            output_schema: base_semantic_output_schema(),
        }
    }
}
