//! 도메인 이름 의미 분석 프롬프트를 만든다.

use super::context::ReviewContext;
use super::response::ReviewProposal;
use serde_json::{json, Value};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy)]
pub struct PromptLimits {
    pub maximum_name_length: usize,
    pub maximum_summary_length: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MissingDomainIds {
    pub domains: BTreeSet<String>,
}

impl MissingDomainIds {
    pub fn is_empty(&self) -> bool {
        self.domains.is_empty()
    }
}

pub fn build_prompt(
    context: &ReviewContext,
    chunk_index: usize,
    chunk_count: usize,
    limits: PromptLimits,
) -> Result<String, serde_json::Error> {
    let context_json = domain_name_context(context);
    Ok(include_str!("../prompts/semantic_review.txt")
        .replace("{chunk_number}", &(chunk_index + 1).to_string())
        .replace("{chunk_count}", &chunk_count.to_string())
        .replace(
            "{maximum_name_length}",
            &limits.maximum_name_length.to_string(),
        )
        .replace("{context_json}", &context_json.to_string()))
}

pub fn missing_domain_ids(
    context: &ReviewContext,
    proposal: &ReviewProposal,
    limits: PromptLimits,
) -> MissingDomainIds {
    let domain_ids = proposal
        .domains
        .iter()
        .filter(|item| {
            valid_suggestion(
                &item.name,
                &item.summary,
                limits.maximum_name_length,
                limits.maximum_summary_length,
            ) || item.is_demoted()
        })
        .map(|item| item.domain_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut missing = MissingDomainIds::default();
    for domain in &context.domains {
        if !domain_ids.contains(domain.domain_id.as_str()) {
            missing.domains.insert(domain.domain_id.clone());
        }
    }
    missing
}

pub fn build_missing_prompt(
    context: &ReviewContext,
    missing: &MissingDomainIds,
    chunk_index: usize,
    chunk_count: usize,
    limits: PromptLimits,
) -> Result<String, serde_json::Error> {
    let narrowed = context_with_missing_domains(context, missing);
    build_prompt(&narrowed, chunk_index, chunk_count, limits)
}

fn context_with_missing_domains(
    context: &ReviewContext,
    missing: &MissingDomainIds,
) -> ReviewContext {
    let mut narrowed = context.clone();
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
    narrowed
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
        "package": domain.current_label,
        "contractKey": domain.current_label,
        "packets": unique_limited(domain.packets.iter().cloned(), MAX_DOMAIN_HINTS),
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

const MAX_DOMAIN_HINTS: usize = 8;

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

const MAX_FEATURE_HINTS: usize = 6;
const MAX_FEATURES_PER_PROMPT: usize = 24;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MissingFeatureIds {
    pub features: BTreeSet<String>,
}

impl MissingFeatureIds {
    pub fn is_empty(&self) -> bool {
        self.features.is_empty()
    }
}

pub fn build_feature_prompt(
    domain_name: &str,
    features: &[crate::semantic::review::context::ReviewFeature],
    limits: PromptLimits,
) -> Result<String, serde_json::Error> {
    let selected: Vec<_> = features.iter().take(MAX_FEATURES_PER_PROMPT).collect();
    let context_json = json!({
        "features": selected.iter().map(|feature| json!({
            "featureId": feature.id,
            "currentLabel": feature.current_label,
            "symbols": unique_limited(feature.symbols.iter().cloned(), MAX_FEATURE_HINTS),
            "sourcePaths": unique_limited(feature.source_paths.iter().cloned(), MAX_FEATURE_HINTS),
        })).collect::<Vec<_>>()
    });
    Ok(include_str!("../prompts/semantic_feature_review.txt")
        .replace("{domain_name}", domain_name)
        .replace(
            "{maximum_name_length}",
            &limits.maximum_name_length.to_string(),
        )
        .replace("{context_json}", &context_json.to_string()))
}

pub fn missing_feature_ids(
    features: &[crate::semantic::review::context::ReviewFeature],
    proposal: &ReviewProposal,
    limits: PromptLimits,
) -> MissingFeatureIds {
    let known = proposal
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
    let mut missing = MissingFeatureIds::default();
    for feature in features.iter().take(MAX_FEATURES_PER_PROMPT) {
        if !known.contains(feature.id.as_str()) {
            missing.features.insert(feature.id.clone());
        }
    }
    missing
}

pub fn build_missing_feature_prompt(
    domain_name: &str,
    features: &[crate::semantic::review::context::ReviewFeature],
    missing: &MissingFeatureIds,
    limits: PromptLimits,
) -> Result<String, serde_json::Error> {
    let narrowed: Vec<_> = features
        .iter()
        .filter(|feature| missing.features.contains(&feature.id))
        .cloned()
        .collect();
    build_feature_prompt(domain_name, &narrowed, limits)
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
        build_missing_prompt, build_prompt, missing_domain_ids, MissingDomainIds, PromptLimits,
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
            domains: vec![
                ReviewDomain {
                    domain_id: "domain-a".into(),
                    source_domain_ids: Vec::new(),
                    current_label: "auth".into(),
                    role: Value::Null,
                    signal: Value::Null,
                    source_paths: Vec::new(),
                    entrypoints: Vec::new(),
                    resources: Vec::new(),
                    packets: Vec::new(),
                    feature_ids: vec!["feature-a".into()],
                    flow_ids: vec!["flow-a".into()],
                },
                ReviewDomain {
                    domain_id: "domain-b".into(),
                    source_domain_ids: Vec::new(),
                    current_label: "billing".into(),
                    role: Value::Null,
                    signal: Value::Null,
                    source_paths: Vec::new(),
                    entrypoints: Vec::new(),
                    resources: Vec::new(),
                    packets: Vec::new(),
                    feature_ids: Vec::new(),
                    flow_ids: Vec::new(),
                },
            ],
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
    fn 응답에_없는_도메인만_재요청_목록에_남긴다() {
        let proposal = ReviewProposal {
            features: Vec::new(),
            domains: vec![DomainSuggestion {
                domain_id: "domain-a".into(),
                canonical_domain_id: None,
                action: None,
                name: "인증".into(),
                summary: None,
            }],
            ..ReviewProposal::default()
        };
        let limits = PromptLimits {
            maximum_name_length: 80,
            maximum_summary_length: 120,
        };
        let missing = missing_domain_ids(&context(), &proposal, limits);
        assert_eq!(
            missing,
            MissingDomainIds {
                domains: ["domain-b".into()].into_iter().collect(),
            }
        );
    }

    #[test]
    fn 재요청_컨텍스트는_누락된_도메인만_포함한다() {
        let missing = MissingDomainIds {
            domains: ["domain-b".into()].into_iter().collect(),
        };
        let prompt = build_missing_prompt(
            &context(),
            &missing,
            0,
            1,
            PromptLimits {
                maximum_name_length: 80,
                maximum_summary_length: 120,
            },
        )
        .unwrap();
        assert!(prompt.contains("domain-b"));
        assert!(!prompt.contains("domain-a"));
        assert!(!prompt.contains("feature-a"));
        assert!(!prompt.contains("flow-a"));
    }

    #[test]
    fn 도메인_prompt는_이름_재료만_포함하고_기능_흐름은_보내지_않는다() {
        let domain_prompt = build_prompt(
            &context(),
            0,
            1,
            PromptLimits {
                maximum_name_length: 80,
                maximum_summary_length: 120,
            },
        )
        .unwrap();
        assert!(domain_prompt.contains("\"domains\":["));
        assert!(domain_prompt.contains("contractKey"));
        assert!(domain_prompt.contains("package"));
        assert!(!domain_prompt.contains("feature-a"));
        assert!(!domain_prompt.contains("flow-a"));
        assert!(!domain_prompt.contains("\"features\""));
        assert!(!domain_prompt.contains("\"flows\""));
        assert!(!domain_prompt.contains("projectProfile"));
        assert!(!domain_prompt.contains("globalSummary"));
    }

    #[test]
    fn 기능_prompt는_기능_id만_보내고_흐름은_보내지_않는다() {
        let feature_prompt = super::build_feature_prompt(
            "인증",
            &context().features,
            PromptLimits {
                maximum_name_length: 80,
                maximum_summary_length: 120,
            },
        )
        .unwrap();
        assert!(feature_prompt.contains("feature-a"));
        assert!(feature_prompt.contains("\"features\":["));
        assert!(!feature_prompt.contains("flow-a"));
        assert!(!feature_prompt.contains("domain-a"));
    }
}
