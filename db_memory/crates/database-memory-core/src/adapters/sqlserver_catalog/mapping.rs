impl SqlServerSnapshotMapper {
    fn new(connection_alias: &str, facts: ServerFacts, strategy: SqlServerCatalogVersion) -> Self {
        Self {
            connection_alias: connection_alias.to_owned(),
            facts,
            strategy,
        }
    }

    fn map(self, raw: RawSqlServerCatalog) -> Result<CatalogDiscovery, CatalogError> {
        validate_raw_inventory(&raw)?;
        let database_name = self.facts.database.clone();
        let database_key = sqlserver_key(
            &self.connection_alias,
            &database_name,
            &database_name,
            ObjectKind::Database,
            &database_name,
            None,
        );
        let database = DatabaseObject {
            key: database_key.clone(),
            name: database_name.clone(),
        };
        let mut metadata = CanonicalMetadata::default();
        add_database_annotation(&mut metadata, &database_key, &self.facts, self.strategy);

        let mut principal_keys = BTreeMap::<i32, ObjectKey>::new();
        for principal in &raw.principals {
            let key = sqlserver_key(
                &self.connection_alias,
                &database_name,
                &database_name,
                ObjectKind::Principal,
                &principal.name,
                Some(principal.id.to_string()),
            );
            if principal_keys.insert(principal.id, key.clone()).is_some() {
                return Err(CatalogError::Mapping(format!(
                    "duplicate database principal id {}",
                    principal.id
                )));
            }
            let mut properties = BTreeMap::new();
            insert_string(&mut properties, "type", &principal.type_code);
            insert_string(&mut properties, "type_description", &principal.type_desc);
            insert_optional_string(
                &mut properties,
                "default_schema",
                principal.default_schema.as_deref(),
            );
            insert_string(
                &mut properties,
                "authentication_type",
                &principal.authentication_type,
            );
            insert_bool(&mut properties, "fixed_role", principal.fixed_role);
            insert_optional_i64(
                &mut properties,
                "owning_principal_id",
                principal.owning_principal_id.map(i64::from),
            );
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(database_key.clone()),
                name: principal.name.clone(),
                extension_kind: None,
                definition: None,
                properties,
            });
        }

        let mut schemas = Vec::new();
        let mut schema_keys = BTreeMap::<String, ObjectKey>::new();
        let mut schema_id_keys = BTreeMap::<i32, ObjectKey>::new();
        let mut schema_owner_ids = BTreeMap::<String, i32>::new();
        for schema in &raw.schemas {
            let key = sqlserver_key(
                &self.connection_alias,
                &database_name,
                &schema.name,
                ObjectKind::Schema,
                &schema.name,
                None,
            );
            if schema_keys
                .insert(schema.name.clone(), key.clone())
                .is_some()
            {
                return Err(CatalogError::Mapping(format!(
                    "duplicate SQL Server schema '{}'",
                    schema.name
                )));
            }
            insert_unique_id(&mut schema_id_keys, schema.id, &key, "schema")?;
            schema_owner_ids.insert(schema.name.clone(), schema.principal_id);
            schemas.push(SchemaObject {
                key: key.clone(),
                database_key: database_key.clone(),
                name: schema.name.clone(),
            });
            add_owned_by(&mut metadata, &key, schema.principal_id, &principal_keys)?;
        }

        let mut object_keys = BTreeMap::<i32, ObjectKey>::new();
        let mut name_keys = BTreeMap::<(String, String), ObjectKey>::new();
        let mut type_keys = BTreeMap::<i32, ObjectKey>::new();
        let mut table_type_keys = BTreeMap::<i32, ObjectKey>::new();
        let mut table_type_user_ids = BTreeMap::<i32, i32>::new();
        for data_type in &raw.user_types {
            let schema_key =
                required_key(&schema_keys, &data_type.schema, "user-defined type schema")?;
            let key = sqlserver_key(
                &self.connection_alias,
                &database_name,
                &data_type.schema,
                ObjectKind::UserDefinedType,
                &data_type.name,
                None,
            );
            if type_keys.insert(data_type.id, key.clone()).is_some() {
                return Err(CatalogError::Mapping(format!(
                    "duplicate user-defined type id {}",
                    data_type.id
                )));
            }
            if let Some(table_object_id) = data_type.table_object_id {
                insert_unique_id(
                    &mut table_type_keys,
                    table_object_id,
                    &key,
                    "table type object",
                )?;
                if object_keys.insert(table_object_id, key.clone()).is_some() {
                    return Err(CatalogError::Mapping(format!(
                        "duplicate SQL Server object id {table_object_id} for table type '{}.{}'",
                        data_type.schema, data_type.name
                    )));
                }
                if table_type_user_ids
                    .insert(table_object_id, data_type.id)
                    .is_some()
                {
                    return Err(CatalogError::Mapping(format!(
                        "duplicate SQL Server table type object id {table_object_id}"
                    )));
                }
            }
            let mut properties = BTreeMap::new();
            insert_i64(
                &mut properties,
                "system_type_id",
                i64::from(data_type.system_type_id),
            );
            insert_string(&mut properties, "base_type", &data_type.base_type);
            insert_i64(
                &mut properties,
                "max_length",
                i64::from(data_type.max_length),
            );
            insert_i64(&mut properties, "precision", i64::from(data_type.precision));
            insert_i64(&mut properties, "scale", i64::from(data_type.scale));
            insert_optional_string(&mut properties, "collation", data_type.collation.as_deref());
            insert_bool(&mut properties, "nullable", data_type.nullable);
            insert_bool(&mut properties, "user_defined", data_type.user_defined);
            insert_bool(&mut properties, "table_type", data_type.table_type);
            insert_bool(
                &mut properties,
                "memory_optimized",
                data_type.memory_optimized,
            );
            insert_optional_i64(
                &mut properties,
                "table_object_id",
                data_type.table_object_id.map(i64::from),
            );
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(schema_key.clone()),
                name: data_type.name.clone(),
                extension_kind: None,
                definition: None,
                properties,
            });
        }

        let xml_collection_keys = map_xml_schema_collections(
            &mut metadata,
            &self.connection_alias,
            &database_name,
            &raw.xml_schema_collections,
            &schema_keys,
            &schema_owner_ids,
            &principal_keys,
        )?;

        let mut sequence_keys = BTreeMap::<i32, ObjectKey>::new();
        for sequence in &raw.sequences {
            let schema_key = required_key(&schema_keys, &sequence.schema, "sequence schema")?;
            let key = sqlserver_key(
                &self.connection_alias,
                &database_name,
                &sequence.schema,
                ObjectKind::Sequence,
                &sequence.name,
                None,
            );
            insert_object_identity(
                &mut sequence_keys,
                &mut object_keys,
                &mut name_keys,
                sequence.id,
                &sequence.schema,
                &sequence.name,
                &key,
                "sequence",
            )?;
            let mut properties = BTreeMap::new();
            insert_string(
                &mut properties,
                "data_type",
                &qualified_type_name(&sequence.type_schema, &sequence.type_name),
            );
            insert_i64(&mut properties, "precision", i64::from(sequence.precision));
            insert_i64(&mut properties, "scale", i64::from(sequence.scale));
            insert_string(&mut properties, "start_value", &sequence.start_value);
            insert_string(&mut properties, "increment", &sequence.increment);
            insert_string(&mut properties, "minimum_value", &sequence.minimum_value);
            insert_string(&mut properties, "maximum_value", &sequence.maximum_value);
            insert_bool(&mut properties, "cyclic", sequence.cyclic);
            insert_optional_i64(
                &mut properties,
                "cache_size",
                sequence.cache_size.map(i64::from),
            );
            insert_bool(&mut properties, "exhausted", sequence.exhausted);
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(schema_key.clone()),
                name: sequence.name.clone(),
                extension_kind: None,
                definition: None,
                properties,
            });
            if let Some(type_key) = type_keys.get(&sequence.type_id) {
                add_relationship(
                    &mut metadata,
                    MetadataRelationshipKind::UsesType,
                    &key,
                    type_key,
                    None,
                    BTreeMap::new(),
                );
            }
            add_effective_owner(
                &mut metadata,
                &key,
                sequence.principal_id,
                &sequence.schema,
                &schema_owner_ids,
                &principal_keys,
            )?;
        }

        let mut tables = Vec::new();
        let mut table_keys = BTreeMap::<i32, ObjectKey>::new();
        for table in &raw.tables {
            let schema_key = required_key(&schema_keys, &table.schema, "table schema")?;
            let key = sqlserver_key(
                &self.connection_alias,
                &database_name,
                &table.schema,
                ObjectKind::Table,
                &table.name,
                None,
            );
            insert_object_identity(
                &mut table_keys,
                &mut object_keys,
                &mut name_keys,
                table.id,
                &table.schema,
                &table.name,
                &key,
                "table",
            )?;
            tables.push(TableObject {
                key: key.clone(),
                schema_key: schema_key.clone(),
                name: table.name.clone(),
                kind: if table.external {
                    TableKind::Foreign
                } else {
                    TableKind::BaseTable
                },
            });
            add_table_annotation(&mut metadata, &key, table);
            add_effective_owner(
                &mut metadata,
                &key,
                table.principal_id,
                &table.schema,
                &schema_owner_ids,
                &principal_keys,
            )?;
        }

        let mut views = Vec::<ViewObject>::new();
        let mut view_keys = BTreeMap::<i32, ObjectKey>::new();
        for view in &raw.views {
            let schema_key = required_key(&schema_keys, &view.schema, "view schema")?;
            if view.indexed {
                let key = sqlserver_key(
                    &self.connection_alias,
                    &database_name,
                    &view.schema,
                    ObjectKind::MaterializedView,
                    &view.name,
                    None,
                );
                insert_view_identity(&mut view_keys, &mut object_keys, &mut name_keys, view, &key)?;
                metadata.objects.push(MetadataObject {
                    key: key.clone(),
                    parent_key: Some(schema_key.clone()),
                    name: view.name.clone(),
                    extension_kind: None,
                    definition: view.definition.clone(),
                    properties: view_properties(view),
                });
                add_effective_owner(
                    &mut metadata,
                    &key,
                    view.principal_id,
                    &view.schema,
                    &schema_owner_ids,
                    &principal_keys,
                )?;
            } else {
                let key = sqlserver_key(
                    &self.connection_alias,
                    &database_name,
                    &view.schema,
                    ObjectKind::View,
                    &view.name,
                    None,
                );
                insert_view_identity(&mut view_keys, &mut object_keys, &mut name_keys, view, &key)?;
                views.push(ViewObject {
                    key: key.clone(),
                    schema_key: schema_key.clone(),
                    name: view.name.clone(),
                    definition: view.definition.clone(),
                    depends_on: Vec::new(),
                });
                add_annotation(&mut metadata, &key, None, view_properties(view));
                add_effective_owner(
                    &mut metadata,
                    &key,
                    view.principal_id,
                    &view.schema,
                    &schema_owner_ids,
                    &principal_keys,
                )?;
            }
        }

        let mut routines = Vec::<RoutineObject>::new();
        let mut routine_keys = BTreeMap::<i32, ObjectKey>::new();
        for routine in &raw.routines {
            let schema_key = required_key(&schema_keys, &routine.schema, "routine schema")?;
            let key = sqlserver_key(
                &self.connection_alias,
                &database_name,
                &routine.schema,
                ObjectKind::Routine,
                &routine.name,
                None,
            );
            insert_object_identity(
                &mut routine_keys,
                &mut object_keys,
                &mut name_keys,
                routine.id,
                &routine.schema,
                &routine.name,
                &key,
                "routine",
            )?;
            routines.push(RoutineObject {
                key: key.clone(),
                schema_key: schema_key.clone(),
                name: routine.name.clone(),
                kind: routine_kind(&routine.type_code)?,
                definition: routine.definition.clone(),
                depends_on: Vec::new(),
            });
            add_annotation(&mut metadata, &key, None, routine_properties(routine));
            add_effective_owner(
                &mut metadata,
                &key,
                routine.principal_id,
                &routine.schema,
                &schema_owner_ids,
                &principal_keys,
            )?;
        }

        let mut triggers = Vec::<TriggerObject>::new();
        let mut trigger_keys = BTreeMap::<i32, ObjectKey>::new();
        for trigger in &raw.triggers {
            let properties = trigger_properties(trigger);
            if trigger.parent_class == 1 {
                let parent_key = object_keys
                    .get(&trigger.parent_id)
                    .cloned()
                    .ok_or_else(|| {
                        CatalogError::Mapping(format!(
                            "trigger '{}' references missing parent object {}",
                            trigger.name, trigger.parent_id
                        ))
                    })?;
                if matches!(parent_key.object_kind, ObjectKind::Table | ObjectKind::View) {
                    let key = sqlserver_key(
                        &self.connection_alias,
                        &database_name,
                        &parent_key.schema,
                        ObjectKind::Trigger,
                        &parent_key.object_name,
                        Some(trigger.name.clone()),
                    );
                    insert_unique_id(&mut trigger_keys, trigger.id, &key, "trigger")?;
                    object_keys.insert(trigger.id, key.clone());
                    triggers.push(TriggerObject {
                        key: key.clone(),
                        table_key: parent_key,
                        name: trigger.name.clone(),
                        timing: Some(if trigger.instead_of {
                            "INSTEAD OF".to_owned()
                        } else {
                            "AFTER".to_owned()
                        }),
                        events: trigger.events.clone(),
                        definition: trigger.definition.clone(),
                        executes_routine_key: None,
                    });
                    add_annotation(&mut metadata, &key, None, properties);
                } else {
                    let key = metadata_trigger_key(&self.connection_alias, &database_name, trigger);
                    insert_unique_id(&mut trigger_keys, trigger.id, &key, "trigger")?;
                    object_keys.insert(trigger.id, key.clone());
                    metadata.objects.push(MetadataObject {
                        key,
                        parent_key: Some(parent_key),
                        name: trigger.name.clone(),
                        extension_kind: None,
                        definition: trigger.definition.clone(),
                        properties,
                    });
                }
            } else if trigger.parent_class == 0 {
                let key = metadata_trigger_key(&self.connection_alias, &database_name, trigger);
                insert_unique_id(&mut trigger_keys, trigger.id, &key, "database trigger")?;
                object_keys.insert(trigger.id, key.clone());
                metadata.objects.push(MetadataObject {
                    key,
                    parent_key: Some(database_key.clone()),
                    name: trigger.name.clone(),
                    extension_kind: None,
                    definition: trigger.definition.clone(),
                    properties,
                });
            } else {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "trigger '{}' has unsupported parent class {}",
                    trigger.name, trigger.parent_class
                )));
            }
        }

        let mut columns = Vec::<ColumnObject>::new();
        let mut column_keys = BTreeMap::<(i32, i32), ObjectKey>::new();
        let mut table_type_property_column_keys = BTreeMap::<(i32, i32), ObjectKey>::new();
        let mut dependency_source_keys = BTreeMap::<i32, ObjectKey>::new();
        for column in &raw.columns {
            let parent_key = object_keys.get(&column.object_id).cloned().ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "column '{}.{}.{}' references an unmapped parent",
                    column.schema, column.relation, column.name
                ))
            })?;
            let key = if parent_key.object_kind == ObjectKind::Table {
                let key = sqlserver_key(
                    &self.connection_alias,
                    &database_name,
                    &column.schema,
                    ObjectKind::Column,
                    &column.relation,
                    Some(column.name.clone()),
                );
                columns.push(ColumnObject {
                    key: key.clone(),
                    table_key: parent_key.clone(),
                    name: column.name.clone(),
                    ordinal_position: positive_u32(column.id, "column ordinal")?,
                    data_type: qualified_type_name(&column.type_schema, &column.type_name),
                    is_nullable: column.nullable,
                    default_value: column.default_definition.clone(),
                    is_generated: column.computed
                        || !column
                            .generated_always
                            .eq_ignore_ascii_case("NOT_APPLICABLE"),
                });
                add_annotation(&mut metadata, &key, None, column_properties(column));
                key
            } else if matches!(
                parent_key.object_kind,
                ObjectKind::View | ObjectKind::MaterializedView
            ) {
                let key = sqlserver_key(
                    &self.connection_alias,
                    &database_name,
                    &column.schema,
                    ObjectKind::ViewColumn,
                    &column.relation,
                    Some(column.name.clone()),
                );
                metadata.objects.push(MetadataObject {
                    key: key.clone(),
                    parent_key: Some(parent_key.clone()),
                    name: column.name.clone(),
                    extension_kind: None,
                    definition: None,
                    properties: column_properties(column),
                });
                key
            } else if parent_key.object_kind == ObjectKind::UserDefinedType
                && table_type_keys.contains_key(&column.object_id)
            {
                let key = sqlserver_key(
                    &self.connection_alias,
                    &database_name,
                    &column.schema,
                    ObjectKind::Extension,
                    &parent_key.object_name,
                    Some(format!("table-type-column:{}:{}", column.id, column.name)),
                );
                let mut properties = column_properties(column);
                insert_i64(
                    &mut properties,
                    "ordinal_position",
                    i64::from(positive_u32(column.id, "table type column ordinal")?),
                );
                insert_string(
                    &mut properties,
                    "data_type",
                    &qualified_type_name(&column.type_schema, &column.type_name),
                );
                metadata.objects.push(MetadataObject {
                    key: key.clone(),
                    parent_key: Some(parent_key.clone()),
                    name: column.name.clone(),
                    extension_kind: Some("sqlserver_table_type_column".to_owned()),
                    definition: column
                        .computed_definition
                        .clone()
                        .or_else(|| column.default_definition.clone()),
                    properties,
                });
                key
            } else {
                return Err(CatalogError::Mapping(format!(
                    "column parent '{}' has unsupported kind {}",
                    parent_key.object_name, parent_key.object_kind
                )));
            };
            if column_keys
                .insert((column.object_id, column.id), key.clone())
                .is_some()
            {
                return Err(CatalogError::Mapping(format!(
                    "duplicate column identity {}:{}",
                    column.object_id, column.id
                )));
            }
            if let Some(user_type_id) = table_type_user_ids.get(&column.object_id) {
                if table_type_property_column_keys
                    .insert((*user_type_id, column.id), key.clone())
                    .is_some()
                {
                    return Err(CatalogError::Mapping(format!(
                        "duplicate table type property column identity {}:{}",
                        user_type_id, column.id
                    )));
                }
            }
            if column.default_object_id > 0
                && dependency_source_keys
                    .insert(column.default_object_id, key.clone())
                    .is_some()
            {
                return Err(CatalogError::Mapping(format!(
                    "default constraint id {} maps to multiple columns",
                    column.default_object_id
                )));
            }
            if let Some(type_key) = type_keys.get(&column.type_id) {
                add_relationship(
                    &mut metadata,
                    if key.object_kind == ObjectKind::Extension {
                        MetadataRelationshipKind::DependsOn
                    } else {
                        MetadataRelationshipKind::UsesType
                    },
                    &key,
                    type_key,
                    None,
                    BTreeMap::new(),
                );
            }
            if column.xml_collection_id > 0 {
                let collection_key = xml_collection_keys
                    .get(&column.xml_collection_id)
                    .ok_or_else(|| {
                        CatalogError::Mapping(format!(
                            "column '{}.{}.{}' references missing XML schema collection {}",
                            column.schema, column.relation, column.name, column.xml_collection_id
                        ))
                    })?;
                add_relationship(
                    &mut metadata,
                    MetadataRelationshipKind::Extension("uses_xml_schema_collection".to_owned()),
                    &key,
                    collection_key,
                    None,
                    BTreeMap::new(),
                );
            }
        }

        let mut constraints = Vec::<ConstraintObject>::new();
        let mut constraint_keys = BTreeMap::<i32, ObjectKey>::new();
        for constraint in &raw.constraints {
            let parent_key = object_keys
                .get(&constraint.table_id)
                .cloned()
                .ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "constraint '{}.{}.{}' references an unmapped parent",
                        constraint.schema, constraint.table, constraint.name
                    ))
                })?;
            require_contiguous_ordinals(
                constraint.columns.iter().map(|column| column.ordinal),
                &format!("constraint '{}.{}'", constraint.table, constraint.name),
            )?;
            let resolved_columns = constraint
                .columns
                .iter()
                .map(|column| {
                    column_keys
                        .get(&(constraint.table_id, column.column_id))
                        .cloned()
                        .ok_or_else(|| {
                            CatalogError::Mapping(format!(
                                "constraint '{}.{}' lost column '{}'",
                                constraint.table, constraint.name, column.name
                            ))
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let (referenced_table_key, referenced_columns) = if constraint.kind
                == ConstraintKind::ForeignKey
            {
                if parent_key.object_kind != ObjectKind::Table {
                    return Err(CatalogError::UnsupportedMetadata(format!(
                        "table type constraint '{}' unexpectedly declares a foreign key",
                        constraint.name
                    )));
                }
                let target_id = constraint.referenced_table_id.ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "foreign key '{}' has no referenced table id",
                        constraint.name
                    ))
                })?;
                let target = table_keys.get(&target_id).cloned().ok_or_else(|| {
                    CatalogError::InvalidScope(format!(
                        "foreign key '{}.{}.{}' references a table outside the selected schema scope; include schema '{}'",
                        constraint.schema,
                        constraint.table,
                        constraint.name,
                        constraint.referenced_schema.as_deref().unwrap_or("unknown")
                    ))
                })?;
                let targets = constraint
                    .columns
                    .iter()
                    .map(|column| {
                        let target_column_id = column.referenced_column_id.ok_or_else(|| {
                            CatalogError::Mapping(format!(
                                "foreign key '{}' lacks a referenced column id",
                                constraint.name
                            ))
                        })?;
                        column_keys
                            .get(&(target_id, target_column_id))
                            .cloned()
                            .ok_or_else(|| {
                                CatalogError::Mapping(format!(
                                    "foreign key '{}' lost referenced column '{}'",
                                    constraint.name,
                                    column.referenced_name.as_deref().unwrap_or("unknown")
                                ))
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                (Some(target), targets)
            } else {
                (None, Vec::new())
            };
            let table_constraint = parent_key.object_kind == ObjectKind::Table;
            let key = if table_constraint {
                sqlserver_key(
                    &self.connection_alias,
                    &database_name,
                    &constraint.schema,
                    constraint_object_kind(constraint.kind),
                    &constraint.table,
                    Some(constraint.name.clone()),
                )
            } else if parent_key.object_kind == ObjectKind::UserDefinedType
                && table_type_keys.contains_key(&constraint.table_id)
            {
                sqlserver_key(
                    &self.connection_alias,
                    &database_name,
                    &constraint.schema,
                    ObjectKind::Extension,
                    &parent_key.object_name,
                    Some(format!(
                        "table-type-constraint:{}:{}",
                        constraint.id, constraint.name
                    )),
                )
            } else {
                return Err(CatalogError::Mapping(format!(
                    "constraint '{}' has unsupported parent kind {}",
                    constraint.name, parent_key.object_kind
                )));
            };
            insert_unique_id(&mut constraint_keys, constraint.id, &key, "constraint")?;
            object_keys.insert(constraint.id, key.clone());
            if table_constraint {
                constraints.push(ConstraintObject {
                    key: key.clone(),
                    table_key: parent_key,
                    name: constraint.name.clone(),
                    kind: constraint.kind,
                    columns: resolved_columns,
                    referenced_table_key,
                    referenced_columns,
                    expression: constraint.expression.clone(),
                });
                add_annotation(&mut metadata, &key, None, constraint_properties(constraint));
            } else {
                let mut properties = constraint_properties(constraint);
                insert_string(
                    &mut properties,
                    "constraint_kind",
                    &constraint_object_kind(constraint.kind).to_string(),
                );
                properties.insert(
                    "columns".to_owned(),
                    MetadataValue::StringList(
                        resolved_columns.iter().map(ObjectKey::to_string).collect(),
                    ),
                );
                metadata.objects.push(MetadataObject {
                    key: key.clone(),
                    parent_key: Some(parent_key),
                    name: constraint.name.clone(),
                    extension_kind: Some("sqlserver_table_type_constraint".to_owned()),
                    definition: constraint.expression.clone(),
                    properties,
                });
                for (ordinal, column_key) in resolved_columns.iter().enumerate() {
                    add_relationship(
                        &mut metadata,
                        MetadataRelationshipKind::Extension(
                            "table_type_constraint_column".to_owned(),
                        ),
                        &key,
                        column_key,
                        Some((ordinal + 1) as u32),
                        BTreeMap::new(),
                    );
                }
            }
        }

        let mut indexes = Vec::<IndexObject>::new();
        let mut index_keys = BTreeMap::<(i32, i32), ObjectKey>::new();
        for index in &raw.indexes {
            let parent_key = object_keys.get(&index.object_id).cloned().ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "index '{}.{}.{}' references missing relation",
                    index.schema, index.relation, index.name
                ))
            })?;
            let mut key_columns = index
                .columns
                .iter()
                .filter(|column| column.key_ordinal > 0)
                .collect::<Vec<_>>();
            if key_columns.is_empty() {
                key_columns = index.columns.iter().collect();
            }
            key_columns.sort_by_key(|column| {
                if column.key_ordinal > 0 {
                    column.key_ordinal
                } else {
                    column.index_column_id
                }
            });
            let resolved_columns = key_columns
                .iter()
                .map(|column| {
                    column_keys
                        .get(&(index.object_id, column.column_id))
                        .cloned()
                        .ok_or_else(|| {
                            CatalogError::Mapping(format!(
                                "index '{}.{}' lost column '{}'",
                                index.relation, index.name, column.name
                            ))
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            if parent_key.object_kind == ObjectKind::Table {
                let key = sqlserver_key(
                    &self.connection_alias,
                    &database_name,
                    &index.schema,
                    ObjectKind::Index,
                    &index.relation,
                    Some(index.name.clone()),
                );
                if index_keys
                    .insert((index.object_id, index.id), key.clone())
                    .is_some()
                {
                    return Err(CatalogError::Mapping(format!(
                        "duplicate index identity {}:{}",
                        index.object_id, index.id
                    )));
                }
                indexes.push(IndexObject {
                    key: key.clone(),
                    table_key: parent_key,
                    name: index.name.clone(),
                    columns: resolved_columns,
                    is_unique: index.unique,
                    is_primary: index.primary,
                    predicate: index.filter.clone(),
                    expression: None,
                });
                add_annotation(&mut metadata, &key, None, index_properties(index));
                add_included_column_relationships(&mut metadata, &key, index, &column_keys)?;
            } else if parent_key.object_kind == ObjectKind::MaterializedView {
                let key = sqlserver_key(
                    &self.connection_alias,
                    &database_name,
                    &index.schema,
                    ObjectKind::Index,
                    &index.relation,
                    Some(index.name.clone()),
                );
                if index_keys
                    .insert((index.object_id, index.id), key.clone())
                    .is_some()
                {
                    return Err(CatalogError::Mapping(format!(
                        "duplicate indexed-view index identity {}:{}",
                        index.object_id, index.id
                    )));
                }
                let mut properties = index_properties(index);
                properties.insert(
                    "key_columns".to_owned(),
                    MetadataValue::StringList(
                        resolved_columns.iter().map(ObjectKey::to_string).collect(),
                    ),
                );
                metadata.objects.push(MetadataObject {
                    key: key.clone(),
                    parent_key: Some(parent_key),
                    name: index.name.clone(),
                    extension_kind: None,
                    definition: index.filter.clone(),
                    properties,
                });
                add_included_column_relationships(&mut metadata, &key, index, &column_keys)?;
            } else if parent_key.object_kind == ObjectKind::UserDefinedType
                && table_type_keys.contains_key(&index.object_id)
            {
                let key = sqlserver_key(
                    &self.connection_alias,
                    &database_name,
                    &index.schema,
                    ObjectKind::Extension,
                    &parent_key.object_name,
                    Some(format!("table-type-index:{}:{}", index.id, index.name)),
                );
                if index_keys
                    .insert((index.object_id, index.id), key.clone())
                    .is_some()
                {
                    return Err(CatalogError::Mapping(format!(
                        "duplicate table type index identity {}:{}",
                        index.object_id, index.id
                    )));
                }
                let mut properties = index_properties(index);
                properties.insert(
                    "key_columns".to_owned(),
                    MetadataValue::StringList(
                        resolved_columns.iter().map(ObjectKey::to_string).collect(),
                    ),
                );
                metadata.objects.push(MetadataObject {
                    key: key.clone(),
                    parent_key: Some(parent_key),
                    name: index.name.clone(),
                    extension_kind: Some("sqlserver_table_type_index".to_owned()),
                    definition: index.filter.clone(),
                    properties,
                });
                for column in &index.columns {
                    let column_key = column_keys
                        .get(&(index.object_id, column.column_id))
                        .ok_or_else(|| {
                            CatalogError::Mapping(format!(
                                "table type index '{}.{}' lost column '{}'",
                                index.relation, index.name, column.name
                            ))
                        })?;
                    let mut relationship_properties = BTreeMap::new();
                    insert_bool(
                        &mut relationship_properties,
                        "descending",
                        column.descending,
                    );
                    insert_bool(&mut relationship_properties, "included", column.included);
                    add_relationship(
                        &mut metadata,
                        MetadataRelationshipKind::Extension("table_type_index_column".to_owned()),
                        &key,
                        column_key,
                        Some(positive_u32(
                            column.index_column_id,
                            "table type index column ordinal",
                        )?),
                        relationship_properties,
                    );
                }
            } else {
                return Err(CatalogError::Mapping(format!(
                    "index '{}' has unsupported parent kind {}",
                    index.name, parent_key.object_kind
                )));
            }
        }

        let mut parameter_keys = BTreeMap::<(i32, i32), ObjectKey>::new();
        for parameter in &raw.parameters {
            let routine_key = routine_keys
                .get(&parameter.object_id)
                .cloned()
                .ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "parameter '{}:{}' references missing routine",
                        parameter.object_id, parameter.id
                    ))
                })?;
            let key = sqlserver_key(
                &self.connection_alias,
                &database_name,
                &routine_key.schema,
                ObjectKind::RoutineParameter,
                &routine_key.object_name,
                Some(format!("{}:{}", parameter.id, parameter.name)),
            );
            if parameter_keys
                .insert((parameter.object_id, parameter.id), key.clone())
                .is_some()
            {
                return Err(CatalogError::Mapping(format!(
                    "duplicate routine parameter identity {}:{}",
                    parameter.object_id, parameter.id
                )));
            }
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(routine_key.clone()),
                name: parameter.name.clone(),
                extension_kind: None,
                definition: None,
                properties: parameter_properties(parameter),
            });
            add_relationship(
                &mut metadata,
                MetadataRelationshipKind::HasParameter,
                &routine_key,
                &key,
                Some(positive_u32_or_return(parameter.id)?),
                BTreeMap::new(),
            );
            if let Some(type_key) = type_keys.get(&parameter.type_id) {
                add_relationship(
                    &mut metadata,
                    if parameter.id == 0 {
                        MetadataRelationshipKind::ReturnsType
                    } else {
                        MetadataRelationshipKind::UsesType
                    },
                    if parameter.id == 0 {
                        &routine_key
                    } else {
                        &key
                    },
                    type_key,
                    None,
                    BTreeMap::new(),
                );
            }
            if parameter.xml_collection_id > 0 {
                let collection_key = xml_collection_keys
                    .get(&parameter.xml_collection_id)
                    .ok_or_else(|| {
                        CatalogError::Mapping(format!(
                            "routine parameter '{}:{}' references missing XML schema collection {}",
                            parameter.object_id, parameter.id, parameter.xml_collection_id
                        ))
                    })?;
                add_relationship(
                    &mut metadata,
                    MetadataRelationshipKind::Extension("uses_xml_schema_collection".to_owned()),
                    if parameter.id == 0 {
                        &routine_key
                    } else {
                        &key
                    },
                    collection_key,
                    None,
                    BTreeMap::new(),
                );
            }
        }

        let mut synonym_keys = BTreeMap::<i32, ObjectKey>::new();
        for synonym in &raw.synonyms {
            let schema_key = required_key(&schema_keys, &synonym.schema, "synonym schema")?;
            let key = sqlserver_key(
                &self.connection_alias,
                &database_name,
                &synonym.schema,
                ObjectKind::Synonym,
                &synonym.name,
                None,
            );
            insert_unique_id(&mut synonym_keys, synonym.id, &key, "synonym")?;
            object_keys.insert(synonym.id, key.clone());
            name_keys.insert((synonym.schema.clone(), synonym.name.clone()), key.clone());
            let mut properties = BTreeMap::new();
            insert_string(
                &mut properties,
                "base_object_name",
                &synonym.base_object_name,
            );
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(schema_key.clone()),
                name: synonym.name.clone(),
                extension_kind: None,
                definition: None,
                properties,
            });
            add_effective_owner(
                &mut metadata,
                &key,
                synonym.principal_id,
                &synonym.schema,
                &schema_owner_ids,
                &principal_keys,
            )?;
        }

        let mut policy_keys = BTreeMap::<i32, ObjectKey>::new();
        for policy in &raw.security_policies {
            let schema_key = required_key(&schema_keys, &policy.schema, "security policy schema")?;
            let key = sqlserver_key(
                &self.connection_alias,
                &database_name,
                &policy.schema,
                ObjectKind::Policy,
                &policy.name,
                None,
            );
            insert_unique_id(&mut policy_keys, policy.id, &key, "security policy")?;
            object_keys.insert(policy.id, key.clone());
            let mut properties = BTreeMap::new();
            insert_bool(&mut properties, "enabled", policy.enabled);
            insert_bool(&mut properties, "schema_bound", policy.schema_bound);
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(schema_key.clone()),
                name: policy.name.clone(),
                extension_kind: None,
                definition: None,
                properties,
            });
            add_effective_owner(
                &mut metadata,
                &key,
                policy.principal_id,
                &policy.schema,
                &schema_owner_ids,
                &principal_keys,
            )?;
            for predicate in &policy.predicates {
                let predicate_key = sqlserver_key(
                    &self.connection_alias,
                    &database_name,
                    &policy.schema,
                    ObjectKind::Extension,
                    &policy.name,
                    Some(format!("predicate:{}", predicate.id)),
                );
                let mut properties = BTreeMap::new();
                insert_string(&mut properties, "predicate_type", &predicate.predicate_type);
                insert_optional_string(
                    &mut properties,
                    "operation",
                    predicate.operation.as_deref(),
                );
                insert_i64(
                    &mut properties,
                    "definition_bytes",
                    i64::from(predicate.definition_bytes),
                );
                metadata.objects.push(MetadataObject {
                    key: predicate_key.clone(),
                    parent_key: Some(key.clone()),
                    name: format!("{} predicate {}", policy.name, predicate.id),
                    extension_kind: Some("sqlserver_security_predicate".to_owned()),
                    definition: Some(predicate.definition.clone()),
                    properties,
                });
                let target_key = table_keys
                    .get(&predicate.target_object_id)
                    .or_else(|| view_keys.get(&predicate.target_object_id))
                    .cloned()
                    .ok_or_else(|| {
                        CatalogError::InvalidScope(format!(
                            "security policy '{}.{}' targets an object outside the selected schema scope",
                            policy.schema, policy.name
                        ))
                    })?;
                add_relationship(
                    &mut metadata,
                    MetadataRelationshipKind::Extension("security_predicate_applies_to".to_owned()),
                    &predicate_key,
                    &target_key,
                    None,
                    BTreeMap::new(),
                );
            }
        }

        let partition_mapping = map_partitions(
            &mut metadata,
            &self.connection_alias,
            &database_name,
            &database_key,
            &raw,
            &object_keys,
            &index_keys,
        )?;

        map_extended_properties(
            &mut metadata,
            &self.connection_alias,
            &database_name,
            &raw.extended_properties,
            ExtendedPropertyTargetRegistry {
                database: &database_key,
                schemas: &schema_id_keys,
                principals: &principal_keys,
                objects: &object_keys,
                columns: &column_keys,
                table_type_columns: &table_type_property_column_keys,
                parameters: &parameter_keys,
                user_types: &type_keys,
                indexes: &index_keys,
                xml_collections: &xml_collection_keys,
                partition_schemes: &partition_mapping.scheme_keys,
                partition_functions: &partition_mapping.function_keys,
            },
        )?;

        let mut external_reference_keys = BTreeMap::<String, ObjectKey>::new();
        map_synonym_targets(
            &mut metadata,
            &database_name,
            &raw.synonyms,
            &synonym_keys,
            &name_keys,
            &mut external_reference_keys,
            &self.connection_alias,
            &database_key,
        )?;
        let dependency_result = map_dependencies(
            &mut metadata,
            &raw.dependencies,
            &object_keys,
            &column_keys,
            &dependency_source_keys,
            &type_keys,
            &xml_collection_keys,
            &index_keys,
            &partition_mapping.function_keys,
            &name_keys,
            &mut external_reference_keys,
            &self.connection_alias,
            &database_name,
            &database_key,
        )?;

        let view_positions = raw
            .views
            .iter()
            .filter(|view| !view.indexed)
            .enumerate()
            .map(|(position, view)| (view.id, position))
            .collect::<BTreeMap<_, _>>();
        for (view_id, dependencies) in &dependency_result.view_dependencies {
            let position = view_positions.get(view_id).copied().ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "dependency ledger references missing ordinary view {view_id}"
                ))
            })?;
            views[position].depends_on = dependencies.clone();
        }
        let routine_positions = raw
            .routines
            .iter()
            .enumerate()
            .map(|(position, routine)| (routine.id, position))
            .collect::<BTreeMap<_, _>>();
        for (routine_id, dependencies) in &dependency_result.routine_dependencies {
            let position = routine_positions.get(routine_id).copied().ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "dependency ledger references missing routine {routine_id}"
                ))
            })?;
            routines[position].depends_on = dependencies.clone();
        }

        let projection_ledger = SqlServerProjectionLedger {
            external_reference_objects: external_reference_keys.len() as u64,
            view_dependencies: dependency_result.view_dependency_count(),
            routine_dependencies: dependency_result.routine_dependency_count(),
            dependency_metadata_relationships: dependency_result.metadata_relationship_count,
        };

        add_principal_memberships(&mut metadata, &raw.principals, &principal_keys)?;
        validate_relationship_uniqueness(&metadata.relationships)?;

        let snapshot = CanonicalSchemaSnapshot {
            schema: SchemaSnapshot {
                source_kind: SQLSERVER_SOURCE.to_owned(),
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
                capabilities: sqlserver_capabilities(),
            },
            metadata,
        };
        let discovered_counts = discovery_counts_from_catalog(&raw, &snapshot, projection_ledger)?;
        Ok(CatalogDiscovery {
            snapshot,
            adapter: AdapterIdentity {
                name: "database-memory-sqlserver-catalog".to_owned(),
                version: SQLSERVER_ADAPTER_VERSION.to_owned(),
            },
            server: ServerIdentity {
                product: "Microsoft SQL Server".to_owned(),
                version: self.facts.version.clone(),
            },
            scope: IntrospectionScope {
                catalogs: vec![database_name],
                schemas: raw
                    .schemas
                    .iter()
                    .map(|schema| schema.name.clone())
                    .collect(),
            },
            discovered_counts,
            capability_checks: sqlserver_capability_checks(&self.facts, self.strategy),
        })
    }
}

