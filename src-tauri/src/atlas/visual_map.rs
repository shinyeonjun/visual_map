use std::collections::{HashMap, HashSet};

use super::api_flow::api_flow_map;
use super::architecture::{atlas_group_detail, atlas_overview, mode_node_cap, narrow_focus_map};
use super::impact::{column_impact_map, table_detail_map};
use super::linker::candidate_links;
use super::model::{
    ChangeIntent, Evidence, InventoryItem, InventorySnapshot, SnapshotLink, VisualEdge, VisualMap,
    VisualNode,
};
use super::projection_support::safe_evidence;
#[cfg(test)]
use super::snapshot::{item, timestamp};
#[cfg(test)]
pub(crate) fn visual_map(
    snapshot: &InventorySnapshot,
    focus: Option<String>,
    mode: String,
) -> VisualMap {
    visual_map_with_change(snapshot, focus, mode, None)
}

pub(crate) fn visual_map_with_change(
    snapshot: &InventorySnapshot,
    focus: Option<String>,
    mode: String,
    change_intent: Option<ChangeIntent>,
) -> VisualMap {
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
        return column_impact_map(snapshot, focus.unwrap(), mode, change_intent.as_ref());
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

pub(super) fn focus_neighborhood_map(
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

pub(super) fn visual_node(item: &InventoryItem) -> VisualNode {
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

pub(super) fn confirmed_link_edges(
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

pub(super) fn confirmed_link_edge(
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
