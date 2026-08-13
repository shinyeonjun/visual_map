//! 정적 관계 그래프의 중복 간선을 압축하는 기능.

use crate::facts::{Evidence, Reference, ReferenceKind, ResolutionStatus};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 같은 코드 단위 쌍에서 발생한 관계를 프론트 집계 전에 압축한 간선이다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregatedReference {
    pub source_unit_id: String,
    pub target_unit_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidate_unit_ids: Vec<String>,
    pub target_name: String,
    pub kind: ReferenceKind,
    pub status: ResolutionStatus,
    pub weight: u32,
    pub evidence: Vec<Evidence>,
}

/// 동일한 출발점·목적지·관계 종류의 간선을 하나로 합친다.
pub fn aggregate(edges: &[Reference]) -> Vec<AggregatedReference> {
    // 입력 그래프가 수백만 간선까지 커질 수 있으므로 매 간선마다 B-tree를
    // 탐색하지 않는다. 해시 집계 후 마지막에만 정렬해 결과의 안정적인
    // 직렬화 순서는 유지한다.
    let mut result: HashMap<(String, String, String), AggregatedReference> = HashMap::new();
    for edge in edges {
        let target_key = edge
            .target_unit_id
            .clone()
            .unwrap_or_else(|| format!("name:{}", edge.target_name));
        let key = (
            edge.source_unit_id.clone(),
            target_key,
            format!("{:?}", edge.kind),
        );
        let entry = result.entry(key).or_insert_with(|| AggregatedReference {
            source_unit_id: edge.source_unit_id.clone(),
            target_unit_id: edge.target_unit_id.clone(),
            candidate_unit_ids: edge.candidate_unit_ids.clone(),
            target_name: edge.target_name.clone(),
            kind: edge.kind.clone(),
            status: edge.status.clone(),
            weight: 0,
            evidence: Vec::new(),
        });
        entry.weight = entry.weight.saturating_add(1);
        entry.evidence.extend(edge.evidence.clone());
        entry
            .candidate_unit_ids
            .extend(edge.candidate_unit_ids.iter().cloned());
        entry.candidate_unit_ids.sort();
        entry.candidate_unit_ids.dedup();
        entry.status = worse_status(&entry.status, &edge.status);
    }
    let mut values: Vec<_> = result.into_values().collect();
    for value in &mut values {
        value.evidence.sort_by(|left, right| left.id.cmp(&right.id));
        value.evidence.dedup_by(|left, right| left.id == right.id);
    }
    values.sort_by(|left, right| {
        left.source_unit_id
            .cmp(&right.source_unit_id)
            .then_with(|| left.target_unit_id.cmp(&right.target_unit_id))
            .then_with(|| left.target_name.cmp(&right.target_name))
            .then_with(|| format!("{:?}", left.kind).cmp(&format!("{:?}", right.kind)))
    });
    values
}

fn worse_status(left: &ResolutionStatus, right: &ResolutionStatus) -> ResolutionStatus {
    let rank = |status: &ResolutionStatus| match status {
        ResolutionStatus::Dynamic => 4,
        ResolutionStatus::Unknown => 3,
        ResolutionStatus::Candidate => 2,
        ResolutionStatus::Confirmed => 1,
    };
    if rank(left) >= rank(right) {
        left.clone()
    } else {
        right.clone()
    }
}
