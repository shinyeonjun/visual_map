//! 도메인별 feature·flow bundle을 실제 byte budget에 맞춰 조립한다.

use super::domains::DomainPlan;
use super::features::select_for_domain as select_features;
use super::flows::candidates_for_features;
use super::indexes::PostprocessIndexes;
use super::model::{
    AdjacentDomain, AiSemanticContext, ContextDomain, ContextFeature, ContextFlow,
    ContextProjectProfile, ContextSummary, ContextWarning, GlobalContextSummary,
};
use super::selection::{fill_domain, promote_required_relationships, required_domain_bytes};
use crate::config::AnalysisConfig;
use crate::model::AnalysisResult;
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct ChunkBuildInput<'a> {
    pub(super) chunk_id: &'a str,
    pub(super) partition: &'a [String],
    pub(super) shells: &'a [ContextDomain],
    pub(super) plan: &'a DomainPlan,
    pub(super) indexes: &'a PostprocessIndexes<'a>,
    pub(super) overview: &'a crate::views::overview::OverviewResponse,
    pub(super) result: &'a AnalysisResult,
    pub(super) config: &'a AnalysisConfig,
    pub(super) profile: ContextProjectProfile,
    pub(super) global_summary: GlobalContextSummary,
}

pub(super) fn build_chunk(input: ChunkBuildInput<'_>) -> AiSemanticContext {
    let ChunkBuildInput {
        chunk_id,
        partition,
        shells,
        plan,
        indexes,
        overview,
        result,
        config,
        profile,
        global_summary,
    } = input;
    let mut domains = partition
        .iter()
        .filter_map(|domain_id| shells.iter().find(|domain| &domain.domain_id == domain_id))
        .cloned()
        .collect::<Vec<_>>();
    let base_bytes = serialized_bytes(&ChunkShell {
        chunk_id,
        source_analysis_id: &result.analysis_id,
        project_id: &result.project.project_id,
        project_profile: &profile,
        global_summary: &global_summary,
        adjacent_domains: &[],
        domains: &domains,
    });
    let total_signal = domains
        .iter()
        .map(|domain| u64::from(domain.signal.score.max(1)))
        .sum::<u64>();
    let remaining = config
        .postprocess
        .target_budget_bytes
        .saturating_sub(base_bytes);
    let mut feature_membership_total = 0;
    let mut flow_membership_total = 0;
    let mut total_feature_ids = BTreeSet::new();
    let mut total_flow_ids = BTreeSet::new();
    let mut required_extra_total = 0usize;
    let mut required_extras = BTreeMap::new();
    let mut selections = Vec::new();
    for domain in &mut domains {
        let shell_bytes = serialized_bytes(domain);
        let feature_selection = select_features(
            plan.clusters
                .iter()
                .find(|cluster| cluster.representative_id == domain.domain_id)
                .expect("domain shell에 대응하는 cluster가 있어야 한다"),
            indexes,
            &config.postprocess,
        );
        let flow_selection = candidates_for_features(
            &feature_selection.included_ids,
            indexes,
            &config.postprocess,
        );
        let mut feature_selection = feature_selection;
        promote_required_relationships(&mut feature_selection, &flow_selection);
        let required_bytes = required_domain_bytes(domain, &feature_selection, &flow_selection);
        let required_extra = required_bytes.saturating_sub(shell_bytes);
        required_extra_total = required_extra_total.saturating_add(required_extra);
        required_extras.insert(domain.domain_id.clone(), required_extra);
        feature_membership_total += feature_selection.total_count;
        flow_membership_total += flow_selection.total_count;
        total_feature_ids.extend(feature_selection.included_ids.iter().cloned());
        total_flow_ids.extend(
            flow_selection
                .candidates
                .iter()
                .map(|candidate| candidate.context.id.clone()),
        );
        selections.push((domain.domain_id.clone(), feature_selection, flow_selection));
    }
    let extra_remaining = remaining.saturating_sub(required_extra_total);
    for domain in &mut domains {
        let shell_bytes = serialized_bytes(domain);
        let signal = domain.signal.score.max(1) as usize;
        let allocation = if total_signal == 0 {
            0
        } else {
            extra_remaining.saturating_mul(signal) / total_signal as usize
        };
        let required_extra = required_extras.get(&domain.domain_id).copied().unwrap_or(0);
        let Some((_, feature_selection, flow_selection)) = selections
            .iter_mut()
            .find(|(domain_id, _, _)| domain_id == &domain.domain_id)
        else {
            continue;
        };
        *domain = fill_domain(
            domain.clone(),
            std::mem::replace(
                feature_selection,
                super::features::FeatureSelection {
                    features: Vec::new(),
                    included_ids: std::collections::HashSet::new(),
                    mandatory_ids: std::collections::HashSet::new(),
                    total_count: 0,
                    priorities: std::collections::HashMap::new(),
                },
            ),
            std::mem::replace(
                flow_selection,
                super::flows::FlowSelection {
                    candidates: Vec::new(),
                    total_count: 0,
                },
            ),
            shell_bytes
                .saturating_add(required_extra)
                .saturating_add(allocation),
        );
    }
    let adjacent_domains = adjacent_domains(partition, overview, shells, config);
    let mut context = AiSemanticContext {
        schema_version: "ai-semantic-context.v1",
        chunk_id: chunk_id.into(),
        source_analysis_id: result.analysis_id.clone(),
        source_schema_version: result.schema_version.clone(),
        project_id: result.project.project_id.clone(),
        analysis_status: result.status.clone(),
        policy_version: "ai-semantic-context-policy.v2",
        project_profile: profile,
        global_summary,
        adjacent_domains,
        domains,
        features: Vec::new(),
        flows: Vec::new(),
        domain_aliases: plan.aliases.clone(),
        suppressed_domains: plan.suppressed.clone(),
        summary: ContextSummary {
            total_source_domains: partition.len(),
            included_domains: partition.len(),
            suppressed_domains: plan.suppressed.len(),
            total_features: feature_membership_total,
            total_unique_features: total_feature_ids.len(),
            total_feature_memberships: feature_membership_total,
            included_features: 0,
            included_unique_features: 0,
            included_feature_memberships: 0,
            omitted_unique_features: 0,
            omitted_feature_memberships: 0,
            total_flows: flow_membership_total,
            total_unique_flows: total_flow_ids.len(),
            total_flow_memberships: flow_membership_total,
            included_flows: 0,
            included_unique_flows: 0,
            included_flow_memberships: 0,
            omitted_unique_flows: 0,
            omitted_flow_memberships: 0,
            budget_bytes: config.postprocess.target_budget_bytes,
            used_bytes: 0,
        },
        warnings: Vec::new(),
    };
    normalize_context_items(&mut context);
    update_summary(&mut context);
    context
}

