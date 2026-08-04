pub fn graph_stats_for_cache_path(cache_path: impl AsRef<Path>) -> GraphStatsResult {
    let path = cache_path.as_ref();
    let cache_path = path.display().to_string();

    if !path.exists() {
        return GraphStatsResult {
            cache_path,
            cache_exists: false,
            indexed_snapshots: 0,
            error: None,
        };
    }

    match GraphStore::open(path).and_then(|store| store.snapshot_count()) {
        Ok(indexed_snapshots) => GraphStatsResult {
            cache_path,
            cache_exists: true,
            indexed_snapshots,
            error: None,
        },
        Err(err) => GraphStatsResult {
            cache_path,
            cache_exists: true,
            indexed_snapshots: 0,
            error: Some(err.to_string()),
        },
    }
}

fn describe_table(
    store: &GraphStore,
    snapshot_key: &str,
    table_name: &str,
) -> Result<TableDescription, String> {
    let table = find_table_node(store, snapshot_key, table_name)?;
    let table_identity = object_key(&table)?;
    let columns = table_columns(store, snapshot_key, &table.node_key)?;
    let constraints = table_constraints(store, snapshot_key, &table.node_key)?;
    let primary_key_columns = constraints
        .iter()
        .find(|constraint| constraint.kind == ConstraintKind::PrimaryKey)
        .map(|constraint| constraint.columns.as_slice())
        .unwrap_or_default();
    let primary_key = names_from_keys(primary_key_columns);
    let primary_key_keys = string_keys(primary_key_columns);
    let mut outbound = constraints
        .iter()
        .filter(|constraint| constraint.kind == ConstraintKind::ForeignKey)
        .map(foreign_key_description)
        .collect::<Vec<_>>();
    outbound.sort_by(|left, right| left.name.cmp(&right.name));

    let mut inbound_keys = BTreeSet::new();
    for column in &columns {
        for edge in store
            .edges_to(snapshot_key, &column.key.to_string())
            .map_err(|err| err.to_string())?
        {
            if edge.edge_type == "FK_TO_COLUMN" {
                inbound_keys.insert(edge.edge_from);
            }
        }
    }
    let mut inbound = Vec::new();
    for key in inbound_keys {
        let node = required_node(store, snapshot_key, &key)?;
        inbound.push(foreign_key_description(&foreign_key_from_node(&node)?));
    }
    inbound.sort_by(|left, right| left.name.cmp(&right.name));

    Ok(TableDescription {
        snapshot_key: snapshot_key.to_owned(),
        object_key: table.node_key.clone(),
        database: table_identity.database,
        schema: table_identity.schema,
        table: table_identity.object_name,
        columns: columns
            .into_iter()
            .map(|column| ColumnDescription {
                object_key: column.key.to_string(),
                name: column.name,
                ordinal_position: column.ordinal_position,
                data_type: column.data_type,
                nullable: column.is_nullable,
                default_value: column.default_value,
                generated: column.is_generated,
            })
            .collect(),
        primary_key,
        primary_key_keys,
        constraints: constraints.iter().map(constraint_description).collect(),
        foreign_keys: ForeignKeysDescription { outbound, inbound },
        indexes: table_indexes(store, snapshot_key, &table.node_key)?
            .into_iter()
            .map(|index| IndexDescription {
                object_key: index.key.to_string(),
                name: index.name,
                column_keys: string_keys(&index.columns),
                columns: names_from_keys(&index.columns),
                unique: index.is_unique,
                primary: index.is_primary,
                predicate: index.predicate,
                expression: index.expression,
            })
            .collect(),
        capability_warnings: snapshot_capability_warnings(store, snapshot_key)?,
    })
}

