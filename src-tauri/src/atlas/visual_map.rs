use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use super::linker::{candidate_links, merge_evidence, MAX_CANDIDATES_PER_CODE_ITEM};
use super::model::{
    ApiReadingAnswer, ApiReadingStep, CandidateLink, Evidence, ImpactReviewBoard, ImpactReviewItem,
    ImpactReviewLane, InventoryItem, InventorySnapshot, SnapshotLink, SourceLocation, VisualEdge,
    VisualMap, VisualNode,
};
#[cfg(test)]
use super::snapshot::{item, timestamp};
pub fn visual_map(snapshot: &InventorySnapshot, focus: Option<String>, mode: String) -> VisualMap {
    if matches!(mode.as_str(), "atlas" | "explore") {
        if let Some(group_id) = focus.as_deref().filter(|id| id.starts_with("group:")) {
            return atlas_group_detail(snapshot, group_id, mode);
        }
    }
    let focus = focus.filter(|id| snapshot.items.iter().any(|item| item.id == *id));
    if focus.is_none() && matches!(mode.as_str(), "atlas" | "explore") {
        return atlas_overview(snapshot, mode);
    }
    if focus.is_none() {
        return narrow_focus_map(snapshot, mode);
    }
    if mode == "api-flow" {
        return api_flow_map(snapshot, focus.unwrap(), mode);
    }
    if mode == "table-usage"
        && focus
            .as_deref()
            .is_some_and(|focus_id| focus_id.starts_with("db:table:"))
    {
        return table_detail_map(snapshot, focus.unwrap(), mode, true);
    }
    if mode == "column-impact"
        && focus
            .as_deref()
            .is_some_and(|focus_id| focus_id.starts_with("db:column:"))
    {
        return column_impact_map(snapshot, focus.unwrap(), mode);
    }

    let candidates = candidate_links(snapshot);
    let mut included = HashSet::new();

    if let Some(focus_id) = &focus {
        included.insert(focus_id.clone());
        for link in &candidates {
            if link.from == *focus_id || link.to == *focus_id {
                included.insert(link.from.clone());
                included.insert(link.to.clone());
            }
        }
        for item in &snapshot.items {
            if item.parent_id.as_deref() == Some(focus_id) {
                included.insert(item.id.clone());
            }
            if included.contains(&item.id) {
                if let Some(parent) = &item.parent_id {
                    included.insert(parent.clone());
                }
            }
        }
    } else {
        included.extend(
            snapshot
                .items
                .iter()
                .filter(|item| item.parent_id.is_none())
                .map(|item| item.id.clone()),
        );
    }

    let item_by_id: HashMap<&str, &InventoryItem> = snapshot
        .items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect();
    include_snapshot_link_neighbors(snapshot, &item_by_id, &mut included);
    let mut included_ids: Vec<&String> = included.iter().collect();
    included_ids.sort();
    let cap = mode_node_cap(&mode);
    let included_count = included_ids.len();
    let mut nodes: Vec<VisualNode> = included_ids
        .into_iter()
        .filter_map(|id| item_by_id.get(id.as_str()))
        .take(cap)
        .map(|item| VisualNode {
            id: item.id.clone(),
            kind: item.kind.clone(),
            title: item.name.clone(),
            subtitle: item.path.clone(),
            layer: item.layer.clone(),
            source: item.source.clone(),
        })
        .collect();
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    let visible_ids = nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<HashSet<_>>();

    let mut edges: Vec<VisualEdge> = snapshot
        .items
        .iter()
        .filter_map(|item| {
            let parent = item.parent_id.as_ref()?;
            if visible_ids.contains(item.id.as_str()) && visible_ids.contains(parent.as_str()) {
                Some(VisualEdge {
                    id: format!("contains:{}->{}", parent, item.id),
                    from: parent.clone(),
                    to: item.id.clone(),
                    kind: "contains".to_string(),
                    confidence: None,
                    evidence: Vec::new(),
                })
            } else {
                None
            }
        })
        .collect();
    edges.extend(candidates.iter().filter_map(|link| {
        if visible_ids.contains(link.from.as_str()) && visible_ids.contains(link.to.as_str()) {
            Some(VisualEdge {
                id: link.id.clone(),
                from: link.from.clone(),
                to: link.to.clone(),
                kind: "candidate_uses".to_string(),
                confidence: Some(link.confidence.clone()),
                evidence: link.evidence.clone(),
            })
        } else {
            None
        }
    }));
    edges.extend(confirmed_link_edges(snapshot, &visible_ids, &item_by_id));

    VisualMap {
        id: format!(
            "map:{}:{}",
            snapshot.workspace_id,
            focus.as_deref().unwrap_or("overview")
        ),
        workspace_id: snapshot.workspace_id.clone(),
        mode,
        focus: focus.unwrap_or_else(|| "overview".to_string()),
        nodes,
        edges,
        warnings: if included_count > cap {
            vec![format!(
                "결과가 너무 넓어 {cap}개 항목만 표시합니다. 대상을 좁히세요."
            )]
        } else {
            Vec::new()
        },
        review_board: None,
        api_reading: None,
    }
}

