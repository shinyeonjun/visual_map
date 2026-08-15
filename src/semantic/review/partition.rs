//! Codex 입력 예산을 넘는 의미 분석 context를 도메인 단위로 안전하게 나눈다.

use super::context::{ReviewContext, ReviewDomain};
use super::prompt::{self, PromptLimits};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub(super) struct PartitionResult {
    pub(super) contexts: Vec<ReviewContext>,
}

#[derive(Clone, Copy)]
struct PartitionOptions {
    max_input_bytes: usize,
    maximum_label_length: usize,
    maximum_summary_length: usize,
}

#[cfg(test)]
fn split_to_budget(
    contexts: Vec<ReviewContext>,
    max_input_bytes: usize,
    maximum_label_length: usize,
    maximum_summary_length: usize,
) -> Result<PartitionResult, String> {
    split_to_budget_with_limits(
        &contexts,
        max_input_bytes,
        maximum_label_length,
        maximum_summary_length,
    )
}

pub(super) fn split_to_budget_with_limits(
    contexts: &[ReviewContext],
    max_input_bytes: usize,
    maximum_label_length: usize,
    maximum_summary_length: usize,
) -> Result<PartitionResult, String> {
    let normalized = normalize_contexts(contexts);
    split_normalized_to_budget(
        normalized,
        PartitionOptions {
            max_input_bytes,
            maximum_label_length,
            maximum_summary_length,
        },
    )
}

fn split_normalized_to_budget(
    contexts: Vec<ReviewContext>,
    options: PartitionOptions,
) -> Result<PartitionResult, String> {
    if options.max_input_bytes == 0 {
        return Err("Codex 입력 바이트 제한이 0입니다.".into());
    }

    let mut pending = VecDeque::from(contexts);
    let mut fitted = Vec::new();

    while let Some(context) = pending.pop_front() {
        if prompt_size(&context, options)? <= options.max_input_bytes {
            fitted.push(context);
            continue;
        }

        if context.domains.len() > 1 {
            let (left, right) = split_domains(&context.domains);
            pending.push_front(with_domains(&context, right));
            pending.push_front(with_domains(&context, left));
            continue;
        }

        let parts = split_single_domain(&context, options)?;
        for part in parts.into_iter().rev() {
            pending.push_front(part);
        }
    }

    assign_unique_chunk_ids(&mut fitted);
    Ok(PartitionResult { contexts: fitted })
}

/// 후처리 결과는 예산과 도메인 관계 때문에 같은 전역 항목을 여러 청크에
/// 표현할 수 있다. 의미분석은 청크가 아니라 전역 ID를 한 번만 분석해야
/// 하므로, 호출 직전에 도메인 관계만 합친다.
fn normalize_contexts(contexts: &[ReviewContext]) -> Vec<ReviewContext> {
    let Some(first) = contexts.first() else {
        return Vec::new();
    };
    let mut merged = first.clone();
    merged.chunk_id = "semantic-input".into();
    merged.domains.clear();
    merged.features.clear();
    merged.flows.clear();
    merged.adjacent_domains.clear();

    let mut domains = BTreeMap::new();
    for context in contexts {
        for domain in &context.domains {
            merge_domain(
                domains
                    .entry(domain.domain_id.clone())
                    .or_insert_with(|| domain.clone()),
                domain,
            );
        }
    }
    merged.domains = domains.into_values().collect();
    project_domains(&mut merged);
    vec![merged]
}

fn project_domains(context: &mut ReviewContext) {
    context.features.clear();
    context.flows.clear();
    for domain in &mut context.domains {
        domain.feature_ids.clear();
        domain.flow_ids.clear();
    }
}

fn merge_domain(destination: &mut ReviewDomain, source: &ReviewDomain) {
    merge_unique(
        &mut destination.source_domain_ids,
        &source.source_domain_ids,
    );
    merge_unique(&mut destination.source_paths, &source.source_paths);
    merge_unique_by_key(&mut destination.entrypoints, &source.entrypoints, |item| {
        item.id.clone()
    });
    merge_unique_by_key(&mut destination.resources, &source.resources, |item| {
        item.id.clone()
    });
}

fn merge_unique<T: PartialEq + Clone>(destination: &mut Vec<T>, source: &[T]) {
    for item in source {
        if !destination.contains(item) {
            destination.push(item.clone());
        }
    }
}

fn merge_unique_by_key<T: Clone>(
    destination: &mut Vec<T>,
    source: &[T],
    key: impl Fn(&T) -> String,
) {
    let mut keys = destination.iter().map(&key).collect::<BTreeSet<_>>();
    for item in source {
        if keys.insert(key(item)) {
            destination.push(item.clone());
        }
    }
}

fn split_single_domain(
    context: &ReviewContext,
    options: PartitionOptions,
) -> Result<Vec<ReviewContext>, String> {
    if context.domains.is_empty() {
        return Err(format!(
            "Codex context {}가 비어 있고 입력 예산을 초과했습니다.",
            context.chunk_id
        ));
    }

    let compact = compact_context(context);
    if prompt_size(&compact, options)? <= options.max_input_bytes {
        return Ok(vec![compact]);
    }
    Err(too_large_message(
        context,
        options.max_input_bytes,
        &compact,
    ))
}

