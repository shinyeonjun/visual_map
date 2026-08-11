use crate::{
    fact_graph,
    semantic::store,
    static_query::{self, TraceLimits},
    workspace::Workspace,
};
use codebase_fact_model::{
    coverage::{AnalysisGap, AnalysisScope, FileCoverageRecord},
    evidence::{EvidenceLocation, FactEvidence},
    fact_graph::{
        DispatchKind, FactEdge, FactEdgeFamily, FactNode, FactNodeKind, FactRole, FactTruth,
    },
    identity::{AnalysisUnitId, EvidenceId, FactEdgeId, FactNodeId},
    source::RepositoryPath,
};
use codebase_semantic_model::{
    ApprovedSemanticArea, AreaCategory, BoundaryRelationSummary, LabelSource, RegionId,
    SemanticAreaId, SemanticFallbackReason, StaticRegionSummary, TracePathId, TracePathState,
    TracePathSummary,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

const MAX_SELECTION_ANALYSIS_GAPS: usize = 16;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MapView {
    pub areas: Vec<MapArea>,
    pub relations: Vec<MapRelation>,
    /// Canonical gaps whose scope cannot be assigned to any published semantic
    /// area (for example a workspace-wide provider failure).
    pub unattributed_analysis_gap_count: usize,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MapArea {
    pub id: String,
    pub name: String,
    pub original_name: Option<String>,
    pub summary: String,
    pub category: &'static str,
    pub label_source: &'static str,
    pub fallback_reason: Option<&'static str>,
    pub depth: u8,
    pub areas: Vec<MapArea>,
    pub nodes: Vec<MapNode>,
    pub hidden_node_count: usize,
    pub position: Option<MapPosition>,
    pub width: Option<u32>,
    pub trace: Option<MapTraceMeta>,
    /// Relations that cross this area's effective member boundary. Internal
    /// region-to-region relations are deliberately excluded.
    pub boundary_relation_counts: MapTruthCounts,
    /// Number of canonical analysis-gap records whose scope overlaps at least
    /// one effective member region. Parent/child area counts are not additive.
    pub affecting_analysis_gap_count: usize,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MapTruthCounts {
    pub verified: u64,
    pub structural: u64,
    pub candidate: u64,
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
    /// Exact definition location for this node. Call-site evidence is never
    /// substituted because it belongs to the caller, not the target symbol.
    pub definition: Option<EvidenceRef>,
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
    pub dispatches: Vec<MapDispatchTally>,
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
    pub analysis_gaps: MapAnalysisGapSummary,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MapAnalysisGapSummary {
    pub total_count: usize,
    pub items: Vec<MapAnalysisGap>,
    pub truncated_count: usize,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MapAnalysisGap {
    pub code: &'static str,
    pub capability: Option<&'static str>,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MapTrace {
    pub id: String,
    pub state: &'static str,
    pub steps: Vec<MapNode>,
    pub hops: Vec<MapTraceHop>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MapTraceHop {
    pub id: String,
    pub from: String,
    pub to: String,
    pub kind: String,
    pub truth: &'static str,
    pub dispatch: &'static str,
    pub evidence: Vec<EvidenceRef>,
    pub execution: Option<MapExecutionOccurrence>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MapExecutionOccurrence {
    pub call_site_evidence_id: String,
    pub call_site: Option<EvidenceRef>,
    pub lexical_ordinal: u32,
    pub guarded: bool,
    pub repeated: bool,
    pub deferred: bool,
    pub awaited: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RelationTally {
    pub label: String,
    pub truth: &'static str,
    pub dispatch: &'static str,
    pub count: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MapDispatchTally {
    pub dispatch: &'static str,
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
    let definition_evidence = facts.evidence_by_ids(
        nodes
            .iter()
            .filter_map(|node| node.definition_evidence_id.clone()),
    )?;
    let gap_snapshot = facts.gap_attribution_snapshot()?;
    Ok(Some(project_map(
        &stored,
        &nodes,
        &definition_evidence,
        &gap_snapshot.file_coverage,
        &gap_snapshot.gaps,
    )))
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
    let gap_snapshot = facts.gap_attribution_snapshot()?;
    let analysis_gaps = selection_gap_summary(
        &gap_snapshot.gaps,
        &gap_snapshot.file_coverage,
        &stored.packet.input.regions,
        &owner_regions,
        fact,
        &evidence_rows,
    );
    let trace_limit = TraceLimits::selection().max_total_paths;
    let traces = if let Some(area) = area {
        let member_regions = area
            .effective_member_region_ids
            .iter()
            .collect::<BTreeSet<_>>();
        let mut entrypoints = stored
            .packet
            .input
            .anchors
            .iter()
            .filter(|anchor| member_regions.contains(&anchor.owner_region_id))
            .filter_map(|anchor| {
                selection_entry_rank(anchor.kind).map(|rank| (rank, anchor.fact_id.clone()))
            })
            .collect::<Vec<_>>();
        if entrypoints.is_empty() {
            entrypoints.extend(
                area_trace_candidates(
                    area,
                    &stored.packet.input.regions,
                    &stored.packet.input.representative_traces,
                )
                .into_iter()
                .map(|trace| (u8::MAX, trace.entry_fact_id.clone())),
            );
        }
        entrypoints.sort();
        entrypoints.dedup_by(|left, right| left.1 == right.1);
        entrypoints.truncate(4);

        let limits = TraceLimits::selection();
        let mut trace_groups = Vec::new();
        for (_, entrypoint) in entrypoints {
            let Some(trace_facts) = facts.trace_snapshot(
                &entrypoint,
                limits.max_depth,
                limits.max_expansions_per_entry,
            )?
            else {
                continue;
            };
            trace_groups.push(static_query::trace_paths_from_fact(
                &trace_facts,
                &entrypoint,
                limits,
            )?);
        }
        let candidates = round_robin_traces(&trace_groups, trace_limit);
        let trace_nodes = facts.nodes_by_ids(
            candidates
                .iter()
                .flat_map(|trace| trace.ordered_fact_ids.iter().cloned()),
        )?;
        let facts_by_id = trace_nodes
            .iter()
            .map(|fact| (fact.id.clone(), fact))
            .collect::<BTreeMap<_, _>>();
        let trace_edges = facts.edges_by_ids(
            candidates
                .iter()
                .flat_map(|trace| trace.ordered_edge_ids.iter().cloned()),
        )?;
        let edges_by_id = trace_edges
            .iter()
            .map(|edge| (edge.id.clone(), edge))
            .collect::<BTreeMap<_, _>>();
        let trace_evidence = facts.evidence_by_ids(
            trace_edges
                .iter()
                .flat_map(|edge| edge.evidence_ids.iter().cloned())
                .chain(
                    trace_nodes
                        .iter()
                        .filter_map(|node| node.definition_evidence_id.clone()),
                ),
        )?;
        candidates
            .iter()
            .filter_map(|trace| project_trace(trace, &facts_by_id, &edges_by_id, &trace_evidence))
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
        let edges_by_id = trace_facts
            .edges
            .iter()
            .map(|edge| (edge.id.clone(), edge))
            .collect::<BTreeMap<_, _>>();
        let trace_evidence = facts.evidence_by_ids(
            trace_facts
                .edges
                .iter()
                .flat_map(|edge| edge.evidence_ids.iter().cloned())
                .chain(
                    trace_facts
                        .nodes
                        .iter()
                        .filter_map(|node| node.definition_evidence_id.clone()),
                ),
        )?;
        static_query::trace_paths_from_fact(&trace_facts, &fact.id, limits)?
            .into_iter()
            .take(trace_limit)
            .filter_map(|trace| project_trace(&trace, &facts_by_id, &edges_by_id, &trace_evidence))
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
        analysis_gaps,
    }))
}

fn project_map(
    stored: &store::StoredSemanticRevision,
    facts: &[FactNode],
    definition_evidence: &[FactEvidence],
    file_coverage: &[FileCoverageRecord],
    gaps: &[AnalysisGap],
) -> MapView {
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
    let gap_regions = attribute_gap_regions(gaps, file_coverage, &stored.packet.input.regions);
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
    let projection = MapProjectionContext {
        children: &children,
        regions: &regions,
        anchors_by_region: &anchors_by_region,
        traces: &stored.packet.input.representative_traces,
        boundary_relations: &stored.packet.input.boundary_relations,
        gap_regions: &gap_regions,
        facts_by_id: &facts_by_id,
        definition_evidence,
    };
    let mut areas = top
        .iter()
        .map(|area| project_area(area, &projection, None))
        .collect::<Vec<_>>();
    assign_default_positions(&mut areas);
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
    let assigned_regions = stored
        .revision
        .areas
        .iter()
        .flat_map(|area| area.effective_member_region_ids.iter().cloned())
        .collect::<BTreeSet<_>>();
    let unattributed_analysis_gap_count = gap_regions
        .iter()
        .filter(|regions| regions.is_empty() || regions.is_disjoint(&assigned_regions))
        .count();
    MapView {
        areas,
        relations,
        unattributed_analysis_gap_count,
    }
}

struct MapProjectionContext<'a> {
    children: &'a BTreeMap<SemanticAreaId, Vec<&'a ApprovedSemanticArea>>,
    regions: &'a BTreeMap<RegionId, &'a StaticRegionSummary>,
    anchors_by_region: &'a BTreeMap<RegionId, Vec<&'a codebase_semantic_model::AnchorFactSummary>>,
    traces: &'a [TracePathSummary],
    boundary_relations: &'a [BoundaryRelationSummary],
    gap_regions: &'a [BTreeSet<RegionId>],
    facts_by_id: &'a BTreeMap<FactNodeId, &'a FactNode>,
    definition_evidence: &'a [FactEvidence],
}

fn project_area(
    area: &ApprovedSemanticArea,
    projection: &MapProjectionContext<'_>,
    position: Option<MapPosition>,
) -> MapArea {
    let mut child_areas = projection
        .children
        .get(&area.area_id)
        .cloned()
        .unwrap_or_default();
    child_areas.sort_by(|left, right| left.area_id.cmp(&right.area_id));
    let nested = child_areas
        .iter()
        .map(|child| project_area(child, projection, None))
        .collect::<Vec<_>>();
    let mut anchors = area
        .direct_member_region_ids
        .iter()
        .flat_map(|region_id| {
            projection
                .anchors_by_region
                .get(region_id)
                .into_iter()
                .flatten()
        })
        .collect::<Vec<_>>();
    anchors.sort_by(|left, right| left.fact_id.cmp(&right.fact_id));
    let representative_trace = nested
        .is_empty()
        .then(|| {
            area_trace_candidates(
                area,
                projection.regions.values().copied(),
                projection.traces,
            )
            .into_iter()
            .next()
        })
        .flatten();
    let traced_nodes = representative_trace.and_then(|trace| {
        project_trace_steps(
            trace,
            projection.facts_by_id,
            projection.definition_evidence,
        )
        .map(|steps| (trace, steps))
    });
    let nodes = traced_nodes
        .as_ref()
        .map(|(_, steps)| steps.clone())
        .or_else(|| {
            anchors.first().map(|anchor| {
                vec![projection
                    .facts_by_id
                    .get(&anchor.fact_id)
                    .map(|fact| project_fact_node(fact, projection.definition_evidence))
                    .unwrap_or_else(|| MapNode {
                        id: anchor.fact_id.to_string(),
                        name: anchor.name.clone(),
                        kind: display_kind(anchor.kind).to_string(),
                        role: node_role(anchor.kind, &anchor.static_roles),
                        definition: None,
                    })]
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
    let trace = traced_nodes.map(|(trace, steps)| MapTraceMeta {
        id: trace.trace_path_id.to_string(),
        state: map_trace_state(trace.state),
        step_count: steps.len(),
    });
    let original_name =
        single_structural_label(area, projection.regions).filter(|label| label != &area.label);
    let width = (!nested.is_empty()).then(|| {
        u32::try_from(nested.len())
            .unwrap_or(u32::MAX)
            .saturating_mul(224)
            .saturating_add(32)
            .max(232)
    });
    let member_regions = area
        .effective_member_region_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    MapArea {
        id: area.area_id.to_string(),
        name: area.label.clone(),
        original_name,
        summary: area.summary.clone(),
        category: map_area_category(area.category),
        label_source: map_label_source(area.label_source),
        fallback_reason: area.fallback_reason.map(map_fallback_reason),
        depth: area.level,
        areas: nested,
        nodes,
        hidden_node_count,
        position,
        width,
        trace,
        boundary_relation_counts: boundary_relation_counts(
            projection.boundary_relations,
            &member_regions,
        ),
        affecting_analysis_gap_count: projection
            .gap_regions
            .iter()
            .filter(|regions| !regions.is_disjoint(&member_regions))
            .count(),
    }
}

fn map_area_category(category: AreaCategory) -> &'static str {
    match category {
        AreaCategory::Domain => "domain",
        AreaCategory::Shared => "shared",
        AreaCategory::Infrastructure => "infrastructure",
        AreaCategory::Integration => "integration",
        AreaCategory::Structural => "structural",
    }
}

fn map_label_source(source: LabelSource) -> &'static str {
    match source {
        LabelSource::Semantic => "semantic",
        LabelSource::Structural => "structural",
    }
}

fn map_fallback_reason(reason: SemanticFallbackReason) -> &'static str {
    match reason {
        SemanticFallbackReason::InsufficientSemanticSignal => "insufficient-semantic-signal",
        SemanticFallbackReason::MixedResponsibility => "mixed-responsibility",
    }
}

fn boundary_relation_counts(
    bundles: &[BoundaryRelationSummary],
    member_regions: &BTreeSet<RegionId>,
) -> MapTruthCounts {
    let mut result = MapTruthCounts::default();
    for bundle in bundles {
        let source_inside = member_regions.contains(&bundle.source_region_id);
        let target_inside = member_regions.contains(&bundle.target_region_id);
        // Exactly one endpoint must be inside. A relation between two member
        // regions describes the area's internals, not its external boundary.
        if source_inside == target_inside {
            continue;
        }
        for family in &bundle.families {
            let counter = match family.truth {
                FactTruth::Confirmed => &mut result.verified,
                FactTruth::Structural => &mut result.structural,
                FactTruth::StaticCandidate => &mut result.candidate,
            };
            *counter = counter.saturating_add(family.relation_count);
        }
    }
    result
}

/// Resolve each canonical analysis gap to the static regions its declared
/// scope affects. This does not use names, semantic labels, or AI output.
fn attribute_gap_regions(
    gaps: &[AnalysisGap],
    file_coverage: &[FileCoverageRecord],
    regions: &[StaticRegionSummary],
) -> Vec<BTreeSet<RegionId>> {
    let mut regions_by_unit = BTreeMap::<AnalysisUnitId, BTreeSet<RegionId>>::new();
    for coverage in file_coverage {
        let Some(unit_id) = &coverage.unit_id else {
            continue;
        };
        regions_by_unit
            .entry(unit_id.clone())
            .or_default()
            .extend(regions_for_file_path(&coverage.path, regions));
    }

    gaps.iter()
        .map(|gap| match &gap.scope {
            AnalysisScope::Workspace => BTreeSet::new(),
            AnalysisScope::File { path, .. } => regions_for_file_path(path, regions),
            AnalysisScope::RepositoryScope { path } => regions
                .iter()
                .filter(|region| {
                    region.path_roots.iter().any(|root| {
                        path_is_at_or_below(root, path) || path_is_at_or_below(path, root)
                    })
                })
                .map(|region| region.region_id.clone())
                .collect(),
            AnalysisScope::AnalysisUnit { unit_id }
            | AnalysisScope::NativeSymbol { unit_id, .. } => {
                regions_by_unit.get(unit_id).cloned().unwrap_or_default()
            }
        })
        .collect()
}

fn selection_gap_summary(
    gaps: &[AnalysisGap],
    file_coverage: &[FileCoverageRecord],
    regions: &[StaticRegionSummary],
    owner_regions: &BTreeSet<RegionId>,
    selected_fact: Option<&FactNode>,
    selected_evidence: &[FactEvidence],
) -> MapAnalysisGapSummary {
    let attributed = attribute_gap_regions(gaps, file_coverage, regions);
    let mut relevant = gaps
        .iter()
        .zip(attributed)
        .filter_map(|(gap, attributed_regions)| {
            let applies = if !owner_regions.is_empty() {
                !attributed_regions.is_disjoint(owner_regions)
            } else {
                selected_fact.is_some_and(|fact| gap_applies_to_fact(gap, fact, selected_evidence))
            };
            applies.then_some(gap)
        })
        .collect::<Vec<_>>();
    relevant.sort_by(|left, right| {
        (
            left.code.as_str(),
            left.capability.map(|capability| capability.as_str()),
            left.message.as_str(),
        )
            .cmp(&(
                right.code.as_str(),
                right.capability.map(|capability| capability.as_str()),
                right.message.as_str(),
            ))
    });
    let total_count = relevant.len();
    let items = relevant
        .into_iter()
        .take(MAX_SELECTION_ANALYSIS_GAPS)
        .map(|gap| MapAnalysisGap {
            code: gap.code.as_str(),
            capability: gap.capability.map(|capability| capability.as_str()),
            message: gap.message.clone(),
        })
        .collect::<Vec<_>>();
    MapAnalysisGapSummary {
        total_count,
        truncated_count: total_count.saturating_sub(items.len()),
        items,
    }
}

fn gap_applies_to_fact(gap: &AnalysisGap, fact: &FactNode, evidence_rows: &[FactEvidence]) -> bool {
    let fact_evidence_ids = fact.evidence_ids.iter().collect::<BTreeSet<_>>();
    if gap
        .evidence_ids
        .iter()
        .any(|evidence_id| fact_evidence_ids.contains(evidence_id))
    {
        return true;
    }
    let source_paths = evidence_rows
        .iter()
        .filter(|evidence| fact_evidence_ids.contains(&evidence.id))
        .filter_map(|evidence| match &evidence.location {
            EvidenceLocation::Source { span } => Some(&span.path),
            EvidenceLocation::RepositoryArtifact { artifact } => Some(&artifact.path),
            EvidenceLocation::DatabaseCatalog { .. } => None,
        })
        .collect::<BTreeSet<_>>();
    match &gap.scope {
        AnalysisScope::Workspace => true,
        AnalysisScope::AnalysisUnit { unit_id } => fact.analysis_unit_id.as_ref() == Some(unit_id),
        AnalysisScope::NativeSymbol { .. } => false,
        AnalysisScope::File { unit_id, path } => {
            unit_id
                .as_ref()
                .is_none_or(|unit_id| fact.analysis_unit_id.as_ref() == Some(unit_id))
                && source_paths.contains(path)
        }
        AnalysisScope::RepositoryScope { path } => source_paths
            .iter()
            .any(|source_path| path_is_at_or_below(source_path, path)),
    }
}

fn regions_for_file_path(
    path: &RepositoryPath,
    regions: &[StaticRegionSummary],
) -> BTreeSet<RegionId> {
    let mut matches = regions
        .iter()
        .filter_map(|region| {
            region
                .path_roots
                .iter()
                .filter(|root| path_is_at_or_below(path, root))
                .map(|root| root.as_str().len())
                .max()
                .map(|specificity| (specificity, region.region_id.clone()))
        })
        .collect::<Vec<_>>();
    let Some(most_specific) = matches.iter().map(|(length, _)| *length).max() else {
        return BTreeSet::new();
    };
    matches.retain(|(length, _)| *length == most_specific);
    matches
        .into_iter()
        .map(|(_, region_id)| region_id)
        .collect()
}

fn path_is_at_or_below(path: &RepositoryPath, root: &RepositoryPath) -> bool {
    root.is_root()
        || path == root
        || path
            .as_str()
            .strip_prefix(root.as_str())
            .is_some_and(|suffix| suffix.starts_with('/'))
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

fn selection_entry_rank(kind: FactNodeKind) -> Option<u8> {
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

fn round_robin_traces(groups: &[Vec<TracePathSummary>], limit: usize) -> Vec<TracePathSummary> {
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for index in 0..limit {
        let mut added = false;
        for group in groups {
            let Some(trace) = group.get(index) else {
                continue;
            };
            if seen.insert(trace.trace_path_id.clone()) {
                result.push(trace.clone());
                added = true;
                if result.len() >= limit {
                    return result;
                }
            }
        }
        if !added && groups.iter().all(|group| group.len() <= index + 1) {
            break;
        }
    }
    result
}

fn project_trace(
    trace: &TracePathSummary,
    facts_by_id: &BTreeMap<FactNodeId, &FactNode>,
    edges_by_id: &BTreeMap<FactEdgeId, &FactEdge>,
    evidence_rows: &[codebase_fact_model::evidence::FactEvidence],
) -> Option<MapTrace> {
    let steps = project_trace_steps(trace, facts_by_id, evidence_rows)?;
    if trace.ordered_edge_ids.len().saturating_add(1) != trace.ordered_fact_ids.len() {
        return None;
    }
    let hops = trace
        .ordered_edge_ids
        .iter()
        .zip(trace.ordered_fact_ids.windows(2))
        .map(|(edge_id, pair)| {
            let edge = edges_by_id.get(edge_id)?;
            let evidence = evidence_refs(evidence_rows, &edge.evidence_ids);
            let execution = edge
                .execution
                .as_ref()
                .map(|execution| MapExecutionOccurrence {
                    call_site_evidence_id: execution.call_site_evidence_id.to_string(),
                    call_site: evidence_refs(
                        evidence_rows,
                        std::slice::from_ref(&execution.call_site_evidence_id),
                    )
                    .into_iter()
                    .next(),
                    lexical_ordinal: execution.lexical_ordinal,
                    guarded: execution.control.guarded,
                    repeated: execution.control.repeated,
                    deferred: execution.control.deferred,
                    awaited: execution.control.awaited,
                });
            Some(MapTraceHop {
                id: edge.id.to_string(),
                from: pair[0].to_string(),
                to: pair[1].to_string(),
                kind: edge.kind.as_str().to_string(),
                truth: map_truth(edge.truth),
                dispatch: map_dispatch(edge.dispatch),
                evidence,
                execution,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(MapTrace {
        id: trace.trace_path_id.to_string(),
        state: map_trace_state(trace.state),
        steps,
        hops,
    })
}

fn project_trace_steps(
    trace: &TracePathSummary,
    facts_by_id: &BTreeMap<FactNodeId, &FactNode>,
    evidence_rows: &[FactEvidence],
) -> Option<Vec<MapNode>> {
    let steps = trace
        .ordered_fact_ids
        .iter()
        .map(|fact_id| {
            facts_by_id
                .get(fact_id)
                .map(|fact| project_fact_node(fact, evidence_rows))
        })
        .collect::<Option<Vec<_>>>()?;
    (!steps.is_empty()).then_some(steps)
}

fn project_fact_node(fact: &FactNode, evidence_rows: &[FactEvidence]) -> MapNode {
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
        definition: definition_ref(fact, evidence_rows),
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

fn map_dispatch(dispatch: DispatchKind) -> &'static str {
    match dispatch {
        DispatchKind::Direct => "direct",
        DispatchKind::Virtual => "virtual",
        DispatchKind::Interface => "interface",
        DispatchKind::Dynamic => "dynamic",
        DispatchKind::Unknown => "unknown",
        DispatchKind::NotApplicable => "not-applicable",
    }
}

fn map_optional_dispatch(dispatch: Option<DispatchKind>) -> &'static str {
    dispatch.map(map_dispatch).unwrap_or("unreported")
}

fn dispatch_option_rank(dispatch: Option<DispatchKind>) -> u8 {
    match dispatch {
        Some(DispatchKind::Direct) => 0,
        Some(DispatchKind::Virtual) => 1,
        Some(DispatchKind::Interface) => 2,
        Some(DispatchKind::Dynamic) => 3,
        Some(DispatchKind::Unknown) => 4,
        Some(DispatchKind::NotApplicable) => 5,
        None => 6,
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
    dispatch_counts: BTreeMap<u8, (Option<DispatchKind>, u64)>,
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
                    dispatch_counts: BTreeMap::new(),
                });
        for count in &bundle.families {
            entry.count = entry.count.saturating_add(count.relation_count);
            *entry.family_counts.entry(count.family).or_default() += count.relation_count;
            entry
                .dispatch_counts
                .entry(dispatch_option_rank(count.dispatch))
                .or_insert((count.dispatch, 0))
                .1 += count.relation_count;
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
            dispatches: relation
                .dispatch_counts
                .into_values()
                .map(|(dispatch, count)| MapDispatchTally {
                    dispatch: map_optional_dispatch(dispatch),
                    count,
                })
                .collect(),
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
    let mut counts = BTreeMap::<
        (bool, u8, u8, u8),
        (FactEdgeFamily, FactTruth, Option<DispatchKind>, u64),
    >::new();
    for bundle in bundles {
        let outbound = owner_regions.contains(&bundle.source_region_id);
        let inbound = owner_regions.contains(&bundle.target_region_id);
        if !outbound && !inbound {
            continue;
        }
        for family in &bundle.families {
            if outbound {
                counts
                    .entry((
                        true,
                        family_rank(family.family),
                        truth_rank(family.truth),
                        dispatch_option_rank(family.dispatch),
                    ))
                    .or_insert((family.family, family.truth, family.dispatch, 0))
                    .3 += family.relation_count;
            }
            if inbound {
                counts
                    .entry((
                        false,
                        family_rank(family.family),
                        truth_rank(family.truth),
                        dispatch_option_rank(family.dispatch),
                    ))
                    .or_insert((family.family, family.truth, family.dispatch, 0))
                    .3 += family.relation_count;
            }
        }
    }
    counts
        .into_iter()
        .map(
            |((outbound, _, _, _), (family, truth, dispatch, count))| RelationTally {
                label: format!(
                    "{} ({})",
                    family_label(family),
                    if outbound { "나감" } else { "들어옴" }
                ),
                truth: map_truth(truth),
                dispatch: map_optional_dispatch(dispatch),
                count,
            },
        )
        .collect()
}

fn evidence_refs(facts: &[FactEvidence], evidence_ids: &[EvidenceId]) -> Vec<EvidenceRef> {
    let wanted = evidence_ids.iter().collect::<BTreeSet<_>>();
    facts
        .iter()
        .filter(|evidence| wanted.contains(&evidence.id))
        .filter_map(evidence_ref)
        .collect()
}

fn definition_ref(fact: &FactNode, evidence_rows: &[FactEvidence]) -> Option<EvidenceRef> {
    let definition_id = fact.definition_evidence_id.as_ref()?;
    evidence_rows
        .iter()
        .find(|evidence| &evidence.id == definition_id)
        .and_then(evidence_ref)
}

fn evidence_ref(evidence: &FactEvidence) -> Option<EvidenceRef> {
    match &evidence.location {
        EvidenceLocation::Source { span } => Some(EvidenceRef {
            path: span.path.to_string(),
            line: Some(span.start.line.saturating_add(1)),
        }),
        EvidenceLocation::RepositoryArtifact { artifact } => Some(EvidenceRef {
            path: artifact.path.to_string(),
            line: None,
        }),
        EvidenceLocation::DatabaseCatalog { .. } => None,
    }
}

fn assign_default_positions(areas: &mut [MapArea]) {
    const LEFT: i32 = 48;
    const TOP: i32 = 96;
    const COLUMN_GAP: i32 = 56;
    const ROW_GAP: i32 = 64;

    let columns = default_column_count(areas.len());
    let mut y = TOP;
    for row in areas.chunks_mut(columns) {
        let mut x = LEFT;
        let mut row_height = 0_i32;
        for area in row {
            area.position = Some(MapPosition { x, y });
            let width = i32::try_from(area.width.unwrap_or(232)).unwrap_or(i32::MAX);
            x = x.saturating_add(width).saturating_add(COLUMN_GAP);
            row_height = row_height.max(estimated_area_height(area));
        }
        y = y.saturating_add(row_height).saturating_add(ROW_GAP);
    }
}

fn default_column_count(area_count: usize) -> usize {
    match area_count {
        0..=4 => area_count.max(1),
        5..=12 => 4,
        13..=30 => 5,
        31..=60 => 6,
        61..=112 => 7,
        _ => 8,
    }
}

fn estimated_area_height(area: &MapArea) -> i32 {
    let trace_height = i32::from(area.trace.is_some()) * 24;
    let hidden_height = i32::from(area.hidden_node_count > 0) * 36;
    if area.areas.is_empty() {
        return 88_i32
            .saturating_add(trace_height)
            .saturating_add(hidden_height)
            .saturating_add(
                i32::try_from(area.nodes.len())
                    .unwrap_or(i32::MAX)
                    .saturating_mul(58),
            );
    }
    let tallest_child = area
        .areas
        .iter()
        .map(estimated_area_height)
        .max()
        .unwrap_or(0);
    112_i32
        .saturating_add(trace_height)
        .saturating_add(tallest_child)
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    fn area(id: &str, width: u32, node_count: usize) -> MapArea {
        MapArea {
            id: id.to_string(),
            name: id.to_string(),
            original_name: None,
            summary: String::new(),
            category: "domain",
            label_source: "semantic",
            fallback_reason: None,
            depth: 0,
            areas: Vec::new(),
            nodes: (0..node_count)
                .map(|index| MapNode {
                    id: format!("{id}-{index}"),
                    name: index.to_string(),
                    kind: "Code".to_string(),
                    role: "code",
                    definition: None,
                })
                .collect(),
            hidden_node_count: 0,
            position: None,
            width: Some(width),
            trace: None,
            boundary_relation_counts: MapTruthCounts::default(),
            affecting_analysis_gap_count: 0,
        }
    }

    #[test]
    fn default_layout_accounts_for_variable_width_and_row_height() {
        let mut areas = vec![
            area("wide", 720, 1),
            area("next", 232, 1),
            area("third", 232, 1),
            area("tall", 232, 8),
            area("next-row", 232, 1),
        ];
        assign_default_positions(&mut areas);

        let wide = areas[0].position.as_ref().unwrap();
        let next = areas[1].position.as_ref().unwrap();
        assert!(next.x >= wide.x + 720 + 56);
        let tall = areas[3].position.as_ref().unwrap();
        let next_row = areas[4].position.as_ref().unwrap();
        assert!(next_row.y >= tall.y + estimated_area_height(&areas[3]) + 64);
    }

    #[test]
    fn large_maps_expand_to_more_columns_without_losing_determinism() {
        assert_eq!(default_column_count(4), 4);
        assert_eq!(default_column_count(20), 5);
        assert_eq!(default_column_count(192), 8);
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
        FactNodeKind::TableReference => "Table Reference",
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
    if kind == FactNodeKind::TableReference {
        return "table-reference";
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
        coverage::{FileCoverageState, GapCode},
        evidence::{
            EvidenceKind, EvidenceLocation, EvidenceProducer, EvidenceProducerKind, FactEvidence,
        },
        fact_graph::{ExecutionControlContext, ExecutionOccurrence, ResolutionMethod, Visibility},
        identity::{AnalysisUnitId, FactEdgeId, SemanticContextId, Sha256Digest, SnapshotId},
        source::{RepositoryPath, SourceFileKind, SourceFlags, SourcePosition, SourceSpan},
    };
    use codebase_semantic_model::{BoundaryRelationCount, RelationBundleId, StaticRegionKind};

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

        let projected = project_trace_steps(&trace, &facts, &[]).unwrap();
        assert_eq!(
            projected
                .iter()
                .map(|step| step.name.as_str())
                .collect::<Vec<_>>(),
            vec!["GET /orders", "getOrders"]
        );

        let incomplete_facts = [(&route.id, &route)]
            .into_iter()
            .map(|(id, fact)| (id.clone(), fact))
            .collect::<BTreeMap<_, _>>();
        assert!(project_trace_steps(&trace, &incomplete_facts, &[]).is_none());
    }

    #[test]
    fn map_node_definition_uses_the_definition_evidence_not_a_call_site() {
        let unit = AnalysisUnitId::from_components(&["map-view", "definition"]).unwrap();
        let mut fact = fact_node(FactNodeKind::Method, "OrderService.create", &unit);
        let definition = source_evidence(
            EvidenceKind::SourceDefinition,
            "src/orders/order.service.ts",
            66,
            "method definition",
        );
        let call_site = source_evidence(
            EvidenceKind::CallSite,
            "src/orders/order.controller.ts",
            41,
            "call site",
        );
        fact.definition_evidence_id = Some(definition.id.clone());
        fact.evidence_ids = vec![call_site.id.clone(), definition.id.clone()];
        fact.evidence_ids.sort();

        let projected = project_fact_node(&fact, &[call_site, definition]);

        assert_eq!(
            projected.definition,
            Some(EvidenceRef {
                path: "src/orders/order.service.ts".to_string(),
                line: Some(67),
            })
        );
    }

    #[test]
    fn semantic_label_provenance_is_not_inferred_from_category() {
        assert_eq!(map_label_source(LabelSource::Semantic), "semantic");
        assert_eq!(map_label_source(LabelSource::Structural), "structural");
        assert_eq!(
            map_fallback_reason(SemanticFallbackReason::InsufficientSemanticSignal),
            "insufficient-semantic-signal"
        );
        assert_eq!(
            map_fallback_reason(SemanticFallbackReason::MixedResponsibility),
            "mixed-responsibility"
        );
    }

    #[test]
    fn map_trace_serves_exact_execution_and_source_evidence_to_the_frontend() {
        let unit = AnalysisUnitId::from_components(&["map-view", "execution"]).unwrap();
        let caller = fact_node(FactNodeKind::Method, "OrderService.create", &unit);
        let callee = fact_node(FactNodeKind::Method, "InventoryService.check", &unit);
        let evidence = FactEvidence::new(
            EvidenceKind::CallSite,
            EvidenceProducer {
                kind: EvidenceProducerKind::SyntaxParser,
                name: "map-view-test".to_string(),
                version: None,
                strategy: Some("exact-call-site".to_string()),
            },
            EvidenceLocation::Source {
                span: SourceSpan {
                    path: RepositoryPath::parse("src/orders.ts").unwrap(),
                    content_digest: Sha256Digest::of_bytes(b"inventory.check()"),
                    start: SourcePosition {
                        line: 41,
                        utf8_column: 8,
                        byte_offset: 128,
                    },
                    end: SourcePosition {
                        line: 41,
                        utf8_column: 25,
                        byte_offset: 145,
                    },
                },
            },
            Some("exact call".to_string()),
        )
        .unwrap();
        let context = SemanticContextId::from_components(&["map-view", "context"]).unwrap();
        let execution = ExecutionOccurrence {
            call_site_evidence_id: evidence.id.clone(),
            lexical_ordinal: 3,
            control: ExecutionControlContext {
                guarded: true,
                repeated: false,
                deferred: false,
                awaited: true,
            },
        };
        let edge_id = FactEdge::stable_id(
            &caller.id,
            &callee.id,
            codebase_fact_model::fact_graph::FactEdgeKind::Calls,
            Some(&context),
            None,
            Some(&execution),
        )
        .unwrap();
        let edge = FactEdge {
            id: edge_id.clone(),
            snapshot_id: SnapshotId::from_components(&["map-view"]).unwrap(),
            source_id: caller.id.clone(),
            target_id: callee.id.clone(),
            family: FactEdgeFamily::Code,
            kind: codebase_fact_model::fact_graph::FactEdgeKind::Calls,
            truth: FactTruth::Confirmed,
            resolution: ResolutionMethod::SyntaxExact,
            dispatch: DispatchKind::Direct,
            semantic_context_id: Some(context),
            qualifier: None,
            execution: Some(execution),
            evidence_ids: vec![evidence.id.clone()],
        };
        let trace = TracePathSummary {
            trace_path_id: TracePathSummary::stable_id(&caller.id, std::slice::from_ref(&edge_id))
                .unwrap(),
            entry_fact_id: caller.id.clone(),
            ordered_fact_ids: vec![caller.id.clone(), callee.id.clone()],
            ordered_edge_ids: vec![edge_id.clone()],
            state: TracePathState::Complete,
            evidence_ids: vec![evidence.id.clone()],
        };
        let nodes = [&caller, &callee]
            .into_iter()
            .map(|fact| (fact.id.clone(), fact))
            .collect::<BTreeMap<_, _>>();
        let edges = [(&edge_id, &edge)]
            .into_iter()
            .map(|(id, edge)| (id.clone(), edge))
            .collect::<BTreeMap<_, _>>();

        let projected = project_trace(&trace, &nodes, &edges, &[evidence]).unwrap();

        assert_eq!(projected.hops.len(), 1);
        assert_eq!(projected.hops[0].from, caller.id.to_string());
        assert_eq!(projected.hops[0].to, callee.id.to_string());
        assert_eq!(projected.hops[0].dispatch, "direct");
        let execution = projected.hops[0].execution.as_ref().unwrap();
        assert_eq!(execution.lexical_ordinal, 3);
        assert!(execution.guarded);
        assert!(execution.awaited);
        assert_eq!(execution.call_site.as_ref().unwrap().line, Some(42));
        let served = serde_json::to_value(projected).unwrap();
        assert_eq!(served["hops"][0]["dispatch"], "direct");
        assert_eq!(served["hops"][0]["execution"]["lexicalOrdinal"], 3);
        assert_eq!(served["hops"][0]["execution"]["callSite"]["line"], 42);
    }

    #[test]
    fn selection_traces_are_fair_across_entrypoints_and_deduplicated() {
        let first = trace_summary("first");
        let second = trace_summary("second");
        let third = trace_summary("third");
        let fourth = trace_summary("fourth");
        let groups = vec![
            vec![first.clone(), third.clone()],
            vec![second.clone()],
            vec![first.clone(), fourth.clone()],
        ];

        let selected = round_robin_traces(&groups, 4);

        assert_eq!(
            selected
                .iter()
                .map(|trace| trace.trace_path_id.clone())
                .collect::<Vec<_>>(),
            vec![
                first.trace_path_id,
                second.trace_path_id,
                third.trace_path_id,
                fourth.trace_path_id,
            ]
        );
    }

    #[test]
    fn area_counters_only_count_relations_that_cross_the_area_boundary() {
        let orders = RegionId::from_components(&["map-view", "orders"]).unwrap();
        let checkout = RegionId::from_components(&["map-view", "checkout"]).unwrap();
        let auth = RegionId::from_components(&["map-view", "auth"]).unwrap();
        let members = BTreeSet::from([orders.clone(), checkout.clone()]);
        let bundles = vec![
            relation_bundle(
                "internal",
                &orders,
                &checkout,
                &[(FactTruth::Confirmed, 99)],
            ),
            relation_bundle(
                "outbound",
                &orders,
                &auth,
                &[(FactTruth::Confirmed, 5), (FactTruth::Structural, 2)],
            ),
            relation_bundle(
                "inbound",
                &auth,
                &checkout,
                &[(FactTruth::StaticCandidate, 3)],
            ),
        ];

        let counts = boundary_relation_counts(&bundles, &members);

        assert_eq!(
            counts,
            MapTruthCounts {
                verified: 5,
                structural: 2,
                candidate: 3,
            }
        );
    }

    #[test]
    fn canonical_gap_scopes_are_attributed_without_semantic_name_guessing() {
        let orders = static_region("orders", "src/orders");
        let auth = static_region("auth", "src/auth");
        let unit = AnalysisUnitId::from_components(&["map-view", "unit", "orders"]).unwrap();
        let coverage = FileCoverageRecord {
            unit_id: Some(unit.clone()),
            path: RepositoryPath::parse("src/orders/service.ts").unwrap(),
            language: Some(ProgrammingLanguage::TypeScript),
            file_kind: SourceFileKind::Source,
            state: FileCoverageState::Indexed,
            byte_size: 32,
            line_count: Some(1),
            non_blank_line_count: Some(1),
            content_digest: Some(Sha256Digest::of_bytes(b"export const service = 1;")),
            gap_codes: Vec::new(),
        };
        let missing_unit =
            AnalysisUnitId::from_components(&["map-view", "unit", "missing"]).unwrap();
        let gaps = vec![
            analysis_gap(AnalysisScope::AnalysisUnit {
                unit_id: unit.clone(),
            }),
            analysis_gap(AnalysisScope::File {
                unit_id: None,
                path: RepositoryPath::parse("src/auth/session.ts").unwrap(),
            }),
            analysis_gap(AnalysisScope::RepositoryScope {
                path: RepositoryPath::parse("src").unwrap(),
            }),
            analysis_gap(AnalysisScope::Workspace),
            analysis_gap(AnalysisScope::AnalysisUnit {
                unit_id: missing_unit,
            }),
        ];

        let attributed = attribute_gap_regions(&gaps, &[coverage], &[orders.clone(), auth.clone()]);

        assert_eq!(attributed[0], BTreeSet::from([orders.region_id.clone()]));
        assert_eq!(attributed[1], BTreeSet::from([auth.region_id.clone()]));
        assert_eq!(
            attributed[2],
            BTreeSet::from([orders.region_id.clone(), auth.region_id.clone()])
        );
        assert!(attributed[3].is_empty());
        assert!(attributed[4].is_empty());
    }

    #[test]
    fn selection_gap_details_match_area_attribution_and_are_bounded() {
        let orders = static_region("orders", "src/orders");
        let auth = static_region("auth", "src/auth");
        let unit = AnalysisUnitId::from_components(&["map-view", "gap-summary"]).unwrap();
        let coverage = FileCoverageRecord {
            unit_id: Some(unit.clone()),
            path: RepositoryPath::parse("src/orders/service.ts").unwrap(),
            language: Some(ProgrammingLanguage::TypeScript),
            file_kind: SourceFileKind::Source,
            state: FileCoverageState::Indexed,
            byte_size: 32,
            line_count: Some(1),
            non_blank_line_count: Some(1),
            content_digest: Some(Sha256Digest::of_bytes(b"export const service = 1;")),
            gap_codes: Vec::new(),
        };
        let mut gaps = (0..20)
            .map(|index| AnalysisGap {
                code: GapCode::UnresolvedTarget,
                scope: AnalysisScope::AnalysisUnit {
                    unit_id: unit.clone(),
                },
                capability: Some(codebase_fact_model::coverage::AnalysisCapability::DirectCalls),
                evidence_ids: Vec::new(),
                message: format!("unresolved call {index:02}"),
            })
            .collect::<Vec<_>>();
        gaps.push(AnalysisGap {
            code: GapCode::DynamicDispatch,
            scope: AnalysisScope::File {
                unit_id: None,
                path: RepositoryPath::parse("src/auth/session.ts").unwrap(),
            },
            capability: None,
            evidence_ids: Vec::new(),
            message: "unrelated auth gap".to_string(),
        });
        gaps.push(AnalysisGap {
            code: GapCode::ProviderUnavailable,
            scope: AnalysisScope::Workspace,
            capability: None,
            evidence_ids: Vec::new(),
            message: "workspace gap is reported globally".to_string(),
        });

        let summary = selection_gap_summary(
            &gaps,
            &[coverage],
            &[orders.clone(), auth],
            &BTreeSet::from([orders.region_id]),
            None,
            &[],
        );

        assert_eq!(summary.total_count, 20);
        assert_eq!(summary.items.len(), MAX_SELECTION_ANALYSIS_GAPS);
        assert_eq!(summary.truncated_count, 4);
        assert_eq!(summary.items[0].code, "unresolved_target");
        assert_eq!(summary.items[0].capability, Some("direct_calls"));
        assert_eq!(summary.items[0].message, "unresolved call 00");
    }

    fn relation_bundle(
        label: &str,
        source: &RegionId,
        target: &RegionId,
        counts: &[(FactTruth, u64)],
    ) -> BoundaryRelationSummary {
        BoundaryRelationSummary {
            bundle_id: RelationBundleId::from_components(&["map-view", "bundle", label]).unwrap(),
            source_region_id: source.clone(),
            target_region_id: target.clone(),
            families: counts
                .iter()
                .map(|(truth, relation_count)| BoundaryRelationCount {
                    family: FactEdgeFamily::Code,
                    truth: *truth,
                    dispatch: Some(DispatchKind::Direct),
                    relation_count: *relation_count,
                })
                .collect(),
            representative_edge_ids: Vec::new(),
            evidence_ids: Vec::new(),
        }
    }

    fn static_region(label: &str, path: &str) -> StaticRegionSummary {
        StaticRegionSummary {
            region_id: RegionId::from_components(&["map-view", "region", label]).unwrap(),
            parent_region_id: None,
            structural_label: label.to_string(),
            structural_kind: StaticRegionKind::Module,
            path_roots: vec![RepositoryPath::parse(path).unwrap()],
            languages: vec![ProgrammingLanguage::TypeScript],
            file_count: 1,
            effective_loc: 1,
            anchor_fact_ids: Vec::new(),
            representative_trace_path_ids: Vec::new(),
            inbound_bundle_ids: Vec::new(),
            outbound_bundle_ids: Vec::new(),
        }
    }

    fn analysis_gap(scope: AnalysisScope) -> AnalysisGap {
        AnalysisGap {
            code: GapCode::UnresolvedTarget,
            scope,
            capability: None,
            evidence_ids: Vec::new(),
            message: "test gap".to_string(),
        }
    }

    fn trace_summary(label: &str) -> TracePathSummary {
        let entry = FactNodeId::from_components(&["map-view", "entrypoint", label]).unwrap();
        TracePathSummary {
            trace_path_id: TracePathSummary::stable_id(&entry, &[]).unwrap(),
            entry_fact_id: entry.clone(),
            ordered_fact_ids: vec![entry],
            ordered_edge_ids: Vec::new(),
            state: TracePathState::Complete,
            evidence_ids: Vec::new(),
        }
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

    fn source_evidence(
        kind: EvidenceKind,
        path: &str,
        zero_based_line: u32,
        summary: &str,
    ) -> FactEvidence {
        FactEvidence::new(
            kind,
            EvidenceProducer {
                kind: EvidenceProducerKind::SyntaxParser,
                name: "map-view-test".to_string(),
                version: None,
                strategy: Some("exact-source-range".to_string()),
            },
            EvidenceLocation::Source {
                span: SourceSpan {
                    path: RepositoryPath::parse(path).unwrap(),
                    content_digest: Sha256Digest::of_bytes(summary.as_bytes()),
                    start: SourcePosition {
                        line: zero_based_line,
                        utf8_column: 0,
                        byte_offset: 0,
                    },
                    end: SourcePosition {
                        line: zero_based_line,
                        utf8_column: 1,
                        byte_offset: 1,
                    },
                },
            },
            Some(summary.to_string()),
        )
        .unwrap()
    }
}
