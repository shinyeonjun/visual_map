impl MysqlFamilySnapshotMapper {
    fn new(connection_alias: &str, strategy: MysqlFamilyVersion) -> Self {
        Self {
            connection_alias: connection_alias.to_owned(),
            strategy,
        }
    }

    fn map(self, raw: RawMysqlFamilyCatalog) -> Result<CatalogDiscovery, CatalogError> {
        if raw.strategy != self.strategy {
            return Err(CatalogError::Mapping(format!(
                "reader strategy {} differs from mapper strategy {}",
                raw.strategy.label(),
                self.strategy.label()
            )));
        }
        validate_raw_table_inventory(&raw)?;

        let source_kind = self.strategy.source_kind();
        let database_name = raw.facts.database.clone();
        let database_key = family_key(
            source_kind,
            &self.connection_alias,
            &database_name,
            ObjectKind::Database,
            &database_name,
            None,
        );
        let schema_key = family_key(
            source_kind,
            &self.connection_alias,
            &database_name,
            ObjectKind::Schema,
            &database_name,
            None,
        );
        let database = DatabaseObject {
            key: database_key.clone(),
            name: database_name.clone(),
        };
        let schemas = vec![SchemaObject {
            key: schema_key.clone(),
            database_key: database_key.clone(),
            name: database_name.clone(),
        }];

        let mut metadata = CanonicalMetadata::default();
        add_database_annotation(&mut metadata, &database_key, &raw);
        let principal_keys = map_principals(
            &mut metadata,
            source_kind,
            &self.connection_alias,
            &database_name,
            &database_key,
            &raw,
        )?;
        let sequence_keys = map_sequences(
            &mut metadata,
            source_kind,
            &self.connection_alias,
            &database_name,
            &schema_key,
            raw.facts.lower_case_table_names,
            &raw.sequences,
        )?;

        let mut tables = Vec::new();
        let mut table_keys = BTreeMap::new();
        let mut table_types = BTreeMap::new();
        for table in &raw.tables {
            let normalized = normalize_object_name(&table.name, raw.facts.lower_case_table_names);
            if table_types
                .insert(normalized.clone(), table.table_type.clone())
                .is_some()
            {
                return Err(CatalogError::Mapping(format!(
                    "duplicate table-like catalog name '{}'",
                    table.name
                )));
            }
            if !table.table_type.eq_ignore_ascii_case("BASE TABLE") {
                continue;
            }
            let key = family_key(
                source_kind,
                &self.connection_alias,
                &database_name,
                ObjectKind::Table,
                &table.name,
                None,
            );
            if table_keys.insert(normalized, key.clone()).is_some() {
                return Err(CatalogError::Mapping(format!(
                    "duplicate base table '{}'",
                    table.name
                )));
            }
            tables.push(TableObject {
                key: key.clone(),
                schema_key: schema_key.clone(),
                name: table.name.clone(),
                kind: TableKind::BaseTable,
            });
            add_table_annotation(&mut metadata, &key, table);
        }

        let mut view_keys = BTreeMap::new();
        for view in &raw.views {
            let normalized = normalize_object_name(&view.name, raw.facts.lower_case_table_names);
            let key = family_key(
                source_kind,
                &self.connection_alias,
                &database_name,
                ObjectKind::View,
                &view.name,
                None,
            );
            if view_keys.insert(normalized, key).is_some() {
                return Err(CatalogError::Mapping(format!(
                    "duplicate view '{}'",
                    view.name
                )));
            }
        }

        let dependencies = resolve_view_dependencies(&raw, &table_keys, &view_keys)?;
        let mut views = Vec::new();
        for view in &raw.views {
            let normalized = normalize_object_name(&view.name, raw.facts.lower_case_table_names);
            let key = view_keys.get(&normalized).cloned().ok_or_else(|| {
                CatalogError::Mapping(format!("view '{}' lost its stable key", view.name))
            })?;
            let definition = view.definition.clone().ok_or_else(|| {
                CatalogError::PermissionDenied(format!(
                    "view '{}' definition is hidden; SHOW VIEW is not effective",
                    view.name
                ))
            })?;
            views.push(ViewObject {
                key: key.clone(),
                schema_key: schema_key.clone(),
                name: view.name.clone(),
                definition: Some(definition.clone()),
                depends_on: dependencies.get(&normalized).cloned().unwrap_or_default(),
            });
            add_view_annotation(&mut metadata, &key, view, definition);
        }

        let MappedColumns {
            objects: columns,
            keys: column_keys,
        } = map_columns(
            &mut metadata,
            source_kind,
            &self.connection_alias,
            &database_name,
            raw.facts.lower_case_table_names,
            &raw.columns,
            &table_keys,
            &view_keys,
            &sequence_keys,
            &table_types,
        )?;
        let constraints = map_constraints(
            &mut metadata,
            source_kind,
            &self.connection_alias,
            &database_name,
            raw.facts.lower_case_table_names,
            &raw,
            &table_keys,
            &column_keys,
        )?;
        let indexes = map_indexes(
            &mut metadata,
            source_kind,
            &self.connection_alias,
            &database_name,
            raw.facts.lower_case_table_names,
            &raw.index_parts,
            &table_keys,
            &column_keys,
        )?;
        let (routines, routine_keys) = map_routines(
            &mut metadata,
            source_kind,
            &self.connection_alias,
            &database_name,
            &schema_key,
            &raw.routines,
            &raw.parameters,
        )?;
        let triggers = map_triggers(
            &mut metadata,
            source_kind,
            &self.connection_alias,
            &database_name,
            raw.facts.lower_case_table_names,
            &raw.triggers,
            &table_keys,
        )?;
        map_events(
            &mut metadata,
            source_kind,
            &self.connection_alias,
            &database_name,
            &schema_key,
            &raw.events,
        )?;
        map_partitions(
            &mut metadata,
            source_kind,
            &self.connection_alias,
            &database_name,
            raw.facts.lower_case_table_names,
            &raw.partitions,
            &table_keys,
        )?;
        map_view_routine_relationships(&mut metadata, &raw, &view_keys, &routine_keys)?;
        add_principal_relationships(&mut metadata, &principal_keys)?;
        validate_relationship_uniqueness(&metadata.relationships)?;

        let snapshot = CanonicalSchemaSnapshot {
            schema: SchemaSnapshot {
                source_kind: source_kind.to_owned(),
                connection_alias: self.connection_alias.clone(),
                database,
                schemas,
                tables,
                columns,
                constraints,
                indexes,
                views,
                triggers,
                routines,
                capabilities: mysql_family_capabilities(source_kind, &raw),
            },
            metadata,
        };
        let discovered_counts = discovery_counts_from_catalog(&raw, &snapshot)?;

        Ok(CatalogDiscovery {
            snapshot,
            adapter: AdapterIdentity {
                name: format!("database-memory-{}-catalog", source_kind),
                version: ADAPTER_VERSION.to_owned(),
            },
            server: ServerIdentity {
                product: match self.strategy.product() {
                    MysqlProduct::Mysql => "MySQL".to_owned(),
                    MysqlProduct::MariaDb => "MariaDB".to_owned(),
                },
                version: raw.facts.version.clone(),
            },
            scope: IntrospectionScope {
                catalogs: vec![database_name.clone()],
                schemas: vec![database_name],
            },
            discovered_counts,
            capability_checks: mysql_family_capability_checks(&raw),
        })
    }
}

fn validate_raw_table_inventory(raw: &RawMysqlFamilyCatalog) -> Result<(), CatalogError> {
    let mut table_names = BTreeSet::new();
    let mut catalog_views = BTreeSet::new();
    let mut catalog_sequences = BTreeSet::new();
    for table in &raw.tables {
        let name = normalize_object_name(&table.name, raw.facts.lower_case_table_names);
        if !table_names.insert(name.clone()) {
            return Err(CatalogError::Mapping(format!(
                "TABLES contains duplicate name '{}'",
                table.name
            )));
        }
        match table.table_type.to_ascii_uppercase().as_str() {
            "BASE TABLE" => {}
            "VIEW" => {
                catalog_views.insert(name);
            }
            "SEQUENCE" if raw.strategy.product() == MysqlProduct::MariaDb => {
                catalog_sequences.insert(name);
            }
            unsupported => {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "table-like object '{}' has unsupported TABLE_TYPE '{unsupported}'",
                    table.name
                )));
            }
        }
    }
    let views = raw
        .views
        .iter()
        .map(|view| normalize_object_name(&view.name, raw.facts.lower_case_table_names))
        .collect::<BTreeSet<_>>();
    if catalog_views != views || views.len() != raw.views.len() {
        return Err(CatalogError::Mapping(
            "TABLES view inventory does not reconcile with VIEWS".to_owned(),
        ));
    }
    let sequences = raw
        .sequences
        .iter()
        .map(|sequence| normalize_object_name(&sequence.name, raw.facts.lower_case_table_names))
        .collect::<BTreeSet<_>>();
    if catalog_sequences != sequences || sequences.len() != raw.sequences.len() {
        return Err(CatalogError::Mapping(
            "TABLES sequence inventory does not reconcile with sequence metadata".to_owned(),
        ));
    }
    Ok(())
}

