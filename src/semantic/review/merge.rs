//! 청크별 의미 제안을 ID와 원본 순서 기준으로 병합한다.

use super::context::ReviewContext;
use super::response::{DomainSuggestion, ReviewProposal};
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
    pub domain_completed_chunks: usize,
    pub domains: Vec<DomainResult>,
    pub features: Vec<FeatureResult>,
    pub flows: Vec<FlowResult>,
    pub warnings: Vec<ReviewWarning>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainResult {
    pub domain_id: String,
    pub source_domain_ids: Vec<String>,
    pub canonical_domain_id: String,
    pub current_name: String,
    pub name: String,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureResult {
    pub feature_id: String,
    pub domain_ids: Vec<String>,
    pub current_name: String,
    pub name: String,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowResult {
    pub flow_id: String,
    pub domain_ids: Vec<String>,
    pub feature_ids: Vec<String>,
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
        }
        for feature in &context.features {
            push_once(&mut feature_order, &mut known_features, &feature.id);
            feature_current
                .entry(feature.id.clone())
                .or_insert_with(|| feature.current_label.clone());
        }
        for flow in &context.flows {
            push_once(&mut flow_order, &mut known_flows, &flow.id);
            flow_current
                .entry(flow.id.clone())
                .or_insert_with(|| flow.owner_name.clone());
        }
    }

    let mut domain_suggestions = BTreeMap::new();
    let mut warnings = Vec::new();
    for proposal in proposals {
        for suggestion in &proposal.domains {
            merge_suggestion(
                &mut domain_suggestions,
                suggestion,
                &known_domains,
                maximum_name_length,
                maximum_summary_length,
                &mut warnings,
            );
        }
    }

    reject_contract_demotes(contexts, &mut domain_suggestions, &mut warnings);

    let demoted_domains = domain_suggestions
        .iter()
        .filter(|(_, suggestion)| suggestion.is_demoted())
        .map(|(domain_id, _)| domain_id.clone())
        .collect::<BTreeSet<_>>();
    let canonical_by_source = canonical_domain_ids(
        &domain_order,
        &known_domains,
        &domain_suggestions,
        &mut warnings,
    );
    let mut canonical_sources = BTreeMap::<String, Vec<String>>::new();
    for source_id in &domain_order {
        if demoted_domains.contains(source_id) {
            continue;
        }
        canonical_sources
            .entry(
                canonical_by_source
                    .get(source_id)
                    .cloned()
                    .unwrap_or_else(|| source_id.clone()),
            )
            .or_default()
            .push(source_id.clone());
    }
    let domains = canonical_sources
        .into_iter()
        .map(|(canonical_id, source_ids)| {
            let representative_id = source_ids
                .iter()
                .find(|id| *id == &canonical_id)
                .cloned()
                .unwrap_or_else(|| source_ids[0].clone());
            let current_name = domain_current
                .get(&representative_id)
                .cloned()
                .unwrap_or_else(|| representative_id.clone());
            let suggestion = domain_suggestions
                .get(&representative_id)
                .or_else(|| source_ids.iter().find_map(|id| domain_suggestions.get(id)));
            DomainResult {
                domain_id: canonical_id.clone(),
                source_domain_ids: source_ids,
                canonical_domain_id: canonical_id,
                current_name: current_name.clone(),
                name: suggestion
                    .map(|suggestion| suggestion.name.clone())
                    .unwrap_or_else(|| current_name.clone()),
                summary: suggestion.and_then(|suggestion| suggestion.summary.clone()),
            }
        })
        .collect::<Vec<_>>();
    let feature_domain_ids = feature_domain_ids(contexts, &canonical_by_source);
    let flow_relations = flow_relations(contexts, &canonical_by_source);
    let features = feature_order
        .into_iter()
        .map(|id| static_feature_result(
            id,
            &feature_current,
            &feature_domain_ids,
        ))
        .collect();
    let flows = flow_order
        .into_iter()
        .map(|id| static_flow_result(id, &flow_current, &flow_relations))
        .collect();

    let status = if proposals.is_empty() {
        "failed"
    } else if failed_chunks > 0 || !warnings.is_empty() {
        "partial"
    } else {
        "completed"
    };

    SemanticReviewResult {
        schema_version: "ai-semantic-review.v2",
        source_context,
        status: status.into(),
        chunk_count: contexts.len(),
        completed_chunks: proposals.len(),
        failed_chunks,
        retry_attempts,
        domain_completed_chunks: proposals.len(),
        domains,
        features,
        flows,
        warnings,
    }
}

