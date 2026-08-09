use crate::{
    fact_graph,
    semantic::store,
    static_query::{self, TraceLimits},
    workspace::Workspace,
};
use codebase_fact_model::{
    evidence::EvidenceLocation,
    fact_graph::{FactEdgeFamily, FactNode, FactNodeKind, FactRole, FactTruth},
    identity::{EvidenceId, FactNodeId},
};
use codebase_semantic_model::{
    ApprovedSemanticArea, RegionId, SemanticAreaId, StaticRegionSummary, TracePathId,
    TracePathState, TracePathSummary,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MapView {
    pub areas: Vec<MapArea>,
    pub relations: Vec<MapRelation>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MapArea {
    pub id: String,
    pub name: String,
    pub original_name: Option<String>,
    pub summary: String,
    pub depth: u8,
    pub areas: Vec<MapArea>,
    pub nodes: Vec<MapNode>,
    pub hidden_node_count: usize,
    pub position: Option<MapPosition>,
    pub width: Option<u32>,
    pub trace: Option<MapTraceMeta>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MapTraceMeta {
    pub id: String,
    pub state: &'static str,
    pub step_count: usize,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MapPosition {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MapNode {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub role: &'static str,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MapRelation {
    pub id: String,
    pub from: String,
    pub to: String,
    pub truth: &'static str,
    pub label: String,
    pub count: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MapSelection {
    pub id: String,
    pub title: String,
    pub role: String,
    pub relations: Vec<RelationTally>,
    pub evidence: Vec<EvidenceRef>,
    pub source: Option<SourceExcerpt>,
    pub traces: Vec<MapTrace>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MapTrace {
    pub id: String,
    pub state: &'static str,
    pub steps: Vec<MapNode>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RelationTally {
    pub label: String,
    pub truth: &'static str,
    pub count: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EvidenceRef {
    pub path: String,
    pub line: Option<u32>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SourceExcerpt {
    pub path: String,
    pub start_line: u32,
    pub lines: Vec<String>,
    pub hit_line: u32,
}

pub(crate) fn get_map_view(
    app_data_dir: &Path,
    workspace: &Workspace,
) -> Result<Option<MapView>, String> {
    let Some(stored) = store::load_current(app_data_dir, &workspace.id)? else {
        return Ok(None);
    };
    let Some(facts) = fact_graph::open_published_read_model(app_data_dir, &workspace.id)? else {
        return Ok(None);
    };
    if facts.manifest.snapshot_id != stored.revision.snapshot_id {
        return Ok(None);
    }
    let fact_ids = stored
        .packet
        .input
        .anchors
        .iter()
        .map(|anchor| anchor.fact_id.clone())
        .chain(
            stored
                .packet
                .input
                .representative_traces
                .iter()
                .flat_map(|trace| trace.ordered_fact_ids.iter().cloned()),
        )
        .collect::<BTreeSet<_>>();
    let nodes = facts.nodes_by_ids(fact_ids)?;
    Ok(Some(project_map(&stored, &nodes)))
}

pub(crate) fn get_map_selection(
    app_data_dir: &Path,
    workspace: &Workspace,
    selected_id: &str,
) -> Result<Option<MapSelection>, String> {
    if selected_id.is_empty()
        || selected_id.len() > 160
        || selected_id.chars().any(char::is_control)
    {
        return Err("선택한 지도 ID가 올바르지 않습니다".to_string());
    }
    let Some(stored) = store::load_current(app_data_dir, &workspace.id)? else {
        return Ok(None);
    };
    let Some(facts) = fact_graph::open_published_read_model(app_data_dir, &workspace.id)? else {
        return Ok(None);
    };
    if facts.manifest.snapshot_id != stored.revision.snapshot_id {
        return Ok(None);
    }
    let area = stored
        .revision
        .areas
        .iter()
        .find(|area| area.area_id.as_str() == selected_id);
    let anchor = stored
        .packet
        .input
        .anchors
        .iter()
        .find(|anchor| anchor.fact_id.as_str() == selected_id);
    let selected_fact_id = FactNodeId::parse(selected_id.to_string()).ok();
    let selected_facts = if area.is_none() && anchor.is_none() {
        facts.nodes_by_ids(selected_fact_id.iter().cloned())?
    } else {
        Vec::new()
    };
    let fact = selected_facts.first();
    let (title, role, evidence_ids, owner_regions) = if let Some(area) = area {
        (
            area.label.clone(),
            area.summary.clone(),
            area.evidence_ids.clone(),
            area.effective_member_region_ids
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>(),
        )
    } else if let Some(anchor) = anchor {
        (
            anchor.name.clone(),
            display_kind(anchor.kind).to_string(),
            anchor.evidence_ids.clone(),
            [anchor.owner_region_id.clone()].into_iter().collect(),
        )
    } else if let Some(fact) = fact {
        let evidence_ids = fact
            .evidence_ids
            .iter()
            .chain(
                fact.roles
                    .iter()
                    .flat_map(|assignment| assignment.evidence_ids.iter()),
            )
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        (
            fact.display_name.clone(),
            display_kind(fact.kind).to_string(),
            evidence_ids,
            BTreeSet::new(),
        )
    } else {
        return Ok(None);
    };
    let evidence_rows = facts.evidence_by_ids(evidence_ids.iter().cloned())?;
    let evidence = evidence_refs(&evidence_rows, &evidence_ids);
    let source = stored
        .packet
        .input
        .excerpts
        .iter()
        .find(|excerpt| evidence_ids.contains(&excerpt.evidence_id))
        .map(|excerpt| SourceExcerpt {
            path: excerpt.relative_path.to_string(),
            start_line: excerpt.start_line,
            lines: excerpt.text.lines().map(str::to_string).collect(),
            hit_line: evidence
                .iter()
                .find(|item| item.path == excerpt.relative_path.as_str())
                .and_then(|item| item.line)
                .unwrap_or(excerpt.start_line),
        });
    let relations = selection_tallies(&stored.packet.input.boundary_relations, &owner_regions);
    let traces = if let Some(area) = area {
        let candidates = area_trace_candidates(
            area,
            &stored.packet.input.regions,
            &stored.packet.input.representative_traces,
        )
        .into_iter()
        .take(3)
        .collect::<Vec<_>>();
        let trace_nodes = facts.nodes_by_ids(
            candidates
                .iter()
                .flat_map(|trace| trace.ordered_fact_ids.iter().cloned()),
        )?;
        let facts_by_id = trace_nodes
            .iter()
            .map(|fact| (fact.id.clone(), fact))
            .collect::<BTreeMap<_, _>>();
        candidates
            .into_iter()
            .filter_map(|trace| project_trace(trace, &facts_by_id))
            .collect()
    } else if let Some(fact) = fact {
        let limits = TraceLimits::selection();
        let Some(trace_facts) =
            facts.trace_snapshot(&fact.id, limits.max_depth, limits.max_expansions_per_entry)?
        else {
            return Ok(None);
        };
        let facts_by_id = trace_facts
            .nodes
            .iter()
            .map(|fact| (fact.id.clone(), fact))
            .collect::<BTreeMap<_, _>>();
        static_query::trace_paths_from_fact(&trace_facts, &fact.id, limits)?
            .into_iter()
            .take(3)
            .filter_map(|trace| project_trace(&trace, &facts_by_id))
            .collect()
    } else {
        Vec::new()
    };
    Ok(Some(MapSelection {
        id: selected_id.to_string(),
        title,
        role,
        relations,
        evidence,
        source,
        traces,
    }))
}

fn project_map(stored: &store::StoredSemanticRevision, facts: &[FactNode]) -> MapView {
    let facts_by_id = facts
        .iter()
        .map(|fact| (fact.id.clone(), fact))
        .collect::<BTreeMap<_, _>>();
    let regions = stored
        .packet
        .input
        .regions
        .iter()
        .map(|region| (region.region_id.clone(), region))
        .collect::<BTreeMap<_, _>>();
    let anchors_by_region = stored.packet.input.anchors.iter().fold(
        BTreeMap::<RegionId, Vec<_>>::new(),
        |mut result, anchor| {
            result
                .entry(anchor.owner_region_id.clone())
                .or_default()
                .push(anchor);
            result
        },
    );
    let area_by_id = stored
        .revision
        .areas
        .iter()
        .map(|area| (area.area_id.clone(), area))
        .collect::<BTreeMap<_, _>>();
    let children = stored
        .revision
        .areas
        .iter()
        .filter_map(|area| {
            area.parent_area_id
                .as_ref()
                .map(|parent| (parent.clone(), area))
        })
        .fold(
            BTreeMap::<SemanticAreaId, Vec<_>>::new(),
            |mut map, pair| {
                map.entry(pair.0).or_default().push(pair.1);
                map
            },
        );
    let mut top = stored
        .revision
        .areas
        .iter()
        .filter(|area| area.parent_area_id.is_none())
        .collect::<Vec<_>>();
    top.sort_by(|left, right| left.area_id.cmp(&right.area_id));
    let areas = top
        .iter()
        .enumerate()
        .map(|(index, area)| {
            project_area(
                area,
                &children,
                &regions,
                &anchors_by_region,
                &stored.packet.input.representative_traces,
                &facts_by_id,
                Some(default_position(index)),
            )
        })
        .collect();
    let assignments = stored
        .revision
        .assignments
        .iter()
        .map(|assignment| (assignment.region_id.clone(), assignment.area_id.clone()))
        .collect::<BTreeMap<_, _>>();
    let relations = project_relations(
        &stored.packet.input.boundary_relations,
        &assignments,
        &area_by_id,
    );
    MapView { areas, relations }
}

fn project_area(
    area: &ApprovedSemanticArea,
    children: &BTreeMap<SemanticAreaId, Vec<&ApprovedSemanticArea>>,
    regions: &BTreeMap<RegionId, &StaticRegionSummary>,
    anchors_by_region: &BTreeMap<RegionId, Vec<&codebase_semantic_model::AnchorFactSummary>>,
    traces: &[TracePathSummary],
    facts_by_id: &BTreeMap<FactNodeId, &FactNode>,
    position: Option<MapPosition>,
) -> MapArea {
    let mut child_areas = children.get(&area.area_id).cloned().unwrap_or_default();
    child_areas.sort_by(|left, right| left.area_id.cmp(&right.area_id));
    let nested = child_areas
        .iter()
        .map(|child| {
            project_area(
                child,
                children,
                regions,
                anchors_by_region,
                traces,
                facts_by_id,
                None,
            )
        })
        .collect::<Vec<_>>();
    let mut anchors = area
        .direct_member_region_ids
        .iter()
        .flat_map(|region_id| anchors_by_region.get(region_id).into_iter().flatten())
        .collect::<Vec<_>>();
    anchors.sort_by(|left, right| left.fact_id.cmp(&right.fact_id));
    let representative_trace = nested
        .is_empty()
        .then(|| {
            area_trace_candidates(area, regions.values().copied(), traces)
                .into_iter()
                .next()
        })
        .flatten();
    let traced_nodes = representative_trace.and_then(|trace| project_trace(trace, facts_by_id));
    let nodes = traced_nodes
        .as_ref()
        .map(|trace| trace.steps.clone())
        .or_else(|| {
            anchors.first().map(|anchor| {
                vec![MapNode {
                    id: anchor.fact_id.to_string(),
                    name: anchor.name.clone(),
                    kind: display_kind(anchor.kind).to_string(),
                    role: node_role(anchor.kind, &anchor.static_roles),
                }]
            })
        })
        .unwrap_or_default();
    let visible_fact_ids = nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<BTreeSet<_>>();
    let hidden_node_count = anchors
        .iter()
        .filter(|anchor| !visible_fact_ids.contains(anchor.fact_id.as_str()))
        .count();
    let trace = traced_nodes.map(|trace| MapTraceMeta {
        id: trace.id,
        state: trace.state,
        step_count: trace.steps.len(),
    });
    let original_name = single_structural_label(area, regions).filter(|label| label != &area.label);
    let width = (!nested.is_empty()).then(|| {
        u32::try_from(nested.len())
            .unwrap_or(u32::MAX)
            .saturating_mul(224)
            .saturating_add(32)
            .max(232)
    });
    MapArea {
        id: area.area_id.to_string(),
        name: area.label.clone(),
        original_name,
        summary: area.summary.clone(),
        depth: area.level,
        areas: nested,
        nodes,
        hidden_node_count,
        position,
        width,
        trace,
    }
}

fn area_trace_candidates<'a, 'r, I>(
    area: &ApprovedSemanticArea,
    regions: I,
    traces: &'a [TracePathSummary],
) -> Vec<&'a TracePathSummary>
where
    I: IntoIterator<Item = &'r StaticRegionSummary>,
{
    let mut trace_regions = BTreeMap::<TracePathId, BTreeSet<RegionId>>::new();
    for region in regions {
        for trace_id in &region.representative_trace_path_ids {
            trace_regions
                .entry(trace_id.clone())
                .or_default()
                .insert(region.region_id.clone());
        }
    }
    let members = area
        .effective_member_region_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let approved_rank = area
        .representative_trace_path_ids
        .iter()
        .enumerate()
        .map(|(index, trace_id)| (trace_id, index))
        .collect::<BTreeMap<_, _>>();
    let mut result = traces
        .iter()
        .filter(|trace| {
            trace_regions
                .get(&trace.trace_path_id)
                .is_some_and(|owners| {
                    !owners.is_empty() && owners.iter().all(|region| members.contains(region))
                })
        })
        .collect::<Vec<_>>();
    result.sort_by(|left, right| {
        approved_rank
            .get(&left.trace_path_id)
            .copied()
            .unwrap_or(usize::MAX)
            .cmp(
                &approved_rank
                    .get(&right.trace_path_id)
                    .copied()
                    .unwrap_or(usize::MAX),
            )
            .then_with(|| left.trace_path_id.cmp(&right.trace_path_id))
    });
    result
}

fn project_trace(
    trace: &TracePathSummary,
    facts_by_id: &BTreeMap<FactNodeId, &FactNode>,
) -> Option<MapTrace> {
    let steps = trace
        .ordered_fact_ids
        .iter()
        .map(|fact_id| facts_by_id.get(fact_id).map(|fact| project_fact_node(fact)))
        .collect::<Option<Vec<_>>>()?;
    (!steps.is_empty()).then(|| MapTrace {
        id: trace.trace_path_id.to_string(),
        state: map_trace_state(trace.state),
        steps,
    })
}

fn project_fact_node(fact: &FactNode) -> MapNode {
    let roles = fact
        .roles
        .iter()
        .map(|assignment| assignment.role)
        .collect::<Vec<_>>();
    MapNode {
        id: fact.id.to_string(),
        name: fact.display_name.clone(),
        kind: display_kind(fact.kind).to_string(),
        role: node_role(fact.kind, &roles),
    }
}

fn map_trace_state(state: TracePathState) -> &'static str {
    match state {
        TracePathState::Complete => "complete",
        TracePathState::Partial => "partial",
        TracePathState::Gap => "gap",
        TracePathState::Cycle => "cycle",
        TracePathState::DepthLimited => "depth-limited",
    }
}

fn single_structural_label(
    area: &ApprovedSemanticArea,
    regions: &BTreeMap<RegionId, &StaticRegionSummary>,
) -> Option<String> {
    let labels = area
        .effective_member_region_ids
        .iter()
        .filter_map(|id| regions.get(id))
        .map(|region| region.structural_label.clone())
        .collect::<BTreeSet<_>>();
    (labels.len() == 1)
        .then(|| labels.into_iter().next())
        .flatten()
}

struct ProjectedRelation {
    count: u64,
    weakest_truth: FactTruth,
    family_counts: BTreeMap<FactEdgeFamily, u64>,
}

fn project_relations(
    bundles: &[codebase_semantic_model::BoundaryRelationSummary],
    assignments: &BTreeMap<RegionId, SemanticAreaId>,
    areas: &BTreeMap<SemanticAreaId, &ApprovedSemanticArea>,
) -> Vec<MapRelation> {
    let mut projected = BTreeMap::<(SemanticAreaId, SemanticAreaId), ProjectedRelation>::new();
    for bundle in bundles {
        let (Some(source), Some(target)) = (
            assignments.get(&bundle.source_region_id),
            assignments.get(&bundle.target_region_id),
        ) else {
            continue;
        };
        let (Some(source), Some(target)) = (top_area(source, areas), top_area(target, areas))
        else {
            continue;
        };
        if source == target {
            continue;
        }
        let entry =
            projected
                .entry((source.clone(), target.clone()))
                .or_insert(ProjectedRelation {
                    count: 0,
                    weakest_truth: FactTruth::Confirmed,
                    family_counts: BTreeMap::new(),
                });
        for count in &bundle.families {
            entry.count = entry.count.saturating_add(count.relation_count);
            *entry.family_counts.entry(count.family).or_default() += count.relation_count;
            if truth_rank(count.truth) > truth_rank(entry.weakest_truth) {
                entry.weakest_truth = count.truth;
            }
        }
    }
    projected
        .into_iter()
        .filter(|(_, relation)| relation.count > 0)
        .map(|((source, target), relation)| MapRelation {
            id: format!("relation:{}:{}", source, target),
            from: source.to_string(),
            to: target.to_string(),
            truth: map_truth(relation.weakest_truth),
            label: relation
                .family_counts
                .into_iter()
                .max_by(|left, right| {
                    left.1
                        .cmp(&right.1)
                        .then_with(|| family_rank(right.0).cmp(&family_rank(left.0)))
                })
                .map(|(family, _)| family_label(family).to_string())
                .unwrap_or_else(|| "관계".to_string()),
            count: relation.count,
        })
        .collect()
}

fn top_area(
    start: &SemanticAreaId,
    areas: &BTreeMap<SemanticAreaId, &ApprovedSemanticArea>,
) -> Option<SemanticAreaId> {
    let mut current = start.clone();
    let mut seen = BTreeSet::new();
    loop {
        if !seen.insert(current.clone()) {
            return None;
        }
        let area = areas.get(&current)?;
        let Some(parent) = &area.parent_area_id else {
            return Some(current);
        };
        current = parent.clone();
    }
}

fn selection_tallies(
    bundles: &[codebase_semantic_model::BoundaryRelationSummary],
    owner_regions: &BTreeSet<RegionId>,
) -> Vec<RelationTally> {
    let mut counts = BTreeMap::<(bool, u8, u8), (FactEdgeFamily, FactTruth, u64)>::new();
    for bundle in bundles {
        let outbound = owner_regions.contains(&bundle.source_region_id);
        let inbound = owner_regions.contains(&bundle.target_region_id);
        if !outbound && !inbound {
            continue;
        }
        for family in &bundle.families {
            if outbound {
                counts
                    .entry((true, family_rank(family.family), truth_rank(family.truth)))
                    .or_insert((family.family, family.truth, 0))
                    .2 += family.relation_count;
            }
            if inbound {
                counts
                    .entry((false, family_rank(family.family), truth_rank(family.truth)))
                    .or_insert((family.family, family.truth, 0))
                    .2 += family.relation_count;
            }
        }
    }
    counts
        .into_iter()
        .map(|((outbound, _, _), (family, truth, count))| RelationTally {
            label: format!(
                "{} ({})",
                family_label(family),
                if outbound { "나감" } else { "들어옴" }
            ),
            truth: map_truth(truth),
            count,
        })
        .collect()
}

fn evidence_refs(
    facts: &[codebase_fact_model::evidence::FactEvidence],
    evidence_ids: &[EvidenceId],
) -> Vec<EvidenceRef> {
    let wanted = evidence_ids.iter().collect::<BTreeSet<_>>();
    facts
        .iter()
        .filter(|evidence| wanted.contains(&evidence.id))
        .filter_map(|evidence| match &evidence.location {
            EvidenceLocation::Source { span } => Some(EvidenceRef {
                path: span.path.to_string(),
                line: Some(span.start.line.saturating_add(1)),
            }),
            EvidenceLocation::RepositoryArtifact { artifact } => Some(EvidenceRef {
                path: artifact.path.to_string(),
                line: None,
            }),
            EvidenceLocation::DatabaseCatalog { .. } => None,
        })
        .collect()
}

fn default_position(index: usize) -> MapPosition {
    let column = i32::try_from(index % 4).unwrap_or(0);
    let row = i32::try_from(index / 4).unwrap_or(0);
    MapPosition {
        x: 48 + column * 300,
        y: 96 + row * 300,
    }
}

fn display_kind(kind: FactNodeKind) -> &'static str {
    match kind {
        FactNodeKind::HttpRoute => "HTTP Endpoint",
        FactNodeKind::GraphqlEndpoint => "GraphQL Endpoint",
        FactNodeKind::RpcEndpoint => "RPC Endpoint",
        FactNodeKind::Class => "Class",
        FactNodeKind::Interface => "Interface",
        FactNodeKind::Trait => "Trait",
        FactNodeKind::Struct => "Struct",
        FactNodeKind::Function => "Function",
        FactNodeKind::Method => "Method",
        FactNodeKind::Constructor => "Constructor",
        FactNodeKind::Table => "Table",
        FactNodeKind::Query => "Query",
        FactNodeKind::Event => "Event",
        FactNodeKind::Queue | FactNodeKind::Topic => "Message Boundary",
        FactNodeKind::ExternalService => "External Service",
        FactNodeKind::Cache => "Cache",
        FactNodeKind::TestCase => "Test",
        _ => "Code",
    }
}

fn node_role(kind: FactNodeKind, roles: &[FactRole]) -> &'static str {
    if kind == FactNodeKind::HttpRoute {
        return "endpoint";
    }
    if kind == FactNodeKind::Table {
        return "table";
    }
    if kind == FactNodeKind::Event {
        return "event";
    }
    if roles.contains(&FactRole::Controller) || roles.contains(&FactRole::Handler) {
        return "controller";
    }
    if roles.contains(&FactRole::Service) {
        return "service";
    }
    if roles.contains(&FactRole::Repository) || roles.contains(&FactRole::DataAccess) {
        return "repository";
    }
    "code"
}

fn map_truth(truth: FactTruth) -> &'static str {
    match truth {
        FactTruth::Confirmed => "verified",
        FactTruth::Structural => "structural",
        FactTruth::StaticCandidate => "candidate",
    }
}

fn truth_rank(truth: FactTruth) -> u8 {
    match truth {
        FactTruth::Confirmed => 0,
        FactTruth::Structural => 1,
        FactTruth::StaticCandidate => 2,
    }
}

fn family_rank(family: FactEdgeFamily) -> u8 {
    match family {
        FactEdgeFamily::Structure => 0,
        FactEdgeFamily::Code => 1,
        FactEdgeFamily::Interface => 2,
        FactEdgeFamily::Data => 3,
        FactEdgeFamily::Integration => 4,
        FactEdgeFamily::Verification => 5,
    }
}

fn family_label(family: FactEdgeFamily) -> &'static str {
    match family {
        FactEdgeFamily::Structure => "구조",
        FactEdgeFamily::Code => "호출",
        FactEdgeFamily::Interface => "인터페이스",
        FactEdgeFamily::Data => "데이터",
        FactEdgeFamily::Integration => "통합",
        FactEdgeFamily::Verification => "테스트",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codebase_fact_model::{
        analysis::ProgrammingLanguage,
        fact_graph::Visibility,
        identity::{AnalysisUnitId, FactEdgeId, SnapshotId},
        source::SourceFlags,
    };

    #[test]
    fn map_chain_uses_exact_trace_order_and_fails_closed_on_a_missing_fact() {
        let unit = AnalysisUnitId::from_components(&["map-view", "typescript"]).unwrap();
        let route = fact_node(FactNodeKind::HttpRoute, "GET /orders", &unit);
        let handler = fact_node(FactNodeKind::Function, "getOrders", &unit);
        let edge_id = FactEdgeId::from_components(&["map-view", "route-handler"]).unwrap();
        let trace = TracePathSummary {
            trace_path_id: TracePathSummary::stable_id(&route.id, std::slice::from_ref(&edge_id))
                .unwrap(),
            entry_fact_id: route.id.clone(),
            ordered_fact_ids: vec![route.id.clone(), handler.id.clone()],
            ordered_edge_ids: vec![edge_id],
            state: TracePathState::Complete,
            evidence_ids: vec![route.evidence_ids[0].clone()],
        };
        let facts = [&route, &handler]
            .into_iter()
            .map(|fact| (fact.id.clone(), fact))
            .collect::<BTreeMap<_, _>>();

        let projected = project_trace(&trace, &facts).unwrap();
        assert_eq!(
            projected
                .steps
                .iter()
                .map(|step| step.name.as_str())
                .collect::<Vec<_>>(),
            vec!["GET /orders", "getOrders"]
        );
        assert_eq!(projected.state, "complete");

        let incomplete_facts = [(&route.id, &route)]
            .into_iter()
            .map(|(id, fact)| (id.clone(), fact))
            .collect::<BTreeMap<_, _>>();
        assert!(project_trace(&trace, &incomplete_facts).is_none());
    }

    fn fact_node(kind: FactNodeKind, name: &str, unit: &AnalysisUnitId) -> FactNode {
        let id = FactNode::stable_id(
            kind,
            Some(ProgrammingLanguage::TypeScript),
            Some(unit),
            name,
            None,
        )
        .unwrap();
        let evidence_id = EvidenceId::from_components(&["map-view", name]).unwrap();
        FactNode {
            id,
            snapshot_id: SnapshotId::from_components(&["map-view"]).unwrap(),
            family: kind.family(),
            kind,
            native_kind: None,
            qualified_name: name.to_string(),
            display_name: name.to_string(),
            signature: None,
            details: None,
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
}