fn family_key(
    source_kind: &str,
    connection_alias: &str,
    database: &str,
    object_kind: ObjectKind,
    object_name: &str,
    sub_object: Option<String>,
) -> ObjectKey {
    ObjectKey::new(
        source_kind,
        connection_alias,
        database,
        database,
        object_kind,
        object_name,
        sub_object,
    )
}

fn normalize_object_name(value: &str, lower_case_table_names: u64) -> String {
    if lower_case_table_names == 0 {
        value.to_owned()
    } else {
        value.to_ascii_lowercase()
    }
}

fn normalize_column_name(value: &str) -> String {
    value.to_ascii_lowercase()
}

fn insert_string(properties: &mut BTreeMap<String, MetadataValue>, key: &str, value: &str) {
    properties.insert(key.to_owned(), MetadataValue::String(value.to_owned()));
}

fn insert_optional_string(
    properties: &mut BTreeMap<String, MetadataValue>,
    key: &str,
    value: Option<&str>,
) {
    if let Some(value) = value {
        insert_string(properties, key, value);
    }
}

fn insert_bool(properties: &mut BTreeMap<String, MetadataValue>, key: &str, value: bool) {
    properties.insert(key.to_owned(), MetadataValue::Boolean(value));
}

fn insert_u64(properties: &mut BTreeMap<String, MetadataValue>, key: &str, value: u64) {
    properties.insert(key.to_owned(), MetadataValue::Unsigned(value));
}

fn add_annotation(
    metadata: &mut CanonicalMetadata,
    object_key: &ObjectKey,
    definition: Option<String>,
    properties: BTreeMap<String, MetadataValue>,
) {
    if definition.is_some() || !properties.is_empty() {
        metadata.annotations.push(ObjectAnnotation {
            object_key: object_key.clone(),
            definition,
            properties,
        });
    }
}

fn add_database_annotation(
    metadata: &mut CanonicalMetadata,
    database_key: &ObjectKey,
    raw: &RawMysqlFamilyCatalog,
) {
    let mut properties = BTreeMap::new();
    insert_string(&mut properties, "server_version", &raw.facts.version);
    insert_string(
        &mut properties,
        "server_version_comment",
        &raw.facts.version_comment,
    );
    insert_string(&mut properties, "current_user", &raw.facts.current_user);
    insert_string(&mut properties, "session_user", &raw.facts.session_user);
    insert_u64(
        &mut properties,
        "lower_case_table_names",
        raw.facts.lower_case_table_names,
    );
    insert_string(&mut properties, "catalog_strategy", raw.strategy.label());
    add_annotation(metadata, database_key, None, properties);
}

#[derive(Clone, Debug)]
struct PrincipalKeys {
    current: ObjectKey,
    roles: Vec<ObjectKey>,
}

fn map_principals(
    metadata: &mut CanonicalMetadata,
    source_kind: &str,
    connection_alias: &str,
    database: &str,
    database_key: &ObjectKey,
    raw: &RawMysqlFamilyCatalog,
) -> Result<PrincipalKeys, CatalogError> {
    let current_key = family_key(
        source_kind,
        connection_alias,
        database,
        ObjectKind::Principal,
        &raw.facts.current_user,
        None,
    );
    let mut properties = BTreeMap::new();
    insert_string(&mut properties, "principal_kind", "current_user");
    properties.insert(
        "schema_privileges".to_owned(),
        MetadataValue::StringList(raw.grants.iter().cloned().collect()),
    );
    metadata.objects.push(MetadataObject {
        key: current_key.clone(),
        parent_key: Some(database_key.clone()),
        name: raw.facts.current_user.clone(),
        extension_kind: None,
        definition: None,
        properties,
    });

    let mut roles = Vec::new();
    let mut seen = BTreeSet::new();
    for role in &raw.active_roles {
        let normalized = normalize_principal(role);
        if !seen.insert(normalized) {
            return Err(CatalogError::Mapping(format!(
                "duplicate active role '{role}'"
            )));
        }
        let key = family_key(
            source_kind,
            connection_alias,
            database,
            ObjectKind::Principal,
            role,
            Some("active_role".to_owned()),
        );
        let mut properties = BTreeMap::new();
        insert_string(&mut properties, "principal_kind", "active_role");
        metadata.objects.push(MetadataObject {
            key: key.clone(),
            parent_key: Some(database_key.clone()),
            name: role.clone(),
            extension_kind: None,
            definition: None,
            properties,
        });
        roles.push(key);
    }
    Ok(PrincipalKeys {
        current: current_key,
        roles,
    })
}

fn add_principal_relationships(
    metadata: &mut CanonicalMetadata,
    principals: &PrincipalKeys,
) -> Result<(), CatalogError> {
    for role in &principals.roles {
        metadata.relationships.push(MetadataRelationship {
            kind: MetadataRelationshipKind::Extension("active_role".to_owned()),
            from_key: principals.current.clone(),
            to_key: role.clone(),
            ordinal: None,
            properties: BTreeMap::new(),
        });
    }
    Ok(())
}

fn map_sequences(
    metadata: &mut CanonicalMetadata,
    source_kind: &str,
    connection_alias: &str,
    database: &str,
    schema_key: &ObjectKey,
    lower_case_table_names: u64,
    raw_sequences: &[RawSequence],
) -> Result<BTreeMap<String, ObjectKey>, CatalogError> {
    let mut keys = BTreeMap::new();
    for sequence in raw_sequences {
        let normalized = normalize_object_name(&sequence.name, lower_case_table_names);
        let key = family_key(
            source_kind,
            connection_alias,
            database,
            ObjectKind::Sequence,
            &sequence.name,
            None,
        );
        if keys.insert(normalized, key.clone()).is_some() {
            return Err(CatalogError::Mapping(format!(
                "duplicate sequence '{}'",
                sequence.name
            )));
        }
        let mut properties = BTreeMap::new();
        insert_optional_string(&mut properties, "data_type", sequence.data_type.as_deref());
        insert_optional_string(
            &mut properties,
            "start_value",
            sequence.start_value.as_deref(),
        );
        insert_optional_string(
            &mut properties,
            "minimum_value",
            sequence.minimum_value.as_deref(),
        );
        insert_optional_string(
            &mut properties,
            "maximum_value",
            sequence.maximum_value.as_deref(),
        );
        insert_optional_string(&mut properties, "increment", sequence.increment.as_deref());
        if let Some(cycles) = sequence.cycles {
            insert_bool(&mut properties, "cycles", cycles);
        }
        metadata.objects.push(MetadataObject {
            key,
            parent_key: Some(schema_key.clone()),
            name: sequence.name.clone(),
            extension_kind: None,
            definition: sequence.definition.clone(),
            properties,
        });
    }
    Ok(keys)
}

fn add_table_annotation(metadata: &mut CanonicalMetadata, table_key: &ObjectKey, table: &RawTable) {
    let mut properties = BTreeMap::new();
    insert_optional_string(&mut properties, "engine", table.engine.as_deref());
    insert_optional_string(&mut properties, "row_format", table.row_format.as_deref());
    insert_optional_string(&mut properties, "collation", table.collation.as_deref());
    insert_optional_string(
        &mut properties,
        "create_options",
        table.create_options.as_deref(),
    );
    insert_string(&mut properties, "comment", &table.comment);
    add_annotation(metadata, table_key, None, properties);
}

fn add_view_annotation(
    metadata: &mut CanonicalMetadata,
    view_key: &ObjectKey,
    view: &RawView,
    _definition: String,
) {
    let mut properties = BTreeMap::new();
    insert_string(&mut properties, "check_option", &view.check_option);
    insert_bool(&mut properties, "updatable", view.updatable);
    insert_string(&mut properties, "definer", &view.definer);
    insert_string(&mut properties, "security_type", &view.security_type);
    insert_string(&mut properties, "character_set_client", &view.character_set);
    insert_string(&mut properties, "collation_connection", &view.collation);
    insert_optional_string(&mut properties, "algorithm", view.algorithm.as_deref());
    add_annotation(metadata, view_key, None, properties);
}