fn reject_contract_demotes(
    contexts: &[ReviewContext],
    suggestions: &mut BTreeMap<String, DomainSuggestion>,
    warnings: &mut Vec<ReviewWarning>,
) {
    let mut records = BTreeMap::new();
    for context in contexts {
        for domain in &context.domains {
            records.entry(domain.domain_id.clone()).or_insert(domain);
        }
    }
    for (domain_id, suggestion) in suggestions.iter_mut() {
        if !suggestion.is_demoted() {
            continue;
        }
        let Some(domain) = records.get(domain_id) else {
            continue;
        };
        if domain.entrypoints.is_empty()
            && domain.resources.is_empty()
            && domain.feature_ids.is_empty()
        {
            continue;
        }
        suggestion.action = Some("keep".into());
        warnings.push(ReviewWarning {
            code: "SEMANTIC_DEMOTE_REJECTED".into(),
            item_id: Some(domain_id.clone()),
            message: "계약 근거가 있는 도메인의 demote를 keep으로 바꿨습니다.".into(),
        });
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::merge;
    use crate::semantic::review::context::{
        ReviewContext, ReviewDomain, ReviewEntrypoint, ReviewFeature, ReviewFlow,
        ReviewGlobalSummary, ReviewProjectProfile,
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
    fn 원본_순서를_유지하고_누락된_항목은_원본이름을_사용한다() {
        let result = merge(
            &[context()],
            &[ReviewProposal {
                domains: vec![DomainSuggestion {
                    domain_id: "domain-a".into(),
                    canonical_domain_id: None,
                    action: None,
                    name: "사용자 인증".into(),
                    summary: Some("로그인을 담당한다".into()),
                }],
            }],
            "context.json".into(),
            0,
            0,
            120,
            500,
        );

        assert_eq!(result.domains[0].domain_id, "domain-a");
        assert_eq!(result.domains[0].name, "사용자 인증");
        assert_eq!(result.features[0].name, "login");
        assert_eq!(result.flows[0].name, "Login");
        assert_eq!(result.status, "completed");
        assert!(result.warnings.is_empty());
    }

    #[test]
    #[allow(non_snake_case)]
    fn 입력에_없는_ID와_줄바꿈_설명은_결과에_들어가지_않는다() {
        let result = merge(
            &[context()],
            &[ReviewProposal {
                domains: vec![
                    DomainSuggestion {
                        domain_id: "unknown".into(),
                        canonical_domain_id: None,
                        action: None,
                        name: "가짜".into(),
                        summary: None,
                    },
                    DomainSuggestion {
                        domain_id: "domain-a".into(),
                        canonical_domain_id: None,
                        action: None,
                        name: "인증".into(),
                        summary: Some("첫 줄\n둘째 줄".into()),
                    },
                ],
            }],
            "context.json".into(),
            0,
            0,
            120,
            500,
        );

        assert_eq!(result.features[0].name, "login");
        assert_eq!(result.features[0].summary, None);
        assert_eq!(result.domains[0].name, "auth");
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.code == "SEMANTIC_UNKNOWN_ID"));
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.code == "SEMANTIC_INVALID_TEXT"));
    }

    #[test]
    fn canonical_domain_id는_원본_도메인을_하나로_묶고_관계를_보존한다() {
        let first = context();
        let mut second = context();
        second.domains[0].domain_id = "domain-b".into();
        second.domains[0].feature_ids = vec!["feature-b".into()];
        second.domains[0].flow_ids = vec!["flow-b".into()];
        second.features[0].id = "feature-b".into();
        second.features[0].domain_ids = vec!["domain-b".into()];
        second.features[0].flow_ids = vec!["flow-b".into()];
        second.flows[0].id = "flow-b".into();
        second.flows[0].domain_ids = vec!["domain-b".into()];
        second.flows[0].feature_ids = vec!["feature-b".into()];

        let result = merge(
            &[first, second],
            &[ReviewProposal {
                domains: vec![
                    DomainSuggestion {
                        domain_id: "domain-a".into(),
                        canonical_domain_id: Some("domain-b".into()),
                        action: None,
                        name: "인증과 세션".into(),
                        summary: Some("사용자 인증과 세션을 담당한다".into()),
                    },
                    DomainSuggestion {
                        domain_id: "domain-b".into(),
                        canonical_domain_id: Some("domain-b".into()),
                        action: None,
                        name: "인증과 세션".into(),
                        summary: Some("사용자 인증과 세션을 담당한다".into()),
                    },
                ],
            }],
            "context.json".into(),
            0,
            0,
            120,
            500,
        );

        assert_eq!(result.domains.len(), 1);
        assert_eq!(result.domains[0].domain_id, "domain-b");
        assert_eq!(
            result.domains[0].source_domain_ids,
            vec!["domain-a", "domain-b"]
        );
        assert_eq!(result.features[0].domain_ids, vec!["domain-b"]);
        assert_eq!(result.flows[0].domain_ids, vec!["domain-b"]);
    }

    #[test]
    fn demote는_빈_도메인만_결과에서_제거한다() {
        let mut empty = context();
        empty.domains[0].feature_ids.clear();
        empty.domains[0].flow_ids.clear();
        empty.features.clear();
        empty.flows.clear();
        let result = merge(
            &[empty],
            &[ReviewProposal {
                domains: vec![DomainSuggestion {
                    domain_id: "domain-a".into(),
                    canonical_domain_id: None,
                    action: Some("demote".into()),
                    name: "배포 표면".into(),
                    summary: None,
                }],
            }],
            "context.json".into(),
            0,
            0,
            120,
            500,
        );

        assert!(result.domains.is_empty());
        assert!(result.features.is_empty());
    }

    #[test]
    fn 계약이_있는_도메인_demote는_keep으로_되돌린다() {
        let mut with_contract = context();
        with_contract.domains[0].entrypoints.push(ReviewEntrypoint {
            id: "entry-a".into(),
            unit_id: "unit-a".into(),
            kind: Value::Null,
            name: "GET /threads".into(),
            method: Some("GET".into()),
            path: Some("/threads".into()),
        });
        let result = merge(
            &[with_contract],
            &[ReviewProposal {
                domains: vec![DomainSuggestion {
                    domain_id: "domain-a".into(),
                    canonical_domain_id: None,
                    action: Some("demote".into()),
                    name: "배포 표면".into(),
                    summary: None,
                }],
            }],
            "context.json".into(),
            0,
            0,
            120,
            500,
        );

        assert_eq!(result.domains.len(), 1);
        assert_eq!(result.domains[0].domain_id, "domain-a");
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.code == "SEMANTIC_DEMOTE_REJECTED"));
    }
}

