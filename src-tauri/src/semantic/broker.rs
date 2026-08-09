use crate::{engine, provider};
use codebase_semantic_compiler::CompiledBasePrompt;
use codebase_semantic_model::{AiProviderKind, ReasoningEffort};
use serde_json::Value;
use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, AtomicUsize, Ordering},
    sync::mpsc,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const DEFAULT_INITIAL_PROVIDER_JOBS: usize = 4;
const DEFAULT_RECOVERY_PROVIDER_JOBS: usize = 2;
const MAX_CONFIGURED_PROVIDER_JOBS: usize = 8;
static STAGING_NONCE: AtomicU64 = AtomicU64::new(0);

pub(super) fn run_provider(
    runtime: &provider::ResolvedProvider,
    compiled: &CompiledBasePrompt,
    operation_id: &str,
) -> Result<String, String> {
    let runtime_kind = match compiled.packet.provider.kind {
        AiProviderKind::Codex => provider::AiProviderKind::Codex,
        AiProviderKind::Claude => provider::AiProviderKind::Claude,
    };
    if runtime.kind != runtime_kind {
        return Err("의미 분석 공급자와 등록된 CLI 종류가 다릅니다".to_string());
    }
    let staging = ProviderStaging::create()?;
    match compiled.packet.provider.kind {
        AiProviderKind::Codex => {
            run_codex(&runtime.executable, &staging.path, compiled, operation_id)
        }
        AiProviderKind::Claude => {
            run_claude(&runtime.executable, &staging.path, compiled, operation_id)
        }
    }
}

/// Executes independent first-pass semantic partitions. Each result is still
/// verified and cached independently; raising concurrency changes wall-clock
/// time only, never the prompt, evidence contract, or accepted output.
pub(super) fn run_provider_batch(
    runtime: &provider::ResolvedProvider,
    prompts: &[CompiledBasePrompt],
    operation_id: &str,
    on_completed: impl FnMut(usize, Result<String, String>),
) {
    run_provider_batch_with_limit(
        runtime,
        prompts,
        configured_provider_parallelism(
            "CODEBASE_WORKSPACE_AI_MAX_PARALLEL",
            DEFAULT_INITIAL_PROVIDER_JOBS,
        ),
        "initial",
        operation_id,
        on_completed,
    );
}

pub(super) fn run_provider_reduce_batch(
    runtime: &provider::ResolvedProvider,
    prompts: &[CompiledBasePrompt],
    operation_id: &str,
    on_completed: impl FnMut(usize, Result<String, String>),
) {
    run_provider_batch_with_limit(
        runtime,
        prompts,
        configured_provider_parallelism(
            "CODEBASE_WORKSPACE_AI_MAX_PARALLEL",
            DEFAULT_INITIAL_PROVIDER_JOBS,
        ),
        "reduce",
        operation_id,
        on_completed,
    );
}

/// Verifier-guided repairs and execution retries use a lower bound than the
/// normal path. Conservative recovery avoids turning one invalid response or
/// transient provider failure into a parallel retry storm.
pub(super) fn run_provider_repair_batch(
    runtime: &provider::ResolvedProvider,
    prompts: &[CompiledBasePrompt],
    operation_id: &str,
    on_completed: impl FnMut(usize, Result<String, String>),
) {
    run_provider_batch_with_limit(
        runtime,
        prompts,
        configured_provider_parallelism(
            "CODEBASE_WORKSPACE_AI_RETRY_MAX_PARALLEL",
            DEFAULT_RECOVERY_PROVIDER_JOBS,
        ),
        "repair",
        operation_id,
        on_completed,
    );
}

