//! import·호출·자원 접근 등 코드 단위의 얕은 정적 관계 그래프.

use crate::facts::{FactStore, Reference, ResolutionStatus};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// 1단계에서 사용하는 코드 단위 관계 그래프다.
///
/// 노드는 Overview의 units를 참조하고, 그래프 자체에는 중복된 노드 객체를
/// 저장하지 않는다. 목적지를 확정하지 못한 간선과 동적 간선도 제거하지 않는다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StaticRelationGraph {
    pub node_ids: Vec<String>,
    pub edges: Arc<[Reference]>,
    pub dynamic_edge_ids: Vec<String>,
    pub unresolved_edge_ids: Vec<String>,
}

impl Default for StaticRelationGraph {
    fn default() -> Self {
        Self {
            node_ids: Vec::new(),
            edges: Arc::from([]),
            dynamic_edge_ids: Vec::new(),
            unresolved_edge_ids: Vec::new(),
        }
    }
}

/// 병합된 공통 Facts에서 얕은 정적 그래프를 만든다.
pub fn build(store: &FactStore) -> StaticRelationGraph {
    let node_ids = store.units.keys().cloned().collect();
    let edges: Arc<[Reference]> = store.references.clone().into();
    let dynamic_edge_ids = edges
        .iter()
        .filter(|edge| edge.status == ResolutionStatus::Dynamic)
        .map(|edge| edge.id.clone())
        .collect();
    let unresolved_edge_ids = edges
        .iter()
        .filter(|edge| edge.target_unit_id.is_none())
        .map(|edge| edge.id.clone())
        .collect();

    StaticRelationGraph {
        node_ids,
        edges,
        dynamic_edge_ids,
        unresolved_edge_ids,
    }
}
