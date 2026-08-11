//! Evidence-backed execution paths over canonical static facts.

use crate::fact_graph::CanonicalFactSnapshot;
use codebase_fact_model::{
    coverage::{
        AnalysisCapability, AnalysisGap, AnalysisScope, CapabilityExecutionState, CapabilityReceipt,
    },
    fact_graph::{DispatchKind, FactEdge, FactEdgeKind, FactNode, FactNodeKind, FactTruth},
    identity::{AnalysisUnitId, EvidenceId, FactEdgeId, FactNodeId},
};
use codebase_semantic_model::{TracePathState, TracePathSummary};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug)]
pub(crate) struct TraceLimits {
    pub max_entrypoints: usize,
    pub max_paths_per_entry: usize,
    pub max_total_paths: usize,
    pub max_depth: usize,
    pub max_expansions_per_entry: usize,
}

impl TraceLimits {
    pub(crate) const fn base_map() -> Self {
        Self {
            max_entrypoints: 64,
            max_paths_per_entry: 2,
            max_total_paths: 64,
            max_depth: 10,
            max_expansions_per_entry: 2_048,
        }
    }

    pub(crate) const fn selection() -> Self {
        Self {
            max_entrypoints: 1,
            max_paths_per_entry: 8,
            max_total_paths: 8,
            max_depth: 16,
            max_expansions_per_entry: 8_192,
        }
    }
}

#[derive(Clone)]
struct ExecutionStep<'a> {
    edge: &'a FactEdge,
    target_id: FactNodeId,
    uncertain_dispatch: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExecutionStepQuality {
    Exact,
    Candidate,
}

struct ExecutionGraph<'a> {
    nodes: BTreeMap<FactNodeId, &'a FactNode>,
    edges: BTreeMap<FactEdgeId, &'a FactEdge>,
    adjacency: BTreeMap<FactNodeId, Vec<ExecutionStep<'a>>>,
    blocked_sources: BTreeSet<FactNodeId>,
    capability_states: BTreeMap<(AnalysisUnitId, AnalysisCapability), CapabilityExecutionState>,
    evidence_gaps: BTreeSet<(EvidenceId, Option<AnalysisCapability>)>,
    unit_gaps: BTreeSet<(AnalysisUnitId, Option<AnalysisCapability>)>,
    workspace_gaps: BTreeSet<Option<AnalysisCapability>>,
}

impl<'a> ExecutionGraph<'a> {
    fn new(
        nodes: &'a [FactNode],
        edges: &'a [FactEdge],
        receipts: &[CapabilityReceipt],
        gaps: &[AnalysisGap],
    ) -> Result<Self, String> {
        let nodes = nodes
            .iter()
            .map(|node| (node.id.clone(), node))
            .collect::<BTreeMap<_, _>>();
        let edge_index = edges
            .iter()
            .map(|edge| (edge.id.clone(), edge))
            .collect::<BTreeMap<_, _>>();
        let mut adjacency = BTreeMap::<FactNodeId, Vec<ExecutionStep<'a>>>::new();
        let mut blocked_sources = BTreeSet::new();

        for edge in edges {
            let Some((source_id, target_id)) = logical_execution_pair(edge) else {
                continue;
            };
            let (Some(source), Some(target)) = (nodes.get(source_id), nodes.get(target_id)) else {
                return Err(format!(
                    "TracePath edge endpoint가 존재하지 않습니다: {}",
                    edge.id
                ));
            };
            if !is_product_source(source) {
                continue;
            }
            if !is_product_target(target) {
                blocked_sources.insert(source_id.clone());
                continue;
            }
            if edge.truth != FactTruth::Confirmed {
                blocked_sources.insert(source_id.clone());
                continue;
            }
            let Some(quality) = execution_step_quality(edge) else {
                blocked_sources.insert(source_id.clone());
                continue;
            };
            if quality == ExecutionStepQuality::Candidate {
                blocked_sources.insert(source_id.clone());
            }
            let step = ExecutionStep {
                edge,
                target_id: target_id.clone(),
                uncertain_dispatch: quality == ExecutionStepQuality::Candidate,
            };
            adjacency.entry(source_id.clone()).or_default().push(step);
        }

        for steps in adjacency.values_mut() {
            steps.sort_by(|left, right| step_key(left).cmp(&step_key(right)));
        }

        let mut capability_states = BTreeMap::new();
        for receipt in receipts {
            let key = (receipt.unit_id.clone(), receipt.capability);
            if let Some(previous) = capability_states.insert(key, receipt.execution_state) {
                if previous != receipt.execution_state {
                    return Err(format!(
                        "TracePath capability receipt가 서로 충돌합니다: {}/{}",
                        receipt.unit_id,
                        receipt.capability.as_str()
                    ));
                }
            }
        }
        let mut unit_gaps = BTreeSet::new();
        let mut workspace_gaps = BTreeSet::new();
        let mut evidence_gaps = BTreeSet::new();
        for gap in gaps {
            if !gap.evidence_ids.is_empty() {
                evidence_gaps.extend(
                    gap.evidence_ids
                        .iter()
                        .cloned()
                        .map(|evidence_id| (evidence_id, gap.capability)),
                );
                continue;
            }
            match &gap.scope {
                AnalysisScope::Workspace | AnalysisScope::RepositoryScope { .. } => {
                    workspace_gaps.insert(gap.capability);
                }
                AnalysisScope::AnalysisUnit { unit_id }
                | AnalysisScope::NativeSymbol { unit_id, .. } => {
                    unit_gaps.insert((unit_id.clone(), gap.capability));
                }
                AnalysisScope::File {
                    unit_id: Some(unit_id),
                    ..
                } => {
                    unit_gaps.insert((unit_id.clone(), gap.capability));
                }
                AnalysisScope::File { unit_id: None, .. } => {
                    workspace_gaps.insert(gap.capability);
                }
            }
        }

        Ok(Self {
            nodes,
            edges: edge_index,
            adjacency,
            blocked_sources,
            capability_states,
            evidence_gaps,
            unit_gaps,
            workspace_gaps,
        })
    }

