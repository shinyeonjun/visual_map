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
            (feature, related_flows, priority, bytes)
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        let left_efficiency = efficiency(left.2, left.3);
        let right_efficiency = efficiency(right.2, right.3);
        right_efficiency
            .cmp(&left_efficiency)
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

    for (feature, related_flows, _, _) in candidates {
        let mut candidate_domain = domain.clone();
        candidate_domain.features.push(feature.clone());
        for flow in &related_flows {
            if !selected_flow_ids.contains(&flow.id) {
                candidate_domain.flows.push((*flow).clone());
            }
        }
        let candidate_bytes = serialized_bytes(&candidate_domain);
        if candidate_bytes <= budget_bytes {
            selected_feature_ids.insert(feature.id.clone());
            domain.features.push(feature);
            for flow in related_flows {
                if selected_flow_ids.insert(flow.id.clone()) {
                    domain.flows.push(flow.clone());
                }
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