fn table_detail_map(
    snapshot: &InventorySnapshot,
    table_id: String,
    mode: String,
    include_candidates: bool,
) -> VisualMap {
    let item_by_id: HashMap<&str, &InventoryItem> = snapshot
        .items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect();
    let Some(table) = item_by_id.get(table_id.as_str()).copied() else {
        return narrow_focus_map(snapshot, mode);
    };
    if table.source != "db" || table.kind != "table" {
        return focus_neighborhood_map(snapshot, Some(table_id), mode);
    }

    let mut columns = snapshot
        .items
        .iter()
        .filter(|item| {
            item.kind == "column" && item.parent_id.as_deref() == Some(table.id.as_str())
        })
        .collect::<Vec<_>>();
    columns.sort_by(|a, b| {
        (!a.is_primary_key, !a.is_foreign_key, a.name.as_str()).cmp(&(
            !b.is_primary_key,
            !b.is_foreign_key,
            b.name.as_str(),
        ))
    });

    let mut usage_candidates = if include_candidates {
        candidate_links(snapshot)
            .into_iter()
            .filter(|link| link.to == table.id)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    usage_candidates.sort_by(|a, b| {
        confidence_rank(&a.confidence)
            .cmp(&confidence_rank(&b.confidence))
            .then_with(|| a.from.cmp(&b.from))
    });
    let candidate_limit = 10;
    let candidate_count = usage_candidates.len();
    let candidate_nodes = usage_candidates
        .iter()
        .take(candidate_limit)
        .filter_map(|link| item_by_id.get(link.from.as_str()).copied())
        .collect::<Vec<_>>();
    let review_candidate_edges = usage_candidates
        .iter()
        .map(|link| VisualEdge {
            id: link.id.clone(),
            from: link.from.clone(),
            to: link.to.clone(),
            kind: "candidate_uses".to_string(),
            confidence: Some(link.confidence.clone()),
            evidence: link.evidence.clone(),
        })
        .collect::<Vec<_>>();
    let fk_links = db_fk_links_for_table(snapshot, &item_by_id, table.id.as_str());
    let fk_nodes = linked_items(&fk_links, &item_by_id);
    let mut db_objects = snapshot
        .items
        .iter()
        .filter(|item| {
            item.parent_id.as_deref() == Some(table.id.as_str())
                && matches!(item.kind.as_str(), "constraint" | "index")
        })
        .collect::<Vec<_>>();
    db_objects.sort_by_key(|item| {
        (
            direct_review_rank(&direct_object_kind(item, &[])),
            item.name.clone(),
        )
    });

    let cap = mode_node_cap(&mode);
    let included_count = 1
        + columns.len()
        + db_objects.len()
        + fk_nodes.len()
        + candidate_count.min(candidate_limit);
    let mut nodes = Vec::with_capacity(cap.min(included_count));
    nodes.push(visual_node(table));
    let mut node_ids = nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<HashSet<_>>();
    for item in db_objects
        .into_iter()
        .chain(candidate_nodes)
        .chain(fk_nodes)
        .chain(columns)
    {
        if nodes.len() >= cap {
            break;
        }
        if node_ids.insert(item.id.clone()) {
            nodes.push(visual_node(item));
        }
    }
    let visible_ids = nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<HashSet<_>>();

    let mut edges = snapshot
        .items
        .iter()
        .filter_map(|item| {
            if item.parent_id.as_deref() == Some(table.id.as_str())
                && visible_ids.contains(item.id.as_str())
            {
                Some(VisualEdge {
                    id: format!("contains:{}->{}", table.id, item.id),
                    from: table.id.clone(),
                    to: item.id.clone(),
                    kind: "contains".to_string(),
                    confidence: None,
                    evidence: Vec::new(),
                })
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    if include_candidates {
        edges.extend(usage_candidates.into_iter().filter_map(|link| {
            if link.to == table.id
                && visible_ids.contains(link.from.as_str())
                && visible_ids.contains(link.to.as_str())
            {
                Some(VisualEdge {
                    id: link.id,
                    from: link.from,
                    to: link.to,
                    kind: "candidate_uses".to_string(),
                    confidence: Some(link.confidence),
                    evidence: link.evidence,
                })
            } else {
                None
            }
        }));
    }
    edges.extend(confirmed_link_edges(snapshot, &visible_ids, &item_by_id));
    edges.sort_by_key(|edge| edge.evidence.is_empty());
    let mut edge_ids = HashSet::new();
    edges.retain(|edge| edge_ids.insert(edge.id.clone()));

    let mut warnings = Vec::new();
    if included_count > cap {
        warnings.push(format!(
            "테이블 컬럼이 많아 {cap}개 항목만 표시합니다. 컬럼 검색으로 좁히세요."
        ));
    }
    if candidate_count > candidate_limit {
        warnings.push(format!(
            "코드 후보가 많아 상위 {candidate_limit}개만 표시합니다."
        ));
    }
    if included_count == 1 {
        warnings.push("이 테이블에는 표시할 컬럼 구조가 없습니다.".to_string());
    }

    let review_board = impact_review_board(snapshot, table, None, &review_candidate_edges);
    VisualMap {
        id: format!("map:{}:{}", snapshot.workspace_id, table.id),
        workspace_id: snapshot.workspace_id.clone(),
        mode,
        focus: table.id.clone(),
        nodes,
        edges,
        warnings,
        review_board: Some(review_board),
        api_reading: None,
    }
}

fn column_impact_map(snapshot: &InventorySnapshot, column_id: String, mode: String) -> VisualMap {
    let item_by_id: HashMap<&str, &InventoryItem> = snapshot
        .items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect();
    let Some(column) = item_by_id.get(column_id.as_str()).copied() else {
        return narrow_focus_map(snapshot, mode);
    };
    if column.source != "db" || column.kind != "column" {
        return focus_neighborhood_map(snapshot, Some(column_id), mode);
    }

    let parent_table = column
        .parent_id
        .as_deref()
        .and_then(|id| item_by_id.get(id).copied());
    let cap = mode_node_cap(&mode);
    let mut nodes = Vec::with_capacity(cap);
    if let Some(table) = parent_table {
        nodes.push(visual_node(table));
    }
    nodes.push(visual_node(column));

    let mut direct_objects = snapshot
        .links
        .iter()
        .filter(|link| {
            matches!(link.kind.as_str(), "db_constraint" | "db_index") && link.to == column.id
        })
        .filter_map(|link| item_by_id.get(link.from.as_str()).copied())
        .collect::<Vec<_>>();
    direct_objects.sort_by_key(|item| item.name.clone());
    let mut direct_object_ids = HashSet::new();
    direct_objects.retain(|item| direct_object_ids.insert(item.id.as_str()));
    for item in direct_objects {
        if nodes.len() < cap {
            nodes.push(visual_node(item));
        }
    }

    let has_primary_object = snapshot.links.iter().any(|link| {
        link.kind == "db_constraint"
            && link.to == column.id
            && item_by_id
                .get(link.from.as_str())
                .is_some_and(|item| item.is_primary_key)
    });
    let has_foreign_object = snapshot.links.iter().any(|link| {
        link.kind == "db_constraint"
            && link.to == column.id
            && item_by_id
                .get(link.from.as_str())
                .is_some_and(|item| item.is_foreign_key)
    });
    if column.is_primary_key && !has_primary_object {
        nodes.push(constraint_node(column, "pk", "Primary Key"));
    }
    if column.is_foreign_key && !has_foreign_object {
        nodes.push(constraint_node(column, "fk", "Foreign Key"));
    }

    let fk_links = db_fk_links_for_column(snapshot, column.id.as_str());
    let fk_nodes = linked_items(&fk_links, &item_by_id);
    for item in fk_nodes {
        if nodes.len() >= cap {
            break;
        }
        if nodes.iter().all(|node| node.id != item.id) {
            nodes.push(visual_node(item));
        }
    }

    let all_candidates = column_reference_candidates(snapshot, column);
    let candidate_count = all_candidates.len();
    let candidates = all_candidates.iter().take(8).cloned().collect::<Vec<_>>();
    nodes.extend(candidates.iter().filter_map(|candidate| {
        item_by_id
            .get(candidate.from.as_str())
            .copied()
            .map(visual_node)
    }));
    nodes.truncate(cap);

    let visible_ids = nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<HashSet<_>>();
    let mut edges = Vec::new();
    if let Some(table) = parent_table.filter(|table| visible_ids.contains(table.id.as_str())) {
        edges.push(VisualEdge {
            id: format!("contains:{}->{}", table.id, column.id),
            from: table.id.clone(),
            to: column.id.clone(),
            kind: "contains".to_string(),
            confidence: None,
            evidence: Vec::new(),
        });
    }
    if column.is_primary_key && !has_primary_object {
        edges.push(constraint_edge(column, "pk"));
    }
    if column.is_foreign_key && !has_foreign_object {
        edges.push(constraint_edge(column, "fk"));
    }
    edges.extend(confirmed_link_edges(snapshot, &visible_ids, &item_by_id));
    edges.extend(candidates.into_iter().filter_map(|candidate| {
        if visible_ids.contains(candidate.from.as_str())
            && visible_ids.contains(candidate.to.as_str())
        {
            Some(candidate)
        } else {
            None
        }
    }));
    edges.sort_by_key(|edge| edge.evidence.is_empty());
    let mut edge_ids = HashSet::new();
    edges.retain(|edge| edge_ids.insert(edge.id.clone()));

    let mut warnings = vec![
        "컬럼 변경 범위는 DB 구조와 이름 기반 코드 후보만 사용합니다. 행 데이터는 조회하지 않습니다."
            .to_string(),
    ];
    if column.is_foreign_key
        && !snapshot
            .links
            .iter()
            .any(|link| link.kind == "db_fk" && (link.from == column.id || link.to == column.id))
    {
        warnings.push(
            "FK 대상 테이블 정보는 현재 읽은 DB 구조에 없어 직접 관련 테이블로 표시하지 않습니다."
                .to_string(),
        );
    }

    let review_board = impact_review_board(
        snapshot,
        parent_table.unwrap_or(column),
        Some(column),
        &all_candidates,
    );
    VisualMap {
        id: format!("map:{}:{}", snapshot.workspace_id, column.id),
        workspace_id: snapshot.workspace_id.clone(),
        mode,
        focus: column.id.clone(),
        nodes,
        edges,
        warnings: if candidate_count > 8 {
            warnings
                .into_iter()
                .chain(["후보 코드 참조가 많아 상위 8개만 표시합니다.".to_string()])
                .collect()
        } else {
            warnings
        },
        review_board: Some(review_board),
        api_reading: None,
    }
}

fn api_flow_map(snapshot: &InventorySnapshot, focus_id: String, mode: String) -> VisualMap {
    let item_by_id: HashMap<&str, &InventoryItem> = snapshot
        .items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect();
    let Some(route) = item_by_id.get(focus_id.as_str()).copied() else {
        return narrow_focus_map(snapshot, mode);
    };
    if route.source != "code" || route.layer != "api" {
        return focus_neighborhood_map(snapshot, Some(focus_id), mode);
    }

    let traversal = reachable_api_flow_links(
        snapshot,
        route.id.as_str(),
        API_CALL_HOP_LIMIT,
        API_CODE_NODE_LIMIT,
        API_EDGE_LIMIT,
    );
    let has_confirmed_handler = traversal
        .links
        .iter()
        .any(|link| link.kind == "code_handle");
    let reachable_code_ids = traversal
        .node_order
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let all_candidates = has_confirmed_handler.then(|| candidate_links(snapshot));
    let candidate_linker_cap_reached = all_candidates.as_ref().is_some_and(|links| {
        let mut counts = HashMap::<&str, usize>::new();
        links
            .iter()
            .filter(|link| reachable_code_ids.contains(link.from.as_str()))
            .any(|link| {
                let count = counts.entry(link.from.as_str()).or_default();
                *count += 1;
                *count == MAX_CANDIDATES_PER_CODE_ITEM
            })
    });
    let mut candidates = if let Some(all_candidates) = all_candidates {
        all_candidates
            .into_iter()
            .filter(|link| reachable_code_ids.contains(link.from.as_str()))
            .filter(|link| {
                item_by_id
                    .get(link.to.as_str())
                    .is_some_and(|item| item.source == "db" && item.kind == "table")
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    candidates.sort_by(|left, right| {
        confidence_rank(left.confidence.as_str())
            .cmp(&confidence_rank(right.confidence.as_str()))
            .then_with(|| left.from.cmp(&right.from))
            .then_with(|| left.to.cmp(&right.to))
            .then_with(|| left.id.cmp(&right.id))
    });

    let mut unique_targets = HashMap::<String, usize>::new();
    let mut merged_candidates = Vec::<CandidateLink>::new();
    for mut candidate in candidates {
        if let (Some(source), Some(target)) = (
            item_by_id.get(candidate.from.as_str()),
            item_by_id.get(candidate.to.as_str()),
        ) {
            candidate.evidence.push(Evidence {
                kind: "candidate-source".to_string(),
                text: format!(
                    "{} 코드에서 {} 테이블 후보를 찾았습니다.",
                    source.name, target.name
                ),
            });
        }
        if let Some(index) = unique_targets.get(candidate.to.as_str()).copied() {
            merged_candidates[index].evidence.extend(candidate.evidence);
        } else {
            unique_targets.insert(candidate.to.clone(), merged_candidates.len());
            merged_candidates.push(candidate);
        }
    }
    for candidate in &mut merged_candidates {
        let mut seen = HashSet::new();
        candidate
            .evidence
            .retain(|entry| seen.insert((entry.kind.clone(), entry.text.clone())));
    }
    let mut candidates = merged_candidates;
    let hidden_candidates = candidates.len().saturating_sub(API_DB_CANDIDATE_LIMIT);
    candidates.truncate(API_DB_CANDIDATE_LIMIT);

    let mut included_ids = vec![route.id.clone()];
    included_ids.extend(traversal.node_order.iter().cloned());
    included_ids.extend(candidates.iter().map(|link| link.to.clone()));
    let mut seen_nodes = HashSet::new();
    let nodes = included_ids
        .into_iter()
        .filter(|id| seen_nodes.insert(id.clone()))
        .filter_map(|id| item_by_id.get(id.as_str()))
        .map(|item| visual_node(item))
        .collect::<Vec<_>>();
    let visible_ids = nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<HashSet<_>>();
    let mut edges = traversal
        .links
        .iter()
        .copied()
        .filter(|link| {
            visible_ids.contains(link.from.as_str()) && visible_ids.contains(link.to.as_str())
        })
        .map(|link| confirmed_link_edge(link, &item_by_id))
        .collect::<Vec<_>>();
    edges.extend(candidates.iter().filter_map(|link| {
        if visible_ids.contains(link.from.as_str()) && visible_ids.contains(link.to.as_str()) {
            Some(VisualEdge {
                id: link.id.clone(),
                from: link.from.clone(),
                to: link.to.clone(),
                kind: "candidate_uses".to_string(),
                confidence: Some(link.confidence.clone()),
                evidence: link.evidence.clone(),
            })
        } else {
            None
        }
    }));
    let api_reading = api_reading_answer(
        snapshot,
        route,
        &traversal,
        &candidates,
        hidden_candidates,
        candidate_linker_cap_reached,
        &item_by_id,
    );

    VisualMap {
        id: format!("map:{}:{}", snapshot.workspace_id, route.id),
        workspace_id: snapshot.workspace_id.clone(),
        mode,
        focus: route.id.clone(),
        nodes,
        edges,
        warnings: {
            let mut warnings = Vec::new();
            if !has_confirmed_handler {
                warnings.push("확정 HANDLES 없음: handler 이후 구간은 알 수 없습니다.".to_string());
            }
            if api_reading.truncated {
                warnings.push(format!(
                    "API 읽기 경로 일부를 접었습니다: {}",
                    api_reading
                        .truncation_reason
                        .as_deref()
                        .unwrap_or("표시 한도 도달")
                ));
            }
            warnings
        },
        review_board: None,
        api_reading: Some(api_reading),
    }
}

fn focus_neighborhood_map(
    snapshot: &InventorySnapshot,
    focus: Option<String>,
    mode: String,
) -> VisualMap {
    let candidates = candidate_links(snapshot);
    let mut included = HashSet::new();

    if let Some(focus_id) = &focus {
        included.insert(focus_id.clone());
        for link in &candidates {
            if link.from == *focus_id || link.to == *focus_id {
                included.insert(link.from.clone());
                included.insert(link.to.clone());
            }
        }
        for item in &snapshot.items {
            if item.parent_id.as_deref() == Some(focus_id) {
                included.insert(item.id.clone());
            }
            if included.contains(&item.id) {
                if let Some(parent) = &item.parent_id {
                    included.insert(parent.clone());
                }
            }
        }
    }

    let item_by_id: HashMap<&str, &InventoryItem> = snapshot
        .items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect();
    include_snapshot_link_neighbors(snapshot, &item_by_id, &mut included);
    let mut included_ids: Vec<&String> = included.iter().collect();
    included_ids.sort();
    let cap = mode_node_cap(&mode);
    let included_count = included_ids.len();
    let mut nodes: Vec<VisualNode> = included_ids
        .into_iter()
        .filter_map(|id| item_by_id.get(id.as_str()))
        .take(cap)
        .map(|item| visual_node(item))
        .collect();
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    let visible_ids = nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<HashSet<_>>();

    let mut edges: Vec<VisualEdge> = snapshot
        .items
        .iter()
        .filter_map(|item| {
            let parent = item.parent_id.as_ref()?;
            if visible_ids.contains(item.id.as_str()) && visible_ids.contains(parent.as_str()) {
                Some(VisualEdge {
                    id: format!("contains:{}->{}", parent, item.id),
                    from: parent.clone(),
                    to: item.id.clone(),
                    kind: "contains".to_string(),
                    confidence: None,
                    evidence: Vec::new(),
                })
            } else {
                None
            }
        })
        .collect();
    edges.extend(candidates.into_iter().filter_map(|link| {
        if visible_ids.contains(link.from.as_str()) && visible_ids.contains(link.to.as_str()) {
            Some(VisualEdge {
                id: link.id,
                from: link.from,
                to: link.to,
                kind: "candidate_uses".to_string(),
                confidence: Some(link.confidence),
                evidence: link.evidence,
            })
        } else {
            None
        }
    }));
    edges.extend(confirmed_link_edges(snapshot, &visible_ids, &item_by_id));

    VisualMap {
        id: format!(
            "map:{}:{}",
            snapshot.workspace_id,
            focus.as_deref().unwrap_or("overview")
        ),
        workspace_id: snapshot.workspace_id.clone(),
        mode,
        focus: focus.unwrap_or_else(|| "overview".to_string()),
        nodes,
        edges,
        warnings: if included_count > cap {
            vec![format!(
                "결과가 너무 넓어 {cap}개 항목만 표시합니다. 대상을 좁히세요."
            )]
        } else {
            Vec::new()
        },
        review_board: None,
        api_reading: None,
    }
}

fn visual_node(item: &InventoryItem) -> VisualNode {
    VisualNode {
        id: item.id.clone(),
        kind: item.kind.clone(),
        title: item.name.clone(),
        subtitle: node_subtitle(item),
        layer: item.layer.clone(),
        source: item.source.clone(),
    }
}

fn node_subtitle(item: &InventoryItem) -> Option<String> {
    if item.kind != "column" {
        return item.path.clone();
    }

    let mut parts = Vec::new();
    if let Some(data_type) = item.path.as_deref().filter(|value| !value.is_empty()) {
        parts.push(data_type.to_string());
    }
    if item.is_primary_key {
        parts.push("PK".to_string());
    }
    if item.is_foreign_key {
        parts.push("FK".to_string());
    }
    if let Some(nullable) = item.nullable {
        parts.push(if nullable { "NULL" } else { "NOT NULL" }.to_string());
    }

    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn confirmed_link_edges(
    snapshot: &InventorySnapshot,
    visible_ids: &HashSet<&str>,
    item_by_id: &HashMap<&str, &InventoryItem>,
) -> Vec<VisualEdge> {
    snapshot
        .links
        .iter()
        .filter(|link| {
            link.truth_class == "confirmed"
                && visible_ids.contains(link.from.as_str())
                && visible_ids.contains(link.to.as_str())
        })
        .map(|link| confirmed_link_edge(link, item_by_id))
        .collect()
}

fn confirmed_link_edge(
    link: &SnapshotLink,
    item_by_id: &HashMap<&str, &InventoryItem>,
) -> VisualEdge {
    let from_name = item_by_id
        .get(link.from.as_str())
        .map(|item| item.name.as_str())
        .unwrap_or(link.from.as_str());
    let to_name = item_by_id
        .get(link.to.as_str())
        .map(|item| item.name.as_str())
        .unwrap_or(link.to.as_str());
    let (kind, text) = match link.kind.as_str() {
        "db_fk" => (
            "db-constraint",
            match link.label.as_deref() {
                Some(label) => {
                    format!("FK 제약 {label}: {from_name} 컬럼이 {to_name} 컬럼을 참조합니다")
                }
                None => {
                    format!("{from_name} 컬럼이 {to_name} 컬럼을 참조하는 FK 구조입니다")
                }
            },
        ),
        "code_call" => (
            "code-call",
            format!("{from_name} 코드 항목이 {to_name} 코드 항목을 호출합니다"),
        ),
        "code_handle" => (
            "code-handle",
            format!("{from_name} Route를 {to_name} handler가 처리합니다"),
        ),
        _ => (
            "snapshot-link",
            format!("{from_name} 항목과 {to_name} 항목의 확정 연결입니다"),
        ),
    };

    let mut evidence = vec![Evidence {
        kind: kind.to_string(),
        text,
    }];
    evidence.extend(safe_evidence(&link.evidence));
    let mut seen = HashSet::new();
    evidence.retain(|entry| seen.insert((entry.kind.clone(), entry.text.clone())));
    VisualEdge {
        id: link.id.clone(),
        from: link.from.clone(),
        to: link.to.clone(),
        kind: link.kind.clone(),
        confidence: None,
        evidence,
    }
}

const API_CALL_HOP_LIMIT: usize = 4;
const API_CODE_NODE_LIMIT: usize = 24;
const API_EDGE_LIMIT: usize = 32;
const API_DB_CANDIDATE_LIMIT: usize = 8;

struct ApiFlowTraversal<'a> {
    links: Vec<&'a SnapshotLink>,
    node_order: Vec<String>,
    depths: HashMap<String, usize>,
    incoming: HashMap<String, &'a SnapshotLink>,
    hidden_branches: usize,
    truncation_reasons: Vec<String>,
}

fn reachable_api_flow_links<'a>(
    snapshot: &'a InventorySnapshot,
    route_id: &str,
    max_call_depth: usize,
    node_limit: usize,
    edge_limit: usize,
) -> ApiFlowTraversal<'a> {
    let code_ids = snapshot
        .items
        .iter()
        .filter(|item| item.source == "code")
        .map(|item| item.id.as_str())
        .collect::<HashSet<_>>();
    let mut handles = snapshot
        .links
        .iter()
        .filter(|link| trusted_api_edge(link, "code_handle", "HANDLES"))
        .filter(|link| link.from == route_id)
        .filter(|link| code_ids.contains(link.from.as_str()) && code_ids.contains(link.to.as_str()))
        .collect::<Vec<_>>();
    handles.sort_by(|left, right| left.to.cmp(&right.to).then_with(|| left.id.cmp(&right.id)));

    let mut links = Vec::new();
    let mut node_order = Vec::new();
    let mut depths = HashMap::new();
    let mut incoming = HashMap::new();
    let mut seen_edges = HashSet::new();
    let mut seen_nodes = HashSet::from([route_id.to_string()]);
    let mut hidden_edges = HashSet::new();
    let mut truncation_reasons = Vec::new();
    let mut handler_branches = HashMap::<String, usize>::new();
    let mut frontier = Vec::<(String, usize)>::new();

    for handle in handles {
        if seen_edges.contains(handle.id.as_str()) {
            continue;
        }
        let is_new_node = !seen_nodes.contains(handle.to.as_str());
        if links.len() >= edge_limit {
            record_hidden_api_edge(
                handle,
                &mut hidden_edges,
                &mut truncation_reasons,
                format!("관계 최대 {edge_limit}개에 도달했습니다."),
            );
            continue;
        }
        if is_new_node && seen_nodes.len() >= node_limit {
            record_hidden_api_edge(
                handle,
                &mut hidden_edges,
                &mut truncation_reasons,
                format!("코드 노드 최대 {node_limit}개에 도달했습니다."),
            );
            continue;
        }

        seen_edges.insert(handle.id.clone());
        links.push(handle);
        if is_new_node {
            let branch = handler_branches.len();
            handler_branches.insert(handle.to.clone(), branch);
            seen_nodes.insert(handle.to.clone());
            node_order.push(handle.to.clone());
            depths.insert(handle.to.clone(), 1);
            incoming.insert(handle.to.clone(), handle);
            frontier.push((handle.to.clone(), branch));
        }
    }

    let mut outgoing = HashMap::<&str, Vec<&SnapshotLink>>::new();
    for link in snapshot
        .links
        .iter()
        .filter(|link| trusted_api_edge(link, "code_call", "CALLS"))
        .filter(|link| code_ids.contains(link.from.as_str()) && code_ids.contains(link.to.as_str()))
    {
        outgoing.entry(link.from.as_str()).or_default().push(link);
    }
    for next_links in outgoing.values_mut() {
        next_links
            .sort_by(|left, right| left.to.cmp(&right.to).then_with(|| left.id.cmp(&right.id)));
    }

    for call_depth in 1..=max_call_depth {
        if frontier.is_empty() {
            break;
        }
        let mut by_branch = BTreeMap::<usize, Vec<&SnapshotLink>>::new();
        for (node_id, branch) in &frontier {
            if let Some(next_links) = outgoing.get(node_id.as_str()) {
                by_branch
                    .entry(*branch)
                    .or_default()
                    .extend(next_links.iter().copied());
            }
        }
        for branch_links in by_branch.values_mut() {
            branch_links.sort_by(|left, right| {
                left.from
                    .cmp(&right.from)
                    .then_with(|| left.to.cmp(&right.to))
                    .then_with(|| left.id.cmp(&right.id))
            });
            branch_links.dedup_by(|left, right| left.id == right.id);
        }

        let rounds = by_branch.values().map(Vec::len).max().unwrap_or(0);
        let mut next_frontier = Vec::new();
        for round in 0..rounds {
            for (branch, branch_links) in &by_branch {
                let Some(link) = branch_links.get(round).copied() else {
                    continue;
                };
                if seen_edges.contains(link.id.as_str()) {
                    continue;
                }
                let is_new_node = !seen_nodes.contains(link.to.as_str());
                if links.len() >= edge_limit {
                    record_hidden_api_edge(
                        link,
                        &mut hidden_edges,
                        &mut truncation_reasons,
                        format!("관계 최대 {edge_limit}개에 도달했습니다."),
                    );
                    continue;
                }
                if is_new_node && seen_nodes.len() >= node_limit {
                    record_hidden_api_edge(
                        link,
                        &mut hidden_edges,
                        &mut truncation_reasons,
                        format!("코드 노드 최대 {node_limit}개에 도달했습니다."),
                    );
                    continue;
                }

                seen_edges.insert(link.id.clone());
                links.push(link);
                if is_new_node {
                    seen_nodes.insert(link.to.clone());
                    node_order.push(link.to.clone());
                    depths.insert(link.to.clone(), call_depth + 1);
                    incoming.insert(link.to.clone(), link);
                    next_frontier.push((link.to.clone(), *branch));
                }
            }
        }
        frontier = next_frontier;
    }

    for (node_id, _) in &frontier {
        for link in outgoing.get(node_id.as_str()).into_iter().flatten() {
            if !seen_edges.contains(link.id.as_str()) {
                record_hidden_api_edge(
                    link,
                    &mut hidden_edges,
                    &mut truncation_reasons,
                    format!("CALLS 최대 {max_call_depth} hop에 도달했습니다."),
                );
            }
        }
    }

    ApiFlowTraversal {
        links,
        node_order,
        depths,
        incoming,
        hidden_branches: hidden_edges.len(),
        truncation_reasons,
    }
}

fn trusted_api_edge(link: &SnapshotLink, kind: &str, engine_edge_type: &str) -> bool {
    link.truth_class == "confirmed"
        && link.kind == kind
        && link.engine_edge_type.as_deref() == Some(engine_edge_type)
}

fn record_hidden_api_edge(
    link: &SnapshotLink,
    hidden_edges: &mut HashSet<String>,
    reasons: &mut Vec<String>,
    reason: String,
) {
    if hidden_edges.insert(link.id.clone()) && !reasons.contains(&reason) {
        reasons.push(reason);
    }
}

fn api_reading_answer(
    snapshot: &InventorySnapshot,
    route: &InventoryItem,
    traversal: &ApiFlowTraversal<'_>,
    candidates: &[CandidateLink],
    hidden_candidates: usize,
    candidate_linker_cap_reached: bool,
    item_by_id: &HashMap<&str, &InventoryItem>,
) -> ApiReadingAnswer {
    let mut steps = vec![api_reading_step(route, None, 0, 1, item_by_id)];
    for node_id in &traversal.node_order {
        let Some(item) = item_by_id.get(node_id.as_str()).copied() else {
            continue;
        };
        let incoming = traversal.incoming.get(node_id).copied();
        let depth = traversal.depths.get(node_id).copied().unwrap_or(1);
        steps.push(api_reading_step(
            item,
            incoming,
            depth,
            steps.len() + 1,
            item_by_id,
        ));
    }

    let mut db_candidates = candidates
        .iter()
        .filter_map(|link| {
            let source = item_by_id.get(link.from.as_str()).copied()?;
            let target = item_by_id.get(link.to.as_str()).copied()?;
            Some(ImpactReviewItem {
                id: format!("api-db-candidate:{}", link.id),
                node_id: Some(target.id.clone()),
                kind: "db-candidate".to_string(),
                title: target.name.clone(),
                detail: safe_text(&format!(
                    "{} 코드에서 {} 테이블 사용 가능성을 확인해야 합니다.",
                    source.name, target.name
                )),
                truth_class: "candidate".to_string(),
                confidence: Some(link.confidence.clone()),
                rank: 0,
                evidence: safe_evidence(&link.evidence),
                location: source.location.clone(),
            })
        })
        .collect::<Vec<_>>();
    assign_review_ranks(&mut db_candidates);

    let has_handler = traversal
        .links
        .iter()
        .any(|link| link.kind == "code_handle");
    let mut unknowns = Vec::new();
    let mut reachable_sources = traversal
        .node_order
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    reachable_sources.insert(route.id.as_str());
    let mut rejected_edges = snapshot
        .links
        .iter()
        .filter_map(|link| {
            let handles = (link.kind == "code_handle"
                || link.engine_edge_type.as_deref() == Some("HANDLES"))
                && link.from == route.id
                && !trusted_api_edge(link, "code_handle", "HANDLES");
            let calls = (link.kind == "code_call"
                || link.engine_edge_type.as_deref() == Some("CALLS"))
                && reachable_sources.contains(link.from.as_str())
                && !trusted_api_edge(link, "code_call", "CALLS");
            (handles || calls).then_some((link, handles))
        })
        .collect::<Vec<_>>();
    rejected_edges.sort_by(|(left, _), (right, _)| left.id.cmp(&right.id));
    let rejected_handles = rejected_edges
        .iter()
        .filter(|(_, handles)| *handles)
        .count();
    for (link, handles) in rejected_edges {
        let relationship = if handles { "HANDLES" } else { "CALLS" };
        let title = if handles && !has_handler {
            "확정 HANDLES 없음"
        } else if handles {
            "비확정 HANDLES 제외"
        } else {
            "비확정 CALLS 제외"
        };
        let mut evidence = safe_evidence(&link.evidence);
        evidence.push(Evidence {
            kind: "excluded-engine-edge".to_string(),
            text: safe_text(&format!(
                "{} → {} 관계의 kind={}, engineEdgeType={}, truthClass={}를 확정 경로에서 제외했습니다.",
                link.from,
                link.to,
                link.kind,
                link.engine_edge_type.as_deref().unwrap_or("없음"),
                link.truth_class
            )),
        });
        unknowns.push(ImpactReviewItem {
            id: format!("api-unknown:edge:{}", link.id),
            node_id: Some(link.to.clone()),
            kind: if handles { "handler-gap" } else { "call-gap" }.to_string(),
            title: title.to_string(),
            detail: safe_text(&format!(
                "{title}: {relationship} 관계가 확정 조건을 충족하지 않아 읽기 순서와 지도에서 제외했습니다."
            )),
            truth_class: link.truth_class.clone(),
            confidence: None,
            rank: 0,
            evidence,
            location: item_by_id
                .get(link.to.as_str())
                .or_else(|| item_by_id.get(link.from.as_str()))
                .and_then(|item| item.location.clone()),
        });
    }

    if !has_handler && rejected_handles == 0 {
        unknowns.push(api_answer_item(
            "api-unknown:handler",
            Some(route.id.clone()),
            "handler-gap",
            "확정 HANDLES 없음",
            "확정 HANDLES 없음: 코드 엔진에서 이 Route의 handler 관계를 찾지 못해 이후 구간은 알 수 없습니다.",
            "unknown",
            route.location.clone(),
        ));
    } else if db_candidates.is_empty() {
        unknowns.push(api_answer_item(
            "api-unknown:db",
            Some(route.id.clone()),
            "db-gap",
            "DB 사용 구간을 확인할 수 없음",
            "확정 CALLS로 도달한 코드에서 이름 기반 DB 후보를 찾지 못했습니다.",
            "unknown",
            None,
        ));
    }

    for (index, reason) in snapshot.stale_reasons.iter().enumerate() {
        unknowns.push(api_answer_item(
            &format!("api-unknown:stale:{index}"),
            Some(route.id.clone()),
            "stale",
            "Snapshot 재확인 필요",
            reason,
            "unknown",
            route.location.clone(),
        ));
    }
    if snapshot.metadata.migration.reindex_required {
        let detail = if snapshot.metadata.migration.notes.is_empty() {
            "현재 snapshot은 최신 계약으로 완전히 검증되지 않아 재인덱싱이 필요합니다.".to_string()
        } else {
            format!(
                "현재 snapshot은 재인덱싱이 필요합니다. {}",
                snapshot.metadata.migration.notes.join(" ")
            )
        };
        unknowns.push(api_answer_item(
            "api-unknown:reindex",
            Some(route.id.clone()),
            "reindex",
            "재인덱싱 필요",
            &detail,
            "unknown",
            route.location.clone(),
        ));
    }

    let mut relevant_ids = reachable_sources;
    relevant_ids.extend(candidates.iter().map(|candidate| candidate.to.as_str()));
    for gap in snapshot.metadata.gaps.iter().filter(|gap| {
        gap.related_ids.is_empty()
            || gap
                .related_ids
                .iter()
                .any(|id| relevant_ids.contains(id.as_str()))
    }) {
        unknowns.push(api_answer_item(
            &format!("api-unknown:gap:{}", gap.id),
            Some(route.id.clone()),
            &gap.kind,
            "인덱싱 메타데이터 누락",
            &gap.message,
            "unknown",
            route.location.clone(),
        ));
    }
    if let Some(db) = snapshot.metadata.db.as_ref() {
        if db.truncated == Some(true) {
            unknowns.push(api_answer_item(
                "api-unknown:db-truncated",
                Some(route.id.clone()),
                "db-inventory-truncated",
                "DB 인벤토리 일부만 확인됨",
                "DB 인벤토리가 잘려 있어 추가 테이블 후보가 누락됐을 수 있습니다.",
                "unknown",
                None,
            ));
        }
        if db.limit_clamped == Some(true) {
            unknowns.push(api_answer_item(
                "api-unknown:db-limit-clamped",
                Some(route.id.clone()),
                "db-limit-clamped",
                "DB 인벤토리 한도 조정됨",
                &format!(
                    "요청한 DB 한도({})가 엔진 한도({})로 조정되어 전체 범위를 확인하지 못했을 수 있습니다.",
                    db.limit_requested
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "알 수 없음".to_string()),
                    db.limit_applied
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "알 수 없음".to_string())
                ),
                "unknown",
                None,
            ));
        }
    }
    if candidate_linker_cap_reached {
        unknowns.push(api_answer_item(
            "api-unknown:candidate-linker-cap",
            Some(route.id.clone()),
            "candidate-cap",
            "DB 후보 선행 한도 도달",
            &format!(
                "코드 항목당 DB 후보가 최대 {MAX_CANDIDATES_PER_CODE_ITEM}개로 제한되어 추가 테이블 후보가 누락됐을 수 있습니다."
            ),
            "unknown",
            None,
        ));
    }

    let mut truncation_reasons = traversal.truncation_reasons.clone();
    if hidden_candidates > 0 {
        truncation_reasons.push(format!(
            "DB 후보는 상위 {API_DB_CANDIDATE_LIMIT}개만 표시합니다."
        ));
    }
    if candidate_linker_cap_reached {
        truncation_reasons.push(format!(
            "DB 후보 연결은 코드 항목당 최대 {MAX_CANDIDATES_PER_CODE_ITEM}개에서 선행 제한되었습니다."
        ));
    }
    let hidden_branches = traversal.hidden_branches + hidden_candidates;
    let hidden_branches_is_lower_bound = hidden_branches > 0 || candidate_linker_cap_reached;
    if hidden_branches > 0 {
        unknowns.push(api_answer_item(
            "api-unknown:truncated",
            Some(route.id.clone()),
            "truncated",
            "읽기 경로 일부가 접힘",
            &format!(
                "최소 {hidden_branches}개의 경계 관계/후보가 표시 한도 밖에 있습니다. 경계 아래는 탐색하지 않아 실제 숨은 항목은 더 많을 수 있습니다. {}",
                truncation_reasons.join(" ")
            ),
            "unknown",
            None,
        ));
    }
    assign_review_ranks(&mut unknowns);

    let mut recommended_checks = Vec::new();
    let snapshot_coverage_risk = !snapshot.stale_reasons.is_empty()
        || snapshot.metadata.migration.reindex_required
        || snapshot.metadata.gaps.iter().any(|gap| {
            gap.related_ids.is_empty()
                || gap
                    .related_ids
                    .iter()
                    .any(|id| relevant_ids.contains(id.as_str()))
        })
        || snapshot
            .metadata
            .db
            .as_ref()
            .is_some_and(|db| db.truncated == Some(true) || db.limit_clamped == Some(true));
    if snapshot_coverage_risk {
        recommended_checks.push(api_answer_item(
            "api-check:reindex",
            Some(route.id.clone()),
            "reindex",
            "Snapshot 범위 확인 후 다시 인덱싱",
            "stale·migration·metadata gap 또는 DB 인벤토리 한도를 해소한 뒤 API 경로를 다시 확인하세요.",
            "action",
            route.location.clone(),
        ));
    }
    if let Some(step) = steps.iter().find(|step| step.item.location.is_some()) {
        recommended_checks.push(api_answer_item(
            "api-check:first-source",
            step.item.node_id.clone(),
            "source",
            "첫 파일부터 열기",
            &format!("{}부터 읽고 다음 확정 CALLS를 따라가세요.", step.item.title),
            "action",
            step.item.location.clone(),
        ));
    }
    if !has_handler {
        recommended_checks.push(api_answer_item(
            "api-check:handles",
            Some(route.id.clone()),
            "route-registration",
            "Route 등록과 handler 연결 확인",
            "라우트 프레임워크 등록부에서 실제 handler를 확인한 뒤 다시 인덱싱하세요.",
            "action",
            route.location.clone(),
        ));
    }
    for (index, candidate) in db_candidates.iter().take(3).enumerate() {
        recommended_checks.push(api_answer_item(
            &format!("api-check:db:{index}"),
            candidate.node_id.clone(),
            "db-candidate",
            &format!("{} 사용 여부 검증", candidate.title),
            "Repository/query의 SQL·ORM 매핑에서 테이블 사용을 직접 확인하세요.",
            "action",
            candidate.location.clone(),
        ));
    }
    if hidden_branches > 0 || candidate_linker_cap_reached {
        recommended_checks.push(api_answer_item(
            "api-check:truncated",
            Some(route.id.clone()),
            "scope",
            "접힌 분기 별도 확인",
            "표시 한도에 걸린 분기는 검색으로 대상을 좁혀 별도로 확인하세요.",
            "action",
            route.location.clone(),
        ));
    }
    assign_review_ranks(&mut recommended_checks);

    ApiReadingAnswer {
        subject: route.name.clone(),
        steps,
        db_candidates,
        unknowns,
        recommended_checks,
        hidden_branches,
        hidden_branches_is_lower_bound,
        truncated: hidden_branches > 0 || candidate_linker_cap_reached,
        truncation_reason: (!truncation_reasons.is_empty()).then(|| truncation_reasons.join(" ")),
    }
}

fn api_reading_step(
    item: &InventoryItem,
    incoming: Option<&SnapshotLink>,
    depth: usize,
    rank: usize,
    item_by_id: &HashMap<&str, &InventoryItem>,
) -> ApiReadingStep {
    let incoming_evidence = incoming
        .map(|link| confirmed_link_edge(link, item_by_id).evidence)
        .unwrap_or_default();
    let evidence = if incoming_evidence.is_empty() {
        vec![Evidence {
            kind: "engine-node".to_string(),
            text: "코드 엔진 inventory에서 Route 항목을 읽었습니다.".to_string(),
        }]
    } else {
        incoming_evidence.clone()
    };
    ApiReadingStep {
        item: ImpactReviewItem {
            id: format!("api-step:{}", item.id),
            node_id: Some(item.id.clone()),
            kind: item.kind.clone(),
            title: item.name.clone(),
            detail: api_item_detail(item),
            truth_class: if incoming.is_some() {
                "confirmed".to_string()
            } else {
                "structural".to_string()
            },
            confidence: None,
            rank,
            evidence,
            location: item.location.clone(),
        },
        depth,
        lane: api_reading_lane(item, incoming).to_string(),
        incoming_evidence,
    }
}

fn api_reading_lane(item: &InventoryItem, incoming: Option<&SnapshotLink>) -> &'static str {
    if incoming.is_none() || item.layer == "api" {
        return "route";
    }
    if incoming.is_some_and(|link| link.kind == "code_handle") {
        return "handler";
    }
    let identity = format!(
        "{} {} {}",
        item.kind,
        item.engine_label.as_deref().unwrap_or_default(),
        item.name
    )
    .to_ascii_lowercase();
    if identity.contains("handler") || identity.contains("controller") {
        "handler"
    } else if ["repository", "query", "mapper", "dao"]
        .iter()
        .any(|token| identity.contains(token))
    {
        "repository-query"
    } else {
        "service-function"
    }
}

fn api_item_detail(item: &InventoryItem) -> String {
    item.location
        .as_ref()
        .map(|location| match location.line {
            Some(line) => format!("{}:{line}", location.path),
            None => location.path.clone(),
        })
        .unwrap_or_else(|| "소스 위치 정보 없음".to_string())
}

fn api_answer_item(
    id: &str,
    node_id: Option<String>,
    kind: &str,
    title: &str,
    detail: &str,
    truth_class: &str,
    location: Option<SourceLocation>,
) -> ImpactReviewItem {
    ImpactReviewItem {
        id: id.to_string(),
        node_id,
        kind: kind.to_string(),
        title: title.to_string(),
        detail: safe_text(detail),
        truth_class: truth_class.to_string(),
        confidence: None,
        rank: 0,
        evidence: Vec::new(),
        location,
    }
}

fn db_fk_links_for_table<'a>(
    snapshot: &'a InventorySnapshot,
    item_by_id: &HashMap<&str, &InventoryItem>,
    table_id: &str,
) -> Vec<&'a SnapshotLink> {
    snapshot
        .links
        .iter()
        .filter(|link| link.kind == "db_fk")
        .filter(|link| {
            link_endpoint_parent(item_by_id, link.from.as_str()) == Some(table_id)
                || link_endpoint_parent(item_by_id, link.to.as_str()) == Some(table_id)
        })
        .collect()
}

