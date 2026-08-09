use crate::{fact_graph::CanonicalFactSnapshot, static_query, workspace::Workspace};
use codebase_fact_model::{
    analysis::ProgrammingLanguage,
    coverage::{FileCoverageRecord, FileCoverageState},
    evidence::{EvidenceLocation, FactEvidence},
    fact_graph::{
        FactEdge, FactEdgeFamily, FactEdgeKind, FactNode, FactNodeKind, FactRole, FactTruth,
        Visibility,
    },
    identity::{EvidenceId, FactNodeId, WorkspaceId},
    source::RepositoryPath,
};
use codebase_semantic_compiler::BaseSemanticDraft;
use codebase_semantic_model::{
    AiProviderDescriptor, AiProviderKind, AnchorFactSummary, BaseSemanticInput,
    BoundaryRelationCount, BoundaryRelationSummary, EvidenceExcerpt, OutputLanguage,
    ProjectSemanticContext, RegionId, RelationBundleId, ScopeReceipt, StaticRegionKind,
    StaticRegionSummary,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

/// Current bounded static region-directory ceiling. Provider work is split
/// into adaptive local jobs and joined deterministically, but the canonical
/// read model still rejects a larger structural directory instead of silently
/// omitting regions. This is a product safety ceiling, not a target job count.
const MAX_GLOBAL_REGIONS: usize = 192;
const MAX_ANCHORS_PER_REGION: usize = 12;
const MAX_EXCERPTS: usize = 48;
const MAX_EXCERPT_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_EXCERPT_TEXT_BYTES: usize = 24 * 1024;

pub(crate) fn build_base_draft(
    workspace: &Workspace,
    snapshot: &CanonicalFactSnapshot,
) -> Result<BaseSemanticDraft, String> {
    let provider = workspace
        .provider
        .as_ref()
        .ok_or_else(|| "AI 모델을 먼저 설정해야 의미 지도를 만들 수 있습니다".to_string())?;
    let workspace_id = WorkspaceId::parse(workspace.id.clone())
        .map_err(|error| format!("workspace identity가 올바르지 않습니다: {error}"))?;
    if snapshot.manifest.workspace_id != workspace_id {
        return Err("게시된 snapshot과 workspace identity가 다릅니다".to_string());
    }
    let repository_root = Path::new(&workspace.repo_path)
        .canonicalize()
        .map_err(|error| format!("프로젝트 폴더를 확인하지 못했습니다: {error}"))?;
    let input = build_input(&workspace_id, &repository_root, snapshot)?;
    let region_count = input.regions.len() as u64;
    Ok(BaseSemanticDraft {
        workspace_id,
        snapshot_id: snapshot.manifest.snapshot_id.clone(),
        provider: AiProviderDescriptor {
            kind: match provider.kind {
                crate::workspace::WorkspaceProviderKind::Codex => AiProviderKind::Codex,
                crate::workspace::WorkspaceProviderKind::Claude => AiProviderKind::Claude,
            },
            model: provider.model.clone(),
            effort: match provider.effort {
                crate::workspace::WorkspaceReasoningEffort::Low => {
                    codebase_semantic_model::ReasoningEffort::Low
                }
                crate::workspace::WorkspaceReasoningEffort::Medium => {
                    codebase_semantic_model::ReasoningEffort::Medium
                }
                crate::workspace::WorkspaceReasoningEffort::High => {
                    codebase_semantic_model::ReasoningEffort::High
                }
                crate::workspace::WorkspaceReasoningEffort::Xhigh => {
                    codebase_semantic_model::ReasoningEffort::Xhigh
                }
                crate::workspace::WorkspaceReasoningEffort::Max => {
                    codebase_semantic_model::ReasoningEffort::Max
                }
                crate::workspace::WorkspaceReasoningEffort::Ultra => {
                    codebase_semantic_model::ReasoningEffort::Ultra
                }
            },
        },
        output_language: OutputLanguage::Korean,
        scope_receipt: ScopeReceipt {
            included: region_count,
            total: region_count,
            truncated: false,
            reason: None,
        },
        input,
    })
}

fn build_input(
    workspace_id: &WorkspaceId,
    repository_root: &Path,
    snapshot: &CanonicalFactSnapshot,
) -> Result<BaseSemanticInput, String> {
    let nodes_by_id = snapshot
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node))
        .collect::<BTreeMap<_, _>>();
    let evidence_by_id = snapshot
        .evidence
        .iter()
        .map(|evidence| (evidence.id.clone(), evidence))
        .collect::<BTreeMap<_, _>>();
    let coverage_by_path = snapshot
        .file_coverage
        .iter()
        .map(|record| (record.path.clone(), record))
        .collect::<BTreeMap<_, _>>();
    let repository = snapshot
        .nodes
        .iter()
        .find(|node| node.kind == FactNodeKind::Repository && node.parent_id.is_none())
        .ok_or_else(|| "canonical snapshot에 repository root node가 없습니다".to_string())?;

    let production_files = snapshot
        .nodes
        .iter()
        .filter(|node| node.kind == FactNodeKind::File)
        .filter(|node| {
            !node.flags.test && !node.flags.generated && !node.flags.vendor && !node.flags.external
        })
        .filter_map(|node| {
            let path = RepositoryPath::parse(node.qualified_name.clone()).ok()?;
            let coverage = coverage_by_path.get(&path)?;
            matches!(
                coverage.state,
                FileCoverageState::Indexed | FileCoverageState::Partial
            )
            .then_some((node, path, *coverage))
        })
        .collect::<Vec<_>>();
    if production_files.is_empty() {
        return Err("의미 지도에 넣을 검증된 production source file이 없습니다".to_string());
    }

    let region_specs = plan_regions(workspace_id, &production_files, &nodes_by_id)?;
    if region_specs.len() > MAX_GLOBAL_REGIONS {
        return Err(format!(
            "구조 단위가 {}개라 현재 지도 안전 한도 {}개를 넘었습니다. 더 높은 상위 구조로 축약해야 합니다",
            region_specs.len(),
            MAX_GLOBAL_REGIONS
        ));
    }

    let mut region_by_file = BTreeMap::<FactNodeId, RegionId>::new();
    let mut regions = Vec::with_capacity(region_specs.len());
    for spec in region_specs {
        let mut languages = BTreeSet::new();
        let mut effective_loc = 0_u64;
        for file in &spec.files {
            let language = file
                .language
                .ok_or_else(|| format!("source file 언어가 없습니다: {}", file.qualified_name))?;
            languages.insert(language);
            let path = RepositoryPath::parse(file.qualified_name.clone())
                .map_err(|error| format!("source file 경로가 올바르지 않습니다: {error}"))?;
            let coverage = coverage_by_path
                .get(&path)
                .ok_or_else(|| format!("source coverage가 없습니다: {path}"))?;
            let nonblank_lines = coverage
                .non_blank_line_count
                .map(Ok)
                .unwrap_or_else(|| measure_nonblank_lines(repository_root, coverage))?;
            effective_loc = effective_loc.saturating_add(nonblank_lines);
            region_by_file.insert(file.id.clone(), spec.region_id.clone());
        }
        regions.push(StaticRegionSummary {
            region_id: spec.region_id,
            parent_region_id: None,
            structural_label: spec.label,
            structural_kind: spec.kind,
            path_roots: spec.path_roots,
            languages: languages.into_iter().collect(),
            file_count: spec.files.len() as u64,
            effective_loc,
            anchor_fact_ids: Vec::new(),
            representative_trace_path_ids: Vec::new(),
            inbound_bundle_ids: Vec::new(),
            outbound_bundle_ids: Vec::new(),
        });
    }
    let mut region_index = regions
        .iter()
        .enumerate()
        .map(|(index, region)| (region.region_id.clone(), index))
        .collect::<BTreeMap<_, _>>();
    let owner_by_node = node_region_owners(
        &snapshot.nodes,
        &snapshot.edges,
        &nodes_by_id,
        &region_by_file,
    );
    let representative_traces = static_query::representative_trace_paths(snapshot, &owner_by_node)?;
    for trace in &representative_traces {
        let trace_regions = trace
            .ordered_fact_ids
            .iter()
            .filter_map(|fact_id| owner_by_node.get(fact_id))
            .cloned()
            .collect::<BTreeSet<_>>();
        for region_id in trace_regions {
            if let Some(index) = region_index.get(&region_id) {
                regions[*index]
                    .representative_trace_path_ids
                    .push(trace.trace_path_id.clone());
            }
        }
    }
    for region in &mut regions {
        region.representative_trace_path_ids.sort();
        region.representative_trace_path_ids.dedup();
    }
    let mut anchor_priority_nodes = snapshot
        .edges
        .iter()
        .filter(|edge| owner_by_node.get(&edge.source_id) != owner_by_node.get(&edge.target_id))
        .flat_map(|edge| [edge.source_id.clone(), edge.target_id.clone()])
        .collect::<BTreeSet<_>>();
    anchor_priority_nodes.extend(
        representative_traces
            .iter()
            .flat_map(|trace| trace.ordered_fact_ids.iter().cloned()),
    );
    let anchors = build_anchors(
        &snapshot.nodes,
        &owner_by_node,
        &anchor_priority_nodes,
        &mut regions,
        &region_index,
    );
    let boundary_relations = build_boundary_relations(
        workspace_id,
        &snapshot.edges,
        &owner_by_node,
        &mut regions,
        &region_index,
    )?;
    let excerpts = build_excerpts(
        repository_root,
        &anchors,
        &nodes_by_id,
        &evidence_by_id,
        &owner_by_node,
    )?;
    region_index.clear();

    let languages = production_files
        .iter()
        .filter_map(|(node, _, _)| node.language)
        .collect::<BTreeSet<ProgrammingLanguage>>()
        .into_iter()
        .collect::<Vec<_>>();
    let root_region_ids = regions
        .iter()
        .map(|region| region.region_id.clone())
        .collect();
    Ok(BaseSemanticInput {
        repository: ProjectSemanticContext {
            fact_id: repository.id.clone(),
            name: repository.display_name.clone(),
            languages,
            framework_fact_ids: snapshot
                .nodes
                .iter()
                .filter(|node| {
                    matches!(
                        node.kind,
                        FactNodeKind::HttpRoute
                            | FactNodeKind::GraphqlEndpoint
                            | FactNodeKind::RpcEndpoint
                    )
                })
                .map(|node| node.id.clone())
                .collect(),
            root_region_ids,
        },
        regions,
        anchors,
        boundary_relations,
        representative_traces,
        excerpts,
        previous_revision: None,
    })
}