fn validate_raw_inventory(raw: &RawSqlServerCatalog) -> Result<(), CatalogError> {
    require_unique(raw.schemas.iter().map(|schema| schema.id), "schema id")?;
    require_unique(
        raw.principals.iter().map(|principal| principal.id),
        "principal id",
    )?;
    require_unique(raw.tables.iter().map(|table| table.id), "table id")?;
    require_unique(
        raw.columns
            .iter()
            .map(|column| (column.object_id, column.id)),
        "column identity",
    )?;
    require_unique(raw.views.iter().map(|view| view.id), "view id")?;
    require_unique(raw.routines.iter().map(|routine| routine.id), "routine id")?;
    require_unique(
        raw.parameters
            .iter()
            .map(|parameter| (parameter.object_id, parameter.id)),
        "routine parameter identity",
    )?;
    require_unique(raw.triggers.iter().map(|trigger| trigger.id), "trigger id")?;
    require_unique(
        raw.constraints.iter().map(|constraint| constraint.id),
        "constraint id",
    )?;
    require_unique(
        raw.user_types.iter().map(|data_type| data_type.id),
        "user-defined type id",
    )?;
    require_unique(
        raw.sequences.iter().map(|sequence| sequence.id),
        "sequence id",
    )?;
    require_unique(raw.synonyms.iter().map(|synonym| synonym.id), "synonym id")?;
    require_unique(
        raw.indexes.iter().map(|index| (index.object_id, index.id)),
        "index identity",
    )?;
    require_unique(
        raw.partition_functions.iter().map(|function| function.id),
        "partition function id",
    )?;
    require_unique(
        raw.partition_schemes.iter().map(|scheme| scheme.id),
        "partition scheme id",
    )?;
    require_unique(
        raw.security_policies.iter().map(|policy| policy.id),
        "security policy id",
    )?;
    require_unique(
        raw.security_policies.iter().flat_map(|policy| {
            policy
                .predicates
                .iter()
                .map(move |predicate| (policy.id, predicate.id))
        }),
        "security predicate identity",
    )?;
    require_unique(
        raw.xml_schema_collections
            .iter()
            .map(|collection| collection.id),
        "XML schema collection id",
    )?;
    require_unique(
        raw.xml_schema_collections.iter().flat_map(|collection| {
            collection
                .namespaces
                .iter()
                .map(move |namespace| (collection.id, namespace.id))
        }),
        "XML schema namespace identity",
    )?;
    require_unique(
        raw.extended_properties.iter().map(|property| {
            (
                property.class,
                property.major_id,
                property.minor_id,
                property.name.clone(),
            )
        }),
        "extended property identity",
    )?;
    for property in &raw.extended_properties {
        let value_is_null = property.value_type.is_none();
        let typed_fields_are_empty = property.value_precision.is_none()
            && property.value_scale.is_none()
            && property.value_max_length.is_none()
            && property.value_collation.is_none()
            && property.display_value.is_none()
            && property.value_hex.is_none();
        if value_is_null != typed_fields_are_empty {
            return Err(CatalogError::Mapping(format!(
                "extended property '{}:{}:{}:{}' has inconsistent sql_variant metadata",
                property.class, property.major_id, property.minor_id, property.name
            )));
        }
    }
    Ok(())
}