fn db_fk_links_for_column<'a>(
    snapshot: &'a InventorySnapshot,
    column_id: &str,
) -> Vec<&'a SnapshotLink> {
    snapshot
        .links
        .iter()
        .filter(|link| link.kind == "db_fk" && (link.from == column_id || link.to == column_id))
        .collect()
}

fn link_endpoint_parent<'a>(
    item_by_id: &'a HashMap<&str, &InventoryItem>,
    item_id: &str,
) -> Option<&'a str> {
    item_by_id
        .get(item_id)
        .and_then(|item| item.parent_id.as_deref())
}

fn linked_items<'a>(
    links: &[&SnapshotLink],
    item_by_id: &HashMap<&str, &'a InventoryItem>,
) -> Vec<&'a InventoryItem> {
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for link in links {
        for id in [link.from.as_str(), link.to.as_str()] {
            if let Some(item) = item_by_id.get(id).copied() {
                if let Some(parent_id) = item.parent_id.as_deref() {
                    if let Some(parent) = item_by_id.get(parent_id).copied() {
                        if seen.insert(parent.id.as_str()) {
                            items.push(parent);
                        }
                    }
                }
                if seen.insert(item.id.as_str()) {
                    items.push(item);
                }
            }
        }
    }
    items.sort_by_key(|item| node_sort_key(Some(*item)));
    items
}