struct RegionSpec<'a> {
    region_id: RegionId,
    label: String,
    kind: StaticRegionKind,
    path_roots: Vec<RepositoryPath>,
    files: Vec<&'a FactNode>,
}

fn plan_regions<'a>(
    workspace_id: &WorkspaceId,
    files: &[(&'a FactNode, RepositoryPath, &'a FileCoverageRecord)],
    nodes_by_id: &BTreeMap<FactNodeId, &'a FactNode>,
) -> Result<Vec<RegionSpec<'a>>, String> {
    let mut by_owner = BTreeMap::<String, Vec<&FactNode>>::new();
    let mut owner_nodes = BTreeMap::<String, &FactNode>::new();
    for (file, _, _) in files {
        if let Some(owner) = nearest_structural_owner(file, nodes_by_id) {
            let key = format!("node:{}", owner.id);
            owner_nodes.insert(key.clone(), owner);
            by_owner.entry(key).or_default().push(*file);
        } else {
            let key = directory_region_key(&file.qualified_name, usize::MAX);
            by_owner.entry(key).or_default().push(*file);
        }
    }
    if by_owner.len() == 1 && files.len() > 1 {
        by_owner.clear();
        owner_nodes.clear();
        for (file, _, _) in files {
            by_owner
                .entry(directory_region_key(&file.qualified_name, usize::MAX))
                .or_default()
                .push(*file);
        }
    }
    if by_owner.len() > MAX_GLOBAL_REGIONS {
        for depth in (1..=6).rev() {
            let mut collapsed = BTreeMap::<String, Vec<&FactNode>>::new();
            for (file, _, _) in files {
                collapsed
                    .entry(directory_region_key(&file.qualified_name, depth))
                    .or_default()
                    .push(*file);
            }
            if collapsed.len() <= MAX_GLOBAL_REGIONS {
                by_owner = collapsed;
                owner_nodes.clear();
                break;
            }
        }
    }

    let mut result = Vec::with_capacity(by_owner.len());
    for (key, mut owned_files) in by_owner {
        owned_files.sort_by(|left, right| left.qualified_name.cmp(&right.qualified_name));
        let owner = owner_nodes.get(&key).copied();
        let label = owner
            .map(|node| node.display_name.clone())
            .unwrap_or_else(|| region_label_from_key(&key));
        let kind = owner.map_or(StaticRegionKind::FileRegion, |node| match node.kind {
            FactNodeKind::BuildTarget => StaticRegionKind::BuildTarget,
            FactNodeKind::Package => StaticRegionKind::Package,
            _ => StaticRegionKind::Module,
        });
        let path_roots = region_path_roots(&key, &owned_files)?;
        let region_id = RegionId::from_components(&[
            "workspace",
            workspace_id.as_str(),
            "structural-owner",
            &key,
        ])
        .map_err(|error| format!("region identity를 만들지 못했습니다: {error}"))?;
        result.push(RegionSpec {
            region_id,
            label,
            kind,
            path_roots,
            files: owned_files,
        });
    }
    result.sort_by(|left, right| left.region_id.cmp(&right.region_id));
    Ok(result)
}

