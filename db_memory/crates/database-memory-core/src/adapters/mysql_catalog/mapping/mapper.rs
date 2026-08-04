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
