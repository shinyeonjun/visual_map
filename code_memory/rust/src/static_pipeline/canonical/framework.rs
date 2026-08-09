use super::store::BundleStore;
use crate::static_pipeline::framework_ir::FrameworkIr;
use codebase_fact_model::analysis::AnalysisUnit;
use codebase_fact_model::coverage::{
    AnalysisCapability, AnalysisGap, AnalysisScope, CapabilityExecutionState, CapabilityReceipt,
    CoverageDenominator, DeclaredSupport, EvidencePrecision, GapCode,
};
use codebase_fact_model::fact_graph::{
    DispatchKind, FactEdge, FactEdgeKind, FactNode, FactNodeDetails, FactNodeKind, FactRole,
    FactRoleAssignment, FactTruth, ResolutionMethod, Visibility,
};
use codebase_fact_model::identity::{
    AnalysisUnitId, EvidenceId, FactEdgeId, FactNodeId, SnapshotId,
};
use codebase_fact_model::validation::Validate;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct CanonicalFrameworkCounts {
    pub(super) route_node_count: u64,
    pub(super) exposes_edge_count: u64,
    pub(super) handles_edge_count: u64,
    pub(super) unresolved_handler_count: u64,
}

#[derive(Default)]
struct UnitCanonicalAudit {
    route_nodes: BTreeSet<FactNodeId>,
    edges: BTreeSet<FactEdgeId>,
    resolved_handlers: u64,
    gap_codes: BTreeSet<GapCode>,
}

pub(super) fn ingest_framework_routes(
    store: &mut BundleStore,
    framework_ir: &FrameworkIr,
    units: &BTreeMap<AnalysisUnitId, &AnalysisUnit>,
    repository_id: &FactNodeId,
    snapshot_id: &SnapshotId,
) -> Result<CanonicalFrameworkCounts, String> {
    if framework_ir.snapshot_id != *snapshot_id || framework_ir.receipt.snapshot_id != *snapshot_id
    {
        return Err("Framework IR belongs to another snapshot".to_string());
    }
    for evidence in &framework_ir.evidence {
        store.insert_evidence(evidence)?;
    }
    for gap in &framework_ir.gaps {
        store.insert_gap(gap)?;
    }

    let mut counts = CanonicalFrameworkCounts::default();
    let mut unit_results = units
        .keys()
        .cloned()
        .map(|unit_id| (unit_id, UnitCanonicalAudit::default()))
        .collect::<BTreeMap<_, _>>();
    for gap in &framework_ir.gaps {
        if let Some(unit_id) = gap.scope.unit_id() {
            unit_results
                .get_mut(unit_id)
                .ok_or_else(|| format!("framework gap references unknown unit {unit_id}"))?
                .gap_codes
                .insert(gap.code);
        }
    }

    for route in &framework_ir.routes {
        let unit = units.get(&route.unit_id).ok_or_else(|| {
            format!(
                "Framework IR route references unknown unit {}",
                route.unit_id
            )
        })?;
        if unit.language != route.language {
            return Err(format!(
                "Framework IR route language differs from unit {}",
                route.unit_id
            ));
        }
        if store.evidence(route.evidence_id.as_str())?.is_none() {
            return Err(format!(
                "Framework IR route references missing evidence {}",
                route.evidence_id
            ));
        }
        let file_id = store
            .resolve_file_exact(&route.unit_id, route.language, &route.source_path)?
            .ok_or_else(|| {
                format!(
                    "Framework IR route has no exact canonical file: {}/{}",
                    route.unit_id, route.source_path
                )
            })?;
        let qualified_name = format!("{} {}", route.method, route.path);
        let route_id = FactNode::stable_id(
            FactNodeKind::HttpRoute,
            Some(route.language),
            Some(&route.unit_id),
            &qualified_name,
            None,
        )
        .map_err(|error| format!("cannot build canonical HTTP route identity: {error}"))?;
        store.insert_node(
            &FactNode {
                id: route_id.clone(),
                snapshot_id: snapshot_id.clone(),
                family: FactNodeKind::HttpRoute.family(),
                kind: FactNodeKind::HttpRoute,
                native_kind: Some(format!("{}:http_route", route.framework)),
                qualified_name: qualified_name.clone(),
                display_name: qualified_name,
                signature: None,
                details: Some(FactNodeDetails::HttpRoute {
                    method: route.method.clone(),
                    path: route.path.clone(),
                }),
                visibility: Visibility::Public,
                language: Some(route.language),
                analysis_unit_id: Some(route.unit_id.clone()),
                // One logical endpoint can have more than one registration
                // file. File ownership is therefore expressed by EXPOSES
                // edges rather than a lossy single parent.
                parent_id: Some(repository_id.clone()),
                definition_evidence_id: Some(route.evidence_id.clone()),
                evidence_ids: vec![route.evidence_id.clone()],
                roles: Vec::new(),
                flags: route.flags,
            },
            true,
        )?;
        let exposes = canonical_edge(
            snapshot_id,
            &file_id,
            &route_id,
            FactEdgeKind::Exposes,
            Some(&unit.context.id),
            vec![route.evidence_id.clone()],
        )?;
        store.insert_edge(&exposes)?;

        let result = unit_results
            .get_mut(&route.unit_id)
            .expect("framework unit result");
        if result.route_nodes.insert(route_id.clone()) {
            counts.route_node_count += 1;
        }
        if result.edges.insert(exposes.id.clone()) {
            counts.exposes_edge_count += 1;
        }

        let handler_id = match route.handler_symbol_id.as_ref() {
            Some(symbol_id) => store.resolve_symbol_exact(&route.unit_id, symbol_id)?,
            None => None,
        };
        let Some(handler_id) = handler_id else {
            counts.unresolved_handler_count += 1;
            result.gap_codes.insert(GapCode::UnresolvedTarget);
            let scope = match route.handler_symbol_id.clone() {
                Some(symbol_id) => AnalysisScope::NativeSymbol {
                    unit_id: route.unit_id.clone(),
                    symbol_id,
                },
                None => AnalysisScope::File {
                    unit_id: Some(route.unit_id.clone()),
                    path: route.source_path.clone(),
                },
            };
            store.insert_gap(&AnalysisGap {
                code: GapCode::UnresolvedTarget,
                scope,
                capability: Some(AnalysisCapability::FrameworkBindings),
                evidence_ids: vec![route.evidence_id.clone()],
                message: format!(
                    "No unique provider definition was available for {} {}",
                    route.method, route.path
                ),
            })?;
            continue;
        };
        assign_role(
            store,
            &handler_id,
            FactRole::Handler,
            route.evidence_id.clone(),
        )?;
        let handles = canonical_edge(
            snapshot_id,
            &handler_id,
            &route_id,
            FactEdgeKind::Handles,
            Some(&unit.context.id),
            vec![route.evidence_id.clone()],
        )?;
        store.insert_edge(&handles)?;
        result.resolved_handlers += 1;
        if result.edges.insert(handles.id.clone()) {
            counts.handles_edge_count += 1;
        }
    }

    for (unit_id, unit) in units {
        let framework_audit = framework_ir
            .unit_audit
            .get(unit_id)
            .ok_or_else(|| format!("Framework IR omitted unit audit {unit_id}"))?;
        let result = unit_results
            .get(unit_id)
            .expect("canonical framework unit audit");
        let (execution_state, precision) = if framework_audit.candidate_count == 0 {
            (
                CapabilityExecutionState::NotApplicable,
                EvidencePrecision::None,
            )
        } else if framework_audit.rejected_route_count == 0
            && result.resolved_handlers == framework_audit.candidate_count
        {
            (
                CapabilityExecutionState::Complete,
                EvidencePrecision::ExactRange,
            )
        } else {
            (
                CapabilityExecutionState::Partial,
                EvidencePrecision::ExactRange,
            )
        };
        let mut gap_codes = result.gap_codes.iter().copied().collect::<Vec<_>>();
        gap_codes.sort();
        let receipt = CapabilityReceipt {
            unit_id: unit_id.clone(),
            capability: AnalysisCapability::FrameworkBindings,
            declared_support: DeclaredSupport::Conditional,
            execution_state,
            precision,
            denominator: CoverageDenominator::Known {
                eligible_count: framework_audit.candidate_count,
            },
            covered_count: result
                .resolved_handlers
                .min(framework_audit.candidate_count),
            emitted_fact_count: result.route_nodes.len() as u64,
            emitted_relation_count: result.edges.len() as u64,
            truncated_count: 0,
            gap_codes,
        };
        receipt.validate().map_err(|error| {
            format!(
                "invalid framework capability receipt for {}/{}: {error}",
                unit.language.as_str(),
                unit_id
            )
        })?;
        store.insert_capability_receipt(&receipt)?;
    }
    Ok(counts)
}