fn nearest_structural_owner<'a>(
    file: &'a FactNode,
    nodes_by_id: &BTreeMap<FactNodeId, &'a FactNode>,
) -> Option<&'a FactNode> {
    let mut current = file.parent_id.as_ref();
    let mut seen = BTreeSet::new();
    while let Some(id) = current {
        if !seen.insert(id.clone()) {
            return None;
        }
        let node = nodes_by_id.get(id)?;
        if matches!(
            node.kind,
            FactNodeKind::BuildTarget
                | FactNodeKind::Package
                | FactNodeKind::Module
                | FactNodeKind::Namespace
        ) {
            return Some(node);
        }
        current = node.parent_id.as_ref();
    }
    None
}

fn directory_region_key(path: &str, max_depth: usize) -> String {
    let parts = path.split('/').collect::<Vec<_>>();
    if parts.len() <= 1 {
        return format!("file:{path}");
    }
    let depth = parts.len().saturating_sub(1).min(max_depth);
    format!("path:{}", parts[..depth].join("/"))
}

fn region_label_from_key(key: &str) -> String {
    let value = key
        .strip_prefix("path:")
        .or_else(|| key.strip_prefix("file:"))
        .unwrap_or(key);
    value
        .rsplit('/')
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or("root")
        .to_string()
}

