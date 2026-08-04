impl<'a> OracleSnapshotMapper<'a> {
    fn map(self, raw: RawOracleCatalog) -> Result<CatalogDiscovery, CatalogError> {
        // Keep the established mapping dataflow intact; only the file boundary changes.
        let database_name = self.facts.container.clone();
        let database_key = oracle_key(
            self.connection_alias,
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

        let schemas = self
            .scope
            .owners
            .iter()
            .map(|owner| SchemaObject {
                key: oracle_key(
                    self.connection_alias,
                    &database_name,
                    owner,
                    ObjectKind::Schema,
                    owner,
                    None,
                ),
                database_key: database_key.clone(),
                name: owner.clone(),
            })
            .collect::<Vec<_>>();
        let schema_keys = schemas
            .iter()
            .map(|schema| (schema.name.clone(), schema.key.clone()))
            .collect::<BTreeMap<_, _>>();

        let mut metadata = CanonicalMetadata::default();
        for principal in &self.scope.principals {
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &database_name,
                ObjectKind::Principal,
                &principal.name,
                None,
            );
            let mut properties = BTreeMap::new();
            insert_i64(&mut properties, "oracle_user_id", principal.user_id);
            insert_string(&mut properties, "account_status", &principal.account_status);
            insert_bool(&mut properties, "common", principal.common);
            insert_bool(
                &mut properties,
                "oracle_maintained",
                principal.oracle_maintained,
            );
            insert_optional_string(
                &mut properties,
                "default_collation",
                principal.default_collation.as_deref(),
            );
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(database_key.clone()),
                name: principal.name.clone(),
                extension_kind: None,
                definition: None,
                properties,
            });
        }

        let inventory = raw
            .inventory
            .iter()
            .filter(|object| !object.secondary && object.subobject.is_none())
            .map(|object| {
                (
                    (
                        object.owner.clone(),
                        object.object_type.clone(),
                        object.name.clone(),
                    ),
                    object,
                )
            })
            .collect::<BTreeMap<_, _>>();
        let subobject_inventory = raw
            .inventory
            .iter()
            .filter(|object| !object.secondary)
            .filter_map(|object| {
                Some((
                    (
                        object.owner.clone(),
                        object.object_type.clone(),
                        object.name.clone(),
                        object.subobject.clone()?,
                    ),
                    object,
                ))
            })
            .collect::<BTreeMap<_, _>>();
        let partitioned_tables = raw
            .partitioned_tables
            .iter()
            .map(|table| ((table.owner.clone(), table.table.clone()), table))
            .collect::<BTreeMap<_, _>>();
        let partitioned_indexes = raw
            .partitioned_indexes
            .iter()
            .map(|index| ((index.owner.clone(), index.index.clone()), index))
            .collect::<BTreeMap<_, _>>();

        let collection_by_type = raw
            .collection_types
            .iter()
            .map(|collection| {
                (
                    (collection.owner.clone(), collection.type_name.clone()),
                    collection,
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut type_keys = BTreeMap::new();
        for user_type in &raw.user_types {
            let schema_key = required(
                schema_keys.get(&user_type.owner),
                format!(
                    "schema key for Oracle type {}.{}",
                    user_type.owner, user_type.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &user_type.owner,
                ObjectKind::UserDefinedType,
                &user_type.name,
                None,
            );
            type_keys.insert(
                (user_type.owner.clone(), user_type.name.clone()),
                key.clone(),
            );
            let inventory_object = required(
                inventory.get(&(
                    user_type.owner.clone(),
                    "TYPE".to_owned(),
                    user_type.name.clone(),
                )),
                format!(
                    "inventory row for Oracle type {}.{}",
                    user_type.owner, user_type.name
                ),
            )?;
            let body_inventory = inventory
                .get(&(
                    user_type.owner.clone(),
                    "TYPE BODY".to_owned(),
                    user_type.name.clone(),
                ))
                .copied();
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(schema_key.clone()),
                name: user_type.name.clone(),
                extension_kind: None,
                definition: Some(oracle_type_definition(user_type)?),
                properties: oracle_type_properties(
                    user_type,
                    inventory_object,
                    body_inventory,
                    collection_by_type
                        .get(&(user_type.owner.clone(), user_type.name.clone()))
                        .copied(),
                ),
            });
        }
        for user_type in &raw.user_types {
            let Some(supertype_owner) = user_type.supertype_owner.as_deref() else {
                continue;
            };
            let supertype_name = user_type.supertype_name.as_deref().ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle type {}.{} has no supertype name",
                    user_type.owner, user_type.name
                ))
            })?;
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::InheritsFrom,
                from_key: required(
                    type_keys.get(&(user_type.owner.clone(), user_type.name.clone())),
                    format!("subtype key for {}.{}", user_type.owner, user_type.name),
                )?
                .clone(),
                to_key: required(
                    type_keys.get(&(supertype_owner.to_owned(), supertype_name.to_owned())),
                    format!("supertype key for {supertype_owner}.{supertype_name}"),
                )?
                .clone(),
                ordinal: None,
                properties: BTreeMap::new(),
            });
        }
        for attribute in &raw.type_attributes {
            let parent_key = required(
                type_keys.get(&(attribute.owner.clone(), attribute.type_name.clone())),
                format!(
                    "parent type key for Oracle attribute {}.{}.{}",
                    attribute.owner, attribute.type_name, attribute.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &attribute.owner,
                ObjectKind::Extension,
                &attribute.type_name,
                Some(format!(
                    "attribute:{}:{}",
                    attribute.position, attribute.name
                )),
            );
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(parent_key.clone()),
                name: attribute.name.clone(),
                extension_kind: Some("oracle_type_attribute".to_owned()),
                definition: None,
                properties: oracle_type_attribute_properties(attribute),
            });
            if let Some(owner) = attribute.data_type_owner.as_deref() {
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::UsesType,
                    from_key: key,
                    to_key: required(
                        type_keys.get(&(owner.to_owned(), attribute.data_type_name.clone())),
                        format!(
                            "type key for Oracle attribute {}.{}.{}",
                            attribute.owner, attribute.type_name, attribute.name
                        ),
                    )?
                    .clone(),
                    ordinal: None,
                    properties: BTreeMap::new(),
                });
            }
        }
        for collection in &raw.collection_types {
            let Some(element_owner) = collection.element_type_owner.as_deref() else {
                continue;
            };
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::UsesType,
                from_key: required(
                    type_keys.get(&(collection.owner.clone(), collection.type_name.clone())),
                    format!(
                        "collection type key for {}.{}",
                        collection.owner, collection.type_name
                    ),
                )?
                .clone(),
                to_key: required(
                    type_keys.get(&(
                        element_owner.to_owned(),
                        collection.element_type_name.clone(),
                    )),
                    format!(
                        "element type key for {}.{}",
                        element_owner, collection.element_type_name
                    ),
                )?
                .clone(),
                ordinal: None,
                properties: BTreeMap::new(),
            });
        }

        let mut type_method_keys = BTreeMap::new();
        for method in &raw.type_methods {
            let parent_key = required(
                type_keys.get(&(method.owner.clone(), method.type_name.clone())),
                format!(
                    "parent type key for Oracle method {}.{}.{}",
                    method.owner, method.type_name, method.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &method.owner,
                ObjectKind::Routine,
                &method.type_name,
                Some(format!("method:{}:{}", method.method_number, method.name)),
            );
            type_method_keys.insert(
                (
                    method.owner.clone(),
                    method.type_name.clone(),
                    method.method_number,
                ),
                key.clone(),
            );
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(parent_key.clone()),
                name: method.name.clone(),
                extension_kind: None,
                definition: None,
                properties: oracle_type_method_properties(method),
            });
        }
        for parameter in &raw.type_method_parameters {
            let method_key = required(
                type_method_keys.get(&(
                    parameter.owner.clone(),
                    parameter.type_name.clone(),
                    parameter.method_number,
                )),
                format!(
                    "method key for Oracle parameter {}.{}.{}",
                    parameter.owner, parameter.type_name, parameter.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &parameter.owner,
                ObjectKind::RoutineParameter,
                &parameter.type_name,
                Some(format!(
                    "method:{}:{}#parameter:{}:{}",
                    parameter.method_number,
                    parameter.method_name,
                    parameter.position,
                    parameter.name
                )),
            );
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(method_key.clone()),
                name: parameter.name.clone(),
                extension_kind: None,
                definition: None,
                properties: oracle_type_method_parameter_properties(parameter),
            });
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::HasParameter,
                from_key: method_key.clone(),
                to_key: key.clone(),
                ordinal: Some(positive_u32(
                    parameter.position + 1,
                    "Oracle type method parameter relationship ordinal",
                )?),
                properties: BTreeMap::new(),
            });
            if let Some(owner) = parameter.data_type_owner.as_deref() {
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::UsesType,
                    from_key: key,
                    to_key: required(
                        type_keys.get(&(owner.to_owned(), parameter.data_type_name.clone())),
                        format!(
                            "type key for Oracle method parameter {}.{}.{}",
                            parameter.owner, parameter.type_name, parameter.name
                        ),
                    )?
                    .clone(),
                    ordinal: None,
                    properties: BTreeMap::new(),
                });
            }
        }

        let mut sequence_keys = BTreeMap::new();
        for sequence in &raw.sequences {
            let schema_key = required(
                schema_keys.get(&sequence.owner),
                format!(
                    "schema key for Oracle sequence {}.{}",
                    sequence.owner, sequence.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &sequence.owner,
                ObjectKind::Sequence,
                &sequence.name,
                None,
            );
            sequence_keys.insert((sequence.owner.clone(), sequence.name.clone()), key.clone());
            let inventory_object = required(
                inventory.get(&(
                    sequence.owner.clone(),
                    "SEQUENCE".to_owned(),
                    sequence.name.clone(),
                )),
                format!(
                    "inventory row for Oracle sequence {}.{}",
                    sequence.owner, sequence.name
                ),
            )?;
            let mut properties = inventory_properties(inventory_object);
            insert_optional_string(&mut properties, "minimum", sequence.min_value.as_deref());
            insert_optional_string(&mut properties, "maximum", sequence.max_value.as_deref());
            insert_string(&mut properties, "increment", &sequence.increment_by);
            insert_string(&mut properties, "cache_size", &sequence.cache_size);
            insert_optional_string(&mut properties, "cycle", sequence.cycle.as_deref());
            insert_optional_string(&mut properties, "ordered", sequence.ordered.as_deref());
            insert_optional_string(&mut properties, "scale", sequence.scale.as_deref());
            insert_optional_string(&mut properties, "extend", sequence.extend.as_deref());
            insert_optional_string(&mut properties, "sharded", sequence.sharded.as_deref());
            insert_optional_string(&mut properties, "session", sequence.session.as_deref());
            insert_optional_string(
                &mut properties,
                "keep_value",
                sequence.keep_value.as_deref(),
            );
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(schema_key.clone()),
                name: sequence.name.clone(),
                extension_kind: None,
                definition: None,
                properties,
            });
        }

        let materialized_view_names = raw
            .materialized_views
            .iter()
            .map(|view| (view.owner.clone(), view.name.clone()))
            .collect::<BTreeSet<_>>();
        let mut materialized_view_keys = BTreeMap::new();
        for view in &raw.materialized_views {
            let schema_key = required(
                schema_keys.get(&view.owner),
                format!(
                    "schema key for Oracle materialized view {}.{}",
                    view.owner, view.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &view.owner,
                ObjectKind::MaterializedView,
                &view.name,
                None,
            );
            materialized_view_keys.insert((view.owner.clone(), view.name.clone()), key.clone());
            let inventory_object = required(
                inventory.get(&(
                    view.owner.clone(),
                    "MATERIALIZED VIEW".to_owned(),
                    view.name.clone(),
                )),
                format!(
                    "inventory row for Oracle materialized view {}.{}",
                    view.owner, view.name
                ),
            )?;
            let mut properties = inventory_properties(inventory_object);
            let storage_object = required(
                inventory.get(&(
                    view.owner.clone(),
                    "TABLE".to_owned(),
                    view.container_name.clone(),
                )),
                format!(
                    "storage inventory row for Oracle materialized view {}.{}",
                    view.owner, view.name
                ),
            )?;
            insert_i64(
                &mut properties,
                "storage_object_id",
                storage_object.object_id,
            );
            insert_optional_i64(
                &mut properties,
                "storage_data_object_id",
                storage_object.data_object_id,
            );
            insert_string(
                &mut properties,
                "storage_object_status",
                &storage_object.status,
            );
            insert_bool(
                &mut properties,
                "storage_generated",
                storage_object.generated,
            );
            insert_string(&mut properties, "container_name", &view.container_name);
            insert_optional_i64(&mut properties, "query_length", view.query_length);
            insert_optional_string(&mut properties, "updatable", view.updatable.as_deref());
            insert_optional_string(
                &mut properties,
                "rewrite_enabled",
                view.rewrite_enabled.as_deref(),
            );
            insert_optional_string(
                &mut properties,
                "rewrite_capability",
                view.rewrite_capability.as_deref(),
            );
            insert_optional_string(
                &mut properties,
                "refresh_mode",
                view.refresh_mode.as_deref(),
            );
            insert_optional_string(
                &mut properties,
                "refresh_method",
                view.refresh_method.as_deref(),
            );
            insert_optional_string(&mut properties, "build_mode", view.build_mode.as_deref());
            insert_optional_string(
                &mut properties,
                "fast_refreshable",
                view.fast_refreshable.as_deref(),
            );
            insert_optional_string(
                &mut properties,
                "compile_state",
                view.compile_state.as_deref(),
            );
            insert_optional_string(
                &mut properties,
                "use_no_index",
                view.use_no_index.as_deref(),
            );
            insert_optional_string(
                &mut properties,
                "segment_created",
                view.segment_created.as_deref(),
            );
            insert_optional_string(
                &mut properties,
                "default_collation",
                view.default_collation.as_deref(),
            );
            insert_optional_string(
                &mut properties,
                "on_query_computation",
                view.on_query_computation.as_deref(),
            );
            insert_optional_string(&mut properties, "automatic", view.automatic.as_deref());
            insert_optional_string(
                &mut properties,
                "concurrent_refresh",
                view.concurrent_refresh.as_deref(),
            );
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(schema_key.clone()),
                name: view.name.clone(),
                extension_kind: None,
                definition: view.definition.clone(),
                properties,
            });
        }


        let mut tables = Vec::new();
        let mut table_keys = BTreeMap::new();
        for table in &raw.tables {
            if materialized_view_names.contains(&(table.owner.clone(), table.name.clone())) {
                continue;
            }
            let schema_key = required(
                schema_keys.get(&table.owner),
                format!("schema key for Oracle table {}.{}", table.owner, table.name),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &table.owner,
                ObjectKind::Table,
                &table.name,
                None,
            );
            table_keys.insert((table.owner.clone(), table.name.clone()), key.clone());
            tables.push(TableObject {
                key: key.clone(),
                schema_key: schema_key.clone(),
                name: table.name.clone(),
                kind: if table.partitioned {
                    TableKind::Partitioned
                } else if table.temporary {
                    TableKind::Temporary
                } else {
                    TableKind::BaseTable
                },
            });
            let inventory_object = required(
                inventory.get(&(table.owner.clone(), "TABLE".to_owned(), table.name.clone())),
                format!(
                    "inventory row for Oracle table {}.{}",
                    table.owner, table.name
                ),
            )?;
            let mut properties = inventory_properties(inventory_object);
            insert_string(&mut properties, "table_status", &table.status);
            insert_bool(&mut properties, "temporary", table.temporary);
            insert_bool(&mut properties, "read_only", table.read_only);
            insert_bool(&mut properties, "has_identity", table.has_identity);
            insert_optional_string(&mut properties, "duration", table.duration.as_deref());
            if let Some(partitioning) =
                partitioned_tables.get(&(table.owner.clone(), table.name.clone()))
            {
                add_oracle_partitioned_table_properties(
                    &mut properties,
                    partitioning,
                    &raw.partition_key_columns,
                );
            }
            metadata.annotations.push(ObjectAnnotation {
                object_key: key,
                definition: None,
                properties,
            });
        }

        let mut views = Vec::new();
        let mut view_keys = BTreeMap::new();
        let mut view_positions = BTreeMap::new();
        for view in &raw.views {
            let schema_key = required(
                schema_keys.get(&view.owner),
                format!("schema key for Oracle view {}.{}", view.owner, view.name),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &view.owner,
                ObjectKind::View,
                &view.name,
                None,
            );
            view_keys.insert((view.owner.clone(), view.name.clone()), key.clone());
            view_positions.insert((view.owner.clone(), view.name.clone()), views.len());
            views.push(ViewObject {
                key: key.clone(),
                schema_key: schema_key.clone(),
                name: view.name.clone(),
                definition: view.definition.clone(),
                depends_on: Vec::new(),
            });
            let inventory_object = required(
                inventory.get(&(view.owner.clone(), "VIEW".to_owned(), view.name.clone())),
                format!("inventory row for Oracle view {}.{}", view.owner, view.name),
            )?;
            let mut properties = inventory_properties(inventory_object);
            insert_optional_i64(&mut properties, "text_length", view.text_length);
            insert_optional_string(&mut properties, "editioning", view.editioning.as_deref());
            insert_optional_string(&mut properties, "read_only", view.read_only.as_deref());
            insert_optional_string(
                &mut properties,
                "container_data",
                view.container_data.as_deref(),
            );
            insert_optional_string(&mut properties, "bequeath", view.bequeath.as_deref());
            insert_optional_string(
                &mut properties,
                "default_collation",
                view.default_collation.as_deref(),
            );
            insert_optional_string(
                &mut properties,
                "has_sensitive_column",
                view.has_sensitive_column.as_deref(),
            );
            insert_optional_string(&mut properties, "admit_null", view.admit_null.as_deref());
            insert_optional_string(
                &mut properties,
                "pdb_local_only",
                view.pdb_local_only.as_deref(),
            );
            insert_optional_string(
                &mut properties,
                "duality_view",
                view.duality_view.as_deref(),
            );
            metadata.annotations.push(ObjectAnnotation {
                object_key: key,
                definition: None,
                properties,
            });
        }

        for column in &raw.view_columns {
            let view_key = required(
                view_keys.get(&(column.owner.clone(), column.table.clone())),
                format!(
                    "view key for Oracle output column {}.{}.{}",
                    column.owner, column.table, column.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &column.owner,
                ObjectKind::ViewColumn,
                &column.table,
                Some(column.name.clone()),
            );
            let mut properties = oracle_column_properties(column);
            insert_i64(
                &mut properties,
                "ordinal_position",
                i64::from(positive_u32(
                    column.internal_column_id,
                    "Oracle view-column ordinal",
                )?),
            );
            insert_string(
                &mut properties,
                "data_type",
                format_oracle_data_type(column),
            );
            insert_bool(&mut properties, "nullable", column.nullable);
            insert_optional_string(
                &mut properties,
                "default_value",
                column.default_value.as_deref(),
            );
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(view_key.clone()),
                name: column.name.clone(),
                extension_kind: None,
                definition: None,
                properties,
            });
            if let Some(owner) = column.data_type_owner.as_deref() {
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::UsesType,
                    from_key: key,
                    to_key: required(
                        type_keys.get(&(owner.to_owned(), column.data_type.clone())),
                        format!(
                            "type key for Oracle view column {}.{}.{}",
                            column.owner, column.table, column.name
                        ),
                    )?
                    .clone(),
                    ordinal: None,
                    properties: BTreeMap::new(),
                });
            }
        }

        let mut materialized_view_column_keys = BTreeMap::new();
        for column in raw.columns.iter().filter(|column| {
            materialized_view_names.contains(&(column.owner.clone(), column.table.clone()))
        }) {
            let view_key = required(
                materialized_view_keys.get(&(column.owner.clone(), column.table.clone())),
                format!(
                    "materialized-view key for Oracle output column {}.{}.{}",
                    column.owner, column.table, column.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &column.owner,
                ObjectKind::ViewColumn,
                &column.table,
                Some(column.name.clone()),
            );
            materialized_view_column_keys.insert(
                (
                    column.owner.clone(),
                    column.table.clone(),
                    column.name.clone(),
                ),
                key.clone(),
            );
            let mut properties = oracle_column_properties(column);
            insert_i64(
                &mut properties,
                "ordinal_position",
                i64::from(positive_u32(
                    column.internal_column_id,
                    "Oracle materialized-view column ordinal",
                )?),
            );
            insert_string(
                &mut properties,
                "data_type",
                format_oracle_data_type(column),
            );
            insert_bool(&mut properties, "nullable", column.nullable);
            insert_optional_string(
                &mut properties,
                "default_value",
                column.default_value.as_deref(),
            );
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(view_key.clone()),
                name: column.name.clone(),
                extension_kind: None,
                definition: None,
                properties,
            });
            if let Some(owner) = column.data_type_owner.as_deref() {
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::UsesType,
                    from_key: key,
                    to_key: required(
                        type_keys.get(&(owner.to_owned(), column.data_type.clone())),
                        format!(
                            "type key for Oracle materialized-view column {}.{}.{}",
                            column.owner, column.table, column.name
                        ),
                    )?
                    .clone(),
                    ordinal: None,
                    properties: BTreeMap::new(),
                });
            }
        }


        let mut routines = Vec::new();
        let mut routine_keys = BTreeMap::new();
        let mut routine_positions = BTreeMap::new();
        for routine in &raw.routines {
            let schema_key = required(
                schema_keys.get(&routine.owner),
                format!(
                    "schema key for Oracle routine {}.{}",
                    routine.owner, routine.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &routine.owner,
                ObjectKind::Routine,
                &routine.name,
                None,
            );
            let identity = (
                routine.owner.clone(),
                routine.name.clone(),
                routine.object_type.clone(),
            );
            routine_keys.insert(identity.clone(), key.clone());
            routine_positions.insert(identity, routines.len());
            routines.push(RoutineObject {
                key: key.clone(),
                schema_key: schema_key.clone(),
                name: routine.name.clone(),
                kind: match routine.object_type.as_str() {
                    "FUNCTION" => RoutineKind::Function,
                    "PROCEDURE" => RoutineKind::Procedure,
                    other => {
                        return Err(CatalogError::Mapping(format!(
                            "unmapped Oracle routine type '{other}'"
                        )));
                    }
                },
                definition: routine.definition.clone(),
                depends_on: Vec::new(),
            });
            let inventory_object = required(
                inventory.get(&(
                    routine.owner.clone(),
                    routine.object_type.clone(),
                    routine.name.clone(),
                )),
                format!(
                    "inventory row for Oracle routine {}.{}",
                    routine.owner, routine.name
                ),
            )?;
            metadata.annotations.push(ObjectAnnotation {
                object_key: key,
                definition: None,
                properties: oracle_routine_properties(routine, inventory_object),
            });
        }
        for argument in &raw.routine_arguments {
            let routine = raw
                .routines
                .iter()
                .find(|routine| routine.owner == argument.owner && routine.name == argument.routine)
                .ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "parent routine for Oracle argument {}.{}",
                        argument.owner, argument.routine
                    ))
                })?;
            let routine_key = required(
                routine_keys.get(&(
                    routine.owner.clone(),
                    routine.name.clone(),
                    routine.object_type.clone(),
                )),
                format!(
                    "parent key for Oracle argument {}.{}",
                    argument.owner, argument.routine
                ),
            )?;
            let display_name = if argument.position == 0 {
                "RETURN".to_owned()
            } else {
                argument
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("ARGUMENT_{}", argument.position))
            };
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &argument.owner,
                ObjectKind::RoutineParameter,
                &argument.routine,
                Some(format!("{}:{display_name}", argument.sequence)),
            );
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(routine_key.clone()),
                name: display_name,
                extension_kind: None,
                definition: argument.default_value.clone(),
                properties: oracle_routine_argument_properties(argument),
            });
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::HasParameter,
                from_key: routine_key.clone(),
                to_key: key.clone(),
                ordinal: Some(positive_u32(
                    argument.sequence,
                    "Oracle routine argument relationship ordinal",
                )?),
                properties: BTreeMap::new(),
            });
            if let (Some(owner), Some(name)) = (
                argument.type_owner.as_deref(),
                argument.type_name.as_deref(),
            ) {
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::UsesType,
                    from_key: key,
                    to_key: required(
                        type_keys.get(&(owner.to_owned(), name.to_owned())),
                        format!(
                            "type key for Oracle routine argument {}.{}",
                            argument.owner, argument.routine
                        ),
                    )?
                    .clone(),
                    ordinal: None,
                    properties: BTreeMap::new(),
                });
            }
        }

        let mut package_keys = BTreeMap::new();
        for package in &raw.packages {
            let schema_key = required(
                schema_keys.get(&package.owner),
                format!(
                    "schema key for Oracle package {}.{}",
                    package.owner, package.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &package.owner,
                ObjectKind::Package,
                &package.name,
                None,
            );
            package_keys.insert((package.owner.clone(), package.name.clone()), key.clone());
            let inventory_object = required(
                inventory.get(&(
                    package.owner.clone(),
                    "PACKAGE".to_owned(),
                    package.name.clone(),
                )),
                format!(
                    "inventory row for Oracle package {}.{}",
                    package.owner, package.name
                ),
            )?;
            let body_inventory = inventory
                .get(&(
                    package.owner.clone(),
                    "PACKAGE BODY".to_owned(),
                    package.name.clone(),
                ))
                .copied();
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(schema_key.clone()),
                name: package.name.clone(),
                extension_kind: None,
                definition: Some(oracle_package_definition(package)?),
                properties: oracle_package_properties(package, inventory_object, body_inventory),
            });
        }
        let package_arguments_by_routine = raw.package_arguments.iter().fold(
            BTreeMap::<(String, String, i64), Vec<&RawRoutineArgument>>::new(),
            |mut map, argument| {
                if let Some(package) = argument.package_name.as_deref() {
                    map.entry((
                        argument.owner.clone(),
                        package.to_owned(),
                        argument.subprogram_id,
                    ))
                    .or_default()
                    .push(argument);
                }
                map
            },
        );
        let mut package_routine_keys = BTreeMap::new();
        let mut package_routine_signatures = BTreeMap::new();
        for routine in &raw.package_routines {
            let package_key = required(
                package_keys.get(&(routine.owner.clone(), routine.package.clone())),
                format!(
                    "package key for Oracle routine {}.{}.{}",
                    routine.owner, routine.package, routine.name
                ),
            )?;
            let arguments = package_arguments_by_routine
                .get(&(
                    routine.owner.clone(),
                    routine.package.clone(),
                    routine.subprogram_id,
                ))
                .map(Vec::as_slice)
                .unwrap_or_default();
            let signature = oracle_package_routine_signature(routine, arguments)?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &routine.owner,
                ObjectKind::Routine,
                &routine.package,
                Some(signature.clone()),
            );
            let identity = (
                routine.owner.clone(),
                routine.package.clone(),
                routine.subprogram_id,
            );
            package_routine_keys.insert(identity.clone(), key.clone());
            package_routine_signatures.insert(identity, signature.clone());
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(package_key.clone()),
                name: routine.name.clone(),
                extension_kind: None,
                definition: None,
                properties: oracle_package_routine_properties(routine, &signature),
            });
        }
        for argument in &raw.package_arguments {
            let package_name = argument.package_name.as_deref().ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle package argument {}.{} has no package",
                    argument.owner, argument.routine
                ))
            })?;
            let identity = (
                argument.owner.clone(),
                package_name.to_owned(),
                argument.subprogram_id,
            );
            let routine_key = required(
                package_routine_keys.get(&identity),
                format!(
                    "package routine key for Oracle argument {}.{}.{}",
                    argument.owner, package_name, argument.routine
                ),
            )?;
            let signature = required(
                package_routine_signatures.get(&identity),
                format!(
                    "package routine signature for Oracle argument {}.{}.{}",
                    argument.owner, package_name, argument.routine
                ),
            )?;
            let display_name = if argument.position == 0 {
                "RETURN".to_owned()
            } else {
                argument
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("ARGUMENT_{}", argument.position))
            };
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &argument.owner,
                ObjectKind::RoutineParameter,
                package_name,
                Some(format!("{signature}#{}:{display_name}", argument.sequence)),
            );
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(routine_key.clone()),
                name: display_name,
                extension_kind: None,
                definition: argument.default_value.clone(),
                properties: oracle_routine_argument_properties(argument),
            });
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::HasParameter,
                from_key: routine_key.clone(),
                to_key: key.clone(),
                ordinal: Some(positive_u32(
                    argument.sequence,
                    "Oracle package argument relationship ordinal",
                )?),
                properties: BTreeMap::new(),
            });
            if let (Some(owner), Some(name)) = (
                argument.type_owner.as_deref(),
                argument.type_name.as_deref(),
            ) {
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::UsesType,
                    from_key: key,
                    to_key: required(
                        type_keys.get(&(owner.to_owned(), name.to_owned())),
                        format!(
                            "type key for Oracle package argument {}.{}.{}",
                            argument.owner, package_name, argument.routine
                        ),
                    )?
                    .clone(),
                    ordinal: None,
                    properties: BTreeMap::new(),
                });
            }
        }


        let mut synonym_keys = BTreeMap::new();
        for synonym in &raw.synonyms {
            let schema_key = required(
                schema_keys.get(&synonym.owner),
                format!(
                    "schema key for Oracle synonym {}.{}",
                    synonym.owner, synonym.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &synonym.owner,
                ObjectKind::Synonym,
                &synonym.name,
                None,
            );
            synonym_keys.insert((synonym.owner.clone(), synonym.name.clone()), key.clone());
            let inventory_object = required(
                inventory.get(&(
                    synonym.owner.clone(),
                    "SYNONYM".to_owned(),
                    synonym.name.clone(),
                )),
                format!(
                    "inventory row for Oracle synonym {}.{}",
                    synonym.owner, synonym.name
                ),
            )?;
            let mut properties = inventory_properties(inventory_object);
            insert_string(&mut properties, "target_owner", &synonym.target_owner);
            insert_string(&mut properties, "target_name", &synonym.target_name);
            insert_optional_string(
                &mut properties,
                "database_link",
                synonym.database_link.as_deref(),
            );
            insert_i64(
                &mut properties,
                "origin_container_id",
                synonym.origin_container_id,
            );
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(schema_key.clone()),
                name: synonym.name.clone(),
                extension_kind: None,
                definition: None,
                properties,
            });
        }
        for dependency in raw
            .dependencies
            .iter()
            .filter(|dependency| dependency.object_type == "SYNONYM")
            .filter(|dependency| !dependency.referenced_owner_oracle_maintained)
        {
            let source_key = required(
                synonym_keys.get(&(dependency.owner.clone(), dependency.name.clone())),
                format!(
                    "source key for Oracle synonym dependency {}.{}",
                    dependency.owner, dependency.name
                ),
            )?;
            let target_key = match dependency.referenced_type.as_str() {
                "TABLE" => match materialized_view_keys.get(&(
                    dependency.referenced_owner.clone(),
                    dependency.referenced_name.clone(),
                )) {
                    Some(key) => key,
                    None => required(
                        table_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                        )),
                        format!(
                            "table target for Oracle synonym dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                },
                "VIEW" => required(
                    view_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "view target for Oracle synonym dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "MATERIALIZED VIEW" => required(
                    materialized_view_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "materialized-view target for Oracle synonym dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "SEQUENCE" => required(
                    sequence_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "sequence target for Oracle synonym dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "FUNCTION" | "PROCEDURE" => required(
                    routine_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                        dependency.referenced_type.clone(),
                    )),
                    format!(
                        "routine target for Oracle synonym dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "PACKAGE" => required(
                    package_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "package target for Oracle synonym dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "SYNONYM" => required(
                    synonym_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "synonym target for Oracle dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "TYPE" => required(
                    type_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "type target for Oracle synonym dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                other => {
                    return Err(CatalogError::Mapping(format!(
                        "unmapped Oracle synonym target type '{other}'"
                    )));
                }
            };
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::SynonymFor,
                from_key: source_key.clone(),
                to_key: target_key.clone(),
                ordinal: None,
                properties: BTreeMap::from([(
                    "oracle_dependency_type".to_owned(),
                    MetadataValue::String(dependency.dependency_type.clone()),
                )]),
            });
        }

        for dependency in &raw.dependencies {
            if dependency.object_type != "VIEW" || dependency.referenced_owner_oracle_maintained {
                continue;
            }
            let source_position = required(
                view_positions.get(&(dependency.owner.clone(), dependency.name.clone())),
                format!(
                    "view position for Oracle dependency {}.{}",
                    dependency.owner, dependency.name
                ),
            )?;
            let target_key = match dependency.referenced_type.as_str() {
                "TABLE" => match materialized_view_keys.get(&(
                    dependency.referenced_owner.clone(),
                    dependency.referenced_name.clone(),
                )) {
                    Some(key) => key,
                    None => required(
                        table_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                        )),
                        format!(
                            "table target for Oracle view dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                },
                "VIEW" => required(
                    view_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "view target for Oracle view dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "MATERIALIZED VIEW" => required(
                    materialized_view_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "materialized-view target for Oracle view dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "SEQUENCE" => required(
                    sequence_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "sequence target for Oracle view dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "FUNCTION" | "PROCEDURE" => required(
                    routine_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                        dependency.referenced_type.clone(),
                    )),
                    format!(
                        "routine target for Oracle view dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "PACKAGE" => required(
                    package_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "package target for Oracle view dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "TYPE" => required(
                    type_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "type target for Oracle view dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                other => {
                    return Err(CatalogError::Mapping(format!(
                        "unmapped Oracle view dependency target type '{other}'"
                    )));
                }
            };
            if dependency.referenced_type == "TYPE" {
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::DependsOn,
                    from_key: views[*source_position].key.clone(),
                    to_key: target_key.clone(),
                    ordinal: None,
                    properties: BTreeMap::from([(
                        "oracle_dependency_type".to_owned(),
                        MetadataValue::String(dependency.dependency_type.clone()),
                    )]),
                });
            } else {
                views[*source_position].depends_on.push(target_key.clone());
            }
        }

        for dependency in raw
            .dependencies
            .iter()
            .filter(|dependency| dependency.object_type == "MATERIALIZED VIEW")
            .filter(|dependency| !dependency.referenced_owner_oracle_maintained)
        {
            if dependency.referenced_type == "TABLE"
                && dependency.owner == dependency.referenced_owner
                && dependency.name == dependency.referenced_name
            {
                continue;
            }
            let source_key = required(
                materialized_view_keys.get(&(dependency.owner.clone(), dependency.name.clone())),
                format!(
                    "source key for Oracle materialized-view dependency {}.{}",
                    dependency.owner, dependency.name
                ),
            )?;
            let (target_key, relationship_kind) = match dependency.referenced_type.as_str() {
                "TABLE" => match materialized_view_keys.get(&(
                    dependency.referenced_owner.clone(),
                    dependency.referenced_name.clone(),
                )) {
                    Some(key) => (key, MetadataRelationshipKind::DependsOn),
                    None => (
                        required(
                            table_keys.get(&(
                                dependency.referenced_owner.clone(),
                                dependency.referenced_name.clone(),
                            )),
                            format!(
                                "table target for Oracle materialized-view dependency {}.{}",
                                dependency.referenced_owner, dependency.referenced_name
                            ),
                        )?,
                        MetadataRelationshipKind::Materializes,
                    ),
                },
                "VIEW" => (
                    required(
                        view_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                        )),
                        format!(
                            "view target for Oracle materialized-view dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                    MetadataRelationshipKind::Materializes,
                ),
                "MATERIALIZED VIEW" => (
                    required(
                        materialized_view_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                        )),
                        format!(
                            "materialized-view target for Oracle dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                    MetadataRelationshipKind::DependsOn,
                ),
                "SEQUENCE" => (
                    required(
                        sequence_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                        )),
                        format!(
                            "sequence target for Oracle materialized-view dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                    MetadataRelationshipKind::DependsOn,
                ),
                "FUNCTION" | "PROCEDURE" => (
                    required(
                        routine_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                            dependency.referenced_type.clone(),
                        )),
                        format!(
                            "routine target for Oracle materialized-view dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                    MetadataRelationshipKind::DependsOn,
                ),
                "PACKAGE" => (
                    required(
                        package_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                        )),
                        format!(
                            "package target for Oracle materialized-view dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                    MetadataRelationshipKind::DependsOn,
                ),
                "TYPE" => (
                    required(
                        type_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                        )),
                        format!(
                            "type target for Oracle materialized-view dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                    MetadataRelationshipKind::DependsOn,
                ),
                other => {
                    return Err(CatalogError::Mapping(format!(
                        "unmapped Oracle materialized-view dependency target type '{other}'"
                    )));
                }
            };
            let mut properties = BTreeMap::new();
            insert_string(
                &mut properties,
                "oracle_dependency_type",
                &dependency.dependency_type,
            );
            metadata.relationships.push(MetadataRelationship {
                kind: relationship_kind,
                from_key: source_key.clone(),
                to_key: target_key.clone(),
                ordinal: None,
                properties,
            });
        }

        for dependency in raw
            .dependencies
            .iter()
            .filter(|dependency| {
                matches!(dependency.object_type.as_str(), "FUNCTION" | "PROCEDURE")
            })
            .filter(|dependency| !dependency.referenced_owner_oracle_maintained)
        {
            let source_identity = (
                dependency.owner.clone(),
                dependency.name.clone(),
                dependency.object_type.clone(),
            );
            let source_position = required(
                routine_positions.get(&source_identity),
                format!(
                    "source position for Oracle routine dependency {}.{}",
                    dependency.owner, dependency.name
                ),
            )?;
            let target_key = match dependency.referenced_type.as_str() {
                "TABLE" => match materialized_view_keys.get(&(
                    dependency.referenced_owner.clone(),
                    dependency.referenced_name.clone(),
                )) {
                    Some(key) => key,
                    None => required(
                        table_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                        )),
                        format!(
                            "table target for Oracle routine dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                },
                "VIEW" => required(
                    view_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "view target for Oracle routine dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "MATERIALIZED VIEW" => required(
                    materialized_view_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "materialized-view target for Oracle routine dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "SEQUENCE" => required(
                    sequence_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "sequence target for Oracle routine dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "FUNCTION" | "PROCEDURE" => required(
                    routine_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                        dependency.referenced_type.clone(),
                    )),
                    format!(
                        "routine target for Oracle routine dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "PACKAGE" => required(
                    package_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "package target for Oracle routine dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "TYPE" => required(
                    type_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "type target for Oracle routine dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                other => {
                    return Err(CatalogError::Mapping(format!(
                        "unmapped Oracle routine dependency target type '{other}'"
                    )));
                }
            };
            if dependency.referenced_type == "TYPE" {
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::DependsOn,
                    from_key: routines[*source_position].key.clone(),
                    to_key: target_key.clone(),
                    ordinal: None,
                    properties: BTreeMap::from([(
                        "oracle_dependency_type".to_owned(),
                        MetadataValue::String(dependency.dependency_type.clone()),
                    )]),
                });
            } else {
                routines[*source_position]
                    .depends_on
                    .push(target_key.clone());
            }
        }

        for (identity, evidence) in oracle_package_dependency_groups(&raw.dependencies) {
            let (owner, package, referenced_owner, referenced_name, referenced_type) = identity;
            let source_key = required(
                package_keys.get(&(owner.clone(), package.clone())),
                format!("source key for Oracle package dependency {owner}.{package}"),
            )?;
            let target_key = match referenced_type.as_str() {
                "TABLE" => match materialized_view_keys
                    .get(&(referenced_owner.clone(), referenced_name.clone()))
                {
                    Some(key) => key,
                    None => required(
                        table_keys.get(&(referenced_owner.clone(), referenced_name.clone())),
                        format!(
                            "table target for Oracle package dependency {referenced_owner}.{referenced_name}"
                        ),
                    )?,
                },
                "VIEW" => required(
                    view_keys.get(&(referenced_owner.clone(), referenced_name.clone())),
                    format!(
                        "view target for Oracle package dependency {referenced_owner}.{referenced_name}"
                    ),
                )?,
                "MATERIALIZED VIEW" => required(
                    materialized_view_keys
                        .get(&(referenced_owner.clone(), referenced_name.clone())),
                    format!(
                        "materialized-view target for Oracle package dependency {referenced_owner}.{referenced_name}"
                    ),
                )?,
                "SEQUENCE" => required(
                    sequence_keys.get(&(referenced_owner.clone(), referenced_name.clone())),
                    format!(
                        "sequence target for Oracle package dependency {referenced_owner}.{referenced_name}"
                    ),
                )?,
                "FUNCTION" | "PROCEDURE" => required(
                    routine_keys.get(&(
                        referenced_owner.clone(),
                        referenced_name.clone(),
                        referenced_type.clone(),
                    )),
                    format!(
                        "routine target for Oracle package dependency {referenced_owner}.{referenced_name}"
                    ),
                )?,
                "PACKAGE" => required(
                    package_keys.get(&(referenced_owner.clone(), referenced_name.clone())),
                    format!(
                        "package target for Oracle package dependency {referenced_owner}.{referenced_name}"
                    ),
                )?,
                "TYPE" => required(
                    type_keys.get(&(referenced_owner.clone(), referenced_name.clone())),
                    format!(
                        "type target for Oracle package dependency {referenced_owner}.{referenced_name}"
                    ),
                )?,
                other => {
                    return Err(CatalogError::Mapping(format!(
                        "unmapped Oracle package dependency target type '{other}'"
                    )));
                }
            };
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::DependsOn,
                from_key: source_key.clone(),
                to_key: target_key.clone(),
                ordinal: None,
                properties: BTreeMap::from([
                    (
                        "oracle_source_object_types".to_owned(),
                        MetadataValue::StringList(
                            evidence.source_object_types.into_iter().collect(),
                        ),
                    ),
                    (
                        "oracle_dependency_types".to_owned(),
                        MetadataValue::StringList(evidence.dependency_types.into_iter().collect()),
                    ),
                ]),
            });
        }

        for (identity, evidence) in oracle_type_dependency_groups(&raw.dependencies) {
            let (owner, type_name, referenced_owner, referenced_name, referenced_type) = identity;
            let source_key = required(
                type_keys.get(&(owner.clone(), type_name.clone())),
                format!("source key for Oracle type dependency {owner}.{type_name}"),
            )?;
            let target_key = match referenced_type.as_str() {
                "TABLE" => match materialized_view_keys
                    .get(&(referenced_owner.clone(), referenced_name.clone()))
                {
                    Some(key) => key,
                    None => required(
                        table_keys.get(&(referenced_owner.clone(), referenced_name.clone())),
                        format!(
                            "table target for Oracle type dependency {referenced_owner}.{referenced_name}"
                        ),
                    )?,
                },
                "VIEW" => required(
                    view_keys.get(&(referenced_owner.clone(), referenced_name.clone())),
                    format!(
                        "view target for Oracle type dependency {referenced_owner}.{referenced_name}"
                    ),
                )?,
                "MATERIALIZED VIEW" => required(
                    materialized_view_keys
                        .get(&(referenced_owner.clone(), referenced_name.clone())),
                    format!(
                        "materialized-view target for Oracle type dependency {referenced_owner}.{referenced_name}"
                    ),
                )?,
                "SEQUENCE" => required(
                    sequence_keys.get(&(referenced_owner.clone(), referenced_name.clone())),
                    format!(
                        "sequence target for Oracle type dependency {referenced_owner}.{referenced_name}"
                    ),
                )?,
                "FUNCTION" | "PROCEDURE" => required(
                    routine_keys.get(&(
                        referenced_owner.clone(),
                        referenced_name.clone(),
                        referenced_type.clone(),
                    )),
                    format!(
                        "routine target for Oracle type dependency {referenced_owner}.{referenced_name}"
                    ),
                )?,
                "PACKAGE" => required(
                    package_keys.get(&(referenced_owner.clone(), referenced_name.clone())),
                    format!(
                        "package target for Oracle type dependency {referenced_owner}.{referenced_name}"
                    ),
                )?,
                "SYNONYM" => required(
                    synonym_keys.get(&(referenced_owner.clone(), referenced_name.clone())),
                    format!(
                        "synonym target for Oracle type dependency {referenced_owner}.{referenced_name}"
                    ),
                )?,
                "TYPE" => required(
                    type_keys.get(&(referenced_owner.clone(), referenced_name.clone())),
                    format!(
                        "type target for Oracle dependency {referenced_owner}.{referenced_name}"
                    ),
                )?,
                other => {
                    return Err(CatalogError::Mapping(format!(
                        "unmapped Oracle type dependency target type '{other}'"
                    )));
                }
            };
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::DependsOn,
                from_key: source_key.clone(),
                to_key: target_key.clone(),
                ordinal: None,
                properties: BTreeMap::from([
                    (
                        "oracle_source_object_types".to_owned(),
                        MetadataValue::StringList(
                            evidence.source_object_types.into_iter().collect(),
                        ),
                    ),
                    (
                        "oracle_dependency_types".to_owned(),
                        MetadataValue::StringList(evidence.dependency_types.into_iter().collect()),
                    ),
                ]),
            });
        }

        let identities = raw
            .identity_columns
            .iter()
            .map(|identity| {
                (
                    (
                        identity.owner.clone(),
                        identity.table.clone(),
                        identity.column.clone(),
                    ),
                    identity,
                )
            })
            .collect::<BTreeMap<_, _>>();

        let mut columns = Vec::new();
        let mut column_keys = BTreeMap::new();
        for column in &raw.columns {
            if materialized_view_names.contains(&(column.owner.clone(), column.table.clone())) {
                continue;
            }
            let table_key = required(
                table_keys.get(&(column.owner.clone(), column.table.clone())),
                format!(
                    "table key for Oracle column {}.{}.{}",
                    column.owner, column.table, column.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &column.owner,
                ObjectKind::Column,
                &column.table,
                Some(column.name.clone()),
            );
            column_keys.insert(
                (
                    column.owner.clone(),
                    column.table.clone(),
                    column.name.clone(),
                ),
                key.clone(),
            );
            columns.push(ColumnObject {
                key: key.clone(),
                table_key: table_key.clone(),
                name: column.name.clone(),
                ordinal_position: positive_u32(
                    column.internal_column_id,
                    "Oracle internal column ordinal",
                )?,
                data_type: format_oracle_data_type(column),
                is_nullable: column.nullable,
                default_value: column.default_value.clone(),
                is_generated: column.virtual_column
                    || column.hidden
                    || !column.user_generated
                    || column.identity,
            });
            let mut properties = oracle_column_properties(column);
            if let Some(identity) = identities.get(&(
                column.owner.clone(),
                column.table.clone(),
                column.name.clone(),
            )) {
                insert_optional_string(
                    &mut properties,
                    "identity_generation_type",
                    identity.generation_type.as_deref(),
                );
                insert_optional_string(
                    &mut properties,
                    "identity_options",
                    identity.options.as_deref(),
                );
                let sequence_key = required(
                    sequence_keys.get(&(identity.owner.clone(), identity.sequence_name.clone())),
                    format!(
                        "identity sequence key {}.{}",
                        identity.owner, identity.sequence_name
                    ),
                )?;
                let mut relationship_properties = BTreeMap::new();
                insert_optional_string(
                    &mut relationship_properties,
                    "generation_type",
                    identity.generation_type.as_deref(),
                );
                insert_optional_string(
                    &mut relationship_properties,
                    "identity_options",
                    identity.options.as_deref(),
                );
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::UsesSequence,
                    from_key: key.clone(),
                    to_key: sequence_key.clone(),
                    ordinal: None,
                    properties: relationship_properties,
                });
            }
            metadata.annotations.push(ObjectAnnotation {
                object_key: key.clone(),
                definition: None,
                properties,
            });
            if let Some(owner) = column.data_type_owner.as_deref() {
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::UsesType,
                    from_key: key,
                    to_key: required(
                        type_keys.get(&(owner.to_owned(), column.data_type.clone())),
                        format!(
                            "type key for Oracle column {}.{}.{}",
                            column.owner, column.table, column.name
                        ),
                    )?
                    .clone(),
                    ordinal: None,
                    properties: BTreeMap::new(),
                });
            }
        }

        let constraint_by_identity = raw
            .constraints
            .iter()
            .map(|constraint| {
                (
                    (constraint.owner.clone(), constraint.name.clone()),
                    constraint,
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut constraints = Vec::new();
        for constraint in &raw.constraints {
            if let Some(materialized_view_key) =
                materialized_view_keys.get(&(constraint.owner.clone(), constraint.table.clone()))
            {
                let object_kind = match constraint.constraint_type.as_str() {
                    "P" => ObjectKind::PrimaryKey,
                    "U" => ObjectKind::UniqueConstraint,
                    "C" => ObjectKind::CheckConstraint,
                    other => {
                        return Err(CatalogError::Mapping(format!(
                            "unmapped Oracle materialized-view constraint type '{other}'"
                        )));
                    }
                };
                let key = oracle_key(
                    self.connection_alias,
                    &database_name,
                    &constraint.owner,
                    object_kind,
                    &constraint.table,
                    Some(constraint.name.clone()),
                );
                metadata.objects.push(MetadataObject {
                    key: key.clone(),
                    parent_key: Some(materialized_view_key.clone()),
                    name: constraint.name.clone(),
                    extension_kind: None,
                    definition: constraint.search_condition.clone(),
                    properties: constraint_properties(constraint),
                });
                for column in &constraint.columns {
                    let column_key = required(
                        materialized_view_column_keys.get(&(
                            constraint.owner.clone(),
                            constraint.table.clone(),
                            column.name.clone(),
                        )),
                        format!(
                            "column {} for Oracle materialized-view constraint {}.{}",
                            column.name, constraint.owner, constraint.name
                        ),
                    )?;
                    metadata.relationships.push(MetadataRelationship {
                        kind: MetadataRelationshipKind::Extension(
                            "oracle_constraint_column".to_owned(),
                        ),
                        from_key: key.clone(),
                        to_key: column_key.clone(),
                        ordinal: column
                            .position
                            .map(|position| {
                                positive_u32(
                                    position,
                                    "Oracle materialized-view constraint ordinal",
                                )
                            })
                            .transpose()?,
                        properties: BTreeMap::new(),
                    });
                }
                continue;
            }
            let table_key = required(
                table_keys.get(&(constraint.owner.clone(), constraint.table.clone())),
                format!(
                    "table key for Oracle constraint {}.{}",
                    constraint.owner, constraint.name
                ),
            )?;
            let (kind, object_kind) = match constraint.constraint_type.as_str() {
                "P" => (ConstraintKind::PrimaryKey, ObjectKind::PrimaryKey),
                "U" => (ConstraintKind::Unique, ObjectKind::UniqueConstraint),
                "R" => (ConstraintKind::ForeignKey, ObjectKind::ForeignKey),
                "C" => (ConstraintKind::Check, ObjectKind::CheckConstraint),
                other => {
                    return Err(CatalogError::Mapping(format!(
                        "unmapped Oracle constraint type '{other}' for {}.{}",
                        constraint.owner, constraint.name
                    )));
                }
            };
            let local_columns = resolve_named_columns(
                &constraint.owner,
                &constraint.table,
                &constraint.columns,
                &column_keys,
                &constraint.name,
            )?;
            let (referenced_table_key, referenced_columns) = if kind == ConstraintKind::ForeignKey {
                let referenced_owner = constraint.referenced_owner.as_deref().ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "foreign key {}.{} has no referenced owner",
                        constraint.owner, constraint.name
                    ))
                })?;
                let referenced_name =
                    constraint.referenced_constraint.as_deref().ok_or_else(|| {
                        CatalogError::Mapping(format!(
                            "foreign key {}.{} has no referenced constraint",
                            constraint.owner, constraint.name
                        ))
                    })?;
                let referenced = required(
                    constraint_by_identity
                        .get(&(referenced_owner.to_owned(), referenced_name.to_owned())),
                    format!(
                        "referenced Oracle constraint {}.{}",
                        referenced_owner, referenced_name
                    ),
                )?;
                let referenced_table = required(
                    table_keys.get(&(referenced.owner.clone(), referenced.table.clone())),
                    format!(
                        "referenced Oracle table {}.{}",
                        referenced.owner, referenced.table
                    ),
                )?;
                let referenced_columns = resolve_named_columns(
                    &referenced.owner,
                    &referenced.table,
                    &referenced.columns,
                    &column_keys,
                    &constraint.name,
                )?;
                (Some(referenced_table.clone()), referenced_columns)
            } else {
                (None, Vec::new())
            };
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &constraint.owner,
                object_kind,
                &constraint.table,
                Some(constraint.name.clone()),
            );
            constraints.push(ConstraintObject {
                key: key.clone(),
                table_key: table_key.clone(),
                name: constraint.name.clone(),
                kind,
                columns: local_columns,
                referenced_table_key,
                referenced_columns,
                expression: (kind == ConstraintKind::Check)
                    .then(|| constraint.search_condition.clone())
                    .flatten(),
            });
            metadata.annotations.push(ObjectAnnotation {
                object_key: key,
                definition: None,
                properties: constraint_properties(constraint),
            });
        }

        let primary_indexes = raw
            .constraints
            .iter()
            .filter(|constraint| constraint.constraint_type == "P")
            .filter_map(|constraint| {
                Some((
                    constraint.index_owner.clone()?,
                    constraint.index_name.clone()?,
                ))
            })
            .collect::<BTreeSet<_>>();
        let mut indexes = Vec::new();
        let mut index_keys = BTreeMap::new();
        for index in &raw.indexes {
            let expression = oracle_index_expression(index);
            let inventory_object = required(
                inventory.get(&(index.owner.clone(), "INDEX".to_owned(), index.name.clone())),
                format!(
                    "inventory row for Oracle index {}.{}",
                    index.owner, index.name
                ),
            )?;
            let mut properties = oracle_index_properties(index, inventory_object);
            if let Some(partitioning) =
                partitioned_indexes.get(&(index.owner.clone(), index.name.clone()))
            {
                add_oracle_partitioned_index_properties(
                    &mut properties,
                    partitioning,
                    &raw.partition_key_columns,
                );
            }
            if let Some(materialized_view_key) =
                materialized_view_keys.get(&(index.table_owner.clone(), index.table.clone()))
            {
                let key = oracle_key(
                    self.connection_alias,
                    &database_name,
                    &index.table_owner,
                    ObjectKind::Index,
                    &index.table,
                    Some(index.name.clone()),
                );
                index_keys.insert((index.owner.clone(), index.name.clone()), key.clone());
                metadata.objects.push(MetadataObject {
                    key: key.clone(),
                    parent_key: Some(materialized_view_key.clone()),
                    name: index.name.clone(),
                    extension_kind: None,
                    definition: expression,
                    properties,
                });
                for column in index
                    .columns
                    .iter()
                    .filter(|column| column.expression.is_none())
                {
                    let column_key = required(
                        materialized_view_column_keys.get(&(
                            index.table_owner.clone(),
                            index.table.clone(),
                            column.name.clone(),
                        )),
                        format!(
                            "column {} for Oracle materialized-view index {}.{}",
                            column.name, index.owner, index.name
                        ),
                    )?;
                    metadata.relationships.push(MetadataRelationship {
                        kind: MetadataRelationshipKind::IncludesColumn,
                        from_key: key.clone(),
                        to_key: column_key.clone(),
                        ordinal: Some(positive_u32(
                            column.position,
                            "Oracle materialized-view index ordinal",
                        )?),
                        properties: BTreeMap::from([(
                            "descending".to_owned(),
                            MetadataValue::Boolean(column.descending),
                        )]),
                    });
                }
                continue;
            }
            let table_key = required(
                table_keys.get(&(index.table_owner.clone(), index.table.clone())),
                format!("table key for Oracle index {}.{}", index.owner, index.name),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &index.table_owner,
                ObjectKind::Index,
                &index.table,
                Some(index.name.clone()),
            );
            index_keys.insert((index.owner.clone(), index.name.clone()), key.clone());
            let direct_columns = index
                .columns
                .iter()
                .filter(|column| column.expression.is_none())
                .cloned()
                .collect::<Vec<_>>();
            let index_columns = resolve_named_columns(
                &index.table_owner,
                &index.table,
                &direct_columns,
                &column_keys,
                &index.name,
            )?;
            indexes.push(IndexObject {
                key: key.clone(),
                table_key: table_key.clone(),
                name: index.name.clone(),
                columns: index_columns,
                is_unique: index.unique,
                is_primary: primary_indexes.contains(&(index.owner.clone(), index.name.clone())),
                predicate: None,
                expression: expression.clone(),
            });
            metadata.annotations.push(ObjectAnnotation {
                object_key: key,
                definition: expression,
                properties,
            });
        }


        let mut table_partition_keys = BTreeMap::new();
        for partition in &raw.table_partitions {
            let parent_key = match materialized_view_keys
                .get(&(partition.owner.clone(), partition.table.clone()))
            {
                Some(key) => key,
                None => required(
                    table_keys.get(&(partition.owner.clone(), partition.table.clone())),
                    format!(
                        "parent table for Oracle partition {}.{}.{}",
                        partition.owner, partition.table, partition.name
                    ),
                )?,
            };
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &partition.owner,
                ObjectKind::Extension,
                &partition.table,
                Some(format!("partition:{}", partition.name)),
            );
            let inventory_object = required(
                subobject_inventory.get(&(
                    partition.owner.clone(),
                    "TABLE PARTITION".to_owned(),
                    partition.table.clone(),
                    partition.name.clone(),
                )),
                format!(
                    "inventory row for Oracle table partition {}.{}.{}",
                    partition.owner, partition.table, partition.name
                ),
            )?;
            table_partition_keys.insert(
                (
                    partition.owner.clone(),
                    partition.table.clone(),
                    partition.name.clone(),
                ),
                key.clone(),
            );
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(parent_key.clone()),
                name: partition.name.clone(),
                extension_kind: Some("oracle_table_partition".to_owned()),
                definition: partition.high_value.clone(),
                properties: oracle_table_partition_properties(partition, inventory_object),
            });
        }
        let mut table_subpartition_keys = BTreeMap::new();
        for subpartition in &raw.table_subpartitions {
            let parent_key = required(
                table_partition_keys.get(&(
                    subpartition.owner.clone(),
                    subpartition.table.clone(),
                    subpartition.partition.clone(),
                )),
                format!(
                    "parent partition for Oracle table subpartition {}.{}.{}",
                    subpartition.owner, subpartition.table, subpartition.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &subpartition.owner,
                ObjectKind::Extension,
                &subpartition.table,
                Some(format!(
                    "partition:{}:subpartition:{}",
                    subpartition.partition, subpartition.name
                )),
            );
            let inventory_object = required(
                subobject_inventory.get(&(
                    subpartition.owner.clone(),
                    "TABLE SUBPARTITION".to_owned(),
                    subpartition.table.clone(),
                    subpartition.name.clone(),
                )),
                format!(
                    "inventory row for Oracle table subpartition {}.{}.{}",
                    subpartition.owner, subpartition.table, subpartition.name
                ),
            )?;
            table_subpartition_keys.insert(
                (
                    subpartition.owner.clone(),
                    subpartition.table.clone(),
                    subpartition.name.clone(),
                ),
                key.clone(),
            );
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(parent_key.clone()),
                name: subpartition.name.clone(),
                extension_kind: Some("oracle_table_subpartition".to_owned()),
                definition: subpartition.high_value.clone(),
                properties: oracle_table_subpartition_properties(subpartition, inventory_object),
            });
        }

        let mut index_partition_keys = BTreeMap::new();
        for partition in &raw.index_partitions {
            let parent_key = required(
                index_keys.get(&(partition.owner.clone(), partition.index.clone())),
                format!(
                    "parent index for Oracle partition {}.{}.{}",
                    partition.owner, partition.index, partition.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &partition.owner,
                ObjectKind::Extension,
                &parent_key.object_name,
                Some(format!(
                    "index:{}:partition:{}",
                    partition.index, partition.name
                )),
            );
            let inventory_object = required(
                subobject_inventory.get(&(
                    partition.owner.clone(),
                    "INDEX PARTITION".to_owned(),
                    partition.index.clone(),
                    partition.name.clone(),
                )),
                format!(
                    "inventory row for Oracle index partition {}.{}.{}",
                    partition.owner, partition.index, partition.name
                ),
            )?;
            index_partition_keys.insert(
                (
                    partition.owner.clone(),
                    partition.index.clone(),
                    partition.name.clone(),
                ),
                key.clone(),
            );
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(parent_key.clone()),
                name: partition.name.clone(),
                extension_kind: Some("oracle_index_partition".to_owned()),
                definition: partition.high_value.clone(),
                properties: oracle_index_partition_properties(partition, inventory_object),
            });
        }
        for subpartition in &raw.index_subpartitions {
            let parent_key = required(
                index_partition_keys.get(&(
                    subpartition.owner.clone(),
                    subpartition.index.clone(),
                    subpartition.partition.clone(),
                )),
                format!(
                    "parent partition for Oracle index subpartition {}.{}.{}",
                    subpartition.owner, subpartition.index, subpartition.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &subpartition.owner,
                ObjectKind::Extension,
                &parent_key.object_name,
                Some(format!(
                    "index:{}:partition:{}:subpartition:{}",
                    subpartition.index, subpartition.partition, subpartition.name
                )),
            );
            let inventory_object = required(
                subobject_inventory.get(&(
                    subpartition.owner.clone(),
                    "INDEX SUBPARTITION".to_owned(),
                    subpartition.index.clone(),
                    subpartition.name.clone(),
                )),
                format!(
                    "inventory row for Oracle index subpartition {}.{}.{}",
                    subpartition.owner, subpartition.index, subpartition.name
                ),
            )?;
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(parent_key.clone()),
                name: subpartition.name.clone(),
                extension_kind: Some("oracle_index_subpartition".to_owned()),
                definition: subpartition.high_value.clone(),
                properties: oracle_index_subpartition_properties(subpartition, inventory_object),
            });
        }

        let mut lob_keys = BTreeMap::new();
        for lob in &raw.lobs {
            let parent_key = required(
                column_keys.get(&(lob.owner.clone(), lob.table.clone(), lob.column.clone())),
                format!(
                    "parent column for Oracle LOB {}.{}.{}",
                    lob.owner, lob.table, lob.column
                ),
            )?;
            let segment_inventory = required(
                inventory.get(&(
                    lob.owner.clone(),
                    "LOB".to_owned(),
                    lob.segment_name.clone(),
                )),
                format!(
                    "inventory row for Oracle LOB segment {}.{}",
                    lob.owner, lob.segment_name
                ),
            )?;
            let index_inventory = required(
                inventory.get(&(
                    lob.owner.clone(),
                    "INDEX".to_owned(),
                    lob.index_name.clone(),
                )),
                format!(
                    "inventory row for Oracle LOB index {}.{}",
                    lob.owner, lob.index_name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &lob.owner,
                ObjectKind::Extension,
                &lob.table,
                Some(format!("column:{}:lob:{}", lob.column, lob.segment_name)),
            );
            lob_keys.insert(
                (lob.owner.clone(), lob.table.clone(), lob.column.clone()),
                key.clone(),
            );
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(parent_key.clone()),
                name: lob.segment_name.clone(),
                extension_kind: Some("oracle_lob_storage".to_owned()),
                definition: None,
                properties: oracle_lob_properties(lob, segment_inventory, index_inventory),
            });
        }

        let lobs_by_identity = raw
            .lobs
            .iter()
            .map(|lob| {
                (
                    (lob.owner.clone(), lob.table.clone(), lob.column.clone()),
                    lob,
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut lob_partition_keys = BTreeMap::new();
        for partition in &raw.lob_partitions {
            let lob_identity = (
                partition.owner.clone(),
                partition.table.clone(),
                partition.column.clone(),
            );
            let lob = required(
                lobs_by_identity.get(&lob_identity),
                format!(
                    "parent LOB for Oracle partition {}.{}.{}",
                    partition.owner, partition.table, partition.name
                ),
            )?;
            let parent_key = required(
                lob_keys.get(&lob_identity),
                format!(
                    "parent LOB key for Oracle partition {}.{}.{}",
                    partition.owner, partition.table, partition.name
                ),
            )?;
            let table_partition_key = required(
                table_partition_keys.get(&(
                    partition.owner.clone(),
                    partition.table.clone(),
                    partition.table_partition.clone(),
                )),
                format!(
                    "table partition key for Oracle LOB partition {}.{}.{}",
                    partition.owner, partition.table, partition.name
                ),
            )?;
            let segment_inventory = required(
                subobject_inventory.get(&(
                    partition.owner.clone(),
                    "LOB PARTITION".to_owned(),
                    partition.lob_name.clone(),
                    partition.name.clone(),
                )),
                format!(
                    "inventory row for Oracle LOB partition {}.{}.{}",
                    partition.owner, partition.table, partition.name
                ),
            )?;
            let index_inventory = required(
                subobject_inventory.get(&(
                    partition.owner.clone(),
                    "INDEX PARTITION".to_owned(),
                    lob.index_name.clone(),
                    partition.index_partition_name.clone(),
                )),
                format!(
                    "inventory row for Oracle LOB index partition {}.{}",
                    partition.owner, partition.index_partition_name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &partition.owner,
                ObjectKind::Extension,
                &partition.table,
                Some(format!(
                    "column:{}:lob:{}:partition:{}",
                    partition.column, partition.lob_name, partition.name
                )),
            );
            lob_partition_keys.insert(
                (
                    partition.owner.clone(),
                    partition.lob_name.clone(),
                    partition.name.clone(),
                ),
                key.clone(),
            );
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(parent_key.clone()),
                name: partition.name.clone(),
                extension_kind: Some("oracle_lob_partition".to_owned()),
                definition: None,
                properties: oracle_lob_partition_properties(
                    partition,
                    segment_inventory,
                    index_inventory,
                ),
            });
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::Extension(
                    "oracle_lob_partition_storage".to_owned(),
                ),
                from_key: key,
                to_key: table_partition_key.clone(),
                ordinal: Some(positive_u32(
                    partition.position,
                    "Oracle LOB partition relationship ordinal",
                )?),
                properties: BTreeMap::new(),
            });
        }

        for subpartition in &raw.lob_subpartitions {
            let lob = required(
                lobs_by_identity.get(&(
                    subpartition.owner.clone(),
                    subpartition.table.clone(),
                    subpartition.column.clone(),
                )),
                format!(
                    "parent LOB for Oracle subpartition {}.{}.{}",
                    subpartition.owner, subpartition.table, subpartition.name
                ),
            )?;
            let parent_key = required(
                lob_partition_keys.get(&(
                    subpartition.owner.clone(),
                    subpartition.lob_name.clone(),
                    subpartition.lob_partition_name.clone(),
                )),
                format!(
                    "parent LOB partition key for Oracle subpartition {}.{}.{}",
                    subpartition.owner, subpartition.table, subpartition.name
                ),
            )?;
            let table_subpartition_key = required(
                table_subpartition_keys.get(&(
                    subpartition.owner.clone(),
                    subpartition.table.clone(),
                    subpartition.table_subpartition.clone(),
                )),
                format!(
                    "table subpartition key for Oracle LOB subpartition {}.{}.{}",
                    subpartition.owner, subpartition.table, subpartition.name
                ),
            )?;
            let segment_inventory = required(
                subobject_inventory.get(&(
                    subpartition.owner.clone(),
                    "LOB SUBPARTITION".to_owned(),
                    subpartition.lob_name.clone(),
                    subpartition.name.clone(),
                )),
                format!(
                    "inventory row for Oracle LOB subpartition {}.{}.{}",
                    subpartition.owner, subpartition.table, subpartition.name
                ),
            )?;
            let index_inventory = required(
                subobject_inventory.get(&(
                    subpartition.owner.clone(),
                    "INDEX SUBPARTITION".to_owned(),
                    lob.index_name.clone(),
                    subpartition.index_subpartition_name.clone(),
                )),
                format!(
                    "inventory row for Oracle LOB index subpartition {}.{}",
                    subpartition.owner, subpartition.index_subpartition_name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &subpartition.owner,
                ObjectKind::Extension,
                &subpartition.table,
                Some(format!(
                    "column:{}:lob:{}:partition:{}:subpartition:{}",
                    subpartition.column,
                    subpartition.lob_name,
                    subpartition.lob_partition_name,
                    subpartition.name
                )),
            );
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(parent_key.clone()),
                name: subpartition.name.clone(),
                extension_kind: Some("oracle_lob_subpartition".to_owned()),
                definition: None,
                properties: oracle_lob_subpartition_properties(
                    subpartition,
                    segment_inventory,
                    index_inventory,
                ),
            });
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::Extension(
                    "oracle_lob_subpartition_storage".to_owned(),
                ),
                from_key: key,
                to_key: table_subpartition_key.clone(),
                ordinal: Some(positive_u32(
                    subpartition.position,
                    "Oracle LOB subpartition relationship ordinal",
                )?),
                properties: BTreeMap::new(),
            });
        }


        let mut triggers = Vec::new();
        let mut trigger_keys = BTreeMap::new();
        let mut trigger_targets = BTreeMap::new();
        for trigger in &raw.triggers {
            let inventory_object = required(
                inventory.get(&(
                    trigger.owner.clone(),
                    "TRIGGER".to_owned(),
                    trigger.name.clone(),
                )),
                format!(
                    "inventory row for Oracle trigger {}.{}",
                    trigger.owner, trigger.name
                ),
            )?;
            let definition = oracle_trigger_definition(trigger)?;
            let properties = oracle_trigger_properties(trigger, inventory_object);
            match trigger.base_object_type.as_str() {
                "TABLE" | "VIEW" => {
                    let target_owner = trigger.table_owner.as_deref().ok_or_else(|| {
                        CatalogError::Mapping(format!(
                            "Oracle trigger {}.{} has no target owner",
                            trigger.owner, trigger.name
                        ))
                    })?;
                    let target_name = trigger.table_name.as_deref().ok_or_else(|| {
                        CatalogError::Mapping(format!(
                            "Oracle trigger {}.{} has no target object",
                            trigger.owner, trigger.name
                        ))
                    })?;
                    let target_key = if trigger.base_object_type == "TABLE" {
                        required(
                            table_keys.get(&(target_owner.to_owned(), target_name.to_owned())),
                            format!(
                                "target table key for Oracle trigger {}.{}",
                                trigger.owner, trigger.name
                            ),
                        )?
                    } else {
                        required(
                            view_keys.get(&(target_owner.to_owned(), target_name.to_owned())),
                            format!(
                                "target view key for Oracle trigger {}.{}",
                                trigger.owner, trigger.name
                            ),
                        )?
                    };
                    let key = oracle_key(
                        self.connection_alias,
                        &database_name,
                        target_owner,
                        ObjectKind::Trigger,
                        target_name,
                        Some(trigger.name.clone()),
                    );
                    trigger_keys.insert((trigger.owner.clone(), trigger.name.clone()), key.clone());
                    trigger_targets.insert(
                        (trigger.owner.clone(), trigger.name.clone()),
                        (
                            target_owner.to_owned(),
                            target_name.to_owned(),
                            trigger.base_object_type.clone(),
                        ),
                    );
                    triggers.push(TriggerObject {
                        key: key.clone(),
                        table_key: target_key.clone(),
                        name: trigger.name.clone(),
                        timing: Some(oracle_trigger_timing(&trigger.trigger_type)?),
                        events: oracle_trigger_events(&trigger.triggering_event)?,
                        definition: Some(definition),
                        executes_routine_key: None,
                    });
                    metadata.annotations.push(ObjectAnnotation {
                        object_key: key,
                        definition: None,
                        properties,
                    });
                }
                "SCHEMA" | "DATABASE" => {
                    let (parent_key, target_name) = if trigger.base_object_type == "SCHEMA" {
                        (
                            required(
                                schema_keys.get(&trigger.owner),
                                format!(
                                    "schema key for Oracle trigger {}.{}",
                                    trigger.owner, trigger.name
                                ),
                            )?,
                            trigger.owner.as_str(),
                        )
                    } else {
                        (&database_key, database_name.as_str())
                    };
                    let key = oracle_key(
                        self.connection_alias,
                        &database_name,
                        &trigger.owner,
                        ObjectKind::Trigger,
                        target_name,
                        Some(trigger.name.clone()),
                    );
                    trigger_keys.insert((trigger.owner.clone(), trigger.name.clone()), key.clone());
                    metadata.objects.push(MetadataObject {
                        key,
                        parent_key: Some(parent_key.clone()),
                        name: trigger.name.clone(),
                        extension_kind: None,
                        definition: Some(definition),
                        properties,
                    });
                }
                other => {
                    return Err(CatalogError::Mapping(format!(
                        "unmapped Oracle trigger target kind '{other}'"
                    )));
                }
            }
        }
        for dependency in raw
            .dependencies
            .iter()
            .filter(|dependency| dependency.object_type == "TRIGGER")
            .filter(|dependency| !dependency.referenced_owner_oracle_maintained)
        {
            if let Some(target) =
                trigger_targets.get(&(dependency.owner.clone(), dependency.name.clone()))
            {
                if dependency.referenced_owner == target.0
                    && dependency.referenced_name == target.1
                    && dependency.referenced_type == target.2
                {
                    continue;
                }
            }
            let source_key = required(
                trigger_keys.get(&(dependency.owner.clone(), dependency.name.clone())),
                format!(
                    "source key for Oracle trigger dependency {}.{}",
                    dependency.owner, dependency.name
                ),
            )?;
            let (target_key, relationship_kind) = match dependency.referenced_type.as_str() {
                "TABLE" => (
                    match materialized_view_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )) {
                        Some(key) => key,
                        None => required(
                            table_keys.get(&(
                                dependency.referenced_owner.clone(),
                                dependency.referenced_name.clone(),
                            )),
                            format!(
                                "table target for Oracle trigger dependency {}.{}",
                                dependency.referenced_owner, dependency.referenced_name
                            ),
                        )?,
                    },
                    MetadataRelationshipKind::DependsOn,
                ),
                "VIEW" => (
                    required(
                        view_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                        )),
                        format!(
                            "view target for Oracle trigger dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                    MetadataRelationshipKind::DependsOn,
                ),
                "MATERIALIZED VIEW" => (
                    required(
                        materialized_view_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                        )),
                        format!(
                            "materialized-view target for Oracle trigger dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                    MetadataRelationshipKind::DependsOn,
                ),
                "SEQUENCE" => (
                    required(
                        sequence_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                        )),
                        format!(
                            "sequence target for Oracle trigger dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                    MetadataRelationshipKind::DependsOn,
                ),
                "FUNCTION" | "PROCEDURE" => (
                    required(
                        routine_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                            dependency.referenced_type.clone(),
                        )),
                        format!(
                            "routine target for Oracle trigger dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                    MetadataRelationshipKind::Invokes,
                ),
                "PACKAGE" => (
                    required(
                        package_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                        )),
                        format!(
                            "package target for Oracle trigger dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                    MetadataRelationshipKind::DependsOn,
                ),
                "TYPE" => (
                    required(
                        type_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                        )),
                        format!(
                            "type target for Oracle trigger dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                    MetadataRelationshipKind::DependsOn,
                ),
                other => {
                    return Err(CatalogError::Mapping(format!(
                        "unmapped Oracle trigger dependency target type '{other}'"
                    )));
                }
            };
            metadata.relationships.push(MetadataRelationship {
                kind: relationship_kind,
                from_key: source_key.clone(),
                to_key: target_key.clone(),
                ordinal: None,
                properties: BTreeMap::from([(
                    "oracle_dependency_type".to_owned(),
                    MetadataValue::String(dependency.dependency_type.clone()),
                )]),
            });
        }

        let snapshot = CanonicalSchemaSnapshot {
            schema: SchemaSnapshot {
                source_kind: ORACLE_SOURCE.to_owned(),
                connection_alias: self.connection_alias.to_owned(),
                database,
                schemas,
                tables,
                columns,
                constraints,
                indexes,
                views,
                triggers,
                routines,
                capabilities: oracle_complete_capabilities(&self.scope),
            },
            metadata,
        };
        let discovered_counts = discovery_counts_from_catalog(&raw, &self.scope);
        let server_version = format!("{} ({})", self.facts.version, self.facts.release);
        Ok(CatalogDiscovery {
            snapshot,
            adapter: AdapterIdentity {
                name: "database-memory-oracle-catalog".to_owned(),
                version: ORACLE_ADAPTER_VERSION.to_owned(),
            },
            server: ServerIdentity {
                product: "Oracle Database".to_owned(),
                version: server_version,
            },
            scope: IntrospectionScope {
                catalogs: vec![database_name],
                schemas: self.scope.owners.clone(),
            },
            discovered_counts,
            capability_checks: vec![
                CapabilityCheck {
                    name: "supported_server_version".to_owned(),
                    evidence: format!(
                        "server release '{}' maps to live-certified strategy {}",
                        self.facts.release,
                        self.strategy.strategy_name()
                    ),
                },
                CapabilityCheck {
                    name: "single_container_scope".to_owned(),
                    evidence: format!(
                        "connected container={} con_id={} database={} and root aggregation was rejected",
                        self.facts.container, self.facts.container_id, self.facts.database
                    ),
                },
                CapabilityCheck {
                    name: "dictionary_scope".to_owned(),
                    evidence: format!(
                        "{} covered {} owner(s): {}",
                        self.scope.mode.label(),
                        self.scope.owners.len(),
                        self.scope.owners.join(", ")
                    ),
                },
                CapabilityCheck {
                    name: "stable_read_only_catalog".to_owned(),
                    evidence: "SET TRANSACTION READ ONLY succeeded and two complete dictionary reads were identical"
                        .to_owned(),
                },
                CapabilityCheck {
                    name: "independent_inventory_reconciliation".to_owned(),
                    evidence: format!(
                        "{} non-secondary USER/DBA_OBJECTS rows reconciled against table, index, partition, LOB storage, sequence, view, materialized-view, synonym, type, trigger, routine, and package detail catalogs",
                        raw.inventory.iter().filter(|object| !object.secondary).count()
                    ),
                },
                CapabilityCheck {
                    name: "metadata_only_catalog_queries".to_owned(),
                    evidence: "adapter queried Oracle data dictionary and session metadata only; no application table appears in a FROM clause"
                        .to_owned(),
                },
                CapabilityCheck {
                    name: "dependency_coverage".to_owned(),
                    evidence: format!(
                        "{} unique USER/DBA_DEPENDENCIES row(s) were resolved; {} Oracle-maintained target row(s) were explicitly collapsed",
                        raw.dependencies.len(),
                        raw.dependencies
                            .iter()
                            .filter(|dependency| dependency.referenced_owner_oracle_maintained)
                            .count()
                    ),
                },
                CapabilityCheck {
                    name: "principal_context".to_owned(),
                    evidence: format!(
                        "session_user={} current_schema={} and {} selected principal row(s) were readable",
                        self.facts.session_user,
                        self.facts.current_schema,
                        self.scope.principals.len()
                    ),
                },
            ],
        })

    }
}