fn static_feature_result(
    id: String,
    current: &BTreeMap<String, String>,
    feature_domain_ids: &BTreeMap<String, Vec<String>>,
) -> FeatureResult {
    let current_name = current.get(&id).cloned().unwrap_or_else(|| id.clone());
    FeatureResult {
        feature_id: id.clone(),
        domain_ids: feature_domain_ids.get(&id).cloned().unwrap_or_default(),
        current_name: current_name.clone(),
        name: current_name,
        summary: None,
    }
}

fn static_flow_result(
    id: String,
    current: &BTreeMap<String, String>,
    flow_relations: &BTreeMap<String, (Vec<String>, Vec<String>)>,
) -> FlowResult {
    let current_name = current.get(&id).cloned().unwrap_or_else(|| id.clone());
    let (domain_ids, feature_ids) = flow_relations.get(&id).cloned().unwrap_or_default();
    FlowResult {
        flow_id: id,
        domain_ids,
        feature_ids,
        current_name: current_name.clone(),
        name: current_name,
        summary: None,
    }
}

fn canonical_domain_ids(
    domain_order: &[String],
    known_domains: &BTreeSet<String>,
    suggestions: &BTreeMap<String, DomainSuggestion>,
    warnings: &mut Vec<ReviewWarning>,
) -> BTreeMap<String, String> {
    let mut requested = BTreeMap::new();
    for domain_id in domain_order {
        let candidate = suggestions
            .get(domain_id)
            .and_then(|suggestion| suggestion.canonical_domain_id.clone())
            .unwrap_or_else(|| domain_id.clone());
        if !known_domains.contains(&candidate) {
            warnings.push(ReviewWarning {
                code: "SEMANTIC_UNKNOWN_CANONICAL_DOMAIN".into(),
                item_id: Some(domain_id.clone()),
                message: "존재하지 않는 canonical domain ID를 무시하고 원본 도메인을 유지했습니다."
                    .into(),
            });
            requested.insert(domain_id.clone(), domain_id.clone());
        } else {
            requested.insert(domain_id.clone(), candidate);
        }
    }
    domain_order
        .iter()
        .map(|domain_id| {
            (
                domain_id.clone(),
                resolve_canonical_domain(domain_id, &requested),
            )
        })
        .collect()
}