fn split_domains(domains: &[ReviewDomain]) -> (Vec<ReviewDomain>, Vec<ReviewDomain>) {
    let total = domains.iter().map(serialized_size).sum::<usize>();
    let target = total / 2;
    let mut accumulated = 0;
    let mut midpoint = 1;
    for (index, domain) in domains.iter().enumerate().take(domains.len() - 1) {
        accumulated += serialized_size(domain);
        if accumulated >= target {
            midpoint = index + 1;
            break;
        }
        midpoint = index + 1;
    }
    (domains[..midpoint].to_vec(), domains[midpoint..].to_vec())
}

fn with_domains(context: &ReviewContext, domains: Vec<ReviewDomain>) -> ReviewContext {
    let mut result = context.clone();
    result.domains = domains;
    project_domains(&mut result);
    result
}

fn compact_context(context: &ReviewContext) -> ReviewContext {
    let mut result = context.clone();
    result.adjacent_domains.clear();
    result.global_summary.domain_ids.clear();
    result.global_summary.domain_labels.clear();
    result.global_summary.language_keys.clear();
    for domain in &mut result.domains {
        domain.source_paths.clear();
        domain.entrypoints.clear();
        domain.resources.clear();
    }
    result
}

fn prompt_size(context: &ReviewContext, options: PartitionOptions) -> Result<usize, String> {
    prompt::build_prompt(
        context,
        0,
        usize::MAX,
        PromptLimits {
            maximum_name_length: options.maximum_label_length,
            maximum_summary_length: options.maximum_summary_length,
        },
    )
    .map(|prompt| prompt.len())
    .map_err(|error| format!("의미 분석 프롬프트를 만들지 못했습니다: {error}"))
}

fn serialized_size<T: serde::Serialize>(value: &T) -> usize {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
}

fn too_large_message(
    context: &ReviewContext,
    max_input_bytes: usize,
    value: &ReviewContext,
) -> String {
    format!(
        "Codex 입력을 재분할·축약해도 제한을 넘습니다 (chunk={}, actual={} bytes, max={} bytes).",
        context.chunk_id,
        serialized_size(value),
        max_input_bytes
    )
}

fn assign_unique_chunk_ids(contexts: &mut [ReviewContext]) {
    let mut counts = BTreeMap::new();
    for context in contexts.iter() {
        *counts.entry(context.chunk_id.clone()).or_insert(0usize) += 1;
    }
    let mut seen = BTreeMap::new();
    for context in contexts {
        if counts.get(&context.chunk_id).copied().unwrap_or(0) > 1 {
            let index = seen.entry(context.chunk_id.clone()).or_insert(0usize);
            let base = context.chunk_id.clone();
            context.chunk_id = format!("{base}-part-{index:03}");
            *index += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::split_to_budget;
    use crate::semantic::review::context::{ReviewContext, ReviewDomain, ReviewProjectProfile};
    use serde_json::Value;

    fn many_domains_context(count: usize) -> ReviewContext {
        ReviewContext {
            schema_version: "context.v1".into(),
            chunk_id: "chunk-0000".into(),
            source_analysis_id: "analysis-1".into(),
            source_schema_version: "analysis.v1".into(),
            project_id: "project-1".into(),
            project_profile: ReviewProjectProfile::default(),
            global_summary: Default::default(),
            adjacent_domains: Vec::new(),
            domains: (0..count)
                .map(|index| ReviewDomain {
                    domain_id: format!("domain-{index}"),
                    source_domain_ids: Vec::new(),
                    current_label: format!("module-{}", "x".repeat(120)),
                    role: Value::Null,
                    signal: Value::Null,
                    source_paths: vec![format!("src/module-{index}/handler.rs")],
                    entrypoints: Vec::new(),
                    resources: Vec::new(),
                    feature_ids: Vec::new(),
                    flow_ids: Vec::new(),
                })
                .collect(),
            features: Vec::new(),
            flows: Vec::new(),
        }
    }

    #[test]
    fn 초과_context를_실제_프롬프트_예산_안으로_나눈다() {
        let result = split_to_budget(vec![many_domains_context(24)], 4_000, 120, 500)
            .expect("context를 분할해야 한다");
        assert!(result.contexts.len() > 1);
        assert!(result.contexts.iter().all(|context| {
            super::prompt_size(
                context,
                super::PartitionOptions {
                    max_input_bytes: 4_000,
                    maximum_label_length: 120,
                    maximum_summary_length: 500,
                },
            )
            .unwrap()
                <= 4_000
        }));
    }

    #[test]
    #[allow(non_snake_case)]
    fn 여러_후처리_청크의_동일_도메인을_한번만_분석한다() {
        let first = many_domains_context(3);
        let mut second = first.clone();
        second.chunk_id = "chunk-0001".into();

        let merged = super::split_to_budget_with_limits(
            &[first, second],
            2_000_000,
            120,
            500,
        )
        .expect("도메인 context를 만들어야 한다");
        assert_eq!(merged.contexts.len(), 1);
        assert_eq!(merged.contexts[0].domains.len(), 3);
        assert!(merged.contexts[0].features.is_empty());
        assert!(merged.contexts[0].flows.is_empty());
    }
}