fn require_unique<T: Ord>(
    values: impl IntoIterator<Item = T>,
    subject: &str,
) -> Result<(), CatalogError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(CatalogError::Mapping(format!(
                "duplicate {subject} in raw catalog"
            )));
        }
    }
    Ok(())
}

fn sqlserver_key(
    connection_alias: &str,
    database: &str,
    schema: &str,
    object_kind: ObjectKind,
    object_name: &str,
    sub_object: Option<String>,
) -> ObjectKey {
    ObjectKey::new(
        SQLSERVER_SOURCE,
        connection_alias,
        database,
        schema,
        object_kind,
        object_name,
        sub_object,
    )
}

#[allow(clippy::too_many_arguments)]
fn insert_object_identity(
    kind_keys: &mut BTreeMap<i32, ObjectKey>,
    object_keys: &mut BTreeMap<i32, ObjectKey>,
    name_keys: &mut BTreeMap<(String, String), ObjectKey>,
    id: i32,
    schema: &str,
    name: &str,
    key: &ObjectKey,
    kind: &str,
) -> Result<(), CatalogError> {
    insert_unique_id(kind_keys, id, key, kind)?;
    if object_keys.insert(id, key.clone()).is_some() {
        return Err(CatalogError::Mapping(format!(
            "object id {id} is shared by multiple mapped objects"
        )));
    }
    if name_keys
        .insert((schema.to_owned(), name.to_owned()), key.clone())
        .is_some()
    {
        return Err(CatalogError::Mapping(format!(
            "duplicate schema object name '{schema}.{name}'"
        )));
    }
    Ok(())
}