fn include_snapshot_link_neighbors(
    snapshot: &InventorySnapshot,
    item_by_id: &HashMap<&str, &InventoryItem>,
    included: &mut HashSet<String>,
) {
    for link in &snapshot.links {
        if !(included.contains(&link.from) || included.contains(&link.to)) {
            continue;
        }
        for id in [&link.from, &link.to] {
            included.insert(id.clone());
            if let Some(parent_id) = item_by_id
                .get(id.as_str())
                .and_then(|item| item.parent_id.clone())
            {
                included.insert(parent_id);
            }
        }
    }
}

fn constraint_node(column: &InventoryItem, suffix: &str, title: &str) -> VisualNode {
    VisualNode {
        id: format!("db:constraint:{}:{suffix}", column.id),
        kind: "constraint".to_string(),
        title: title.to_string(),
        subtitle: Some("확정 DB 구조".to_string()),
        layer: "data".to_string(),
        source: "db".to_string(),
    }
}

fn constraint_edge(column: &InventoryItem, suffix: &str) -> VisualEdge {
    let title = if suffix == "pk" {
        "Primary Key"
    } else {
        "Foreign Key"
    };
    VisualEdge {
        id: format!("db-constraint:{}->{suffix}", column.id),
        from: column.id.clone(),
        to: format!("db:constraint:{}:{suffix}", column.id),
        kind: "db_constraint".to_string(),
        confidence: None,
        evidence: vec![super::model::Evidence {
            kind: "db-metadata".to_string(),
            text: format!("{} 컬럼은 {title}로 표시된 DB 구조입니다", column.name),
        }],
    }
}

fn column_reference_candidates(
    snapshot: &InventorySnapshot,
    column: &InventoryItem,
) -> Vec<VisualEdge> {
    let full_name = column.name.to_ascii_lowercase();
    let compact_name = compact_token(&full_name);

    let mut edges = snapshot
        .links
        .iter()
        .filter(|link| {
            link.kind == "code_db_column_text_reference"
                && link.truth_class == "candidate"
                && link.to == column.id
        })
        .map(|link| VisualEdge {
            id: format!("candidate-column:{}->{}", link.from, link.to),
            from: link.from.clone(),
            to: link.to.clone(),
            kind: "candidate_column_ref".to_string(),
            confidence: Some(
                if link
                    .evidence
                    .iter()
                    .any(|entry| entry.kind == "code-search-schema-ambiguous")
                {
                    "medium"
                } else {
                    "high"
                }
                .to_string(),
            ),
            evidence: link.evidence.clone(),
        })
        .collect::<Vec<_>>();
    edges.extend(
        snapshot
            .items
            .iter()
            .filter(|item| item.source == "code")
            .filter_map(|item| {
                let haystack = format!("{} {}", item.name, item.path.clone().unwrap_or_default())
                    .to_ascii_lowercase();
                let compact_haystack = compact_token(&haystack);
                let confidence = if (full_name.len() >= 3 && haystack.contains(&full_name))
                    || (compact_name.len() >= 4 && compact_haystack.contains(&compact_name))
                {
                    Some("medium")
                } else {
                    None
                }?;

                Some(VisualEdge {
                    id: format!("candidate-column:{}->{}", item.id, column.id),
                    from: item.id.clone(),
                    to: column.id.clone(),
                    kind: "candidate_column_ref".to_string(),
                    confidence: Some(confidence.to_string()),
                    evidence: vec![super::model::Evidence {
                        kind: "column-name-match".to_string(),
                        text: format!(
                            "{} 코드 항목이 {} 컬럼명과 이름 기반으로 일치합니다",
                            item.name, column.name
                        ),
                    }],
                })
            }),
    );
    let mut merged = BTreeMap::<(String, String), VisualEdge>::new();
    for edge in edges {
        let key = (edge.from.clone(), edge.to.clone());
        match merged.entry(key) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(edge);
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                let existing = entry.get_mut();
                merge_evidence(&mut existing.evidence, edge.evidence);
                let ambiguous = existing
                    .evidence
                    .iter()
                    .any(|entry| entry.kind == "code-search-schema-ambiguous");
                let left = existing.confidence.as_deref().unwrap_or("low");
                let right = edge.confidence.as_deref().unwrap_or("low");
                existing.confidence = Some(
                    if ambiguous {
                        "medium"
                    } else if confidence_rank(left) <= confidence_rank(right) {
                        left
                    } else {
                        right
                    }
                    .to_string(),
                );
            }
        }
    }
    let mut edges = merged.into_values().collect::<Vec<_>>();
    edges.sort_by(|a, b| {
        confidence_rank(a.confidence.as_deref().unwrap_or(""))
            .cmp(&confidence_rank(b.confidence.as_deref().unwrap_or("")))
            .then_with(|| {
                let a_explicit = a
                    .evidence
                    .iter()
                    .any(|entry| entry.kind == "code-search-exact-token");
                let b_explicit = b
                    .evidence
                    .iter()
                    .any(|entry| entry.kind == "code-search-exact-token");
                b_explicit.cmp(&a_explicit)
            })
            .then_with(|| a.from.cmp(&b.from))
    });
    edges
}