fn run_provider_batch_with_limit(
    runtime: &provider::ResolvedProvider,
    prompts: &[CompiledBasePrompt],
    max_parallel: usize,
    phase: &'static str,
    operation_id: &str,
    mut on_completed: impl FnMut(usize, Result<String, String>),
) {
    if prompts.is_empty() {
        return;
    }
    let worker_count = worker_count(prompts.len(), max_parallel);
    let batch_started = Instant::now();
    eprintln!(
        "@codebase-workspace-ai-batch {}",
        serde_json::json!({
            "phase": phase,
            "partitions": prompts.len(),
            "workers": worker_count,
        })
    );
    let next = AtomicUsize::new(0);
    let (sender, receiver) = mpsc::channel();
    let mut received = vec![false; prompts.len()];
    thread::scope(|scope| {
        for _ in 0..worker_count {
            let next = &next;
            let sender = sender.clone();
            scope.spawn(move || loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(prompt) = prompts.get(index) else {
                    break;
                };
                let started = Instant::now();
                let input_bytes = prompt.rendered_prompt().len();
                let result = run_provider(runtime, prompt, operation_id);
                eprintln!(
                    "@codebase-workspace-ai-performance {}",
                    serde_json::json!({
                        "phase": phase,
                        "partition": index,
                        "provider": match prompt.packet.provider.kind {
                            AiProviderKind::Codex => "codex",
                            AiProviderKind::Claude => "claude",
                        },
                        "model": &prompt.packet.provider.model,
                        "inputBytes": input_bytes,
                        "elapsedMs": started.elapsed().as_millis(),
                        "ok": result.is_ok(),
                    })
                );
                if sender.send((index, result)).is_err() {
                    break;
                }
            });
        }
        drop(sender);

        // Consume completions while the remaining provider processes are still
        // running. The caller can validate and persist each successful result
        // immediately instead of holding the whole batch in memory and losing
        // all later successes when one earlier partition is invalid.
        while let Ok((index, result)) = receiver.recv() {
            if let Some(seen) = received.get_mut(index) {
                *seen = true;
                on_completed(index, result);
            }
        }
    });

    for (index, was_received) in received.into_iter().enumerate() {
        if !was_received {
            on_completed(
                index,
                Err(format!(
                    "AI 의미 분석 작업 {}의 실행 결과를 회수하지 못했습니다",
                    index + 1
                )),
            );
        }
    }
    eprintln!(
        "@codebase-workspace-ai-batch-complete {}",
        serde_json::json!({
            "phase": phase,
            "partitions": prompts.len(),
            "workers": worker_count,
            "elapsedMs": batch_started.elapsed().as_millis(),
        })
    );
}

fn configured_provider_parallelism(variable: &str, default: usize) -> usize {
    env::var(variable)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| (1..=MAX_CONFIGURED_PROVIDER_JOBS).contains(value))
        .unwrap_or(default)
}

fn worker_count(prompt_count: usize, max_parallel: usize) -> usize {
    prompt_count.min(max_parallel.max(1))
}

fn run_codex(
    executable: &Path,
    staging: &Path,
    compiled: &CompiledBasePrompt,
    operation_id: &str,
) -> Result<String, String> {
    let schema_path = staging.join("base-semantic-output.schema.json");
    let output_path = staging.join("provider-output.json");
    fs::write(
        &schema_path,
        compiled
            .output_schema_pretty_json()
            .map_err(|error| format!("Codex output schema 생성 실패: {error}"))?,
    )
    .map_err(|error| format!("Codex output schema 저장 실패: {error}"))?;
    let args = codex_args(staging, compiled)?;
    let result = run_with_prompt(executable, &args, staging, compiled, operation_id)?;
    if !result.ok {
        return Err(provider_failure("Codex", &result.stderr));
    }
    fs::read_to_string(&output_path)
        .map_err(|error| format!("Codex 의미 결과를 읽지 못했습니다: {error}"))
}

fn codex_args(staging: &Path, compiled: &CompiledBasePrompt) -> Result<Vec<String>, String> {
    let schema_path = staging.join("base-semantic-output.schema.json");
    let output_path = staging.join("provider-output.json");
    Ok(vec![
        "exec".to_string(),
        // Semantic analysis is not a user conversation. Never persist or
        // resume a Codex session for a local partition or reconciliation.
        "--ephemeral".to_string(),
        "--ignore-user-config".to_string(),
        "--ignore-rules".to_string(),
        "--skip-git-repo-check".to_string(),
        "--sandbox".to_string(),
        "read-only".to_string(),
        "--output-schema".to_string(),
        path_text(&schema_path)?,
        "--output-last-message".to_string(),
        path_text(&output_path)?,
        "--model".to_string(),
        compiled.packet.provider.model.clone(),
        "--config".to_string(),
        codex_effort_config(compiled.packet.provider.effort),
        "-".to_string(),
    ])
}

fn run_claude(
    executable: &Path,
    staging: &Path,
    compiled: &CompiledBasePrompt,
    operation_id: &str,
) -> Result<String, String> {
    let args = claude_args(compiled)?;
    let result = run_with_prompt(executable, &args, staging, compiled, operation_id)?;
    if !result.ok {
        return Err(provider_failure("Claude", &result.stderr));
    }
    extract_claude_structured_output(&result.stdout)
}

