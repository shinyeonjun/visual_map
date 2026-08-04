pub(crate) fn describe_table(
    store: &GraphStore,
    snapshot_key: &str,
    table_object_key: Option<&str>,
    table_name: Option<&str>,
) -> Result<TableDescription, String> {
    let table = resolve_table_node(store, snapshot_key, table_object_key, table_name)?;
    let table_key = object_key(&table)?;
    let columns = table_columns(store, snapshot_key, &table.node_key)?;
    let constraints = table_constraints(store, snapshot_key, &table.node_key)?;
    let primary_key = constraints
        .iter()
        .find(|constraint| constraint.kind == ConstraintKind::PrimaryKey)
        .map(|constraint| names_from_keys(&constraint.columns))
        .unwrap_or_default();
    let mut outbound_foreign_keys = constraints
        .iter()
        .filter(|constraint| constraint.kind == ConstraintKind::ForeignKey)
        .map(foreign_key_description)
        .collect::<Vec<_>>();
    outbound_foreign_keys.sort_by(|left, right| left.name.cmp(&right.name));

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
    let mut inbound_foreign_keys = Vec::new();
    for key in inbound_keys {
        let node = required_node(store, snapshot_key, &key)?;
        inbound_foreign_keys.push(foreign_key_description(&foreign_key_from_node(&node)?));
    }
    inbound_foreign_keys.sort_by(|left, right| left.name.cmp(&right.name));
    let dependents = table_dependents(store, snapshot_key, &table.node_key, &columns)?;

    Ok(TableDescription {
        snapshot_key: snapshot_key.to_owned(),
        table_key: table.node_key.clone(),
        table_name: table_key.object_name,
        columns,
        primary_key,
        constraints,
        outbound_foreign_keys,
        inbound_foreign_keys,
        indexes: table_indexes(store, snapshot_key, &table.node_key)?,
        dependents,
        capability_warnings: snapshot_capability_warnings(store, snapshot_key)?,
    })
}

fn table_dependents(
    store: &GraphStore,
    snapshot_key: &str,
    table_key: &str,
    columns: &[ColumnObject],
) -> Result<Vec<DependentObjectDescription>, String> {
    let mut dependents = BTreeMap::<String, DependentObjectDescription>::new();
    for edge in store
        .edges_to(snapshot_key, table_key)
        .map_err(|error| error.to_string())?
    {
        if matches!(
            edge.edge_type.as_str(),
            "VIEW_DEPENDS_ON_TABLE" | "ROUTINE_DEPENDS_ON_TABLE"
        ) {
            let node = required_node(store, snapshot_key, &edge.edge_from)?;
            merge_dependent(
                &mut dependents,
                dependent_from_node(&node, &edge.edge_type)?,
                None,
            );
        }
    }
    for column in columns {
        for edge in store
            .edges_to(snapshot_key, &column.key.to_string())
            .map_err(|error| error.to_string())?
        {
            if matches!(
                edge.edge_type.as_str(),
                "VIEW_DEPENDS_ON_COLUMN" | "ROUTINE_DEPENDS_ON_COLUMN"
            ) {
                let node = required_node(store, snapshot_key, &edge.edge_from)?;
                merge_dependent(
                    &mut dependents,
                    dependent_from_node(&node, &edge.edge_type)?,
                    Some(column.key.to_string()),
                );
            }
        }
    }
    for edge in store
        .edges_from(snapshot_key, table_key)
        .map_err(|error| error.to_string())?
    {
        if edge.edge_type == "TABLE_HAS_TRIGGER" {
            let node = required_node(store, snapshot_key, &edge.edge_to)?;
            merge_dependent(
                &mut dependents,
                dependent_from_node(&node, &edge.edge_type)?,
                None,
            );
        }
    }
    Ok(dependents.into_values().collect())
}

