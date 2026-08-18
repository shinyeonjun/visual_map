//! 기능 진입점에서 실제로 이어지는 실행 흐름만 고른다.

use crate::flow::{ExecutionFlowGraph, FlowLink};
use std::collections::{BTreeSet, HashMap, VecDeque};

pub(crate) struct FeatureFlowIndex<'a> {
    flow_id_by_owner: HashMap<&'a str, &'a str>,
    links_by_source: HashMap<&'a str, Vec<&'a FlowLink>>,
}

impl<'a> FeatureFlowIndex<'a> {
    pub(crate) fn new(flows: &'a ExecutionFlowGraph) -> Self {
        let flow_id_by_owner = flows
            .flows
            .iter()
            .map(|flow| (flow.owner_unit_id.as_str(), flow.id.as_str()))
            .collect();
        let mut links_by_source: HashMap<&'a str, Vec<&'a FlowLink>> = HashMap::new();
        for link in &flows.links {
            links_by_source
                .entry(link.source_flow_id.as_str())
                .or_default()
                .push(link);
        }
        Self {
            flow_id_by_owner,
            links_by_source,
        }
    }

    /// 진입 유닛에서 시작해 호출 링크로만 도달 가능한 flow id를 반환한다.
    pub(crate) fn collect_for_units(&self, unit_ids: &[&str]) -> Vec<String> {
        let mut selected = BTreeSet::new();
        let mut queue = VecDeque::new();

        for unit_id in unit_ids {
            let Some(flow_id) = self.flow_id_by_owner.get(unit_id) else {
                continue;
            };
            if selected.insert((*flow_id).to_string()) {
                queue.push_back(*flow_id);
            }
        }

        while let Some(flow_id) = queue.pop_front() {
            for link in self.links_by_source.get(flow_id).into_iter().flatten() {
                if selected.insert(link.target_flow_id.clone()) {
                    queue.push_back(link.target_flow_id.as_str());
                }
            }
        }

        selected.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::ResolutionStatus;
    use crate::flow::{ExecutionFlow, ExecutionFlowGraph, FlowLink};

    fn flow(id: &str, owner: &str) -> ExecutionFlow {
        ExecutionFlow {
            id: id.into(),
            owner_unit_id: owner.into(),
            entry_node_id: format!("{id}-entry"),
            exit_node_id: format!("{id}-exit"),
            nodes: Vec::new(),
            edges: Vec::new(),
            dynamic_boundary_ids: Vec::new(),
        }
    }

    fn link(source_flow: &str, target_flow: &str) -> FlowLink {
        FlowLink {
            id: format!("link-{source_flow}-{target_flow}"),
            reference_id: format!("ref-{source_flow}-{target_flow}"),
            source_flow_id: source_flow.into(),
            source_node_id: format!("{source_flow}-call"),
            target_flow_id: target_flow.into(),
            target_entry_node_id: format!("{target_flow}-entry"),
            target_exit_node_id: format!("{target_flow}-exit"),
            return_node_id: None,
            status: ResolutionStatus::Confirmed,
        }
    }

    #[test]
    fn 진입점에서_호출_링크로_이어진_flow만_포함한다() {
        let flows = ExecutionFlowGraph {
            flows: vec![
                flow("flow-list", "unit-list"),
                flow("flow-save", "unit-save"),
                flow("flow-unrelated", "unit-unrelated"),
            ],
            links: vec![link("flow-list", "flow-save")],
        };
        let index = FeatureFlowIndex::new(&flows);
        let selected = index.collect_for_units(&["unit-list"]);
        assert_eq!(selected, vec!["flow-list", "flow-save"]);
    }

    #[test]
    fn 같은_파일의_무관한_flow는_포함하지_않는다() {
        let flows = ExecutionFlowGraph {
            flows: vec![
                flow("flow-list", "unit-list"),
                flow("flow-admin", "unit-admin"),
            ],
            links: Vec::new(),
        };
        let index = FeatureFlowIndex::new(&flows);
        let selected = index.collect_for_units(&["unit-list"]);
        assert_eq!(selected, vec!["flow-list"]);
    }
}