    fn leaf_state(&self, fact_id: &FactNodeId) -> TracePathState {
        let Some(node) = self.nodes.get(fact_id) else {
            return TracePathState::Gap;
        };
        if self.blocked_sources.contains(fact_id) || self.node_has_explicit_gap(node) {
            return TracePathState::Gap;
        }
        // Source text can prove that a table name was referenced, but only a
        // certified database snapshot can prove the target object exists.
        if node.kind == FactNodeKind::TableReference {
            return TracePathState::Gap;
        }
        if is_terminal_kind(node.kind) {
            return TracePathState::Complete;
        }
        let Some(capability) = leaf_capability(node.kind) else {
            return TracePathState::Partial;
        };
        let Some(unit_id) = node.analysis_unit_id.as_ref() else {
            return TracePathState::Partial;
        };
        match self.capability_states.get(&(unit_id.clone(), capability)) {
            Some(CapabilityExecutionState::Complete | CapabilityExecutionState::NotApplicable) => {
                TracePathState::Complete
            }
            Some(
                CapabilityExecutionState::Partial
                | CapabilityExecutionState::Failed
                | CapabilityExecutionState::NotRun,
            ) => TracePathState::Gap,
            None => TracePathState::Partial,
        }
    }

    fn node_has_explicit_gap(&self, node: &FactNode) -> bool {
        let capability = leaf_capability(node.kind);
        if gap_set_matches(&self.workspace_gaps, capability)
            || node.analysis_unit_id.as_ref().is_some_and(|unit_id| {
                gap_set_matches_for_unit(&self.unit_gaps, unit_id, capability)
            })
        {
            return true;
        }
        node.definition_evidence_id
            .iter()
            .chain(node.evidence_ids.iter())
            .chain(
                node.roles
                    .iter()
                    .flat_map(|assignment| assignment.evidence_ids.iter()),
            )
            .any(|evidence_id| {
                self.evidence_gaps.contains(&(evidence_id.clone(), None))
                    || capability.is_some_and(|capability| {
                        self.evidence_gaps
                            .contains(&(evidence_id.clone(), Some(capability)))
                    })
            })
    }
}

fn gap_set_matches(
    gaps: &BTreeSet<Option<AnalysisCapability>>,
    capability: Option<AnalysisCapability>,
) -> bool {
    gaps.contains(&None) || capability.is_some_and(|capability| gaps.contains(&Some(capability)))
}

fn gap_set_matches_for_unit(
    gaps: &BTreeSet<(AnalysisUnitId, Option<AnalysisCapability>)>,
    unit_id: &AnalysisUnitId,
    capability: Option<AnalysisCapability>,
) -> bool {
    gaps.contains(&(unit_id.clone(), None))
        || capability.is_some_and(|capability| gaps.contains(&(unit_id.clone(), Some(capability))))
}

#[derive(Clone)]
struct PathCandidate {
    ordered_fact_ids: Vec<FactNodeId>,
    ordered_edge_ids: Vec<FactEdgeId>,
    encountered_gap: bool,
}

pub(crate) fn representative_trace_paths<K: Ord>(
    snapshot: &CanonicalFactSnapshot,
    owner_by_node: &BTreeMap<FactNodeId, K>,
) -> Result<Vec<TracePathSummary>, String> {
    let limits = TraceLimits::base_map();
    let graph = ExecutionGraph::new(
        &snapshot.nodes,
        &snapshot.edges,
        &snapshot.capability_receipts,
        &snapshot.gaps,
    )?;
    let entrypoints = select_entrypoints_by_owner(&graph, owner_by_node, limits.max_entrypoints);

    let mut paths_by_entrypoint = Vec::new();
    for entrypoint in entrypoints {
        paths_by_entrypoint.push(paths_from_entry(&graph, &entrypoint.id, limits)?);
    }
    let mut result = round_robin_take(
        &paths_by_entrypoint,
        limits.max_paths_per_entry,
        limits.max_total_paths,
    );
    result.sort_by(|left, right| trace_order(&graph, left, right));
    result.dedup_by(|left, right| left.trace_path_id == right.trace_path_id);
    Ok(result)
}

fn round_robin_take<T: Clone>(groups: &[Vec<T>], max_per_group: usize, limit: usize) -> Vec<T> {
    let mut result = Vec::new();
    for item_index in 0..max_per_group {
        for group in groups {
            if result.len() >= limit {
                return result;
            }
            if let Some(item) = group.get(item_index) {
                result.push(item.clone());
            }
        }
    }
    result
}

fn select_entrypoints_by_owner<'a, K: Ord>(
    graph: &ExecutionGraph<'a>,
    owner_by_node: &BTreeMap<FactNodeId, K>,
    limit: usize,
) -> Vec<&'a FactNode> {
    let mut entrypoints_by_owner = graph
        .nodes
        .values()
        .filter(|node| is_product_source(node) && entry_rank(node.kind).is_some())
        .filter_map(|node| owner_by_node.get(&node.id).map(|owner| (owner, *node)))
        .fold(
            BTreeMap::<&K, VecDeque<&FactNode>>::new(),
            |mut result, (owner, node)| {
                result.entry(owner).or_default().push_back(node);
                result
            },
        );
    for entrypoints in entrypoints_by_owner.values_mut() {
        entrypoints.make_contiguous().sort_by(|left, right| {
            entry_rank(left.kind)
                .cmp(&entry_rank(right.kind))
                .then_with(|| left.id.cmp(&right.id))
        });
    }
    let mut entrypoints = Vec::new();
    while entrypoints.len() < limit {
        let mut added = false;
        for queue in entrypoints_by_owner.values_mut() {
            if let Some(entrypoint) = queue.pop_front() {
                entrypoints.push(entrypoint);
                added = true;
                if entrypoints.len() >= limit {
                    break;
                }
            }
        }
        if !added {
            break;
        }
    }

    entrypoints
}

pub(crate) fn trace_paths_from_fact(
    snapshot: &CanonicalFactSnapshot,
    fact_id: &FactNodeId,
    limits: TraceLimits,
) -> Result<Vec<TracePathSummary>, String> {
    let graph = ExecutionGraph::new(
        &snapshot.nodes,
        &snapshot.edges,
        &snapshot.capability_receipts,
        &snapshot.gaps,
    )?;
    paths_from_entry(&graph, fact_id, limits)
}

