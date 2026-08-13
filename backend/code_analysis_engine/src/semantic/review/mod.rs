//! `codex-context`를 Codex에 보내고 의미 이름·한 줄 설명을 병합한다.

mod context;
mod merge;
mod prompt;
mod response;

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
    let provider = CodexProvider {
        executable: policy.codex_executable.clone(),
        timeout_ms: policy.codex_timeout_ms,
        max_input_bytes: policy.codex_max_input_bytes,
        command_prefix: Vec::new(),
    };
    let mut proposals = Vec::new();
    let mut failed_chunks = 0;
    let mut retry_attempts = 0;
    for (index, context) in input.contexts.iter().enumerate() {
        let prompt = prompt::build(
            context,
            index,
            input.contexts.len(),
            policy.maximum_label_length,
            policy.maximum_summary_length,
        )
        .map_err(ReviewError::Serialize)?;
        let mut proposal = match provider.execute_prompt(&prompt, project_root) {
            Ok(stdout) => response::parse_jsonl(&stdout).ok(),
            Err(_) => None,
        };
        let mut missing = proposal
            .as_ref()
            .map(|proposal| {
                prompt::missing_items(
                    context,
                    proposal,
                    policy.maximum_label_length,
                    policy.maximum_summary_length,
                )
            })
            .unwrap_or_else(|| {
                prompt::missing_items(
                    context,
                    &response::ReviewProposal::default(),
                    policy.maximum_label_length,
                    policy.maximum_summary_length,
                )
            });

        for _ in 0..policy.missing_item_retries {
            if missing.is_empty() {
                break;
            }
            retry_attempts += 1;
            let retry_prompt = prompt::build_missing(
                context,
                &missing,
                index,
                input.contexts.len(),
                policy.maximum_label_length,
                policy.maximum_summary_length,
            )
            .map_err(ReviewError::Serialize)?;
            let Ok(stdout) = provider.execute_prompt(&retry_prompt, project_root) else {
                continue;
            };
            let Ok(retry_proposal) = response::parse_jsonl(&stdout) else {
                continue;
            };
            if let Some(current) = proposal.as_mut() {
                current.merge_missing(retry_proposal);
            } else {
                proposal = Some(retry_proposal);
            }
            missing = prompt::missing_items(
                context,
                proposal
                    .as_ref()
                    .expect("재시도 성공 시 proposal이 존재해야 한다"),
                policy.maximum_label_length,
                policy.maximum_summary_length,
            );
        }

        if let Some(proposal) = proposal {
            // 일부 항목이 계속 누락된 경우에도 부분 proposal을 merge에 전달한다.
            // merge가 누락된 각 ID에 대해 명시적인 warning을 생성한다.
            proposals.push(proposal);
        } else {
            failed_chunks += 1;
        }
    }

    let result = merge::merge(
        &input.contexts,
        &proposals,
        input.source_path.display().to_string(),
        failed_chunks,
        retry_attempts,
        policy.maximum_label_length,
        policy.maximum_summary_length,
    );
    let json = serde_json::to_vec_pretty(&result).map_err(ReviewError::Serialize)?;
    write_atomic(output_path, &json)?;
    Ok(result)
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
