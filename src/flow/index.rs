//! 실행 흐름 생성에 필요한 사실을 소유 유닛별로 역색인한다.

use crate::facts::{ControlFlowFact, FactStore, Reference, ReferenceKind};
use std::collections::HashMap;

/// 함수마다 전체 Facts를 다시 검색하지 않도록 실행 흐름 입력을 역색인한다.
pub(super) struct FlowInputIndex<'a> {
    control_by_owner: HashMap<&'a str, Vec<&'a ControlFlowFact>>,
    calls_by_owner: HashMap<&'a str, Vec<&'a Reference>>,
}

impl<'a> FlowInputIndex<'a> {
    pub(super) fn new(facts: &'a FactStore) -> Self {
        let mut control_by_owner: HashMap<&str, Vec<&ControlFlowFact>> = HashMap::new();
        for fact in &facts.control_flow {
            control_by_owner
                .entry(fact.owner_unit_id.as_str())
                .or_default()
                .push(fact);
        }

        let mut calls_by_owner: HashMap<&str, Vec<&Reference>> = HashMap::new();
        for reference in &facts.references {
            if matches!(
                reference.kind,
                ReferenceKind::Call | ReferenceKind::Constructs
            ) {
                calls_by_owner
                    .entry(reference.source_unit_id.as_str())
                    .or_default()
                    .push(reference);
            }
        }

        Self {
            control_by_owner,
            calls_by_owner,
        }
    }

    pub(super) fn control_for(&self, owner_unit_id: &str) -> &[&'a ControlFlowFact] {
        self.control_by_owner
            .get(owner_unit_id)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub(super) fn calls_for(&self, owner_unit_id: &str) -> &[&'a Reference] {
        self.calls_by_owner
            .get(owner_unit_id)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
}
