use std::collections::{HashMap, HashSet, VecDeque};

use super::{
    linker::candidate_links_for,
    model::{Evidence, InventoryItem, InventorySnapshot, VisualEdge, VisualMap},
    visual_map::{confirmed_link_edge, visual_node},
};

const MIN_SELECTIONS: usize = 2;
const MAX_SELECTIONS: usize = 8;
const MAX_PATH_HOPS: usize = 8;
const MAX_SEARCHED_NODES: usize = 20_000;
const MAX_GRAPH_SOURCE_ITEMS: usize = 100_000;
const MAX_GRAPH_SOURCE_LINKS: usize = 200_000;
const MAX_SCOPED_CONFIRMED_EDGES: usize = 45_000;
const MAX_SCOPED_CANDIDATE_EDGES: usize = 15_000;
const MAX_SCOPED_EDGES: usize = 80_000;
const MAX_VISIBLE_NODES: usize = 40;
const MAX_VISIBLE_EDGES: usize = 80;

pub(crate) fn validate_composition_request(
    snapshot: &InventorySnapshot,
    focus_ids: &[String],
    relation_view: &str,
) -> Result<(), String> {
    normalize_selection(snapshot, focus_ids.to_vec())?;
    RelationView::parse(relation_view)?;
    Ok(())
}

