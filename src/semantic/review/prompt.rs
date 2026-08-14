//! 도메인·기능·실행 흐름 의미 분석 프롬프트를 만든다.

use super::context::ReviewContext;
use super::response::ReviewProposal;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptStage {
    Domain,
    Feature,
    Flow,
}

#[derive(Debug, Clone, Default)]
pub struct SemanticNames {
    pub domains: BTreeMap<String, SemanticDomainName>,
}

#[derive(Debug, Clone, Default)]
pub struct SemanticDomainName {
    pub canonical_domain_id: String,
    pub name: String,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct PromptLimits {
    pub maximum_name_length: usize,
    pub maximum_summary_length: usize,
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
    Ok(include_str!("../prompts/semantic_review.txt")
        .replace("{chunk_number}", &(chunk_index + 1).to_string())
        .replace("{chunk_count}", &chunk_count.to_string())
        .replace(
            "{maximum_name_length}",
            &limits.maximum_name_length.to_string(),
        )
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
        PromptStage::Domain => {
            for domain in &context.domains {
                if !domain_ids.contains(domain.domain_id.as_str()) {
                    missing.domains.insert(domain.domain_id.clone());
                }
            }
        }
        PromptStage::Feature => {
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
        PromptStage::Domain => {
            narrowed.domains = context
                .domains
                .iter()
                .filter(|domain| missing.domains.contains(&domain.domain_id))
                .map(|domain| {
                    let mut domain = domain.clone();
                    domain.feature_ids.clear();
                    domain.flow_ids.clear();
                    domain
                })
                .collect();
            narrowed.features.clear();
            narrowed.flows.clear();
        }
        PromptStage::Feature => {
            narrowed.domains = context
                .domains
                .iter()
                .filter_map(|domain| {
                    let mut domain = domain.clone();
                    domain
                        .feature_ids
                        .retain(|id| missing.features.contains(id));
                    domain.flow_ids.clear();
                    if !domain.feature_ids.is_empty() {
                        Some(domain)
                    } else {
                        None
                    }
                })
                .collect();
            narrowed
                .features
                .retain(|feature| missing.features.contains(&feature.id));
            for feature in &mut narrowed.features {
                feature.flow_ids.clear();
            }
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

const MAX_DOMAIN_HINTS: usize = 8;

fn stage_context(context: &ReviewContext, stage: PromptStage, names: &SemanticNames) -> Value {
    if matches!(stage, PromptStage::Domain) {
        return domain_name_context(context);
    }

    let include_features = matches!(stage, PromptStage::Feature | PromptStage::Flow);
    let include_flows = matches!(stage, PromptStage::Flow);
    let domains = context
        .domains
        .iter()
        .map(|domain| {
            let semantic = names.domains.get(&domain.domain_id);
            json!({
                "domainId": domain.domain_id,
                "currentLabel": domain.current_label,
                "businessDomainId": semantic.map(|item| item.canonical_domain_id.clone()),
                "businessName": semantic.map(|item| item.name.clone()),
                "featureIds": if include_features { domain.feature_ids.clone() } else { Vec::new() },
                "flowIds": if include_flows { domain.flow_ids.clone() } else { Vec::new() }
            })
        })
        .collect::<Vec<_>>();
    let features = if include_features {
        context
            .features
            .iter()
            .map(|feature| {
                json!({
                    "featureId": feature.id,
                    "domainIds": feature.domain_ids,
                    "currentLabel": feature.current_label,
                    "flowIds": if include_flows { feature.flow_ids.clone() } else { Vec::new() }
                })
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let flows = if include_flows {
        context
            .flows
            .iter()
            .map(|flow| {
                json!({
                    "flowId": flow.id,
                    "ownerName": flow.owner_name,
                    "steps": flow.steps.iter().map(|step| json!({ "label": step.label })).collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    json!({
        "domains": domains,
        "features": features,
        "flows": flows
    })
}

fn domain_name_context(context: &ReviewContext) -> Value {
    json!({
        "languages": context.global_summary.language_keys,
        "domains": context.domains.iter().map(domain_name_item).collect::<Vec<_>>()
    })
}

fn domain_name_item(domain: &crate::semantic::review::context::ReviewDomain) -> Value {
    json!({
        "domainId": domain.domain_id,
        "currentLabel": domain.current_label,
        "paths": unique_limited(
            domain.source_paths.iter().map(|path| directory_hint(path)),
            MAX_DOMAIN_HINTS
        ),
        "entrypoints": unique_limited(
            domain.entrypoints.iter().map(entrypoint_hint),
            MAX_DOMAIN_HINTS
        ),
        "resources": unique_limited(
            domain.resources.iter().map(|resource| resource.name.clone()),
            MAX_DOMAIN_HINTS
        )
    })
}

fn directory_hint(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    normalized
        .rsplit_once('/')
        .map(|(directory, _)| directory.to_string())
        .unwrap_or(normalized)
}

fn entrypoint_hint(entrypoint: &crate::semantic::review::context::ReviewEntrypoint) -> String {
    match (&entrypoint.method, &entrypoint.path) {
        (Some(method), Some(path)) => format!("{method} {path}"),
        (_, Some(path)) => path.clone(),
        _ => entrypoint.name.clone(),
    }
}

fn unique_limited(values: impl IntoIterator<Item = String>, limit: usize) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() || !seen.insert(trimmed.to_string()) {
            continue;
        }
        result.push(trimmed.to_string());
        if result.len() >= limit {
            break;
        }
    }
    result
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
        PromptStage, SemanticDomainName, SemanticNames,
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
            missing_items_for_stage(&context(), &proposal, PromptStage::Domain, limits);
        missing.features =
            missing_items_for_stage(&context(), &proposal, PromptStage::Feature, limits).features;
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
            PromptStage::Feature,
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
        assert!(!prompt.contains("flow-a"));
    }

    #[test]
    fn 도메인_prompt는_이름_재료만_포함하고_기능_흐름은_보내지_않는다() {
        let domain_prompt = build_stage(
            &context(),
            PromptStage::Domain,
            &SemanticNames::default(),
            0,
            1,
            PromptLimits {
                maximum_name_length: 80,
                maximum_summary_length: 120,
            },
        )
        .unwrap();
        assert!(domain_prompt.contains("\"domains\":["));
        assert!(domain_prompt.contains("currentLabel"));
        assert!(!domain_prompt.contains("feature-a"));
        assert!(!domain_prompt.contains("flow-a"));
        assert!(!domain_prompt.contains("\"features\""));
        assert!(!domain_prompt.contains("\"flows\""));
        assert!(!domain_prompt.contains("projectProfile"));
        assert!(!domain_prompt.contains("globalSummary"));

        let mut names = SemanticNames::default();
        names.domains.insert(
            "domain-a".into(),
            SemanticDomainName {
                canonical_domain_id: "domain-a".into(),
                name: "사용자 인증".into(),
                summary: Some("사용자 인증 기능".into()),
            },
        );
        let feature_prompt = build_stage(
            &context(),
            PromptStage::Feature,
            &names,
            0,
            1,
            PromptLimits {
                maximum_name_length: 80,
                maximum_summary_length: 120,
            },
        )
        .unwrap();
        assert!(feature_prompt.contains("feature-a"));
        assert!(feature_prompt.contains("사용자 인증"));
        assert!(feature_prompt.contains("\"flows\":[]"));
        assert!(!feature_prompt.contains("flow-a"));
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