#[allow(clippy::too_many_arguments)]
fn map_columns(
    metadata: &mut CanonicalMetadata,
    source_kind: &str,
    connection_alias: &str,
    database: &str,
    lower_case_table_names: u64,
    raw_columns: &[RawColumn],
    table_keys: &BTreeMap<String, ObjectKey>,
    view_keys: &BTreeMap<String, ObjectKey>,
    sequence_keys: &BTreeMap<String, ObjectKey>,
    table_types: &BTreeMap<String, String>,
) -> Result<MappedColumns, CatalogError> {
    let mut columns = Vec::new();
    let mut column_keys = BTreeMap::new();
    for column in raw_columns {
        let relation_name = normalize_object_name(&column.table, lower_case_table_names);
        let column_name = normalize_column_name(&column.name);
        let table_type = table_types.get(&relation_name).ok_or_else(|| {
            CatalogError::Mapping(format!(
                "column '{}.{}' references a missing table-like object",
                column.table, column.name
            ))
        })?;
        match table_type.to_ascii_uppercase().as_str() {
            "BASE TABLE" => {
                let table_key = table_keys.get(&relation_name).cloned().ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "column '{}.{}' lost its table key",
                        column.table, column.name
                    ))
                })?;
                let key = family_key(
                    source_kind,
                    connection_alias,
                    database,
                    ObjectKind::Column,
                    &column.table,
                    Some(column.name.clone()),
                );
                if column_keys
                    .insert((relation_name.clone(), column_name), key.clone())
                    .is_some()
                {
                    return Err(CatalogError::Mapping(format!(
                        "duplicate column '{}.{}'",
                        column.table, column.name
                    )));
                }
                columns.push(ColumnObject {
                    key: key.clone(),
                    table_key,
                    name: column.name.clone(),
                    ordinal_position: column.ordinal,
                    data_type: column.column_type.clone(),
                    is_nullable: column.nullable,
                    default_value: column.default_value.clone(),
                    is_generated: column
                        .generation_expression
                        .as_deref()
                        .is_some_and(|value| !value.is_empty())
                        || column.extra.to_ascii_uppercase().contains("GENERATED"),
                });
                add_column_annotation(metadata, &key, column);
            }
            "VIEW" => {
                let view_key = view_keys.get(&relation_name).cloned().ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "view column '{}.{}' lost its view key",
                        column.table, column.name
                    ))
                })?;
                let key = family_key(
                    source_kind,
                    connection_alias,
                    database,
                    ObjectKind::ViewColumn,
                    &column.table,
                    Some(column.name.clone()),
                );
                let mut properties = column_properties(column);
                insert_u64(&mut properties, "ordinal_position", column.ordinal as u64);
                metadata.objects.push(MetadataObject {
                    key,
                    parent_key: Some(view_key),
                    name: column.name.clone(),
                    extension_kind: None,
                    definition: column.generation_expression.clone(),
                    properties,
                });
            }
            "SEQUENCE" => {
                let sequence_key = sequence_keys.get(&relation_name).cloned().ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "sequence column '{}.{}' lost its sequence key",
                        column.table, column.name
                    ))
                })?;
                let key = family_key(
                    source_kind,
                    connection_alias,
                    database,
                    ObjectKind::Extension,
                    &column.table,
                    Some(format!("sequence_column:{}", column.name)),
                );
                let mut properties = column_properties(column);
                insert_u64(&mut properties, "ordinal_position", column.ordinal as u64);
                metadata.objects.push(MetadataObject {
                    key,
                    parent_key: Some(sequence_key),
                    name: column.name.clone(),
                    extension_kind: Some("mariadb_sequence_column".to_owned()),
                    definition: column.generation_expression.clone(),
                    properties,
                });
            }
            unsupported => {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "column '{}.{}' belongs to unsupported TABLE_TYPE '{unsupported}'",
                    column.table, column.name
                )));
            }
        }
    }
    Ok(MappedColumns {
        objects: columns,
        keys: column_keys,
    })
}

struct MappedColumns {
    objects: Vec<ColumnObject>,
    keys: BTreeMap<(String, String), ObjectKey>,
}

fn column_properties(column: &RawColumn) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_string(&mut properties, "data_type", &column.data_type);
    insert_string(&mut properties, "column_type", &column.column_type);
    insert_optional_string(
        &mut properties,
        "character_set",
        column.character_set.as_deref(),
    );
    insert_optional_string(&mut properties, "collation", column.collation.as_deref());
    insert_string(&mut properties, "extra", &column.extra);
    insert_string(&mut properties, "privileges", &column.privileges);
    insert_string(&mut properties, "comment", &column.comment);
    if let Some(spatial_reference_id) = column.spatial_reference_id {
        insert_u64(
            &mut properties,
            "spatial_reference_id",
            spatial_reference_id,
        );
    }
    insert_bool(
        &mut properties,
        "system_period_start",
        column.system_period_start,
    );
    insert_bool(
        &mut properties,
        "system_period_end",
        column.system_period_end,
    );
    properties
}

fn add_column_annotation(
    metadata: &mut CanonicalMetadata,
    column_key: &ObjectKey,
    column: &RawColumn,
) {
    add_annotation(
        metadata,
        column_key,
        column.generation_expression.clone(),
        column_properties(column),
    );
}