fn resolve_canonical_domain(id: &str, requested: &BTreeMap<String, String>) -> String {
    let mut path = Vec::new();
    let mut current = id.to_string();
    loop {
        if let Some(cycle_start) = path.iter().position(|candidate| candidate == &current) {
            return path[cycle_start..].iter().min().cloned().unwrap_or(current);
        }
        path.push(current.clone());
        let Some(next) = requested.get(&current) else {
            return current;
        };
        if next == &current {
            return current;
        }
        current = next.clone();
    }
}

fn feature_domain_ids(
    contexts: &[ReviewContext],
    canonical_by_source: &BTreeMap<String, String>,
) -> BTreeMap<String, Vec<String>> {
    let mut result = BTreeMap::<String, BTreeSet<String>>::new();
    for context in contexts {
        for feature in &context.features {
            for domain_id in &feature.domain_ids {
                result.entry(feature.id.clone()).or_default().insert(
                    canonical_by_source
                        .get(domain_id)
                        .cloned()
                        .unwrap_or_else(|| domain_id.clone()),
                );
            }
        }
    }
    result
        .into_iter()
        .map(|(id, domains)| (id, domains.into_iter().collect()))
        .collect()
}

fn flow_relations(
    contexts: &[ReviewContext],
    canonical_by_source: &BTreeMap<String, String>,
) -> BTreeMap<String, (Vec<String>, Vec<String>)> {
    let mut result = BTreeMap::<String, (BTreeSet<String>, BTreeSet<String>)>::new();
    for context in contexts {
        for flow in &context.flows {
            let entry = result.entry(flow.id.clone()).or_default();
            for domain_id in &flow.domain_ids {
                entry.0.insert(
                    canonical_by_source
                        .get(domain_id)
                        .cloned()
                        .unwrap_or_else(|| domain_id.clone()),
                );
            }
            entry.1.extend(flow.feature_ids.iter().cloned());
        }
    }
    result
        .into_iter()
        .map(|(id, (domains, features))| {
            (
                id,
                (
                    domains.into_iter().collect(),
                    features.into_iter().collect(),
                ),
            )
        })
        .collect()
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
    if suggestion.bypass_validation() {
        destination.entry(id).or_insert_with(|| suggestion.clone());
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
    fn bypass_validation(&self) -> bool {
        false
    }
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
    fn bypass_validation(&self) -> bool {
        self.is_demoted()
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