const DIRECT_REVIEW_LIMIT: usize = 12;
const CANDIDATE_REVIEW_LIMIT: usize = 10;
const UNKNOWN_REVIEW_LIMIT: usize = 8;
const CHECK_REVIEW_LIMIT: usize = 10;
type ImpactLinkIndex<'a> = HashMap<&'a str, Vec<&'a SnapshotLink>>;

fn impact_review_board(
    snapshot: &InventorySnapshot,
    table: &InventoryItem,
    column: Option<&InventoryItem>,
    candidate_edges: &[VisualEdge],
) -> ImpactReviewBoard {
    let item_by_id = snapshot
        .items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let (links_by_from, links_by_to) = impact_link_indexes(snapshot);
    let mut direct = impact_direct_items(
        snapshot,
        table,
        column,
        &item_by_id,
        &links_by_from,
        &links_by_to,
    );
    direct.sort_by(|left, right| {
        direct_review_rank(&left.kind)
            .cmp(&direct_review_rank(&right.kind))
            .then_with(|| left.title.cmp(&right.title))
            .then_with(|| left.id.cmp(&right.id))
    });
    assign_review_ranks(&mut direct);

    let mut candidates = impact_candidate_items(candidate_edges, &item_by_id);
    candidates.sort_by(|left, right| {
        confidence_rank(left.confidence.as_deref().unwrap_or(""))
            .cmp(&confidence_rank(right.confidence.as_deref().unwrap_or("")))
            .then_with(|| {
                candidate_review_rank(&left.kind).cmp(&candidate_review_rank(&right.kind))
            })
            .then_with(|| left.title.cmp(&right.title))
            .then_with(|| left.id.cmp(&right.id))
    });
    assign_review_ranks(&mut candidates);

    let mut unknowns = impact_unknown_items(
        snapshot,
        table,
        column,
        &direct,
        &candidates,
        &item_by_id,
        &links_by_from,
    );
    assign_review_ranks(&mut unknowns);
    let mut checks = impact_check_items(snapshot, table, column, &direct, &candidates);
    checks.sort_by(|left, right| {
        check_review_rank(&left.kind)
            .cmp(&check_review_rank(&right.kind))
            .then_with(|| left.title.cmp(&right.title))
    });
    assign_review_ranks(&mut checks);

    let subject = match column {
        Some(column) if table.id != column.id => format!("{}.{}", table.name, column.name),
        Some(column) => column.name.clone(),
        None => table.name.clone(),
    };
    let scope = if column.is_some() { "column" } else { "table" }.to_string();
    let lanes = vec![
        review_lane("direct", direct, DIRECT_REVIEW_LIMIT),
        review_lane("candidates", candidates, CANDIDATE_REVIEW_LIMIT),
        review_lane("unknowns", unknowns, UNKNOWN_REVIEW_LIMIT),
        review_lane("checks", checks, CHECK_REVIEW_LIMIT),
    ];
    let markdown_summary = impact_markdown_summary(&subject, &lanes);

    ImpactReviewBoard {
        subject,
        scope,
        lanes,
        markdown_summary,
    }
}

fn impact_direct_items(
    snapshot: &InventorySnapshot,
    table: &InventoryItem,
    column: Option<&InventoryItem>,
    item_by_id: &HashMap<&str, &InventoryItem>,
    links_by_from: &ImpactLinkIndex<'_>,
    links_by_to: &ImpactLinkIndex<'_>,
) -> Vec<ImpactReviewItem> {
    let relevant_objects = snapshot
        .items
        .iter()
        .filter(|item| {
            item.parent_id.as_deref() == Some(table.id.as_str())
                && matches!(item.kind.as_str(), "constraint" | "index")
        })
        .filter(|item| {
            column.is_none()
                || links_by_from
                    .get(item.id.as_str())
                    .into_iter()
                    .flatten()
                    .any(|link| {
                        matches!(link.kind.as_str(), "db_constraint" | "db_index")
                            && column.is_some_and(|column| link.to == column.id)
                    })
        });

    let mut items = relevant_objects
        .map(|object| {
            direct_object_review_item(object, column, item_by_id, links_by_from, links_by_to)
        })
        .collect::<Vec<_>>();

    for link in snapshot.links.iter().filter(|link| {
        link.kind == "db_fk" && impact_link_touches_focus(link, table, column, item_by_id)
    }) {
        let from = item_by_id
            .get(link.from.as_str())
            .map(|item| item.name.as_str())
            .unwrap_or(link.from.as_str());
        let to = item_by_id
            .get(link.to.as_str())
            .map(|item| item.name.as_str())
            .unwrap_or(link.to.as_str());
        items.push(ImpactReviewItem {
            id: format!("direct:{}", link.id),
            node_id: related_fk_node_id(link, table, item_by_id),
            kind: "foreign-key-reference".to_string(),
            title: link
                .label
                .clone()
                .unwrap_or_else(|| "Foreign key reference".to_string()),
            detail: safe_text(&format!("{from} → {to}")),
            truth_class: review_truth_class(&link.truth_class),
            confidence: None,
            rank: 0,
            evidence: safe_evidence(&link.evidence),
            location: None,
        });
    }

    let focus_columns = match column {
        Some(column) => vec![column],
        None => snapshot
            .items
            .iter()
            .filter(|item| {
                item.kind == "column" && item.parent_id.as_deref() == Some(table.id.as_str())
            })
            .collect(),
    };
    for focus_column in focus_columns {
        if focus_column.is_primary_key
            && !items.iter().any(|item| {
                item.kind == "primary-key"
                    && review_item_mentions_node(item, focus_column.id.as_str(), links_by_from)
            })
        {
            items.push(metadata_constraint_review_item(
                focus_column,
                "primary-key",
                "Primary key",
            ));
        }
        if focus_column.is_foreign_key
            && !items.iter().any(|item| {
                matches!(item.kind.as_str(), "foreign-key" | "foreign-key-reference")
                    && review_item_mentions_node(item, focus_column.id.as_str(), links_by_from)
            })
        {
            items.push(metadata_constraint_review_item(
                focus_column,
                "foreign-key",
                "Foreign key",
            ));
        }
    }

    let mut seen = HashSet::new();
    items.retain(|item| seen.insert(item.id.clone()));
    items
}

fn direct_object_review_item(
    object: &InventoryItem,
    column: Option<&InventoryItem>,
    item_by_id: &HashMap<&str, &InventoryItem>,
    links_by_from: &ImpactLinkIndex<'_>,
    links_by_to: &ImpactLinkIndex<'_>,
) -> ImpactReviewItem {
    let links = links_by_from
        .get(object.id.as_str())
        .into_iter()
        .flatten()
        .filter(|link| {
            matches!(link.kind.as_str(), "db_constraint" | "db_index")
                && column.is_none_or(|column| link.to == column.id)
        })
        .chain(
            links_by_to
                .get(object.id.as_str())
                .into_iter()
                .flatten()
                .filter(|link| column.is_none() && link.kind == "contains"),
        )
        .collect::<Vec<_>>();
    let confirmed = links.iter().any(|link| link.truth_class == "confirmed");
    let evidence = links
        .iter()
        .flat_map(|link| link.evidence.iter().cloned())
        .collect::<Vec<_>>();
    let kind = direct_object_kind(object, &evidence);
    let mut columns = links
        .iter()
        .filter_map(|link| item_by_id.get(link.to.as_str()))
        .filter(|item| item.kind == "column")
        .map(|item| item.name.clone())
        .collect::<Vec<_>>();
    columns.sort();
    columns.dedup();
    let detail = if columns.is_empty() {
        direct_kind_label(&kind).to_string()
    } else {
        format!("{} · {}", direct_kind_label(&kind), columns.join(", "))
    };
    ImpactReviewItem {
        id: format!("direct:{}", object.id),
        node_id: Some(object.id.clone()),
        kind,
        title: object.name.clone(),
        detail: safe_text(&detail),
        truth_class: if confirmed { "confirmed" } else { "structural" }.to_string(),
        confidence: None,
        rank: 0,
        evidence: safe_evidence(&evidence),
        location: None,
    }
}

fn metadata_constraint_review_item(
    column: &InventoryItem,
    kind: &str,
    title: &str,
) -> ImpactReviewItem {
    ImpactReviewItem {
        id: format!("direct:metadata:{kind}:{}", column.id),
        node_id: Some(column.id.clone()),
        kind: kind.to_string(),
        title: format!("{title} · {}", column.name),
        detail: "컬럼 메타데이터에서 직접 읽음".to_string(),
        truth_class: "confirmed".to_string(),
        confidence: None,
        rank: 0,
        evidence: vec![Evidence {
            kind: "db-metadata".to_string(),
            text: format!("{} 컬럼의 {} 표시", column.name, title.to_ascii_uppercase()),
        }],
        location: None,
    }
}

fn impact_candidate_items(
    edges: &[VisualEdge],
    item_by_id: &HashMap<&str, &InventoryItem>,
) -> Vec<ImpactReviewItem> {
    let mut seen = HashSet::new();
    edges
        .iter()
        .filter_map(|edge| {
            let code = item_by_id.get(edge.from.as_str()).copied()?;
            if code.source != "code" || !seen.insert(code.id.as_str()) {
                return None;
            }
            Some(ImpactReviewItem {
                id: format!("candidate:{}", code.id),
                node_id: Some(code.id.clone()),
                kind: code.kind.clone(),
                title: code.name.clone(),
                detail: candidate_detail(code),
                truth_class: "candidate".to_string(),
                confidence: edge.confidence.clone(),
                rank: 0,
                evidence: safe_evidence(&edge.evidence),
                location: code.location.clone(),
            })
        })
        .collect()
}

fn impact_unknown_items(
    snapshot: &InventorySnapshot,
    table: &InventoryItem,
    column: Option<&InventoryItem>,
    direct: &[ImpactReviewItem],
    candidates: &[ImpactReviewItem],
    item_by_id: &HashMap<&str, &InventoryItem>,
    links_by_from: &ImpactLinkIndex<'_>,
) -> Vec<ImpactReviewItem> {
    let mut items = Vec::new();
    for (index, reason) in snapshot.stale_reasons.iter().enumerate() {
        items.push(unknown_review_item(
            format!("unknown:stale:{index}"),
            "stale",
            "Snapshot 재확인 필요",
            reason,
        ));
    }
    if snapshot.metadata.migration.reindex_required {
        items.push(unknown_review_item(
            "unknown:reindex".to_string(),
            "reindex",
            "재인덱싱 필요",
            "이 snapshot은 현재 계약으로 완전히 검증되지 않았습니다.",
        ));
    }
    for (index, note) in snapshot.metadata.migration.notes.iter().enumerate() {
        items.push(unknown_review_item(
            format!("unknown:migration-note:{index}"),
            "snapshot-migration",
            "Snapshot 변환 기록",
            note,
        ));
    }
    let candidate_ids = candidates
        .iter()
        .filter_map(|candidate| candidate.node_id.as_deref())
        .collect::<HashSet<_>>();
    for gap in snapshot.metadata.gaps.iter().filter(|gap| {
        gap.kind == "db-capability"
            || gap.related_ids.is_empty()
            || gap.related_ids.iter().any(|id| {
                id == &table.id
                    || column.is_some_and(|column| id == &column.id)
                    || candidate_ids.contains(id.as_str())
                    || item_by_id
                        .get(id.as_str())
                        .is_some_and(|item| item.parent_id.as_deref() == Some(table.id.as_str()))
            })
    }) {
        items.push(unknown_review_item(
            format!("unknown:{}", gap.id),
            &gap.kind,
            if gap.kind == "db-capability" {
                "DB 지원 범위 제한"
            } else if gap.kind.starts_with("code-search") {
                "코드 텍스트 근거 확인 필요"
            } else {
                "DB 메타데이터 누락"
            },
            &gap.message,
        ));
    }
    if snapshot.metadata.db.is_none() {
        items.push(unknown_review_item(
            "unknown:db-source".to_string(),
            "missing-source",
            "DB 출처 정보 없음",
            "DB snapshot 출처와 capability를 확인할 수 없습니다.",
        ));
    }
    if snapshot.metadata.code.is_none() {
        items.push(unknown_review_item(
            "unknown:code-source".to_string(),
            "missing-source",
            "코드 출처 정보 없음",
            "코드 후보의 snapshot 출처를 확인할 수 없습니다.",
        ));
    }
    if column.is_some_and(|column| column.parent_id.as_deref() != Some(table.id.as_str())) {
        items.push(unknown_review_item(
            "unknown:missing-parent-table".to_string(),
            "missing-db-parent",
            "상위 테이블 미확인",
            "컬럼의 상위 테이블 관계가 snapshot에 없어 테이블 단위 영향은 알 수 없습니다.",
        ));
    }
    if direct.is_empty() {
        items.push(unknown_review_item(
            "unknown:no-direct-facts".to_string(),
            "missing-db-facts",
            "직접 영향 미확인",
            "연결된 제약·인덱스를 읽지 못했습니다. 영향 없음으로 확정하지 않습니다.",
        ));
    }
    if candidates.is_empty() {
        items.push(unknown_review_item(
            "unknown:no-code-candidates".to_string(),
            "missing-code-candidates",
            "코드 영향 미확인",
            "이름·경로 근거 후보가 없습니다. 코드 영향 없음으로 확정하지 않습니다.",
        ));
    } else {
        let missing_locations = candidates
            .iter()
            .filter(|candidate| candidate.location.is_none())
            .count();
        if missing_locations > 0 {
            items.push(unknown_review_item(
                "unknown:missing-code-locations".to_string(),
                "missing-source-location",
                "소스 위치 누락",
                &format!("후보 {missing_locations}개의 파일·라인 위치를 확인할 수 없습니다."),
            ));
        }

        let reachable = api_reachable_code_ids(snapshot, links_by_from, 4, 20_000);
        let disconnected = candidates
            .iter()
            .filter_map(|candidate| candidate.node_id.as_deref())
            .filter(|node_id| !reachable.contains(*node_id))
            .count();
        if disconnected > 0 {
            items.push(unknown_review_item(
                "unknown:disconnected-api-path".to_string(),
                "disconnected-call-path",
                "API 경로 미연결",
                &format!("후보 {disconnected}개는 4 hop 내 확정 HANDLES/CALLS 경로가 없습니다."),
            ));
        }
    }
    let mut seen = HashSet::new();
    items.retain(|item| seen.insert(item.id.clone()));
    items
}

