//! capability 벡터를 클러스터링 입력으로 변환한다.

use crate::domain::capabilities::Capability;
use crate::facts::FactStore;
use crate::flow::ExecutionFlowGraph;
use std::collections::{BTreeSet, HashMap, HashSet};

pub(super) struct CapabilityData {
    pub unit_ids: Vec<Vec<String>>,
    pub resource_ids: Vec<Vec<String>>,
    pub flow_ids: Vec<Vec<String>>,
    pub paths: Vec<HashSet<String>>,
    pub keys: Vec<String>,
    pub contract_paths: Vec<BTreeSet<String>>,
}

pub(super) fn extract_capability_data(
    capabilities: &[Capability],
    store: &FactStore,
    flows: &ExecutionFlowGraph,
) -> CapabilityData {
    let flow_ids_by_owner: HashMap<&str, Vec<&str>> = flows
        .flows
        .iter()
        .fold(HashMap::new(), |mut index, flow| {
            index
                .entry(flow.owner_unit_id.as_str())
                .or_default()
                .push(flow.id.as_str());
            index
        });

    CapabilityData {
        unit_ids: capabilities
            .iter()
            .map(|capability| capability.unit_ids.clone())
            .collect(),
        resource_ids: capabilities
            .iter()
            .map(|capability| capability.resource_ids.clone())
            .collect(),
        flow_ids: capabilities
            .iter()
            .map(|capability| {
                capability
                    .unit_ids
                    .iter()
                    .flat_map(|unit_id| {
                        flow_ids_by_owner
                            .get(unit_id.as_str())
                            .into_iter()
                            .flatten()
                            .map(|flow_id| (*flow_id).to_string())
                    })
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect()
            })
            .collect(),
        paths: capabilities
            .iter()
            .map(|capability| {
                capability
                    .unit_ids
                    .iter()
                    .filter_map(|unit_id| {
                        store
                            .unit(unit_id)
                            .map(|unit| unit.relative_path.replace('\\', "/"))
                    })
                    .collect()
            })
            .collect(),
        keys: capabilities
            .iter()
            .map(|capability| capability.key.clone())
            .collect(),
        contract_paths: capabilities
            .iter()
            .map(|capability| capability.contract_paths.clone())
            .collect(),
    }
}
