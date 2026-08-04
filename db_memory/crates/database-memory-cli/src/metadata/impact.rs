pub(crate) fn render_find_table(
    store: &GraphStore,
    snapshot_key: &str,
    query: &str,
    format: OutputFormat,
) -> Result<String, String> {
    let needle = query.to_lowercase();
    let mut table_matches = Vec::new();
    for node in store
        .nodes_by_label(snapshot_key, "Table")
        .map_err(|err| err.to_string())?
    {
        let key = object_key(&node)?;
        if key.object_name.to_lowercase().contains(&needle) {
            table_matches.push((key.to_string(), key.object_name, key.schema, key.database));
        }
    }
    table_matches.sort_by(|left, right| {
        left.1
            .cmp(&right.1)
            .then_with(|| left.2.cmp(&right.2))
            .then_with(|| left.3.cmp(&right.3))
            .then_with(|| left.0.cmp(&right.0))
    });
    let mut tables = table_matches
        .iter()
        .map(|(_, name, _, _)| name.clone())
        .collect::<Vec<_>>();
    tables.sort();
    match format {
        OutputFormat::Text => Ok(lines(&tables)),
        OutputFormat::Json => Ok(json_line(json!({
            "tables": tables,
            "table_matches": table_matches.into_iter().map(|(table_key, name, schema, database)| json!({
                "table_key": table_key,
                "name": name,
                "schema": schema,
                "database": database,
            })).collect::<Vec<_>>(),
        }))),
    }
}

