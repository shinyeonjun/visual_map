//! `codex-context`를 Codex에 보내고 의미 이름·한 줄 설명을 병합한다.

mod context;
mod merge;
mod partition;
mod prompt;
mod response;

use self::prompt::{PromptLimits, PromptStage, SemanticNames};
use self::response::ReviewProposal;
use crate::config::SemanticPolicy;
use crate::semantic::codex::CodexProvider;
use std::path::{Path, PathBuf};

pub use context::{load, ReviewInput};
pub use merge::{ReviewWarning, SemanticReviewResult};

#[derive(Debug)]
pub enum ReviewError {
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    InvalidInput {
        path: PathBuf,
        message: String,
    },
    Serialize(serde_json::Error),
}

impl std::fmt::Display for ReviewError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read { path, source } => write!(
                formatter,
                "입력 파일을 읽지 못했습니다 ({}): {source}",
                path.display()
            ),
            Self::Write { path, source } => write!(
                formatter,
                "의미 분석 결과를 저장하지 못했습니다 ({}): {source}",
                path.display()
            ),
            Self::InvalidInput { path, message } => write!(
                formatter,
                "Codex context가 올바르지 않습니다 ({}): {message}",
                path.display()
            ),
            Self::Serialize(error) => {
                write!(formatter, "의미 분석 JSON을 만들지 못했습니다: {error}")
            }
        }
    }
}

impl std::error::Error for ReviewError {}

pub fn run(
    input_path: &Path,
    output_path: &Path,
    project_root: &Path,
    policy: &SemanticPolicy,
) -> Result<SemanticReviewResult, ReviewError> {
    let input = context::load(input_path)?;
    let source_contexts = input.contexts;
    let original_context_count = source_contexts.len();
    let domain_partition = partition::split_to_budget_for_stage(
        &source_contexts,
        PromptStage::DomainFeature,
        &SemanticNames::default(),
        policy.codex_max_input_bytes,
        policy.maximum_label_length,
        policy.maximum_summary_length,
    )
    .map_err(|message| ReviewError::InvalidInput {
        path: input_path.to_path_buf(),
        message,
    })?;
    let domain_contexts = domain_partition.contexts;
    eprintln!(
        "[semantic] progress started source={} domain_feature={}",
        original_context_count,
        domain_contexts.len()
    );
    if domain_contexts.len() != original_context_count {
        eprintln!(
            "[semantic] context_partition source={} domain_feature={} max_input_bytes={}",
            original_context_count,
            domain_contexts.len(),
            policy.codex_max_input_bytes
        );
    }
    let provider = CodexProvider {
        executable: policy.codex_executable.clone(),
        model: policy.codex_model.clone(),
        timeout_ms: policy.codex_timeout_ms,
        max_input_bytes: policy.codex_max_input_bytes,
        command_prefix: Vec::new(),
    };
    let domain_stage = run_stage(
        &provider,
        &domain_contexts,
        project_root,
        policy,
        PromptStage::DomainFeature,
        &SemanticNames::default(),
    )?;
    let semantic_names = prompt::semantic_names(&domain_stage.proposals);
    let flow_partition = partition::split_to_budget_for_stage(
        &source_contexts,
        PromptStage::Flow,
        &semantic_names,
        policy.codex_max_input_bytes,
        policy.maximum_label_length,
        policy.maximum_summary_length,
    )
    .map_err(|message| ReviewError::InvalidInput {
        path: input_path.to_path_buf(),
        message,
    })?;
    let flow_contexts = flow_partition.contexts;
    eprintln!(
        "[semantic] stage_partition domain_feature={} flow={}",
        domain_contexts.len(),
        flow_contexts.len()
    );
    let flow_stage = run_stage(
        &provider,
        &flow_contexts,
        project_root,
        policy,
        PromptStage::Flow,
        &semantic_names,
    )?;
    let mut proposals = domain_stage.proposals;
    proposals.extend(flow_stage.proposals);
    let failed_chunks = domain_stage.failed_chunks + flow_stage.failed_chunks;
    let retry_attempts = domain_stage.retry_attempts + flow_stage.retry_attempts;
    let mut failure_details = domain_stage.warnings;
    failure_details.extend(flow_stage.warnings);

    let mut result = merge::merge(
        &source_contexts,
        &proposals,
        input.source_path.display().to_string(),
        failed_chunks,
        retry_attempts,
        policy.maximum_label_length,
        policy.maximum_summary_length,
    );
    result.warnings.extend(failure_details);
    result.semantic_stage_count = 2;
    result.domain_feature_completed_chunks = domain_stage.completed_chunks;
    result.flow_completed_chunks = flow_stage.completed_chunks;
    result.chunk_count = domain_contexts.len() + flow_contexts.len();
    result.completed_chunks = domain_stage.completed_chunks + flow_stage.completed_chunks;
    let json = serde_json::to_vec_pretty(&result).map_err(ReviewError::Serialize)?;
    write_atomic(output_path, &json)?;
    Ok(result)
}

struct StageRun {
    proposals: Vec<ReviewProposal>,
    failed_chunks: usize,
    retry_attempts: usize,
    completed_chunks: usize,
    warnings: Vec<ReviewWarning>,
}

