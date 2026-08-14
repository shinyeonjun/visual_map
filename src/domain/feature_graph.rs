//! Feature 쌍의 다차원 유사도를 계산하는 Multi-view Feature Similarity Graph.

use crate::config::DomainPolicy;
use crate::domain::tfidf::{self, FeatureTerms};
use crate::facts::{FactStore, ReferenceKind, ResolutionStatus};
use crate::flow::ExecutionFlowGraph;
use std::collections::{HashMap, HashSet};

struct SimilarityWeights {
    call: f64,
    flow: f64,
    resource: f64,
    path: f64,
    lexical: f64,
}

impl SimilarityWeights {
    fn from_policy(policy: &DomainPolicy) -> Self {
        let raw = [
            policy.feature_call_weight,
            policy.feature_flow_weight,
            policy.feature_resource_weight,
            policy.feature_path_weight,
            policy.feature_lexical_weight,
        ];
        let total = raw.iter().sum::<f64>().max(f64::MIN_POSITIVE);
        Self {
            call: raw[0] / total,
            flow: raw[1] / total,
            resource: raw[2] / total,
            path: raw[3] / total,
            lexical: raw[4] / total,
        }
    }
}

/// 두 Feature 사이의 다차원 유사도다.
#[derive(Debug, Clone)]
pub(super) struct FeatureSimilarity {
    pub call: f64,
    pub flow: f64,
    pub resource: f64,
    pub path: f64,
    pub lexical: f64,
    pub combined: f64,
}

#[allow(dead_code)]
impl FeatureSimilarity {
    /// 구조적 증거(call 또는 flow)가 하나 이상 있는지 확인한다.
    pub fn has_structural_evidence(&self) -> bool {
        self.call > 0.0 || self.flow > 0.0
    }

    /// 유사도가 0이 아닌 독립 증거 유형의 수를 센다.
    pub fn evidence_type_count(&self) -> usize {
        let mut count = 0;
        if self.call > 0.0 {
            count += 1;
        }
        if self.flow > 0.0 {
            count += 1;
        }
        if self.resource > 0.0 {
            count += 1;
        }
        if self.path > 0.0 {
            count += 1;
        }
        if self.lexical > 0.0 {
            count += 1;
        }
        count
    }
}

/// Feature 쌍의 유사도 행렬이다. 대칭 상삼각만 저장한다.
pub(super) struct SimilarityMatrix {
    size: usize,
    values: Vec<FeatureSimilarity>,
}

impl SimilarityMatrix {
    pub fn get(&self, i: usize, j: usize) -> &FeatureSimilarity {
        let (lo, hi) = if i < j { (i, j) } else { (j, i) };
        &self.values[lo * self.size - lo * (lo + 1) / 2 + hi - lo - 1]
    }

    pub fn size(&self) -> usize {
        self.size
    }

    #[cfg(test)]
    pub(super) fn uniform(size: usize, combined: f64) -> Self {
        let pair_count = size.saturating_mul(size.saturating_sub(1)) / 2;
        Self {
            size,
            values: vec![
                FeatureSimilarity {
                    call: 0.0,
                    flow: 0.0,
                    resource: 0.0,
                    path: 0.0,
                    lexical: combined,
                    combined,
                };
                pair_count
            ],
        }
    }
}

/// Feature 목록과 FactStore에서 전체 유사도 행렬을 계산한다.
pub(super) fn compute(
    feature_unit_ids: &[Vec<String>],
    feature_resource_ids: &[Vec<String>],
    feature_flow_ids: &[Vec<String>],
    feature_paths: &[HashSet<String>],
    terms: &[FeatureTerms],
    store: &FactStore,
    flows: &ExecutionFlowGraph,
    domain_policy: &DomainPolicy,
) -> SimilarityMatrix {
    let weights = SimilarityWeights::from_policy(domain_policy);
    let n = feature_unit_ids.len();
    let unit_to_feature = build_unit_to_feature_index(feature_unit_ids);
    let call_counts = compute_call_counts(feature_unit_ids, &unit_to_feature, store);
    let flow_links = compute_flow_connections(feature_flow_ids, flows);

    let mut values = Vec::with_capacity(n * (n - 1) / 2);
    for i in 0..n {
        for j in (i + 1)..n {
            let call = call_similarity(&call_counts, i, j, feature_unit_ids);
            let flow = flow_similarity(&flow_links, i, j);
            let resource = resource_similarity(&feature_resource_ids[i], &feature_resource_ids[j]);
            let path = path_similarity(&feature_paths[i], &feature_paths[j]);
            let lexical = tfidf::cosine_similarity(&terms[i], &terms[j]);

            let combined = weights.call * call
                + weights.flow * flow
                + weights.resource * resource
                + weights.path * path
                + weights.lexical * lexical;

            values.push(FeatureSimilarity {
                call,
                flow,
                resource,
                path,
                lexical,
                combined,
            });
        }
    }

    SimilarityMatrix { size: n, values }
}

