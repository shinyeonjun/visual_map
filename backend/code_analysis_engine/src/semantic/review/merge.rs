//! 청크별 의미 제안을 ID와 원본 순서 기준으로 병합한다.

use super::context::ReviewContext;
use super::response::{DomainSuggestion, FeatureSuggestion, FlowSuggestion, ReviewProposal};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticReviewResult {
    pub schema_version: &'static str,
    pub source_context: String,
    pub status: String,
    pub chunk_count: usize,
    pub completed_chunks: usize,
    pub failed_chunks: usize,
    pub retry_attempts: usize,
    pub domains: Vec<DomainResult>,
    pub features: Vec<FeatureResult>,
    pub flows: Vec<FlowResult>,
    pub warnings: Vec<ReviewWarning>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainResult {
    pub domain_id: String,
    pub current_name: String,
    pub name: String,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureResult {
    pub feature_id: String,
    pub current_name: String,
    pub name: String,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowResult {
    pub flow_id: String,
    pub current_name: String,
    pub name: String,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewWarning {
    pub code: String,
    pub item_id: Option<String>,
    pub message: String,
}

pub fn merge(
    contexts: &[ReviewContext],
    proposals: &[ReviewProposal],
    source_context: String,
    failed_chunks: usize,
    retry_attempts: usize,
    maximum_name_length: usize,
    maximum_summary_length: usize,
) -> SemanticReviewResult {
    let mut domain_order = Vec::new();
    let mut feature_order = Vec::new();
    let mut flow_order = Vec::new();
    let mut domain_current = BTreeMap::new();
    let mut feature_current = BTreeMap::new();
    let mut flow_current = BTreeMap::new();
    let mut known_domains = BTreeSet::new();
    let mut known_features = BTreeSet::new();
    let mut known_flows = BTreeSet::new();

    for context in contexts {
        for domain in &context.domains {
            push_once(&mut domain_order, &mut known_domains, &domain.domain_id);
            domain_current
                .entry(domain.domain_id.clone())
                .or_insert_with(|| domain.current_label.clone());
            for feature in &domain.features {
                push_once(&mut feature_order, &mut known_features, &feature.id);
                feature_current
                    .entry(feature.id.clone())
                    .or_insert_with(|| feature.current_label.clone());
            }
            for flow in &domain.flows {
                push_once(&mut flow_order, &mut known_flows, &flow.id);
                flow_current
                    .entry(flow.id.clone())
                    .or_insert_with(|| flow.owner_name.clone());
            }
        }
    }

    let mut domains = BTreeMap::new();
    let mut features = BTreeMap::new();
    let mut flows = BTreeMap::new();
    let mut warnings = Vec::new();
    for proposal in proposals {
        for suggestion in &proposal.domains {
            merge_suggestion(
                &mut domains,
                suggestion,
                &known_domains,
                maximum_name_length,
                maximum_summary_length,
                &mut warnings,
            );
        }
        for suggestion in &proposal.features {
            merge_suggestion(
                &mut features,
                suggestion,
                &known_features,
                maximum_name_length,
                maximum_summary_length,
                &mut warnings,
            );
        }
        for suggestion in &proposal.flows {
            merge_suggestion(
                &mut flows,
                suggestion,
                &known_flows,
                maximum_name_length,
                maximum_summary_length,
                &mut warnings,
            );
        }
    }

    let domains = domain_order
        .into_iter()
        .map(|id| result_for(id, &domain_current, &domains, &mut warnings, "domain"))
        .collect();
    let features = feature_order
        .into_iter()
        .map(|id| result_for(id, &feature_current, &features, &mut warnings, "feature"))
        .collect();
    let flows = flow_order
        .into_iter()
        .map(|id| result_for(id, &flow_current, &flows, &mut warnings, "flow"))
        .collect();

    let status = if proposals.is_empty() {
        "failed"
    } else if failed_chunks > 0 || !warnings.is_empty() {
        "partial"
    } else {
        "completed"
    };

    SemanticReviewResult {
        schema_version: "codex-semantic-review.v1",
        source_context,
        status: status.into(),
        chunk_count: contexts.len(),
        completed_chunks: proposals.len(),
        failed_chunks,
        retry_attempts,
        domains,
        features,
        flows,
        warnings,
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::merge;
    use crate::semantic::review::context::{
        ReviewContext, ReviewDomain, ReviewFeature, ReviewFlow, ReviewGlobalSummary,
        ReviewProjectProfile,
    };
    use crate::semantic::review::response::{DomainSuggestion, FeatureSuggestion, ReviewProposal};
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
    fn 원본_순서를_유지하고_누락된_항목은_원본이름을_사용한다() {
        let result = merge(
            &[context()],
            &[ReviewProposal {
                domains: vec![DomainSuggestion {
                    domain_id: "domain-a".into(),
                    name: "사용자 인증".into(),
                    summary: Some("로그인을 담당한다".into()),
                }],
                features: vec![FeatureSuggestion {
                    feature_id: "feature-a".into(),
                    name: "로그인".into(),
                    summary: None,
                }],
                flows: Vec::new(),
            }],
            "context.json".into(),
            0,
            0,
            120,
            500,
        );

        assert_eq!(result.domains[0].domain_id, "domain-a");
        assert_eq!(result.domains[0].name, "사용자 인증");
        assert_eq!(result.features[0].name, "로그인");
        assert_eq!(result.flows[0].name, "Login");
        assert_eq!(result.status, "partial");
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.item_id.as_deref() == Some("flow-a")));
    }

    #[test]
    #[allow(non_snake_case)]
    fn 입력에_없는_ID와_줄바꿈_설명은_결과에_들어가지_않는다() {
        let result = merge(
            &[context()],
            &[ReviewProposal {
                domains: vec![DomainSuggestion {
                    domain_id: "unknown".into(),
                    name: "가짜".into(),
                    summary: None,
                }],
                features: vec![FeatureSuggestion {
                    feature_id: "feature-a".into(),
                    name: "로그인".into(),
                    summary: Some("첫 줄\n둘째 줄".into()),
                }],
                flows: Vec::new(),
            }],
            "context.json".into(),
            0,
            0,
            120,
            500,
        );

        assert_eq!(result.features[0].name, "login");
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.code == "SEMANTIC_UNKNOWN_ID"));
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.code == "SEMANTIC_INVALID_TEXT"));
    }
}

