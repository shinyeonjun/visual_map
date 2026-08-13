//! Codex 입력 예산을 넘는 의미 분석 context를 안전하게 나눈다.
//!
//! 후처리 단계의 청크 예산은 정적 카드의 크기를 제한한다. 필수 항목이
//! 많은 도메인은 그 예산을 초과할 수 있으므로, 실제 프롬프트를 만들기
//! 직전에 한 번 더 확인하고 도메인·기능·실행 흐름 순서로 재분할한다.

use super::context::{ReviewContext, ReviewDomain, ReviewFeature, ReviewFlow};
use super::prompt::{self, PromptLimits, PromptStage, SemanticNames};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub(super) struct PartitionResult {
    pub(super) contexts: Vec<ReviewContext>,
}

#[derive(Clone, Copy)]
struct PartitionOptions<'a> {
    stage: PromptStage,
    names: &'a SemanticNames,
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
    split_to_budget_for_stage(
        &contexts,
        PromptStage::Flow,
        &SemanticNames::default(),
        max_input_bytes,
        maximum_label_length,
        maximum_summary_length,
    )
}

pub(super) fn split_to_budget_for_stage(
    contexts: &[ReviewContext],
    stage: PromptStage,
    names: &SemanticNames,
    max_input_bytes: usize,
    maximum_label_length: usize,
    maximum_summary_length: usize,
) -> Result<PartitionResult, String> {
    let normalized = normalize_contexts(contexts, stage);
    split_normalized_to_budget(
        normalized,
        PartitionOptions {
            stage,
            names,
            max_input_bytes,
            maximum_label_length,
            maximum_summary_length,
        },
    )
}

