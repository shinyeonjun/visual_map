//! FeatureGroup 조립 컨텍스트.

use super::super::model::{
    FeatureConfidence, FeatureGroup, FeatureKind, FeatureStatus, FeatureVisibility,
};
use super::super::reachability::ReachabilityIndex;
use crate::facts::{Entrypoint, FactStore, Reference, ResolutionStatus, ResourceAccess};
use crate::languages::common::metadata::stable_id;
use std::collections::{BTreeSet, HashMap};

pub(super) struct FeatureBuildContext<'a> {
    pub domains_by_unit: &'a HashMap<String, Vec<String>>,
    pub outgoing: &'a HashMap<String, Vec<&'a Reference>>,
    pub facts: &'a FactStore,
    pub resources_by_unit: &'a HashMap<String, Vec<&'a ResourceAccess>>,
    pub reachability: &'a ReachabilityIndex,
}

pub(super) struct FeatureBuildInput<'a> {
    pub kind: FeatureKind,
    pub base_status: FeatureStatus,
    pub visibility: FeatureVisibility,
    pub entrypoint: Option<&'a Entrypoint>,
    pub feature_key: Option<&'a str>,
    pub operation_root_id: Option<&'a str>,
    pub owner_units: &'a [usize],
    pub scope_units: &'a [usize],
    pub reachable_unit_count: usize,
    pub flow_ids: &'a [String],
}

impl FeatureBuildContext<'_> {
    pub fn build_feature(&self, input: FeatureBuildInput<'_>) -> FeatureGroup {
        let root_unit_id = input
            .operation_root_id
            .or_else(|| {
                input
                    .entrypoint
                    .map(|value| value.unit_id.as_str())
                    .or_else(|| {
                        input
                            .owner_units
                            .first()
                            .map(|index| self.reachability.unit_id(*index))
                    })
            })
            .unwrap_or_default();
        let key = input
            .feature_key
            .map(str::to_string)
            .or_else(|| {
                input
                    .entrypoint
                    .map(|value| format!("entrypoint:{}", value.id))
            })
            .unwrap_or_else(|| format!("operation:{}", root_unit_id));
        let id = stable_id("feature", &key);
        let root_unit = self.facts.unit(root_unit_id);

        let mut domain_ids = BTreeSet::new();
        for &unit_index in input.owner_units {
            let unit_id = self.reachability.unit_id(unit_index);
            if let Some(ids) = self.domains_by_unit.get(unit_id) {
                domain_ids.extend(ids.iter().cloned());
            }
        }

        let mut resolved_edge_count = 0;
        let mut unresolved_edge_count = 0;
        let mut dynamic_edge_count = 0;
        let mut dynamic_boundary_ids = Vec::new();
        let mut evidence = Vec::new();
        if let Some(unit) = root_unit {
            evidence.push(crate::facts::Evidence::new(
                "featureUnit",
                unit.qualified_name.clone(),
                unit.span.clone(),
            ));
        }
        if let Some(entrypoint) = input.entrypoint {
            evidence.extend(entrypoint.evidence.clone());
        }

        for &unit_index in input.scope_units {
            let unit_id = self.reachability.unit_id(unit_index);
            for reference in self.outgoing.get(unit_id).into_iter().flatten() {
                match reference.status {
                    ResolutionStatus::Confirmed => resolved_edge_count += 1,
                    ResolutionStatus::Candidate | ResolutionStatus::Unknown => {
                        unresolved_edge_count += 1
                    }
                    ResolutionStatus::Dynamic => {
                        dynamic_edge_count += 1;
                        dynamic_boundary_ids.push(reference.id.clone());
                    }
                }
                evidence.extend(reference.evidence.clone());
            }
        }
        evidence.sort_by(|left, right| left.id.cmp(&right.id));
        evidence.dedup_by(|left, right| left.id == right.id);
        evidence.truncate(24);
        dynamic_boundary_ids.sort();
        dynamic_boundary_ids.dedup();

        let flow_ids = input.flow_ids.to_vec();
        let resource_ids = input
            .scope_units
            .iter()
            .filter_map(|index| {
                self.resources_by_unit
                    .get(self.reachability.unit_id(*index))
            })
            .flatten()
            .map(|resource| resource.id.clone())
            .collect::<Vec<_>>();
        let status = if dynamic_edge_count > 0 || unresolved_edge_count > 0 {
            if matches!(input.base_status, FeatureStatus::Confirmed) {
                FeatureStatus::Candidate
            } else {
                input.base_status.clone()
            }
        } else {
            input.base_status.clone()
        };

        let label = input
            .entrypoint
            .map(entrypoint_label)
            .or_else(|| root_unit.map(|unit| unit.qualified_name.clone()))
            .unwrap_or_else(|| key.clone());

        FeatureGroup {
            id,
            key,
            label,
            kind: input.kind,
            status,
            visibility: input.visibility,
            confidence: FeatureConfidence {
                level: confidence_level(
                    resolved_edge_count,
                    unresolved_edge_count,
                    dynamic_edge_count,
                ),
                resolved_edge_count,
                unresolved_edge_count,
                dynamic_edge_count,
                evidence_count: evidence.len(),
            },
            domain_ids: domain_ids.into_iter().collect(),
            unit_ids: input
                .owner_units
                .iter()
                .map(|index| self.reachability.unit_id(*index).to_string())
                .collect(),
            reachable_unit_count: input.reachable_unit_count,
            entrypoint_ids: input
                .entrypoint
                .map(|value| vec![value.id.clone()])
                .unwrap_or_default(),
            flow_ids,
            resource_ids,
            dynamic_boundary_ids,
            evidence,
            summary: None,
        }
    }
}

fn entrypoint_label(entrypoint: &Entrypoint) -> String {
    match (&entrypoint.method, &entrypoint.path) {
        (Some(method), Some(path)) => format!("{method} {path}"),
        (_, Some(path)) => path.clone(),
        _ => entrypoint.name.clone(),
    }
}

fn confidence_level(resolved: usize, unresolved: usize, dynamic: usize) -> String {
    if unresolved == 0 && dynamic == 0 {
        "high".into()
    } else if resolved > 0 {
        "medium".into()
    } else {
        "low".into()
    }
}
