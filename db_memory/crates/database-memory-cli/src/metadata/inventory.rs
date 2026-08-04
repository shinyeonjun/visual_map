pub(crate) fn render_inventory(
    store: &GraphStore,
    snapshot_key: &str,
    offset: usize,
    limit_requested: usize,
) -> Result<String, String> {
    let record = store
        .get_snapshot(snapshot_key)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("snapshot '{snapshot_key}' not found in cache; run index first"))?;
    let snapshot = match store
        .get_certified_snapshot(snapshot_key)
        .map_err(|error| error.to_string())?
    {
        Some(certified) => certified.snapshot.schema,
        None => serde_json::from_str::<SchemaSnapshot>(&record.payload_json).map_err(|error| {
            format!("snapshot '{snapshot_key}' payload is incompatible; re-index it: {error}")
        })?,
    };
    let warnings = capability_warnings(&snapshot.capabilities);
    let index = InventoryDescriptionIndex::new(&snapshot);
    let mut table_entries = snapshot
        .tables
        .iter()
        .map(|table| (table.key.to_string(), table))
        .collect::<Vec<_>>();
    table_entries.sort_by(|left, right| left.0.cmp(&right.0));

    let total_tables = table_entries.len();
    let (limit_applied, limit_clamped) = inventory_bounds(limit_requested);
    let mut tables = Vec::with_capacity(total_tables.saturating_sub(offset).min(limit_applied));
    for (table_key, table) in table_entries.into_iter().skip(offset).take(limit_applied) {
        let description = index.describe(snapshot_key, table_key, table, &warnings);
        tables.push(table_description_json_value(&description));
    }
    let next_offset = offset.saturating_add(tables.len());
    let has_more = next_offset < total_tables;

    Ok(json_line(json!({
        "contract_version": PRODUCT_CONTRACT_VERSION,
        "snapshot_key": snapshot_key,
        "offset": offset,
        "limit_requested": limit_requested,
        "limit_applied": limit_applied,
        "limit_clamped": limit_clamped,
        "result_count": tables.len(),
        "total_tables": total_tables,
        "has_more": has_more,
        "next_offset": has_more.then_some(next_offset),
        "truncated": has_more,
        "capability_warnings": warnings,
        "tables": tables,
    })))
}

struct InventoryDescriptionIndex<'a> {
    columns: HashMap<String, Vec<&'a ColumnObject>>,
    constraints: HashMap<String, Vec<&'a ConstraintObject>>,
    inbound_foreign_keys: HashMap<String, Vec<&'a ConstraintObject>>,
    indexes: HashMap<String, Vec<&'a IndexObject>>,
    dependents: HashMap<String, BTreeMap<String, DependentObjectDescription>>,
}

impl<'a> InventoryDescriptionIndex<'a> {
    fn new(snapshot: &'a SchemaSnapshot) -> Self {
        let mut index = Self {
            columns: HashMap::new(),
            constraints: HashMap::new(),
            inbound_foreign_keys: HashMap::new(),
            indexes: HashMap::new(),
            dependents: HashMap::new(),
        };
        for column in &snapshot.columns {
            index
                .columns
                .entry(column.table_key.to_string())
                .or_default()
                .push(column);
        }
        for constraint in &snapshot.constraints {
            index
                .constraints
                .entry(constraint.table_key.to_string())
                .or_default()
                .push(constraint);
            if constraint.kind == ConstraintKind::ForeignKey {
                if let Some(referenced_table) = &constraint.referenced_table_key {
                    index
                        .inbound_foreign_keys
                        .entry(referenced_table.to_string())
                        .or_default()
                        .push(constraint);
                }
            }
        }
        for item in &snapshot.indexes {
            index
                .indexes
                .entry(item.table_key.to_string())
                .or_default()
                .push(item);
        }
        let column_tables = snapshot
            .columns
            .iter()
            .map(|column| (column.key.to_string(), column.table_key.to_string()))
            .collect::<HashMap<_, _>>();
        for view in &snapshot.views {
            index.record_dependencies(
                &view.key,
                &view.name,
                "view",
                "view_depends_on",
                &view.depends_on,
                &column_tables,
            );
        }
        for routine in &snapshot.routines {
            index.record_dependencies(
                &routine.key,
                &routine.name,
                "routine",
                "routine_depends_on",
                &routine.depends_on,
                &column_tables,
            );
        }
        for trigger in &snapshot.triggers {
            if trigger.table_key.object_kind == ObjectKind::Table {
                index.record_dependent(
                    trigger.table_key.to_string(),
                    &trigger.key,
                    &trigger.name,
                    "trigger",
                    "table_has_trigger",
                    None,
                );
            }
        }
        index
    }