fn split_normalized_to_budget(
    contexts: Vec<ReviewContext>,
    options: PartitionOptions<'_>,
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
/// 하므로, 호출 직전에 관계를 합치고 단계별 투영을 적용한다.
fn normalize_contexts(contexts: &[ReviewContext], stage: PromptStage) -> Vec<ReviewContext> {
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
    let mut features = BTreeMap::new();
    let mut flows = BTreeMap::new();
    let mut adjacent_domains = BTreeMap::new();
    for context in contexts {
        for domain in &context.domains {
            merge_domain(
                domains
                    .entry(domain.domain_id.clone())
                    .or_insert_with(|| domain.clone()),
                domain,
            );
        }
        for feature in &context.features {
            merge_feature(
                features
                    .entry(feature.id.clone())
                    .or_insert_with(|| feature.clone()),
                feature,
            );
        }
        for flow in &context.flows {
            merge_flow(
                flows.entry(flow.id.clone()).or_insert_with(|| flow.clone()),
                flow,
            );
        }
        for adjacent in &context.adjacent_domains {
            let entry = adjacent_domains
                .entry(adjacent.domain_id.clone())
                .or_insert_with(|| adjacent.clone());
            merge_unique(&mut entry.relation_kinds, &adjacent.relation_kinds);
        }
    }
    merged.domains = domains.into_values().collect();
    merged.features = features.into_values().collect();
    merged.flows = flows.into_values().collect();
    merged.adjacent_domains = adjacent_domains.into_values().collect();
    project_stage(&mut merged, stage);
    vec![merged]
}

fn project_stage(context: &mut ReviewContext, stage: PromptStage) {
    match stage {
        PromptStage::DomainFeature => {
            context.flows.clear();
            for domain in &mut context.domains {
                domain.flow_ids.clear();
            }
            for feature in &mut context.features {
                feature.flow_ids.clear();
            }
        }
        PromptStage::Flow => {
            let flow_ids = context
                .flows
                .iter()
                .map(|flow| flow.id.as_str())
                .collect::<BTreeSet<_>>();
            context.domains.retain(|domain| {
                domain
                    .flow_ids
                    .iter()
                    .any(|id| flow_ids.contains(id.as_str()))
            });
            for domain in &mut context.domains {
                domain.feature_ids.retain(|id| {
                    context.features.iter().any(|feature| {
                        feature.id == *id
                            && feature
                                .flow_ids
                                .iter()
                                .any(|flow| flow_ids.contains(flow.as_str()))
                    })
                });
                domain.flow_ids.retain(|id| flow_ids.contains(id.as_str()));
            }
            context.features.retain(|feature| {
                feature
                    .flow_ids
                    .iter()
                    .any(|id| flow_ids.contains(id.as_str()))
            });
            for feature in &mut context.features {
                feature.flow_ids.retain(|id| flow_ids.contains(id.as_str()));
            }
            for flow in &mut context.flows {
                flow.feature_ids
                    .retain(|id| context.features.iter().any(|feature| feature.id == *id));
                flow.domain_ids
                    .retain(|id| context.domains.iter().any(|domain| domain.domain_id == *id));
            }
        }
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
    merge_unique(&mut destination.feature_ids, &source.feature_ids);
    merge_unique(&mut destination.flow_ids, &source.flow_ids);
}

fn merge_feature(destination: &mut ReviewFeature, source: &ReviewFeature) {
    merge_unique(&mut destination.domain_ids, &source.domain_ids);
    merge_unique(&mut destination.tags, &source.tags);
    merge_unique(&mut destination.symbols, &source.symbols);
    merge_unique(&mut destination.source_paths, &source.source_paths);
    merge_unique(&mut destination.entrypoint_ids, &source.entrypoint_ids);
    merge_unique(&mut destination.resource_ids, &source.resource_ids);
    merge_unique(&mut destination.flow_ids, &source.flow_ids);
    destination.required |= source.required;
}

fn merge_flow(destination: &mut ReviewFlow, source: &ReviewFlow) {
    merge_unique(&mut destination.domain_ids, &source.domain_ids);
    merge_unique(&mut destination.feature_ids, &source.feature_ids);
    merge_unique(&mut destination.steps, &source.steps);
    merge_unique(
        &mut destination.dynamic_boundary_ids,
        &source.dynamic_boundary_ids,
    );
    if destination.owner_name.is_empty() {
        destination.owner_name = source.owner_name.clone();
    }
    if destination.owner_unit_id.is_empty() {
        destination.owner_unit_id = source.owner_unit_id.clone();
    }
    if destination.selection_reason.is_empty() {
        destination.selection_reason = source.selection_reason.clone();
    }
    destination.required |= source.required;
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
    options: PartitionOptions<'_>,
) -> Result<Vec<ReviewContext>, String> {
    let Some(domain) = context.domains.first() else {
        return Err(format!(
            "Codex context {}가 비어 있고 입력 예산을 초과했습니다.",
            context.chunk_id
        ));
    };

    let flows_by_id = context
        .flows
        .iter()
        .map(|flow| (flow.id.clone(), flow.clone()))
        .collect::<BTreeMap<_, _>>();
    let features_by_id = context
        .features
        .iter()
        .map(|feature| (feature.id.clone(), feature.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut referenced_flow_ids = BTreeSet::new();
    let mut bundles = Vec::new();

    for feature_id in &domain.feature_ids {
        let Some(feature) = features_by_id.get(feature_id) else {
            continue;
        };
        let flows = feature
            .flow_ids
            .iter()
            .filter_map(|flow_id| {
                let flow = flows_by_id.get(flow_id)?.clone();
                referenced_flow_ids.insert(flow_id.clone());
                Some(flow)
            })
            .collect::<Vec<_>>();
        bundles.push((vec![feature.clone()], flows));
    }

    for flow_id in &domain.flow_ids {
        let Some(flow) = flows_by_id.get(flow_id) else {
            continue;
        };
        if !referenced_flow_ids.contains(&flow.id) {
            bundles.push((Vec::new(), vec![flow.clone()]));
        }
    }

    if bundles.is_empty() {
        let compact = compact_context(context);
        if prompt_size(&compact, options)? <= options.max_input_bytes {
            return Ok(vec![compact]);
        }
        return Err(too_large_message(
            context,
            options.max_input_bytes,
            &compact,
        ));
    }

    let mut parts = Vec::new();
    let mut current_features = Vec::new();
    let mut current_flows = Vec::new();

    for (features, flows) in bundles {
        let candidate_features = append_unique_features(&current_features, &features);
        let candidate_flows = append_unique_flows(&current_flows, &flows);
        let candidate = context_with_items(context, &candidate_features, &candidate_flows);
        if current_features.is_empty() && current_flows.is_empty() {
            let fitting = split_bundle_if_needed(context, &features, &flows, options)?;
            if fitting.len() == 1 {
                current_features = candidate_features;
                current_flows = candidate_flows;
            } else {
                parts.extend(fitting);
            }
            continue;
        }

        if prompt_size(&candidate, options)? <= options.max_input_bytes {
            current_features = candidate_features;
            current_flows = candidate_flows;
        } else {
            parts.push(context_with_items(
                context,
                &current_features,
                &current_flows,
            ));
            let fitting = split_bundle_if_needed(context, &features, &flows, options)?;
            if fitting.len() == 1 {
                current_features = features;
                current_flows = flows;
            } else {
                parts.extend(fitting);
                current_features = Vec::new();
                current_flows = Vec::new();
            }
        }
    }

    if !current_features.is_empty() || !current_flows.is_empty() {
        parts.push(context_with_items(
            context,
            &current_features,
            &current_flows,
        ));
    }

    for part in &parts {
        if prompt_size(part, options)? > options.max_input_bytes {
            return Err(too_large_message(context, options.max_input_bytes, part));
        }
    }
    Ok(parts)
}

fn split_bundle_if_needed(
    context: &ReviewContext,
    features: &[ReviewFeature],
    flows: &[ReviewFlow],
    options: PartitionOptions<'_>,
) -> Result<Vec<ReviewContext>, String> {
    let candidate = context_with_items(context, features, flows);
    if prompt_size(&candidate, options)? <= options.max_input_bytes {
        return Ok(vec![candidate]);
    }

    if flows.len() > 1 {
        let midpoint = flows.len() / 2;
        let mut left = split_bundle_if_needed(context, features, &flows[..midpoint], options)?;
        let right = split_bundle_if_needed(context, features, &flows[midpoint..], options)?;
        left.extend(right);
        return Ok(left);
    }

    let compact = compact_context(&candidate);
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
    let domain_ids = domains
        .iter()
        .map(|domain| domain.domain_id.clone())
        .collect::<BTreeSet<_>>();
    let fallback_domain_id = context
        .domains
        .iter()
        .map(|domain| domain.domain_id.as_str())
        .min()
        .map(str::to_owned);
    result.domains = domains;
    let feature_ids = result
        .domains
        .iter()
        .flat_map(|domain| domain.feature_ids.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    let flow_ids = result
        .domains
        .iter()
        .flat_map(|domain| domain.flow_ids.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    result.features.retain(|feature| {
        feature_ids.contains(&feature.id)
            && item_owner_domain(&feature.domain_ids, fallback_domain_id.as_deref())
                .is_some_and(|owner| domain_ids.contains(owner))
    });
    result.flows.retain(|flow| {
        flow_ids.contains(&flow.id)
            && item_owner_domain(&flow.domain_ids, fallback_domain_id.as_deref())
                .is_some_and(|owner| domain_ids.contains(owner))
    });
    result
}

fn item_owner_domain<'a>(domain_ids: &'a [String], fallback: Option<&'a str>) -> Option<&'a str> {
    domain_ids.iter().map(String::as_str).min().or(fallback)
}

fn context_with_items(
    context: &ReviewContext,
    features: &[ReviewFeature],
    flows: &[ReviewFlow],
) -> ReviewContext {
    let mut result = context.clone();
    let Some(original) = context.domains.first() else {
        result.domains.clear();
        return result;
    };

    let flow_ids = flows
        .iter()
        .map(|flow| flow.id.as_str())
        .collect::<BTreeSet<_>>();
    let feature_ids = features
        .iter()
        .map(|feature| feature.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut domain = original.clone();
    domain.feature_ids = features.iter().map(|feature| feature.id.clone()).collect();
    domain.flow_ids = flows.iter().map(|flow| flow.id.clone()).collect();
    result.domains = vec![domain];
    result.features = features
        .iter()
        .cloned()
        .map(|mut feature| {
            feature
                .flow_ids
                .retain(|flow_id| flow_ids.contains(flow_id.as_str()));
            feature
        })
        .collect();
    result.flows = flows
        .iter()
        .cloned()
        .map(|mut flow| {
            flow.feature_ids
                .retain(|feature_id| feature_ids.contains(feature_id.as_str()));
            flow
        })
        .collect();
    result
}

fn append_unique_features(
    current: &[ReviewFeature],
    additional: &[ReviewFeature],
) -> Vec<ReviewFeature> {
    let mut result = current.to_vec();
    let ids = result
        .iter()
        .map(|feature| feature.id.clone())
        .collect::<BTreeSet<_>>();
    result.extend(
        additional
            .iter()
            .filter(|feature| !ids.contains(feature.id.as_str()))
            .cloned(),
    );
    result
}

fn append_unique_flows(current: &[ReviewFlow], additional: &[ReviewFlow]) -> Vec<ReviewFlow> {
    let mut result = current.to_vec();
    let ids = result
        .iter()
        .map(|flow| flow.id.clone())
        .collect::<BTreeSet<_>>();
    result.extend(
        additional
            .iter()
            .filter(|flow| !ids.contains(flow.id.as_str()))
            .cloned(),
    );
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
    for feature in &mut result.features {
        feature.symbols.clear();
        feature.source_paths.clear();
        feature.entrypoint_ids.clear();
        feature.resource_ids.clear();
    }
    for flow in &mut result.flows {
        flow.steps.clear();
        flow.dynamic_boundary_ids.clear();
        flow.selection_reason.clear();
    }
    result
}

fn prompt_size(context: &ReviewContext, options: PartitionOptions<'_>) -> Result<usize, String> {
    prompt::build_stage(
        context,
        options.stage,
        options.names,
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
    use crate::semantic::review::context::{
        ReviewContext, ReviewDomain, ReviewFeature, ReviewFlow, ReviewFlowStep,
        ReviewProjectProfile,
    };
    use serde_json::Value;
    use std::collections::BTreeSet;

    fn oversized_context() -> ReviewContext {
        let flows = (0..12)
            .map(|index| ReviewFlow {
                id: format!("flow-{index}"),
                domain_ids: vec!["domain-1".into()],
                feature_ids: vec!["feature-1".into()],
                owner_unit_id: "unit-1".into(),
                owner_name: "handler".into(),
                required: true,
                steps: vec![ReviewFlowStep {
                    kind: Value::String("call".into()),
                    label: "x".repeat(20_000),
                }],
                dynamic_boundary_ids: Vec::new(),
                selection_reason: String::new(),
            })
            .collect();
        ReviewContext {
            schema_version: "context.v1".into(),
            chunk_id: "chunk-0000".into(),
            source_analysis_id: "analysis-1".into(),
            source_schema_version: "analysis.v1".into(),
            project_id: "project-1".into(),
            project_profile: ReviewProjectProfile::default(),
            global_summary: Default::default(),
            adjacent_domains: Vec::new(),
            domains: vec![ReviewDomain {
                domain_id: "domain-1".into(),
                source_domain_ids: Vec::new(),
                current_label: "domain".into(),
                role: Value::Null,
                signal: Value::Null,
                source_paths: Vec::new(),
                entrypoints: Vec::new(),
                resources: Vec::new(),
                feature_ids: vec!["feature-1".into()],
                flow_ids: (0..12).map(|index| format!("flow-{index}")).collect(),
            }],
            features: vec![ReviewFeature {
                id: "feature-1".into(),
                domain_ids: vec!["domain-1".into()],
                current_label: "feature".into(),
                visibility: Value::Null,
                required: true,
                tags: Vec::new(),
                symbols: Vec::new(),
                source_paths: Vec::new(),
                entrypoint_ids: Vec::new(),
                resource_ids: Vec::new(),
                flow_ids: (0..12).map(|index| format!("flow-{index}")).collect(),
            }],
            flows,
        }
    }

    #[test]
    fn 초과_context를_실제_프롬프트_예산_안으로_나눈다() {
        let result = split_to_budget(vec![oversized_context()], 100_000, 120, 500)
            .expect("context를 분할해야 한다");
        assert!(result.contexts.len() > 1);
        assert!(result.contexts.iter().all(|context| {
            super::prompt_size(
                context,
                super::PartitionOptions {
                    stage: super::PromptStage::Flow,
                    names: &super::SemanticNames::default(),
                    max_input_bytes: 100_000,
                    maximum_label_length: 120,
                    maximum_summary_length: 500,
                },
            )
            .unwrap()
                <= 100_000
        }));
        assert!(result
            .contexts
            .iter()
            .flat_map(|context| context.domains.iter())
            .flat_map(|domain| domain.flow_ids.iter())
            .any(|flow_id| flow_id == "flow-11"));
    }

    #[test]
    fn 분할된_context의_feature_flow_참조를_국소적으로_정리한다() {
        let result = split_to_budget(vec![oversized_context()], 100_000, 120, 500)
            .expect("context를 분할해야 한다");
        for context in result.contexts {
            let domain = &context.domains[0];
            let feature_ids = domain
                .feature_ids
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            let flow_ids = domain
                .flow_ids
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            assert!(context.features.iter().all(|feature| feature
                .flow_ids
                .iter()
                .all(|id| flow_ids.contains(id.as_str()))));
            assert!(context.flows.iter().all(|flow| flow
                .feature_ids
                .iter()
                .all(|id| feature_ids.contains(id.as_str()))));
        }
    }

    #[test]
    #[allow(non_snake_case)]
    fn 여러_후처리_청크의_동일_ID를_단계별로_한번만_분석한다() {
        let first = oversized_context();
        let mut second = first.clone();
        second.chunk_id = "chunk-0001".into();

        let domain_stage = super::split_to_budget_for_stage(
            &[first.clone(), second.clone()],
            super::PromptStage::DomainFeature,
            &super::SemanticNames::default(),
            2_000_000,
            120,
            500,
        )
        .expect("도메인·기능 단계 context를 만들어야 한다");
        assert_eq!(domain_stage.contexts.len(), 1);
        assert_eq!(domain_stage.contexts[0].domains.len(), 1);
        assert_eq!(domain_stage.contexts[0].features.len(), 1);
        assert!(domain_stage.contexts[0].flows.is_empty());

        let flow_stage = super::split_to_budget_for_stage(
            &[first, second],
            super::PromptStage::Flow,
            &super::SemanticNames::default(),
            2_000_000,
            120,
            500,
        )
        .expect("흐름 단계 context를 만들어야 한다");
        let flow_ids = flow_stage.contexts[0]
            .flows
            .iter()
            .map(|flow| flow.id.clone())
            .collect::<BTreeSet<_>>();
        assert_eq!(flow_stage.contexts.len(), 1);
        assert_eq!(flow_ids.len(), 12);
        assert_eq!(flow_stage.contexts[0].flows.len(), 12);
    }
}