fn claude_args(compiled: &CompiledBasePrompt) -> Result<Vec<String>, String> {
    let schema = serde_json::to_string(&compiled.output_schema)
        .map_err(|error| format!("Claude output schema 생성 실패: {error}"))?;
    Ok(vec![
        "--print".to_string(),
        "--safe-mode".to_string(),
        // Match Codex ephemeral execution: no local analysis call becomes a
        // Claude conversation and no later partition inherits its context.
        "--no-session-persistence".to_string(),
        "--permission-mode".to_string(),
        "dontAsk".to_string(),
        "--tools".to_string(),
        String::new(),
        "--output-format".to_string(),
        "json".to_string(),
        "--json-schema".to_string(),
        schema,
        "--model".to_string(),
        compiled.packet.provider.model.clone(),
        "--effort".to_string(),
        effort_value(compiled.packet.provider.effort).to_string(),
    ])
}

fn run_with_prompt(
    executable: &Path,
    args: &[String],
    staging: &Path,
    compiled: &CompiledBasePrompt,
    operation_id: &str,
) -> Result<engine::EngineRunResult, String> {
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    engine::run_command_with_input(
        executable,
        &refs,
        engine::EngineRunPolicy {
            hard_timeout: Duration::from_secs(30 * 60),
            idle_timeout: Duration::from_secs(10 * 60),
        },
        &[("CODEBASE_WORKSPACE_OPERATION_ID", operation_id)],
        staging,
        compiled.rendered_prompt().into_bytes(),
    )
}

fn extract_claude_structured_output(stdout: &str) -> Result<String, String> {
    let envelope: Value = serde_json::from_str(stdout.trim())
        .map_err(|error| format!("Claude JSON envelope 형식이 올바르지 않습니다: {error}"))?;
    if let Some(structured) = envelope
        .get("structured_output")
        .or_else(|| envelope.get("structuredOutput"))
    {
        return serde_json::to_string(structured)
            .map_err(|error| format!("Claude structured output 변환 실패: {error}"));
    }
    if let Some(result) = envelope.get("result").and_then(Value::as_str) {
        serde_json::from_str::<Value>(result)
            .map_err(|error| format!("Claude result에 구조화 JSON이 없습니다: {error}"))?;
        return Ok(result.to_string());
    }
    Err("Claude 응답에 structured output이 없습니다".to_string())
}

fn provider_failure(label: &str, stderr: &str) -> String {
    let detail = stderr.trim();
    if detail.is_empty() {
        format!("{label} 의미 분석이 실패했습니다")
    } else {
        format!("{label} 의미 분석이 실패했습니다: {detail}")
    }
}

const fn effort_value(effort: ReasoningEffort) -> &'static str {
    match effort {
        ReasoningEffort::Low => "low",
        ReasoningEffort::Medium => "medium",
        ReasoningEffort::High => "high",
        ReasoningEffort::Xhigh => "xhigh",
        ReasoningEffort::Max => "max",
        ReasoningEffort::Ultra => "ultra",
    }
}

fn codex_effort_config(effort: ReasoningEffort) -> String {
    format!("model_reasoning_effort=\"{}\"", effort_value(effort))
}

fn path_text(path: &Path) -> Result<String, String> {
    path.to_str()
        .filter(|value| !value.is_empty() && !value.chars().any(char::is_control))
        .map(str::to_string)
        .ok_or_else(|| "AI provider staging 경로를 실행 인자로 표현할 수 없습니다".to_string())
}

struct ProviderStaging {
    path: PathBuf,
}

impl ProviderStaging {
    fn create() -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!(
            "codebase-workspace-semantic-{}-{}-{}",
            std::process::id(),
            unix_millis(),
            STAGING_NONCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)
            .map_err(|error| format!("AI provider staging 폴더 생성 실패: {error}"))?;
        Ok(Self { path })
    }
}

