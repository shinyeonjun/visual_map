//! 도메인별 feature+flow bundle을 실제 직렬화 바이트 예산 안에 채운다.

use super::features::FeatureSelection;
use super::flows::FlowSelection;
use super::model::{ContextDomain, ContextFeature, ContextFlow};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

pub(crate) fn fill_domain(
    mut domain: ContextDomain,
    features: FeatureSelection,
    flows: FlowSelection,
    budget_bytes: usize,
) -> ContextDomain {
    let flow_by_id = flows
        .candidates
        .into_iter()
        .map(|candidate| (candidate.context.id.clone(), candidate))
        .collect::<HashMap<_, _>>();
    let mut candidates = features
        .features
        .into_iter()
        .map(|feature| {
            let related_flows = flow_by_feature(&feature, &flow_by_id);
            let priority = features.priorities.get(&feature.id).copied().unwrap_or(0);
            let bytes = serialized_bundle_bytes(&feature, &related_flows);
            let required = feature.required || related_flows.iter().any(|flow| flow.required);
            (feature, related_flows, priority, bytes, required)
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .4
            .cmp(&left.4)
            .then_with(|| efficiency(right.2, right.3).cmp(&efficiency(left.2, left.3)))
            .then_with(|| right.2.cmp(&left.2))
            .then_with(|| left.0.id.cmp(&right.0.id))
    });

    let total_features = candidates.len();
    let total_flows = flows.total_count;
    let mut reasons = BTreeMap::new();
    let mut selected_flow_ids = HashSet::new();
    let mut selected_feature_ids = HashSet::new();
    domain.features.clear();
    domain.flows.clear();
    domain.omission.total_features = total_features;
    domain.omission.total_flows = total_flows;
    domain.omission.budget_bytes = budget_bytes;

    for (mut feature, related_flows, _, _, required) in candidates {
        feature.required = required;
        let mut candidate_domain = domain.clone();
        candidate_domain.features.push(feature.clone());
        for flow in &related_flows {
            if !selected_flow_ids.contains(&flow.id) {
                candidate_domain.flows.push((*flow).clone());
            }
        }
        let candidate_bytes = serialized_bytes(&candidate_domain);
        if candidate_bytes <= budget_bytes || required {
            selected_feature_ids.insert(feature.id.clone());
            domain.features.push(feature);
            for flow in related_flows {
                if selected_flow_ids.insert(flow.id.clone()) {
                    domain.flows.push(flow.clone());
                }
            }
            if candidate_bytes > budget_bytes {
                domain.omission.required_content_exceeds_budget = true;
            }
            continue;
        }
        *reasons.entry("budget_exceeded".to_string()).or_insert(0) += 1;
    }

    for feature in &mut domain.features {
        feature.flow_ids.retain(|flow_id| {
            selected_flow_ids.contains(flow_id) && selected_feature_ids.contains(&feature.id)
        });
    }
    for flow in &mut domain.flows {
        flow.feature_ids
            .retain(|feature_id| selected_feature_ids.contains(feature_id));
    }
    domain
        .features
        .sort_by(|left, right| left.id.cmp(&right.id));
    domain.flows.sort_by(|left, right| left.id.cmp(&right.id));
    domain.omission.included_features = domain.features.len();
    domain.omission.included_flows = domain.flows.len();
    domain.omission.reasons = reasons;
    domain.omission.used_bytes = serialized_bytes(&domain);
    domain
}

/// 필수 feature·flow만 담은 도메인 shell의 실제 직렬화 크기를 계산한다.
///
/// 이 값은 선택 항목을 담기 전에 청크 분할과 예산 예약에 사용한다. 숫자 개수
/// 기준이 아니라 현재 모델의 실제 JSON 크기를 기준으로 하므로 언어·프레임워크와
/// 프로젝트 규모가 달라도 같은 보존 계약을 적용할 수 있다.
pub(crate) fn required_domain_bytes(
    domain: &ContextDomain,
    features: &FeatureSelection,
    flows: &FlowSelection,
) -> usize {
    let required_feature_ids = features
        .features
        .iter()
        .filter(|feature| feature.required)
        .map(|feature| feature.id.clone())
        .collect::<HashSet<_>>();
    let mut required_domain = domain.clone();
    required_domain.features = features
        .features
        .iter()
        .filter(|feature| required_feature_ids.contains(&feature.id))
        .cloned()
        .collect();
    required_domain.flows = flows
        .candidates
        .iter()
        .filter(|candidate| {
            candidate.required
                || candidate
                    .context
                    .feature_ids
                    .iter()
                    .any(|feature_id| required_feature_ids.contains(feature_id))
        })
        .map(|candidate| candidate.context.clone())
        .collect();
    for feature in &mut required_domain.features {
        feature
            .flow_ids
            .retain(|flow_id| required_domain.flows.iter().any(|flow| &flow.id == flow_id));
    }
    serialized_bytes(&required_domain)
}