fn paths_from_entry(
    graph: &ExecutionGraph<'_>,
    entry_fact_id: &FactNodeId,
    limits: TraceLimits,
) -> Result<Vec<TracePathSummary>, String> {
    if !graph.nodes.contains_key(entry_fact_id) {
        return Err(format!(
            "TracePath 시작 fact가 존재하지 않습니다: {entry_fact_id}"
        ));
    }
    let mut queue = VecDeque::from([PathCandidate {
        ordered_fact_ids: vec![entry_fact_id.clone()],
        ordered_edge_ids: Vec::new(),
        encountered_gap: false,
    }]);
    let mut result = Vec::new();
    let mut expansions = 0_usize;

    while let Some(mut candidate) = queue.pop_front() {
        if result.len() >= limits.max_paths_per_entry {
            break;
        }
        let current = candidate
            .ordered_fact_ids
            .last()
            .expect("a TracePath candidate always has an entry")
            .clone();
        candidate.encountered_gap |= graph.blocked_sources.contains(&current)
            || graph
                .nodes
                .get(&current)
                .is_some_and(|node| graph.node_has_explicit_gap(node));
        let outgoing = graph.adjacency.get(&current).cloned().unwrap_or_default();
        if outgoing.is_empty() {
            result.push(finalize_path(graph, candidate, graph.leaf_state(&current))?);
            continue;
        }
        if candidate.ordered_edge_ids.len() >= limits.max_depth
            || expansions >= limits.max_expansions_per_entry
        {
            result.push(finalize_path(
                graph,
                candidate,
                TracePathState::DepthLimited,
            )?);
            continue;
        }

        for step in outgoing {
            if result.len() + queue.len() >= limits.max_expansions_per_entry {
                break;
            }
            expansions = expansions.saturating_add(1);
            let mut next = candidate.clone();
            next.encountered_gap |= step.uncertain_dispatch;
            next.ordered_edge_ids.push(step.edge.id.clone());
            let cycle = next.ordered_fact_ids.contains(&step.target_id);
            next.ordered_fact_ids.push(step.target_id.clone());
            if cycle {
                result.push(finalize_path(graph, next, TracePathState::Cycle)?);
                if result.len() >= limits.max_paths_per_entry {
                    break;
                }
            } else {
                queue.push_back(next);
            }
        }
    }

    result.sort_by(|left, right| trace_order(graph, left, right));
    result.dedup_by(|left, right| left.trace_path_id == right.trace_path_id);
    Ok(result)
}

fn finalize_path(
    graph: &ExecutionGraph<'_>,
    candidate: PathCandidate,
    state: TracePathState,
) -> Result<TracePathSummary, String> {
    let state = if candidate.encountered_gap
        && matches!(state, TracePathState::Complete | TracePathState::Partial)
    {
        TracePathState::Gap
    } else {
        state
    };
    let entry_fact_id = candidate
        .ordered_fact_ids
        .first()
        .cloned()
        .ok_or_else(|| "TracePath에 시작 fact가 없습니다".to_string())?;
    let mut evidence_ids = BTreeSet::<EvidenceId>::new();
    for fact_id in &candidate.ordered_fact_ids {
        if let Some(node) = graph.nodes.get(fact_id) {
            if let Some(evidence_id) = node
                .definition_evidence_id
                .as_ref()
                .or_else(|| node.evidence_ids.first())
            {
                evidence_ids.insert(evidence_id.clone());
            }
        }
    }
    for edge_id in &candidate.ordered_edge_ids {
        let edge = graph
            .edges
            .get(edge_id)
            .ok_or_else(|| format!("TracePath edge가 존재하지 않습니다: {edge_id}"))?;
        evidence_ids.extend(edge.evidence_ids.iter().cloned());
    }
    let trace_path_id = TracePathSummary::stable_id(&entry_fact_id, &candidate.ordered_edge_ids)
        .map_err(|error| format!("TracePath identity를 만들지 못했습니다: {error}"))?;
    Ok(TracePathSummary {
        trace_path_id,
        entry_fact_id,
        ordered_fact_ids: candidate.ordered_fact_ids,
        ordered_edge_ids: candidate.ordered_edge_ids,
        state,
        evidence_ids: evidence_ids.into_iter().collect(),
    })
}

fn logical_execution_pair(edge: &FactEdge) -> Option<(&FactNodeId, &FactNodeId)> {
    match edge.kind {
        // `Handles` is a responsibility fact (handler -> endpoint). Runtime
        // order is the inverse: a request enters the endpoint, then its exact
        // handler runs. The canonical edge direction itself is never changed.
        FactEdgeKind::Handles => Some((&edge.target_id, &edge.source_id)),
        // The canonical type relation is implementation -> contract. Runtime
        // dispatch proceeds from a contract method to one of its exact
        // implementation methods, so expose the inverse only as a candidate
        // trace hop (see `execution_step_quality`).
        FactEdgeKind::Overrides => Some((&edge.target_id, &edge.source_id)),
        FactEdgeKind::Calls
        | FactEdgeKind::Constructs
        | FactEdgeKind::RoutesTo
        | FactEdgeKind::MiddlewareBefore
        | FactEdgeKind::FrontendActionCallsApi
        | FactEdgeKind::Reads
        | FactEdgeKind::Writes
        | FactEdgeKind::ExecutesQuery
        | FactEdgeKind::MapsToTable
        | FactEdgeKind::Publishes
        | FactEdgeKind::Dispatches
        | FactEdgeKind::CallsExternal
        | FactEdgeKind::UsesCache
        | FactEdgeKind::UsesFile => Some((&edge.source_id, &edge.target_id)),
        _ => None,
    }
}

fn execution_step_quality(edge: &FactEdge) -> Option<ExecutionStepQuality> {
    match edge.kind {
        FactEdgeKind::Overrides => Some(ExecutionStepQuality::Candidate),
        FactEdgeKind::Calls | FactEdgeKind::Constructs => {
            if edge
                .execution
                .as_ref()
                .is_none_or(|execution| execution.control.deferred)
            {
                return None;
            }
            match edge.dispatch {
                DispatchKind::Direct => Some(ExecutionStepQuality::Exact),
                DispatchKind::Virtual | DispatchKind::Interface | DispatchKind::Dynamic => {
                    Some(ExecutionStepQuality::Candidate)
                }
                DispatchKind::Unknown | DispatchKind::NotApplicable => None,
            }
        }
        _ => matches!(
            edge.dispatch,
            DispatchKind::Direct | DispatchKind::NotApplicable
        )
        .then_some(ExecutionStepQuality::Exact),
    }
}

