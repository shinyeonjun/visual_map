impl<'a> PostgresSnapshotMapper<'a> {
    fn new(connection_alias: &'a str, source_kind: &'static str) -> Self {
        Self {
            connection_alias,
            source_kind,
        }
    }

    fn map(&self, raw: RawPostgresCatalog) -> Result<CatalogDiscovery, CatalogError> {
        validate_raw_catalog(&raw)?;
        let strategy = raw.strategy;
        if self.source_kind != strategy.source_kind() {
            return Err(CatalogError::Mapping(format!(
                "mapper source '{}' does not match selected strategy {}",
                self.source_kind,
                strategy.strategy_name()
            )));
        }

        let database_name = raw.server.database.clone();
        let database_key = pg_key(
            self.source_kind,
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

        let schemas = raw
            .schemas
            .iter()
            .map(|schema| SchemaObject {
                key: pg_key(
                    self.source_kind,
                    self.connection_alias,
                    &database_name,
                    &schema.name,
                    ObjectKind::Schema,
                    &schema.name,
                    None,
                ),
                database_key: database_key.clone(),
                name: schema.name.clone(),
            })
            .collect::<Vec<_>>();
        let schema_keys = schemas
            .iter()
            .map(|schema| (schema.name.clone(), schema.key.clone()))
            .collect::<BTreeMap<_, _>>();

        let mut metadata = CanonicalMetadata::default();
        let mut principal_keys = BTreeMap::new();
        for principal in &raw.principals {
            let key = pg_key(
                self.source_kind,
                self.connection_alias,
                &database_name,
                &database_name,
                ObjectKind::Principal,
                &principal.name,
                None,
            );
            principal_keys.insert(principal.oid, key.clone());
            let mut properties = BTreeMap::new();
            insert_bool(&mut properties, "superuser", principal.superuser);
            insert_bool(&mut properties, "inherit", principal.inherit);
            insert_bool(&mut properties, "create_role", principal.create_role);
            insert_bool(
                &mut properties,
                "create_database",
                principal.create_database,
            );
            insert_bool(&mut properties, "can_login", principal.can_login);
            insert_bool(&mut properties, "replication", principal.replication);
            insert_bool(&mut properties, "bypass_rls", principal.bypass_rls);
            insert_optional_string(
                &mut properties,
                "valid_until",
                principal.valid_until.as_deref(),
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

        for schema in &raw.schemas {
            let schema_key = required(
                schema_keys.get(&schema.name),
                format!("schema key for {}", schema.name),
            )?;
            let mut properties = BTreeMap::new();
            insert_i64(&mut properties, "postgres_oid", schema.oid);
            insert_optional_string(&mut properties, "comment", schema.comment.as_deref());
            metadata.annotations.push(ObjectAnnotation {
                object_key: schema_key.clone(),
                definition: None,
                properties,
            });
            add_owned_by(
                &mut metadata.relationships,
                schema_key,
                schema.owner_oid,
                &principal_keys,
                "schema",
            )?;
        }

        let mut type_keys = BTreeMap::new();
        for raw_type in &raw.types {
            let parent = required(
                schema_keys.get(&raw_type.schema),
                format!("schema key for pg_catalog type {}", raw_type.name),
            )?;
            let kind = if raw_type.kind == 'd' {
                ObjectKind::Domain
            } else {
                ObjectKind::UserDefinedType
            };
            let key = pg_key(
                self.source_kind,
                self.connection_alias,
                &database_name,
                &raw_type.schema,
                kind,
                &raw_type.name,
                None,
            );
            if type_keys.insert(raw_type.oid, key.clone()).is_some() {
                return Err(CatalogError::Mapping(format!(
                    "duplicate pg_catalog type oid {}",
                    raw_type.oid
                )));
            }
            let mut properties = BTreeMap::new();
            insert_i64(&mut properties, "postgres_oid", raw_type.oid);
            insert_string(
                &mut properties,
                "postgres_type_kind",
                type_kind_name(raw_type.kind),
            );
            insert_string(&mut properties, "category", raw_type.category.to_string());
            insert_bool(&mut properties, "not_null", raw_type.not_null);
            insert_optional_string(
                &mut properties,
                "default",
                raw_type.default_value.as_deref(),
            );
            insert_optional_string(&mut properties, "collation", raw_type.collation.as_deref());
            insert_optional_string(&mut properties, "comment", raw_type.comment.as_deref());
            insert_bool(
                &mut properties,
                "implicit_relation_type",
                raw_type.relation_oid.is_some(),
            );
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(parent.clone()),
                name: raw_type.name.clone(),
                extension_kind: None,
                definition: None,
                properties,
            });
            add_owned_by(
                &mut metadata.relationships,
                &key,
                raw_type.owner_oid,
                &principal_keys,
                "type",
            )?;
        }

        for enum_value in &raw.enum_values {
            let type_key = required(
                type_keys.get(&enum_value.type_oid),
                format!("enum parent type oid {}", enum_value.type_oid),
            )?;
            let key = pg_key(
                self.source_kind,
                self.connection_alias,
                &database_name,
                &type_key.schema,
                ObjectKind::EnumValue,
                &type_key.object_name,
                Some(enum_value.label.clone()),
            );
            let mut properties = BTreeMap::new();
            insert_string(&mut properties, "sort_order", &enum_value.sort_order);
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(type_key.clone()),
                name: enum_value.label.clone(),
                extension_kind: None,
                definition: None,
                properties,
            });
        }

        let sequence_facts = raw
            .sequences
            .iter()
            .map(|sequence| (sequence.relation_oid, sequence))
            .collect::<BTreeMap<_, _>>();
        let mut tables = Vec::new();
        let mut views = Vec::new();
        let mut table_keys = BTreeMap::new();
        let mut view_keys = BTreeMap::new();
        let mut materialized_view_keys = BTreeMap::new();
        let mut sequence_keys = BTreeMap::new();
        let mut relation_keys = BTreeMap::new();
        let mut relation_row_type_keys = BTreeMap::new();
        for relation in &raw.relations {
            if let Some(type_key) = type_keys.get(&relation.row_type_oid) {
                relation_row_type_keys.insert(relation.row_type_oid, type_key.clone());
            }
            let schema_key = required(
                schema_keys.get(&relation.schema),
                format!(
                    "schema key for relation {}.{}",
                    relation.schema, relation.name
                ),
            )?;
            match relation.relkind {
                'r' | 'p' | 'f' => {
                    let key = pg_key(
                        self.source_kind,
                        self.connection_alias,
                        &database_name,
                        &relation.schema,
                        ObjectKind::Table,
                        &relation.name,
                        None,
                    );
                    table_keys.insert(relation.oid, key.clone());
                    relation_keys.insert(relation.oid, key.clone());
                    tables.push(TableObject {
                        key: key.clone(),
                        schema_key: schema_key.clone(),
                        name: relation.name.clone(),
                        kind: table_kind(relation),
                    });
                    metadata
                        .annotations
                        .push(relation_annotation(relation, &key));
                    add_owned_by(
                        &mut metadata.relationships,
                        &key,
                        relation.owner_oid,
                        &principal_keys,
                        "table",
                    )?;
                }
                'v' => {
                    let key = pg_key(
                        self.source_kind,
                        self.connection_alias,
                        &database_name,
                        &relation.schema,
                        ObjectKind::View,
                        &relation.name,
                        None,
                    );
                    view_keys.insert(relation.oid, key.clone());
                    relation_keys.insert(relation.oid, key.clone());
                    views.push(ViewObject {
                        key: key.clone(),
                        schema_key: schema_key.clone(),
                        name: relation.name.clone(),
                        definition: relation.definition.clone(),
                        depends_on: Vec::new(),
                    });
                    metadata
                        .annotations
                        .push(relation_annotation(relation, &key));
                    add_owned_by(
                        &mut metadata.relationships,
                        &key,
                        relation.owner_oid,
                        &principal_keys,
                        "view",
                    )?;
                }
                'm' => {
                    let key = pg_key(
                        self.source_kind,
                        self.connection_alias,
                        &database_name,
                        &relation.schema,
                        ObjectKind::MaterializedView,
                        &relation.name,
                        None,
                    );
                    materialized_view_keys.insert(relation.oid, key.clone());
                    relation_keys.insert(relation.oid, key.clone());
                    metadata.objects.push(MetadataObject {
                        key: key.clone(),
                        parent_key: Some(schema_key.clone()),
                        name: relation.name.clone(),
                        extension_kind: None,
                        definition: relation.definition.clone(),
                        properties: relation_properties(relation),
                    });
                    add_owned_by(
                        &mut metadata.relationships,
                        &key,
                        relation.owner_oid,
                        &principal_keys,
                        "materialized view",
                    )?;
                }
                'S' => {
                    let key = pg_key(
                        self.source_kind,
                        self.connection_alias,
                        &database_name,
                        &relation.schema,
                        ObjectKind::Sequence,
                        &relation.name,
                        None,
                    );
                    sequence_keys.insert(relation.oid, key.clone());
                    relation_keys.insert(relation.oid, key.clone());
                    let sequence = required(
                        sequence_facts.get(&relation.oid).copied(),
                        format!("pg_sequence row for {}.{}", relation.schema, relation.name),
                    )?;
                    let mut properties = relation_properties(relation);
                    insert_i64(&mut properties, "type_oid", sequence.type_oid);
                    insert_i64(&mut properties, "start", sequence.start_value);
                    insert_i64(&mut properties, "minimum", sequence.min_value);
                    insert_i64(&mut properties, "maximum", sequence.max_value);
                    insert_i64(&mut properties, "increment", sequence.increment_by);
                    insert_bool(&mut properties, "cycle", sequence.cycle);
                    insert_i64(&mut properties, "cache", sequence.cache_size);
                    metadata.objects.push(MetadataObject {
                        key: key.clone(),
                        parent_key: Some(schema_key.clone()),
                        name: relation.name.clone(),
                        extension_kind: None,
                        definition: None,
                        properties,
                    });
                    add_owned_by(
                        &mut metadata.relationships,
                        &key,
                        relation.owner_oid,
                        &principal_keys,
                        "sequence",
                    )?;
                }
                'c' => {}
                other => {
                    return Err(CatalogError::Mapping(format!(
                        "unmapped pg_catalog relation kind '{other}' for {}.{}",
                        relation.schema, relation.name
                    )));
                }
            }
        }
        let mut physical_relation_keys = relation_keys.clone();

        let mut columns = Vec::new();
        let mut column_keys = BTreeMap::new();
        for column in &raw.columns {
            let parent_key = relation_keys.get(&column.relation_oid);
            match column.relation_kind {
                'r' | 'p' | 'f' => {
                    let table_key = required(
                        parent_key,
                        format!(
                            "table key for column {}.{}.{}",
                            column.schema, column.relation, column.name
                        ),
                    )?;
                    let key = pg_key(
                        self.source_kind,
                        self.connection_alias,
                        &database_name,
                        &column.schema,
                        ObjectKind::Column,
                        &column.relation,
                        Some(column.name.clone()),
                    );
                    column_keys.insert((column.relation_oid, column.attnum as i32), key.clone());
                    columns.push(ColumnObject {
                        key: key.clone(),
                        table_key: table_key.clone(),
                        name: column.name.clone(),
                        ordinal_position: positive_u32(column.attnum, "column ordinal")?,
                        data_type: column.data_type.clone(),
                        is_nullable: column.nullable,
                        default_value: column.default_expression.clone(),
                        is_generated: column.generated != '\0',
                    });
                    metadata.annotations.push(column_annotation(column, &key));
                    add_type_use(
                        &mut metadata.relationships,
                        &key,
                        column.type_oid,
                        &column.type_schema,
                        &type_keys,
                    )?;
                }
                'v' | 'm' => {
                    let view_key = required(
                        parent_key,
                        format!(
                            "view key for output column {}.{}.{}",
                            column.schema, column.relation, column.name
                        ),
                    )?;
                    let key = pg_key(
                        self.source_kind,
                        self.connection_alias,
                        &database_name,
                        &column.schema,
                        ObjectKind::ViewColumn,
                        &column.relation,
                        Some(column.name.clone()),
                    );
                    column_keys.insert((column.relation_oid, column.attnum as i32), key.clone());
                    let mut properties = column_properties(column);
                    insert_u64(
                        &mut properties,
                        "ordinal_position",
                        positive_u32(column.attnum, "view column ordinal")? as u64,
                    );
                    insert_string(&mut properties, "data_type", &column.data_type);
                    insert_bool(&mut properties, "nullable", column.nullable);
                    metadata.objects.push(MetadataObject {
                        key: key.clone(),
                        parent_key: Some(view_key.clone()),
                        name: column.name.clone(),
                        extension_kind: None,
                        definition: None,
                        properties,
                    });
                    add_type_use(
                        &mut metadata.relationships,
                        &key,
                        column.type_oid,
                        &column.type_schema,
                        &type_keys,
                    )?;
                }
                'c' => {
                    let parent_type = required(
                        type_keys.get(&relation_row_type_oid(&raw.relations, column.relation_oid)?),
                        format!(
                            "composite type for attribute {}.{}.{}",
                            column.schema, column.relation, column.name
                        ),
                    )?;
                    let key = pg_key(
                        self.source_kind,
                        self.connection_alias,
                        &database_name,
                        &column.schema,
                        ObjectKind::Extension,
                        &column.relation,
                        Some(column.name.clone()),
                    );
                    column_keys.insert((column.relation_oid, column.attnum as i32), key.clone());
                    let mut properties = column_properties(column);
                    insert_u64(
                        &mut properties,
                        "ordinal_position",
                        positive_u32(column.attnum, "composite attribute ordinal")? as u64,
                    );
                    insert_string(&mut properties, "data_type", &column.data_type);
                    metadata.objects.push(MetadataObject {
                        key: key.clone(),
                        parent_key: Some(parent_type.clone()),
                        name: column.name.clone(),
                        extension_kind: Some("postgres_composite_attribute".to_owned()),
                        definition: None,
                        properties,
                    });
                    if let Some(type_key) = type_keys.get(&column.type_oid) {
                        metadata.relationships.push(MetadataRelationship {
                            kind: MetadataRelationshipKind::DependsOn,
                            from_key: key,
                            to_key: type_key.clone(),
                            ordinal: None,
                            properties: BTreeMap::new(),
                        });
                    }
                }
                other => {
                    return Err(CatalogError::Mapping(format!(
                        "unmapped pg_catalog column relation kind '{other}' for {}.{}.{}",
                        column.schema, column.relation, column.name
                    )));
                }
            }
        }

        let mut constraints = Vec::new();
        for constraint in &raw.constraints {
            if let Some(domain_oid) = constraint.domain_type_oid {
                if constraint.kind != 'c' {
                    return Err(CatalogError::Mapping(format!(
                        "unsupported domain constraint kind '{}' for {}",
                        constraint.kind, constraint.name
                    )));
                }
                let domain_key = required(
                    type_keys.get(&domain_oid),
                    format!(
                        "domain parent oid {domain_oid} for constraint {}",
                        constraint.name
                    ),
                )?;
                let key = pg_key(
                    self.source_kind,
                    self.connection_alias,
                    &database_name,
                    &constraint.schema,
                    ObjectKind::CheckConstraint,
                    &domain_key.object_name,
                    Some(constraint.name.clone()),
                );
                metadata.objects.push(MetadataObject {
                    key,
                    parent_key: Some(domain_key.clone()),
                    name: constraint.name.clone(),
                    extension_kind: None,
                    definition: constraint.definition.clone(),
                    properties: constraint_properties(constraint),
                });
                continue;
            }

            let relation_oid = constraint.relation_oid.ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "constraint {} has neither relation nor domain parent",
                    constraint.name
                ))
            })?;
            let table_key = required(
                table_keys.get(&relation_oid),
                format!(
                    "table parent oid {relation_oid} for constraint {}",
                    constraint.name
                ),
            )?;
            if constraint.kind == 'x' {
                let key = pg_key(
                    self.source_kind,
                    self.connection_alias,
                    &database_name,
                    &constraint.schema,
                    ObjectKind::ExclusionConstraint,
                    &table_key.object_name,
                    Some(constraint.name.clone()),
                );
                metadata.objects.push(MetadataObject {
                    key: key.clone(),
                    parent_key: Some(table_key.clone()),
                    name: constraint.name.clone(),
                    extension_kind: None,
                    definition: constraint.definition.clone(),
                    properties: constraint_properties(constraint),
                });
                for (ordinal, column_number) in constraint.columns.iter().enumerate() {
                    if *column_number <= 0 {
                        continue;
                    }
                    let column_key = required(
                        column_keys.get(&(relation_oid, i32::from(*column_number))),
                        format!(
                            "exclusion constraint column {} at ordinal {}",
                            constraint.name,
                            ordinal + 1
                        ),
                    )?;
                    metadata.relationships.push(MetadataRelationship {
                        kind: MetadataRelationshipKind::ExcludesWith,
                        from_key: key.clone(),
                        to_key: column_key.clone(),
                        ordinal: Some((ordinal + 1) as u32),
                        properties: BTreeMap::new(),
                    });
                }
                continue;
            }

            let (kind, object_kind) = match constraint.kind {
                'p' => (ConstraintKind::PrimaryKey, ObjectKind::PrimaryKey),
                'u' => (ConstraintKind::Unique, ObjectKind::UniqueConstraint),
                'f' => (ConstraintKind::ForeignKey, ObjectKind::ForeignKey),
                'c' => (ConstraintKind::Check, ObjectKind::CheckConstraint),
                other => {
                    return Err(CatalogError::Mapping(format!(
                        "unmapped pg_catalog constraint kind '{other}' for {}",
                        constraint.name
                    )));
                }
            };
            let local_columns = resolve_columns(
                relation_oid,
                &constraint.columns,
                &column_keys,
                &constraint.name,
            )?;
            let (referenced_table_key, referenced_columns) = if kind == ConstraintKind::ForeignKey {
                let referenced_oid = constraint.referenced_relation_oid.ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "foreign key {} has no referenced relation",
                        constraint.name
                    ))
                })?;
                let referenced_table = required(
                    table_keys.get(&referenced_oid),
                    format!(
                        "foreign key {} references a table outside the certified schema scope (oid {referenced_oid})",
                        constraint.name
                    ),
                )?;
                let referenced = resolve_columns(
                    referenced_oid,
                    &constraint.referenced_columns,
                    &column_keys,
                    &constraint.name,
                )?;
                (Some(referenced_table.clone()), referenced)
            } else {
                (None, Vec::new())
            };
            let key = pg_key(
                self.source_kind,
                self.connection_alias,
                &database_name,
                &constraint.schema,
                object_kind,
                &table_key.object_name,
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
                    .then(|| constraint.definition.clone())
                    .flatten(),
            });
            metadata.annotations.push(ObjectAnnotation {
                object_key: key,
                definition: None,
                properties: constraint_properties(constraint),
            });
        }

        let terms_by_index = group_index_terms(&raw.index_terms);
        let relation_kind_by_oid = raw
            .relations
            .iter()
            .map(|relation| (relation.oid, relation.relkind))
            .collect::<BTreeMap<_, _>>();
        let mut indexes = Vec::new();
        for index in &raw.indexes {
            let terms = terms_by_index.get(&index.oid).cloned().unwrap_or_default();
            if terms.is_empty() {
                return Err(CatalogError::Mapping(format!(
                    "index {}.{}.{} has no catalog terms",
                    index.schema, index.relation, index.name
                )));
            }
            let relation_kind = required(
                relation_kind_by_oid.get(&index.relation_oid),
                format!("indexed relation oid {}", index.relation_oid),
            )?;
            match *relation_kind {
                'r' | 'p' | 'f' => {
                    let table_key = required(
                        table_keys.get(&index.relation_oid),
                        format!("indexed table oid {}", index.relation_oid),
                    )?;
                    let key = pg_key(
                        self.source_kind,
                        self.connection_alias,
                        &database_name,
                        &index.schema,
                        ObjectKind::Index,
                        &index.relation,
                        Some(index.name.clone()),
                    );
                    if physical_relation_keys
                        .insert(index.oid, key.clone())
                        .is_some()
                    {
                        return Err(CatalogError::Mapping(format!(
                            "duplicate physical relation oid {} for index {}",
                            index.oid, index.name
                        )));
                    }
                    let mut key_columns = Vec::new();
                    for term in &terms {
                        if term.is_key && term.column_number > 0 {
                            let column_key = required(
                                column_keys
                                    .get(&(index.relation_oid, i32::from(term.column_number))),
                                format!(
                                    "index key column for {} ordinal {}",
                                    index.name, term.ordinal
                                ),
                            )?
                            .clone();
                            if !key_columns.contains(&column_key) {
                                key_columns.push(column_key);
                            }
                        }
                    }
                    indexes.push(IndexObject {
                        key: key.clone(),
                        table_key: table_key.clone(),
                        name: index.name.clone(),
                        columns: key_columns,
                        is_unique: index.unique,
                        is_primary: index.primary,
                        predicate: index.predicate.clone(),
                        expression: index.expression.clone(),
                    });
                    metadata.annotations.push(ObjectAnnotation {
                        object_key: key.clone(),
                        definition: index.definition.clone(),
                        properties: index_properties(index, &terms),
                    });
                    add_included_columns(
                        &mut metadata.relationships,
                        &key,
                        index,
                        &terms,
                        &column_keys,
                        false,
                    )?;
                }
                'm' => {
                    let parent = required(
                        materialized_view_keys.get(&index.relation_oid),
                        format!("materialized view parent for index {}", index.name),
                    )?;
                    let key = pg_key(
                        self.source_kind,
                        self.connection_alias,
                        &database_name,
                        &index.schema,
                        ObjectKind::Index,
                        &index.relation,
                        Some(index.name.clone()),
                    );
                    if physical_relation_keys
                        .insert(index.oid, key.clone())
                        .is_some()
                    {
                        return Err(CatalogError::Mapping(format!(
                            "duplicate physical relation oid {} for index {}",
                            index.oid, index.name
                        )));
                    }
                    metadata.objects.push(MetadataObject {
                        key: key.clone(),
                        parent_key: Some(parent.clone()),
                        name: index.name.clone(),
                        extension_kind: None,
                        definition: index.definition.clone(),
                        properties: index_properties(index, &terms),
                    });
                    add_included_columns(
                        &mut metadata.relationships,
                        &key,
                        index,
                        &terms,
                        &column_keys,
                        true,
                    )?;
                }
                other => {
                    return Err(CatalogError::Mapping(format!(
                        "index {} belongs to unsupported relation kind '{other}'",
                        index.name
                    )));
                }
            }
        }

        if let Some(yugabyte) = &raw.yugabyte {
            map_yugabyte_metadata(
                &mut metadata,
                yugabyte,
                self.source_kind,
                self.connection_alias,
                &database_name,
                &database_key,
                &principal_keys,
                &physical_relation_keys,
            )?;
        }

        let view_position_by_oid = views
            .iter()
            .enumerate()
            .map(|(position, view)| {
                let oid = view_keys
                    .iter()
                    .find_map(|(oid, key)| (key == &view.key).then_some(*oid))
                    .expect("view key was inserted with its oid");
                (oid, position)
            })
            .collect::<BTreeMap<_, _>>();
        let mut view_dependency_ordinals = BTreeMap::<i64, u32>::new();
        for dependency in &raw.view_dependencies {
            let Some(target_key) = resolve_relation_dependency(
                dependency.target_relation_oid,
                dependency.target_column_number,
                &dependency.target_schema,
                &relation_keys,
                &column_keys,
            )?
            else {
                continue;
            };
            let owner_key = view_keys
                .get(&dependency.view_oid)
                .or_else(|| materialized_view_keys.get(&dependency.view_oid))
                .ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "view dependency owner oid {} is not mapped",
                        dependency.view_oid
                    ))
                })?;
            if let Some(position) = view_position_by_oid.get(&dependency.view_oid) {
                if is_base_snapshot_kind(target_key.object_kind) {
                    views[*position].depends_on.push(target_key.clone());
                } else {
                    metadata.relationships.push(MetadataRelationship {
                        kind: MetadataRelationshipKind::DependsOn,
                        from_key: owner_key.clone(),
                        to_key: target_key.clone(),
                        ordinal: None,
                        properties: BTreeMap::new(),
                    });
                }
            } else if let Some(materialized_key) = materialized_view_keys.get(&dependency.view_oid)
            {
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::DependsOn,
                    from_key: materialized_key.clone(),
                    to_key: target_key.clone(),
                    ordinal: None,
                    properties: BTreeMap::new(),
                });
            } else {
                return Err(CatalogError::Mapping(format!(
                    "view dependency owner oid {} is not mapped",
                    dependency.view_oid
                )));
            }
            let ordinal = view_dependency_ordinals
                .entry(dependency.view_oid)
                .and_modify(|value| *value += 1)
                .or_insert(1);
            let mut properties = BTreeMap::new();
            insert_string(
                &mut properties,
                "postgres_dependency_type",
                dependency.dependency_type.to_string(),
            );
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::Extension("postgres_catalog_dependency".to_owned()),
                from_key: owner_key.clone(),
                to_key: target_key,
                ordinal: Some(*ordinal),
                properties,
            });
        }

        let mut routine_keys = BTreeMap::new();
        for routine in &raw.routines {
            let key = pg_key(
                self.source_kind,
                self.connection_alias,
                &database_name,
                &routine.schema,
                ObjectKind::Routine,
                &routine.name,
                Some(routine.identity_arguments.clone()),
            );
            if routine_keys.insert(routine.oid, key).is_some() {
                return Err(CatalogError::Mapping(format!(
                    "duplicate pg_catalog routine oid {}",
                    routine.oid
                )));
            }
        }
        let mut routines = Vec::new();
        let mut routine_position_by_oid = BTreeMap::new();
        for routine in &raw.routines {
            let schema_key = required(
                schema_keys.get(&routine.schema),
                format!("schema key for routine {}.{}", routine.schema, routine.name),
            )?;
            let key = required(
                routine_keys.get(&routine.oid),
                format!("routine key oid {}", routine.oid),
            )?;
            routine_position_by_oid.insert(routine.oid, routines.len());
            routines.push(RoutineObject {
                key: key.clone(),
                schema_key: schema_key.clone(),
                name: routine.name.clone(),
                kind: if routine.kind == 'p' {
                    RoutineKind::Procedure
                } else {
                    RoutineKind::Function
                },
                definition: routine.definition.clone(),
                depends_on: Vec::new(),
            });
            metadata.annotations.push(ObjectAnnotation {
                object_key: key.clone(),
                definition: None,
                properties: routine_properties(routine),
            });
            add_owned_by(
                &mut metadata.relationships,
                key,
                routine.owner_oid,
                &principal_keys,
                "routine",
            )?;
            if let Some(return_type_key) = type_keys.get(&routine.return_type_oid) {
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::ReturnsType,
                    from_key: key.clone(),
                    to_key: return_type_key.clone(),
                    ordinal: None,
                    properties: BTreeMap::new(),
                });
            } else if !is_system_schema(&routine.return_type_schema) {
                return Err(CatalogError::Mapping(format!(
                    "routine {}.{} returns type outside the certified schema scope (type oid {})",
                    routine.schema, routine.name, routine.return_type_oid
                )));
            }
        }

        for parameter in &raw.routine_parameters {
            let routine_key = required(
                routine_keys.get(&parameter.routine_oid),
                format!("parameter parent routine oid {}", parameter.routine_oid),
            )?;
            let ordinal = u32::try_from(parameter.ordinal).map_err(|_| {
                CatalogError::Mapping(format!(
                    "routine parameter ordinal {} is invalid",
                    parameter.ordinal
                ))
            })?;
            if ordinal == 0 {
                return Err(CatalogError::Mapping(
                    "routine parameter ordinal cannot be zero".to_owned(),
                ));
            }
            let display_name = parameter
                .name
                .clone()
                .unwrap_or_else(|| format!("argument_{ordinal}"));
            let identity = format!(
                "{}#{}:{}",
                routine_key.sub_object.as_deref().unwrap_or_default(),
                ordinal,
                display_name
            );
            let key = pg_key(
                self.source_kind,
                self.connection_alias,
                &database_name,
                &routine_key.schema,
                ObjectKind::RoutineParameter,
                &routine_key.object_name,
                Some(identity),
            );
            let mut properties = BTreeMap::new();
            insert_u64(&mut properties, "ordinal_position", ordinal as u64);
            insert_string(
                &mut properties,
                "mode",
                routine_parameter_mode(parameter.mode),
            );
            insert_string(&mut properties, "data_type", &parameter.data_type);
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
                from_key: routine_key.clone(),
                to_key: key.clone(),
                ordinal: Some(ordinal),
                properties: BTreeMap::new(),
            });
            add_type_use(
                &mut metadata.relationships,
                &key,
                parameter.type_oid,
                &parameter.type_schema,
                &type_keys,
            )?;
        }

        let mut routine_dependency_ordinals = BTreeMap::<i64, u32>::new();
        for dependency in &raw.routine_dependencies {
            let position = required(
                routine_position_by_oid.get(&dependency.owner_oid),
                format!("routine dependency owner oid {}", dependency.owner_oid),
            )?;
            let Some(target) = resolve_routine_dependency(
                dependency,
                &relation_keys,
                &column_keys,
                &routine_keys,
                &type_keys,
            )?
            else {
                continue;
            };
            routines[*position].depends_on.push(target.clone());
            let ordinal = routine_dependency_ordinals
                .entry(dependency.owner_oid)
                .and_modify(|value| *value += 1)
                .or_insert(1);
            let mut properties = BTreeMap::new();
            insert_string(
                &mut properties,
                "postgres_dependency_type",
                dependency.dependency_type.to_string(),
            );
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::Extension("postgres_catalog_dependency".to_owned()),
                from_key: routines[*position].key.clone(),
                to_key: target,
                ordinal: Some(*ordinal),
                properties,
            });
        }

        let mut triggers = Vec::new();
        for trigger in &raw.triggers {
            let relation_key = required(
                relation_keys.get(&trigger.relation_oid),
                format!("trigger target relation oid {}", trigger.relation_oid),
            )?;
            if !matches!(
                relation_key.object_kind,
                ObjectKind::Table | ObjectKind::View
            ) {
                return Err(CatalogError::Mapping(format!(
                    "trigger {} target kind {} is unsupported",
                    trigger.name, relation_key.object_kind
                )));
            }
            let routine_key = routine_keys.get(&trigger.routine_oid).cloned();
            if routine_key.is_none() && !raw.extension_routine_oids.contains(&trigger.routine_oid) {
                return Err(CatalogError::Mapping(format!(
                    "trigger {} invokes routine outside the certified schema scope (oid {})",
                    trigger.name, trigger.routine_oid
                )));
            }
            let key = pg_key(
                self.source_kind,
                self.connection_alias,
                &database_name,
                &relation_key.schema,
                ObjectKind::Trigger,
                &relation_key.object_name,
                Some(trigger.name.clone()),
            );
            triggers.push(TriggerObject {
                key: key.clone(),
                table_key: relation_key.clone(),
                name: trigger.name.clone(),
                timing: Some(trigger.timing.clone()),
                events: trigger.events.clone(),
                definition: trigger.definition.clone(),
                executes_routine_key: routine_key,
            });
            metadata.annotations.push(ObjectAnnotation {
                object_key: key,
                definition: None,
                properties: trigger_properties(trigger, &column_keys)?,
            });
        }

        for inheritance in &raw.inheritance {
            let child = required(
                table_keys.get(&inheritance.child_oid),
                format!("inheritance child table oid {}", inheritance.child_oid),
            )?;
            let parent = required(
                table_keys.get(&inheritance.parent_oid),
                format!(
                    "table {} inherits from a parent outside the certified schema scope (oid {})",
                    child.object_name, inheritance.parent_oid
                ),
            )?;
            let mut properties = BTreeMap::new();
            insert_i64(
                &mut properties,
                "sequence_number",
                i64::from(inheritance.sequence_number),
            );
            metadata.relationships.push(MetadataRelationship {
                kind: if inheritance.child_is_partition {
                    MetadataRelationshipKind::PartitionOf
                } else {
                    MetadataRelationshipKind::InheritsFrom
                },
                from_key: child.clone(),
                to_key: parent.clone(),
                ordinal: None,
                properties,
            });
        }

        for usage in &raw.sequence_usages {
            let column = required(
                column_keys.get(&(usage.column_relation_oid, usage.column_number)),
                format!(
                    "sequence usage source column {}:{}",
                    usage.column_relation_oid, usage.column_number
                ),
            )?;
            if column.object_kind != ObjectKind::Column {
                return Err(CatalogError::Mapping(format!(
                    "sequence usage source {} is not a table column",
                    column
                )));
            }
            let sequence = required(
                sequence_keys.get(&usage.sequence_oid),
                format!(
                    "column {} uses sequence outside the certified schema scope (oid {})",
                    column, usage.sequence_oid
                ),
            )?;
            let mut properties = BTreeMap::new();
            insert_string(
                &mut properties,
                "postgres_dependency_type",
                usage.dependency_type.to_string(),
            );
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::UsesSequence,
                from_key: column.clone(),
                to_key: sequence.clone(),
                ordinal: None,
                properties,
            });
        }

        add_type_relationships(
            &raw,
            &mut metadata.relationships,
            &type_keys,
            &relation_keys,
        )?;

        for policy in &raw.policies {
            let parent = required(
                relation_keys.get(&policy.relation_oid),
                format!("policy target relation oid {}", policy.relation_oid),
            )?;
            let key = pg_key(
                self.source_kind,
                self.connection_alias,
                &database_name,
                &parent.schema,
                ObjectKind::Policy,
                &parent.object_name,
                Some(policy.name.clone()),
            );
            let mut properties = BTreeMap::new();
            insert_i64(&mut properties, "postgres_oid", policy.oid);
            insert_string(&mut properties, "command", policy_command(policy.command));
            insert_bool(&mut properties, "permissive", policy.permissive);
            insert_optional_string(
                &mut properties,
                "using_expression",
                policy.using_expression.as_deref(),
            );
            insert_optional_string(
                &mut properties,
                "check_expression",
                policy.check_expression.as_deref(),
            );
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(parent.clone()),
                name: policy.name.clone(),
                extension_kind: None,
                definition: None,
                properties,
            });
            for role_oid in &policy.role_oids {
                if *role_oid == 0 {
                    continue;
                }
                let role = required(
                    principal_keys.get(role_oid),
                    format!("policy {} role oid {role_oid}", policy.name),
                )?;
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::Extension("postgres_policy_role".to_owned()),
                    from_key: key.clone(),
                    to_key: role.clone(),
                    ordinal: None,
                    properties: BTreeMap::new(),
                });
            }
        }

        for extension in &raw.extensions {
            let key = pg_key(
                self.source_kind,
                self.connection_alias,
                &database_name,
                &database_name,
                ObjectKind::Extension,
                &extension.name,
                None,
            );
            let mut properties = BTreeMap::new();
            insert_i64(&mut properties, "postgres_oid", extension.oid);
            insert_optional_string(&mut properties, "schema", extension.schema.as_deref());
            insert_bool(&mut properties, "relocatable", extension.relocatable);
            insert_string(&mut properties, "version", &extension.version);
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(database_key.clone()),
                name: extension.name.clone(),
                extension_kind: Some("postgres_extension".to_owned()),
                definition: None,
                properties,
            });
            add_owned_by(
                &mut metadata.relationships,
                &key,
                extension.owner_oid,
                &principal_keys,
                "extension",
            )?;
        }

        for event in &raw.event_triggers {
            let routine = routine_keys.get(&event.routine_oid).cloned();
            if routine.is_none()
                && !raw.extension_routine_oids.contains(&event.routine_oid)
                && !is_system_schema(&event.routine_schema)
            {
                return Err(CatalogError::Mapping(format!(
                    "event trigger {} invokes routine outside the certified schema scope ({}. oid {})",
                    event.name, event.routine_schema, event.routine_oid
                )));
            }
            let key = pg_key(
                self.source_kind,
                self.connection_alias,
                &database_name,
                &database_name,
                ObjectKind::Event,
                &event.name,
                None,
            );
            let mut properties = BTreeMap::new();
            insert_i64(&mut properties, "postgres_oid", event.oid);
            insert_string(&mut properties, "event", &event.event);
            insert_string(&mut properties, "enabled", event.enabled.to_string());
            properties.insert(
                "tags".to_owned(),
                MetadataValue::StringList(event.tags.clone()),
            );
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(database_key.clone()),
                name: event.name.clone(),
                extension_kind: None,
                definition: None,
                properties,
            });
            add_owned_by(
                &mut metadata.relationships,
                &key,
                event.owner_oid,
                &principal_keys,
                "event trigger",
            )?;
            if let Some(routine) = routine {
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::Invokes,
                    from_key: key,
                    to_key: routine,
                    ordinal: None,
                    properties: BTreeMap::new(),
                });
            }
        }

        deduplicate_metadata_relationships(&mut metadata.relationships)?;
        let snapshot = CanonicalSchemaSnapshot {
            schema: SchemaSnapshot {
                source_kind: self.source_kind.to_owned(),
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
                capabilities: pg_catalog_complete_capabilities(strategy, &raw),
            },
            metadata,
        };
        let discovered_counts = discovery_counts_from_catalog(&raw, &snapshot)?;
        let mut scope_schemas = raw
            .schemas
            .iter()
            .map(|schema| schema.name.clone())
            .collect::<Vec<_>>();
        scope_schemas.sort();
        let mut capability_checks = vec![
            CapabilityCheck {
                name: "supported_server_version".to_owned(),
                evidence: format!(
                    "server_version={} and server_version_num={} map to certified {} strategy {}",
                    raw.server.version,
                    raw.server.version_num,
                    strategy.product_name(),
                    strategy.strategy_name()
                ),
            },
            CapabilityCheck {
                name: "read_only_repeatable_read_transaction".to_owned(),
                evidence: format!(
                    "transaction_read_only={} and transaction_isolation={}",
                    raw.server.transaction_read_only, raw.server.transaction_isolation
                ),
            },
            CapabilityCheck {
                name: "schema_visibility".to_owned(),
                evidence: format!(
                    "has_schema_privilege(..., USAGE) succeeded for {} requested schema(s)",
                    scope_schemas.len()
                ),
            },
            CapabilityCheck {
                name: "metadata_only_catalog_queries".to_owned(),
                evidence: "adapter queried pg_catalog metadata and server information only; no application relation appears in a FROM clause"
                    .to_owned(),
            },
            CapabilityCheck {
                name: "routine_dependency_proof".to_owned(),
                evidence: format!(
                    "{} selected routine(s) have catalog-proven dependency bodies; {} opaque routine(s) remain boundary objects and emit no guessed edges",
                    raw.routines
                        .iter()
                        .filter(|routine| routine.body_catalog_tracked)
                        .count(),
                    raw.routines
                        .iter()
                        .filter(|routine| !routine.body_catalog_tracked)
                        .count()
                ),
            },
            CapabilityCheck {
                name: "principal_context".to_owned(),
                evidence: format!(
                    "current_user={} session_user={} and pg_roles inventory was readable",
                    raw.server.current_user, raw.server.session_user
                ),
            },
            CapabilityCheck {
                name: "transport_security".to_owned(),
                evidence: if raw.server.tls {
                    format!(
                        "TLS enabled version={} cipher={}",
                        raw.server.tls_version.as_deref().unwrap_or("reported"),
                        raw.server.tls_cipher.as_deref().unwrap_or("reported")
                    )
                } else {
                    "plaintext transport accepted only for a loopback/local connection".to_owned()
                },
            },
        ];
        if let Some(yugabyte) = &raw.yugabyte {
            capability_checks.push(CapabilityCheck {
                name: "yugabytedb_distributed_metadata".to_owned(),
                evidence: format!(
                    "yb_table_properties certified {} physical relation(s), including tablet count, hash-key count, colocation, tablegroup, and range split metadata",
                    yugabyte.relation_properties.len()
                ),
            });
            capability_checks.push(CapabilityCheck {
                name: "yugabytedb_placement_metadata".to_owned(),
                evidence: format!(
                    "pg_yb_tablegroup and pg_tablespace certified {} tablegroup(s), {} tablespace(s), database_colocated={}",
                    yugabyte.tablegroups.len(),
                    yugabyte.tablespaces.len(),
                    yugabyte.database_colocated
                ),
            });
        }

        Ok(CatalogDiscovery {
            snapshot,
            adapter: AdapterIdentity {
                name: strategy.adapter_name().to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
            },
            server: ServerIdentity {
                product: strategy.product_name().to_owned(),
                version: raw.server.version.clone(),
            },
            scope: IntrospectionScope {
                catalogs: vec![database_name],
                schemas: scope_schemas.clone(),
            },
            discovered_counts,
            capability_checks,
        })
    }
}
