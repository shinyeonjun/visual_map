use std::collections::{BTreeMap, HashMap, HashSet};

use super::architecture::{mode_node_cap, narrow_focus_map};
use super::impact_review::{direct_object_kind, direct_review_rank, impact_review_board};
use super::linker::{
    candidate_links, identifier_terms, identifier_tokens, merge_evidence, table_aliases,
};
use super::model::{
    ChangeIntent, InventoryItem, InventorySnapshot, SnapshotLink, VisualEdge, VisualMap, VisualNode,
};
use super::projection_support::{confidence_rank, node_sort_key};
use super::visual_map::{confirmed_link_edges, focus_neighborhood_map, visual_node};

pub(super) fn table_detail_map(
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
            .iter()
            .filter(|link| link.to == table.id)
            .cloned()
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
    let table_column_ids = columns
        .iter()
        .map(|column| column.id.as_str())
        .collect::<HashSet<_>>();
    let mut db_dependents = snapshot
        .items
        .iter()
        .filter(|item| matches!(item.kind.as_str(), "view" | "trigger" | "routine"))
        .filter(|item| {
            snapshot.links.iter().any(|link| {
                matches!(link.kind.as_str(), "db_dependency" | "db_trigger")
                    && (link.from == item.id || link.to == item.id)
                    && (link.from == table.id
                        || link.to == table.id
                        || table_column_ids.contains(link.from.as_str())
                        || table_column_ids.contains(link.to.as_str()))
            })
        })
        .collect::<Vec<_>>();
    db_dependents.sort_by_key(|item| {
        (
            direct_review_rank(&direct_object_kind(item, &[])),
            item.name.clone(),
        )
    });
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
        + db_dependents.len()
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
        .chain(db_dependents)
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
                && matches!(item.kind.as_str(), "column" | "constraint" | "index")
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

    let review_board = impact_review_board(snapshot, table, None, &review_candidate_edges, None);
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

pub(super) fn column_impact_map(
    snapshot: &InventorySnapshot,
    column_id: String,
    mode: String,
    change_intent: Option<&ChangeIntent>,
) -> VisualMap {
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
            matches!(
                link.kind.as_str(),
                "db_constraint" | "db_index" | "db_dependency"
            ) && link.to == column.id
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
        change_intent,
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

fn constraint_node(column: &InventoryItem, suffix: &str, title: &str) -> VisualNode {
    VisualNode {
        id: format!("db:constraint:{}:{suffix}", column.id),
        kind: "constraint".to_string(),
        title: title.to_string(),
        subtitle: Some("확정 DB 구조".to_string()),
        layer: "data".to_string(),
        source: "db".to_string(),
        location: None,
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
    let column_tokens = identifier_tokens(&column.name);
    let column_term = column_tokens.join("_");
    let parent_table_aliases = column
        .parent_id
        .as_deref()
        .and_then(|parent_id| snapshot.items.iter().find(|item| item.id == parent_id))
        .map(|table| table_aliases(&table.name))
        .unwrap_or_default();

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
                let terms = identifier_terms(&format!(
                    "{} {}",
                    item.name,
                    item.path.clone().unwrap_or_default()
                ));
                let has_column_term = !column_term.is_empty() && terms.contains(&column_term);
                let has_table_context = column_tokens.len() > 1
                    || parent_table_aliases
                        .iter()
                        .any(|alias| terms.contains(alias));
                (has_column_term && has_table_context).then_some(())?;

                Some(VisualEdge {
                    id: format!("candidate-column:{}->{}", item.id, column.id),
                    from: item.id.clone(),
                    to: column.id.clone(),
                    kind: "candidate_column_ref".to_string(),
                    confidence: Some("medium".to_string()),
                    evidence: vec![super::model::Evidence {
                        kind: "column-name-match".to_string(),
                        text: if column_tokens.len() > 1 {
                            format!(
                                "{} 코드 항목이 {} 컬럼 전체 식별자와 일치합니다",
                                item.name, column.name
                            )
                        } else {
                            format!(
                                "{} 코드 항목에서 테이블 문맥과 {} 컬럼명을 함께 찾았습니다",
                                item.name, column.name
                            )
                        },
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
