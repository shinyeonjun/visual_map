fn open_existing_store(cache_path: &Path) -> Result<GraphStore, String> {
    if !cache_path.exists() {
        return Err(format!(
            "cache path '{}' not found; run index first",
            cache_path.display()
        ));
    }
    GraphStore::open(cache_path).map_err(|err| err.to_string())
}

fn require_snapshot(store: &GraphStore, snapshot_key: &str) -> Result<(), String> {
    store
        .get_snapshot(snapshot_key)
        .map_err(|err| err.to_string())?
        .map(|_| ())
        .ok_or_else(|| format!("snapshot '{snapshot_key}' not found in cache; run index first"))
}

fn snapshot_capability_warnings(
    store: &GraphStore,
    snapshot_key: &str,
) -> Result<Vec<String>, String> {
    store
        .get_snapshot_capabilities(snapshot_key)
        .map_err(|err| err.to_string())?
        .map(|capabilities| capability_warnings(&capabilities))
        .ok_or_else(|| format!("snapshot '{snapshot_key}' not found in cache; run index first"))
}

fn cache_path(cache_path: Option<String>) -> PathBuf {
    cache_path
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CACHE_PATH))
}

fn paginate<T>(
    items: Vec<T>,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<Page<T>, String> {
    let offset = offset.unwrap_or(0);
    let limit_requested = limit.unwrap_or(DEFAULT_PAGE_LIMIT);
    if limit_requested == 0 {
        return Err("limit must be greater than zero".to_owned());
    }
    let limit_applied = limit_requested.min(MAX_PAGE_LIMIT);
    let total = items.len();
    let has_more = offset.saturating_add(limit_applied) < total;
    let items = items.into_iter().skip(offset).take(limit_applied).collect();

    Ok(Page {
        items,
        metadata: PageMetadata {
            total,
            offset,
            limit_requested,
            limit_applied,
            limit_clamped: limit_requested != limit_applied,
            has_more,
        },
    })
}

fn resolve_snapshot_key(store: &GraphStore, selector: &str) -> Result<String, String> {
    if store
        .get_snapshot(selector)
        .map_err(|error| error.to_string())?
        .is_some()
    {
        return Ok(selector.to_owned());
    }

    let matches = store
        .list_snapshots()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|snapshot| alias_from_snapshot_key(&snapshot.snapshot_key) == selector)
        .map(|snapshot| snapshot.snapshot_key)
        .collect::<Vec<_>>();

    match matches.as_slice() {
        [snapshot_key] => Ok(snapshot_key.clone()),
        [] => Err(format!("database snapshot '{selector}' not found")),
        _ => Err(format!(
            "database alias '{selector}' is ambiguous; pass one snapshot key: {}",
            matches.join(", ")
        )),
    }
}

fn alias_from_snapshot_key(snapshot_key: &str) -> String {
    snapshot_key
        .split_once(':')
        .map(|(_, alias)| alias.to_owned())
        .unwrap_or_else(|| snapshot_key.to_owned())
}

fn parse_direction(direction: &str) -> Result<Direction, String> {
    match direction {
        "inbound" => Ok(Direction::Inbound),
        "outbound" => Ok(Direction::Outbound),
        "both" => Ok(Direction::Both),
        _ => Err(format!(
            "unknown direction '{direction}'; expected inbound, outbound, or both"
        )),
    }
}

fn direction_name(direction: Direction) -> &'static str {
    match direction {
        Direction::Inbound => "inbound",
        Direction::Outbound => "outbound",
        Direction::Both => "both",
    }
}

fn node_json(node: &GraphNodeRecord) -> Value {
    json!({
        "snapshot_key": &node.snapshot_key,
        "node_key": &node.node_key,
        "label": &node.label,
        "display_name": &node.display_name,
        "payload": serde_json::from_str::<Value>(&node.payload_json).unwrap_or(Value::Null),
    })
}

fn edge_json(edge: &database_memory_core::graph_store::GraphEdgeRecord) -> Value {
    json!({
        "snapshot_key": &edge.snapshot_key,
        "edge_key": &edge.edge_key,
        "edge_from": &edge.edge_from,
        "edge_to": &edge.edge_to,
        "edge_type": &edge.edge_type,
        "payload": serde_json::from_str::<Value>(&edge.payload_json).unwrap_or(Value::Null),
    })
}

fn impact_json(result: &ImpactAnalysisResult) -> Value {
    json!({
        "snapshot_key": &result.snapshot_key,
        "object_key": &result.object_key,
        "direction": direction_name(result.direction),
        "max_depth": result.max_depth,
        "groups": result.groups.iter().map(|group| json!({
            "label": &group.label,
            "depth": group.depth,
            "nodes": group.nodes.iter().map(|node| json!({
                "node_key": &node.node_key,
                "label": &node.label,
                "display_name": &node.display_name,
                "depth": node.depth,
                "edge_type_used": &node.edge_type_used,
                "edge_from": &node.edge_from,
                "edge_to": &node.edge_to,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

fn graph_paths_json(paths: &[GraphPath]) -> Vec<Value> {
    paths
        .iter()
        .map(|path| {
            json!({
                "hops": path.hops.iter().map(|hop| json!({
                    "node_key": &hop.node_key,
                    "label": &hop.label,
                    "edge_type_used": &hop.edge_type_used,
                    "edge_from": &hop.edge_from,
                    "edge_to": &hop.edge_to,
                })).collect::<Vec<_>>()
            })
        })
        .collect()
}

fn schema_diff_json(
    bounded: &BoundedSchemaDiff,
    result_limit_requested: usize,
    result_limit_applied: usize,
) -> Value {
    let diff = &bounded.diff;
    json!({
        "from_snapshot_key": &diff.from_snapshot_key,
        "to_snapshot_key": &diff.to_snapshot_key,
        "counts": {
            "added_nodes": bounded.counts.added_nodes,
            "removed_nodes": bounded.counts.removed_nodes,
            "changed_nodes": bounded.counts.changed_nodes,
            "added_edges": bounded.counts.added_edges,
            "removed_edges": bounded.counts.removed_edges,
            "impacted_seeds": bounded.counts.impacted_seeds,
        },
        "result_limit_requested": result_limit_requested,
        "result_limit_applied": result_limit_applied,
        "result_limit_clamped": result_limit_requested != result_limit_applied,
        "truncated": bounded.truncated,
        "added_nodes": diff.added_nodes.iter().map(node_json).collect::<Vec<_>>(),
        "removed_nodes": diff.removed_nodes.iter().map(node_json).collect::<Vec<_>>(),
        "changed_nodes": diff.changed_nodes.iter().map(|changed| json!({
            "from": node_json(&changed.from),
            "to": node_json(&changed.to),
        })).collect::<Vec<_>>(),
        "added_edges": diff.added_edges.iter().map(edge_json).collect::<Vec<_>>(),
        "removed_edges": diff.removed_edges.iter().map(edge_json).collect::<Vec<_>>(),
        "impacted": diff.impacted.iter().map(|impact| json!({
            "seed_node_key": &impact.seed_node_key,
            "snapshot_key": &impact.snapshot_key,
            "truncated": impact.truncated,
            "impact": impact_json(&impact.impact),
        })).collect::<Vec<_>>(),
    })
}

