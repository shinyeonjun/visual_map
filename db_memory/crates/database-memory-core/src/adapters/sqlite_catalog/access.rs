impl AccessorFilter {
    fn matches(&self, accessor: Option<&str>) -> bool {
        match self {
            Self::Any => true,
            Self::Exact(expected) => accessor.is_some_and(|value| same_identifier(value, expected)),
        }
    }
}

#[derive(Clone, Debug)]
struct RawAccess {
    relation_name: String,
    column_name: Option<String>,
}

fn capture_prepare_accesses(
    conn: &Connection,
    sql: &str,
    filter: AccessorFilter,
) -> Result<Vec<RawAccess>, SqliteAdapterError> {
    let accesses = Arc::new(Mutex::new(Vec::<RawAccess>::new()));
    let captured = Arc::clone(&accesses);
    conn.authorizer(Some(move |context: AuthContext<'_>| {
        if !filter.matches(context.accessor) {
            return Authorization::Allow;
        }
        let access = match context.action {
            AuthAction::Read {
                table_name,
                column_name,
            }
            | AuthAction::Update {
                table_name,
                column_name,
            } => Some(RawAccess {
                relation_name: table_name.to_owned(),
                column_name: (!column_name.is_empty()).then(|| column_name.to_owned()),
            }),
            AuthAction::Insert { table_name } | AuthAction::Delete { table_name } => {
                Some(RawAccess {
                    relation_name: table_name.to_owned(),
                    column_name: None,
                })
            }
            _ => None,
        };
        if let Some(access) = access {
            if let Ok(mut values) = captured.lock() {
                values.push(access);
            }
        }
        Authorization::Allow
    }));
    let prepare_result = conn.prepare(sql).map(|_| ());
    conn.authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
    prepare_result.map_err(SqliteAdapterError::from)?;
    let values = accesses.lock().map_err(|_| {
        SqliteAdapterError::mapping(
            "SQLite dependency authorizer",
            "dependency capture lock was poisoned",
        )
    })?;
    Ok(values.clone())
}

fn map_accesses(
    accesses: Vec<RawAccess>,
    table_keys: &BTreeMap<String, ObjectKey>,
    view_keys: &BTreeMap<String, ObjectKey>,
    column_keys: &BTreeMap<(String, String), ObjectKey>,
) -> Result<Vec<ObjectKey>, SqliteAdapterError> {
    let mut keys = Vec::new();
    for access in accesses {
        let relation_folded = fold_identifier(&access.relation_name);
        let relation_key = table_keys
            .get(&relation_folded)
            .or_else(|| view_keys.get(&relation_folded))
            .cloned();
        let Some(relation_key) = relation_key else {
            if access.relation_name.starts_with("sqlite_") {
                continue;
            }
            return Err(SqliteAdapterError::mapping(
                format!("dependency relation {}", access.relation_name),
                "SQLite authorizer reported a relation outside the selected catalog inventory",
            ));
        };
        keys.push(relation_key);
        if let Some(column_name) = access.column_name {
            if is_rowid_alias(&column_name) {
                continue;
            }
            let column_key = column_keys
                .get(&(relation_folded, fold_identifier(&column_name)))
                .cloned()
                .ok_or_else(|| {
                    SqliteAdapterError::mapping(
                        format!("dependency column {}.{column_name}", access.relation_name),
                        "SQLite authorizer reported a column absent from table_xinfo",
                    )
                })?;
            keys.push(column_key);
        }
    }
    deduplicate_keys(&mut keys);
    Ok(keys)
}

fn is_rowid_alias(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "rowid" | "oid" | "_rowid_"
    )
}

fn deduplicate_keys(keys: &mut Vec<ObjectKey>) {
    let mut seen = BTreeSet::new();
    keys.retain(|key| seen.insert(key.to_string()));
    keys.sort_by_key(ObjectKey::to_string);
}