fn region_path_roots(key: &str, files: &[&FactNode]) -> Result<Vec<RepositoryPath>, String> {
    if let Some(path) = key.strip_prefix("path:") {
        return Ok(vec![RepositoryPath::parse(path.to_string()).map_err(
            |error| format!("region path가 올바르지 않습니다: {error}"),
        )?]);
    }
    if let Some(path) = key.strip_prefix("file:") {
        return Ok(vec![RepositoryPath::parse(path.to_string()).map_err(
            |error| format!("region file path가 올바르지 않습니다: {error}"),
        )?]);
    }
    let directories = files
        .iter()
        .map(|file| parent_repository_path(&file.qualified_name))
        .collect::<Result<BTreeSet<_>, _>>()?;
    Ok(directories.into_iter().collect())
}

fn parent_repository_path(path: &str) -> Result<RepositoryPath, String> {
    let parent = path
        .rsplit_once('/')
        .map(|(parent, _)| parent)
        .unwrap_or(".");
    RepositoryPath::parse(parent.to_string())
        .map_err(|error| format!("source parent path가 올바르지 않습니다: {error}"))
}

fn node_region_owners(
    nodes: &[FactNode],
    edges: &[FactEdge],
    nodes_by_id: &BTreeMap<FactNodeId, &FactNode>,
    region_by_file: &BTreeMap<FactNodeId, RegionId>,
) -> BTreeMap<FactNodeId, RegionId> {
    let mut result = BTreeMap::new();
    for node in nodes {
        let mut current = Some(&node.id);
        let mut seen = BTreeSet::new();
        while let Some(id) = current {
            if !seen.insert(id.clone()) {
                break;
            }
            if let Some(region) = region_by_file.get(id) {
                result.insert(node.id.clone(), region.clone());
                break;
            }
            current = nodes_by_id
                .get(id)
                .and_then(|owner| owner.parent_id.as_ref());
        }
    }
    // Framework endpoints intentionally live directly under the repository because
    // one logical route may be registered by more than one file.  EXPOSES is the
    // canonical ownership evidence.  Keep an ambiguous multi-file route unowned
    // instead of assigning it to an arbitrary region.
    let mut exposed_regions = BTreeMap::<FactNodeId, BTreeSet<RegionId>>::new();
    for edge in edges
        .iter()
        .filter(|edge| edge.kind == FactEdgeKind::Exposes && edge.truth == FactTruth::Confirmed)
    {
        let endpoint = nodes_by_id.get(&edge.target_id).is_some_and(|node| {
            matches!(
                node.kind,
                FactNodeKind::HttpRoute | FactNodeKind::GraphqlEndpoint | FactNodeKind::RpcEndpoint
            )
        });
        if !endpoint {
            continue;
        }
        if let Some(region_id) = result.get(&edge.source_id) {
            exposed_regions
                .entry(edge.target_id.clone())
                .or_default()
                .insert(region_id.clone());
        }
    }
    for (fact_id, regions) in exposed_regions {
        if regions.len() == 1 {
            let exposed_region = regions
                .into_iter()
                .next()
                .expect("one exposed region exists");
            if result
                .get(&fact_id)
                .is_none_or(|current| current == &exposed_region)
            {
                result.insert(fact_id, exposed_region);
            } else {
                result.remove(&fact_id);
            }
        } else {
            result.remove(&fact_id);
        }
    }
    result
}