impl Drop for ProviderStaging {
    fn drop(&mut self) {
        let temp_root = std::env::temp_dir();
        if self.path.parent() == Some(temp_root.as_path())
            && self
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("codebase-workspace-semantic-"))
        {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::{WorkspaceProvider, WorkspaceProviderKind, WorkspaceReasoningEffort};

    #[test]
    fn claude_envelope_extracts_only_structured_output() {
        let output = extract_claude_structured_output(
            r#"{"type":"result","result":"ignored","structured_output":{"schemaVersion":1}}"#,
        )
        .unwrap();
        assert_eq!(output, r#"{"schemaVersion":1}"#);
    }

    #[test]
    fn reasoning_effort_is_passed_to_both_cli_contracts_verbatim() {
        assert_eq!(effort_value(ReasoningEffort::High), "high");
        assert_eq!(
            codex_effort_config(ReasoningEffort::High),
            "model_reasoning_effort=\"high\""
        );
    }

    #[test]
    fn semantic_provider_calls_are_ephemeral_and_never_resume_chat_sessions() {
        let compiled = semantic_prompt_fixture();
        let staging = Path::new("semantic-test-staging");
        let codex = codex_args(staging, &compiled).unwrap();
        let claude = claude_args(&compiled).unwrap();

        assert!(codex.iter().any(|value| value == "--ephemeral"));
        assert!(!codex
            .iter()
            .any(|value| matches!(value.as_str(), "resume" | "--resume")));
        assert!(claude
            .iter()
            .any(|value| value == "--no-session-persistence"));
        assert!(!claude.iter().any(|value| value == "--continue"));
        let tools = claude
            .iter()
            .position(|value| value == "--tools")
            .and_then(|index| claude.get(index + 1));
        assert_eq!(tools.map(String::as_str), Some(""));
    }

    #[test]
    fn semantic_batch_parallelism_is_bounded_without_serializing_normal_work() {
        assert_eq!(worker_count(16, DEFAULT_INITIAL_PROVIDER_JOBS), 4);
        assert_eq!(worker_count(16, DEFAULT_RECOVERY_PROVIDER_JOBS), 2);
        assert_eq!(worker_count(1, DEFAULT_INITIAL_PROVIDER_JOBS), 1);
        assert_eq!(worker_count(0, DEFAULT_INITIAL_PROVIDER_JOBS), 0);
        assert_eq!(worker_count(3, 0), 1);
    }

    fn semantic_prompt_fixture() -> CompiledBasePrompt {
        let workspace = crate::workspace::Workspace {
            schema_version: 2,
            id: "ws-0123456789abcdef".to_string(),
            name: "fixture".to_string(),
            repo_path: ".".to_string(),
            provider: Some(WorkspaceProvider {
                kind: WorkspaceProviderKind::Codex,
                model: "gpt-5.6-sol".to_string(),
                effort: WorkspaceReasoningEffort::High,
            }),
            created_at: 1,
            updated_at: 1,
        };
        // Argument construction only reads the provider fields. Keeping this
        // fixture local avoids invoking either CLI in the offline test suite.
        CompiledBasePrompt {
            packet: codebase_semantic_model::BaseSemanticPacket {
                schema_version: codebase_semantic_model::BASE_SEMANTIC_SCHEMA_VERSION,
                task: codebase_semantic_model::SemanticTask::Base,
                workspace_id: codebase_fact_model::identity::WorkspaceId::parse(workspace.id)
                    .unwrap(),
                snapshot_id: codebase_fact_model::identity::SnapshotId::from_components(&[
                    "broker-test",
                ])
                .unwrap(),
                semantic_input_digest: codebase_fact_model::identity::Sha256Digest::of_bytes(
                    b"broker-test",
                ),
                provider: codebase_semantic_model::AiProviderDescriptor {
                    kind: codebase_semantic_model::AiProviderKind::Codex,
                    model: workspace.provider.as_ref().unwrap().model.clone(),
                    effort: ReasoningEffort::High,
                },
                output_language: codebase_semantic_model::OutputLanguage::Korean,
                packet_compiler_version: "test".to_string(),
                prompt_policy_version: "test".to_string(),
                scope_receipt: codebase_semantic_model::ScopeReceipt {
                    included: 0,
                    total: 0,
                    truncated: false,
                    reason: None,
                },
                input: codebase_semantic_model::BaseSemanticInput {
                    repository: codebase_semantic_model::ProjectSemanticContext {
                        fact_id: codebase_fact_model::identity::FactNodeId::from_components(&[
                            "broker-test",
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
            system_policy: "test".to_string(),
            task_prompt: "test".to_string(),
            output_schema: serde_json::json!({"type":"object"}),
        }
    }
}
