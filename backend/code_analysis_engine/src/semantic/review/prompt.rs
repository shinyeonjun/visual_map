//! 도메인·기능·실행 흐름 의미 분석 프롬프트를 만든다.

use super::context::ReviewContext;
use super::response::ReviewProposal;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptStage {
    DomainFeature,
    Flow,
}

#[derive(Debug, Clone, Default)]
pub struct SemanticNames {
    pub domains: BTreeMap<String, SemanticDomainName>,
    pub features: BTreeMap<String, SemanticItemName>,
}

#[derive(Debug, Clone, Default)]
pub struct SemanticDomainName {
    pub canonical_domain_id: String,
    pub name: String,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SemanticItemName {
    pub name: String,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct PromptLimits {
    pub maximum_name_length: usize,
    pub maximum_summary_length: usize,
}

pub fn semantic_names(proposals: &[ReviewProposal]) -> SemanticNames {
    let mut names = SemanticNames::default();
    for proposal in proposals {
        for domain in &proposal.domains {
            names
                .domains
                .entry(domain.domain_id.clone())
                .or_insert_with(|| SemanticDomainName {
                    canonical_domain_id: domain
                        .canonical_domain_id
                        .clone()
                        .unwrap_or_else(|| domain.domain_id.clone()),
                    name: domain.name.clone(),
                    summary: domain.summary.clone(),
                });
        }
        for feature in &proposal.features {
            names
                .features
                .entry(feature.feature_id.clone())
                .or_insert_with(|| SemanticItemName {
                    name: feature.name.clone(),
                    summary: feature.summary.clone(),
                });
        }
    }
    let resolved = names.domains.clone();
    for domain in names.domains.values_mut() {
        if let Some(canonical) = resolved.get(&domain.canonical_domain_id) {
            domain.name = canonical.name.clone();
            domain.summary = canonical.summary.clone();
        }
    }
    names
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MissingItems {
    pub domains: BTreeSet<String>,
    pub features: BTreeSet<String>,
    pub flows: BTreeSet<String>,
}

impl MissingItems {
    pub fn is_empty(&self) -> bool {
        self.domains.is_empty() && self.features.is_empty() && self.flows.is_empty()
    }
}

pub fn build_stage(
    context: &ReviewContext,
    stage: PromptStage,
    names: &SemanticNames,
    chunk_index: usize,
    chunk_count: usize,
    limits: PromptLimits,
) -> Result<String, serde_json::Error> {
    let context_json = stage_context(context, stage, names);
    let stage_instructions = match stage {
        PromptStage::DomainFeature => {
            "이번 호출은 비즈니스 도메인과 기능 의미분석 단계다.\n- domainId마다 canonicalDomainId를 반드시 입력하라. canonicalDomainId는 입력에 존재하는 domainId 중 하나만 사용하라.\n- 같은 비즈니스 영역이라고 판단되는 domainId는 하나의 canonicalDomainId로 매핑하라.\n- 도메인·기능의 name과 한 줄 summary만 생성하라.\n- flows는 빈 배열로 반환하라."
        }
        PromptStage::Flow => {
            "이번 호출은 실행 흐름 의미분석 단계다.\n- 입력의 각 flowId를 정확히 한 번씩 반환하라.\n- 입력에 표시된 비즈니스 도메인·기능 이름과 정적 단계만 근거로 흐름 name과 한 줄 summary를 생성하라.\n- domains와 features는 빈 배열로 반환하라."
        }
    };
    Ok(include_str!("../prompts/semantic_review.txt")
        .replace("{chunk_number}", &(chunk_index + 1).to_string())
        .replace("{chunk_count}", &chunk_count.to_string())
        .replace(
            "{maximum_name_length}",
            &limits.maximum_name_length.to_string(),
        )
        .replace(
            "{maximum_summary_length}",
            &limits.maximum_summary_length.to_string(),
        )
        .replace("{stage_instructions}", stage_instructions)
        .replace("{context_json}", &context_json.to_string()))
}

pub fn missing_items_for_stage(
    context: &ReviewContext,
    proposal: &ReviewProposal,
    stage: PromptStage,
    limits: PromptLimits,
) -> MissingItems {
    let domain_ids = proposal
        .domains
        .iter()
        .filter(|item| {
            valid_suggestion(
                &item.name,
                &item.summary,
                limits.maximum_name_length,
                limits.maximum_summary_length,
            )
        })
        .map(|item| item.domain_id.as_str())
        .collect::<BTreeSet<_>>();
    let feature_ids = proposal
        .features
        .iter()
        .filter(|item| {
            valid_suggestion(
                &item.name,
                &item.summary,
                limits.maximum_name_length,
                limits.maximum_summary_length,
            )
        })
        .map(|item| item.feature_id.as_str())
        .collect::<BTreeSet<_>>();
    let flow_ids = proposal
        .flows
        .iter()
        .filter(|item| {
            valid_suggestion(
                &item.name,
                &item.summary,
                limits.maximum_name_length,
                limits.maximum_summary_length,
            )
        })
        .map(|item| item.flow_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut missing = MissingItems::default();
    match stage {
        PromptStage::DomainFeature => {
            for domain in &context.domains {
                if !domain_ids.contains(domain.domain_id.as_str()) {
                    missing.domains.insert(domain.domain_id.clone());
                }
            }
            for feature in &context.features {
                if !feature_ids.contains(feature.id.as_str()) {
                    missing.features.insert(feature.id.clone());
                }
            }
        }
        PromptStage::Flow => {
            for flow in &context.flows {
                if !flow_ids.contains(flow.id.as_str()) {
                    missing.flows.insert(flow.id.clone());
                }
            }
        }
    }
    missing
}

pub fn build_missing_stage(
    context: &ReviewContext,
    missing: &MissingItems,
    stage: PromptStage,
    names: &SemanticNames,
    chunk_index: usize,
    chunk_count: usize,
    limits: PromptLimits,
) -> Result<String, serde_json::Error> {
    let narrowed = context_with_missing_items(context, missing, stage);
    build_stage(&narrowed, stage, names, chunk_index, chunk_count, limits)
}

fn context_with_missing_items(
    context: &ReviewContext,
    missing: &MissingItems,
    stage: PromptStage,
) -> ReviewContext {
    let mut narrowed = context.clone();
    match stage {
        PromptStage::DomainFeature => {
            narrowed.domains = context
                .domains
                .iter()
                .filter_map(|domain| {
                    let requested = missing.domains.contains(&domain.domain_id);
                    let mut domain = domain.clone();
                    domain
                        .feature_ids
                        .retain(|id| missing.features.contains(id));
                    domain.flow_ids.clear();
                    if requested || !domain.feature_ids.is_empty() {
                        Some(domain)
                    } else {
                        None
                    }
                })
                .collect();
            narrowed
                .features
                .retain(|feature| missing.features.contains(&feature.id));
            narrowed.flows.clear();
        }
        PromptStage::Flow => {
            narrowed.domains = context
                .domains
                .iter()
                .filter_map(|domain| {
                    let mut domain = domain.clone();
                    domain.flow_ids.retain(|id| missing.flows.contains(id));
                    if !domain.flow_ids.is_empty() {
                        Some(domain)
                    } else {
                        None
                    }
                })
                .collect();
            narrowed
                .features
                .retain(|feature| feature.flow_ids.iter().any(|id| missing.flows.contains(id)));
            narrowed
                .flows
                .retain(|flow| missing.flows.contains(&flow.id));
        }
    }
    narrowed
}

fn stage_context(context: &ReviewContext, stage: PromptStage, names: &SemanticNames) -> Value {
    let domains = context.domains.iter().map(|domain| {
        let semantic = names.domains.get(&domain.domain_id);
        json!({
            "domainId": domain.domain_id,
            "currentLabel": domain.current_label,
            "businessDomainId": semantic.map(|item| item.canonical_domain_id.clone()),
            "businessName": semantic.map(|item| item.name.clone()),
            "businessSummary": semantic.and_then(|item| item.summary.clone()),
            "featureIds": domain.feature_ids,
            "flowIds": if matches!(stage, PromptStage::Flow) { domain.flow_ids.clone() } else { Vec::new() },
            "entrypoints": domain.entrypoints.iter().map(|entrypoint| json!({
                "name": entrypoint.name,
                "method": entrypoint.method,
                "path": entrypoint.path
            })).collect::<Vec<_>>(),
            "resources": domain.resources.iter().map(|resource| json!({
                "name": resource.name,
                "kind": resource.kind,
                "mode": resource.mode
            })).collect::<Vec<_>>()
        })
    }).collect::<Vec<_>>();

    let features = context.features.iter().map(|feature| {
        let semantic = names.features.get(&feature.id);
        json!({
            "featureId": feature.id,
            "domainIds": feature.domain_ids,
            "currentLabel": feature.current_label,
            "businessName": semantic.map(|item| item.name.clone()),
            "businessSummary": semantic.and_then(|item| item.summary.clone()),
            "visibility": feature.visibility,
            "tags": feature.tags,
            "symbols": feature.symbols,
            "entrypointIds": feature.entrypoint_ids,
            "resourceIds": feature.resource_ids,
            "flowIds": if matches!(stage, PromptStage::Flow) { feature.flow_ids.clone() } else { Vec::new() }
        })
    }).collect::<Vec<_>>();

    let flows = if matches!(stage, PromptStage::Flow) {
        context.flows.iter().map(|flow| json!({
            "flowId": flow.id,
            "domainIds": flow.domain_ids,
            "featureIds": flow.feature_ids,
            "ownerName": flow.owner_name,
            "required": flow.required,
            "dynamic": !flow.dynamic_boundary_ids.is_empty(),
            "selectionReason": flow.selection_reason,
            "steps": flow.steps.iter().map(|step| json!({ "kind": step.kind, "label": step.label })).collect::<Vec<_>>()
        })).collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    json!({
        "schemaVersion": context.schema_version,
        "chunkId": context.chunk_id,
        "projectId": context.project_id,
        "projectProfile": context.project_profile,
        "globalSummary": context.global_summary,
        "domains": domains,
        "features": features,
        "flows": flows
    })
}

fn valid_suggestion(
    name: &str,
    summary: &Option<String>,
    maximum_name_length: usize,
    maximum_summary_length: usize,
) -> bool {
    valid_text(name, maximum_name_length)
        && summary.as_ref().is_none_or(|summary| {
            valid_text(summary, maximum_summary_length) && !summary.contains(['\r', '\n'])
        })
}

fn valid_text(value: &str, maximum_length: usize) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && !value.contains(['\r', '\n'])
        && trimmed.chars().count() <= maximum_length
}

#[cfg(test)]
mod tests {
    use super::{
        build_missing_stage, build_stage, missing_items_for_stage, MissingItems, PromptLimits,
        PromptStage, SemanticDomainName, SemanticItemName, SemanticNames,
    };
    use crate::semantic::review::context::{
        ReviewContext, ReviewDomain, ReviewFeature, ReviewFlow, ReviewGlobalSummary,
        ReviewProjectProfile,
    };
    use crate::semantic::review::response::{DomainSuggestion, ReviewProposal};
    use serde_json::Value;

    fn context() -> ReviewContext {
        ReviewContext {
            schema_version: "context.v1".into(),
            chunk_id: "chunk-1".into(),
            source_analysis_id: "analysis-1".into(),
            source_schema_version: "analysis.v1".into(),
            project_id: "project-1".into(),
            project_profile: ReviewProjectProfile::default(),
            global_summary: ReviewGlobalSummary::default(),
            adjacent_domains: Vec::new(),
            domains: vec![ReviewDomain {
                domain_id: "domain-a".into(),
                source_domain_ids: Vec::new(),
                current_label: "auth".into(),
                role: Value::Null,
                signal: Value::Null,
                source_paths: Vec::new(),
                entrypoints: Vec::new(),
                resources: Vec::new(),
                feature_ids: vec!["feature-a".into()],
                flow_ids: vec!["flow-a".into()],
            }],
            features: vec![ReviewFeature {
                id: "feature-a".into(),
                domain_ids: vec!["domain-a".into()],
                current_label: "login".into(),
                visibility: Value::Null,
                required: true,
                tags: Vec::new(),
                symbols: Vec::new(),
                source_paths: Vec::new(),
                entrypoint_ids: Vec::new(),
                resource_ids: Vec::new(),
                flow_ids: vec!["flow-a".into()],
            }],
            flows: vec![ReviewFlow {
                id: "flow-a".into(),
                domain_ids: vec!["domain-a".into()],
                feature_ids: vec!["feature-a".into()],
                owner_unit_id: "unit-a".into(),
                owner_name: "Login".into(),
                required: true,
                steps: Vec::new(),
                dynamic_boundary_ids: Vec::new(),
                selection_reason: String::new(),
            }],
        }
    }

    #[test]
    #[allow(non_snake_case)]
    fn 응답에_없는_ID만_재요청_목록에_남긴다() {
        let proposal = ReviewProposal {
            domains: vec![DomainSuggestion {
                domain_id: "domain-a".into(),
                canonical_domain_id: None,
                name: "인증".into(),
                summary: None,
            }],
            ..ReviewProposal::default()
        };
        let limits = PromptLimits {
            maximum_name_length: 80,
            maximum_summary_length: 120,
        };
        let mut missing =
            missing_items_for_stage(&context(), &proposal, PromptStage::DomainFeature, limits);
        missing.flows =
            missing_items_for_stage(&context(), &proposal, PromptStage::Flow, limits).flows;
        assert_eq!(
            missing,
            MissingItems {
                domains: Default::default(),
                features: ["feature-a".into()].into_iter().collect(),
                flows: ["flow-a".into()].into_iter().collect(),
            }
        );
    }

    #[test]
    fn 재요청_컨텍스트는_누락된_항목만_포함한다() {
        let missing = MissingItems {
            features: ["feature-a".into()].into_iter().collect(),
            ..MissingItems::default()
        };
        let prompt = build_missing_stage(
            &context(),
            &missing,
            PromptStage::DomainFeature,
            &Default::default(),
            0,
            1,
            PromptLimits {
                maximum_name_length: 80,
                maximum_summary_length: 120,
            },
        )
        .unwrap();
        assert!(prompt.contains("feature-a"));
        assert!(!prompt.contains("domain-a\\\""));
    }

    #[test]
    fn 단계별_prompt는_담당하지_않는_배열을_비운다() {
        let domain_prompt = build_stage(
            &context(),
            PromptStage::DomainFeature,
            &SemanticNames::default(),
            0,
            1,
            PromptLimits {
                maximum_name_length: 80,
                maximum_summary_length: 120,
            },
        )
        .unwrap();
        assert!(domain_prompt.contains("\"features\":["));
        assert!(domain_prompt.contains("\"flows\":[]"));
        assert!(!domain_prompt.contains("flow-a"));

        let mut names = SemanticNames::default();
        names.domains.insert(
            "domain-a".into(),
            SemanticDomainName {
                canonical_domain_id: "domain-a".into(),
                name: "사용자 인증".into(),
                summary: Some("사용자 인증 기능".into()),
            },
        );
        names.features.insert(
            "feature-a".into(),
            SemanticItemName {
                name: "로그인".into(),
                summary: Some("사용자를 인증한다".into()),
            },
        );
        let flow_prompt = build_stage(
            &context(),
            PromptStage::Flow,
            &names,
            0,
            1,
            PromptLimits {
                maximum_name_length: 80,
                maximum_summary_length: 120,
            },
        )
        .unwrap();
        assert!(flow_prompt.contains("flow-a"));
        assert!(flow_prompt.contains("사용자 인증"));
        assert!(flow_prompt.contains("\"domains\":["));
        assert!(flow_prompt.contains("\"features\":["));
    }
}