fn build_anchors(
    nodes: &[FactNode],
    owner_by_node: &BTreeMap<FactNodeId, RegionId>,
    boundary_endpoints: &BTreeSet<FactNodeId>,
    regions: &mut [StaticRegionSummary],
    region_index: &BTreeMap<RegionId, usize>,
) -> Vec<AnchorFactSummary> {
    let mut candidates = BTreeMap::<RegionId, Vec<(u8, &FactNode)>>::new();
    for node in nodes {
        let Some(region_id) = owner_by_node.get(&node.id) else {
            continue;
        };
        let Some(rank) = anchor_rank(node, boundary_endpoints.contains(&node.id)) else {
            continue;
        };
        candidates
            .entry(region_id.clone())
            .or_default()
            .push((rank, node));
    }
    let mut result = Vec::new();
    for (region_id, mut items) in candidates {
        items.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.id.cmp(&right.1.id))
        });
        items.truncate(MAX_ANCHORS_PER_REGION);
        let region = &mut regions[region_index[&region_id]];
        for (_, node) in items {
            let evidence_ids = node
                .evidence_ids
                .iter()
                .chain(
                    node.roles
                        .iter()
                        .flat_map(|assignment| assignment.evidence_ids.iter()),
                )
                .cloned()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let roles = node
                .roles
                .iter()
                .map(|assignment| assignment.role)
                .collect::<BTreeSet<FactRole>>()
                .into_iter()
                .collect::<Vec<_>>();
            region.anchor_fact_ids.push(node.id.clone());
            result.push(AnchorFactSummary {
                fact_id: node.id.clone(),
                owner_region_id: region_id.clone(),
                kind: node.kind,
                name: node.display_name.clone(),
                qualified_name: (node.qualified_name != node.display_name)
                    .then(|| node.qualified_name.clone()),
                signature: node.signature.clone(),
                static_roles: roles,
                evidence_ids,
            });
        }
    }
    result
}

fn anchor_rank(node: &FactNode, boundary_endpoint: bool) -> Option<u8> {
    if matches!(
        node.kind,
        FactNodeKind::HttpRoute
            | FactNodeKind::GraphqlEndpoint
            | FactNodeKind::RpcEndpoint
            | FactNodeKind::Entrypoint
            | FactNodeKind::Job
            | FactNodeKind::Event
            | FactNodeKind::ExternalService
            | FactNodeKind::Queue
            | FactNodeKind::Topic
            | FactNodeKind::Cache
    ) {
        return Some(0);
    }
    if !node.roles.is_empty() {
        return Some(1);
    }
    if boundary_endpoint {
        return Some(2);
    }
    if node.visibility == Visibility::Public
        && matches!(
            node.kind,
            FactNodeKind::Class
                | FactNodeKind::Interface
                | FactNodeKind::Trait
                | FactNodeKind::Struct
                | FactNodeKind::Callable
                | FactNodeKind::Function
                | FactNodeKind::Method
        )
    {
        return Some(3);
    }
    None
}

