use super::model::PreparedStaticOverview;
use super::CleanBundleError;
use std::collections::{HashMap, HashSet};

pub(super) fn validate(prepared: &PreparedStaticOverview) -> Result<(), CleanBundleError> {
    let domain_ids = unique_ids("domain", prepared.domains.iter().map(|item| &item.id))?;
    let feature_ids = unique_ids("feature", prepared.features.iter().map(|item| &item.id))?;
    let flow_ids = unique_ids(
        "flow",
        prepared.execution_flows.flows.iter().map(|item| &item.id),
    )?;
    let unit_ids = unique_ids("unit", prepared.units.iter().map(|item| &item.id))?;
    let entrypoint_ids = unique_ids(
        "entrypoint",
        prepared.entrypoints.iter().map(|item| &item.id),
    )?;
    let resource_ids = unique_ids("resource", prepared.resources.iter().map(|item| &item.id))?;
    let dynamic_ids = unique_ids(
        "dynamicBoundary",
        prepared.dynamic_boundaries.iter().map(|item| &item.id),
    )?;

    for domain in &prepared.domains {
        ensure_refs("domain.unitIds", &domain.id, &domain.unit_ids, &unit_ids)?;
        ensure_refs(
            "domain.featureIds",
            &domain.id,
            &domain.feature_ids,
            &feature_ids,
        )?;
        ensure_refs(
            "domain.entrypointIds",
            &domain.id,
            &domain.entrypoint_ids,
            &entrypoint_ids,
        )?;
        ensure_refs(
            "domain.resourceIds",
            &domain.id,
            &domain.resource_ids,
            &resource_ids,
        )?;
    }
    for feature in &prepared.features {
        ensure_refs(
            "feature.domainIds",
            &feature.id,
            &feature.domain_ids,
            &domain_ids,
        )?;
        ensure_refs("feature.unitIds", &feature.id, &feature.unit_ids, &unit_ids)?;
        ensure_refs("feature.flowIds", &feature.id, &feature.flow_ids, &flow_ids)?;
        ensure_refs(
            "feature.entrypointIds",
            &feature.id,
            &feature.entrypoint_ids,
            &entrypoint_ids,
        )?;
        ensure_refs(
            "feature.resourceIds",
            &feature.id,
            &feature.resource_ids,
            &resource_ids,
        )?;
        ensure_refs(
            "feature.dynamicBoundaryIds",
            &feature.id,
            &feature.dynamic_boundary_ids,
            &dynamic_ids,
        )?;
    }
    for entrypoint in &prepared.entrypoints {
        ensure_ref(
            "entrypoint.unitId",
            &entrypoint.id,
            &entrypoint.unit_id,
            &unit_ids,
        )?;
    }
    for resource in &prepared.resources {
        ensure_ref(
            "resource.unitId",
            &resource.id,
            &resource.unit_id,
            &unit_ids,
        )?;
    }
    for boundary in &prepared.dynamic_boundaries {
        ensure_ref(
            "dynamicBoundary.sourceUnitId",
            &boundary.id,
            &boundary.source_unit_id,
            &unit_ids,
        )?;
    }
    for unit_id in &prepared.unassigned_unit_ids {
        ensure_ref("unassignedUnitIds", "overview", unit_id, &unit_ids)?;
    }

    let flow_nodes = validate_flows(prepared, &unit_ids, &dynamic_ids)?;
    for link in &prepared.execution_flows.links {
        ensure_ref(
            "flowLink.sourceFlowId",
            &link.id,
            &link.source_flow_id,
            &flow_ids,
        )?;
        ensure_ref(
            "flowLink.targetFlowId",
            &link.id,
            &link.target_flow_id,
            &flow_ids,
        )?;
        ensure_ref(
            "flowLink.sourceNodeId",
            &link.id,
            &link.source_node_id,
            flow_nodes.get(&link.source_flow_id).unwrap(),
        )?;
        ensure_ref(
            "flowLink.targetEntryNodeId",
            &link.id,
            &link.target_entry_node_id,
            flow_nodes.get(&link.target_flow_id).unwrap(),
        )?;
        ensure_ref(
            "flowLink.targetExitNodeId",
            &link.id,
            &link.target_exit_node_id,
            flow_nodes.get(&link.target_flow_id).unwrap(),
        )?;
        if let Some(return_node_id) = &link.return_node_id {
            ensure_ref(
                "flowLink.returnNodeId",
                &link.id,
                return_node_id,
                flow_nodes.get(&link.source_flow_id).unwrap(),
            )?;
        }
    }
    for relation in &prepared.relations {
        ensure_ref(
            "relation.sourceDomainId",
            "relation",
            &relation.source_domain_id,
            &domain_ids,
        )?;
        ensure_ref(
            "relation.targetDomainId",
            "relation",
            &relation.target_domain_id,
            &domain_ids,
        )?;
    }

    Ok(())
}

fn validate_flows(
    prepared: &PreparedStaticOverview,
    unit_ids: &HashSet<String>,
    dynamic_ids: &HashSet<String>,
) -> Result<HashMap<String, HashSet<String>>, CleanBundleError> {
    let mut flow_nodes = HashMap::new();
    for flow in &prepared.execution_flows.flows {
        ensure_ref("flow.ownerUnitId", &flow.id, &flow.owner_unit_id, unit_ids)?;
        let node_ids = unique_ids("flow.node", flow.nodes.iter().map(|node| &node.id))?;
        ensure_ref("flow.entryNodeId", &flow.id, &flow.entry_node_id, &node_ids)?;
        ensure_ref("flow.exitNodeId", &flow.id, &flow.exit_node_id, &node_ids)?;
        for node in &flow.nodes {
            ensure_ref(
                "flow.node.ownerUnitId",
                &node.id,
                &node.owner_unit_id,
                unit_ids,
            )?;
        }
        for edge in &flow.edges {
            ensure_ref(
                "flow.edge.sourceNodeId",
                &edge.id,
                &edge.source_node_id,
                &node_ids,
            )?;
            ensure_ref(
                "flow.edge.targetNodeId",
                &edge.id,
                &edge.target_node_id,
                &node_ids,
            )?;
        }
        for boundary_id in &flow.dynamic_boundary_ids {
            ensure_ref(
                "flow.dynamicBoundaryIds",
                &flow.id,
                boundary_id,
                dynamic_ids,
            )?;
        }
        flow_nodes.insert(flow.id.clone(), node_ids);
    }
    Ok(flow_nodes)
}

fn unique_ids<'a>(
    kind: &str,
    ids: impl Iterator<Item = &'a String>,
) -> Result<HashSet<String>, CleanBundleError> {
    let mut unique = HashSet::new();
    for id in ids {
        if !unique.insert(id.clone()) {
            return Err(CleanBundleError::Validation(format!(
                "{kind} ID가 중복됩니다: {id}"
            )));
        }
    }
    Ok(unique)
}

fn ensure_refs(
    kind: &str,
    owner_id: &str,
    references: &[String],
    available: &HashSet<String>,
) -> Result<(), CleanBundleError> {
    for reference in references {
        ensure_ref(kind, owner_id, reference, available)?;
    }
    Ok(())
}

fn ensure_ref(
    kind: &str,
    owner_id: &str,
    reference: &str,
    available: &HashSet<String>,
) -> Result<(), CleanBundleError> {
    if !available.contains(reference) {
        return Err(CleanBundleError::Validation(format!(
            "{kind}의 {owner_id}가 존재하지 않는 ID를 참조합니다: {reference}"
        )));
    }
    Ok(())
}