fn is_product_source(node: &FactNode) -> bool {
    !node.flags.test && !node.flags.generated && !node.flags.vendor && !node.flags.external
}

fn is_product_target(node: &FactNode) -> bool {
    (!node.flags.test && !node.flags.generated && !node.flags.vendor && !node.flags.external)
        || is_terminal_kind(node.kind)
}

fn is_terminal_kind(kind: FactNodeKind) -> bool {
    matches!(
        kind,
        FactNodeKind::Database
            | FactNodeKind::Table
            | FactNodeKind::View
            | FactNodeKind::MaterializedView
            | FactNodeKind::Routine
            | FactNodeKind::Event
            | FactNodeKind::Queue
            | FactNodeKind::Topic
            | FactNodeKind::Stream
            | FactNodeKind::Channel
            | FactNodeKind::ExternalService
            | FactNodeKind::Cache
            | FactNodeKind::FileResource
    )
}

fn leaf_capability(kind: FactNodeKind) -> Option<AnalysisCapability> {
    match kind {
        FactNodeKind::HttpRoute | FactNodeKind::GraphqlEndpoint | FactNodeKind::RpcEndpoint => {
            Some(AnalysisCapability::FrameworkBindings)
        }
        FactNodeKind::Entrypoint
        | FactNodeKind::Job
        | FactNodeKind::Callable
        | FactNodeKind::Function
        | FactNodeKind::Method
        | FactNodeKind::Constructor => Some(AnalysisCapability::DirectCalls),
        FactNodeKind::Query => Some(AnalysisCapability::OrmQuery),
        FactNodeKind::FrontendAction => Some(AnalysisCapability::EventExternal),
        _ => None,
    }
}

fn entry_rank(kind: FactNodeKind) -> Option<u8> {
    match kind {
        FactNodeKind::HttpRoute | FactNodeKind::GraphqlEndpoint | FactNodeKind::RpcEndpoint => {
            Some(0)
        }
        FactNodeKind::FrontendAction => Some(1),
        FactNodeKind::Entrypoint => Some(2),
        FactNodeKind::Job => Some(3),
        _ => None,
    }
}

fn step_key<'a>(step: &'a ExecutionStep<'a>) -> (u8, u32, &'a FactNodeId, &'a FactEdgeId) {
    (
        edge_rank(step.edge.kind),
        step.edge
            .execution
            .as_ref()
            .map(|execution| execution.lexical_ordinal)
            .unwrap_or(u32::MAX),
        &step.target_id,
        &step.edge.id,
    )
}

fn edge_rank(kind: FactEdgeKind) -> u8 {
    match kind {
        FactEdgeKind::Handles => 0,
        FactEdgeKind::MiddlewareBefore => 1,
        FactEdgeKind::RoutesTo => 2,
        FactEdgeKind::FrontendActionCallsApi => 3,
        FactEdgeKind::Calls => 4,
        FactEdgeKind::Overrides => 5,
        FactEdgeKind::Constructs => 6,
        FactEdgeKind::ExecutesQuery => 7,
        FactEdgeKind::Reads => 8,
        FactEdgeKind::Writes => 9,
        FactEdgeKind::CallsExternal => 10,
        FactEdgeKind::Publishes => 11,
        FactEdgeKind::Dispatches => 12,
        FactEdgeKind::UsesCache => 13,
        FactEdgeKind::UsesFile => 14,
        _ => u8::MAX,
    }
}

fn trace_order(
    graph: &ExecutionGraph<'_>,
    left: &TracePathSummary,
    right: &TracePathSummary,
) -> std::cmp::Ordering {
    left.entry_fact_id
        .cmp(&right.entry_fact_id)
        .then_with(|| compare_trace_edges(graph, left, right))
        .then_with(|| trace_state_rank(left.state).cmp(&trace_state_rank(right.state)))
}

fn compare_trace_edges(
    graph: &ExecutionGraph<'_>,
    left: &TracePathSummary,
    right: &TracePathSummary,
) -> std::cmp::Ordering {
    for (left_id, right_id) in left.ordered_edge_ids.iter().zip(&right.ordered_edge_ids) {
        let ordering = match (graph.edges.get(left_id), graph.edges.get(right_id)) {
            (Some(left_edge), Some(right_edge)) => (
                edge_rank(left_edge.kind),
                left_edge
                    .execution
                    .as_ref()
                    .map(|execution| execution.lexical_ordinal)
                    .unwrap_or(u32::MAX),
                left_id,
            )
                .cmp(&(
                    edge_rank(right_edge.kind),
                    right_edge
                        .execution
                        .as_ref()
                        .map(|execution| execution.lexical_ordinal)
                        .unwrap_or(u32::MAX),
                    right_id,
                )),
            _ => left_id.cmp(right_id),
        };
        if ordering != std::cmp::Ordering::Equal {
            return ordering;
        }
    }
    left.ordered_edge_ids
        .len()
        .cmp(&right.ordered_edge_ids.len())
}