struct BundleAccumulator {
    counts: BTreeMap<(u8, u8), (FactEdgeFamily, FactTruth, u64)>,
    edge_ids: BTreeSet<codebase_fact_model::identity::FactEdgeId>,
    evidence_ids: BTreeSet<EvidenceId>,
}

fn build_boundary_relations(
    workspace_id: &WorkspaceId,
    edges: &[codebase_fact_model::fact_graph::FactEdge],
    owner_by_node: &BTreeMap<FactNodeId, RegionId>,
    regions: &mut [StaticRegionSummary],
    region_index: &BTreeMap<RegionId, usize>,
) -> Result<Vec<BoundaryRelationSummary>, String> {
    let mut bundles = BTreeMap::<(RegionId, RegionId), BundleAccumulator>::new();
    for edge in edges {
        let (Some(source), Some(target)) = (
            owner_by_node.get(&edge.source_id),
            owner_by_node.get(&edge.target_id),
        ) else {
            continue;
        };
        if source == target {
            continue;
        }
        let accumulator = bundles
            .entry((source.clone(), target.clone()))
            .or_insert_with(|| BundleAccumulator {
                counts: BTreeMap::new(),
                edge_ids: BTreeSet::new(),
                evidence_ids: BTreeSet::new(),
            });
        let key = (family_rank(edge.family), truth_rank(edge.truth));
        let count = accumulator
            .counts
            .entry(key)
            .or_insert((edge.family, edge.truth, 0));
        count.2 = count.2.saturating_add(1);
        if accumulator.edge_ids.len() < 16 {
            accumulator.edge_ids.insert(edge.id.clone());
        }
        for evidence_id in &edge.evidence_ids {
            if accumulator.evidence_ids.len() < 32 {
                accumulator.evidence_ids.insert(evidence_id.clone());
            }
        }
    }

    let mut result = Vec::with_capacity(bundles.len());
    for ((source, target), accumulator) in bundles {
        let bundle_id = RelationBundleId::from_components(&[
            "workspace",
            workspace_id.as_str(),
            "source-region",
            source.as_str(),
            "target-region",
            target.as_str(),
        ])
        .map_err(|error| format!("relation bundle identity를 만들지 못했습니다: {error}"))?;
        regions[region_index[&source]]
            .outbound_bundle_ids
            .push(bundle_id.clone());
        regions[region_index[&target]]
            .inbound_bundle_ids
            .push(bundle_id.clone());
        result.push(BoundaryRelationSummary {
            bundle_id,
            source_region_id: source,
            target_region_id: target,
            families: accumulator
                .counts
                .into_values()
                .map(|(family, truth, relation_count)| BoundaryRelationCount {
                    family,
                    truth,
                    relation_count,
                })
                .collect(),
            representative_edge_ids: accumulator.edge_ids.into_iter().collect(),
            evidence_ids: accumulator.evidence_ids.into_iter().collect(),
        });
    }
    Ok(result)
}

fn family_rank(value: FactEdgeFamily) -> u8 {
    match value {
        FactEdgeFamily::Structure => 0,
        FactEdgeFamily::Code => 1,
        FactEdgeFamily::Interface => 2,
        FactEdgeFamily::Data => 3,
        FactEdgeFamily::Integration => 4,
        FactEdgeFamily::Verification => 5,
    }
}

fn truth_rank(value: FactTruth) -> u8 {
    match value {
        FactTruth::Confirmed => 0,
        FactTruth::Structural => 1,
        FactTruth::StaticCandidate => 2,
    }
}