/// 필수 flow가 연결된 feature도 필수로 승격한다.
///
/// flow를 보존하면서 그 소유 feature를 제거하면 프론트엔드와 Codex 모두
/// 관계의 시작점을 잃기 때문에, 관계 보존을 위해 양쪽을 함께 승격한다.
pub(crate) fn promote_required_relationships(
    features: &mut FeatureSelection,
    flows: &FlowSelection,
) {
    let required_feature_ids = flows
        .candidates
        .iter()
        .filter(|candidate| candidate.required)
        .flat_map(|candidate| candidate.context.feature_ids.iter().cloned())
        .collect::<HashSet<_>>();
    for feature in &mut features.features {
        if required_feature_ids.contains(&feature.id) {
            feature.required = true;
            features.mandatory_ids.insert(feature.id.clone());
        }
    }
}

fn flow_by_feature<'a>(
    feature: &ContextFeature,
    flow_by_id: &'a HashMap<String, super::flows::FlowCandidate>,
) -> Vec<&'a ContextFlow> {
    feature
        .flow_ids
        .iter()
        .filter_map(|flow_id| flow_by_id.get(flow_id).map(|candidate| &candidate.context))
        .collect()
}

fn efficiency(priority: u64, bytes: usize) -> u128 {
    if bytes == 0 {
        u128::MAX
    } else {
        u128::from(priority).saturating_mul(1_000_000) / bytes as u128
    }
}

fn serialized_bundle_bytes(feature: &ContextFeature, flows: &[&ContextFlow]) -> usize {
    serde_json::to_vec(&FeatureBundle {
        feature,
        flows: flows.to_vec(),
    })
    .map(|bytes| bytes.len())
    .unwrap_or(usize::MAX)
}

fn serialized_bytes<T: Serialize>(value: &T) -> usize {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
}

#[derive(Serialize)]
struct FeatureBundle<'a> {
    feature: &'a ContextFeature,
    flows: Vec<&'a ContextFlow>,
}

#[cfg(test)]
mod tests {
    use super::fill_domain;
    use crate::flow::{FlowEdgeKind, FlowNodeKind};
    use crate::postprocess::features::FeatureSelection;
    use crate::postprocess::flows::{FlowCandidate, FlowSelection};
    use crate::postprocess::model::{
        ContextDomain, ContextFeature, ContextFlow, DomainDecision, DomainRole, DomainSignal,
        FeatureVisibility,
    };
    use std::collections::HashMap;

    fn domain() -> ContextDomain {
        ContextDomain {
            domain_id: "domain-orders".into(),
            source_domain_ids: vec!["domain-orders".into()],
            current_label: "orders".into(),
            role: DomainRole::Business,
            decision: DomainDecision::Original,
            signal: DomainSignal::default(),
            source_paths: Vec::new(),
            entrypoints: Vec::new(),
            resources: Vec::new(),
            features: Vec::new(),
            flows: Vec::new(),
            evidence_ids: Vec::new(),
            omission: Default::default(),
        }
    }

    fn required_feature() -> ContextFeature {
        ContextFeature {
            id: "feature-create-order".into(),
            current_label: "createOrder".into(),
            visibility: FeatureVisibility::UserFacing,
            required: true,
            tags: Vec::new(),
            symbols: vec!["createOrder".into()],
            source_paths: vec!["orders.ts".into()],
            entrypoint_ids: vec!["entrypoint-post-orders".into()],
            resource_ids: Vec::new(),
            flow_ids: vec!["flow-create-order".into()],
        }
    }

    fn required_flow() -> ContextFlow {
        ContextFlow {
            id: "flow-create-order".into(),
            feature_ids: vec!["feature-create-order".into()],
            owner_unit_id: "unit-create-order".into(),
            owner_name: "createOrder".into(),
            required: true,
            steps: vec![crate::postprocess::model::ContextFlowStep {
                id: "node-entry".into(),
                kind: FlowNodeKind::Entry,
                label: "createOrder".into(),
                target_unit_id: None,
            }],
            edges: vec![crate::postprocess::model::ContextFlowEdge {
                source_node_id: "node-entry".into(),
                target_node_id: "node-exit".into(),
                kind: FlowEdgeKind::Sequential,
                status: crate::facts::ResolutionStatus::Confirmed,
            }],
            dynamic_boundary_ids: Vec::new(),
            selection_reason: "entrypoint_flow".into(),
        }
    }

    #[test]
    fn 필수_기능과_흐름은_예산보다_커도_보존한다() {
        let feature = required_feature();
        let flow = required_flow();
        let output = fill_domain(
            domain(),
            FeatureSelection {
                features: vec![feature.clone()],
                included_ids: [feature.id.clone()].into_iter().collect(),
                mandatory_ids: [feature.id.clone()].into_iter().collect(),
                total_count: 1,
                priorities: HashMap::from([(feature.id.clone(), 100)]),
            },
            FlowSelection {
                candidates: vec![FlowCandidate {
                    context: flow.clone(),
                    score: 100,
                    required: true,
                }],
                total_count: 1,
            },
            1,
        );

        assert_eq!(output.features.len(), 1);
        assert_eq!(output.flows.len(), 1);
        assert!(output.omission.required_content_exceeds_budget);
        assert_eq!(output.features[0].flow_ids, vec![flow.id]);
        assert_eq!(output.flows[0].feature_ids, vec![feature.id]);
    }
}
