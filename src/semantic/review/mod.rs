//! `ai-context`를 AI provider에 보내고 의미 이름·한 줄 설명을 병합한다.

mod context;
mod merge;
mod partition;
mod prompt;
mod response;

use self::prompt::PromptLimits;
use self::response::ReviewProposal;
use crate::config::SemanticPolicy;
use crate::semantic::AiProvider;
use rayon::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
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
    let domain_partition = partition::split_to_budget_with_limits(
        &source_contexts,
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
        "[semantic] progress started source={} domain={}",
        original_context_count,
        domain_contexts.len()
    );
    if domain_contexts.len() != original_context_count {
        eprintln!(
            "[semantic] context_partition source={} domain={} max_input_bytes={}",
            original_context_count,
            domain_contexts.len(),
            policy.codex_max_input_bytes
        );
    }
    let provider = build_provider(policy);
    let domain_stage = run_domain_review(&provider, &domain_contexts, project_root, policy)?;

    let mut result = merge::merge(
        &source_contexts,
        &domain_stage.proposals,
        input.source_path.display().to_string(),
        domain_stage.failed_chunks,
        domain_stage.retry_attempts,
        policy.maximum_label_length,
        policy.maximum_summary_length,
    );
    result.warnings.extend(domain_stage.warnings);
    result.domain_completed_chunks = domain_stage.completed_chunks;
    result.chunk_count = domain_contexts.len();
    result.completed_chunks = domain_stage.completed_chunks;

    let feature_stage =
        run_feature_review(&provider, &source_contexts, &result, project_root, policy)?;
    merge::apply_feature_names(
        &mut result,
        &feature_stage.proposals,
        policy.maximum_label_length,
        policy.maximum_summary_length,
    );
    result.warnings.extend(feature_stage.warnings);
    result.retry_attempts += feature_stage.retry_attempts;
    eprintln!(
        "[semantic] static_labels features={} flows={}",
        result.features.len(),
        result.flows.len()
    );
    let json = serde_json::to_vec_pretty(&result).map_err(ReviewError::Serialize)?;
    write_atomic(output_path, &json)?;
    Ok(result)
}

fn build_provider(policy: &SemanticPolicy) -> AiProvider {
    match policy.provider.as_str() {
        "claude" => AiProvider::Claude(crate::semantic::ClaudeProvider {
            executable: policy.claude_executable.clone(),
            model: policy.claude_model.clone(),
            timeout_ms: policy.codex_timeout_ms,
            max_input_bytes: policy.codex_max_input_bytes,
        }),
        _ => AiProvider::Codex(crate::semantic::CodexProvider {
            executable: policy.codex_executable.clone(),
            model: policy.codex_model.clone(),
            timeout_ms: policy.codex_timeout_ms,
            max_input_bytes: policy.codex_max_input_bytes,
            command_prefix: Vec::new(),
        }),
    }
}

struct DomainReviewRun {
    proposals: Vec<ReviewProposal>,
    failed_chunks: usize,
    retry_attempts: usize,
    completed_chunks: usize,
    warnings: Vec<ReviewWarning>,
}