fn insert_view_identity(
    view_keys: &mut BTreeMap<i32, ObjectKey>,
    object_keys: &mut BTreeMap<i32, ObjectKey>,
    name_keys: &mut BTreeMap<(String, String), ObjectKey>,
    view: &RawView,
    key: &ObjectKey,
) -> Result<(), CatalogError> {
    insert_object_identity(
        view_keys,
        object_keys,
        name_keys,
        view.id,
        &view.schema,
        &view.name,
        key,
        "view",
    )
}

fn insert_unique_id(
    keys: &mut BTreeMap<i32, ObjectKey>,
    id: i32,
    key: &ObjectKey,
    subject: &str,
) -> Result<(), CatalogError> {
    if keys.insert(id, key.clone()).is_some() {
        return Err(CatalogError::Mapping(format!(
            "duplicate {subject} id {id}"
        )));
    }
    Ok(())
}

fn required_key<'a>(
    keys: &'a BTreeMap<String, ObjectKey>,
    name: &str,
    subject: &str,
) -> Result<&'a ObjectKey, CatalogError> {
    keys.get(name)
        .ok_or_else(|| CatalogError::Mapping(format!("{subject} '{name}' is not mapped")))
}

fn add_database_annotation(
    metadata: &mut CanonicalMetadata,
    database_key: &ObjectKey,
    facts: &ServerFacts,
    strategy: SqlServerCatalogVersion,
) {
    let mut properties = BTreeMap::new();
    insert_string(&mut properties, "server_version", &facts.version);
    insert_i64(&mut properties, "server_major", i64::from(facts.major));
    insert_i64(
        &mut properties,
        "engine_edition",
        i64::from(facts.engine_edition),
    );
    insert_string(&mut properties, "edition", &facts.edition);
    insert_string(&mut properties, "current_user", &facts.current_user);
    insert_string(&mut properties, "login", &facts.login);
    insert_string(&mut properties, "original_login", &facts.original_login);
    insert_string(&mut properties, "collation", &facts.collation);
    insert_i64(
        &mut properties,
        "compatibility_level",
        i64::from(facts.compatibility_level),
    );
    insert_bool(
        &mut properties,
        "database_read_only",
        facts.database_read_only,
    );
    insert_string(&mut properties, "containment", &facts.containment);
    insert_bool(
        &mut properties,
        "encrypted_transport",
        facts.encrypted_transport,
    );
    insert_string(
        &mut properties,
        "catalog_strategy",
        strategy.strategy_name(),
    );
    add_annotation(metadata, database_key, None, properties);
}

fn add_table_annotation(metadata: &mut CanonicalMetadata, key: &ObjectKey, table: &RawTable) {
    let mut properties = BTreeMap::new();
    insert_i64(
        &mut properties,
        "lob_data_space_id",
        i64::from(table.lob_data_space_id),
    );
    insert_optional_i64(
        &mut properties,
        "filestream_data_space_id",
        table.filestream_data_space_id.map(i64::from),
    );
    insert_bool(&mut properties, "replicated", table.replicated);
    insert_bool(&mut properties, "merge_published", table.merge_published);
    insert_bool(
        &mut properties,
        "sync_transaction_subscribed",
        table.sync_tran_subscribed,
    );
    insert_bool(&mut properties, "cdc_tracked", table.cdc_tracked);
    insert_bool(
        &mut properties,
        "lock_on_bulk_load",
        table.lock_on_bulk_load,
    );
    insert_bool(&mut properties, "file_table", table.file_table);
    insert_bool(&mut properties, "memory_optimized", table.memory_optimized);
    insert_string(&mut properties, "durability", &table.durability);
    insert_string(&mut properties, "temporal_type", &table.temporal_type);
    insert_optional_string(
        &mut properties,
        "history_schema",
        table.history_schema.as_deref(),
    );
    insert_optional_string(
        &mut properties,
        "history_table",
        table.history_table.as_deref(),
    );
    insert_bool(
        &mut properties,
        "remote_data_archive",
        table.remote_data_archive,
    );
    insert_bool(&mut properties, "graph_node", table.node);
    insert_bool(&mut properties, "graph_edge", table.edge);
    insert_string(&mut properties, "ledger_type", &table.ledger_type);
    add_annotation(metadata, key, None, properties);
}

fn view_properties(view: &RawView) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_bool(&mut properties, "replicated", view.replicated);
    insert_bool(
        &mut properties,
        "replication_filter",
        view.replication_filter,
    );
    insert_bool(&mut properties, "schema_bound", view.schema_bound);
    insert_bool(&mut properties, "ansi_nulls", view.ansi_nulls);
    insert_bool(&mut properties, "quoted_identifier", view.quoted_identifier);
    insert_optional_i64(
        &mut properties,
        "execute_as_principal_id",
        view.execute_as_principal_id.map(i64::from),
    );
    insert_bool(&mut properties, "indexed", view.indexed);
    properties
}

fn routine_properties(routine: &RawRoutine) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_string(&mut properties, "type", &routine.type_code);
    insert_string(&mut properties, "type_description", &routine.type_desc);
    insert_bool(&mut properties, "schema_bound", routine.schema_bound);
    insert_bool(&mut properties, "recompiled", routine.recompiled);
    insert_bool(
        &mut properties,
        "native_compilation",
        routine.native_compilation,
    );
    insert_bool(&mut properties, "ansi_nulls", routine.ansi_nulls);
    insert_bool(
        &mut properties,
        "quoted_identifier",
        routine.quoted_identifier,
    );
    insert_optional_i64(
        &mut properties,
        "execute_as_principal_id",
        routine.execute_as_principal_id.map(i64::from),
    );
    insert_bool(
        &mut properties,
        "null_on_null_input",
        routine.null_on_null_input,
    );
    insert_bool(&mut properties, "inlineable", routine.inlineable);
    insert_bool(&mut properties, "inline_type", routine.inline_type);
    insert_bool(&mut properties, "startup", routine.startup);
    insert_bool(&mut properties, "replication", routine.replication);
    properties
}

fn trigger_properties(trigger: &RawTrigger) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_i64(
        &mut properties,
        "parent_class",
        i64::from(trigger.parent_class),
    );
    insert_bool(&mut properties, "instead_of", trigger.instead_of);
    insert_bool(&mut properties, "disabled", trigger.disabled);
    insert_bool(
        &mut properties,
        "not_for_replication",
        trigger.not_for_replication,
    );
    insert_bool(&mut properties, "schema_bound", trigger.schema_bound);
    insert_optional_i64(
        &mut properties,
        "execute_as_principal_id",
        trigger.execute_as_principal_id.map(i64::from),
    );
    properties.insert(
        "events".to_owned(),
        MetadataValue::StringList(trigger.events.clone()),
    );
    properties
}

fn metadata_trigger_key(connection_alias: &str, database: &str, trigger: &RawTrigger) -> ObjectKey {
    sqlserver_key(
        connection_alias,
        database,
        trigger.parent_schema.as_deref().unwrap_or(database),
        ObjectKind::Trigger,
        &trigger.name,
        Some(trigger.id.to_string()),
    )
}