fn resolve_view_dependencies(
    raw: &RawMysqlFamilyCatalog,
    table_keys: &BTreeMap<String, ObjectKey>,
    view_keys: &BTreeMap<String, ObjectKey>,
) -> Result<BTreeMap<String, Vec<ObjectKey>>, CatalogError> {
    let mut grouped = raw
        .views
        .iter()
        .map(|view| {
            (
                normalize_object_name(&view.name, raw.facts.lower_case_table_names),
                BTreeSet::<String>::new(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut keys = BTreeMap::new();
    for key in table_keys.values().chain(view_keys.values()) {
        keys.insert(key.to_string(), key.clone());
    }

    match raw.strategy.product() {
        MysqlProduct::Mysql => {
            for usage in &raw.view_table_usage {
                let view = normalize_object_name(&usage.view, raw.facts.lower_case_table_names);
                let dependencies = grouped.get_mut(&view).ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "VIEW_TABLE_USAGE references missing view '{}'",
                        usage.view
                    ))
                })?;
                if normalize_object_name(&usage.target_schema, raw.facts.lower_case_table_names)
                    != normalize_object_name(&raw.facts.database, raw.facts.lower_case_table_names)
                {
                    return Err(CatalogError::UnsupportedMetadata(format!(
                        "view '{}' depends on out-of-scope database '{}.{}'",
                        usage.view, usage.target_schema, usage.target_name
                    )));
                }
                let target =
                    normalize_object_name(&usage.target_name, raw.facts.lower_case_table_names);
                let key = table_keys
                    .get(&target)
                    .or_else(|| view_keys.get(&target))
                    .ok_or_else(|| {
                        CatalogError::Mapping(format!(
                            "view '{}' dependency '{}.{}' is absent from the selected catalog",
                            usage.view, usage.target_schema, usage.target_name
                        ))
                    })?;
                dependencies.insert(key.to_string());
            }
        }
        MysqlProduct::MariaDb => {
            for view in &raw.views {
                let definition = view.definition.as_deref().ok_or_else(|| {
                    CatalogError::PermissionDenied(format!(
                        "view '{}' definition is hidden; SHOW VIEW is not effective",
                        view.name
                    ))
                })?;
                let relations =
                    parse_mariadb_view_relations(definition, raw.facts.lower_case_table_names)?;
                let view_name = normalize_object_name(&view.name, raw.facts.lower_case_table_names);
                let dependencies = grouped.get_mut(&view_name).ok_or_else(|| {
                    CatalogError::Mapping(format!("view '{}' has no dependency ledger", view.name))
                })?;
                for (schema, relation) in relations {
                    if schema.as_deref().is_some_and(|schema| {
                        normalize_object_name(schema, raw.facts.lower_case_table_names)
                            != normalize_object_name(
                                &raw.facts.database,
                                raw.facts.lower_case_table_names,
                            )
                    }) {
                        return Err(CatalogError::UnsupportedMetadata(format!(
                            "view '{}' depends on out-of-scope relation '{}.{}'",
                            view.name,
                            schema.unwrap_or_default(),
                            relation
                        )));
                    }
                    let target = normalize_object_name(&relation, raw.facts.lower_case_table_names);
                    let key = table_keys
                        .get(&target)
                        .or_else(|| view_keys.get(&target))
                        .ok_or_else(|| {
                            CatalogError::Mapping(format!(
                                "MariaDB view '{}' AST dependency '{}' is absent from the selected catalog",
                                view.name, relation
                            ))
                        })?;
                    dependencies.insert(key.to_string());
                }
            }
        }
    }

    grouped
        .into_iter()
        .map(|(view, dependency_ids)| {
            let dependencies = dependency_ids
                .into_iter()
                .map(|id| {
                    keys.get(&id).cloned().ok_or_else(|| {
                        CatalogError::Mapping(format!(
                            "view dependency stable key '{id}' was not registered"
                        ))
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok((view, dependencies))
        })
        .collect()
}

#[derive(Default)]
struct CteAliasCollector {
    aliases: BTreeSet<String>,
    lower_case_table_names: u64,
}

impl Visitor for CteAliasCollector {
    type Break = ();

    fn pre_visit_query(&mut self, query: &Query) -> ControlFlow<Self::Break> {
        if let Some(with) = &query.with {
            for cte in &with.cte_tables {
                self.aliases.insert(normalize_object_name(
                    &cte.alias.name.value,
                    self.lower_case_table_names,
                ));
            }
        }
        ControlFlow::Continue(())
    }
}

fn parse_mariadb_view_relations(
    definition: &str,
    lower_case_table_names: u64,
) -> Result<BTreeSet<(Option<String>, String)>, CatalogError> {
    let statements = Parser::parse_sql(&MySqlDialect {}, definition).map_err(|error| {
        CatalogError::UnsupportedMetadata(format!(
            "MariaDB view definition cannot be parsed as SQL AST: {error}"
        ))
    })?;
    if statements.len() != 1 {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "MariaDB view definition parsed into {} statements instead of one",
            statements.len()
        )));
    }
    let mut ctes = CteAliasCollector {
        aliases: BTreeSet::new(),
        lower_case_table_names,
    };
    let _ = statements.visit(&mut ctes);

    let mut relations = BTreeSet::new();
    let mut failure = None;
    let _: ControlFlow<()> = visit_relations(&statements, |relation| {
        if failure.is_some() {
            return ControlFlow::Continue(());
        }
        match object_name_identifiers(relation) {
            Ok(parts) if parts.len() == 1 => {
                let name = parts[0].clone();
                let normalized = normalize_object_name(&name, lower_case_table_names);
                if !ctes.aliases.contains(&normalized) && !name.eq_ignore_ascii_case("dual") {
                    relations.insert((None, name));
                }
            }
            Ok(parts) if parts.len() == 2 => {
                relations.insert((Some(parts[0].clone()), parts[1].clone()));
            }
            Ok(parts) => {
                failure = Some(CatalogError::UnsupportedMetadata(format!(
                    "MariaDB view relation '{}' uses unsupported {}-part qualification",
                    relation,
                    parts.len()
                )));
            }
            Err(error) => failure = Some(error),
        }
        ControlFlow::Continue(())
    });
    match failure {
        Some(error) => Err(error),
        None => Ok(relations),
    }
}

fn object_name_identifiers(name: &ObjectName) -> Result<Vec<String>, CatalogError> {
    name.0
        .iter()
        .map(|part| match part {
            ObjectNamePart::Identifier(identifier) => Ok(identifier.value.clone()),
            ObjectNamePart::Function(_) => Err(CatalogError::UnsupportedMetadata(format!(
                "dynamic relation identifier '{name}' cannot be proven"
            ))),
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn map_constraints(
    metadata: &mut CanonicalMetadata,
    source_kind: &str,
    connection_alias: &str,
    database: &str,
    lower_case_table_names: u64,
    raw: &RawMysqlFamilyCatalog,
    table_keys: &BTreeMap<String, ObjectKey>,
    column_keys: &BTreeMap<(String, String), ObjectKey>,
) -> Result<Vec<ConstraintObject>, CatalogError> {
    let mut key_usage = BTreeMap::<(String, String), Vec<&RawKeyUsage>>::new();
    for usage in &raw.key_usage {
        key_usage
            .entry((
                normalize_object_name(&usage.table, lower_case_table_names),
                usage.constraint.clone(),
            ))
            .or_default()
            .push(usage);
    }
    let mut checks = BTreeMap::new();
    for check in &raw.checks {
        let key = (
            normalize_object_name(&check.table, lower_case_table_names),
            check.constraint.clone(),
        );
        if checks.insert(key, check).is_some() {
            return Err(CatalogError::Mapping(format!(
                "duplicate check definition '{}.{}'",
                check.table, check.constraint
            )));
        }
    }
    let mut reference_rules = BTreeMap::new();
    for rule in &raw.reference_rules {
        let key = (
            normalize_object_name(&rule.table, lower_case_table_names),
            rule.constraint.clone(),
        );
        if reference_rules.insert(key, rule).is_some() {
            return Err(CatalogError::Mapping(format!(
                "duplicate referential rule '{}.{}'",
                rule.table, rule.constraint
            )));
        }
    }

    let mut constraints = Vec::new();
    let mut seen = BTreeSet::new();
    for raw_constraint in &raw.constraints {
        let table_name = normalize_object_name(&raw_constraint.table, lower_case_table_names);
        let identity = (table_name.clone(), raw_constraint.name.clone());
        if !seen.insert(identity.clone()) {
            return Err(CatalogError::Mapping(format!(
                "duplicate constraint '{}.{}'",
                raw_constraint.table, raw_constraint.name
            )));
        }
        let table_key = table_keys.get(&table_name).cloned().ok_or_else(|| {
            CatalogError::Mapping(format!(
                "constraint '{}.{}' targets a non-base or missing table",
                raw_constraint.table, raw_constraint.name
            ))
        })?;
        let (kind, object_kind) = match raw_constraint.constraint_type.as_str() {
            "PRIMARY KEY" => (ConstraintKind::PrimaryKey, ObjectKind::PrimaryKey),
            "FOREIGN KEY" => (ConstraintKind::ForeignKey, ObjectKind::ForeignKey),
            "UNIQUE" => (ConstraintKind::Unique, ObjectKind::UniqueConstraint),
            "CHECK" => (ConstraintKind::Check, ObjectKind::CheckConstraint),
            unsupported => {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "constraint '{}.{}' has unsupported type '{unsupported}'",
                    raw_constraint.table, raw_constraint.name
                )));
            }
        };
        let key = family_key(
            source_kind,
            connection_alias,
            database,
            object_kind,
            &raw_constraint.table,
            Some(raw_constraint.name.clone()),
        );
        let mut source_columns = Vec::new();
        let mut referenced_columns = Vec::new();
        let mut referenced_table_key = None;
        let mut uses = key_usage.remove(&identity).unwrap_or_default();
        uses.sort_by_key(|usage| usage.ordinal);
        if kind != ConstraintKind::Check {
            require_contiguous_ordinals(
                uses.iter().map(|usage| usage.ordinal),
                &format!(
                    "constraint '{}.{}'",
                    raw_constraint.table, raw_constraint.name
                ),
            )?;
            if uses.is_empty() {
                return Err(CatalogError::Mapping(format!(
                    "constraint '{}.{}' has no KEY_COLUMN_USAGE rows",
                    raw_constraint.table, raw_constraint.name
                )));
            }
        } else if !uses.is_empty() {
            return Err(CatalogError::Mapping(format!(
                "check constraint '{}.{}' unexpectedly has KEY_COLUMN_USAGE rows",
                raw_constraint.table, raw_constraint.name
            )));
        }
        for usage in uses {
            let source = column_keys
                .get(&(table_name.clone(), normalize_column_name(&usage.column)))
                .cloned()
                .ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "constraint '{}.{}' references missing source column '{}'",
                        raw_constraint.table, raw_constraint.name, usage.column
                    ))
                })?;
            source_columns.push(source);
            if kind == ConstraintKind::ForeignKey {
                let referenced_schema = usage.referenced_schema.as_deref().ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "foreign key '{}.{}' lacks referenced schema",
                        raw_constraint.table, raw_constraint.name
                    ))
                })?;
                if normalize_object_name(referenced_schema, lower_case_table_names)
                    != normalize_object_name(database, lower_case_table_names)
                {
                    return Err(CatalogError::UnsupportedMetadata(format!(
                        "foreign key '{}.{}' references out-of-scope database '{}'",
                        raw_constraint.table, raw_constraint.name, referenced_schema
                    )));
                }
                let referenced_table = usage.referenced_table.as_deref().ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "foreign key '{}.{}' lacks referenced table",
                        raw_constraint.table, raw_constraint.name
                    ))
                })?;
                let referenced_name =
                    normalize_object_name(referenced_table, lower_case_table_names);
                let candidate = table_keys.get(&referenced_name).cloned().ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "foreign key '{}.{}' references missing table '{}'",
                        raw_constraint.table, raw_constraint.name, referenced_table
                    ))
                })?;
                if referenced_table_key
                    .as_ref()
                    .is_some_and(|existing| existing != &candidate)
                {
                    return Err(CatalogError::Mapping(format!(
                        "foreign key '{}.{}' references multiple target tables",
                        raw_constraint.table, raw_constraint.name
                    )));
                }
                referenced_table_key = Some(candidate);
                let referenced_column = usage.referenced_column.as_deref().ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "foreign key '{}.{}' lacks referenced column",
                        raw_constraint.table, raw_constraint.name
                    ))
                })?;
                referenced_columns.push(
                    column_keys
                        .get(&(referenced_name, normalize_column_name(referenced_column)))
                        .cloned()
                        .ok_or_else(|| {
                            CatalogError::Mapping(format!(
                                "foreign key '{}.{}' references missing column '{}.{}'",
                                raw_constraint.table,
                                raw_constraint.name,
                                referenced_table,
                                referenced_column
                            ))
                        })?,
                );
            } else if usage.referenced_table.is_some()
                || usage.referenced_column.is_some()
                || usage.referenced_schema.is_some()
            {
                return Err(CatalogError::Mapping(format!(
                    "non-foreign constraint '{}.{}' has referenced target metadata",
                    raw_constraint.table, raw_constraint.name
                )));
            }
        }

        let expression = if kind == ConstraintKind::Check {
            Some(
                checks
                    .remove(&identity)
                    .ok_or_else(|| {
                        CatalogError::Mapping(format!(
                            "check constraint '{}.{}' has no CHECK_CONSTRAINTS row",
                            raw_constraint.table, raw_constraint.name
                        ))
                    })?
                    .clause
                    .clone(),
            )
        } else {
            None
        };
        let mut properties = BTreeMap::new();
        insert_bool(&mut properties, "enforced", raw_constraint.enforced);
        if kind == ConstraintKind::ForeignKey {
            let rule = reference_rules.remove(&identity).ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "foreign key '{}.{}' has no REFERENTIAL_CONSTRAINTS row",
                    raw_constraint.table, raw_constraint.name
                ))
            })?;
            insert_string(&mut properties, "match_option", &rule.match_option);
            insert_string(&mut properties, "update_rule", &rule.update_rule);
            insert_string(&mut properties, "delete_rule", &rule.delete_rule);
        }
        add_annotation(metadata, &key, None, properties);
        constraints.push(ConstraintObject {
            key,
            table_key,
            name: raw_constraint.name.clone(),
            kind,
            columns: source_columns,
            referenced_table_key,
            referenced_columns,
            expression,
        });
    }
    if let Some(((table, name), _)) = key_usage.into_iter().next() {
        return Err(CatalogError::Mapping(format!(
            "KEY_COLUMN_USAGE row '{table}.{name}' has no TABLE_CONSTRAINTS owner"
        )));
    }
    if let Some(((table, name), _)) = checks.into_iter().next() {
        return Err(CatalogError::Mapping(format!(
            "CHECK_CONSTRAINTS row '{table}.{name}' has no TABLE_CONSTRAINTS owner"
        )));
    }
    if let Some(((table, name), _)) = reference_rules.into_iter().next() {
        return Err(CatalogError::Mapping(format!(
            "REFERENTIAL_CONSTRAINTS row '{table}.{name}' has no foreign key owner"
        )));
    }
    Ok(constraints)
}