pub(crate) fn render_find_column(
    store: &GraphStore,
    snapshot_key: &str,
    query: &str,
    format: OutputFormat,
) -> Result<String, String> {
    let needle = query.to_lowercase();
    let mut columns = Vec::new();
    for node in store
        .nodes_by_label(snapshot_key, "Column")
        .map_err(|err| err.to_string())?
    {
        let column = column_from_node(&node)?;
        let key = &column.key;
        let column_name = column.name.clone();
        if column_name.to_lowercase().contains(&needle) {
            let column_key = key.to_string();
            let table_key = ObjectKey::new(
                key.source_kind.clone(),
                key.connection_alias.clone(),
                key.database.clone(),
                key.schema.clone(),
                ObjectKind::Table,
                key.object_name.clone(),
                None,
            )
            .to_string();
            columns.push(json!({
                "key": &column_key,
                "column_key": column_key,
                "table_key": table_key,
                "schema": key.schema,
                "database": key.database,
                "table": key.object_name,
                "column": column_name,
                "ordinal_position": column.ordinal_position,
                "type": column.data_type,
                "nullable": column.is_nullable,
                "default_value": column.default_value,
                "generated": column.is_generated,
            }));
        }
    }
    columns.sort_by(|left, right| {
        left["table"]
            .as_str()
            .cmp(&right["table"].as_str())
            .then_with(|| left["column"].as_str().cmp(&right["column"].as_str()))
            .then_with(|| left["key"].as_str().cmp(&right["key"].as_str()))
    });
    match format {
        OutputFormat::Text => Ok(lines(
            &columns
                .iter()
                .map(|column| {
                    format!(
                        "{}.{}",
                        column["table"].as_str().unwrap_or_default(),
                        column["column"].as_str().unwrap_or_default()
                    )
                })
                .collect::<Vec<_>>(),
        )),
        OutputFormat::Json => Ok(json_line(json!({ "columns": columns }))),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_impact_analysis(
    store: &GraphStore,
    snapshot_key: &str,
    object_key: Option<&str>,
    table_name: Option<&str>,
    column_name: Option<&str>,
    direction: Direction,
    max_depth_requested: u32,
    result_limit_requested: usize,
) -> Result<String, String> {
    let object_key =
        resolve_impact_object_key(store, snapshot_key, object_key, table_name, column_name)?;
    let max_depth = max_depth_requested.min(MAX_TRAVERSAL_DEPTH);
    let result_limit = result_limit_requested.min(MAX_RESULT_LIMIT);
    let bounded = impact_analysis_bounded(
        store,
        snapshot_key,
        &object_key,
        direction,
        max_depth,
        result_limit,
    )
    .map_err(|err| err.to_string())?;
    let result_count = bounded
        .result
        .groups
        .iter()
        .map(|group| group.nodes.len())
        .sum::<usize>();

    Ok(json_line(json!({
        "contract_version": PRODUCT_CONTRACT_VERSION,
        "snapshot_key": snapshot_key,
        "object_key": object_key,
        "direction": direction_name(direction),
        "max_depth_requested": max_depth_requested,
        "max_depth_applied": max_depth,
        "max_depth_clamped": max_depth_requested != max_depth,
        "result_limit_requested": result_limit_requested,
        "result_limit_applied": result_limit,
        "result_limit_clamped": result_limit_requested != result_limit,
        "result_count": result_count,
        "truncated": bounded.truncated,
        "groups": impact_groups_json(&bounded.result),
        "capability_warnings": snapshot_capability_warnings(store, snapshot_key)?,
    })))
}

pub(crate) fn render_relationship_trace(
    store: &GraphStore,
    snapshot_key: &str,
    object_key: &str,
    direction: Direction,
    max_depth_requested: u32,
    result_limit_requested: usize,
) -> Result<String, String> {
    required_node(store, snapshot_key, object_key)?;
    let max_depth = max_depth_requested.min(MAX_TRAVERSAL_DEPTH);
    let result_limit = result_limit_requested.min(MAX_RESULT_LIMIT);
    let bounded = trace_relationships_bounded(
        store,
        snapshot_key,
        object_key,
        direction,
        max_depth,
        result_limit,
    )
    .map_err(|err| err.to_string())?;

    Ok(json_line(json!({
        "contract_version": PRODUCT_CONTRACT_VERSION,
        "snapshot_key": snapshot_key,
        "start_object_key": object_key,
        "direction": direction_name(direction),
        "max_depth_requested": max_depth_requested,
        "max_depth_applied": max_depth,
        "max_depth_clamped": max_depth_requested != max_depth,
        "result_limit_requested": result_limit_requested,
        "result_limit_applied": result_limit,
        "result_limit_clamped": result_limit_requested != result_limit,
        "result_count": bounded.paths.len(),
        "truncated": bounded.truncated,
        "paths": relationship_paths_json(&bounded.paths),
        "capability_warnings": snapshot_capability_warnings(store, snapshot_key)?,
    })))
}

fn resolve_impact_object_key(
    store: &GraphStore,
    snapshot_key: &str,
    object_key: Option<&str>,
    table_name: Option<&str>,
    column_name: Option<&str>,
) -> Result<String, String> {
    if let Some(object_key) = object_key {
        required_node(store, snapshot_key, object_key)?;
        return Ok(object_key.to_owned());
    }

    let table_name = table_name.ok_or("pass --object-key or --table")?;
    let table = resolve_table_node(store, snapshot_key, None, Some(table_name))?;
    let Some(column_name) = column_name else {
        return Ok(table.node_key);
    };

    table_columns(store, snapshot_key, &table.node_key)?
        .into_iter()
        .find(|column| column.name == column_name)
        .map(|column| column.key.to_string())
        .ok_or_else(|| format!("column '{column_name}' not found on table '{table_name}'"))
}

fn direction_name(direction: Direction) -> &'static str {
    match direction {
        Direction::Inbound => "inbound",
        Direction::Outbound => "outbound",
        Direction::Both => "both",
    }
}

fn impact_groups_json(result: &ImpactAnalysisResult) -> Vec<serde_json::Value> {
    result
        .groups
        .iter()
        .map(|group| {
            json!({
                "label": &group.label,
                "depth": group.depth,
                "nodes": group.nodes.iter().map(|node| json!({
                    "node_key": &node.node_key,
                    "label": &node.label,
                    "display_name": &node.display_name,
                    "depth": node.depth,
                    "edge_type": &node.edge_type_used,
                    "edge_from": &node.edge_from,
                    "edge_to": &node.edge_to,
                })).collect::<Vec<_>>(),
            })
        })
        .collect()
}

fn relationship_paths_json(paths: &[GraphPath]) -> Vec<serde_json::Value> {
    paths
        .iter()
        .map(|path| {
            json!({
                "depth": path.hops.len().saturating_sub(1),
                "hops": path.hops.iter().enumerate().map(|(depth, hop)| json!({
                    "node_key": &hop.node_key,
                    "label": &hop.label,
                    "depth": depth,
                    "edge_type": &hop.edge_type_used,
                    "edge_from": &hop.edge_from,
                    "edge_to": &hop.edge_to,
                })).collect::<Vec<_>>(),
            })
        })
        .collect()
}

fn lines(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("{value}\n"))
        .collect::<String>()
}