fn column_properties(column: &RawColumn) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_i64(&mut properties, "column_id", i64::from(column.id));
    insert_string(
        &mut properties,
        "data_type",
        &qualified_type_name(&column.type_schema, &column.type_name),
    );
    insert_i64(&mut properties, "max_length", i64::from(column.max_length));
    insert_i64(&mut properties, "precision", i64::from(column.precision));
    insert_i64(&mut properties, "scale", i64::from(column.scale));
    insert_optional_string(&mut properties, "collation", column.collation.as_deref());
    insert_bool(&mut properties, "nullable", column.nullable);
    insert_bool(&mut properties, "ansi_padded", column.ansi_padded);
    insert_bool(&mut properties, "rowguid", column.rowguid);
    insert_bool(&mut properties, "identity", column.identity);
    insert_optional_string(
        &mut properties,
        "identity_seed",
        column.identity_seed.as_deref(),
    );
    insert_optional_string(
        &mut properties,
        "identity_increment",
        column.identity_increment.as_deref(),
    );
    insert_bool(&mut properties, "computed", column.computed);
    insert_optional_string(
        &mut properties,
        "computed_definition",
        column.computed_definition.as_deref(),
    );
    if let Some(persisted) = column.persisted {
        insert_bool(&mut properties, "computed_persisted", persisted);
    }
    insert_optional_string(
        &mut properties,
        "default_definition",
        column.default_definition.as_deref(),
    );
    insert_bool(&mut properties, "filestream", column.filestream);
    insert_bool(&mut properties, "replicated", column.replicated);
    insert_bool(
        &mut properties,
        "non_sql_subscribed",
        column.non_sql_subscribed,
    );
    insert_bool(&mut properties, "merge_published", column.merge_published);
    insert_bool(&mut properties, "dts_replicated", column.dts_replicated);
    insert_bool(&mut properties, "xml_document", column.xml_document);
    insert_i64(
        &mut properties,
        "xml_collection_id",
        i64::from(column.xml_collection_id),
    );
    insert_bool(&mut properties, "sparse", column.sparse);
    insert_bool(&mut properties, "column_set", column.column_set);
    insert_string(
        &mut properties,
        "generated_always",
        &column.generated_always,
    );
    insert_optional_string(
        &mut properties,
        "encryption_type",
        column.encryption_type.as_deref(),
    );
    insert_bool(&mut properties, "hidden", column.hidden);
    insert_bool(&mut properties, "masked", column.masked);
    insert_optional_string(
        &mut properties,
        "masking_function",
        column.masking_function.as_deref(),
    );
    insert_optional_string(&mut properties, "graph_type", column.graph_type.as_deref());
    properties
}

fn constraint_properties(constraint: &RawConstraint) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_bool(&mut properties, "disabled", constraint.disabled);
    insert_bool(&mut properties, "not_trusted", constraint.not_trusted);
    insert_bool(
        &mut properties,
        "not_for_replication",
        constraint.not_for_replication,
    );
    insert_optional_string(
        &mut properties,
        "delete_action",
        constraint.delete_action.as_deref(),
    );
    insert_optional_string(
        &mut properties,
        "update_action",
        constraint.update_action.as_deref(),
    );
    properties
}

fn index_properties(index: &RawIndex) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_i64(&mut properties, "index_id", i64::from(index.id));
    insert_i64(&mut properties, "type_code", i64::from(index.type_code));
    insert_string(&mut properties, "type_description", &index.type_desc);
    insert_bool(&mut properties, "unique", index.unique);
    insert_bool(&mut properties, "primary", index.primary);
    insert_bool(
        &mut properties,
        "unique_constraint",
        index.unique_constraint,
    );
    insert_bool(&mut properties, "disabled", index.disabled);
    insert_bool(&mut properties, "hypothetical", index.hypothetical);
    insert_bool(&mut properties, "padded", index.padded);
    insert_i64(&mut properties, "fill_factor", i64::from(index.fill_factor));
    insert_bool(
        &mut properties,
        "ignore_duplicate_key",
        index.ignore_duplicate_key,
    );
    insert_bool(&mut properties, "allow_row_locks", index.allow_row_locks);
    insert_bool(&mut properties, "allow_page_locks", index.allow_page_locks);
    insert_bool(&mut properties, "auto_created", index.auto_created);
    insert_i64(
        &mut properties,
        "data_space_id",
        i64::from(index.data_space_id),
    );
    properties
}

fn parameter_properties(parameter: &RawParameter) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_i64(&mut properties, "parameter_id", i64::from(parameter.id));
    insert_string(
        &mut properties,
        "data_type",
        &qualified_type_name(&parameter.type_schema, &parameter.type_name),
    );
    insert_i64(
        &mut properties,
        "max_length",
        i64::from(parameter.max_length),
    );
    insert_i64(&mut properties, "precision", i64::from(parameter.precision));
    insert_i64(&mut properties, "scale", i64::from(parameter.scale));
    insert_bool(&mut properties, "output", parameter.output);
    insert_bool(&mut properties, "readonly", parameter.readonly);
    insert_bool(&mut properties, "nullable", parameter.nullable);
    insert_optional_string(
        &mut properties,
        "default_value",
        parameter.default_value.as_deref(),
    );
    insert_i64(
        &mut properties,
        "xml_collection_id",
        i64::from(parameter.xml_collection_id),
    );
    properties
}

fn add_included_column_relationships(
    metadata: &mut CanonicalMetadata,
    index_key: &ObjectKey,
    index: &RawIndex,
    column_keys: &BTreeMap<(i32, i32), ObjectKey>,
) -> Result<(), CatalogError> {
    for column in index.columns.iter().filter(|column| column.included) {
        let column_key = column_keys
            .get(&(index.object_id, column.column_id))
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "included index column '{}.{}' is not mapped",
                    index.name, column.name
                ))
            })?;
        let mut properties = BTreeMap::new();
        insert_i64(
            &mut properties,
            "index_column_id",
            i64::from(column.index_column_id),
        );
        add_relationship(
            metadata,
            MetadataRelationshipKind::IncludesColumn,
            index_key,
            column_key,
            Some(positive_u32(
                column.index_column_id,
                "index column ordinal",
            )?),
            properties,
        );
    }
    Ok(())
}

fn routine_kind(type_code: &str) -> Result<RoutineKind, CatalogError> {
    match type_code {
        "P" => Ok(RoutineKind::Procedure),
        "FN" | "IF" | "TF" => Ok(RoutineKind::Function),
        unsupported => Err(CatalogError::UnsupportedMetadata(format!(
            "routine type '{unsupported}' is not SQL-backed"
        ))),
    }
}

fn constraint_object_kind(kind: ConstraintKind) -> ObjectKind {
    match kind {
        ConstraintKind::PrimaryKey => ObjectKind::PrimaryKey,
        ConstraintKind::ForeignKey => ObjectKind::ForeignKey,
        ConstraintKind::Unique => ObjectKind::UniqueConstraint,
        ConstraintKind::Check => ObjectKind::CheckConstraint,
    }
}

fn qualified_type_name(schema: &str, name: &str) -> String {
    if schema.eq_ignore_ascii_case("sys") {
        name.to_owned()
    } else {
        format!("{schema}.{name}")
    }
}

fn positive_u32(value: i32, subject: &str) -> Result<u32, CatalogError> {
    u32::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| CatalogError::Mapping(format!("{subject} must be a positive u32")))
}

fn positive_u32_or_return(value: i32) -> Result<u32, CatalogError> {
    if value == 0 {
        Ok(1)
    } else {
        positive_u32(value + 1, "parameter relationship ordinal")
    }
}

fn require_contiguous_ordinals(
    ordinals: impl IntoIterator<Item = i32>,
    subject: &str,
) -> Result<(), CatalogError> {
    for (position, ordinal) in ordinals.into_iter().enumerate() {
        let expected = i32::try_from(position + 1)
            .map_err(|_| CatalogError::Mapping(format!("{subject} ordinal overflow")))?;
        if ordinal != expected {
            return Err(CatalogError::Mapping(format!(
                "{subject} has ordinal {ordinal}, expected {expected}"
            )));
        }
    }
    Ok(())
}

fn add_effective_owner(
    metadata: &mut CanonicalMetadata,
    object_key: &ObjectKey,
    principal_id: Option<i32>,
    schema: &str,
    schema_owner_ids: &BTreeMap<String, i32>,
    principal_keys: &BTreeMap<i32, ObjectKey>,
) -> Result<(), CatalogError> {
    let owner = principal_id
        .or_else(|| schema_owner_ids.get(schema).copied())
        .ok_or_else(|| {
            CatalogError::Mapping(format!(
                "object '{}' has no effective owner",
                object_key.object_name
            ))
        })?;
    add_owned_by(metadata, object_key, owner, principal_keys)
}

fn add_owned_by(
    metadata: &mut CanonicalMetadata,
    object_key: &ObjectKey,
    principal_id: i32,
    principal_keys: &BTreeMap<i32, ObjectKey>,
) -> Result<(), CatalogError> {
    let principal_key = principal_keys.get(&principal_id).ok_or_else(|| {
        CatalogError::Mapping(format!(
            "object '{}' references missing principal {principal_id}",
            object_key.object_name
        ))
    })?;
    add_relationship(
        metadata,
        MetadataRelationshipKind::OwnedBy,
        object_key,
        principal_key,
        None,
        BTreeMap::new(),
    );
    Ok(())
}

fn add_principal_memberships(
    metadata: &mut CanonicalMetadata,
    principals: &[RawPrincipal],
    principal_keys: &BTreeMap<i32, ObjectKey>,
) -> Result<(), CatalogError> {
    for principal in principals {
        let Some(owner_id) = principal.owning_principal_id else {
            continue;
        };
        let source = principal_keys.get(&principal.id).ok_or_else(|| {
            CatalogError::Mapping(format!("principal {} lost its key", principal.id))
        })?;
        let owner = principal_keys.get(&owner_id).ok_or_else(|| {
            CatalogError::Mapping(format!(
                "principal '{}' references missing owner {owner_id}",
                principal.name
            ))
        })?;
        add_relationship(
            metadata,
            MetadataRelationshipKind::OwnedBy,
            source,
            owner,
            None,
            BTreeMap::new(),
        );
    }
    Ok(())
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

fn add_relationship(
    metadata: &mut CanonicalMetadata,
    kind: MetadataRelationshipKind,
    from_key: &ObjectKey,
    to_key: &ObjectKey,
    ordinal: Option<u32>,
    properties: BTreeMap<String, MetadataValue>,
) {
    metadata.relationships.push(MetadataRelationship {
        kind,
        from_key: from_key.clone(),
        to_key: to_key.clone(),
        ordinal,
        properties,
    });
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

fn insert_i64(properties: &mut BTreeMap<String, MetadataValue>, key: &str, value: i64) {
    properties.insert(key.to_owned(), MetadataValue::Integer(value));
}

fn insert_optional_i64(
    properties: &mut BTreeMap<String, MetadataValue>,
    key: &str,
    value: Option<i64>,
) {
    if let Some(value) = value {
        insert_i64(properties, key, value);
    }
}

#[allow(clippy::too_many_arguments)]
fn map_xml_schema_collections(
    metadata: &mut CanonicalMetadata,
    connection_alias: &str,
    database: &str,
    collections: &[RawXmlSchemaCollection],
    schema_keys: &BTreeMap<String, ObjectKey>,
    schema_owner_ids: &BTreeMap<String, i32>,
    principal_keys: &BTreeMap<i32, ObjectKey>,
) -> Result<BTreeMap<i32, ObjectKey>, CatalogError> {
    let mut collection_keys = BTreeMap::<i32, ObjectKey>::new();
    for collection in collections {
        let schema_key = required_key(
            schema_keys,
            &collection.schema,
            "XML schema collection schema",
        )?;
        let key = sqlserver_key(
            connection_alias,
            database,
            &collection.schema,
            ObjectKind::Extension,
            &collection.name,
            Some("xml-schema-collection".to_owned()),
        );
        insert_unique_id(
            &mut collection_keys,
            collection.id,
            &key,
            "XML schema collection",
        )?;
        let mut properties = BTreeMap::new();
        insert_i64(
            &mut properties,
            "xml_collection_id",
            i64::from(collection.id),
        );
        insert_string(&mut properties, "created_at", &collection.created_at);
        insert_string(&mut properties, "modified_at", &collection.modified_at);
        metadata.objects.push(MetadataObject {
            key: key.clone(),
            parent_key: Some(schema_key.clone()),
            name: collection.name.clone(),
            extension_kind: Some("sqlserver_xml_schema_collection".to_owned()),
            definition: None,
            properties,
        });
        add_effective_owner(
            metadata,
            &key,
            collection.principal_id,
            &collection.schema,
            schema_owner_ids,
            principal_keys,
        )?;

        for namespace in &collection.namespaces {
            let namespace_key = sqlserver_key(
                connection_alias,
                database,
                &collection.schema,
                ObjectKind::Extension,
                &collection.name,
                Some(format!("xml-schema-namespace:{}", namespace.id)),
            );
            let mut namespace_properties = BTreeMap::new();
            insert_i64(
                &mut namespace_properties,
                "xml_namespace_id",
                i64::from(namespace.id),
            );
            insert_string(&mut namespace_properties, "namespace", &namespace.name);
            metadata.objects.push(MetadataObject {
                key: namespace_key,
                parent_key: Some(key.clone()),
                name: if namespace.name.is_empty() {
                    "default namespace".to_owned()
                } else {
                    namespace.name.clone()
                },
                extension_kind: Some("sqlserver_xml_schema_namespace".to_owned()),
                definition: None,
                properties: namespace_properties,
            });
        }
    }
    Ok(collection_keys)
}

struct ExtendedPropertyTargetRegistry<'a> {
    database: &'a ObjectKey,
    schemas: &'a BTreeMap<i32, ObjectKey>,
    principals: &'a BTreeMap<i32, ObjectKey>,
    objects: &'a BTreeMap<i32, ObjectKey>,
    columns: &'a BTreeMap<(i32, i32), ObjectKey>,
    table_type_columns: &'a BTreeMap<(i32, i32), ObjectKey>,
    parameters: &'a BTreeMap<(i32, i32), ObjectKey>,
    user_types: &'a BTreeMap<i32, ObjectKey>,
    indexes: &'a BTreeMap<(i32, i32), ObjectKey>,
    xml_collections: &'a BTreeMap<i32, ObjectKey>,
    partition_schemes: &'a BTreeMap<i32, ObjectKey>,
    partition_functions: &'a BTreeMap<i32, ObjectKey>,
}

impl ExtendedPropertyTargetRegistry<'_> {
    fn resolve(&self, property: &RawExtendedProperty) -> Option<&ObjectKey> {
        match property.class {
            0 if property.major_id == 0 && property.minor_id == 0 => Some(self.database),
            1 if property.minor_id == 0 => self.objects.get(&property.major_id),
            1 => self.columns.get(&(property.major_id, property.minor_id)),
            2 => self.parameters.get(&(property.major_id, property.minor_id)),
            3 => self.schemas.get(&property.major_id),
            4 => self.principals.get(&property.major_id),
            6 => self.user_types.get(&property.major_id),
            7 => self.indexes.get(&(property.major_id, property.minor_id)),
            8 => self
                .table_type_columns
                .get(&(property.major_id, property.minor_id)),
            10 => self.xml_collections.get(&property.major_id),
            20 => self.partition_schemes.get(&property.major_id),
            21 => self.partition_functions.get(&property.major_id),
            _ => None,
        }
    }
}