fn run_stage(
    provider: &CodexProvider,
    contexts: &[context::ReviewContext],
    project_root: &Path,
    policy: &SemanticPolicy,
    stage: PromptStage,
    names: &SemanticNames,
) -> Result<StageRun, ReviewError> {
    let stage_name = match stage {
        PromptStage::DomainFeature => "domain_feature",
        PromptStage::Flow => "flow",
    };
    let failure_code = match stage {
        PromptStage::DomainFeature => "SEMANTIC_DOMAIN_FEATURE_CHUNK_FAILED",
        PromptStage::Flow => "SEMANTIC_FLOW_CHUNK_FAILED",
    };
    let mut run = StageRun {
        proposals: Vec::new(),
        failed_chunks: 0,
        retry_attempts: 0,
        completed_chunks: 0,
        warnings: Vec::new(),
    };
    eprintln!(
        "[semantic] stage={} started total={}",
        stage_name,
        contexts.len()
    );
    for (index, context) in contexts.iter().enumerate() {
        let prompt = prompt::build_stage(
            context,
            stage,
            names,
            index,
            contexts.len(),
            PromptLimits {
                maximum_name_length: policy.maximum_label_length,
                maximum_summary_length: policy.maximum_summary_length,
            },
        )
        .map_err(ReviewError::Serialize)?;
        let mut last_error = None;
        let mut proposal =
            execute_proposal(provider, &prompt, project_root, stage, &mut last_error);
        let mut empty_proposal = ReviewProposal::default();
        if let Some(proposal) = proposal.as_mut() {
            retain_stage_items(proposal, stage);
        } else {
            retain_stage_items(&mut empty_proposal, stage);
        }
        let mut missing = prompt::missing_items_for_stage(
            context,
            proposal.as_ref().unwrap_or(&empty_proposal),
            stage,
            PromptLimits {
                maximum_name_length: policy.maximum_label_length,
                maximum_summary_length: policy.maximum_summary_length,
            },
        );
        for _ in 0..policy.missing_item_retries {
            if missing.is_empty() {
                break;
            }
            run.retry_attempts += 1;
            let retry_prompt = prompt::build_missing_stage(
                context,
                &missing,
                stage,
                names,
                index,
                contexts.len(),
                PromptLimits {
                    maximum_name_length: policy.maximum_label_length,
                    maximum_summary_length: policy.maximum_summary_length,
                },
            )
            .map_err(ReviewError::Serialize)?;
            let Some(retry_proposal) = execute_proposal(
                provider,
                &retry_prompt,
                project_root,
                stage,
                &mut last_error,
            ) else {
                continue;
            };
            let mut retry_proposal = retry_proposal;
            retain_stage_items(&mut retry_proposal, stage);
            if let Some(current) = proposal.as_mut() {
                current.merge_missing(retry_proposal);
            } else {
                proposal = Some(retry_proposal);
            }
            missing = prompt::missing_items_for_stage(
                context,
                proposal
                    .as_ref()
                    .expect("재시도 성공 시 proposal이 존재해야 한다"),
                stage,
                PromptLimits {
                    maximum_name_length: policy.maximum_label_length,
                    maximum_summary_length: policy.maximum_summary_length,
                },
            );
        }
        if let Some(proposal) = proposal {
            run.proposals.push(proposal);
            run.completed_chunks += 1;
        } else {
            run.failed_chunks += 1;
            run.warnings.push(ReviewWarning {
                code: failure_code.into(),
                item_id: Some(context.chunk_id.clone()),
                message: last_error.unwrap_or_else(|| "Codex 응답이 없습니다.".into()),
            });
        }
        eprintln!(
            "[semantic] stage={} progress completed={} total={} retries={}",
            stage_name,
            index + 1,
            contexts.len(),
            run.retry_attempts
        );
    }
    Ok(run)
}

fn execute_proposal(
    provider: &CodexProvider,
    prompt: &str,
    project_root: &Path,
    stage: PromptStage,
    last_error: &mut Option<String>,
) -> Option<ReviewProposal> {
    let stdout = match provider.execute_prompt(prompt, project_root) {
        Ok(stdout) => stdout,
        Err(error) => {
            *last_error = Some(error.to_string());
            return None;
        }
    };
    match response::parse_jsonl(&stdout) {
        Ok(mut proposal) => {
            retain_stage_items(&mut proposal, stage);
            Some(proposal)
        }
        Err(error) => {
            *last_error = Some(error.to_string());
            None
        }
    }
}

fn retain_stage_items(proposal: &mut ReviewProposal, stage: PromptStage) {
    match stage {
        PromptStage::DomainFeature => proposal.flows.clear(),
        PromptStage::Flow => {
            proposal.domains.clear();
            proposal.features.clear();
        }
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ReviewError> {
    let temporary = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("json")
    ));
    std::fs::write(&temporary, bytes).map_err(|source| ReviewError::Write {
        path: temporary.clone(),
        source,
    })?;
    if let Err(source) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(ReviewError::Write {
            path: path.to_path_buf(),
            source,
        });
    }
    Ok(())
}