fn dependent_from_node(
    node: &GraphNodeRecord,
    relation: &str,
) -> Result<DependentObjectDescription, String> {
    let key = object_key(node)?;
    let kind = match key.object_kind {
        ObjectKind::View => "view",
        ObjectKind::Trigger => "trigger",
        ObjectKind::Routine => "routine",
        _ => {
            return Err(format!(
                "graph node '{}' is not a DB dependent object",
                node.node_key
            ))
        }
    };
    let relation = match relation {
        "VIEW_DEPENDS_ON_TABLE" | "VIEW_DEPENDS_ON_COLUMN" => "view_depends_on",
        "ROUTINE_DEPENDS_ON_TABLE" | "ROUTINE_DEPENDS_ON_COLUMN" => "routine_depends_on",
        "TABLE_HAS_TRIGGER" => "table_has_trigger",
        _ => relation,
    };
    Ok(DependentObjectDescription {
        key: node.node_key.clone(),
        kind: kind.to_owned(),
        name: node
            .display_name
            .clone()
            .unwrap_or_else(|| key.object_name.clone()),
        relation: relation.to_owned(),
        column_keys: Vec::new(),
    })
}

fn merge_dependent(
    dependents: &mut BTreeMap<String, DependentObjectDescription>,
    dependent: DependentObjectDescription,
    column_key: Option<String>,
) {
    let entry = dependents.entry(dependent.key.clone()).or_insert(dependent);
    if let Some(column_key) = column_key {
        if !entry.column_keys.contains(&column_key) {
            entry.column_keys.push(column_key);
            entry.column_keys.sort();
        }
    }
}