#[allow(clippy::too_many_arguments)]
fn map_indexes(
    metadata: &mut CanonicalMetadata,
    source_kind: &str,
    connection_alias: &str,
    database: &str,
    lower_case_table_names: u64,
    raw_parts: &[RawIndexPart],
    table_keys: &BTreeMap<String, ObjectKey>,
    column_keys: &BTreeMap<(String, String), ObjectKey>,
) -> Result<Vec<IndexObject>, CatalogError> {
    let mut grouped = BTreeMap::<(String, String), Vec<&RawIndexPart>>::new();
    for part in raw_parts {
        grouped
            .entry((
                normalize_object_name(&part.table, lower_case_table_names),
                part.index.clone(),
            ))
            .or_default()
            .push(part);
    }
    let mut indexes = Vec::new();
    for ((table_name, index_name), mut parts) in grouped {
        parts.sort_by_key(|part| part.ordinal);
        require_contiguous_ordinals(
            parts.iter().map(|part| part.ordinal),
            &format!("index '{table_name}.{index_name}'"),
        )?;
        let first = parts[0];
        if parts.iter().any(|part| {
            part.non_unique != first.non_unique
                || part.index_type != first.index_type
                || part.visible != first.visible
                || part.comment != first.comment
                || part.index_comment != first.index_comment
        }) {
            return Err(CatalogError::Mapping(format!(
                "index '{table_name}.{index_name}' has inconsistent part metadata"
            )));
        }
        let table_key = table_keys.get(&table_name).cloned().ok_or_else(|| {
            CatalogError::Mapping(format!(
                "index '{table_name}.{index_name}' targets a non-base or missing table"
            ))
        })?;
        let mut columns = Vec::new();
        let mut expressions = Vec::new();
        let mut part_descriptions = Vec::new();
        for part in parts {
            match (part.column.as_deref(), part.expression.as_deref()) {
                (Some(column), None) => {
                    columns.push(
                        column_keys
                            .get(&(
                                table_name.clone(),
                                normalize_column_name(column),
                            ))
                            .cloned()
                            .ok_or_else(|| {
                                CatalogError::Mapping(format!(
                                    "index '{table_name}.{index_name}' references missing column '{column}'"
                                ))
                            })?,
                    );
                    part_descriptions.push(format_index_part(part, column));
                }
                (None, Some(expression)) if !expression.trim().is_empty() => {
                    expressions.push(expression.to_owned());
                    part_descriptions.push(format_index_part(part, expression));
                }
                (Some(_), Some(_)) => {
                    return Err(CatalogError::Mapping(format!(
                        "index '{table_name}.{index_name}' part {} has both column and expression",
                        part.ordinal
                    )));
                }
                _ => {
                    return Err(CatalogError::Mapping(format!(
                        "index '{table_name}.{index_name}' part {} has neither column nor expression",
                        part.ordinal
                    )));
                }
            }
        }
        let key = family_key(
            source_kind,
            connection_alias,
            database,
            ObjectKind::Index,
            &first.table,
            Some(index_name.clone()),
        );
        let mut properties = BTreeMap::new();
        insert_string(&mut properties, "index_type", &first.index_type);
        insert_bool(&mut properties, "visible", first.visible);
        insert_string(&mut properties, "comment", &first.comment);
        insert_string(&mut properties, "index_comment", &first.index_comment);
        properties.insert(
            "parts".to_owned(),
            MetadataValue::StringList(part_descriptions),
        );
        add_annotation(metadata, &key, None, properties);
        indexes.push(IndexObject {
            key,
            table_key,
            name: index_name.clone(),
            columns,
            is_unique: !first.non_unique,
            is_primary: index_name == "PRIMARY",
            predicate: None,
            expression: (!expressions.is_empty()).then(|| expressions.join(", ")),
        });
    }
    Ok(indexes)
}

fn format_index_part(part: &RawIndexPart, value: &str) -> String {
    let mut description = format!("{}:{value}", part.ordinal);
    if let Some(prefix_length) = part.prefix_length {
        description.push_str(&format!(":prefix={prefix_length}"));
    }
    if let Some(collation) = part.collation.as_deref() {
        description.push_str(&format!(":order={collation}"));
    }
    description
}

fn require_contiguous_ordinals(
    ordinals: impl IntoIterator<Item = u32>,
    subject: &str,
) -> Result<(), CatalogError> {
    for (index, ordinal) in ordinals.into_iter().enumerate() {
        let expected = u32::try_from(index + 1)
            .map_err(|_| CatalogError::Mapping(format!("{subject} has too many terms")))?;
        if ordinal != expected {
            return Err(CatalogError::Mapping(format!(
                "{subject} ordinal {ordinal} is not contiguous; expected {expected}"
            )));
        }
    }
    Ok(())
}