fn impact_check_items(
    snapshot: &InventorySnapshot,
    table: &InventoryItem,
    column: Option<&InventoryItem>,
    direct: &[ImpactReviewItem],
    candidates: &[ImpactReviewItem],
) -> Vec<ImpactReviewItem> {
    let mut checks = Vec::new();
    if !snapshot.stale_reasons.is_empty() || snapshot.metadata.migration.reindex_required {
        checks.push(action_review_item(
            "check:reindex",
            None,
            "reindex",
            "먼저 snapshot 다시 읽기",
            "stale 또는 migration 상태를 해소한 뒤 영향 범위를 다시 확인합니다.",
            None,
            Vec::new(),
        ));
    }
    for item in direct.iter().filter(|item| {
        matches!(
            item.kind.as_str(),
            "primary-key"
                | "foreign-key"
                | "foreign-key-reference"
                | "unique"
                | "check"
                | "index"
                | "unique-index"
                | "primary-index"
        )
    }) {
        checks.push(action_review_item(
            &format!("check:{}", item.id),
            item.node_id.clone(),
            "constraint",
            &format!("DB 정의 확인 · {}", item.title),
            &item.detail,
            None,
            item.evidence.clone(),
        ));
    }
    for candidate in candidates {
        let kind = candidate_check_kind(candidate);
        checks.push(action_review_item(
            &format!("check:{}", candidate.id),
            candidate.node_id.clone(),
            kind,
            &format!("{} · {}", check_action_label(kind), candidate.title),
            &candidate.detail,
            candidate.location.clone(),
            candidate.evidence.clone(),
        ));
    }
    if direct.is_empty() {
        checks.push(action_review_item(
            "check:db-coverage",
            Some(table.id.clone()),
            "coverage",
            "DB metadata coverage 확인",
            "adapter가 PK/FK/unique/check/index를 지원하는지 확인한 뒤 변경합니다.",
            None,
            Vec::new(),
        ));
    }
    if !candidates
        .iter()
        .any(|item| candidate_check_kind(item) == "migration")
    {
        checks.push(action_review_item(
            "check:migration-location",
            column
                .map(|column| column.id.clone())
                .or_else(|| Some(table.id.clone())),
            "migration-missing",
            "마이그레이션 위치 확인",
            "연결된 migration/DDL 파일 후보가 없어 저장소의 실제 스키마 변경 경로를 확인합니다.",
            None,
            Vec::new(),
        ));
    }
    if !candidates
        .iter()
        .any(|item| candidate_check_kind(item) == "test")
    {
        checks.push(action_review_item(
            "check:test-location",
            column
                .map(|column| column.id.clone())
                .or_else(|| Some(table.id.clone())),
            "test-missing",
            "회귀 테스트 위치 확인",
            "연결된 test/spec 파일 후보가 없어 제약·조회 동작을 검증할 테스트 위치를 확인합니다.",
            None,
            Vec::new(),
        ));
    }
    let mut seen = HashSet::new();
    checks.retain(|item| seen.insert(item.id.clone()));
    checks
}

fn review_lane(id: &str, mut items: Vec<ImpactReviewItem>, limit: usize) -> ImpactReviewLane {
    let (order, title, description, tone, empty_message) = match id {
        "direct" => (
            1,
            "직접 영향",
            "DB에서 직접 읽은 제약·인덱스·참조 구조",
            "confirmed",
            "직접 영향 메타데이터가 없습니다. 영향 없음으로 확정하지 않습니다.",
        ),
        "candidates" => (
            2,
            "코드 영향 후보",
            "이름·경로 근거로 정렬한 코드·파일·API 후보",
            "candidate",
            "코드 후보를 찾지 못했습니다. 코드 영향 없음으로 확정하지 않습니다.",
        ),
        "unknowns" => (
            3,
            "확인 필요",
            "지원 범위·stale·누락·끊긴 경로",
            "unknown",
            "현재 snapshot에 기록된 추가 확인 항목은 없습니다.",
        ),
        "checks" => (
            4,
            "권장 확인",
            "수정 전에 열어볼 근거를 순서대로 정리",
            "action",
            "확정 근거가 부족해 권장 확인 순서를 만들 수 없습니다.",
        ),
        _ => unreachable!("review lane ids are fixed by the projection"),
    };
    let total = items.len();
    items.truncate(limit);
    ImpactReviewLane {
        id: id.to_string(),
        order,
        title: title.to_string(),
        description: description.to_string(),
        tone: tone.to_string(),
        total,
        hidden: total.saturating_sub(items.len()),
        empty_message: empty_message.to_string(),
        items,
    }
}

fn impact_markdown_summary(subject: &str, lanes: &[ImpactReviewLane]) -> String {
    let mut lines = vec![format!("# 변경 영향 검토 — {}", markdown_text(subject))];
    for lane in lanes {
        lines.push(String::new());
        lines.push(format!("## {}. {}", lane.order, markdown_text(&lane.title)));
        if lane.items.is_empty() {
            lines.push(format!("- {}", markdown_text(&lane.empty_message)));
        } else {
            for item in &lane.items {
                let marker = item.confidence.as_deref().unwrap_or(&item.truth_class);
                let location = item
                    .location
                    .as_ref()
                    .map(review_location)
                    .unwrap_or_default();
                lines.push(format!(
                    "- [{}] {} — {}{}",
                    markdown_text(marker),
                    markdown_text(&item.title),
                    markdown_text(&item.detail),
                    markdown_text(&location)
                ));
            }
            if lane.hidden > 0 {
                lines.push(format!("- +{}개 접힘", lane.hidden));
            }
        }
    }
    safe_text(&lines.join("\n"))
}

fn review_location(location: &SourceLocation) -> String {
    match location.line {
        Some(line) => format!(" · {}:L{line}", location.path),
        None => format!(" · {}", location.path),
    }
}

fn direct_object_kind(object: &InventoryItem, evidence: &[Evidence]) -> String {
    if object.kind == "index" {
        if evidence
            .iter()
            .any(|entry| entry.kind == "db-index-primary" && entry.text == "true")
        {
            return "primary-index".to_string();
        }
        if evidence
            .iter()
            .any(|entry| entry.kind == "db-index-unique" && entry.text == "true")
        {
            return "unique-index".to_string();
        }
        return "index".to_string();
    }
    evidence
        .iter()
        .find(|entry| entry.kind == "db-constraint-kind")
        .map(|entry| entry.text.replace('_', "-"))
        .or_else(|| {
            object
                .engine_label
                .as_deref()
                .and_then(|label| label.strip_prefix("Constraint:"))
                .map(|kind| kind.replace('_', "-"))
        })
        .unwrap_or_else(|| {
            if object.is_primary_key {
                "primary-key".to_string()
            } else if object.is_foreign_key {
                "foreign-key".to_string()
            } else {
                "constraint".to_string()
            }
        })
}

fn direct_kind_label(kind: &str) -> &str {
    match kind {
        "primary-key" => "PK",
        "foreign-key" | "foreign-key-reference" => "FK",
        "unique" => "UNIQUE",
        "check" => "CHECK",
        "primary-index" => "PRIMARY INDEX",
        "unique-index" => "UNIQUE INDEX",
        "index" => "INDEX",
        _ => "CONSTRAINT",
    }
}

fn direct_review_rank(kind: &str) -> u8 {
    match kind {
        "primary-key" => 0,
        "foreign-key" => 1,
        "foreign-key-reference" => 2,
        "unique" => 3,
        "check" => 4,
        "primary-index" => 5,
        "unique-index" => 6,
        "index" => 7,
        _ => 8,
    }
}

fn candidate_review_rank(kind: &str) -> u8 {
    match kind {
        "repository" => 0,
        "function" | "method" | "handler" => 1,
        "service" => 2,
        "api" | "route" => 3,
        "file" => 4,
        _ => 5,
    }
}

fn candidate_detail(item: &InventoryItem) -> String {
    match item.kind.as_str() {
        "api" | "route" => "API",
        "repository" => "Repository",
        "function" | "method" | "handler" => "Function",
        "service" => "Service",
        "file" => "File",
        _ => "Code",
    }
    .to_string()
}

fn candidate_check_kind(item: &ImpactReviewItem) -> &'static str {
    let haystack = format!(
        "{} {} {}",
        item.kind,
        item.title,
        item.location
            .as_ref()
            .map(|location| location.path.as_str())
            .unwrap_or_default()
    )
    .to_ascii_lowercase();
    let tokens = haystack
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<HashSet<_>>();
    if ["migration", "migrations", "ddl", "schema"]
        .iter()
        .any(|token| tokens.contains(*token))
    {
        "migration"
    } else if ["test", "tests", "spec"]
        .iter()
        .any(|token| tokens.contains(*token))
    {
        "test"
    } else if matches!(
        item.kind.as_str(),
        "repository" | "query" | "mapper" | "dao"
    ) {
        "data-access"
    } else if matches!(item.kind.as_str(), "api" | "route") {
        "api"
    } else {
        "code"
    }
}

fn check_review_rank(kind: &str) -> u8 {
    match kind {
        "reindex" => 0,
        "constraint" => 1,
        "migration" => 2,
        "migration-missing" => 3,
        "data-access" => 4,
        "code" => 5,
        "api" => 6,
        "test" => 7,
        "test-missing" => 8,
        "coverage" => 9,
        _ => 10,
    }
}

fn check_action_label(kind: &str) -> &str {
    match kind {
        "migration" => "마이그레이션 확인",
        "test" => "회귀 테스트 확인",
        "data-access" => "데이터 접근 확인",
        "api" => "API 경계 확인",
        _ => "코드 확인",
    }
}

fn unknown_review_item(id: String, kind: &str, title: &str, detail: &str) -> ImpactReviewItem {
    ImpactReviewItem {
        id,
        node_id: None,
        kind: kind.to_string(),
        title: title.to_string(),
        detail: safe_text(detail),
        truth_class: "unknown".to_string(),
        confidence: None,
        rank: 0,
        evidence: Vec::new(),
        location: None,
    }
}

fn action_review_item(
    id: &str,
    node_id: Option<String>,
    kind: &str,
    title: &str,
    detail: &str,
    location: Option<SourceLocation>,
    evidence: Vec<Evidence>,
) -> ImpactReviewItem {
    ImpactReviewItem {
        id: id.to_string(),
        node_id,
        kind: kind.to_string(),
        title: title.to_string(),
        detail: safe_text(detail),
        truth_class: "action".to_string(),
        confidence: None,
        rank: 0,
        evidence: safe_evidence(&evidence),
        location,
    }
}

fn assign_review_ranks(items: &mut [ImpactReviewItem]) {
    for (index, item) in items.iter_mut().enumerate() {
        item.rank = index + 1;
    }
}

fn safe_evidence(evidence: &[Evidence]) -> Vec<Evidence> {
    let mut seen = HashSet::new();
    evidence
        .iter()
        .filter_map(|entry| {
            let text = safe_text(&entry.text);
            seen.insert((entry.kind.clone(), text.clone()))
                .then(|| Evidence {
                    kind: entry.kind.clone(),
                    text,
                })
        })
        .take(6)
        .collect()
}

fn safe_text(value: &str) -> String {
    crate::engine::redact_secrets(value)
}