fn map_extended_properties(
    metadata: &mut CanonicalMetadata,
    connection_alias: &str,
    database: &str,
    properties: &[RawExtendedProperty],
    targets: ExtendedPropertyTargetRegistry<'_>,
) -> Result<(), CatalogError> {
    for property in properties {
        let target = targets.resolve(property).ok_or_else(|| {
            CatalogError::Mapping(format!(
                "extended property '{}:{}:{}:{}' references an unmapped target",
                property.class, property.major_id, property.minor_id, property.name
            ))
        })?;
        let key = sqlserver_key(
            connection_alias,
            database,
            &target.schema,
            ObjectKind::Extension,
            &target.object_name,
            Some(format!(
                "extended-property:{}:{}:{}:{}",
                property.class, property.major_id, property.minor_id, property.name
            )),
        );
        let mut values = BTreeMap::new();
        insert_i64(&mut values, "class", i64::from(property.class));
        insert_string(
            &mut values,
            "class_description",
            &property.class_description,
        );
        insert_i64(&mut values, "major_id", i64::from(property.major_id));
        insert_i64(&mut values, "minor_id", i64::from(property.minor_id));
        insert_bool(&mut values, "value_is_null", property.value_type.is_none());
        insert_optional_string(&mut values, "value_type", property.value_type.as_deref());
        insert_optional_i64(
            &mut values,
            "value_precision",
            property.value_precision.map(i64::from),
        );
        insert_optional_i64(
            &mut values,
            "value_scale",
            property.value_scale.map(i64::from),
        );
        insert_optional_i64(
            &mut values,
            "value_max_length",
            property.value_max_length.map(i64::from),
        );
        insert_optional_string(
            &mut values,
            "value_collation",
            property.value_collation.as_deref(),
        );
        insert_optional_string(
            &mut values,
            "display_value",
            property.display_value.as_deref(),
        );
        insert_optional_string(&mut values, "value_hex", property.value_hex.as_deref());
        metadata.objects.push(MetadataObject {
            key,
            parent_key: Some(target.clone()),
            name: property.name.clone(),
            extension_kind: Some("sqlserver_extended_property".to_owned()),
            definition: None,
            properties: values,
        });
    }
    Ok(())
}

struct PartitionMappingResult {
    function_keys: BTreeMap<i32, ObjectKey>,
    scheme_keys: BTreeMap<i32, ObjectKey>,
}

fn map_partitions(
    metadata: &mut CanonicalMetadata,
    connection_alias: &str,
    database: &str,
    database_key: &ObjectKey,
    raw: &RawSqlServerCatalog,
    object_keys: &BTreeMap<i32, ObjectKey>,
    index_keys: &BTreeMap<(i32, i32), ObjectKey>,
) -> Result<PartitionMappingResult, CatalogError> {
    let mut function_keys = BTreeMap::<i32, ObjectKey>::new();
    for function in &raw.partition_functions {
        let key = sqlserver_key(
            connection_alias,
            database,
            database,
            ObjectKind::Extension,
            &function.name,
            Some(format!("partition-function:{}", function.id)),
        );
        if function_keys.insert(function.id, key.clone()).is_some() {
            return Err(CatalogError::Mapping(format!(
                "duplicate partition function id {}",
                function.id
            )));
        }
        let mut properties = BTreeMap::new();
        insert_i64(&mut properties, "fanout", i64::from(function.fanout));
        insert_bool(
            &mut properties,
            "boundary_on_right",
            function.boundary_on_right,
        );
        metadata.objects.push(MetadataObject {
            key: key.clone(),
            parent_key: Some(database_key.clone()),
            name: function.name.clone(),
            extension_kind: Some("sqlserver_partition_function".to_owned()),
            definition: None,
            properties,
        });
        for value in &function.values {
            let value_key = sqlserver_key(
                connection_alias,
                database,
                database,
                ObjectKind::Extension,
                &function.name,
                Some(format!("partition-boundary:{}", value.boundary_id)),
            );
            let mut properties = BTreeMap::new();
            insert_i64(&mut properties, "boundary_id", i64::from(value.boundary_id));
            insert_optional_string(&mut properties, "value", value.value.as_deref());
            metadata.objects.push(MetadataObject {
                key: value_key,
                parent_key: Some(key.clone()),
                name: format!("{} boundary {}", function.name, value.boundary_id),
                extension_kind: Some("sqlserver_partition_boundary".to_owned()),
                definition: None,
                properties,
            });
        }
    }

    let mut scheme_keys = BTreeMap::<i32, ObjectKey>::new();
    for scheme in &raw.partition_schemes {
        let function_key = function_keys.get(&scheme.function_id).ok_or_else(|| {
            CatalogError::Mapping(format!(
                "partition scheme '{}' references missing function {}",
                scheme.name, scheme.function_id
            ))
        })?;
        let key = sqlserver_key(
            connection_alias,
            database,
            database,
            ObjectKind::Extension,
            &scheme.name,
            Some(format!("partition-scheme:{}", scheme.id)),
        );
        if scheme_keys.insert(scheme.id, key.clone()).is_some() {
            return Err(CatalogError::Mapping(format!(
                "duplicate partition scheme id {}",
                scheme.id
            )));
        }
        metadata.objects.push(MetadataObject {
            key: key.clone(),
            parent_key: Some(database_key.clone()),
            name: scheme.name.clone(),
            extension_kind: Some("sqlserver_partition_scheme".to_owned()),
            definition: None,
            properties: BTreeMap::new(),
        });
        add_relationship(
            metadata,
            MetadataRelationshipKind::Extension("partition_scheme_uses_function".to_owned()),
            &key,
            function_key,
            None,
            BTreeMap::new(),
        );
    }

    let index_data_spaces = raw
        .indexes
        .iter()
        .map(|index| ((index.object_id, index.id), index.data_space_id))
        .collect::<BTreeMap<_, _>>();
    for partition in &raw.partitions {
        let parent_key = if partition.index_id == 0 {
            object_keys.get(&partition.object_id)
        } else {
            index_keys.get(&(partition.object_id, partition.index_id))
        }
        .cloned()
        .ok_or_else(|| {
            CatalogError::Mapping(format!(
                "partition {}:{}:{} references missing parent",
                partition.object_id, partition.index_id, partition.partition_number
            ))
        })?;
        let key = sqlserver_key(
            connection_alias,
            database,
            &parent_key.schema,
            ObjectKind::Extension,
            &parent_key.object_name,
            Some(format!(
                "partition:{}:{}",
                partition.index_id, partition.partition_number
            )),
        );
        let mut properties = BTreeMap::new();
        insert_i64(&mut properties, "index_id", i64::from(partition.index_id));
        insert_i64(
            &mut properties,
            "partition_number",
            i64::from(partition.partition_number),
        );
        insert_string(
            &mut properties,
            "data_compression",
            &partition.data_compression,
        );
        insert_string(
            &mut properties,
            "xml_compression",
            &partition.xml_compression,
        );
        metadata.objects.push(MetadataObject {
            key: key.clone(),
            parent_key: Some(parent_key),
            name: format!("partition {}", partition.partition_number),
            extension_kind: Some("sqlserver_partition".to_owned()),
            definition: None,
            properties,
        });
        if let Some(data_space_id) = index_data_spaces
            .get(&(partition.object_id, partition.index_id))
            .and_then(|id| scheme_keys.get(id))
        {
            add_relationship(
                metadata,
                MetadataRelationshipKind::Extension("partition_uses_scheme".to_owned()),
                &key,
                data_space_id,
                None,
                BTreeMap::new(),
            );
        }
    }

    for table in &raw.tables {
        let (Some(history_schema), Some(history_table)) =
            (table.history_schema.as_ref(), table.history_table.as_ref())
        else {
            continue;
        };
        let source = object_keys.get(&table.id).ok_or_else(|| {
            CatalogError::Mapping(format!("temporal table {} lost its key", table.id))
        })?;
        let target = raw
            .tables
            .iter()
            .find(|candidate| {
                &candidate.schema == history_schema && &candidate.name == history_table
            })
            .and_then(|candidate| object_keys.get(&candidate.id))
            .ok_or_else(|| {
                CatalogError::InvalidScope(format!(
                    "temporal table '{}.{}' history table '{}.{}' is outside the selected schema scope",
                    table.schema, table.name, history_schema, history_table
                ))
            })?;
        add_relationship(
            metadata,
            MetadataRelationshipKind::Extension("temporal_history_table".to_owned()),
            source,
            target,
            None,
            BTreeMap::new(),
        );
    }
    Ok(PartitionMappingResult {
        function_keys,
        scheme_keys,
    })
}

#[allow(clippy::too_many_arguments)]
fn map_synonym_targets(
    metadata: &mut CanonicalMetadata,
    database: &str,
    synonyms: &[RawSynonym],
    synonym_keys: &BTreeMap<i32, ObjectKey>,
    name_keys: &BTreeMap<(String, String), ObjectKey>,
    external_reference_keys: &mut BTreeMap<String, ObjectKey>,
    connection_alias: &str,
    database_key: &ObjectKey,
) -> Result<(), CatalogError> {
    for synonym in synonyms {
        let source = synonym_keys
            .get(&synonym.id)
            .ok_or_else(|| CatalogError::Mapping(format!("synonym {} lost its key", synonym.id)))?;
        let local_database = synonym
            .database
            .as_deref()
            .is_none_or(|target| target.eq_ignore_ascii_case(database));
        let local_server = synonym.server.is_none();
        let local_target = if local_database && local_server {
            synonym
                .target_schema
                .as_ref()
                .zip(synonym.target_entity.as_ref())
                .and_then(|(schema, entity)| name_keys.get(&(schema.clone(), entity.clone())))
                .cloned()
        } else {
            None
        };
        let target = match local_target {
            Some(target) => target,
            None => ensure_external_reference(
                metadata,
                external_reference_keys,
                connection_alias,
                database,
                database_key,
                &synonym.base_object_name,
                synonym.server.as_deref(),
                synonym.database.as_deref(),
                synonym.target_schema.as_deref(),
                synonym.target_entity.as_deref(),
                "synonym_target",
            )?,
        };
        add_relationship(
            metadata,
            MetadataRelationshipKind::SynonymFor,
            source,
            &target,
            None,
            BTreeMap::new(),
        );
    }
    Ok(())
}

#[derive(Default)]
struct DependencyMappingResult {
    view_dependencies: BTreeMap<i32, Vec<ObjectKey>>,
    routine_dependencies: BTreeMap<i32, Vec<ObjectKey>>,
    metadata_relationship_count: u64,
}

impl DependencyMappingResult {
    fn view_dependency_count(&self) -> u64 {
        self.view_dependencies
            .values()
            .map(|dependencies| dependencies.len() as u64)
            .sum()
    }