fn find_tables(
    store: &GraphStore,
    snapshot_key: &str,
    filter: Option<&str>,
) -> Result<Vec<TableMatch>, String> {
    let needle = filter.map(str::to_lowercase);
    let mut tables = Vec::new();
    for node in store
        .nodes_by_label(snapshot_key, "Table")
        .map_err(|err| err.to_string())?
    {
        let key = object_key(&node)?;
        if needle
            .as_ref()
            .map(|needle| key.object_name.to_lowercase().contains(needle))
            .unwrap_or(true)
        {
            tables.push(TableMatch {
                object_key: node.node_key,
                database: key.database,
                schema: key.schema,
                table: key.object_name,
            });
        }
    }
    tables.sort_by(|left, right| {
        left.database
            .cmp(&right.database)
            .then_with(|| left.schema.cmp(&right.schema))
            .then_with(|| left.table.cmp(&right.table))
            .then_with(|| left.object_key.cmp(&right.object_key))
    });
    Ok(tables)
}

fn find_columns(
    store: &GraphStore,
    snapshot_key: &str,
    query: &str,
) -> Result<Vec<ColumnMatch>, String> {
    let needle = query.to_lowercase();
    let mut columns = Vec::new();
    for node in store
        .nodes_by_label(snapshot_key, "Column")
        .map_err(|err| err.to_string())?
    {
        let key = object_key(&node)?;
        let column = key
            .sub_object
            .clone()
            .unwrap_or_else(|| key.object_name.clone());
        if column.to_lowercase().contains(&needle) {
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
            columns.push(ColumnMatch {
                object_key: node.node_key,
                table_key,
                database: key.database,
                schema: key.schema,
                table: key.object_name,
                column,
            });
        }
    }
    columns.sort_by(|left, right| {
        left.database
            .cmp(&right.database)
            .then_with(|| left.schema.cmp(&right.schema))
            .then_with(|| left.table.cmp(&right.table))
            .then_with(|| left.column.cmp(&right.column))
            .then_with(|| left.object_key.cmp(&right.object_key))
    });
    Ok(columns)
}