fn map_routines(
    metadata: &mut CanonicalMetadata,
    source_kind: &str,
    connection_alias: &str,
    database: &str,
    schema_key: &ObjectKey,
    raw_routines: &[RawRoutine],
    raw_parameters: &[RawParameter],
) -> Result<(Vec<RoutineObject>, BTreeMap<String, ObjectKey>), CatalogError> {
    let mut routines = Vec::new();
    let mut routine_keys = BTreeMap::new();
    for routine in raw_routines {
        let kind = match routine.routine_type.as_str() {
            "FUNCTION" => RoutineKind::Function,
            "PROCEDURE" => RoutineKind::Procedure,
            unsupported => {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "routine '{}' has unsupported ROUTINE_TYPE '{unsupported}'",
                    routine.name
                )));
            }
        };
        let key = family_key(
            source_kind,
            connection_alias,
            database,
            ObjectKind::Routine,
            &routine.name,
            Some(routine.specific_name.clone()),
        );
        let normalized = routine.specific_name.to_ascii_lowercase();
        if routine_keys.insert(normalized, key.clone()).is_some() {
            return Err(CatalogError::Mapping(format!(
                "duplicate routine specific name '{}'",
                routine.specific_name
            )));
        }
        routines.push(RoutineObject {
            key: key.clone(),
            schema_key: schema_key.clone(),
            name: routine.name.clone(),
            kind,
            definition: routine.definition.clone(),
            depends_on: Vec::new(),
        });
        let mut properties = BTreeMap::new();
        insert_string(&mut properties, "specific_name", &routine.specific_name);
        insert_string(&mut properties, "data_type", &routine.data_type);
        insert_optional_string(
            &mut properties,
            "dtd_identifier",
            routine.dtd_identifier.as_deref(),
        );
        insert_bool(&mut properties, "deterministic", routine.deterministic);
        insert_string(&mut properties, "sql_data_access", &routine.sql_data_access);
        insert_string(&mut properties, "security_type", &routine.security_type);
        insert_string(&mut properties, "sql_mode", &routine.sql_mode);
        insert_string(&mut properties, "comment", &routine.comment);
        insert_string(&mut properties, "definer", &routine.definer);
        insert_optional_string(
            &mut properties,
            "character_set_client",
            routine.character_set.as_deref(),
        );
        insert_optional_string(
            &mut properties,
            "collation_connection",
            routine.collation.as_deref(),
        );
        insert_string(
            &mut properties,
            "database_collation",
            &routine.database_collation,
        );
        add_annotation(metadata, &key, None, properties);
    }

    let mut parameter_ids = BTreeSet::new();
    for parameter in raw_parameters {
        let routine_key = routine_keys
            .get(&parameter.specific_name.to_ascii_lowercase())
            .cloned()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "parameter {}:{} has no routine owner",
                    parameter.specific_name, parameter.ordinal
                ))
            })?;
        let owner = raw_routines
            .iter()
            .find(|routine| routine.specific_name == parameter.specific_name)
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "parameter {}:{} lost its raw routine owner",
                    parameter.specific_name, parameter.ordinal
                ))
            })?;
        if parameter.routine_type != owner.routine_type {
            return Err(CatalogError::Mapping(format!(
                "parameter {}:{} routine type '{}' differs from owner type '{}'",
                parameter.specific_name,
                parameter.ordinal,
                parameter.routine_type,
                owner.routine_type
            )));
        }
        let identity = (parameter.specific_name.clone(), parameter.ordinal);
        if !parameter_ids.insert(identity) {
            return Err(CatalogError::Mapping(format!(
                "duplicate routine parameter {}:{}",
                parameter.specific_name, parameter.ordinal
            )));
        }
        let display_name = parameter.name.clone().unwrap_or_else(|| {
            if parameter.ordinal == 0 {
                "return"
            } else {
                "unnamed"
            }
            .to_owned()
        });
        let key = family_key(
            source_kind,
            connection_alias,
            database,
            ObjectKind::RoutineParameter,
            &owner.name,
            Some(format!(
                "{}:{}:{}",
                parameter.specific_name, parameter.ordinal, display_name
            )),
        );
        let mut properties = BTreeMap::new();
        insert_u64(
            &mut properties,
            "ordinal_position",
            parameter.ordinal as u64,
        );
        insert_optional_string(&mut properties, "mode", parameter.mode.as_deref());
        insert_string(&mut properties, "data_type", &parameter.data_type);
        insert_optional_string(
            &mut properties,
            "dtd_identifier",
            parameter.dtd_identifier.as_deref(),
        );
        insert_optional_string(
            &mut properties,
            "default_value",
            parameter.default_value.as_deref(),
        );
        metadata.objects.push(MetadataObject {
            key: key.clone(),
            parent_key: Some(routine_key.clone()),
            name: display_name,
            extension_kind: None,
            definition: None,
            properties,
        });
        metadata.relationships.push(MetadataRelationship {
            kind: MetadataRelationshipKind::HasParameter,
            from_key: routine_key,
            to_key: key,
            ordinal: Some(parameter.ordinal),
            properties: BTreeMap::new(),
        });
    }
    Ok((routines, routine_keys))
}

#[allow(clippy::too_many_arguments)]
fn map_triggers(
    metadata: &mut CanonicalMetadata,
    source_kind: &str,
    connection_alias: &str,
    database: &str,
    lower_case_table_names: u64,
    raw_triggers: &[RawTrigger],
    table_keys: &BTreeMap<String, ObjectKey>,
) -> Result<Vec<TriggerObject>, CatalogError> {
    let mut triggers = Vec::new();
    let mut seen = BTreeSet::new();
    for trigger in raw_triggers {
        if !seen.insert(trigger.name.to_ascii_lowercase()) {
            return Err(CatalogError::Mapping(format!(
                "duplicate trigger '{}'",
                trigger.name
            )));
        }
        let table_name = normalize_object_name(&trigger.table, lower_case_table_names);
        let table_key = table_keys.get(&table_name).cloned().ok_or_else(|| {
            CatalogError::Mapping(format!(
                "trigger '{}' targets missing base table '{}'",
                trigger.name, trigger.table
            ))
        })?;
        let key = family_key(
            source_kind,
            connection_alias,
            database,
            ObjectKind::Trigger,
            &trigger.table,
            Some(trigger.name.clone()),
        );
        triggers.push(TriggerObject {
            key: key.clone(),
            table_key,
            name: trigger.name.clone(),
            timing: Some(trigger.timing.clone()),
            events: vec![trigger.event.clone()],
            definition: trigger.statement.clone(),
            executes_routine_key: None,
        });
        let mut properties = BTreeMap::new();
        insert_u64(&mut properties, "action_order", trigger.action_order);
        insert_optional_string(
            &mut properties,
            "action_condition",
            trigger.condition.as_deref(),
        );
        insert_string(&mut properties, "orientation", &trigger.orientation);
        insert_string(&mut properties, "sql_mode", &trigger.sql_mode);
        insert_string(&mut properties, "definer", &trigger.definer);
        insert_string(
            &mut properties,
            "character_set_client",
            &trigger.character_set,
        );
        insert_string(&mut properties, "collation_connection", &trigger.collation);
        insert_string(
            &mut properties,
            "database_collation",
            &trigger.database_collation,
        );
        add_annotation(metadata, &key, None, properties);
    }
    Ok(triggers)
}

fn map_events(
    metadata: &mut CanonicalMetadata,
    source_kind: &str,
    connection_alias: &str,
    database: &str,
    schema_key: &ObjectKey,
    raw_events: &[RawEvent],
) -> Result<(), CatalogError> {
    let mut seen = BTreeSet::new();
    for event in raw_events {
        if !seen.insert(event.name.to_ascii_lowercase()) {
            return Err(CatalogError::Mapping(format!(
                "duplicate scheduled event '{}'",
                event.name
            )));
        }
        let key = family_key(
            source_kind,
            connection_alias,
            database,
            ObjectKind::Event,
            &event.name,
            None,
        );
        let mut properties = BTreeMap::new();
        insert_string(&mut properties, "definer", &event.definer);
        insert_string(&mut properties, "time_zone", &event.time_zone);
        insert_string(&mut properties, "body", &event.body);
        insert_string(&mut properties, "event_type", &event.event_type);
        insert_optional_string(&mut properties, "execute_at", event.execute_at.as_deref());
        insert_optional_string(
            &mut properties,
            "interval_value",
            event.interval_value.as_deref(),
        );
        insert_optional_string(
            &mut properties,
            "interval_field",
            event.interval_field.as_deref(),
        );
        insert_string(&mut properties, "sql_mode", &event.sql_mode);
        insert_optional_string(&mut properties, "starts", event.starts.as_deref());
        insert_optional_string(&mut properties, "ends", event.ends.as_deref());
        insert_string(&mut properties, "status", &event.status);
        insert_string(&mut properties, "on_completion", &event.on_completion);
        insert_string(&mut properties, "comment", &event.comment);
        metadata.objects.push(MetadataObject {
            key,
            parent_key: Some(schema_key.clone()),
            name: event.name.clone(),
            extension_kind: None,
            definition: event.definition.clone(),
            properties,
        });
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn map_partitions(
    metadata: &mut CanonicalMetadata,
    source_kind: &str,
    connection_alias: &str,
    database: &str,
    lower_case_table_names: u64,
    raw_partitions: &[RawPartition],
    table_keys: &BTreeMap<String, ObjectKey>,
) -> Result<(), CatalogError> {
    let mut partition_keys = BTreeMap::<(String, String), ObjectKey>::new();
    let mut subpartitions = BTreeSet::new();
    for partition in raw_partitions {
        let table_name = normalize_object_name(&partition.table, lower_case_table_names);
        let table_key = table_keys.get(&table_name).cloned().ok_or_else(|| {
            CatalogError::Mapping(format!(
                "partition '{}.{}' targets missing base table",
                partition.table, partition.partition
            ))
        })?;
        let partition_identity = (table_name.clone(), partition.partition.clone());
        let partition_key = match partition_keys.get(&partition_identity) {
            Some(key) => key.clone(),
            None => {
                let key = family_key(
                    source_kind,
                    connection_alias,
                    database,
                    ObjectKind::Extension,
                    &partition.table,
                    Some(format!("partition:{}", partition.partition)),
                );
                let mut properties = BTreeMap::new();
                insert_u64(
                    &mut properties,
                    "ordinal_position",
                    partition.partition_ordinal as u64,
                );
                insert_optional_string(&mut properties, "method", partition.method.as_deref());
                insert_optional_string(
                    &mut properties,
                    "expression",
                    partition.expression.as_deref(),
                );
                insert_optional_string(
                    &mut properties,
                    "description",
                    partition.description.as_deref(),
                );
                insert_string(&mut properties, "comment", &partition.comment);
                insert_optional_string(
                    &mut properties,
                    "tablespace",
                    partition.tablespace.as_deref(),
                );
                metadata.objects.push(MetadataObject {
                    key: key.clone(),
                    parent_key: Some(table_key),
                    name: partition.partition.clone(),
                    extension_kind: Some("mysql_partition".to_owned()),
                    definition: partition.expression.clone(),
                    properties,
                });
                partition_keys.insert(partition_identity.clone(), key.clone());
                key
            }
        };
        if let Some(subpartition) = partition.subpartition.as_deref() {
            let identity = (
                table_name,
                partition.partition.clone(),
                subpartition.to_owned(),
            );
            if !subpartitions.insert(identity) {
                return Err(CatalogError::Mapping(format!(
                    "duplicate subpartition '{}.{}.{}'",
                    partition.table, partition.partition, subpartition
                )));
            }
            let key = family_key(
                source_kind,
                connection_alias,
                database,
                ObjectKind::Extension,
                &partition.table,
                Some(format!(
                    "partition:{}:subpartition:{subpartition}",
                    partition.partition
                )),
            );
            let mut properties = BTreeMap::new();
            if let Some(ordinal) = partition.subpartition_ordinal {
                insert_u64(&mut properties, "ordinal_position", ordinal as u64);
            }
            insert_optional_string(
                &mut properties,
                "method",
                partition.subpartition_method.as_deref(),
            );
            insert_optional_string(
                &mut properties,
                "expression",
                partition.subpartition_expression.as_deref(),
            );
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(partition_key),
                name: subpartition.to_owned(),
                extension_kind: Some("mysql_subpartition".to_owned()),
                definition: partition.subpartition_expression.clone(),
                properties,
            });
        }
    }
    Ok(())
}