    fn routine_dependency_count(&self) -> u64 {
        self.routine_dependencies
            .values()
            .map(|dependencies| dependencies.len() as u64)
            .sum()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct SqlServerProjectionLedger {
    external_reference_objects: u64,
    view_dependencies: u64,
    routine_dependencies: u64,
    dependency_metadata_relationships: u64,
}

#[allow(clippy::too_many_arguments)]
fn map_dependencies(
    metadata: &mut CanonicalMetadata,
    dependencies: &[RawDependency],
    object_keys: &BTreeMap<i32, ObjectKey>,
    column_keys: &BTreeMap<(i32, i32), ObjectKey>,
    dependency_source_keys: &BTreeMap<i32, ObjectKey>,
    type_keys: &BTreeMap<i32, ObjectKey>,
    xml_collection_keys: &BTreeMap<i32, ObjectKey>,
    index_keys: &BTreeMap<(i32, i32), ObjectKey>,
    partition_function_keys: &BTreeMap<i32, ObjectKey>,
    name_keys: &BTreeMap<(String, String), ObjectKey>,
    external_reference_keys: &mut BTreeMap<String, ObjectKey>,
    connection_alias: &str,
    database: &str,
    database_key: &ObjectKey,
) -> Result<DependencyMappingResult, CatalogError> {
    let mut result = DependencyMappingResult::default();
    for dependency in dependencies {
        let source = if dependency.referencing_minor_id > 0 {
            column_keys
                .get(&(dependency.referencing_id, dependency.referencing_minor_id))
                .or_else(|| object_keys.get(&dependency.referencing_id))
                .cloned()
        } else {
            object_keys
                .get(&dependency.referencing_id)
                .or_else(|| dependency_source_keys.get(&dependency.referencing_id))
                .cloned()
        }
        .ok_or_else(|| {
            CatalogError::UnsupportedMetadata(format!(
                "dependency source class {} identity {}:{} has no canonical representation",
                dependency.referencing_class,
                dependency.referencing_id,
                dependency.referencing_minor_id
            ))
        })?;

        let target = resolve_dependency_target(
            metadata,
            dependency,
            object_keys,
            column_keys,
            type_keys,
            xml_collection_keys,
            index_keys,
            partition_function_keys,
            name_keys,
            external_reference_keys,
            connection_alias,
            database,
            database_key,
        )?;
        if source == target {
            continue;
        }
        match source.object_kind {
            ObjectKind::Column if target.object_kind == ObjectKind::Sequence => {
                add_relationship(
                    metadata,
                    MetadataRelationshipKind::UsesSequence,
                    &source,
                    &target,
                    None,
                    dependency_properties(dependency),
                );
                result.metadata_relationship_count += 1;
            }
            ObjectKind::View if is_legacy_schema_object_kind(target.object_kind) => {
                push_unique_dependency(
                    result
                        .view_dependencies
                        .entry(dependency.referencing_id)
                        .or_default(),
                    target,
                );
            }
            ObjectKind::Routine if is_legacy_schema_object_kind(target.object_kind) => {
                push_unique_dependency(
                    result
                        .routine_dependencies
                        .entry(dependency.referencing_id)
                        .or_default(),
                    target,
                );
            }
            ObjectKind::MaterializedView
                if matches!(target.object_kind, ObjectKind::Table | ObjectKind::View) =>
            {
                add_relationship(
                    metadata,
                    MetadataRelationshipKind::Materializes,
                    &source,
                    &target,
                    None,
                    dependency_properties(dependency),
                );
                result.metadata_relationship_count += 1;
            }
            ObjectKind::Trigger if target.object_kind == ObjectKind::Routine => {
                add_relationship(
                    metadata,
                    MetadataRelationshipKind::Invokes,
                    &source,
                    &target,
                    None,
                    dependency_properties(dependency),
                );
                result.metadata_relationship_count += 1;
            }
            _ => {
                add_relationship(
                    metadata,
                    MetadataRelationshipKind::DependsOn,
                    &source,
                    &target,
                    None,
                    dependency_properties(dependency),
                );
                result.metadata_relationship_count += 1;
            }
        }
    }
    for dependencies in result.view_dependencies.values_mut() {
        dependencies.sort_by_key(ObjectKey::to_string);
        dependencies.dedup();
    }
    for dependencies in result.routine_dependencies.values_mut() {
        dependencies.sort_by_key(ObjectKey::to_string);
        dependencies.dedup();
    }
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn resolve_dependency_target(
    metadata: &mut CanonicalMetadata,
    dependency: &RawDependency,
    object_keys: &BTreeMap<i32, ObjectKey>,
    column_keys: &BTreeMap<(i32, i32), ObjectKey>,
    type_keys: &BTreeMap<i32, ObjectKey>,
    xml_collection_keys: &BTreeMap<i32, ObjectKey>,
    index_keys: &BTreeMap<(i32, i32), ObjectKey>,
    partition_function_keys: &BTreeMap<i32, ObjectKey>,
    name_keys: &BTreeMap<(String, String), ObjectKey>,
    external_reference_keys: &mut BTreeMap<String, ObjectKey>,
    connection_alias: &str,
    database: &str,
    database_key: &ObjectKey,
) -> Result<ObjectKey, CatalogError> {
    let direct = match dependency.referenced_class {
        1 => dependency.referenced_id.and_then(|id| {
            if dependency.referenced_minor_id > 0 {
                column_keys
                    .get(&(id, dependency.referenced_minor_id))
                    .cloned()
            } else {
                object_keys.get(&id).cloned()
            }
        }),
        6 => dependency
            .referenced_id
            .and_then(|id| type_keys.get(&id).cloned()),
        7 => dependency.referenced_id.and_then(|object_id| {
            index_keys
                .get(&(object_id, dependency.referenced_minor_id))
                .cloned()
        }),
        10 => dependency
            .referenced_id
            .and_then(|id| xml_collection_keys.get(&id).cloned()),
        21 => dependency
            .referenced_id
            .and_then(|id| partition_function_keys.get(&id).cloned()),
        other => {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "dependency uses unsupported referenced class {other}"
            )))
        }
    };
    if let Some(key) = direct {
        return Ok(key);
    }