fn markdown_text(value: &str) -> String {
    value
        .replace(['\r', '\n'], " ")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

fn review_truth_class(value: &str) -> String {
    match value {
        "confirmed" => "confirmed",
        "structural" => "structural",
        _ => "unknown",
    }
    .to_string()
}

fn impact_link_touches_focus(
    link: &SnapshotLink,
    table: &InventoryItem,
    column: Option<&InventoryItem>,
    item_by_id: &HashMap<&str, &InventoryItem>,
) -> bool {
    if let Some(column) = column {
        return link.from == column.id || link.to == column.id;
    }
    [link.from.as_str(), link.to.as_str()].iter().any(|id| {
        *id == table.id
            || item_by_id
                .get(*id)
                .is_some_and(|item| item.parent_id.as_deref() == Some(table.id.as_str()))
    })
}

fn related_fk_node_id(
    link: &SnapshotLink,
    table: &InventoryItem,
    item_by_id: &HashMap<&str, &InventoryItem>,
) -> Option<String> {
    [link.from.as_str(), link.to.as_str()]
        .iter()
        .find_map(|id| {
            let item = item_by_id.get(*id)?;
            let parent = item.parent_id.as_deref()?;
            (parent != table.id).then(|| parent.to_string())
        })
        .or_else(|| Some(link.from.clone()))
}

fn review_item_mentions_node(
    item: &ImpactReviewItem,
    node_id: &str,
    links_by_from: &ImpactLinkIndex<'_>,
) -> bool {
    item.node_id.as_deref() == Some(node_id)
        || item.node_id.as_deref().is_some_and(|object_id| {
            links_by_from
                .get(object_id)
                .into_iter()
                .flatten()
                .any(|link| {
                    link.to == node_id && matches!(link.kind.as_str(), "db_constraint" | "db_index")
                })
        })
}

fn impact_link_indexes(snapshot: &InventorySnapshot) -> (ImpactLinkIndex<'_>, ImpactLinkIndex<'_>) {
    let mut by_from = ImpactLinkIndex::new();
    let mut by_to = ImpactLinkIndex::new();
    for link in &snapshot.links {
        by_from.entry(link.from.as_str()).or_default().push(link);
        by_to.entry(link.to.as_str()).or_default().push(link);
    }
    (by_from, by_to)
}

fn api_reachable_code_ids(
    snapshot: &InventorySnapshot,
    links_by_from: &ImpactLinkIndex<'_>,
    max_depth: usize,
    limit: usize,
) -> HashSet<String> {
    let mut visited = snapshot
        .items
        .iter()
        .filter(|item| item.source == "code" && item.layer == "api")
        .map(|item| item.id.clone())
        .collect::<HashSet<_>>();
    let mut queue = visited
        .iter()
        .cloned()
        .map(|id| (id, 0usize))
        .collect::<VecDeque<_>>();
    while let Some((id, depth)) = queue.pop_front() {
        if depth >= max_depth || visited.len() >= limit {
            continue;
        }
        for link in links_by_from
            .get(id.as_str())
            .into_iter()
            .flatten()
            .filter(|link| {
                link.truth_class == "confirmed"
                    && matches!(link.kind.as_str(), "code_handle" | "code_call")
            })
        {
            if visited.insert(link.to.clone()) {
                queue.push_back((link.to.clone(), depth + 1));
            }
            if visited.len() >= limit {
                break;
            }
        }
    }
    visited
}

fn compact_token(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>()
}

fn node_sort_key(item: Option<&InventoryItem>) -> (u8, String) {
    match item {
        Some(item) => (layer_rank(&item.layer), item.name.clone()),
        None => (9, String::new()),
    }
}

fn confidence_rank(confidence: &str) -> u8 {
    match confidence {
        "high" => 0,
        "medium" => 1,
        "low" => 2,
        _ => 3,
    }
}

fn atlas_overview(snapshot: &InventorySnapshot, mode: String) -> VisualMap {
    let (groups, item_group, _) = atlas_groups(snapshot);
    let hidden = groups.len().saturating_sub(40);
    let visible_groups = groups.into_iter().take(40).collect::<Vec<_>>();
    let visible_ids = visible_groups
        .iter()
        .map(|group| group.id.clone())
        .collect::<HashSet<_>>();
    let nodes = visible_groups
        .iter()
        .map(atlas_group_node)
        .collect::<Vec<_>>();
    let (edges, hidden_edges) = atlas_group_edges(snapshot, &item_group, &visible_ids);
    let mut warnings = vec![format!(
        "원본 항목 {}개를 도메인 카드 {}개로 축약했습니다",
        snapshot.items.len(),
        nodes.len()
    )];
    if hidden > 0 {
        warnings.push(format!(
            "도메인 카드 +{hidden}개는 중요도 순위 밖이라 접었습니다"
        ));
    }
    if hidden_edges > 0 {
        warnings.push(format!(
            "그룹 간 관계 +{hidden_edges}개는 우선순위 밖이라 접었습니다"
        ));
    }

    VisualMap {
        id: format!("map:{}:atlas", snapshot.workspace_id),
        workspace_id: snapshot.workspace_id.clone(),
        mode,
        focus: "overview".to_string(),
        nodes,
        edges,
        warnings,
        review_board: None,
        api_reading: None,
    }
}

fn atlas_group_detail(snapshot: &InventorySnapshot, group_id: &str, mode: String) -> VisualMap {
    let (groups, _, item_evidence) = atlas_groups(snapshot);
    let Some(group) = groups.iter().find(|group| group.id == group_id) else {
        let mut map = atlas_overview(snapshot, mode);
        map.warnings
            .push("선택한 도메인 카드를 찾지 못해 전체 구조를 표시합니다".to_string());
        return map;
    };

    let item_by_id = snapshot
        .items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let members = select_atlas_detail_members(&group.member_ids, &item_by_id, 35);
    let hidden = group.member_ids.len().saturating_sub(members.len());
    let visible_member_ids = members
        .iter()
        .map(|item| item.id.as_str())
        .collect::<HashSet<_>>();
    let mut nodes = Vec::with_capacity(members.len() + 1);
    nodes.push(atlas_group_node(group));
    nodes.extend(members.iter().map(|item| visual_node(item)));

    let mut edges = members
        .iter()
        .map(|item| VisualEdge {
            id: format!("group-contains:{group_id}->{}", item.id),
            from: group_id.to_string(),
            to: item.id.clone(),
            kind: "group_contains".to_string(),
            confidence: None,
            evidence: item_evidence
                .get(item.id.as_str())
                .map(|text| {
                    vec![Evidence {
                        kind: "group-evidence".to_string(),
                        text: text.clone(),
                    }]
                })
                .unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    let (member_edges, hidden_edges) =
        atlas_member_edges(snapshot, &item_by_id, &visible_member_ids);
    edges.extend(member_edges);
    edges.sort_by(|left, right| left.id.cmp(&right.id));

    let mut warnings = vec![format!(
        "{} 도메인 · API {} → 코드 {} → DB {} 순서로 표시합니다",
        group.title, group.api_count, group.code_count, group.db_count
    )];
    if hidden > 0 {
        warnings.push(format!(
            "도메인 항목 +{hidden}개는 상세 화면에서 접었습니다"
        ));
    }
    if hidden_edges > 0 {
        warnings.push(format!(
            "도메인 관계 +{hidden_edges}개는 우선순위 밖이라 접었습니다"
        ));
    }

    VisualMap {
        id: format!("map:{}:atlas:{group_id}", snapshot.workspace_id),
        workspace_id: snapshot.workspace_id.clone(),
        mode,
        focus: group_id.to_string(),
        nodes,
        edges,
        warnings,
        review_board: None,
        api_reading: None,
    }
}

fn narrow_focus_map(snapshot: &InventorySnapshot, mode: String) -> VisualMap {
    VisualMap {
        id: format!("map:{}:narrow-focus:{mode}", snapshot.workspace_id),
        workspace_id: snapshot.workspace_id.clone(),
        mode,
        focus: "narrow-focus".to_string(),
        nodes: Vec::new(),
        edges: Vec::new(),
        warnings: vec![
            "결과가 너무 넓습니다. 왼쪽 목록에서 API/테이블 대상을 선택하거나 검색어를 좁히세요."
                .to_string(),
        ],
        review_board: None,
        api_reading: None,
    }
}

fn mode_node_cap(mode: &str) -> usize {
    match mode {
        "atlas" | "explore" => 40,
        "api-flow" | "search-focus" => 32,
        "table-usage" | "column-impact" => 36,
        _ => 30,
    }
}

fn atlas_groups(
    snapshot: &InventorySnapshot,
) -> (
    Vec<AtlasGroup>,
    HashMap<String, String>,
    HashMap<String, String>,
) {
    let mut groups = HashMap::<String, AtlasGroup>::new();
    let mut item_group = HashMap::new();
    let mut item_evidence = HashMap::new();

    for item in &snapshot.items {
        let Some(seed) = atlas_group_seed(item) else {
            continue;
        };
        let group_id = format!("group:domain:{}", slug(&seed.key));
        item_group.insert(item.id.clone(), group_id.clone());
        item_evidence.insert(item.id.clone(), seed.evidence.clone());
        groups
            .entry(group_id.clone())
            .and_modify(|group| group.add(item, &seed))
            .or_insert_with(|| AtlasGroup::new(group_id, item, &seed));
    }

    // FK endpoints are columns, but the architecture card owns the table rather than every column.
    for item in snapshot.items.iter().filter(|item| item.kind == "column") {
        let Some(parent_id) = item.parent_id.as_deref() else {
            continue;
        };
        let Some(group_id) = item_group.get(parent_id).cloned() else {
            continue;
        };
        item_group.insert(item.id.clone(), group_id);
    }

    for link in snapshot
        .links
        .iter()
        .filter(|link| link.truth_class == "confirmed")
    {
        let Some(from) = item_group.get(&link.from) else {
            continue;
        };
        let Some(to) = item_group.get(&link.to) else {
            continue;
        };
        if let Some(group) = groups.get_mut(from) {
            group.confirmed_degree += 1;
        }
        if from != to {
            if let Some(group) = groups.get_mut(to) {
                group.confirmed_degree += 1;
            }
        }
    }

    let item_by_id = snapshot
        .items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let mut degrees = HashMap::<&str, usize>::new();
    for link in snapshot
        .links
        .iter()
        .filter(|link| link.truth_class == "confirmed")
    {
        *degrees.entry(link.from.as_str()).or_default() += 1;
        *degrees.entry(link.to.as_str()).or_default() += 1;
    }
    let mut groups = groups.into_values().collect::<Vec<_>>();
    for group in &mut groups {
        group.sort_members(&item_by_id, &degrees);
    }
    groups.sort_by(|a, b| {
        let a_has_product_surface = a.api_count > 0 || a.db_count > 0;
        let b_has_product_surface = b.api_count > 0 || b.db_count > 0;
        b_has_product_surface
            .cmp(&a_has_product_surface)
            .then_with(|| b.confirmed_degree.cmp(&a.confirmed_degree))
            .then_with(|| b.api_count.cmp(&a.api_count))
            .then_with(|| b.db_count.cmp(&a.db_count))
            .then_with(|| b.member_ids.len().cmp(&a.member_ids.len()))
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.id.cmp(&b.id))
    });
    (groups, item_group, item_evidence)
}

fn atlas_group_seed(item: &InventoryItem) -> Option<AtlasGroupSeed> {
    if item.source == "code" && item.layer == "api" {
        let label = route_domain(&item.name).unwrap_or_else(|| "root".to_string());
        return Some(AtlasGroupSeed {
            key: canonical_domain(&label),
            label: label.clone(),
            title_priority: 0,
            evidence: format!("라우트 경로에서 도메인 `{label}`을 읽었습니다"),
        });
    }
    if item.source == "code" && item.layer == "code" {
        let label = item
            .group_id
            .as_deref()
            .and_then(group_id_domain)
            .or_else(|| item.path.as_deref().and_then(path_domain))
            .or_else(|| text_domain(&item.name))
            .unwrap_or_else(|| "code".to_string());
        let evidence_source = item
            .group_id
            .as_deref()
            .or(item.path.as_deref())
            .unwrap_or(&item.name);
        return Some(AtlasGroupSeed {
            key: canonical_domain(&label),
            label,
            title_priority: 1,
            evidence: format!("코드 경로/그룹 `{evidence_source}` 기준으로 묶었습니다"),
        });
    }
    if item.source == "db" && item.kind == "table" {
        let schema = item.path.as_deref().filter(|schema| !schema.is_empty());
        let label = schema
            .filter(|schema| !is_default_schema(schema))
            .and_then(text_domain)
            .or_else(|| text_domain(&item.name))
            .unwrap_or_else(|| schema.unwrap_or("database").to_string());
        let evidence = match schema {
            Some(schema) if !is_default_schema(schema) => {
                format!("DB 스키마 `{schema}` 기준으로 묶었습니다")
            }
            Some(schema) => format!("DB `{schema}.{}` 테이블명 기준으로 묶었습니다", item.name),
            None => format!("DB `{}` 테이블명 기준으로 묶었습니다", item.name),
        };
        return Some(AtlasGroupSeed {
            key: canonical_domain(&label),
            label,
            title_priority: 2,
            evidence,
        });
    }

    None
}

fn atlas_group_node(group: &AtlasGroup) -> VisualNode {
    VisualNode {
        id: group.id.clone(),
        kind: "group-domain".to_string(),
        title: group.title.clone(),
        subtitle: Some(format!(
            "API {} · 코드 {} · DB {}|{}|{}|{}",
            group.api_count,
            group.code_count,
            group.db_count,
            atlas_top_summary(&group.top_api, group.api_count),
            atlas_top_summary(&group.top_code, group.code_count),
            atlas_top_summary(&group.top_db, group.db_count)
        )),
        layer: "mixed".to_string(),
        source: "projection".to_string(),
    }
}

fn atlas_group_edges(
    snapshot: &InventorySnapshot,
    item_group: &HashMap<String, String>,
    visible_ids: &HashSet<String>,
) -> (Vec<VisualEdge>, usize) {
    let mut seen = HashSet::new();
    let mut edges = snapshot
        .links
        .iter()
        .filter_map(|link| {
            let from = item_group.get(&link.from)?;
            let to = item_group.get(&link.to)?;
            if from == to || !visible_ids.contains(from) || !visible_ids.contains(to) {
                return None;
            }
            let kind = atlas_truth_kind(link, "group_")?;
            let id = format!("{kind}:{from}->{to}");
            if !seen.insert(id.clone()) {
                return None;
            }
            Some(VisualEdge {
                id,
                from: from.clone(),
                to: to.clone(),
                kind,
                confidence: None,
                evidence: link.evidence.clone(),
            })
        })
        .collect::<Vec<_>>();
    edges.sort_by(|left, right| {
        atlas_projection_edge_rank(left)
            .cmp(&atlas_projection_edge_rank(right))
            .then_with(|| left.id.cmp(&right.id))
    });
    let hidden = edges.len().saturating_sub(80);
    edges.truncate(80);
    (edges, hidden)
}

fn atlas_member_edges(
    snapshot: &InventorySnapshot,
    item_by_id: &HashMap<&str, &InventoryItem>,
    visible_ids: &HashSet<&str>,
) -> (Vec<VisualEdge>, usize) {
    let mut seen = HashSet::new();
    let mut edges = snapshot
        .links
        .iter()
        .filter_map(|link| {
            let from = atlas_visible_endpoint(&link.from, item_by_id, visible_ids)?;
            let to = atlas_visible_endpoint(&link.to, item_by_id, visible_ids)?;
            if from == to {
                return None;
            }
            let kind = atlas_truth_kind(link, "")?;
            let id = format!("atlas:{kind}:{from}->{to}");
            if !seen.insert(id.clone()) {
                return None;
            }
            Some(VisualEdge {
                id,
                from: from.to_string(),
                to: to.to_string(),
                kind,
                confidence: None,
                evidence: link.evidence.clone(),
            })
        })
        .collect::<Vec<_>>();
    edges.sort_by(|left, right| {
        atlas_projection_edge_rank(left)
            .cmp(&atlas_projection_edge_rank(right))
            .then_with(|| left.id.cmp(&right.id))
    });
    let hidden = edges.len().saturating_sub(64);
    edges.truncate(64);
    (edges, hidden)
}

fn atlas_truth_kind(link: &SnapshotLink, prefix: &str) -> Option<String> {
    match link.truth_class.as_str() {
        "confirmed" => Some(format!("{prefix}{}", link.kind)),
        "candidate" => Some(format!("candidate_{prefix}{}", link.kind)),
        "structural" | "" => Some(format!("structural_{prefix}{}", link.kind)),
        _ => None,
    }
}

fn atlas_projection_edge_rank(edge: &VisualEdge) -> u8 {
    if edge.kind.starts_with("candidate_") {
        2
    } else if edge.kind.starts_with("structural_") {
        1
    } else {
        0
    }
}

fn atlas_visible_endpoint<'a>(
    id: &'a str,
    item_by_id: &HashMap<&str, &'a InventoryItem>,
    visible_ids: &HashSet<&str>,
) -> Option<&'a str> {
    if visible_ids.contains(id) {
        return Some(id);
    }
    item_by_id
        .get(id)
        .and_then(|item| item.parent_id.as_deref())
        .filter(|parent| visible_ids.contains(parent))
}