fn map_view_routine_relationships(
    metadata: &mut CanonicalMetadata,
    raw: &RawMysqlFamilyCatalog,
    view_keys: &BTreeMap<String, ObjectKey>,
    routine_keys: &BTreeMap<String, ObjectKey>,
) -> Result<(), CatalogError> {
    for usage in &raw.view_routine_usage {
        if normalize_object_name(&usage.routine_schema, raw.facts.lower_case_table_names)
            != normalize_object_name(&raw.facts.database, raw.facts.lower_case_table_names)
        {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "view '{}' invokes out-of-scope routine '{}.{}'",
                usage.view, usage.routine_schema, usage.specific_name
            )));
        }
        let view = view_keys
            .get(&normalize_object_name(
                &usage.view,
                raw.facts.lower_case_table_names,
            ))
            .cloned()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "VIEW_ROUTINE_USAGE references missing view '{}'",
                    usage.view
                ))
            })?;
        let routine = routine_keys
            .get(&usage.specific_name.to_ascii_lowercase())
            .cloned()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "VIEW_ROUTINE_USAGE references missing routine '{}'",
                    usage.specific_name
                ))
            })?;
        metadata.relationships.push(MetadataRelationship {
            kind: MetadataRelationshipKind::Invokes,
            from_key: view,
            to_key: routine,
            ordinal: None,
            properties: BTreeMap::new(),
        });
    }
    Ok(())
}

fn validate_relationship_uniqueness(
    relationships: &[MetadataRelationship],
) -> Result<(), CatalogError> {
    let mut seen = BTreeSet::new();
    for relationship in relationships.iter() {
        let identity = (
            relationship.kind.clone(),
            relationship.from_key.to_string(),
            relationship.to_key.to_string(),
            relationship.ordinal,
        );
        if !seen.insert(identity) {
            return Err(CatalogError::Mapping(format!(
                "duplicate metadata relationship {} -> {}",
                relationship.from_key, relationship.to_key
            )));
        }
    }
    Ok(())
}

fn mysql_family_capabilities(
    source_kind: &str,
    raw: &RawMysqlFamilyCatalog,
) -> AdapterCapabilities {
    let has_routines = !raw.routines.is_empty();
    let has_triggers = !raw.triggers.is_empty();
    let has_events = !raw.events.is_empty();
    let has_opaque_procedural_metadata = raw
        .routines
        .iter()
        .any(|routine| routine.definition.is_none())
        || raw
            .triggers
            .iter()
            .any(|trigger| trigger.statement.is_none())
        || raw.events.iter().any(|event| event.definition.is_none());
    let mut limitations = Vec::new();
    if has_routines {
        limitations.push(format!(
            "{} routine body dependency path(s) are not catalog-proven; only direct catalog relationships are emitted",
            raw.routines.len()
        ));
    }
    if has_triggers {
        limitations.push(format!(
            "{} trigger body dependency path(s) are not catalog-proven; trigger target relationships are emitted",
            raw.triggers.len()
        ));
    }
    if has_events {
        limitations.push(format!(
            "{} scheduled event body dependency path(s) are not catalog-proven",
            raw.events.len()
        ));
    }
    if has_opaque_procedural_metadata {
        limitations.push(
            "one or more procedural definitions are hidden; structural metadata is retained without guessed SQL"
                .to_owned(),
        );
    }
    AdapterCapabilities {
        source_kind: source_kind.to_owned(),
        metadata_only: true,
        schemas: true,
        tables: true,
        columns: true,
        constraints: true,
        indexes: true,
        views: CapabilitySupport::Supported,
        triggers: if has_triggers {
            CapabilitySupport::Partial
        } else {
            CapabilitySupport::Supported
        },
        routines: if has_routines {
            CapabilitySupport::Partial
        } else {
            CapabilitySupport::Supported
        },
        dependencies: if has_routines || has_triggers || has_events {
            CapabilitySupport::Partial
        } else {
            CapabilitySupport::Supported
        },
        limitations,
        notes: vec![
            "Reads INFORMATION_SCHEMA and SHOW CREATE metadata only; application table rows are never queried."
                .to_owned(),
            "The selected MySQL-family database is mapped to the common database and schema scope."
                .to_owned(),
            "Objects whose procedural dependencies cannot be proven remain structural boundary objects; no guessed dependency edge is emitted."
                .to_owned(),
        ],
    }
}

fn mysql_family_capability_checks(raw: &RawMysqlFamilyCatalog) -> Vec<CapabilityCheck> {
    vec![
        CapabilityCheck {
            name: "catalog_stability".to_owned(),
            evidence: "ordered metadata signatures matched before and after catalog discovery"
                .to_owned(),
        },
        CapabilityCheck {
            name: "metadata_only_catalog_queries".to_owned(),
            evidence: "adapter queried INFORMATION_SCHEMA, session/server facts, and SHOW CREATE SEQUENCE only; no application relation appears in a SELECT FROM clause"
                .to_owned(),
        },
        CapabilityCheck {
            name: "metadata_visibility".to_owned(),
            evidence: format!(
                "effective schema/global privilege proof includes SELECT, SHOW VIEW, EXECUTE, EVENT, and TRIGGER ({} privilege entries)",
                raw.grants.len()
            ),
        },
        CapabilityCheck {
            name: "principal_context".to_owned(),
            evidence: format!(
                "current_user={} session_user={} active_roles={}",
                raw.facts.current_user,
                raw.facts.session_user,
                raw.active_roles.len()
            ),
        },
        CapabilityCheck {
            name: "read_only_repeatable_read_transaction".to_owned(),
            evidence: format!(
                "transaction_read_only={} transaction_isolation={}",
                raw.transaction_read_only, raw.transaction_isolation
            ),
        },
        CapabilityCheck {
            name: "supported_server_version".to_owned(),
            evidence: format!(
                "server version {} maps to certified strategy {}",
                raw.facts.version,
                raw.strategy.label()
            ),
        },
        CapabilityCheck {
            name: "transport_security".to_owned(),
            evidence: raw
                .facts
                .tls_cipher
                .as_deref()
                .map(|cipher| format!("TLS enabled with cipher {cipher}"))
                .unwrap_or_else(|| {
                    "plaintext transport is accepted only by the connection policy for a loopback/local endpoint"
                        .to_owned()
                }),
        },
        CapabilityCheck {
            name: "view_dependency_proof".to_owned(),
            evidence: match raw.strategy.product() {
                MysqlProduct::Mysql => format!(
                    "{} VIEW_TABLE_USAGE and {} VIEW_ROUTINE_USAGE rows reconciled to canonical dependencies",
                    raw.view_table_usage.len(),
                    raw.view_routine_usage.len()
                ),
                MysqlProduct::MariaDb => format!(
                    "all {} frozen MariaDB view definitions were parsed with the MySQL SQL AST dialect",
                    raw.views.len()
                ),
            },
        },
    ]
}