fn build_excerpts(
    repository_root: &Path,
    anchors: &[AnchorFactSummary],
    nodes_by_id: &BTreeMap<FactNodeId, &FactNode>,
    evidence_by_id: &BTreeMap<EvidenceId, &FactEvidence>,
    owner_by_node: &BTreeMap<FactNodeId, RegionId>,
) -> Result<Vec<EvidenceExcerpt>, String> {
    let mut result = Vec::new();
    let mut regions_with_excerpt = BTreeSet::new();
    for anchor in anchors {
        if result.len() >= MAX_EXCERPTS || regions_with_excerpt.contains(&anchor.owner_region_id) {
            continue;
        }
        let Some((file_fact_id, path)) = owning_file(&anchor.fact_id, nodes_by_id) else {
            continue;
        };
        if owner_by_node.get(&file_fact_id) != Some(&anchor.owner_region_id) {
            continue;
        }
        let Some((evidence_id, span)) = anchor.evidence_ids.iter().find_map(|id| {
            let evidence = evidence_by_id.get(id)?;
            match &evidence.location {
                EvidenceLocation::Source { span } if span.path == path => Some((id, span)),
                _ => None,
            }
        }) else {
            continue;
        };
        let source_path = safe_source_path(repository_root, &span.path)?;
        let metadata = fs_err_metadata(&source_path)?;
        if metadata.len() > MAX_EXCERPT_FILE_BYTES {
            continue;
        }
        let bytes = std::fs::read(&source_path)
            .map_err(|error| format!("근거 source를 읽지 못했습니다: {error}"))?;
        if sha256_digest(&bytes) != span.content_digest {
            return Err(format!(
                "source가 게시된 snapshot 이후 변경되었습니다: {}",
                span.path
            ));
        }
        let text = String::from_utf8_lossy(&bytes);
        let lines = text.lines().collect::<Vec<_>>();
        let start_index = usize::try_from(span.start.line.saturating_sub(2)).unwrap_or(0);
        let end_index = usize::try_from(span.end.line.saturating_add(3))
            .unwrap_or(lines.len())
            .min(lines.len());
        let mut excerpt_text = lines[start_index..end_index].join("\n");
        if excerpt_text.len() > MAX_EXCERPT_TEXT_BYTES {
            excerpt_text.truncate(MAX_EXCERPT_TEXT_BYTES);
        }
        if excerpt_text.trim().is_empty() {
            continue;
        }
        result.push(EvidenceExcerpt {
            evidence_id: (*evidence_id).clone(),
            owner_region_id: anchor.owner_region_id.clone(),
            file_fact_id,
            relative_path: path,
            start_line: u32::try_from(start_index + 1).unwrap_or(u32::MAX),
            end_line: u32::try_from(end_index.max(start_index + 1)).unwrap_or(u32::MAX),
            content_hash: span.content_digest,
            text: excerpt_text,
        });
        regions_with_excerpt.insert(anchor.owner_region_id.clone());
    }
    Ok(result)
}

fn owning_file(
    fact_id: &FactNodeId,
    nodes_by_id: &BTreeMap<FactNodeId, &FactNode>,
) -> Option<(FactNodeId, RepositoryPath)> {
    let mut current = Some(fact_id);
    let mut seen = BTreeSet::new();
    while let Some(id) = current {
        if !seen.insert(id.clone()) {
            return None;
        }
        let node = nodes_by_id.get(id)?;
        if node.kind == FactNodeKind::File {
            return RepositoryPath::parse(node.qualified_name.clone())
                .ok()
                .map(|path| (node.id.clone(), path));
        }
        current = node.parent_id.as_ref();
    }
    None
}