fn find_table_node(
    store: &GraphStore,
    snapshot_key: &str,
    table_name: &str,
) -> Result<GraphNodeRecord, String> {
    let mut matches = store
        .nodes_by_label(snapshot_key, "Table")
        .map_err(|err| err.to_string())?
        .into_iter()
        .filter_map(|node| match object_key(&node) {
            Ok(key) if key.object_name == table_name => Some(Ok(node)),
            Ok(_) => None,
            Err(err) => Some(Err(err)),
        })
        .collect::<Result<Vec<_>, _>>()?;
    matches.sort_by(|left, right| left.node_key.cmp(&right.node_key));

    match matches.len() {
        0 => Err(format!(
            "table '{table_name}' not found in snapshot '{snapshot_key}'"
        )),
        1 => Ok(matches.remove(0)),
        _ => Err(format!(
            "table '{table_name}' is ambiguous in snapshot '{snapshot_key}'; use --object-key with one of: {}",
            matches
                .iter()
                .map(|node| node.node_key.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn resolve_table_node(
    store: &GraphStore,
    snapshot_key: &str,
    table_object_key: Option<&str>,
    table_name: Option<&str>,
) -> Result<GraphNodeRecord, String> {
    match (table_object_key, table_name) {
        (Some(object_key), None) => {
            let node = required_node(store, snapshot_key, object_key)?;
            let key = self::object_key(&node)?;
            if node.label != "Table" || key.object_kind != ObjectKind::Table {
                return Err(format!("graph node '{object_key}' is not a table"));
            }
            Ok(node)
        }
        (None, Some(table_name)) => find_table_node(store, snapshot_key, table_name),
        _ => Err("pass one table selector: a table name or --object-key".to_owned()),
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

pub(crate) fn render_table_description(
    description: &TableDescription,
    format: OutputFormat,
) -> String {
    match format {
        OutputFormat::Text => render_table_description_text(description),
        OutputFormat::Json => render_table_description_json(description),
    }
}

fn render_table_description_text(description: &TableDescription) -> String {
    let mut out = format!(
        "table: {}
",
        description.table_name
    );
    out.push_str(
        "columns:
",
    );
    for column in &description.columns {
        out.push_str(&format!(
            "  {} {} nullable: {}
",
            column.name,
            column.data_type,
            yes_no(column.is_nullable)
        ));
    }
    out.push_str(&format!(
        "primary key: {}
",
        list_or_none(&description.primary_key)
    ));
    out.push_str(
        "foreign keys:
  outbound:
",
    );
    push_foreign_keys(&mut out, &description.outbound_foreign_keys);
    out.push_str(
        "  inbound:
",
    );
    push_foreign_keys(&mut out, &description.inbound_foreign_keys);
    out.push_str(
        "indexes:
",
    );
    if description.indexes.is_empty() {
        out.push_str(
            "  (none)
",
        );
    } else {
        for index in &description.indexes {
            out.push_str(&format!(
                "  {}: {} unique: {} primary: {}
",
                index.name,
                list_or_none(&names_from_keys(&index.columns)),
                yes_no(index.is_unique),
                yes_no(index.is_primary)
            ));
        }
    }
    out.push_str("dependents:\n");
    if description.dependents.is_empty() {
        out.push_str("  (none)\n");
    } else {
        for dependent in &description.dependents {
            out.push_str(&format!(
                "  {} {} via {}\n",
                dependent.kind, dependent.name, dependent.relation
            ));
        }
    }
    if !description.capability_warnings.is_empty() {
        out.push_str(
            "capability warnings:
",
        );
        for warning in &description.capability_warnings {
            out.push_str(&format!(
                "  {warning}
"
            ));
        }
    }
    out
}

fn push_foreign_keys(out: &mut String, foreign_keys: &[ForeignKeyDescription]) {
    if foreign_keys.is_empty() {
        out.push_str(
            "    (none)
",
        );
    } else {
        for fk in foreign_keys {
            out.push_str(&format!(
                "    {}: {}({}) -> {}({})
",
                fk.name,
                fk.table,
                list_or_none(&fk.columns),
                fk.referenced_table,
                list_or_none(&fk.referenced_columns)
            ));
        }
    }
}

fn render_table_description_json(description: &TableDescription) -> String {
    json_line(table_description_json_value(description))
}

fn table_description_json_value(description: &TableDescription) -> serde_json::Value {
    json!({
        "contract_version": PRODUCT_CONTRACT_VERSION,
        "snapshot_key": &description.snapshot_key,
        "table_key": &description.table_key,
        "table": &description.table_name,
        "columns": description.columns.iter().map(|column| json!({
            "key": column.key.to_string(),
            "table_key": column.table_key.to_string(),
            "schema": &column.key.schema,
            "database": &column.key.database,
            "name": &column.name,
            "type": &column.data_type,
            "nullable": column.is_nullable,
        })).collect::<Vec<_>>(),
        "primary_key": &description.primary_key,
        "constraints": description.constraints.iter().map(|constraint| json!({
            "key": constraint.key.to_string(),
            "table_key": constraint.table_key.to_string(),
            "name": &constraint.name,
            "kind": constraint_kind_name(constraint.kind),
            "columns": names_from_keys(&constraint.columns),
            "column_keys": keys_as_strings(&constraint.columns),
            "referenced_table_key": constraint.referenced_table_key.as_ref().map(ToString::to_string),
            "referenced_columns": names_from_keys(&constraint.referenced_columns),
            "referenced_column_keys": keys_as_strings(&constraint.referenced_columns),
            "expression": &constraint.expression,
        })).collect::<Vec<_>>(),
        "foreign_keys": {
            "outbound": foreign_keys_json(&description.outbound_foreign_keys),
            "inbound": foreign_keys_json(&description.inbound_foreign_keys),
        },
        "indexes": description.indexes.iter().map(|index| json!({
            "key": index.key.to_string(),
            "table_key": index.table_key.to_string(),
            "name": &index.name,
            "columns": names_from_keys(&index.columns),
            "column_keys": keys_as_strings(&index.columns),
            "unique": index.is_unique,
            "primary": index.is_primary,
            "predicate": &index.predicate,
            "expression": &index.expression,
        })).collect::<Vec<_>>(),
        "dependents": description.dependents.iter().map(|dependent| json!({
            "key": &dependent.key,
            "kind": &dependent.kind,
            "name": &dependent.name,
            "relation": &dependent.relation,
            "column_keys": &dependent.column_keys,
        })).collect::<Vec<_>>(),
        "capability_warnings": &description.capability_warnings,
    })
}