    let local_database = dependency
        .referenced_database
        .as_deref()
        .is_none_or(|name| name.eq_ignore_ascii_case(database));
    if dependency.referenced_server.is_none() && local_database {
        if let Some(key) = dependency
            .referenced_schema
            .as_ref()
            .and_then(|schema| {
                name_keys.get(&(schema.clone(), dependency.referenced_entity.clone()))
            })
            .cloned()
        {
            return Ok(key);
        }
    }
    let full_name = [
        dependency.referenced_server.as_deref(),
        dependency.referenced_database.as_deref(),
        dependency.referenced_schema.as_deref(),
        Some(dependency.referenced_entity.as_str()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(".");
    ensure_external_reference(
        metadata,
        external_reference_keys,
        connection_alias,
        database,
        database_key,
        &full_name,
        dependency.referenced_server.as_deref(),
        dependency.referenced_database.as_deref(),
        dependency.referenced_schema.as_deref(),
        Some(&dependency.referenced_entity),
        if dependency.referenced_id.is_some() {
            "unmodeled_local_reference"
        } else {
            "symbolic_reference"
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn ensure_external_reference(
    metadata: &mut CanonicalMetadata,
    keys: &mut BTreeMap<String, ObjectKey>,
    connection_alias: &str,
    database: &str,
    database_key: &ObjectKey,
    full_name: &str,
    server: Option<&str>,
    target_database: Option<&str>,
    schema: Option<&str>,
    entity: Option<&str>,
    reference_kind: &str,
) -> Result<ObjectKey, CatalogError> {
    if full_name.trim().is_empty() {
        return Err(CatalogError::Mapping(
            "external dependency has no stable name".to_owned(),
        ));
    }
    if let Some(key) = keys.get(full_name) {
        return Ok(key.clone());
    }
    let key = sqlserver_key(
        connection_alias,
        database,
        "external",
        ObjectKind::Extension,
        full_name,
        Some("reference".to_owned()),
    );
    let mut properties = BTreeMap::new();
    insert_string(&mut properties, "reference_kind", reference_kind);
    insert_optional_string(&mut properties, "server", server);
    insert_optional_string(&mut properties, "database", target_database);
    insert_optional_string(&mut properties, "schema", schema);
    insert_optional_string(&mut properties, "entity", entity);
    metadata.objects.push(MetadataObject {
        key: key.clone(),
        parent_key: Some(database_key.clone()),
        name: full_name.to_owned(),
        extension_kind: Some("sqlserver_external_reference".to_owned()),
        definition: None,
        properties,
    });
    keys.insert(full_name.to_owned(), key.clone());
    Ok(key)
}

fn dependency_properties(dependency: &RawDependency) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_i64(
        &mut properties,
        "referencing_class",
        i64::from(dependency.referencing_class),
    );
    insert_i64(
        &mut properties,
        "referenced_class",
        i64::from(dependency.referenced_class),
    );
    insert_i64(
        &mut properties,
        "referencing_minor_id",
        i64::from(dependency.referencing_minor_id),
    );
    insert_i64(
        &mut properties,
        "referenced_minor_id",
        i64::from(dependency.referenced_minor_id),
    );
    insert_bool(&mut properties, "schema_bound", dependency.schema_bound);
    properties
}

fn push_unique_dependency(dependencies: &mut Vec<ObjectKey>, key: ObjectKey) {
    if !dependencies.contains(&key) {
        dependencies.push(key);
    }
}

fn is_legacy_schema_object_kind(kind: ObjectKind) -> bool {
    matches!(
        kind,
        ObjectKind::Database
            | ObjectKind::Schema
            | ObjectKind::Table
            | ObjectKind::Column
            | ObjectKind::PrimaryKey
            | ObjectKind::ForeignKey
            | ObjectKind::UniqueConstraint
            | ObjectKind::CheckConstraint
            | ObjectKind::Index
            | ObjectKind::View
            | ObjectKind::Trigger
            | ObjectKind::Routine
    )
}

fn validate_relationship_uniqueness(
    relationships: &[MetadataRelationship],
) -> Result<(), CatalogError> {
    let mut seen = BTreeSet::new();
    for relationship in relationships {
        let identity = (
            relationship.kind.clone(),
            relationship.from_key.to_string(),
            relationship.to_key.to_string(),
            relationship.ordinal,
        );
        if !seen.insert(identity) {
            return Err(CatalogError::Mapping(format!(
                "duplicate metadata relationship {}:{}->{}",
                relationship.kind.graph_edge_type(),
                relationship.from_key,
                relationship.to_key
            )));
        }
    }
    Ok(())
}

fn discovery_counts_from_catalog(
    raw: &RawSqlServerCatalog,
    snapshot: &CanonicalSchemaSnapshot,
    projection: SqlServerProjectionLedger,
) -> Result<DiscoveryCounts, CatalogError> {
    let emitted_objects = emitted_object_counts(snapshot);
    let emitted_relationships = emitted_relationship_counts(snapshot);
    let table_ids = raw
        .tables
        .iter()
        .map(|table| table.id)
        .collect::<BTreeSet<_>>();
    let table_type_ids = raw
        .user_types
        .iter()
        .filter_map(|data_type| data_type.table_object_id)
        .collect::<BTreeSet<_>>();
    let mut expected_objects = ObjectCategory::ALL
        .into_iter()
        .map(|category| (category, 0_u64))
        .collect::<BTreeMap<_, _>>();
    expected_objects.insert(ObjectCategory::Database, 1);
    expected_objects.insert(ObjectCategory::Schema, raw.schemas.len() as u64);
    expected_objects.insert(ObjectCategory::Principal, raw.principals.len() as u64);
    expected_objects.insert(ObjectCategory::Table, raw.tables.len() as u64);
    expected_objects.insert(
        ObjectCategory::Column,
        raw.columns
            .iter()
            .filter(|column| column.object_type == "U")
            .count() as u64,
    );
    expected_objects.insert(
        ObjectCategory::ViewColumn,
        raw.columns
            .iter()
            .filter(|column| column.object_type == "V")
            .count() as u64,
    );
    expected_objects.insert(
        ObjectCategory::PrimaryKey,
        raw.constraints
            .iter()
            .filter(|constraint| {
                table_ids.contains(&constraint.table_id)
                    && constraint.kind == ConstraintKind::PrimaryKey
            })
            .count() as u64,
    );
    expected_objects.insert(
        ObjectCategory::ForeignKey,
        raw.constraints
            .iter()
            .filter(|constraint| {
                table_ids.contains(&constraint.table_id)
                    && constraint.kind == ConstraintKind::ForeignKey
            })
            .count() as u64,
    );
    expected_objects.insert(
        ObjectCategory::UniqueConstraint,
        raw.constraints
            .iter()
            .filter(|constraint| {
                table_ids.contains(&constraint.table_id)
                    && constraint.kind == ConstraintKind::Unique
            })
            .count() as u64,
    );
    expected_objects.insert(
        ObjectCategory::CheckConstraint,
        raw.constraints
            .iter()
            .filter(|constraint| {
                table_ids.contains(&constraint.table_id) && constraint.kind == ConstraintKind::Check
            })
            .count() as u64,
    );
    expected_objects.insert(
        ObjectCategory::Index,
        raw.indexes
            .iter()
            .filter(|index| index.relation_type != "TT")
            .count() as u64,
    );
    expected_objects.insert(
        ObjectCategory::View,
        raw.views.iter().filter(|view| !view.indexed).count() as u64,
    );
    expected_objects.insert(
        ObjectCategory::MaterializedView,
        raw.views.iter().filter(|view| view.indexed).count() as u64,
    );
    expected_objects.insert(ObjectCategory::Routine, raw.routines.len() as u64);
    expected_objects.insert(
        ObjectCategory::RoutineParameter,
        raw.parameters.len() as u64,
    );
    expected_objects.insert(ObjectCategory::Trigger, raw.triggers.len() as u64);
    expected_objects.insert(ObjectCategory::UserDefinedType, raw.user_types.len() as u64);
    expected_objects.insert(ObjectCategory::Sequence, raw.sequences.len() as u64);
    expected_objects.insert(ObjectCategory::Synonym, raw.synonyms.len() as u64);
    expected_objects.insert(ObjectCategory::Policy, raw.security_policies.len() as u64);
    expected_objects.insert(
        ObjectCategory::Extension,
        expected_extension_object_count(raw, &table_type_ids, projection),
    );
    if expected_objects != emitted_objects {
        return Err(CatalogError::Mapping(format!(
            "SQL Server raw/emitted object counts differ: raw={expected_objects:?}, emitted={emitted_objects:?}"
        )));
    }

    let mut expected_relationships = RelationshipCategory::ALL
        .into_iter()
        .map(|category| (category, 0_u64))
        .collect::<BTreeMap<_, _>>();
    expected_relationships.insert(
        RelationshipCategory::DatabaseHasSchema,
        raw.schemas.len() as u64,
    );
    expected_relationships.insert(
        RelationshipCategory::SchemaHasTable,
        raw.tables.len() as u64,
    );
    expected_relationships.insert(
        RelationshipCategory::TableHasColumn,
        raw.columns
            .iter()
            .filter(|column| column.object_type == "U")
            .count() as u64,
    );
    expected_relationships.insert(
        RelationshipCategory::TableHasConstraint,
        raw.constraints
            .iter()
            .filter(|constraint| table_ids.contains(&constraint.table_id))
            .count() as u64,
    );
    expected_relationships.insert(
        RelationshipCategory::ConstraintColumn,
        raw.constraints
            .iter()
            .filter(|constraint| {
                table_ids.contains(&constraint.table_id)
                    && constraint.kind != ConstraintKind::ForeignKey
            })
            .map(|constraint| constraint.columns.len() as u64)
            .sum(),
    );
    expected_relationships.insert(
        RelationshipCategory::ForeignKeyColumnPair,
        raw.constraints
            .iter()
            .filter(|constraint| {
                table_ids.contains(&constraint.table_id)
                    && constraint.kind == ConstraintKind::ForeignKey
            })
            .map(|constraint| constraint.columns.len() as u64)
            .sum(),
    );
    expected_relationships.insert(
        RelationshipCategory::TableHasIndex,
        raw.indexes
            .iter()
            .filter(|index| index.relation_type == "U")
            .count() as u64,
    );
    expected_relationships.insert(
        RelationshipCategory::IndexColumn,
        raw.indexes
            .iter()
            .filter(|index| index.relation_type == "U")
            .map(projected_index_column_count)
            .sum(),
    );
    expected_relationships.insert(
        RelationshipCategory::SchemaHasView,
        raw.views.iter().filter(|view| !view.indexed).count() as u64,
    );
    expected_relationships.insert(
        RelationshipCategory::ViewDependency,
        projection.view_dependencies,
    );
    expected_relationships.insert(
        RelationshipCategory::TriggerTarget,
        raw.triggers
            .iter()
            .filter(|trigger| {
                trigger.parent_class == 1
                    && trigger
                        .parent_type
                        .as_deref()
                        .is_some_and(|kind| kind == "U" || kind == "V")
                    && !raw
                        .views
                        .iter()
                        .any(|view| view.indexed && view.id == trigger.parent_id)
            })
            .count() as u64,
    );
    expected_relationships.insert(RelationshipCategory::TriggerRoutine, 0);
    expected_relationships.insert(
        RelationshipCategory::SchemaHasRoutine,
        raw.routines.len() as u64,
    );
    expected_relationships.insert(
        RelationshipCategory::RoutineDependency,
        projection.routine_dependencies,
    );
    expected_relationships.insert(
        RelationshipCategory::MetadataParent,
        expected_metadata_parent_count(raw, &table_type_ids, projection),
    );
    expected_relationships.insert(
        RelationshipCategory::MetadataRelationship,
        expected_metadata_relationship_count(raw, &table_type_ids, projection),
    );
    if expected_relationships != emitted_relationships {
        return Err(CatalogError::Mapping(format!(
            "SQL Server raw/emitted relationship counts differ: raw={expected_relationships:?}, emitted={emitted_relationships:?}"
        )));
    }

    Ok(DiscoveryCounts {
        objects: expected_objects
            .into_iter()
            .map(|(category, count)| {
                (
                    category,
                    DiscoveredCount {
                        count,
                        evidence: "SQL Server sys catalog raw inventory".to_owned(),
                    },
                )
            })
            .collect(),
        relationships: expected_relationships
            .into_iter()
            .map(|(category, count)| {
                (
                    category,
                    DiscoveredCount {
                        count,
                        evidence: "SQL Server catalog identity and dependency ledger".to_owned(),
                    },
                )
            })
            .collect(),
    })
}

fn expected_extension_object_count(
    raw: &RawSqlServerCatalog,
    table_type_ids: &BTreeSet<i32>,
    projection: SqlServerProjectionLedger,
) -> u64 {
    let table_type_columns = raw
        .columns
        .iter()
        .filter(|column| table_type_ids.contains(&column.object_id))
        .count() as u64;
    let table_type_constraints = raw
        .constraints
        .iter()
        .filter(|constraint| table_type_ids.contains(&constraint.table_id))
        .count() as u64;
    let table_type_indexes = raw
        .indexes
        .iter()
        .filter(|index| table_type_ids.contains(&index.object_id))
        .count() as u64;
    let security_predicates = raw
        .security_policies
        .iter()
        .map(|policy| policy.predicates.len() as u64)
        .sum::<u64>();
    let partition_boundaries = raw
        .partition_functions
        .iter()
        .map(|function| function.values.len() as u64)
        .sum::<u64>();
    let xml_namespaces = raw
        .xml_schema_collections
        .iter()
        .map(|collection| collection.namespaces.len() as u64)
        .sum::<u64>();

    table_type_columns
        + table_type_constraints
        + table_type_indexes
        + security_predicates
        + raw.partition_functions.len() as u64
        + partition_boundaries
        + raw.partition_schemes.len() as u64
        + raw.partitions.len() as u64
        + raw.xml_schema_collections.len() as u64
        + xml_namespaces
        + raw.extended_properties.len() as u64
        + projection.external_reference_objects
}

fn expected_metadata_parent_count(
    raw: &RawSqlServerCatalog,
    table_type_ids: &BTreeSet<i32>,
    projection: SqlServerProjectionLedger,
) -> u64 {
    let indexed_view_ids = raw
        .views
        .iter()
        .filter(|view| view.indexed)
        .map(|view| view.id)
        .collect::<BTreeSet<_>>();
    let metadata_triggers = raw
        .triggers
        .iter()
        .filter(|trigger| {
            trigger.parent_class == 0
                || (trigger.parent_class == 1 && indexed_view_ids.contains(&trigger.parent_id))
        })
        .count() as u64;

    raw.principals.len() as u64
        + raw.user_types.len() as u64
        + raw.sequences.len() as u64
        + indexed_view_ids.len() as u64
        + metadata_triggers
        + raw
            .columns
            .iter()
            .filter(|column| column.object_type == "V")
            .count() as u64
        + raw
            .indexes
            .iter()
            .filter(|index| index.relation_type == "V")
            .count() as u64
        + raw.parameters.len() as u64
        + raw.synonyms.len() as u64
        + raw.security_policies.len() as u64
        + expected_extension_object_count(raw, table_type_ids, projection)
}

fn expected_metadata_relationship_count(
    raw: &RawSqlServerCatalog,
    table_type_ids: &BTreeSet<i32>,
    projection: SqlServerProjectionLedger,
) -> u64 {
    let user_type_ids = raw
        .user_types
        .iter()
        .map(|data_type| data_type.id)
        .collect::<BTreeSet<_>>();
    let ownerships = raw.schemas.len()
        + raw.sequences.len()
        + raw.tables.len()
        + raw.views.len()
        + raw.routines.len()
        + raw.synonyms.len()
        + raw.security_policies.len()
        + raw
            .principals
            .iter()
            .filter(|principal| principal.owning_principal_id.is_some())
            .count();
    let sequence_types = raw
        .sequences
        .iter()
        .filter(|sequence| user_type_ids.contains(&sequence.type_id))
        .count() as u64;
    let column_types = raw
        .columns
        .iter()
        .filter(|column| user_type_ids.contains(&column.type_id))
        .count() as u64;
    let table_type_constraint_columns = raw
        .constraints
        .iter()
        .filter(|constraint| table_type_ids.contains(&constraint.table_id))
        .map(|constraint| constraint.columns.len() as u64)
        .sum::<u64>();
    let table_type_index_columns = raw
        .indexes
        .iter()
        .filter(|index| table_type_ids.contains(&index.object_id))
        .map(|index| index.columns.len() as u64)
        .sum::<u64>();
    let parameter_types = raw
        .parameters
        .iter()
        .filter(|parameter| user_type_ids.contains(&parameter.type_id))
        .count() as u64;
    let security_predicates = raw
        .security_policies
        .iter()
        .map(|policy| policy.predicates.len() as u64)
        .sum::<u64>();
    let included_columns = raw
        .indexes
        .iter()
        .filter(|index| index.relation_type == "U" || index.relation_type == "V")
        .flat_map(|index| &index.columns)
        .filter(|column| column.included)
        .count() as u64;
    let partition_scheme_ids = raw
        .partition_schemes
        .iter()
        .map(|scheme| scheme.id)
        .collect::<BTreeSet<_>>();
    let index_data_spaces = raw
        .indexes
        .iter()
        .map(|index| ((index.object_id, index.id), index.data_space_id))
        .collect::<BTreeMap<_, _>>();
    let partition_scheme_uses = raw
        .partitions
        .iter()
        .filter(|partition| {
            index_data_spaces
                .get(&(partition.object_id, partition.index_id))
                .is_some_and(|id| partition_scheme_ids.contains(id))
        })
        .count() as u64;
    let temporal_histories = raw
        .tables
        .iter()
        .filter(|table| table.history_schema.is_some() && table.history_table.is_some())
        .count() as u64;
    let typed_xml_columns = raw
        .columns
        .iter()
        .filter(|column| column.xml_collection_id > 0)
        .count() as u64;
    let typed_xml_parameters = raw
        .parameters
        .iter()
        .filter(|parameter| parameter.xml_collection_id > 0)
        .count() as u64;

    ownerships as u64
        + sequence_types
        + column_types
        + table_type_constraint_columns
        + table_type_index_columns
        + raw.parameters.len() as u64
        + parameter_types
        + security_predicates
        + included_columns
        + raw.partition_schemes.len() as u64
        + partition_scheme_uses
        + temporal_histories
        + raw.synonyms.len() as u64
        + raw.xml_schema_collections.len() as u64
        + typed_xml_columns
        + typed_xml_parameters
        + projection.dependency_metadata_relationships
}

fn projected_index_column_count(index: &RawIndex) -> u64 {
    let key_columns = index
        .columns
        .iter()
        .filter(|column| column.key_ordinal > 0)
        .count();
    if key_columns == 0 {
        index.columns.len() as u64
    } else {
        key_columns as u64
    }
}

fn sqlserver_capabilities() -> AdapterCapabilities {
    AdapterCapabilities {
        source_kind: SQLSERVER_SOURCE.to_owned(),
        metadata_only: true,
        schemas: true,
        tables: true,
        columns: true,
        constraints: true,
        indexes: true,
        views: CapabilitySupport::Supported,
        triggers: CapabilitySupport::Supported,
        routines: CapabilitySupport::Supported,
        dependencies: CapabilitySupport::Supported,
        limitations: Vec::new(),
        notes: vec![
            "Reads SQL Server sys catalog metadata and module definitions only; application table rows are never queried.".to_owned(),
            "Dynamic SQL, encrypted definitions, runtime-bound dependencies, and unsupported CLR or legacy objects fail closed.".to_owned(),
        ],
    }
}

fn sqlserver_capability_checks(
    facts: &ServerFacts,
    strategy: SqlServerCatalogVersion,
) -> Vec<CapabilityCheck> {
    vec![
        CapabilityCheck {
            name: "catalog_version_strategy".to_owned(),
            evidence: strategy.strategy_name().to_owned(),
        },
        CapabilityCheck {
            name: "metadata_visibility".to_owned(),
            evidence: "database VIEW DEFINITION and dependency SELECT effective".to_owned(),
        },
        CapabilityCheck {
            name: "catalog_stability".to_owned(),
            evidence: "two exact ordered raw catalog reads matched under READ COMMITTED".to_owned(),
        },
        CapabilityCheck {
            name: "metadata_only".to_owned(),
            evidence: "adapter queries sys catalogs, SERVERPROPERTY, and metadata functions only"
                .to_owned(),
        },
        CapabilityCheck {
            name: "transport".to_owned(),
            evidence: if facts.encrypted_transport {
                "TDS transport reported encrypted".to_owned()
            } else {
                "loopback TDS transport reported unencrypted".to_owned()
            },
        },
        CapabilityCheck {
            name: "module_dependency_policy".to_owned(),
            evidence: "dynamic, encrypted, CLR, caller-dependent, and ambiguous modules reject certification"
                .to_owned(),
        },
        CapabilityCheck {
            name: "xml_schema_collections".to_owned(),
            evidence: "typed XML columns and parameters resolve to sys.xml_schema_collections"
                .to_owned(),
        },
        CapabilityCheck {
            name: "extended_properties".to_owned(),
            evidence: "supported sys.extended_properties targets preserve sql_variant type, display, and raw hex values"
                .to_owned(),
        },
    ]
}