pub(super) fn fit_context_budget(context: &mut AiSemanticContext, budget: usize) {
    while serialized_bytes(context) > budget {
        let optional_feature = context
            .features
            .iter()
            .enumerate()
            .filter(|(_, feature)| !feature.required)
            .min_by(|(_, left), (_, right)| {
                feature_optional_score(left)
                    .cmp(&feature_optional_score(right))
                    .then_with(|| right.id.cmp(&left.id))
            })
            .map(|(index, _)| index);
        let optional_flow = context
            .flows
            .iter()
            .enumerate()
            .filter(|(_, flow)| !flow.required)
            .min_by(|(_, left), (_, right)| {
                flow_optional_score(left)
                    .cmp(&flow_optional_score(right))
                    .then_with(|| right.id.cmp(&left.id))
            })
            .map(|(index, _)| index);

        if let Some(feature_index) = optional_feature {
            let feature_id = context.features.remove(feature_index).id;
            for domain in &mut context.domains {
                domain.feature_ids.retain(|id| id != &feature_id);
            }
            for flow in &mut context.flows {
                if !flow.required {
                    flow.feature_ids.retain(|id| id != &feature_id);
                }
            }
            context
                .flows
                .retain(|flow| flow.required || !flow.feature_ids.is_empty());
            record_budget_omission(context);
        } else if let Some(flow_index) = optional_flow {
            let flow_id = context.flows.remove(flow_index).id;
            for domain in &mut context.domains {
                domain.flow_ids.retain(|id| id != &flow_id);
            }
            for feature in &mut context.features {
                feature.flow_ids.retain(|id| id != &flow_id);
            }
            record_budget_omission(context);
        } else {
            context.warnings.push(ContextWarning {
                code: "required_content_exceeds_budget".into(),
                message: "필수 기능·실행 흐름을 보존하면 byte budget을 초과합니다.".into(),
                related_ids: context
                    .domains
                    .iter()
                    .map(|domain| domain.domain_id.clone())
                    .collect(),
            });
            break;
        }
        sync_item_domain_links(context);
        update_summary(context);
    }
    context.summary.used_bytes = serialized_bytes(context);
    for domain in &mut context.domains {
        domain.omission.used_bytes = serialized_bytes(domain);
    }
}