fn assign_role(
    store: &mut BundleStore,
    node_id: &FactNodeId,
    role: FactRole,
    evidence_id: EvidenceId,
) -> Result<(), String> {
    let mut node = store
        .node(node_id)?
        .ok_or_else(|| format!("framework handler node disappeared: {node_id}"))?;
    node.evidence_ids.push(evidence_id.clone());
    node.evidence_ids.sort();
    node.evidence_ids.dedup();
    match node
        .roles
        .iter_mut()
        .find(|assignment| assignment.role == role)
    {
        Some(assignment) => {
            assignment.evidence_ids.push(evidence_id);
            assignment.evidence_ids.sort();
            assignment.evidence_ids.dedup();
        }
        None => node.roles.push(FactRoleAssignment {
            role,
            evidence_ids: vec![evidence_id],
        }),
    }
    node.roles.sort_by_key(|assignment| assignment.role);
    store.insert_node(&node, true)
}

fn canonical_edge(
    snapshot_id: &SnapshotId,
    source_id: &FactNodeId,
    target_id: &FactNodeId,
    kind: FactEdgeKind,
    semantic_context_id: Option<&codebase_fact_model::identity::SemanticContextId>,
    mut evidence_ids: Vec<EvidenceId>,
) -> Result<FactEdge, String> {
    evidence_ids.sort();
    evidence_ids.dedup();
    let id = FactEdge::stable_id(source_id, target_id, kind, semantic_context_id, None)
        .map_err(|error| format!("cannot build canonical framework edge identity: {error}"))?;
    let edge = FactEdge {
        id,
        snapshot_id: snapshot_id.clone(),
        source_id: source_id.clone(),
        target_id: target_id.clone(),
        family: kind.family(),
        kind,
        truth: FactTruth::Confirmed,
        resolution: ResolutionMethod::FrameworkAdapter,
        dispatch: DispatchKind::NotApplicable,
        semantic_context_id: semantic_context_id.cloned(),
        qualifier: None,
        evidence_ids,
    };
    edge.validate()
        .map_err(|error| format!("invalid canonical framework edge: {error}"))?;
    Ok(edge)
}
