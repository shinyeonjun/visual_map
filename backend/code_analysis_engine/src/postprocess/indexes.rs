//! 후보정 단계에서 반복되는 ID 조회를 한 번만 준비한다.

use crate::facts::{CodeUnit, Entrypoint, ResourceAccess};
use crate::flow::ExecutionFlow;
use crate::model::FileEntry;
use crate::views::overview::{FeatureGroup, OverviewResponse};
use std::collections::{HashMap, HashSet};

pub(crate) struct PostprocessIndexes<'a> {
    pub overview: &'a OverviewResponse,
    pub units: HashMap<&'a str, &'a CodeUnit>,
    pub domains: HashMap<&'a str, &'a crate::domain::DomainGroup>,
    pub features: HashMap<&'a str, &'a FeatureGroup>,
    pub entrypoints: HashMap<&'a str, &'a Entrypoint>,
    pub resources: HashMap<&'a str, &'a ResourceAccess>,
    pub flows: HashMap<&'a str, &'a ExecutionFlow>,
    pub visible_unit_ids: HashSet<String>,
    pub visible_feature_ids: HashSet<String>,
    pub visible_entrypoint_ids: HashSet<String>,
    pub visible_resource_ids: HashSet<String>,
    pub visible_flow_ids: HashSet<String>,
}

impl<'a> PostprocessIndexes<'a> {
    pub fn build(overview: &'a OverviewResponse, files: &[FileEntry]) -> Self {
        let test_file_ids = files
            .iter()
            .filter(|file| file.is_test)
            .map(|file| file.file_id.as_str())
            .collect::<HashSet<_>>();
        let units = overview
            .units
            .iter()
            .map(|unit| (unit.id.as_str(), unit))
            .collect::<HashMap<_, _>>();
        let domains = overview
            .domains
            .iter()
            .map(|domain| (domain.id.as_str(), domain))
            .collect::<HashMap<_, _>>();
        let features = overview
            .features
            .iter()
            .map(|feature| (feature.id.as_str(), feature))
            .collect::<HashMap<_, _>>();
        let entrypoints = overview
            .entrypoints
            .iter()
            .map(|entrypoint| (entrypoint.id.as_str(), entrypoint))
            .collect::<HashMap<_, _>>();
        let resources = overview
            .resources
            .iter()
            .map(|resource| (resource.id.as_str(), resource))
            .collect::<HashMap<_, _>>();
        let flows = overview
            .execution_flows
            .flows
            .iter()
            .map(|flow| (flow.id.as_str(), flow))
            .collect::<HashMap<_, _>>();

        let visible_unit_ids = overview
            .units
            .iter()
            .filter(|unit| !test_file_ids.contains(unit.file_id.as_str()))
            .map(|unit| unit.id.clone())
            .collect::<HashSet<_>>();
        let visible_feature_ids = overview
            .features
            .iter()
            .filter(|feature| {
                feature
                    .unit_ids
                    .iter()
                    .any(|unit_id| visible_unit_ids.contains(unit_id))
            })
            .map(|feature| feature.id.clone())
            .collect::<HashSet<_>>();
        let visible_entrypoint_ids = overview
            .entrypoints
            .iter()
            .filter(|entrypoint| visible_unit_ids.contains(&entrypoint.unit_id))
            .map(|entrypoint| entrypoint.id.clone())
            .collect::<HashSet<_>>();
        let visible_resource_ids = overview
            .resources
            .iter()
            .filter(|resource| visible_unit_ids.contains(&resource.unit_id))
            .map(|resource| resource.id.clone())
            .collect::<HashSet<_>>();
        let visible_flow_ids = overview
            .execution_flows
            .flows
            .iter()
            .filter(|flow| visible_unit_ids.contains(&flow.owner_unit_id))
            .map(|flow| flow.id.clone())
            .collect::<HashSet<_>>();

        Self {
            overview,
            units,
            domains,
            features,
            entrypoints,
            resources,
            flows,
            visible_unit_ids,
            visible_feature_ids,
            visible_entrypoint_ids,
            visible_resource_ids,
            visible_flow_ids,
        }
    }

    pub fn feature(&self, id: &str) -> Option<&'a FeatureGroup> {
        self.features
            .get(id)
            .copied()
            .filter(|feature| self.visible_feature_ids.contains(&feature.id))
    }

    pub fn unit(&self, id: &str) -> Option<&'a CodeUnit> {
        self.units
            .get(id)
            .copied()
            .filter(|unit| self.visible_unit_ids.contains(&unit.id))
    }

    pub fn entrypoint(&self, id: &str) -> Option<&'a Entrypoint> {
        self.entrypoints
            .get(id)
            .copied()
            .filter(|entrypoint| self.visible_entrypoint_ids.contains(&entrypoint.id))
    }

    pub fn resource(&self, id: &str) -> Option<&'a ResourceAccess> {
        self.resources
            .get(id)
            .copied()
            .filter(|resource| self.visible_resource_ids.contains(&resource.id))
    }

    pub fn flow(&self, id: &str) -> Option<&'a ExecutionFlow> {
        self.flows
            .get(id)
            .copied()
            .filter(|flow| self.visible_flow_ids.contains(&flow.id))
    }
}