fn build_unit_to_feature_index(feature_unit_ids: &[Vec<String>]) -> HashMap<String, usize> {
    let mut index = HashMap::new();
    for (feature_index, unit_ids) in feature_unit_ids.iter().enumerate() {
        for unit_id in unit_ids {
            index.entry(unit_id.clone()).or_insert(feature_index);
        }
    }
    index
}

/// Feature 쌍 사이의 확정 참조(Call/Constructs) 수를 집계한다.
fn compute_call_counts(
    _feature_unit_ids: &[Vec<String>],
    unit_to_feature: &HashMap<String, usize>,
    store: &FactStore,
) -> HashMap<(usize, usize), usize> {
    let mut counts: HashMap<(usize, usize), usize> = HashMap::new();
    for reference in &store.references {
        if reference.status != ResolutionStatus::Confirmed {
            continue;
        }
        if !matches!(
            reference.kind,
            ReferenceKind::Call | ReferenceKind::Constructs
        ) {
            continue;
        }
        let Some(target_id) = &reference.target_unit_id else {
            continue;
        };
        let Some(&source_feature) = unit_to_feature.get(&reference.source_unit_id) else {
            continue;
        };
        let Some(&target_feature) = unit_to_feature.get(target_id) else {
            continue;
        };
        if source_feature == target_feature {
            continue;
        }
        let key = if source_feature < target_feature {
            (source_feature, target_feature)
        } else {
            (target_feature, source_feature)
        };
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
}

fn call_similarity(
    counts: &HashMap<(usize, usize), usize>,
    i: usize,
    j: usize,
    feature_unit_ids: &[Vec<String>],
) -> f64 {
    let key = if i < j { (i, j) } else { (j, i) };
    let count = *counts.get(&key).unwrap_or(&0);
    if count == 0 {
        return 0.0;
    }
    let max_possible = feature_unit_ids[i].len().max(feature_unit_ids[j].len()).max(1);
    (count as f64 / max_possible as f64).clamp(0.0, 1.0)
}

/// Feature 쌍 사이의 FlowLink 연결 수를 집계한다.
fn compute_flow_connections(
    feature_flow_ids: &[Vec<String>],
    flows: &ExecutionFlowGraph,
) -> HashMap<(usize, usize), usize> {
    let flow_to_feature: HashMap<&str, usize> = feature_flow_ids
        .iter()
        .enumerate()
        .flat_map(|(feature_index, flow_ids)| {
            flow_ids
                .iter()
                .map(move |flow_id| (flow_id.as_str(), feature_index))
        })
        .collect();

    let mut counts: HashMap<(usize, usize), usize> = HashMap::new();
    for link in &flows.links {
        let Some(&source_feature) = flow_to_feature.get(link.source_flow_id.as_str()) else {
            continue;
        };
        let Some(&target_feature) = flow_to_feature.get(link.target_flow_id.as_str()) else {
            continue;
        };
        if source_feature == target_feature {
            continue;
        }
        let key = if source_feature < target_feature {
            (source_feature, target_feature)
        } else {
            (target_feature, source_feature)
        };
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
}

fn flow_similarity(counts: &HashMap<(usize, usize), usize>, i: usize, j: usize) -> f64 {
    let key = if i < j { (i, j) } else { (j, i) };
    let count = *counts.get(&key).unwrap_or(&0);
    if count == 0 {
        return 0.0;
    }
    (count as f64 / (count as f64 + 2.0)).clamp(0.0, 1.0)
}

fn resource_similarity(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let set_a: HashSet<&str> = a.iter().map(String::as_str).collect();
    let set_b: HashSet<&str> = b.iter().map(String::as_str).collect();
    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();
    if union == 0 {
        return 0.0;
    }
    intersection as f64 / union as f64
}

fn path_similarity(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let mut total_similarity = 0.0;
    let mut pair_count = 0;
    for path_a in a {
        for path_b in b {
            total_similarity += single_path_similarity(path_a, path_b);
            pair_count += 1;
        }
    }
    if pair_count == 0 {
        return 0.0;
    }
    (total_similarity / pair_count as f64).clamp(0.0, 1.0)
}

fn single_path_similarity(a: &str, b: &str) -> f64 {
    let parts_a: Vec<&str> = a.split('/').collect();
    let parts_b: Vec<&str> = b.split('/').collect();
    let common = parts_a
        .iter()
        .zip(parts_b.iter())
        .take_while(|(x, y)| x == y)
        .count();
    let max_len = parts_a.len().max(parts_b.len());
    if max_len == 0 {
        return 0.0;
    }
    common as f64 / max_len as f64
}
