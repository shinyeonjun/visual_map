use super::broker;
use crate::provider::ResolvedProvider;
use codebase_semantic_compiler::{
    compile_base_repair_prompt, compile_base_repair_prompt_with_history,
    parse_and_verify_base_response, CompiledBasePrompt, SemanticCompileError,
};

const MAX_VERIFIER_REPAIR_ATTEMPTS: usize = 2;

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

#[derive(Clone, Copy, Debug)]
enum RecoveryKind {
    Repair,
    ExecutionRetry,
}

#[derive(Clone, Debug)]
pub(super) struct RecoveryRequest {
    pub(super) partition_index: usize,
    pub(super) prompt: CompiledBasePrompt,
    kind: RecoveryKind,
    repair_attempts: usize,
    error_history: Vec<String>,
    verifier_errors: Vec<SemanticCompileError>,
}

impl RecoveryRequest {
    pub(super) fn attempt_label(&self) -> String {
        match self.kind {
            RecoveryKind::Repair => format!("AI {}차 교정", self.repair_attempts),
            RecoveryKind::ExecutionRetry => "1회 실행 재시도".to_string(),
        }
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
    let (prompt, kind, repair_attempts, verifier_errors) = match failure {
        ProviderAttemptFailure::Execution(_) => (
            original.clone(),
            RecoveryKind::ExecutionRetry,
            0,
            Vec::new(),
        ),
        ProviderAttemptFailure::Rejected { raw, error } => {
            let prompt =
                compile_base_repair_prompt(original, &raw, &error).map_err(|compile_error| {
                    format!("{first_error}; AI 교정 입력 생성 실패: {compile_error}")
                })?;
            (prompt, RecoveryKind::Repair, 1, vec![error])
        }
    };
    Ok(RecoveryRequest {
        partition_index,
        prompt,
        kind,
        repair_attempts,
        error_history: vec![first_error],
        verifier_errors,
    })
}

pub(super) fn continue_recovery_prompt(
    original: &CompiledBasePrompt,
    previous: &RecoveryRequest,
    failure: ProviderAttemptFailure,
) -> Result<RecoveryRequest, String> {
    let mut error_history = previous.error_history.clone();
    error_history.push(failure.describe(&previous.attempt_label()));
    let ProviderAttemptFailure::Rejected { raw, error } = failure else {
        return Err(error_history.join("; "));
    };
    if previous.repair_attempts >= MAX_VERIFIER_REPAIR_ATTEMPTS {
        return Err(error_history.join("; "));
    }
    let prompt =
        compile_base_repair_prompt_with_history(original, &raw, &error, &previous.verifier_errors)
            .map_err(|compile_error| {
                format!(
                    "{}; AI 후속 교정 입력 생성 실패: {compile_error}",
                    error_history.join("; ")
                )
            })?;
    let mut verifier_errors = previous.verifier_errors.clone();
    verifier_errors.push(error);
    Ok(RecoveryRequest {
        partition_index: previous.partition_index,
        prompt,
        kind: RecoveryKind::Repair,
        repair_attempts: previous.repair_attempts + 1,
        error_history,
        verifier_errors,
    })
}

pub(super) fn run_provider_with_repair(
    runtime: &ResolvedProvider,
    original: &CompiledBasePrompt,
    operation_id: &str,
    context: &str,
) -> Result<codebase_semantic_model::ApprovedSemanticRevision, String> {
    let failure = match verify_provider_result(
        original,
        broker::run_provider(runtime, original, operation_id),
    ) {
        Ok(revision) => return Ok(revision),
        Err(failure) => failure,
    };
    let mut request = compile_recovery_prompt(0, original, failure)
        .map_err(|error| format!("{context} {error}"))?;
    loop {
        match verify_provider_result(
            &request.prompt,
            broker::run_provider(runtime, &request.prompt, operation_id),
        ) {
            Ok(revision) => return Ok(revision),
            Err(failure) => {
                request = continue_recovery_prompt(original, &request, failure)
                    .map_err(|error| format!("{context} {error}"))?;
            }
        }
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
        assert_eq!(repair.attempt_label(), "AI 1차 교정");
        assert!(repair.prompt.task_prompt.contains("REPAIR_PAYLOAD_JSON"));
        assert_ne!(repair.prompt.task_prompt, original.task_prompt);

        let retry = compile_recovery_prompt(
            3,
            &original,
            ProviderAttemptFailure::Execution("provider unavailable".to_string()),
        )
        .unwrap();
        assert_eq!(retry.partition_index, 3);
        assert_eq!(retry.attempt_label(), "1회 실행 재시도");
        assert_eq!(retry.prompt.rendered_prompt(), original.rendered_prompt());
    }

    #[test]
    fn a_second_rejected_result_gets_one_more_repair_but_not_a_third() {
        let original = prompt_fixture();
        let first_failure = verify_provider_result(&original, Ok("{}".to_string())).unwrap_err();
        let first_repair = compile_recovery_prompt(2, &original, first_failure).unwrap();
        let second_failure =
            verify_provider_result(&first_repair.prompt, Ok("[]".to_string())).unwrap_err();
        let second_repair =
            continue_recovery_prompt(&original, &first_repair, second_failure).unwrap();

        assert_eq!(second_repair.attempt_label(), "AI 2차 교정");
        assert_eq!(second_repair.partition_index, 2);
        assert!(second_repair
            .prompt
            .task_prompt
            .contains("previousVerifierErrors"));
        assert!(second_repair
            .prompt
            .task_prompt
            .contains("missing field `schemaVersion`"));

        let third_failure =
            verify_provider_result(&second_repair.prompt, Ok("{}".to_string())).unwrap_err();
        let terminal =
            continue_recovery_prompt(&original, &second_repair, third_failure).unwrap_err();
        assert!(terminal.contains("AI 2차 교정 결과 검증 실패"));
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
            verification_phase: codebase_semantic_compiler::SemanticVerificationPhase::FinalMap,
            system_policy: "test policy".to_string(),
            task_prompt: "test task".to_string(),
            output_schema: base_semantic_output_schema(),
        }
    }
}