    fn record_dependencies(
        &mut self,
        key: &ObjectKey,
        name: &str,
        kind: &str,
        relation: &str,
        dependencies: &[ObjectKey],
        column_tables: &HashMap<String, String>,
    ) {
        for dependency in dependencies {
            match dependency.object_kind {
                ObjectKind::Table => {
                    self.record_dependent(dependency.to_string(), key, name, kind, relation, None)
                }
                ObjectKind::Column => {
                    if let Some(table_key) = column_tables.get(&dependency.to_string()) {
                        self.record_dependent(
                            table_key.clone(),
                            key,
                            name,
                            kind,
                            relation,
                            Some(dependency.to_string()),
                        );
                    }
                }
                _ => {}
            }
        }
    }

    fn record_dependent(
        &mut self,
        table_key: String,
        key: &ObjectKey,
        name: &str,
        kind: &str,
        relation: &str,
        column_key: Option<String>,
    ) {
        merge_dependent(
            self.dependents.entry(table_key).or_default(),
            DependentObjectDescription {
                key: key.to_string(),
                kind: kind.to_owned(),
                name: name.to_owned(),
                relation: relation.to_owned(),
                column_keys: Vec::new(),
            },
            column_key,
        );
    }

    fn describe(
        &self,
        snapshot_key: &str,
        table_key: String,
        table: &TableObject,
        capability_warnings: &[String],
    ) -> TableDescription {
        let mut columns = self
            .columns
            .get(&table_key)
            .into_iter()
            .flatten()
            .map(|column| (*column).clone())
            .collect::<Vec<_>>();
        columns.sort_by_key(|column| column.ordinal_position);
        let mut constraints = self
            .constraints
            .get(&table_key)
            .into_iter()
            .flatten()
            .map(|constraint| (*constraint).clone())
            .collect::<Vec<_>>();
        constraints.sort_by_key(|constraint| constraint.key.to_string());
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
        let mut inbound_foreign_keys = self
            .inbound_foreign_keys
            .get(&table_key)
            .into_iter()
            .flatten()
            .map(|constraint| foreign_key_description(constraint))
            .collect::<Vec<_>>();
        inbound_foreign_keys.sort_by(|left, right| left.name.cmp(&right.name));
        let mut indexes = self
            .indexes
            .get(&table_key)
            .into_iter()
            .flatten()
            .map(|index| (*index).clone())
            .collect::<Vec<_>>();
        indexes.sort_by(|left, right| left.name.cmp(&right.name));
        let dependents = self
            .dependents
            .get(&table_key)
            .into_iter()
            .flat_map(|items| items.values().cloned())
            .collect();

        TableDescription {
            snapshot_key: snapshot_key.to_owned(),
            table_key,
            table_name: table.name.clone(),
            columns,
            primary_key,
            constraints,
            outbound_foreign_keys,
            inbound_foreign_keys,
            indexes,
            dependents,
            capability_warnings: capability_warnings.to_vec(),
        }
    }
}

fn inventory_bounds(limit_requested: usize) -> (usize, bool) {
    let limit_applied = limit_requested.min(MAX_INVENTORY_TABLES);
    (limit_applied, limit_requested != limit_applied)
}

fn constraint_kind_name(kind: ConstraintKind) -> &'static str {
    match kind {
        ConstraintKind::PrimaryKey => "primary_key",
        ConstraintKind::ForeignKey => "foreign_key",
        ConstraintKind::Unique => "unique",
        ConstraintKind::Check => "check",
    }
}

fn foreign_keys_json(foreign_keys: &[ForeignKeyDescription]) -> Vec<serde_json::Value> {
    foreign_keys
        .iter()
        .map(|fk| {
            json!({
                "key": &fk.key,
                "table_key": &fk.table_key,
                "name": &fk.name,
                "table": &fk.table,
                "columns": &fk.columns,
                "column_keys": &fk.column_keys,
                "referenced_table_key": &fk.referenced_table_key,
                "referenced_table": &fk.referenced_table,
                "referenced_columns": &fk.referenced_columns,
                "referenced_column_keys": &fk.referenced_column_keys,
            })
        })
        .collect()
}