fn json_line(value: serde_json::Value) -> String {
    format!("{}\n", serde_json::to_string_pretty(&value).unwrap())
}

fn required_node(
    store: &GraphStore,
    snapshot_key: &str,
    node_key: &str,
) -> Result<GraphNodeRecord, String> {
    store
        .get_node(snapshot_key, node_key)
        .map_err(|err| err.to_string())?
        .ok_or_else(|| format!("graph node '{node_key}' not found"))
}

fn object_key(node: &GraphNodeRecord) -> Result<ObjectKey, String> {
    node.node_key
        .parse()
        .map_err(|err| format!("invalid graph node key '{}': {err}", node.node_key))
}

fn column_from_node(node: &GraphNodeRecord) -> Result<ColumnObject, String> {
    serde_json::from_str(&node.payload_json).map_err(|_| old_cache_error(node))
}

fn constraint_from_node(node: &GraphNodeRecord) -> Result<ConstraintObject, String> {
    serde_json::from_str(&node.payload_json).map_err(|_| old_cache_error(node))
}

fn foreign_key_from_node(node: &GraphNodeRecord) -> Result<ConstraintObject, String> {
    let constraint = constraint_from_node(node)?;
    if constraint.kind == ConstraintKind::ForeignKey {
        Ok(constraint)
    } else {
        Err(format!(
            "graph node '{}' is not a foreign key",
            node.node_key
        ))
    }
}

fn index_from_node(node: &GraphNodeRecord) -> Result<IndexObject, String> {
    serde_json::from_str(&node.payload_json).map_err(|_| old_cache_error(node))
}

fn old_cache_error(node: &GraphNodeRecord) -> String {
    format!(
        "graph node '{}' is missing metadata payload; re-run index for this alias",
        node.node_key
    )
}

fn foreign_key_description(constraint: &ConstraintObject) -> ForeignKeyDescription {
    ForeignKeyDescription {
        key: constraint.key.to_string(),
        table_key: constraint.table_key.to_string(),
        name: constraint.name.clone(),
        table: constraint.table_key.object_name.clone(),
        columns: names_from_keys(&constraint.columns),
        column_keys: keys_as_strings(&constraint.columns),
        referenced_table_key: constraint
            .referenced_table_key
            .as_ref()
            .map(ToString::to_string),
        referenced_table: constraint
            .referenced_table_key
            .as_ref()
            .map(|key| key.object_name.clone())
            .unwrap_or_default(),
        referenced_columns: names_from_keys(&constraint.referenced_columns),
        referenced_column_keys: keys_as_strings(&constraint.referenced_columns),
    }
}

fn names_from_keys(keys: &[ObjectKey]) -> Vec<String> {
    keys.iter()
        .map(|key| {
            key.sub_object
                .clone()
                .unwrap_or_else(|| key.object_name.clone())
        })
        .collect()
}

fn keys_as_strings(keys: &[ObjectKey]) -> Vec<String> {
    keys.iter().map(ToString::to_string).collect()
}

fn list_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "(none)".to_owned()
    } else {
        values.join(", ")
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}