/// 도메인별 bundle을 Codex 입력용 전역 인덱스로 정규화한다.
///
/// 원본 overview에서는 하나의 기능이나 흐름이 여러 도메인에 걸쳐 보일 수
/// 있다. 이를 도메인 안에 매번 직렬화하면 같은 객체와 실행 단계가 청크마다
/// 반복되어 컨텍스트 크기와 AI 호출 수가 함께 증가한다. 도메인은 ID 목록만
/// 유지하고 실제 객체는 context 전역 배열에 한 번만 둔다.
fn normalize_context_items(context: &mut AiSemanticContext) {
    let mut features = BTreeMap::<String, ContextFeature>::new();
    let mut flows = BTreeMap::<String, ContextFlow>::new();
    let mut feature_domains = BTreeMap::<String, BTreeSet<String>>::new();
    let mut flow_domains = BTreeMap::<String, BTreeSet<String>>::new();

    context.features.clear();
    context.flows.clear();

    for domain in &mut context.domains {
        domain.feature_ids = domain
            .features
            .iter()
            .map(|feature| feature.id.clone())
            .collect();
        domain.feature_ids.sort();
        domain.feature_ids.dedup();
        domain.flow_ids = domain.flows.iter().map(|flow| flow.id.clone()).collect();
        domain.flow_ids.sort();
        domain.flow_ids.dedup();

        for feature in &domain.features {
            feature_domains
                .entry(feature.id.clone())
                .or_default()
                .insert(domain.domain_id.clone());
            features
                .entry(feature.id.clone())
                .or_insert_with(|| feature.clone());
        }
        for flow in &domain.flows {
            flow_domains
                .entry(flow.id.clone())
                .or_default()
                .insert(domain.domain_id.clone());
            flows.entry(flow.id.clone()).or_insert_with(|| flow.clone());
        }

        domain.features.clear();
        domain.flows.clear();
    }

    for (feature_id, mut feature) in features {
        feature.domain_ids = feature_domains
            .remove(&feature_id)
            .unwrap_or_default()
            .into_iter()
            .collect();
        feature.shared = feature.domain_ids.len() > 1;
        context.features.push(feature);
    }

    // 하나의 실행 흐름이 여러 도메인에 걸쳐 보이는 것은 정적 관계상
    // 유효하지만, Codex 입력에는 같은 흐름을 도메인마다 반복해서 넣을
    // 이유가 없다. 신호가 가장 강한 도메인을 대표 소유자로 정하고,
    // shared 플래그로 원래의 공유 관계만 보존한다.
    let domain_scores = context
        .domains
        .iter()
        .map(|domain| (domain.domain_id.clone(), domain.signal.score))
        .collect::<BTreeMap<_, _>>();
    let flow_primary_domains = flow_domains
        .iter()
        .filter_map(|(flow_id, domains)| {
            domains
                .iter()
                .max_by(|left, right| {
                    domain_scores
                        .get(*left)
                        .cmp(&domain_scores.get(*right))
                        .then_with(|| right.cmp(left))
                })
                .map(|domain_id| (flow_id.clone(), domain_id.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    for domain in &mut context.domains {
        domain.flow_ids.retain(|flow_id| {
            flow_primary_domains
                .get(flow_id)
                .is_some_and(|primary| primary == &domain.domain_id)
        });
    }
    for (flow_id, mut flow) in flows {
        let original_domains = flow_domains.get(&flow_id).cloned().unwrap_or_default();
        flow.shared = original_domains.len() > 1;
        flow.domain_ids = flow_primary_domains
            .get(&flow_id)
            .cloned()
            .into_iter()
            .collect();
        context.flows.push(flow);
    }
}

fn record_budget_omission(context: &mut AiSemanticContext) {
    for domain in &mut context.domains {
        *domain
            .omission
            .reasons
            .entry("global_budget_exceeded".into())
            .or_insert(0) += 1;
    }
}

/// 정규화된 전역 항목과 도메인 ID 참조를 서로 일치시킨다.
fn sync_item_domain_links(context: &mut AiSemanticContext) {
    let feature_domains = context
        .domains
        .iter()
        .map(|domain| {
            (
                domain.domain_id.as_str(),
                domain.feature_ids.iter().collect::<BTreeSet<_>>(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let flow_domains = context
        .domains
        .iter()
        .map(|domain| {
            (
                domain.domain_id.as_str(),
                domain.flow_ids.iter().collect::<BTreeSet<_>>(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for feature in &mut context.features {
        feature.domain_ids.retain(|domain_id| {
            feature_domains
                .get(domain_id.as_str())
                .is_some_and(|ids| ids.contains(&&feature.id))
        });
    }
    for flow in &mut context.flows {
        flow.domain_ids.retain(|domain_id| {
            flow_domains
                .get(domain_id.as_str())
                .is_some_and(|ids| ids.contains(&&flow.id))
        });
    }
}

fn update_summary(context: &mut AiSemanticContext) {
    let feature_memberships = context
        .domains
        .iter()
        .map(|domain| domain.feature_ids.len())
        .sum::<usize>();
    let flow_memberships = context
        .domains
        .iter()
        .map(|domain| domain.flow_ids.len())
        .sum::<usize>();
    let feature_ids = context
        .features
        .iter()
        .map(|feature| feature.id.clone())
        .collect::<BTreeSet<_>>();
    let flow_ids = context
        .flows
        .iter()
        .map(|flow| flow.id.clone())
        .collect::<BTreeSet<_>>();
    context.summary.included_features = feature_memberships;
    context.summary.included_unique_features = feature_ids.len();
    context.summary.included_feature_memberships = feature_memberships;
    context.summary.omitted_unique_features = context
        .summary
        .total_unique_features
        .saturating_sub(feature_ids.len());
    context.summary.omitted_feature_memberships = context
        .summary
        .total_feature_memberships
        .saturating_sub(feature_memberships);
    context.summary.included_flows = flow_memberships;
    context.summary.included_unique_flows = flow_ids.len();
    context.summary.included_flow_memberships = flow_memberships;
    context.summary.omitted_unique_flows = context
        .summary
        .total_unique_flows
        .saturating_sub(flow_ids.len());
    context.summary.omitted_flow_memberships = context
        .summary
        .total_flow_memberships
        .saturating_sub(flow_memberships);
    context.summary.used_bytes = serialized_bytes(context);
    for domain in &mut context.domains {
        domain.omission.included_features = domain.feature_ids.len();
        domain.omission.included_flows = domain.flow_ids.len();
        domain.omission.used_bytes = serialized_bytes(domain);
    }
}

fn feature_optional_score(feature: &super::model::ContextFeature) -> usize {
    feature.entrypoint_ids.len()
        + feature.resource_ids.len()
        + feature.flow_ids.len()
        + feature.symbols.len()
}

fn flow_optional_score(flow: &super::model::ContextFlow) -> usize {
    flow.feature_ids.len() + flow.steps.len()
}

fn adjacent_domains(
    partition: &[String],
    overview: &crate::views::overview::OverviewResponse,
    shells: &[ContextDomain],
    config: &AnalysisConfig,
) -> Vec<AdjacentDomain> {
    let mut relation_kinds: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for relation in &overview.relations {
        let inside_source = partition.contains(&relation.source_domain_id);
        let inside_target = partition.contains(&relation.target_domain_id);
        if inside_source == inside_target {
            continue;
        }
        let outside = if inside_source {
            &relation.target_domain_id
        } else {
            &relation.source_domain_id
        };
        relation_kinds
            .entry(outside.clone())
            .or_default()
            .insert(relation.kind.clone());
    }
    relation_kinds
        .into_iter()
        .filter_map(|(domain_id, kinds)| {
            let domain = shells.iter().find(|domain| domain.domain_id == domain_id)?;
            Some(AdjacentDomain {
                domain_id,
                label: domain.current_label.clone(),
                relation_kinds: kinds.into_iter().collect(),
            })
        })
        .take(config.postprocess.max_adjacent_domains)
        .collect()
}

pub(super) fn serialized_bytes<T: serde::Serialize>(value: &T) -> usize {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
}

#[derive(serde::Serialize)]
struct ChunkShell<'a> {
    chunk_id: &'a str,
    source_analysis_id: &'a str,
    project_id: &'a str,
    project_profile: &'a ContextProjectProfile,
    global_summary: &'a GlobalContextSummary,
    adjacent_domains: &'a [AdjacentDomain],
    domains: &'a [ContextDomain],
}