fn route_domain(value: &str) -> Option<String> {
    let path = value
        .split_whitespace()
        .find(|part| part.starts_with('/'))
        .unwrap_or(value);
    path.trim_start_matches('/')
        .split('/')
        .find_map(text_domain)
}

fn path_domain(value: &str) -> Option<String> {
    let mut parts = value
        .split(['/', '\\'])
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>();
    if parts.len() > 1 {
        // The final segment is normally a source file. Grouping by it merges every
        // `service.rs`/`index.ts` across the project instead of the owning folder.
        parts.pop();
    }
    parts.into_iter().rev().find_map(text_domain)
}

fn group_id_domain(value: &str) -> Option<String> {
    value
        .split(['/', '\\', '.', ':'])
        .filter(|part| !part.is_empty())
        .rev()
        .find_map(text_domain)
}

fn text_domain(value: &str) -> Option<String> {
    semantic_tokens(value).into_iter().next()
}

fn canonical_domain(value: &str) -> String {
    text_domain(value).unwrap_or_else(|| "other".to_string())
}

fn semantic_tokens(value: &str) -> Vec<String> {
    let mut words = String::with_capacity(value.len());
    let mut previous_lower = false;
    for character in value.chars() {
        if character.is_alphanumeric() {
            if character.is_uppercase() && previous_lower {
                words.push(' ');
            }
            for lower in character.to_lowercase() {
                words.push(lower);
            }
            previous_lower = character.is_lowercase();
        } else {
            words.push(' ');
            previous_lower = false;
        }
    }

    words
        .split_whitespace()
        .filter(|word| !is_generic_domain_word(word))
        .map(singular_domain)
        .collect()
}

fn singular_domain(value: &str) -> String {
    if value.len() > 3 && value.ends_with("ies") {
        return format!("{}y", &value[..value.len() - 3]);
    }
    for suffix in ["ches", "shes", "xes", "zes", "ses"] {
        if value.len() > suffix.len() && value.ends_with(suffix) {
            return value[..value.len() - 2].to_string();
        }
    }
    if value.len() > 2
        && value.ends_with('s')
        && !value.ends_with("ss")
        && !value.ends_with("us")
        && !value.ends_with("is")
    {
        return value[..value.len() - 1].to_string();
    }
    value.to_string()
}

fn is_generic_domain_word(value: &str) -> bool {
    value.len() < 2
        || value.chars().all(|character| character.is_ascii_digit())
        || (value.starts_with('v')
            && value[1..]
                .chars()
                .all(|character| character.is_ascii_digit()))
        || matches!(
            value,
            "api"
                | "app"
                | "apps"
                | "backend"
                | "code"
                | "common"
                | "controller"
                | "controllers"
                | "core"
                | "create"
                | "database"
                | "db"
                | "domain"
                | "domains"
                | "feature"
                | "features"
                | "find"
                | "get"
                | "handler"
                | "handlers"
                | "internal"
                | "id"
                | "java"
                | "kotlin"
                | "lib"
                | "libs"
                | "list"
                | "main"
                | "model"
                | "models"
                | "module"
                | "modules"
                | "package"
                | "packages"
                | "python"
                | "pkg"
                | "repository"
                | "repositories"
                | "repo"
                | "read"
                | "route"
                | "routes"
                | "schema"
                | "save"
                | "server"
                | "service"
                | "services"
                | "shared"
                | "source"
                | "src"
                | "test"
                | "tests"
                | "update"
                | "util"
                | "utils"
                | "write"
                | "delete"
                | "com"
                | "org"
                | "net"
                | "io"
                | "js"
                | "jsx"
                | "ts"
                | "tsx"
                | "rs"
                | "py"
                | "kt"
        )
}

fn is_default_schema(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "public" | "dbo" | "main" | "default"
    )
}

fn slug(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn layer_rank(layer: &str) -> u8 {
    match layer {
        "api" => 0,
        "code" => 1,
        "data" => 2,
        _ => 3,
    }
}

struct AtlasGroupSeed {
    key: String,
    label: String,
    title_priority: u8,
    evidence: String,
}

struct AtlasGroup {
    id: String,
    title: String,
    title_priority: u8,
    member_ids: Vec<String>,
    api_count: usize,
    code_count: usize,
    db_count: usize,
    confirmed_degree: usize,
    top_api: Vec<String>,
    top_code: Vec<String>,
    top_db: Vec<String>,
}

impl AtlasGroup {
    fn new(id: String, item: &InventoryItem, seed: &AtlasGroupSeed) -> Self {
        let mut group = Self {
            id,
            title: seed.label.clone(),
            title_priority: seed.title_priority,
            member_ids: Vec::new(),
            api_count: 0,
            code_count: 0,
            db_count: 0,
            confirmed_degree: 0,
            top_api: Vec::new(),
            top_code: Vec::new(),
            top_db: Vec::new(),
        };
        group.add(item, seed);
        group
    }

    fn add(&mut self, item: &InventoryItem, seed: &AtlasGroupSeed) {
        if seed.title_priority < self.title_priority
            || (seed.title_priority == self.title_priority && seed.label < self.title)
        {
            self.title = seed.label.clone();
            self.title_priority = seed.title_priority;
        }
        self.member_ids.push(item.id.clone());
        if item.layer == "api" {
            self.api_count += 1;
        } else if item.source == "db" {
            self.db_count += 1;
        } else {
            self.code_count += 1;
        }
    }

    fn sort_members(
        &mut self,
        item_by_id: &HashMap<&str, &InventoryItem>,
        degrees: &HashMap<&str, usize>,
    ) {
        self.member_ids.sort_by(|left, right| {
            let left_item = item_by_id.get(left.as_str()).copied().unwrap();
            let right_item = item_by_id.get(right.as_str()).copied().unwrap();
            let left_order = atlas_member_order(left_item);
            let right_order = atlas_member_order(right_item);
            left_order
                .0
                .cmp(&right_order.0)
                .then_with(|| {
                    degrees
                        .get(right.as_str())
                        .unwrap_or(&0)
                        .cmp(degrees.get(left.as_str()).unwrap_or(&0))
                })
                .then_with(|| left_order.1.cmp(&right_order.1))
                .then_with(|| left_item.name.cmp(&right_item.name))
                .then_with(|| left_item.id.cmp(&right_item.id))
        });
        self.top_api = atlas_top_titles(&self.member_ids, item_by_id, "api");
        self.top_code = atlas_top_titles(&self.member_ids, item_by_id, "code");
        self.top_db = atlas_top_titles(&self.member_ids, item_by_id, "db");
    }
}

fn atlas_top_summary(items: &[String], total: usize) -> String {
    let mut summary = items.join(" · ");
    let hidden = total.saturating_sub(items.len());
    if hidden > 0 {
        if !summary.is_empty() {
            summary.push_str(" · ");
        }
        summary.push_str(&format!("+{hidden}"));
    }
    summary
}

fn select_atlas_detail_members<'a>(
    member_ids: &[String],
    item_by_id: &HashMap<&str, &'a InventoryItem>,
    limit: usize,
) -> Vec<&'a InventoryItem> {
    let members = member_ids
        .iter()
        .filter_map(|id| item_by_id.get(id.as_str()).copied())
        .collect::<Vec<_>>();
    if members.len() <= limit {
        return members;
    }

    let mut selected = HashSet::new();
    for layer in 0..=2 {
        for item in members
            .iter()
            .filter(|item| atlas_member_order(item).0 == layer)
            .take(4)
        {
            if selected.len() >= limit {
                break;
            }
            selected.insert(item.id.as_str());
        }
    }
    for item in &members {
        if selected.len() >= limit {
            break;
        }
        selected.insert(item.id.as_str());
    }
    members
        .into_iter()
        .filter(|item| selected.contains(item.id.as_str()))
        .collect()
}

fn atlas_member_order(item: &InventoryItem) -> (u8, u8) {
    let layer = if item.layer == "api" {
        0
    } else if item.source == "code" {
        1
    } else {
        2
    };
    let kind = match item.kind.as_str() {
        "handler" => 0,
        "service" => 1,
        "repository" => 2,
        "function" | "method" => 3,
        "class" => 4,
        "file" => 5,
        "table" => 0,
        _ => 6,
    };
    (layer, kind)
}

fn atlas_top_titles(
    member_ids: &[String],
    item_by_id: &HashMap<&str, &InventoryItem>,
    bucket: &str,
) -> Vec<String> {
    member_ids
        .iter()
        .filter_map(|id| item_by_id.get(id.as_str()).copied())
        .filter(|item| match bucket {
            "api" => item.layer == "api",
            "code" => item.source == "code" && item.layer != "api",
            "db" => item.source == "db" && item.kind == "table",
            _ => false,
        })
        .map(|item| item.name.replace('|', "/"))
        .take(2)
        .collect()
}

#[cfg(test)]
pub(crate) fn fixture_inventory(workspace_id: String) -> InventorySnapshot {
    InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id,
        saved_at: timestamp(),
        metadata: Default::default(),
        stale_reasons: Vec::new(),
        links: vec![
            super::model::SnapshotLink {
                id: "code-handle:route-orders-create->create-order-handler".to_string(),
                from: "code:route:orders:create".to_string(),
                to: "code:function:CreateOrderHandler".to_string(),
                kind: "code_handle".to_string(),
                label: Some("HANDLES".to_string()),
                truth_class: "confirmed".to_string(),
                direction: "outbound".to_string(),
                engine_edge_type: Some("HANDLES".to_string()),
                evidence: vec![Evidence {
                    kind: "engine-edge".to_string(),
                    text: "HANDLES 관계를 제품 읽기 방향으로 정규화했습니다".to_string(),
                }],
            },
            super::model::SnapshotLink {
                id: "code-call:create-order-handler->order-service".to_string(),
                from: "code:function:CreateOrderHandler".to_string(),
                to: "code:class:OrderService".to_string(),
                kind: "code_call".to_string(),
                label: Some("CALLS".to_string()),
                truth_class: "confirmed".to_string(),
                direction: "outbound".to_string(),
                engine_edge_type: Some("CALLS".to_string()),
                evidence: vec![Evidence {
                    kind: "engine-edge".to_string(),
                    text: "CALLS 관계를 코드 엔진에서 직접 읽었습니다".to_string(),
                }],
            },
            super::model::SnapshotLink {
                id: "db-fk:orders:customer_id->customers:id".to_string(),
                from: "db:column:orders:customer_id".to_string(),
                to: "db:column:customers:id".to_string(),
                kind: "db_fk".to_string(),
                label: Some("orders_customer_id_fkey".to_string()),
                truth_class: "confirmed".to_string(),
                direction: "outbound".to_string(),
                engine_edge_type: Some("FK_TO_COLUMN".to_string()),
                evidence: vec![Evidence {
                    kind: "db-constraint".to_string(),
                    text: "외래 키 제약을 DB 메타데이터에서 직접 읽었습니다".to_string(),
                }],
            },
        ],
        items: vec![
            item(
                "code:route:orders:create",
                "api",
                "POST /orders",
                "api",
                "code",
                None,
                None,
            ),
            item(
                "code:function:CreateOrderHandler",
                "handler",
                "CreateOrderHandler",
                "code",
                "code",
                None,
                Some("order_handler.ts"),
            ),
            item(
                "code:class:OrderService",
                "service",
                "OrderService",
                "code",
                "code",
                None,
                Some("order_service.ts"),
            ),
            item(
                "code:class:OrderRepository",
                "repository",
                "OrderRepository",
                "code",
                "code",
                None,
                Some("order_repository_customer_id.ts"),
            ),
            item(
                "db:table:orders",
                "table",
                "orders",
                "data",
                "db",
                None,
                Some("public"),
            ),
            InventoryItem {
                is_primary_key: true,
                ..item(
                    "db:column:orders:id",
                    "column",
                    "id",
                    "data",
                    "db",
                    Some("db:table:orders"),
                    Some("bigint"),
                )
            },
            InventoryItem {
                is_foreign_key: true,
                ..item(
                    "db:column:orders:customer_id",
                    "column",
                    "customer_id",
                    "data",
                    "db",
                    Some("db:table:orders"),
                    Some("bigint"),
                )
            },
            item(
                "db:table:customers",
                "table",
                "customers",
                "data",
                "db",
                None,
                Some("public"),
            ),
            InventoryItem {
                is_primary_key: true,
                ..item(
                    "db:column:customers:id",
                    "column",
                    "id",
                    "data",
                    "db",
                    Some("db:table:customers"),
                    Some("bigint"),
                )
            },
        ],
    }
}