fn trace_state_rank(state: TracePathState) -> u8 {
    match state {
        TracePathState::Complete => 0,
        TracePathState::Partial => 1,
        TracePathState::Gap => 2,
        TracePathState::Cycle => 3,
        TracePathState::DepthLimited => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codebase_fact_model::{
        analysis::ProgrammingLanguage,
        coverage::{CoverageDenominator, DeclaredSupport, EvidencePrecision},
        fact_graph::{
            ExecutionControlContext, ExecutionOccurrence, FactNodeDetails, ResolutionMethod,
            Visibility,
        },
        identity::{EvidenceId, SemanticContextId, SnapshotId},
        source::SourceFlags,
    };

    #[test]
    fn route_runs_handler_then_service_even_though_handles_fact_points_to_route() {
        let unit = AnalysisUnitId::from_components(&["typescript", "orders"]).unwrap();
        let route = node(FactNodeKind::HttpRoute, "GET /orders", &unit);
        let handler = node(FactNodeKind::Method, "OrdersController.list", &unit);
        let service = node(FactNodeKind::Method, "OrdersService.list", &unit);
        let handles = edge(
            FactEdgeKind::Handles,
            &handler,
            &route,
            FactTruth::Confirmed,
            DispatchKind::NotApplicable,
        );
        let calls = edge(
            FactEdgeKind::Calls,
            &handler,
            &service,
            FactTruth::Confirmed,
            DispatchKind::Direct,
        );
        let receipts = vec![
            receipt(&unit, AnalysisCapability::FrameworkBindings),
            receipt(&unit, AnalysisCapability::DirectCalls),
        ];
        let nodes = [route.clone(), handler.clone(), service.clone()];
        let edges = [handles.clone(), calls.clone()];
        let graph = ExecutionGraph::new(&nodes, &edges, &receipts, &[]).unwrap();

        let paths = paths_from_entry(&graph, &route.id, TraceLimits::selection()).unwrap();

        assert_eq!(paths.len(), 1);
        assert_eq!(
            paths[0].ordered_fact_ids,
            vec![route.id, handler.id, service.id]
        );
        assert_eq!(paths[0].ordered_edge_ids, vec![handles.id, calls.id]);
        assert_eq!(paths[0].state, TracePathState::Complete);
    }

    #[test]
    fn resolved_non_direct_call_is_visible_as_a_candidate_gap() {
        let unit = AnalysisUnitId::from_components(&["java", "orders"]).unwrap();
        let entry = node(FactNodeKind::Entrypoint, "OrdersApplication.main", &unit);
        let service = node(FactNodeKind::Method, "OrdersService.run", &unit);
        let uncertain = edge(
            FactEdgeKind::Calls,
            &entry,
            &service,
            FactTruth::Confirmed,
            DispatchKind::Virtual,
        );
        let nodes = [entry.clone(), service.clone()];
        let edges = [uncertain.clone()];
        let receipts = [receipt(&unit, AnalysisCapability::DirectCalls)];
        let graph = ExecutionGraph::new(&nodes, &edges, &receipts, &[]).unwrap();

        let paths = paths_from_entry(&graph, &entry.id, TraceLimits::selection()).unwrap();

        assert_eq!(paths[0].ordered_fact_ids, vec![entry.id, service.id]);
        assert_eq!(paths[0].ordered_edge_ids, vec![uncertain.id]);
        assert_eq!(paths[0].state, TracePathState::Gap);
    }

    #[test]
    fn interface_dispatch_can_continue_through_an_exact_override_candidate() {
        let unit = AnalysisUnitId::from_components(&["python", "repository-dispatch"]).unwrap();
        let entry = node(FactNodeKind::Function, "create_session", &unit);
        let contract = node(FactNodeKind::Method, "SessionRepository.save", &unit);
        let implementation = node(
            FactNodeKind::Method,
            "PostgreSQLSessionRepository.save",
            &unit,
        );
        let query = node(FactNodeKind::Function, "upsert_session", &unit);
        let sql = node(FactNodeKind::Query, "UPDATE sessions SET title = ?", &unit);
        let table = node(FactNodeKind::Table, "sessions", &unit);
        let contract_call = edge(
            FactEdgeKind::Calls,
            &entry,
            &contract,
            FactTruth::Confirmed,
            DispatchKind::Interface,
        );
        let override_edge = edge(
            FactEdgeKind::Overrides,
            &implementation,
            &contract,
            FactTruth::Confirmed,
            DispatchKind::NotApplicable,
        );
        let query_call = edge(
            FactEdgeKind::Calls,
            &implementation,
            &query,
            FactTruth::Confirmed,
            DispatchKind::Direct,
        );
        let executes_query = edge(
            FactEdgeKind::ExecutesQuery,
            &query,
            &sql,
            FactTruth::Confirmed,
            DispatchKind::NotApplicable,
        );
        let writes = edge(
            FactEdgeKind::Writes,
            &sql,
            &table,
            FactTruth::Confirmed,
            DispatchKind::NotApplicable,
        );
        let nodes = [
            entry.clone(),
            contract,
            implementation.clone(),
            query.clone(),
            sql.clone(),
            table.clone(),
        ];
        let edges = [
            contract_call.clone(),
            override_edge.clone(),
            query_call.clone(),
            executes_query.clone(),
            writes.clone(),
        ];
        let receipts = [
            receipt(&unit, AnalysisCapability::DirectCalls),
            receipt(&unit, AnalysisCapability::OrmQuery),
        ];
        let graph = ExecutionGraph::new(&nodes, &edges, &receipts, &[]).unwrap();

        let paths = paths_from_entry(&graph, &entry.id, TraceLimits::selection()).unwrap();

        assert_eq!(paths.len(), 1);
        assert_eq!(
            paths[0].ordered_fact_ids,
            vec![
                entry.id,
                nodes[1].id.clone(),
                implementation.id,
                query.id,
                sql.id,
                table.id,
            ]
        );
        assert_eq!(
            paths[0].ordered_edge_ids,
            vec![
                contract_call.id,
                override_edge.id,
                query_call.id,
                executes_query.id,
                writes.id,
            ]
        );
        assert_eq!(paths[0].state, TracePathState::Gap);
    }

    #[test]
    fn table_reference_requires_exact_database_reconciliation_to_complete() {
        let unit = AnalysisUnitId::from_components(&["typescript", "database-reference"]).unwrap();
        let entry = node(FactNodeKind::Function, "saveOrder", &unit);
        let query = node(FactNodeKind::Query, "INSERT INTO orders", &unit);
        let reference = node(FactNodeKind::TableReference, "orders", &unit);
        let table = node(FactNodeKind::Table, "database.public.orders", &unit);
        let executes = edge(
            FactEdgeKind::ExecutesQuery,
            &entry,
            &query,
            FactTruth::Confirmed,
            DispatchKind::NotApplicable,
        );
        let writes = edge(
            FactEdgeKind::Writes,
            &query,
            &reference,
            FactTruth::Confirmed,
            DispatchKind::NotApplicable,
        );
        let maps = edge(
            FactEdgeKind::MapsToTable,
            &reference,
            &table,
            FactTruth::Confirmed,
            DispatchKind::NotApplicable,
        );
        let receipts = [receipt(&unit, AnalysisCapability::OrmQuery)];

        let unresolved_nodes = [entry.clone(), query.clone(), reference.clone()];
        let unresolved_edges = [executes.clone(), writes.clone()];
        let unresolved_graph =
            ExecutionGraph::new(&unresolved_nodes, &unresolved_edges, &receipts, &[]).unwrap();
        let unresolved =
            paths_from_entry(&unresolved_graph, &entry.id, TraceLimits::selection()).unwrap();
        assert_eq!(unresolved[0].state, TracePathState::Gap);
        assert_eq!(
            unresolved[0].ordered_fact_ids,
            vec![entry.id.clone(), query.id.clone(), reference.id.clone()]
        );

        let reconciled_nodes = [entry.clone(), query, reference, table.clone()];
        let reconciled_edges = [executes, writes, maps.clone()];
        let reconciled_graph =
            ExecutionGraph::new(&reconciled_nodes, &reconciled_edges, &receipts, &[]).unwrap();
        let reconciled =
            paths_from_entry(&reconciled_graph, &entry.id, TraceLimits::selection()).unwrap();
        assert_eq!(reconciled[0].state, TracePathState::Complete);
        assert_eq!(reconciled[0].ordered_fact_ids.last(), Some(&table.id));
        assert_eq!(reconciled[0].ordered_edge_ids.last(), Some(&maps.id));
    }

    #[test]
    fn repeated_calls_to_one_target_remain_distinct_and_use_lexical_order() {
        let unit = AnalysisUnitId::from_components(&["typescript", "repeat-call"]).unwrap();
        let entry = node(FactNodeKind::Entrypoint, "main", &unit);
        let target = node(FactNodeKind::Function, "render", &unit);
        let first = edge_at(
            FactEdgeKind::Calls,
            &entry,
            &target,
            FactTruth::Confirmed,
            DispatchKind::Direct,
            0,
            Default::default(),
        );
        let second = edge_at(
            FactEdgeKind::Calls,
            &entry,
            &target,
            FactTruth::Confirmed,
            DispatchKind::Direct,
            1,
            Default::default(),
        );
        let nodes = [entry.clone(), target.clone()];
        let edges = [second.clone(), first.clone()];
        let receipts = [receipt(&unit, AnalysisCapability::DirectCalls)];
        let graph = ExecutionGraph::new(&nodes, &edges, &receipts, &[]).unwrap();

        let paths = paths_from_entry(&graph, &entry.id, TraceLimits::selection()).unwrap();

        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0].ordered_edge_ids, vec![first.id]);
        assert_eq!(paths[1].ordered_edge_ids, vec![second.id]);
        assert_eq!(
            paths[0].ordered_fact_ids,
            vec![entry.id.clone(), target.id.clone()]
        );
        assert_eq!(paths[1].ordered_fact_ids, vec![entry.id, target.id]);
    }

    #[test]
    fn deferred_and_legacy_calls_are_not_claimed_as_immediate_execution_hops() {
        let unit = AnalysisUnitId::from_components(&["typescript", "deferred-call"]).unwrap();
        let entry = node(FactNodeKind::Entrypoint, "main", &unit);
        let target = node(FactNodeKind::Function, "later", &unit);
        let deferred = edge_at(
            FactEdgeKind::Calls,
            &entry,
            &target,
            FactTruth::Confirmed,
            DispatchKind::Direct,
            0,
            ExecutionControlContext {
                deferred: true,
                ..Default::default()
            },
        );
        let mut legacy = edge_at(
            FactEdgeKind::Calls,
            &entry,
            &target,
            FactTruth::Confirmed,
            DispatchKind::Direct,
            1,
            Default::default(),
        );
        legacy.execution = None;
        legacy.id = FactEdge::stable_id(
            &legacy.source_id,
            &legacy.target_id,
            legacy.kind,
            legacy.semantic_context_id.as_ref(),
            legacy.qualifier.as_deref(),
            None,
        )
        .unwrap();
        let nodes = [entry.clone(), target];
        let receipts = [receipt(&unit, AnalysisCapability::DirectCalls)];

        for blocked in [deferred, legacy] {
            let edges = [blocked];
            let graph = ExecutionGraph::new(&nodes, &edges, &receipts, &[]).unwrap();
            let paths = paths_from_entry(&graph, &entry.id, TraceLimits::selection()).unwrap();
            assert!(paths[0].ordered_edge_ids.is_empty());
            assert_eq!(paths[0].state, TracePathState::Gap);
        }
    }

    #[test]
    fn a_real_path_stays_visible_but_is_not_called_complete_when_a_sibling_branch_is_unresolved() {
        let unit = AnalysisUnitId::from_components(&["java", "orders-branch"]).unwrap();
        let entry = node(FactNodeKind::Entrypoint, "OrdersApplication.main", &unit);
        let resolved = node(FactNodeKind::Method, "OrdersService.run", &unit);
        let unresolved = node(FactNodeKind::Method, "Plugin.run", &unit);
        let direct = edge(
            FactEdgeKind::Calls,
            &entry,
            &resolved,
            FactTruth::Confirmed,
            DispatchKind::Direct,
        );
        let virtual_call = edge(
            FactEdgeKind::Calls,
            &entry,
            &unresolved,
            FactTruth::Confirmed,
            DispatchKind::Virtual,
        );
        let nodes = [entry.clone(), resolved.clone(), unresolved];
        let edges = [direct.clone(), virtual_call.clone()];
        let receipts = [receipt(&unit, AnalysisCapability::DirectCalls)];
        let graph = ExecutionGraph::new(&nodes, &edges, &receipts, &[]).unwrap();

        let paths = paths_from_entry(&graph, &entry.id, TraceLimits::selection()).unwrap();

        assert_eq!(paths.len(), 2);
        let direct_path = paths
            .iter()
            .find(|path| path.ordered_edge_ids == vec![direct.id.clone()])
            .unwrap();
        let candidate_path = paths
            .iter()
            .find(|path| path.ordered_edge_ids == vec![virtual_call.id.clone()])
            .unwrap();
        assert_eq!(
            direct_path.ordered_fact_ids,
            vec![entry.id.clone(), resolved.id]
        );
        assert_eq!(direct_path.state, TracePathState::Gap);
        assert_eq!(candidate_path.state, TracePathState::Gap);
    }

    #[test]
    fn evidence_scoped_framework_gap_does_not_taint_another_resolved_route_in_the_same_unit() {
        use codebase_fact_model::{coverage::GapCode, source::RepositoryPath};

        let unit = AnalysisUnitId::from_components(&["typescript", "two-routes"]).unwrap();
        let health_route = node(FactNodeKind::HttpRoute, "GET /health", &unit);
        let health_handler = node(FactNodeKind::Function, "health", &unit);
        let unknown_route = node(FactNodeKind::HttpRoute, "GET /unknown", &unit);
        let handles = edge(
            FactEdgeKind::Handles,
            &health_handler,
            &health_route,
            FactTruth::Confirmed,
            DispatchKind::NotApplicable,
        );
        let mut framework_receipt = receipt(&unit, AnalysisCapability::FrameworkBindings);
        framework_receipt.execution_state = CapabilityExecutionState::Partial;
        let receipts = [
            framework_receipt,
            receipt(&unit, AnalysisCapability::DirectCalls),
        ];
        let gap = AnalysisGap {
            code: GapCode::UnresolvedTarget,
            scope: AnalysisScope::File {
                unit_id: Some(unit.clone()),
                path: RepositoryPath::parse("src/app.ts").unwrap(),
            },
            capability: Some(AnalysisCapability::FrameworkBindings),
            evidence_ids: unknown_route.evidence_ids.clone(),
            message: "unknown route handler was not exact".to_string(),
        };
        let nodes = [health_route.clone(), health_handler, unknown_route.clone()];
        let edges = [handles];
        let gaps = [gap];
        let graph = ExecutionGraph::new(&nodes, &edges, &receipts, &gaps).unwrap();

        let health = paths_from_entry(&graph, &health_route.id, TraceLimits::selection()).unwrap();
        let unknown =
            paths_from_entry(&graph, &unknown_route.id, TraceLimits::selection()).unwrap();

        assert_eq!(health[0].state, TracePathState::Complete);
        assert_eq!(unknown[0].state, TracePathState::Gap);
    }

    #[test]
    fn representative_entry_budget_is_distributed_across_owners_before_second_paths() {
        let unit = AnalysisUnitId::from_components(&["typescript", "fairness"]).unwrap();
        let first = node(FactNodeKind::HttpRoute, "GET /a", &unit);
        let second = node(FactNodeKind::HttpRoute, "GET /b", &unit);
        let third = node(FactNodeKind::HttpRoute, "GET /c", &unit);
        let job = node(FactNodeKind::Job, "nightly", &unit);
        let nodes = [first.clone(), second.clone(), third.clone(), job.clone()];
        let graph = ExecutionGraph::new(&nodes, &[], &[], &[]).unwrap();
        let owners = [
            (first.id.clone(), 0_u8),
            (second.id.clone(), 0_u8),
            (third.id.clone(), 0_u8),
            (job.id.clone(), 1_u8),
        ]
        .into_iter()
        .collect::<BTreeMap<_, _>>();

        let selected = select_entrypoints_by_owner(&graph, &owners, 2);
        let selected_owners = selected
            .iter()
            .map(|node| owners[&node.id])
            .collect::<BTreeSet<_>>();

        assert_eq!(selected.len(), 2);
        assert_eq!(selected_owners, BTreeSet::from([0_u8, 1_u8]));
        assert_eq!(
            round_robin_take(&[vec!["a1", "a2"], vec!["b1", "b2"]], 2, 3),
            vec!["a1", "b1", "a2"]
        );
    }

    #[test]
    fn type_and_structure_relations_never_become_execution_steps() {
        let unit = AnalysisUnitId::from_components(&["rust", "app"]).unwrap();
        let entry = node(FactNodeKind::Entrypoint, "app::main", &unit);
        let imported = node(FactNodeKind::Module, "app::helpers", &unit);
        let import = edge(
            FactEdgeKind::Imports,
            &entry,
            &imported,
            FactTruth::Confirmed,
            DispatchKind::NotApplicable,
        );
        let nodes = [entry.clone(), imported];
        let edges = [import];
        let receipts = [receipt(&unit, AnalysisCapability::DirectCalls)];
        let graph = ExecutionGraph::new(&nodes, &edges, &receipts, &[]).unwrap();

        let paths = paths_from_entry(&graph, &entry.id, TraceLimits::selection()).unwrap();

        assert_eq!(paths[0].ordered_fact_ids, vec![entry.id]);
        assert_eq!(paths[0].state, TracePathState::Complete);
    }

    #[test]
    fn cycles_are_explicit_and_repeat_only_the_closing_fact() {
        let unit = AnalysisUnitId::from_components(&["go", "worker"]).unwrap();
        let entry = node(FactNodeKind::Job, "worker.Run", &unit);
        let first = node(FactNodeKind::Function, "worker.first", &unit);
        let second = node(FactNodeKind::Function, "worker.second", &unit);
        let first_edge = edge(
            FactEdgeKind::Calls,
            &entry,
            &first,
            FactTruth::Confirmed,
            DispatchKind::Direct,
        );
        let second_edge = edge(
            FactEdgeKind::Calls,
            &first,
            &second,
            FactTruth::Confirmed,
            DispatchKind::Direct,
        );
        let closing_edge = edge(
            FactEdgeKind::Calls,
            &second,
            &first,
            FactTruth::Confirmed,
            DispatchKind::Direct,
        );
        let nodes = [entry.clone(), first.clone(), second.clone()];
        let edges = [first_edge, second_edge, closing_edge];
        let receipts = [receipt(&unit, AnalysisCapability::DirectCalls)];
        let graph = ExecutionGraph::new(&nodes, &edges, &receipts, &[]).unwrap();

        let first_run = paths_from_entry(&graph, &entry.id, TraceLimits::selection()).unwrap();
        let second_run = paths_from_entry(&graph, &entry.id, TraceLimits::selection()).unwrap();

        assert_eq!(first_run, second_run);
        assert_eq!(first_run[0].state, TracePathState::Cycle);
        assert_eq!(
            first_run[0].ordered_fact_ids,
            vec![entry.id, first.id.clone(), second.id, first.id]
        );
    }

    #[test]
    #[ignore = "requires CODEBASE_TRACE_MANIFEST from a real code-engine run"]
    fn real_canonical_express_bundle_keeps_resolved_and_unresolved_routes_distinct() {
        use crate::fact_graph::CanonicalFactBundleArtifact;
        use codebase_fact_model::fact_graph::FactBundleManifest;
        use std::{env, fs, path::PathBuf};

        let manifest_path = PathBuf::from(env::var("CODEBASE_TRACE_MANIFEST").unwrap());
        let manifest: FactBundleManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        let bundle_path = manifest_path
            .parent()
            .unwrap()
            .join(format!("canonical-{}.sqlite", manifest.bundle_digest));
        let app_data = env::temp_dir().join(format!(
            "codebase-workspace-static-trace-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&app_data);
        let artifact = CanonicalFactBundleArtifact {
            schema: "codebase-workspace.canonical-fact-bundle-artifact.v1".to_string(),
            snapshot_id: manifest.snapshot_id.clone(),
            semantic_digest: manifest.semantic_digest,
            bundle_digest: manifest.bundle_digest,
            bundle_path,
            manifest_path,
        };
        crate::fact_graph::import_and_publish(&app_data, manifest.workspace_id.as_str(), &artifact)
            .unwrap();
        let reader =
            crate::fact_graph::open_published_read_model(&app_data, manifest.workspace_id.as_str())
                .unwrap()
                .unwrap();
        let snapshot = reader.semantic_analysis_snapshot().unwrap();

        let owners = snapshot
            .nodes
            .iter()
            .map(|node| (node.id.clone(), 0_u8))
            .collect::<BTreeMap<_, _>>();
        let traces = representative_trace_paths(&snapshot, &owners).unwrap();
        let route_by_path = snapshot
            .nodes
            .iter()
            .filter_map(|node| match node.details.as_ref() {
                Some(FactNodeDetails::HttpRoute { path, .. }) => Some((path.as_str(), &node.id)),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        let health = traces
            .iter()
            .find(|trace| Some(&trace.entry_fact_id) == route_by_path.get("/health").copied())
            .unwrap();
        let unresolved = traces
            .iter()
            .find(|trace| Some(&trace.entry_fact_id) == route_by_path.get("/unknown").copied())
            .unwrap();

        assert_eq!(health.ordered_fact_ids.len(), 2);
        assert_eq!(health.ordered_edge_ids.len(), 1);
        assert_eq!(
            snapshot
                .edges
                .iter()
                .find(|edge| edge.id == health.ordered_edge_ids[0])
                .unwrap()
                .kind,
            FactEdgeKind::Handles
        );
        assert_eq!(unresolved.ordered_fact_ids.len(), 1);
        assert!(unresolved.ordered_edge_ids.is_empty());
        assert_eq!(unresolved.state, TracePathState::Gap);
        fs::remove_dir_all(app_data).unwrap();
    }

    fn node(kind: FactNodeKind, name: &str, unit: &AnalysisUnitId) -> FactNode {
        let evidence_id = EvidenceId::from_components(&["node", name]).unwrap();
        let details = (kind == FactNodeKind::HttpRoute).then(|| FactNodeDetails::HttpRoute {
            method: "GET".to_string(),
            path: "/orders".to_string(),
        });
        let id = FactNode::stable_id(
            kind,
            Some(ProgrammingLanguage::TypeScript),
            Some(unit),
            name,
            None,
        )
        .unwrap();
        FactNode {
            id,
            snapshot_id: snapshot(),
            family: kind.family(),
            kind,
            native_kind: None,
            qualified_name: name.to_string(),
            display_name: name.to_string(),
            signature: None,
            details,
            visibility: Visibility::Public,
            language: Some(ProgrammingLanguage::TypeScript),
            analysis_unit_id: Some(unit.clone()),
            parent_id: None,
            definition_evidence_id: Some(evidence_id.clone()),
            evidence_ids: vec![evidence_id],
            roles: Vec::new(),
            flags: SourceFlags::default(),
        }
    }

    fn edge(
        kind: FactEdgeKind,
        source: &FactNode,
        target: &FactNode,
        truth: FactTruth,
        dispatch: DispatchKind,
    ) -> FactEdge {
        edge_at(kind, source, target, truth, dispatch, 0, Default::default())
    }

    fn edge_at(
        kind: FactEdgeKind,
        source: &FactNode,
        target: &FactNode,
        truth: FactTruth,
        dispatch: DispatchKind,
        lexical_ordinal: u32,
        control: ExecutionControlContext,
    ) -> FactEdge {
        let context = SemanticContextId::from_components(&["test"]).unwrap();
        let lexical_ordinal_text = lexical_ordinal.to_string();
        let evidence_id = EvidenceId::from_components(&[
            "edge",
            source.id.as_str(),
            target.id.as_str(),
            kind.as_str(),
            &lexical_ordinal_text,
        ])
        .unwrap();
        let execution = matches!(kind, FactEdgeKind::Calls | FactEdgeKind::Constructs).then(|| {
            ExecutionOccurrence {
                call_site_evidence_id: evidence_id.clone(),
                lexical_ordinal,
                control,
            }
        });
        let id = FactEdge::stable_id(
            &source.id,
            &target.id,
            kind,
            Some(&context),
            None,
            execution.as_ref(),
        )
        .unwrap();
        FactEdge {
            id,
            snapshot_id: snapshot(),
            source_id: source.id.clone(),
            target_id: target.id.clone(),
            family: kind.family(),
            kind,
            truth,
            resolution: ResolutionMethod::Provider,
            dispatch,
            semantic_context_id: Some(context),
            qualifier: None,
            execution,
            evidence_ids: vec![evidence_id],
        }
    }

    fn receipt(unit: &AnalysisUnitId, capability: AnalysisCapability) -> CapabilityReceipt {
        CapabilityReceipt {
            unit_id: unit.clone(),
            capability,
            declared_support: DeclaredSupport::Conditional,
            execution_state: CapabilityExecutionState::Complete,
            precision: EvidencePrecision::ExactRange,
            denominator: CoverageDenominator::Known { eligible_count: 1 },
            covered_count: 1,
            emitted_fact_count: 0,
            emitted_relation_count: 1,
            truncated_count: 0,
            gap_codes: Vec::new(),
        }
    }

    fn snapshot() -> SnapshotId {
        SnapshotId::from_components(&["trace-path-tests"]).unwrap()
    }
}