fn discovery_counts_from_catalog(
    raw: &RawMysqlFamilyCatalog,
    snapshot: &CanonicalSchemaSnapshot,
) -> Result<DiscoveryCounts, CatalogError> {
    let table_type_by_name = raw
        .tables
        .iter()
        .map(|table| {
            (
                normalize_object_name(&table.name, raw.facts.lower_case_table_names),
                table.table_type.to_ascii_uppercase(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let base_table_count = raw
        .tables
        .iter()
        .filter(|table| table.table_type.eq_ignore_ascii_case("BASE TABLE"))
        .count() as u64;
    let base_column_count = raw
        .columns
        .iter()
        .filter(|column| {
            table_type_by_name
                .get(&normalize_object_name(
                    &column.table,
                    raw.facts.lower_case_table_names,
                ))
                .is_some_and(|table_type| table_type == "BASE TABLE")
        })
        .count() as u64;
    let view_column_count = raw
        .columns
        .iter()
        .filter(|column| {
            table_type_by_name
                .get(&normalize_object_name(
                    &column.table,
                    raw.facts.lower_case_table_names,
                ))
                .is_some_and(|table_type| table_type == "VIEW")
        })
        .count() as u64;
    let sequence_column_count = raw
        .columns
        .iter()
        .filter(|column| {
            table_type_by_name
                .get(&normalize_object_name(
                    &column.table,
                    raw.facts.lower_case_table_names,
                ))
                .is_some_and(|table_type| table_type == "SEQUENCE")
        })
        .count() as u64;
    let index_identities = raw
        .index_parts
        .iter()
        .map(|part| {
            (
                normalize_object_name(&part.table, raw.facts.lower_case_table_names),
                part.index.clone(),
            )
        })
        .collect::<BTreeSet<_>>();
    let partition_identities = raw
        .partitions
        .iter()
        .map(|partition| {
            (
                normalize_object_name(&partition.table, raw.facts.lower_case_table_names),
                partition.partition.clone(),
            )
        })
        .collect::<BTreeSet<_>>();
    let subpartition_count = raw
        .partitions
        .iter()
        .filter(|partition| partition.subpartition.is_some())
        .count() as u64;

    let mut objects = ObjectCategory::ALL
        .into_iter()
        .map(|category| (category, 0_u64))
        .collect::<BTreeMap<_, _>>();
    objects.insert(ObjectCategory::Database, 1);
    objects.insert(ObjectCategory::Schema, 1);
    objects.insert(ObjectCategory::Table, base_table_count);
    objects.insert(ObjectCategory::Column, base_column_count);
    for (constraint_type, category) in [
        ("PRIMARY KEY", ObjectCategory::PrimaryKey),
        ("FOREIGN KEY", ObjectCategory::ForeignKey),
        ("UNIQUE", ObjectCategory::UniqueConstraint),
        ("CHECK", ObjectCategory::CheckConstraint),
    ] {
        objects.insert(
            category,
            raw.constraints
                .iter()
                .filter(|constraint| constraint.constraint_type == constraint_type)
                .count() as u64,
        );
    }
    objects.insert(ObjectCategory::Index, index_identities.len() as u64);
    objects.insert(ObjectCategory::View, raw.views.len() as u64);
    objects.insert(ObjectCategory::ViewColumn, view_column_count);
    objects.insert(ObjectCategory::Trigger, raw.triggers.len() as u64);
    objects.insert(ObjectCategory::Routine, raw.routines.len() as u64);
    objects.insert(ObjectCategory::Sequence, raw.sequences.len() as u64);
    objects.insert(
        ObjectCategory::RoutineParameter,
        raw.parameters.len() as u64,
    );
    objects.insert(ObjectCategory::Event, raw.events.len() as u64);
    objects.insert(
        ObjectCategory::Principal,
        1_u64 + raw.active_roles.len() as u64,
    );
    objects.insert(
        ObjectCategory::Extension,
        sequence_column_count + partition_identities.len() as u64 + subpartition_count,
    );

    let emitted_objects = emitted_object_counts(snapshot);
    for category in ObjectCategory::ALL {
        let discovered = objects.get(&category).copied().unwrap_or_default();
        let emitted = emitted_objects.get(&category).copied().unwrap_or_default();
        if discovered != emitted {
            return Err(CatalogError::Mapping(format!(
                "{} raw/emitted object count mismatch for {category:?}: discovered={discovered}, emitted={emitted}",
                raw.strategy.label()
            )));
        }
    }

    let constraint_types = raw
        .constraints
        .iter()
        .map(|constraint| {
            (
                (
                    normalize_object_name(&constraint.table, raw.facts.lower_case_table_names),
                    constraint.name.clone(),
                ),
                constraint.constraint_type.as_str(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let constraint_columns = raw
        .key_usage
        .iter()
        .filter(|usage| {
            constraint_types.get(&(
                normalize_object_name(&usage.table, raw.facts.lower_case_table_names),
                usage.constraint.clone(),
            )) != Some(&"FOREIGN KEY")
        })
        .count() as u64;
    let foreign_key_pairs = raw
        .key_usage
        .iter()
        .filter(|usage| {
            constraint_types.get(&(
                normalize_object_name(&usage.table, raw.facts.lower_case_table_names),
                usage.constraint.clone(),
            )) == Some(&"FOREIGN KEY")
        })
        .count() as u64;
    let index_column_count = raw
        .index_parts
        .iter()
        .filter(|part| part.column.is_some())
        .count() as u64;

    let mut relationships = RelationshipCategory::ALL
        .into_iter()
        .map(|category| (category, 0_u64))
        .collect::<BTreeMap<_, _>>();
    relationships.insert(RelationshipCategory::DatabaseHasSchema, 1);
    relationships.insert(RelationshipCategory::SchemaHasTable, base_table_count);
    relationships.insert(RelationshipCategory::TableHasColumn, base_column_count);
    relationships.insert(
        RelationshipCategory::TableHasConstraint,
        raw.constraints.len() as u64,
    );
    relationships.insert(RelationshipCategory::ConstraintColumn, constraint_columns);
    relationships.insert(
        RelationshipCategory::ForeignKeyColumnPair,
        foreign_key_pairs,
    );
    relationships.insert(
        RelationshipCategory::TableHasIndex,
        index_identities.len() as u64,
    );
    relationships.insert(RelationshipCategory::IndexColumn, index_column_count);
    relationships.insert(RelationshipCategory::SchemaHasView, raw.views.len() as u64);
    relationships.insert(
        RelationshipCategory::ViewDependency,
        snapshot
            .schema
            .views
            .iter()
            .map(|view| view.depends_on.len() as u64)
            .sum(),
    );
    relationships.insert(
        RelationshipCategory::TriggerTarget,
        raw.triggers.len() as u64,
    );
    relationships.insert(RelationshipCategory::TriggerRoutine, 0);
    relationships.insert(
        RelationshipCategory::SchemaHasRoutine,
        raw.routines.len() as u64,
    );
    relationships.insert(RelationshipCategory::RoutineDependency, 0);
    relationships.insert(
        RelationshipCategory::MetadataParent,
        snapshot
            .metadata
            .objects
            .iter()
            .filter(|object| object.parent_key.is_some())
            .count() as u64,
    );
    relationships.insert(
        RelationshipCategory::MetadataRelationship,
        snapshot.metadata.relationships.len() as u64,
    );

    let emitted_relationships = emitted_relationship_counts(snapshot);
    for category in RelationshipCategory::ALL {
        let discovered = relationships.get(&category).copied().unwrap_or_default();
        let emitted = emitted_relationships
            .get(&category)
            .copied()
            .unwrap_or_default();
        if discovered != emitted {
            return Err(CatalogError::Mapping(format!(
                "{} raw/emitted relationship count mismatch for {category:?}: discovered={discovered}, emitted={emitted}",
                raw.strategy.label()
            )));
        }
    }

    Ok(DiscoveryCounts {
        objects: objects
            .into_iter()
            .map(|(category, count)| {
                (
                    category,
                    DiscoveredCount {
                        count,
                        evidence: format!(
                            "{} INFORMATION_SCHEMA raw object inventory for {category:?}",
                            raw.strategy.label()
                        ),
                    },
                )
            })
            .collect(),
        relationships: relationships
            .into_iter()
            .map(|(category, count)| {
                (
                    category,
                    DiscoveredCount {
                        count,
                        evidence: format!(
                            "{} strict relationship ledger for {category:?}",
                            raw.strategy.label()
                        ),
                    },
                )
            })
            .collect(),
    })
}

