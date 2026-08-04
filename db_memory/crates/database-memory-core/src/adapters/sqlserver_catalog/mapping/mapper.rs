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