fn run_domain_review(
    provider: &AiProvider,
    contexts: &[context::ReviewContext],
    project_root: &Path,
    policy: &SemanticPolicy,
) -> Result<DomainReviewRun, ReviewError> {
    let mut run = DomainReviewRun {
        proposals: Vec::new(),
        failed_chunks: 0,
        retry_attempts: 0,
        completed_chunks: 0,
        warnings: Vec::new(),
    };
    eprintln!("[semantic] stage=domain started total={}", contexts.len());
    eprintln!(
        "[semantic] progress stage=domain total={} status=stage_started",
        contexts.len()
    );
    for (index, context) in contexts.iter().enumerate() {
        eprintln!(
            "[semantic] progress stage=domain chunk={} total={} status=started",
            index + 1,
            contexts.len()
        );
        let prompt = prompt::build_prompt(
            context,
            index,
            contexts.len(),
            PromptLimits {
                maximum_name_length: policy.maximum_label_length,
                maximum_summary_length: policy.maximum_summary_length,
            },
        )
        .map_err(ReviewError::Serialize)?;
        let mut last_error = None;
        let mut proposal = execute_proposal(provider, &prompt, project_root, &mut last_error);
        let empty_proposal = ReviewProposal::default();
        let mut missing = prompt::missing_domain_ids(
            context,
            proposal.as_ref().unwrap_or(&empty_proposal),
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
            let retry_prompt = prompt::build_missing_prompt(
                context,
                &missing,
                index,
                contexts.len(),
                PromptLimits {
                    maximum_name_length: policy.maximum_label_length,
                    maximum_summary_length: policy.maximum_summary_length,
                },
            )
            .map_err(ReviewError::Serialize)?;
            let Some(retry_proposal) =
                execute_proposal(provider, &retry_prompt, project_root, &mut last_error)
            else {
                continue;
            };
            if let Some(current) = proposal.as_mut() {
                current.merge_missing(retry_proposal);
            } else {
                proposal = Some(retry_proposal);
            }
            missing = prompt::missing_domain_ids(
                context,
                proposal
                    .as_ref()
                    .expect("재시도 성공 시 proposal이 존재해야 한다"),
                PromptLimits {
                    maximum_name_length: policy.maximum_label_length,
                    maximum_summary_length: policy.maximum_summary_length,
                },
            );
        }
        if let Some(proposal) = proposal {
            run.proposals.push(proposal);
            run.completed_chunks += 1;
            eprintln!(
                "[semantic] progress stage=domain chunk={} total={} status=completed",
                index + 1,
                contexts.len()
            );
        } else {
            run.failed_chunks += 1;
            run.warnings.push(ReviewWarning {
                code: "SEMANTIC_DOMAIN_CHUNK_FAILED".into(),
                item_id: Some(context.chunk_id.clone()),
                message: last_error.unwrap_or_else(|| "Codex 응답이 없습니다.".into()),
            });
            eprintln!(
                "[semantic] progress stage=domain chunk={} total={} status=failed",
                index + 1,
                contexts.len()
            );
        }
    }
    Ok(run)
}

const MIN_FEATURES_FOR_REVIEW: usize = 3;

struct FeatureReviewRun {
    proposals: Vec<ReviewProposal>,
    retry_attempts: usize,
    warnings: Vec<ReviewWarning>,
}

struct FeatureDomainOutcome {
    proposal: Option<ReviewProposal>,
    retry_attempts: usize,
    warnings: Vec<ReviewWarning>,
}

fn run_feature_review(
    provider: &AiProvider,
    source_contexts: &[context::ReviewContext],
    result: &SemanticReviewResult,
    project_root: &Path,
    policy: &SemanticPolicy,
) -> Result<FeatureReviewRun, ReviewError> {
    let mut originals = BTreeMap::new();
    for context in source_contexts {
        for feature in &context.features {
            originals
                .entry(feature.id.clone())
                .or_insert_with(|| feature.clone());
        }
    }
    let mut by_domain: BTreeMap<String, Vec<context::ReviewFeature>> = BTreeMap::new();
    let mut assigned = BTreeSet::new();
    for feature in &result.features {
        if !assigned.insert(feature.feature_id.clone()) {
            continue;
        }
        let Some(original) = originals.get(&feature.feature_id) else {
            continue;
        };
        let Some(domain_id) = feature.domain_ids.first() else {
            continue;
        };
        by_domain
            .entry(domain_id.clone())
            .or_default()
            .push(original.clone());
    }
    let domain_names = result
        .domains
        .iter()
        .map(|domain| (domain.domain_id.clone(), domain.name.clone()))
        .collect::<BTreeMap<_, _>>();
    let jobs: Vec<_> = by_domain
        .into_iter()
        .filter(|(_, features)| features.len() >= MIN_FEATURES_FOR_REVIEW)
        .map(|(domain_id, features)| {
            let name = domain_names
                .get(&domain_id)
                .cloned()
                .unwrap_or_else(|| domain_id.clone());
            (domain_id, name, features)
        })
        .collect();
    eprintln!("[semantic] stage=feature started total={}", jobs.len());
    if jobs.is_empty() {
        return Ok(FeatureReviewRun {
            proposals: Vec::new(),
            retry_attempts: 0,
            warnings: Vec::new(),
        });
    }

    let outcomes: Vec<FeatureDomainOutcome> = jobs
        .par_iter()
        .map(|(domain_id, domain_name, features)| {
            review_domain_features(
                provider,
                domain_id,
                domain_name,
                features,
                project_root,
                policy,
            )
        })
        .collect();

    let mut run = FeatureReviewRun {
        proposals: Vec::new(),
        retry_attempts: 0,
        warnings: Vec::new(),
    };
    for outcome in outcomes {
        run.retry_attempts += outcome.retry_attempts;
        run.warnings.extend(outcome.warnings);
        if let Some(proposal) = outcome.proposal {
            run.proposals.push(proposal);
        }
    }
    Ok(run)
}