fn deduplicate_metadata_relationships(relationships: &mut Vec<MetadataRelationship>) {
    let mut seen = BTreeSet::new();
    relationships.retain(|relationship| {
        seen.insert((
            relationship.kind.clone(),
            relationship.from_key.to_string(),
            relationship.to_key.to_string(),
            relationship.ordinal,
        ))
    });
}

fn discovery_counts(
    raw: &RawSqliteCatalog,
    check_dependency_count: u64,
    view_dependency_count: u64,
    metadata_relationship_count: u64,
) -> DiscoveryCounts {
    let tables = raw
        .relations
        .iter()
        .filter(|relation| relation.kind.is_table())
        .count() as u64;
    let views = raw
        .relations
        .iter()
        .filter(|relation| relation.kind == RawRelationKind::View)
        .count() as u64;
    let columns = raw
        .relations
        .iter()
        .map(|relation| relation.columns.len() as u64)
        .sum::<u64>();
    let table_columns = raw
        .relations
        .iter()
        .filter(|relation| relation.kind.is_table())
        .map(|relation| relation.columns.len() as u64)
        .sum::<u64>();
    let view_columns = columns.saturating_sub(table_columns);
    let primary_keys = raw
        .relations
        .iter()
        .filter(|relation| {
            relation.kind.is_table()
                && relation
                    .columns
                    .iter()
                    .any(|column| column.primary_key_position > 0)
        })
        .count() as u64;
    let primary_key_columns = raw
        .relations
        .iter()
        .filter(|relation| relation.kind.is_table())
        .flat_map(|relation| relation.columns.iter())
        .filter(|column| column.primary_key_position > 0)
        .count() as u64;
    let foreign_keys = raw
        .foreign_keys
        .values()
        .map(|foreign_keys| foreign_keys.len() as u64)
        .sum::<u64>();
    let foreign_key_pairs = raw
        .foreign_keys
        .values()
        .flatten()
        .map(|foreign_key| foreign_key.parts.len() as u64)
        .sum::<u64>();
    let unique_constraints = raw
        .indexes
        .iter()
        .filter(|index| index.origin == "u")
        .count() as u64;
    let unique_columns = raw
        .indexes
        .iter()
        .filter(|index| index.origin == "u")
        .flat_map(|index| index.terms.iter())
        .filter(|term| term.key)
        .count() as u64;
    let check_constraints = raw
        .relations
        .iter()
        .filter_map(|relation| relation.parsed_table.as_ref())
        .flat_map(|table| table.constraints.iter())
        .filter(|constraint| constraint.kind == ParsedConstraintKind::Check)
        .count() as u64;
    let direct_index_columns = raw
        .indexes
        .iter()
        .flat_map(|index| index.terms.iter())
        .filter(|term| term.key && term.cid >= 0)
        .count() as u64;

    let mut counts = DiscoveryCounts {
        objects: ObjectCategory::ALL
            .into_iter()
            .map(|category| {
                (
                    category,
                    DiscoveredCount {
                        count: 0,
                        evidence:
                            "SQLite selected schema has no persisted object in this vendor category"
                                .to_owned(),
                    },
                )
            })
            .collect(),
        relationships: RelationshipCategory::ALL
            .into_iter()
            .map(|category| {
                (
                    category,
                    DiscoveredCount {
                        count: 0,
                        evidence:
                            "SQLite selected schema has no relationship in this vendor category"
                                .to_owned(),
                    },
                )
            })
            .collect(),
    };
    set_object_count(
        &mut counts,
        ObjectCategory::Database,
        raw.database_names.len() as u64,
        "PRAGMA database_list within the certified main-only scope",
    );
    set_object_count(
        &mut counts,
        ObjectCategory::Schema,
        1,
        "SQLite main catalog maps to one canonical main schema",
    );
    set_object_count(
        &mut counts,
        ObjectCategory::Table,
        tables,
        "PRAGMA main.table_list table/virtual/shadow rows excluding sqlite_*",
    );
    set_object_count(
        &mut counts,
        ObjectCategory::Column,
        table_columns,
        "Sum of PRAGMA main.table_xinfo rows for selected tables",
    );
    set_object_count(
        &mut counts,
        ObjectCategory::ViewColumn,
        view_columns,
        "Sum of PRAGMA main.table_xinfo rows for selected views",
    );
    set_object_count(
        &mut counts,
        ObjectCategory::PrimaryKey,
        primary_keys,
        "Distinct selected tables with table_xinfo pk ordinals",
    );
    set_object_count(
        &mut counts,
        ObjectCategory::ForeignKey,
        foreign_keys,
        "Distinct ids returned by PRAGMA main.foreign_key_list per table",
    );
    set_object_count(
        &mut counts,
        ObjectCategory::UniqueConstraint,
        unique_constraints,
        "PRAGMA main.index_list rows with origin='u'",
    );
    set_object_count(
        &mut counts,
        ObjectCategory::CheckConstraint,
        check_constraints,
        "CHECK clauses parsed from every persisted CREATE TABLE definition",
    );
    set_object_count(
        &mut counts,
        ObjectCategory::Index,
        raw.indexes.len() as u64,
        "All PRAGMA main.index_list rows for selected tables",
    );
    set_object_count(
        &mut counts,
        ObjectCategory::View,
        views,
        "PRAGMA main.table_list rows with type='view' excluding sqlite_*",
    );
    set_object_count(
        &mut counts,
        ObjectCategory::Trigger,
        raw.triggers.len() as u64,
        "sqlite_schema trigger rows excluding sqlite_*",
    );

    set_relationship_count(
        &mut counts,
        RelationshipCategory::DatabaseHasSchema,
        1,
        "Canonical SQLite main catalog-to-schema mapping",
    );
    set_relationship_count(
        &mut counts,
        RelationshipCategory::SchemaHasTable,
        tables,
        "Selected PRAGMA table_list table/virtual/shadow rows",
    );
    set_relationship_count(
        &mut counts,
        RelationshipCategory::TableHasColumn,
        table_columns,
        "PRAGMA table_xinfo rows whose owner is a selected table",
    );
    set_relationship_count(
        &mut counts,
        RelationshipCategory::TableHasConstraint,
        primary_keys + foreign_keys + unique_constraints + check_constraints,
        "Reconciled table_xinfo, foreign_key_list, index_list origin, and parsed CHECK inventory",
    );
    set_relationship_count(
        &mut counts,
        RelationshipCategory::ConstraintColumn,
        primary_key_columns + unique_columns + check_dependency_count,
        "PK/UQ catalog ordinals plus prepare-authorizer CHECK column reads",
    );
    set_relationship_count(
        &mut counts,
        RelationshipCategory::ForeignKeyColumnPair,
        foreign_key_pairs,
        "Rows returned by PRAGMA foreign_key_list",
    );
    set_relationship_count(
        &mut counts,
        RelationshipCategory::TableHasIndex,
        raw.indexes.len() as u64,
        "PRAGMA index_list rows attached to selected tables",
    );
    set_relationship_count(
        &mut counts,
        RelationshipCategory::IndexColumn,
        direct_index_columns,
        "PRAGMA index_xinfo key terms with non-negative column ids",
    );
    set_relationship_count(
        &mut counts,
        RelationshipCategory::SchemaHasView,
        views,
        "Selected PRAGMA table_list view rows",
    );
    set_relationship_count(
        &mut counts,
        RelationshipCategory::ViewDependency,
        view_dependency_count,
        "SQLite prepare authorizer reads while selecting every output column from each view",
    );
    set_relationship_count(
        &mut counts,
        RelationshipCategory::TriggerTarget,
        raw.triggers.len() as u64,
        "sqlite_schema trigger tbl_name ownership",
    );
    set_relationship_count(
        &mut counts,
        RelationshipCategory::MetadataParent,
        view_columns,
        "PRAGMA table_xinfo output columns parented by selected views",
    );
    set_relationship_count(
        &mut counts,
        RelationshipCategory::MetadataRelationship,
        metadata_relationship_count,
        "Deduplicated SQLite authorizer expression/trigger dependencies and index auxiliary terms",
    );
    counts
}