fn find_table_node(
    store: &GraphStore,
    snapshot_key: &str,
    table_name: &str,
) -> Result<GraphNodeRecord, String> {
    if let Some(node) = store
        .get_node(snapshot_key, table_name)
        .map_err(|err| err.to_string())?
    {
        if node.label == "Table" {
            return Ok(node);
        }
        return Err(format!("graph node '{table_name}' is not a table"));
    }

    let mut matches = store
        .nodes_by_label(snapshot_key, "Table")
        .map_err(|err| err.to_string())?
        .into_iter()
        .filter_map(|node| match object_key(&node) {
            Ok(key) if key.object_name == table_name => Some(Ok(node)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<Vec<_>, _>>()?;
    matches.sort_by(|left, right| left.node_key.cmp(&right.node_key));

    match matches.len() {
        0 => Err(format!(
            "table '{table_name}' not found in snapshot '{snapshot_key}'"
        )),
        1 => Ok(matches.remove(0)),
        _ => Err(format!(
            "table '{table_name}' is ambiguous in snapshot '{snapshot_key}'; pass one object key: {}",
            matches
                .iter()
                .map(|node| node.node_key.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn table_columns(
    store: &GraphStore,
    snapshot_key: &str,
    table_key: &str,
) -> Result<Vec<ColumnObject>, String> {
    let mut columns = Vec::new();
    for edge in store
        .edges_from(snapshot_key, table_key)
        .map_err(|err| err.to_string())?
    {
        if edge.edge_type == "TABLE_HAS_COLUMN" {
            let node = required_node(store, snapshot_key, &edge.edge_to)?;
            columns.push(column_from_node(&node)?);
        }
    }
    columns.sort_by_key(|column| column.ordinal_position);
    Ok(columns)
}

fn table_constraints(
    store: &GraphStore,
    snapshot_key: &str,
    table_key: &str,
) -> Result<Vec<ConstraintObject>, String> {
    let mut constraints = Vec::new();
    for edge in store
        .edges_from(snapshot_key, table_key)
        .map_err(|err| err.to_string())?
    {
        if edge.edge_type == "TABLE_HAS_CONSTRAINT" {
            let node = required_node(store, snapshot_key, &edge.edge_to)?;
            constraints.push(constraint_from_node(&node)?);
        }
    }
    Ok(constraints)
}

fn table_indexes(
    store: &GraphStore,
    snapshot_key: &str,
    table_key: &str,
) -> Result<Vec<IndexObject>, String> {
    let mut indexes = Vec::new();
    for edge in store
        .edges_from(snapshot_key, table_key)
        .map_err(|err| err.to_string())?
    {
        if edge.edge_type == "TABLE_HAS_INDEX" {
            let node = required_node(store, snapshot_key, &edge.edge_to)?;
            indexes.push(index_from_node(&node)?);
        }
    }
    indexes.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(indexes)
}

fn resolve_object_key(
    store: &GraphStore,
    snapshot_key: &str,
    object_key: Option<&str>,
    table: Option<&str>,
    column: Option<&str>,
) -> Result<String, String> {
    if let Some(object_key) = object_key {
        required_node(store, snapshot_key, object_key)?;
        return Ok(object_key.to_owned());
    }

    match (table, column) {
        (Some(table), Some(column)) => {
            let table_name = table;
            let table = find_table_node(store, snapshot_key, table_name)?;
            for column_object in table_columns(store, snapshot_key, &table.node_key)? {
                if column_object.name == column {
                    return Ok(column_object.key.to_string());
                }
            }
            Err(format!(
                "column '{column}' not found on table '{table_name}'"
            ))
        }
        (Some(table), None) => Ok(find_table_node(store, snapshot_key, table)?.node_key),
        (None, Some(column)) => {
            let matches = find_columns(store, snapshot_key, column)?
                .into_iter()
                .filter(|item| item.column == column)
                .collect::<Vec<_>>();
            match matches.as_slice() {
                [item] => Ok(item.object_key.clone()),
                [] => Err(format!(
                    "column '{column}' not found in snapshot '{snapshot_key}'"
                )),
                _ => Err(format!(
                    "column '{column}' is ambiguous; pass table and column together"
                )),
            }
        }
        (None, None) => Err("pass object_key, table, or table plus column".to_owned()),
    }
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
        object_key: constraint.key.to_string(),
        name: constraint.name.clone(),
        table_key: constraint.table_key.to_string(),
        table: constraint.table_key.object_name.clone(),
        column_keys: string_keys(&constraint.columns),
        columns: names_from_keys(&constraint.columns),
        referenced_table_key: constraint
            .referenced_table_key
            .as_ref()
            .map(ToString::to_string),
        referenced_table: constraint
            .referenced_table_key
            .as_ref()
            .map(|key| key.object_name.clone())
            .unwrap_or_default(),
        referenced_column_keys: string_keys(&constraint.referenced_columns),
        referenced_columns: names_from_keys(&constraint.referenced_columns),
    }
}

fn constraint_description(constraint: &ConstraintObject) -> ConstraintDescription {
    ConstraintDescription {
        object_key: constraint.key.to_string(),
        name: constraint.name.clone(),
        kind: constraint_kind_name(constraint.kind).to_owned(),
        column_keys: string_keys(&constraint.columns),
        columns: names_from_keys(&constraint.columns),
        referenced_table_key: constraint
            .referenced_table_key
            .as_ref()
            .map(ToString::to_string),
        referenced_column_keys: string_keys(&constraint.referenced_columns),
        expression: constraint.expression.clone(),
    }
}

fn constraint_kind_name(kind: ConstraintKind) -> &'static str {
    match kind {
        ConstraintKind::PrimaryKey => "primary_key",
        ConstraintKind::ForeignKey => "foreign_key",
        ConstraintKind::Unique => "unique",
        ConstraintKind::Check => "check",
    }
}

fn string_keys(keys: &[ObjectKey]) -> Vec<String> {
    keys.iter().map(ToString::to_string).collect()
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