fn review_domain_features(
    provider: &AiProvider,
    domain_id: &str,
    domain_name: &str,
    features: &[context::ReviewFeature],
    project_root: &Path,
    policy: &SemanticPolicy,
) -> FeatureDomainOutcome {
    let limits = PromptLimits {
        maximum_name_length: policy.maximum_label_length,
        maximum_summary_length: policy.maximum_summary_length,
    };
    let mut outcome = FeatureDomainOutcome {
        proposal: None,
        retry_attempts: 0,
        warnings: Vec::new(),
    };
    let Ok(prompt) = prompt::build_feature_prompt(domain_name, features, limits) else {
        outcome.warnings.push(ReviewWarning {
            code: "SEMANTIC_FEATURE_PROMPT_FAILED".into(),
            item_id: Some(domain_id.into()),
            message: "기능 이름 프롬프트를 만들지 못했습니다.".into(),
        });
        return outcome;
    };
    let mut last_error = None;
    let mut proposal = execute_proposal(provider, &prompt, project_root, &mut last_error);
    let empty = ReviewProposal::default();
    let mut missing =
        prompt::missing_feature_ids(features, proposal.as_ref().unwrap_or(&empty), limits);
    for _ in 0..policy.missing_item_retries {
        if missing.is_empty() {
            break;
        }
        outcome.retry_attempts += 1;
        let Ok(retry_prompt) =
            prompt::build_missing_feature_prompt(domain_name, features, &missing, limits)
        else {
            continue;
        };
        let Some(retry_proposal) =
            execute_proposal(provider, &retry_prompt, project_root, &mut last_error)
        else {
            continue;
        };
        if let Some(current) = proposal.as_mut() {
            current.merge_missing(retry_proposal);
        } else {
            proposal = Some(retry_proposal);
        }
        missing = prompt::missing_feature_ids(
            features,
            proposal
                .as_ref()
                .expect("재시도 성공 시 proposal이 존재해야 한다"),
            limits,
        );
    }
    if let Some(proposal) = proposal {
        outcome.proposal = Some(proposal);
    } else {
        outcome.warnings.push(ReviewWarning {
            code: "SEMANTIC_FEATURE_DOMAIN_FAILED".into(),
            item_id: Some(domain_id.into()),
            message: last_error.unwrap_or_else(|| "Codex 응답이 없습니다.".into()),
        });
    }
    outcome
}

fn execute_proposal(
    provider: &AiProvider,
    prompt: &str,
    project_root: &Path,
    last_error: &mut Option<String>,
) -> Option<ReviewProposal> {
    let stdout = match provider.execute_prompt(prompt, project_root) {
        Ok(stdout) => stdout,
        Err(error) => {
            *last_error = Some(error.to_string());
            return None;
        }
    };
    match response::parse_response(&stdout) {
        Ok(proposal) => Some(proposal),
        Err(error) => {
            *last_error = Some(error.to_string());
            None
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
