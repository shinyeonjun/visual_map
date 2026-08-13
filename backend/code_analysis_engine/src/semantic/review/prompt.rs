//! 도메인·기능·실행 흐름 의미 분석 프롬프트를 만든다.

use super::context::ReviewContext;
use super::response::ReviewProposal;
use std::collections::BTreeSet;

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

pub fn build(
    context: &ReviewContext,
    chunk_index: usize,
    chunk_count: usize,
    maximum_name_length: usize,
    maximum_summary_length: usize,
) -> Result<String, serde_json::Error> {
    build_for_context(
        context,
        chunk_index,
        chunk_count,
        maximum_name_length,
        maximum_summary_length,
    )
}

pub fn build_missing(
    context: &ReviewContext,
    missing: &MissingItems,
    chunk_index: usize,
    chunk_count: usize,
    maximum_name_length: usize,
    maximum_summary_length: usize,
) -> Result<String, serde_json::Error> {
    let narrowed = context_with_missing_items(context, missing);
    build_for_context(
        &narrowed,
        chunk_index,
        chunk_count,
        maximum_name_length,
        maximum_summary_length,
    )
}

pub fn missing_items(
    context: &ReviewContext,
    proposal: &ReviewProposal,
    maximum_name_length: usize,
    maximum_summary_length: usize,
) -> MissingItems {
    let domain_ids = proposal
        .domains
        .iter()
        .filter(|item| {
            valid_suggestion(
                &item.name,
                &item.summary,
                maximum_name_length,
                maximum_summary_length,
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
                maximum_name_length,
                maximum_summary_length,
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
                maximum_name_length,
                maximum_summary_length,
            )
        })
        .map(|item| item.flow_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut missing = MissingItems::default();
    for domain in &context.domains {
        if !domain_ids.contains(domain.domain_id.as_str()) {
            missing.domains.insert(domain.domain_id.clone());
        }
        for feature in &domain.features {
            if !feature_ids.contains(feature.id.as_str()) {
                missing.features.insert(feature.id.clone());
            }
        }
        for flow in &domain.flows {
            if !flow_ids.contains(flow.id.as_str()) {
                missing.flows.insert(flow.id.clone());
            }
        }
    }
    missing
}

fn build_for_context(
    context: &ReviewContext,
    chunk_index: usize,
    chunk_count: usize,
    maximum_name_length: usize,
    maximum_summary_length: usize,
) -> Result<String, serde_json::Error> {
    let context_json = serde_json::to_string(context)?;
    Ok(include_str!("../prompts/semantic_review.txt")
        .replace("{chunk_number}", &(chunk_index + 1).to_string())
        .replace("{chunk_count}", &chunk_count.to_string())
        .replace("{maximum_name_length}", &maximum_name_length.to_string())
        .replace(
            "{maximum_summary_length}",
            &maximum_summary_length.to_string(),
        )
        .replace("{context_json}", &context_json))
}

fn context_with_missing_items(context: &ReviewContext, missing: &MissingItems) -> ReviewContext {
    let mut narrowed = context.clone();
    narrowed.domains = context
        .domains
        .iter()
        .filter_map(|domain| {
            let domain_requested = missing.domains.contains(&domain.domain_id);
            let mut narrowed_domain = domain.clone();
            narrowed_domain
                .features
                .retain(|feature| missing.features.contains(&feature.id));
            narrowed_domain
                .flows
                .retain(|flow| missing.flows.contains(&flow.id));
            if domain_requested
                || !narrowed_domain.features.is_empty()
                || !narrowed_domain.flows.is_empty()
            {
                Some(narrowed_domain)
            } else {
                None
            }
        })
        .collect();
    narrowed
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
    use super::{build_missing, missing_items, MissingItems};
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
                features: vec![ReviewFeature {
                    id: "feature-a".into(),
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
                    feature_ids: vec!["feature-a".into()],
                    owner_unit_id: "unit-a".into(),
                    owner_name: "Login".into(),
                    required: true,
                    steps: Vec::new(),
                    edges: Vec::new(),
                    dynamic_boundary_ids: Vec::new(),
                    selection_reason: String::new(),
                }],
            }],
        }
    }

    #[test]
    #[allow(non_snake_case)]
    fn 응답에_없는_ID만_재요청_목록에_남긴다() {
        let missing = missing_items(
            &context(),
            &ReviewProposal {
                domains: vec![DomainSuggestion {
                    domain_id: "domain-a".into(),
                    name: "인증".into(),
                    summary: None,
                }],
                ..ReviewProposal::default()
            },
            80,
            120,
        );
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
        let prompt = build_missing(&context(), &missing, 0, 1, 80, 120).unwrap();
        assert!(prompt.contains("feature-a"));
        assert!(!prompt.contains("domain-a\\\""));
    }
}