fn measure_nonblank_lines(
    repository_root: &Path,
    coverage: &FileCoverageRecord,
) -> Result<u64, String> {
    let expected = coverage.content_digest.ok_or_else(|| {
        format!(
            "indexed source content digest가 없습니다: {}",
            coverage.path
        )
    })?;
    let path = safe_source_path(repository_root, &coverage.path)?;
    let mut file = File::open(&path)
        .map_err(|error| format!("source metric 파일을 열지 못했습니다: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut nonblank_lines = 0_u64;
    let mut line_has_content = false;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("source metric 파일을 읽지 못했습니다: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        for byte in &buffer[..read] {
            if *byte == b'\n' {
                if line_has_content {
                    nonblank_lines = nonblank_lines.saturating_add(1);
                }
                line_has_content = false;
            } else if !byte.is_ascii_whitespace() {
                line_has_content = true;
            }
        }
    }
    if line_has_content {
        nonblank_lines = nonblank_lines.saturating_add(1);
    }
    let actual = digest_from_hasher(hasher)?;
    if actual != expected {
        return Err(format!(
            "source가 게시된 snapshot 이후 변경되었습니다: {}",
            coverage.path
        ));
    }
    Ok(nonblank_lines)
}

fn safe_source_path(repository_root: &Path, relative: &RepositoryPath) -> Result<PathBuf, String> {
    if relative.is_root() {
        return Err("source evidence가 repository root를 가리킵니다".to_string());
    }
    let candidate = relative
        .as_str()
        .split('/')
        .fold(repository_root.to_path_buf(), |path, segment| {
            path.join(segment)
        });
    let canonical = candidate
        .canonicalize()
        .map_err(|error| format!("source evidence 경로를 확인하지 못했습니다: {error}"))?;
    if !canonical.starts_with(repository_root) || !canonical.is_file() {
        return Err("source evidence 경로가 프로젝트 폴더를 벗어났습니다".to_string());
    }
    Ok(canonical)
}

fn fs_err_metadata(path: &Path) -> Result<std::fs::Metadata, String> {
    std::fs::metadata(path).map_err(|error| format!("source metadata를 읽지 못했습니다: {error}"))
}

fn sha256_digest(bytes: &[u8]) -> codebase_fact_model::identity::Sha256Digest {
    codebase_fact_model::identity::Sha256Digest::of_bytes(bytes)
}

fn digest_from_hasher(
    hasher: Sha256,
) -> Result<codebase_fact_model::identity::Sha256Digest, String> {
    codebase_fact_model::identity::Sha256Digest::parse(&format!("{:x}", hasher.finalize()))
        .map_err(|error| format!("source digest를 해석하지 못했습니다: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_regions_are_exact_and_deterministic() {
        assert_eq!(
            directory_region_key("src/orders/create.ts", usize::MAX),
            "path:src/orders"
        );
        assert_eq!(directory_region_key("src/orders/create.ts", 1), "path:src");
        assert_eq!(directory_region_key("main.py", 1), "file:main.py");
    }

    #[test]
    fn relation_order_is_explicit_instead_of_debug_string_dependent() {
        assert!(family_rank(FactEdgeFamily::Code) < family_rank(FactEdgeFamily::Data));
        assert!(truth_rank(FactTruth::Confirmed) < truth_rank(FactTruth::StaticCandidate));
    }

    #[test]
    #[ignore = "requires CODEBASE_TRACE_MANIFEST and CODEBASE_TRACE_REPO from a real engine run"]
    fn real_canonical_trace_reaches_the_semantic_input_without_ai() {
        use crate::fact_graph::CanonicalFactBundleArtifact;
        use codebase_fact_model::fact_graph::{FactBundleManifest, FactNodeDetails};
        use std::{env, fs, path::PathBuf};

        let manifest_path = PathBuf::from(env::var("CODEBASE_TRACE_MANIFEST").unwrap());
        let repository_root = PathBuf::from(env::var("CODEBASE_TRACE_REPO").unwrap())
            .canonicalize()
            .unwrap();
        let manifest: FactBundleManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        let bundle_path = manifest_path
            .parent()
            .unwrap()
            .join(format!("canonical-{}.sqlite", manifest.bundle_digest));
        let app_data = env::temp_dir().join(format!(
            "codebase-workspace-semantic-trace-{}",
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
        let snapshot =
            crate::fact_graph::load_published_snapshot(&app_data, manifest.workspace_id.as_str())
                .unwrap()
                .unwrap();

        let input = build_input(&manifest.workspace_id, &repository_root, &snapshot).unwrap();
        let health_route_id = snapshot
            .nodes
            .iter()
            .find_map(|node| match node.details.as_ref() {
                Some(FactNodeDetails::HttpRoute { path, .. }) if path == "/health" => {
                    Some(node.id.clone())
                }
                _ => None,
            })
            .unwrap();
        let health_trace = input
            .representative_traces
            .iter()
            .find(|trace| trace.entry_fact_id == health_route_id)
            .unwrap();

        assert_eq!(health_trace.ordered_fact_ids.len(), 2);
        assert!(input
            .anchors
            .iter()
            .any(|anchor| anchor.fact_id == health_route_id));
        assert!(input.regions.iter().any(|region| region
            .representative_trace_path_ids
            .contains(&health_trace.trace_path_id)));
        assert!(input
            .representative_traces
            .iter()
            .all(|trace| input.regions.iter().any(|region| region
                .representative_trace_path_ids
                .contains(&trace.trace_path_id))));
        fs::remove_dir_all(app_data).unwrap();
    }
}