fn result_for<T: Clone + IntoResult>(
    id: String,
    current: &BTreeMap<String, String>,
    suggestions: &BTreeMap<String, T>,
    warnings: &mut Vec<ReviewWarning>,
    kind: &str,
) -> T::Output {
    let current_name = current.get(&id).cloned().unwrap_or_else(|| id.clone());
    if let Some(suggestion) = suggestions.get(&id) {
        suggestion.clone().into_result(id, current_name)
    } else {
        warnings.push(ReviewWarning {
            code: "SEMANTIC_ITEM_UNREVIEWED".into(),
            item_id: Some(id.clone()),
            message: format!("{kind} 의미 분석 결과가 없어 원본 이름을 유지했습니다."),
        });
        T::Output::fallback(id, current_name)
    }
}

trait IntoResult: Clone {
    type Output: FallbackResult;
    fn into_result(self, id: String, current_name: String) -> Self::Output;
}

trait FallbackResult {
    fn fallback(id: String, current_name: String) -> Self;
}

impl IntoResult for DomainSuggestion {
    type Output = DomainResult;
    fn into_result(self, id: String, current_name: String) -> Self::Output {
        DomainResult {
            domain_id: id,
            current_name,
            name: self.name,
            summary: self.summary,
        }
    }
}
impl FallbackResult for DomainResult {
    fn fallback(id: String, current_name: String) -> Self {
        Self {
            domain_id: id,
            name: current_name.clone(),
            current_name,
            summary: None,
        }
    }
}
impl IntoResult for FeatureSuggestion {
    type Output = FeatureResult;
    fn into_result(self, id: String, current_name: String) -> Self::Output {
        FeatureResult {
            feature_id: id,
            current_name,
            name: self.name,
            summary: self.summary,
        }
    }
}
impl FallbackResult for FeatureResult {
    fn fallback(id: String, current_name: String) -> Self {
        Self {
            feature_id: id,
            name: current_name.clone(),
            current_name,
            summary: None,
        }
    }
}
impl IntoResult for FlowSuggestion {
    type Output = FlowResult;
    fn into_result(self, id: String, current_name: String) -> Self::Output {
        FlowResult {
            flow_id: id,
            current_name,
            name: self.name,
            summary: self.summary,
        }
    }
}
impl FallbackResult for FlowResult {
    fn fallback(id: String, current_name: String) -> Self {
        Self {
            flow_id: id,
            name: current_name.clone(),
            current_name,
            summary: None,
        }
    }
}

fn merge_suggestion<T: Suggestion>(
    destination: &mut BTreeMap<String, T>,
    suggestion: &T,
    known: &BTreeSet<String>,
    maximum_name_length: usize,
    maximum_summary_length: usize,
    warnings: &mut Vec<ReviewWarning>,
) {
    let id = suggestion.id().to_string();
    if !known.contains(&id) {
        warnings.push(ReviewWarning {
            code: "SEMANTIC_UNKNOWN_ID".into(),
            item_id: Some(id),
            message: "입력 context에 없는 ID를 무시했습니다.".into(),
        });
        return;
    }
    if !valid_text(suggestion.name(), maximum_name_length)
        || suggestion.name().contains(['\r', '\n'])
        || suggestion.summary().as_ref().is_some_and(|value| {
            !valid_text(value, maximum_summary_length) || value.contains(['\r', '\n'])
        })
    {
        warnings.push(ReviewWarning {
            code: "SEMANTIC_INVALID_TEXT".into(),
            item_id: Some(id),
            message: "이름 또는 한 줄 설명 형식이 잘못되어 무시했습니다.".into(),
        });
        return;
    }
    destination.entry(id).or_insert_with(|| suggestion.clone());
}

trait Suggestion: Clone {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn summary(&self) -> &Option<String>;
}
impl Suggestion for DomainSuggestion {
    fn id(&self) -> &str {
        &self.domain_id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn summary(&self) -> &Option<String> {
        &self.summary
    }
}
impl Suggestion for FeatureSuggestion {
    fn id(&self) -> &str {
        &self.feature_id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn summary(&self) -> &Option<String> {
        &self.summary
    }
}
impl Suggestion for FlowSuggestion {
    fn id(&self) -> &str {
        &self.flow_id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn summary(&self) -> &Option<String> {
        &self.summary
    }
}

fn valid_text(value: &str, maximum_length: usize) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && trimmed.chars().count() <= maximum_length
}

fn push_once(order: &mut Vec<String>, known: &mut BTreeSet<String>, id: &str) {
    if known.insert(id.to_string()) {
        order.push(id.to_string());
    }
}