pub(crate) fn composition_map(
    snapshot: &InventorySnapshot,
    focus_ids: Vec<String>,
    relation_view: &str,
) -> Result<VisualMap, String> {
    let selected = normalize_selection(snapshot, focus_ids)?;
    let view = RelationView::parse(relation_view)?;
    let item_by_id = snapshot
        .items
        .iter()
        .take(MAX_GRAPH_SOURCE_ITEMS)
        .map(|item| (item.id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let scoped_graph = graph_edges(snapshot, &item_by_id, view, &selected);
    let graph_edges = scoped_graph.edges;
    let edge_by_id = graph_edges
        .iter()
        .map(|edge| (edge.id.as_str(), edge))
        .collect::<HashMap<_, _>>();
    let all_adjacency = adjacency(&graph_edges);
    let confirmed_edges = graph_edges
        .iter()
        .filter(|edge| edge.confidence.is_none())
        .cloned()
        .collect::<Vec<_>>();
    let confirmed_adjacency = adjacency(&confirmed_edges);
    let mut included_nodes = selected.iter().cloned().collect::<HashSet<_>>();
    let mut included_edges = HashSet::<String>::new();
    let mut warnings = Vec::new();
    if scoped_graph.truncated {
        warnings.push(format!(
            "{} 관계 후보가 많아 탐색 범위를 {MAX_SEARCHED_NODES}개 노드와 {MAX_SCOPED_EDGES}개 관계로 제한했습니다.",
            view.label()
        ));
    }

    for left in 0..selected.len() {
        for right in left + 1..selected.len() {
            let trusted_path =
                shortest_path(&selected[left], &selected[right], &confirmed_adjacency);
            let path = match trusted_path {
                PathResult::NotFound => {
                    shortest_path(&selected[left], &selected[right], &all_adjacency)
                }
                result => result,
            };
            match path {
                PathResult::Found(path) => {
                    let path_nodes = path_nodes(&selected[left], &path, &edge_by_id);
                    let new_node_count = path_nodes
                        .iter()
                        .filter(|node| !included_nodes.contains(*node))
                        .count();
                    let new_edge_count = path
                        .iter()
                        .filter(|edge| !included_edges.contains(*edge))
                        .count();
                    if included_nodes.len() + new_node_count > MAX_VISIBLE_NODES
                        || included_edges.len() + new_edge_count > MAX_VISIBLE_EDGES
                    {
                        push_unique(
                            &mut warnings,
                            "표시 한도 때문에 일부 선택 쌍의 연결 경로를 접었습니다.".to_string(),
                        );
                        continue;
                    }
                    included_nodes.extend(path_nodes);
                    included_edges.extend(path);
                }
                PathResult::NotFound => warnings.push(format!(
                    "{} ↔ {}: {MAX_PATH_HOPS}단계 안에서 {} 관계를 찾지 못했습니다.",
                    item_name(&selected[left], &item_by_id),
                    item_name(&selected[right], &item_by_id),
                    view.label()
                )),
                PathResult::Truncated => push_unique(
                    &mut warnings,
                    format!(
                        "{} 관계 탐색이 {MAX_SEARCHED_NODES}개 노드 한도에 도달해 일부 연결은 미확인입니다.",
                        view.label()
                    ),
                ),
            }
        }
    }

    if view == RelationView::Impact {
        for edge in &graph_edges {
            if !(selected.contains(&edge.from) || selected.contains(&edge.to)) {
                continue;
            }
            let new_nodes = [&edge.from, &edge.to]
                .into_iter()
                .filter(|node| !included_nodes.contains(node.as_str()))
                .count();
            if included_nodes.len() + new_nodes > MAX_VISIBLE_NODES
                || included_edges.len() == MAX_VISIBLE_EDGES
            {
                push_unique(
                    &mut warnings,
                    "영향 주변 관계가 많아 화면 한도 밖 항목을 접었습니다.".to_string(),
                );
                break;
            }
            included_nodes.insert(edge.from.clone());
            included_nodes.insert(edge.to.clone());
            included_edges.insert(edge.id.clone());
        }
    }

    let mut ordered_ids = selected.clone();
    let mut remainder = included_nodes
        .into_iter()
        .filter(|id| !selected.contains(id))
        .collect::<Vec<_>>();
    remainder.sort();
    ordered_ids.extend(remainder);
    let nodes = ordered_ids
        .iter()
        .filter_map(|id| item_by_id.get(id.as_str()).copied())
        .map(visual_node)
        .collect::<Vec<_>>();
    let visible_ids = nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<HashSet<_>>();
    let mut edges = graph_edges
        .into_iter()
        .filter(|edge| included_edges.contains(&edge.id))
        .filter(|edge| {
            visible_ids.contains(edge.from.as_str()) && visible_ids.contains(edge.to.as_str())
        })
        .collect::<Vec<_>>();
    edges.sort_by(|left, right| {
        edge_truth_rank(left)
            .cmp(&edge_truth_rank(right))
            .then_with(|| left.from.cmp(&right.from))
            .then_with(|| left.to.cmp(&right.to))
            .then_with(|| left.id.cmp(&right.id))
    });
    if edges.iter().any(|edge| edge.confidence.is_some()) {
        warnings
            .push("점선 후보 관계는 이름/경로 근거이며 확정 연결과 분리해 표시합니다.".to_string());
    }

    Ok(VisualMap {
        id: format!("map:{}:composition", snapshot.workspace_id),
        workspace_id: snapshot.workspace_id.clone(),
        mode: "composition".to_string(),
        focus: selected[0].clone(),
        nodes,
        edges,
        warnings,
        review_board: None,
        api_reading: None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RelationView {
    Connections,
    Calls,
    Data,
    Impact,
}

impl RelationView {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "connections" => Ok(Self::Connections),
            "calls" => Ok(Self::Calls),
            "data" => Ok(Self::Data),
            "impact" => Ok(Self::Impact),
            _ => {
                Err("관계 보기는 connections, calls, data, impact 중 하나여야 합니다.".to_string())
            }
        }
    }

    fn allows(self, kind: &str) -> bool {
        match self {
            Self::Connections | Self::Impact => true,
            Self::Calls => matches!(kind, "code_handle" | "code_call"),
            Self::Data => matches!(
                kind,
                "code_handle"
                    | "code_call"
                    | "code_db_read"
                    | "code_db_write"
                    | "code_db_uses_column"
                    | "db_fk"
                    | "db_dependency"
                    | "db_trigger"
                    | "contains"
                    | "candidate_uses"
            ),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Connections => "전체 연결",
            Self::Calls => "호출",
            Self::Data => "데이터",
            Self::Impact => "영향",
        }
    }
}

fn normalize_selection(
    snapshot: &InventorySnapshot,
    focus_ids: Vec<String>,
) -> Result<Vec<String>, String> {
    let requested = focus_ids.len();
    if !(MIN_SELECTIONS..=MAX_SELECTIONS).contains(&requested) {
        return Err(format!(
            "관계 분석 대상은 중복 없이 {MIN_SELECTIONS}~{MAX_SELECTIONS}개를 선택해야 합니다."
        ));
    }
    let mut seen = HashSet::new();
    let selected = focus_ids
        .into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect::<Vec<_>>();
    if selected.len() != requested {
        return Err("관계 분석 대상에는 중복 ID를 넣을 수 없습니다.".to_string());
    }
    if !(MIN_SELECTIONS..=MAX_SELECTIONS).contains(&selected.len()) {
        return Err(format!(
            "관계 분석 대상은 중복 없이 {MIN_SELECTIONS}~{MAX_SELECTIONS}개를 선택해야 합니다."
        ));
    }
    let known = snapshot
        .items
        .iter()
        .take(MAX_GRAPH_SOURCE_ITEMS)
        .map(|item| (item.id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let missing = selected
        .iter()
        .filter(|id| !known.contains_key(id.as_str()))
        .count();
    if missing > 0 {
        if snapshot.items.len() > MAX_GRAPH_SOURCE_ITEMS {
            return Err(format!(
                "선택 대상 {missing}개가 {MAX_GRAPH_SOURCE_ITEMS}개 관계 분석 안전 범위 밖에 있습니다. 범위를 더 좁혀 다시 선택하세요."
            ));
        }
        return Err(format!("선택 대상 {missing}개가 현재 snapshot에 없습니다."));
    }
    let unsupported = selected
        .iter()
        .filter(|id| {
            known
                .get(id.as_str())
                .is_some_and(|item| !composition_item_is_supported(item))
        })
        .count();
    if unsupported > 0 {
        return Err(format!(
            "선택 대상 {unsupported}개는 관계 분석에서 지원하지 않는 항목 종류입니다."
        ));
    }
    Ok(selected)
}

fn composition_item_is_supported(item: &InventoryItem) -> bool {
    item.source == "code"
        || (item.source == "db" && matches!(item.kind.as_str(), "table" | "column"))
}

struct ScopedGraphEdges {
    edges: Vec<VisualEdge>,
    truncated: bool,
}

fn graph_edges(
    snapshot: &InventorySnapshot,
    item_by_id: &HashMap<&str, &InventoryItem>,
    view: RelationView,
    selected: &[String],
) -> ScopedGraphEdges {
    let (scope, mut truncated) = selected_confirmed_scope(snapshot, item_by_id, view, selected);
    let selected_ids = selected.iter().map(String::as_str).collect::<HashSet<_>>();
    let mut edges = Vec::new();
    'confirmed_edges: for direct_to_selection in [true, false] {
        for link in snapshot
            .links
            .iter()
            .take(MAX_GRAPH_SOURCE_LINKS)
            .filter(|link| {
                link.truth_class == "confirmed"
                    && view.allows(&link.kind)
                    && scope.contains(&link.from)
                    && scope.contains(&link.to)
                    && item_by_id.contains_key(link.from.as_str())
                    && item_by_id.contains_key(link.to.as_str())
            })
        {
            let is_direct = selected_ids.contains(link.from.as_str())
                || selected_ids.contains(link.to.as_str());
            if is_direct != direct_to_selection {
                continue;
            }
            if edges.len() == MAX_SCOPED_CONFIRMED_EDGES {
                truncated = true;
                break 'confirmed_edges;
            }
            edges.push(confirmed_link_edge(link, item_by_id));
        }
    }
    if view.allows("candidate_uses") {
        let code_ids = scope
            .iter()
            .filter(|id| {
                item_by_id
                    .get(id.as_str())
                    .is_some_and(|item| item.source == "code")
            })
            .cloned()
            .collect::<HashSet<_>>();
        let (candidate_links, candidates_truncated) = candidate_links_for(
            snapshot,
            &code_ids,
            MAX_SCOPED_CANDIDATE_EDGES,
            MAX_GRAPH_SOURCE_ITEMS,
            MAX_GRAPH_SOURCE_LINKS,
        );
        truncated |= candidates_truncated;
        edges.extend(
            candidate_links
                .iter()
                .filter(|link| {
                    item_by_id.contains_key(link.from.as_str())
                        && item_by_id.contains_key(link.to.as_str())
                })
                .map(|link| VisualEdge {
                    id: link.id.clone(),
                    from: link.from.clone(),
                    to: link.to.clone(),
                    kind: "candidate_uses".to_string(),
                    confidence: Some(link.confidence.clone()),
                    evidence: link.evidence.clone(),
                }),
        );
    }
    if view.allows("contains") {
        edges.extend(scope.iter().filter_map(|item_id| {
            let item = item_by_id.get(item_id.as_str())?;
            let parent = item.parent_id.as_ref()?;
            if !scope.contains(parent) || !item_by_id.contains_key(parent.as_str()) {
                return None;
            }
            Some(VisualEdge {
                id: format!("contains:{parent}->{}", item.id),
                from: parent.clone(),
                to: item.id.clone(),
                kind: "contains".to_string(),
                confidence: None,
                evidence: vec![Evidence {
                    kind: "inventory-parent".to_string(),
                    text: format!(
                        "{} 항목이 {}에 포함됩니다.",
                        item.name,
                        item_name(parent, item_by_id)
                    ),
                }],
            })
        }));
    }
    let mut seen = HashSet::new();
    edges.retain(|edge| seen.insert(edge.id.clone()));
    edges.sort_by(|left, right| {
        let left_selected =
            selected_ids.contains(left.from.as_str()) || selected_ids.contains(left.to.as_str());
        let right_selected =
            selected_ids.contains(right.from.as_str()) || selected_ids.contains(right.to.as_str());
        right_selected
            .cmp(&left_selected)
            .then_with(|| edge_truth_rank(left).cmp(&edge_truth_rank(right)))
            .then_with(|| left.id.cmp(&right.id))
    });
    if edges.len() > MAX_SCOPED_EDGES {
        edges.truncate(MAX_SCOPED_EDGES);
        truncated = true;
    }
    ScopedGraphEdges { edges, truncated }
}

fn selected_confirmed_scope(
    snapshot: &InventorySnapshot,
    item_by_id: &HashMap<&str, &InventoryItem>,
    view: RelationView,
    selected: &[String],
) -> (HashSet<String>, bool) {
    let mut neighbors = HashMap::<&str, Vec<&str>>::new();
    let truncated = snapshot.links.len() > MAX_GRAPH_SOURCE_LINKS
        || snapshot.items.len() > MAX_GRAPH_SOURCE_ITEMS;
    for link in snapshot
        .links
        .iter()
        .take(MAX_GRAPH_SOURCE_LINKS)
        .filter(|link| {
            link.truth_class == "confirmed"
                && view.allows(&link.kind)
                && item_by_id.contains_key(link.from.as_str())
                && item_by_id.contains_key(link.to.as_str())
        })
    {
        neighbors
            .entry(link.from.as_str())
            .or_default()
            .push(link.to.as_str());
        neighbors
            .entry(link.to.as_str())
            .or_default()
            .push(link.from.as_str());
    }
    if view.allows("contains") {
        for item in snapshot.items.iter().take(MAX_GRAPH_SOURCE_ITEMS) {
            let Some(parent) = item.parent_id.as_deref() else {
                continue;
            };
            if !item_by_id.contains_key(parent) {
                continue;
            }
            neighbors.entry(item.id.as_str()).or_default().push(parent);
            neighbors.entry(parent).or_default().push(item.id.as_str());
        }
    }
    for values in neighbors.values_mut() {
        values.sort_unstable();
        values.dedup();
    }

    let mut scope = selected.iter().cloned().collect::<HashSet<_>>();
    let mut queue = selected
        .iter()
        .map(|id| (id.as_str(), 0usize))
        .collect::<VecDeque<_>>();
    while let Some((current, depth)) = queue.pop_front() {
        if depth == MAX_PATH_HOPS {
            continue;
        }
        for next in neighbors.get(current).into_iter().flatten() {
            if scope.contains(*next) {
                continue;
            }
            if scope.len() == MAX_SEARCHED_NODES {
                return (scope, true);
            }
            scope.insert((*next).to_string());
            queue.push_back((next, depth + 1));
        }
    }
    (scope, truncated)
}

fn adjacency(edges: &[VisualEdge]) -> HashMap<&str, Vec<(&str, &str)>> {
    let mut adjacency = HashMap::<&str, Vec<(&str, &str)>>::new();
    for edge in edges {
        adjacency
            .entry(edge.from.as_str())
            .or_default()
            .push((edge.to.as_str(), edge.id.as_str()));
        adjacency
            .entry(edge.to.as_str())
            .or_default()
            .push((edge.from.as_str(), edge.id.as_str()));
    }
    for neighbors in adjacency.values_mut() {
        neighbors.sort_by(|left, right| left.0.cmp(right.0).then_with(|| left.1.cmp(right.1)));
    }
    adjacency
}

enum PathResult {
    Found(Vec<String>),
    NotFound,
    Truncated,
}

fn shortest_path(
    start: &str,
    target: &str,
    adjacency: &HashMap<&str, Vec<(&str, &str)>>,
) -> PathResult {
    let mut queue = VecDeque::from([(start, 0usize)]);
    let mut visited = HashSet::from([start]);
    let mut previous = HashMap::<&str, (&str, &str)>::new();
    while let Some((node, depth)) = queue.pop_front() {
        if node == target {
            let mut path = Vec::new();
            let mut current = target;
            while current != start {
                let Some((parent, edge)) = previous.get(current).copied() else {
                    return PathResult::NotFound;
                };
                path.push(edge.to_string());
                current = parent;
            }
            path.reverse();
            return PathResult::Found(path);
        }
        if depth == MAX_PATH_HOPS {
            continue;
        }
        for (next, edge) in adjacency.get(node).into_iter().flatten() {
            if visited.insert(*next) {
                if visited.len() > MAX_SEARCHED_NODES {
                    return PathResult::Truncated;
                }
                previous.insert(*next, (node, *edge));
                queue.push_back((*next, depth + 1));
            }
        }
    }
    PathResult::NotFound
}

fn path_nodes(
    start: &str,
    path: &[String],
    edge_by_id: &HashMap<&str, &VisualEdge>,
) -> HashSet<String> {
    let mut nodes = HashSet::from([start.to_string()]);
    for edge_id in path {
        if let Some(edge) = edge_by_id.get(edge_id.as_str()) {
            nodes.insert(edge.from.clone());
            nodes.insert(edge.to.clone());
        }
    }
    nodes
}

fn item_name(id: &str, items: &HashMap<&str, &InventoryItem>) -> String {
    items
        .get(id)
        .map_or_else(|| id.to_string(), |item| item.name.clone())
}

fn edge_truth_rank(edge: &VisualEdge) -> u8 {
    u8::from(edge.confidence.is_some())
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atlas::{item, model::SnapshotLink};

    #[test]
    fn data_view_projects_only_the_path_between_selected_code_and_table() {
        let snapshot = fixture();
        let item_by_id = snapshot
            .items
            .iter()
            .map(|item| (item.id.as_str(), item))
            .collect::<HashMap<_, _>>();
        let available = graph_edges(
            &snapshot,
            &item_by_id,
            RelationView::Data,
            &["code:route".to_string(), "db:table:orders".to_string()],
        )
        .edges;
        assert!(["code_handle", "code_call", "code_db_read"]
            .into_iter()
            .all(|kind| available.iter().any(|edge| edge.kind == kind)));
        let map = composition_map(
            &snapshot,
            vec!["code:route".to_string(), "db:table:orders".to_string()],
            "data",
        )
        .unwrap();

        assert_eq!(
            map.nodes
                .iter()
                .map(|node| node.id.as_str())
                .collect::<HashSet<_>>(),
            HashSet::from(["code:route", "code:handler", "code:repo", "db:table:orders"])
        );
        assert_eq!(map.edges.len(), 3);
        assert!(map.edges.iter().any(|edge| edge.kind == "code_db_read"));
    }

    #[test]
    fn disconnected_selections_remain_visible() {
        let map = composition_map(
            &fixture(),
            vec!["code:route".to_string(), "db:table:audit".to_string()],
            "calls",
        )
        .unwrap();

        assert_eq!(map.nodes.len(), 2);
        assert!(map
            .warnings
            .iter()
            .any(|warning| warning.contains("찾지 못했습니다")));
    }

    #[test]
    fn candidate_table_can_bridge_two_selected_code_components() {
        let mut snapshot = fixture();
        snapshot.items.extend([
            item(
                "code:route-b",
                "api",
                "GET /archived-orders",
                "api",
                "code",
                None,
                None,
            ),
            item(
                "code:repo-b",
                "repository",
                "OrderRepository",
                "code",
                "code",
                None,
                None,
            ),
        ]);
        snapshot.items.iter_mut().for_each(|item| {
            if item.id == "code:repo" {
                item.name = "OrderRepository".to_string();
                item.kind = "repository".to_string();
            }
        });
        snapshot.links.push(link(
            "calls-b",
            "code:route-b",
            "code:repo-b",
            "code_call",
            "CALLS",
        ));
        snapshot.links.retain(|link| link.kind != "code_db_read");

        let map = composition_map(
            &snapshot,
            vec!["code:route".to_string(), "code:route-b".to_string()],
            "connections",
        )
        .unwrap();

        assert!(map.nodes.iter().any(|node| node.id == "db:table:orders"));
        assert_eq!(
            map.edges
                .iter()
                .filter(|edge| edge.kind == "candidate_uses")
                .count(),
            2
        );
    }

    #[test]
    fn selection_count_is_bounded() {
        assert!(
            composition_map(&fixture(), vec!["code:route".to_string()], "connections").is_err()
        );
        let oversized = (0..9)
            .map(|index| format!("missing:{index}"))
            .collect::<Vec<_>>();
        assert!(validate_composition_request(&fixture(), &oversized, "connections").is_err());
    }

    #[test]
    fn validation_rejects_duplicates_unknown_views_and_unsupported_db_objects() {
        let mut snapshot = fixture();
        snapshot.items.push(item(
            "db:index:orders",
            "index",
            "idx_orders",
            "db",
            "db",
            None,
            None,
        ));
        assert!(validate_composition_request(
            &snapshot,
            &["code:route".to_string(), "code:route".to_string()],
            "connections",
        )
        .is_err());
        assert!(validate_composition_request(
            &snapshot,
            &["code:route".to_string(), "db:table:orders".to_string()],
            "everything",
        )
        .is_err());
        assert!(validate_composition_request(
            &snapshot,
            &["code:route".to_string(), "db:index:orders".to_string()],
            "connections",
        )
        .is_err());
    }

    fn fixture() -> InventorySnapshot {
        InventorySnapshot {
            schema_version: 2,
            workspace_id: "workspace".to_string(),
            saved_at: "1".to_string(),
            metadata: Default::default(),
            stale_reasons: Vec::new(),
            items: vec![
                item(
                    "code:route",
                    "api",
                    "GET /orders",
                    "api",
                    "code",
                    None,
                    None,
                ),
                item(
                    "code:handler",
                    "function",
                    "handle",
                    "code",
                    "code",
                    None,
                    None,
                ),
                item("code:repo", "function", "load", "code", "code", None, None),
                item("db:table:orders", "table", "orders", "db", "db", None, None),
                item("db:table:audit", "table", "audit", "db", "db", None, None),
            ],
            links: vec![
                link(
                    "handles",
                    "code:route",
                    "code:handler",
                    "code_handle",
                    "HANDLES",
                ),
                link("calls", "code:handler", "code:repo", "code_call", "CALLS"),
                link(
                    "reads",
                    "code:repo",
                    "db:table:orders",
                    "code_db_read",
                    "READS",
                ),
            ],
        }
    }

    fn link(id: &str, from: &str, to: &str, kind: &str, edge_type: &str) -> SnapshotLink {
        SnapshotLink {
            id: id.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            kind: kind.to_string(),
            label: Some(edge_type.to_string()),
            truth_class: "confirmed".to_string(),
            direction: "outbound".to_string(),
            engine_edge_type: Some(edge_type.to_string()),
            evidence: Vec::new(),
        }
    }
}
