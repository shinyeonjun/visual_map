impl RawSqlServerCatalog {
    async fn read(
        client: &mut TdsClient,
        strategy: SqlServerCatalogVersion,
        selected_schemas: &BTreeSet<String>,
    ) -> Result<Self, CatalogError> {
        let schemas = read_schemas(client)
            .await?
            .into_iter()
            .filter(|schema| selected_schemas.contains(&schema.name))
            .collect::<Vec<_>>();
        let principals = read_principals(client).await?;
        let tables = read_tables(client, strategy, selected_schemas).await?;
        let columns = read_columns(client, selected_schemas).await?;
        let constraints = read_constraints(client, selected_schemas).await?;
        let indexes = read_indexes(client, selected_schemas).await?;
        let views = read_views(client, selected_schemas).await?;
        let routines = read_routines(client, strategy, selected_schemas).await?;
        let parameters = read_parameters(client, &routines).await?;
        let triggers = read_triggers(client, selected_schemas).await?;
        let user_types = read_user_types(client, selected_schemas).await?;
        let sequences = read_sequences(client, selected_schemas).await?;
        let synonyms = read_synonyms(client, selected_schemas).await?;
        let dependencies = read_dependencies(client, selected_schemas).await?;
        let partition_functions = read_partition_functions(client).await?;
        let partition_schemes = read_partition_schemes(client).await?;
        let partitions = read_partitions(client, strategy, &tables, &views).await?;
        let security_policies = read_security_policies(client, selected_schemas).await?;
        let xml_schema_collections = read_xml_schema_collections(client, selected_schemas).await?;
        let extended_properties = select_extended_properties(
            read_extended_properties(client).await?,
            &schemas,
            &principals,
            &tables,
            &columns,
            &constraints,
            &indexes,
            &views,
            &routines,
            &parameters,
            &triggers,
            &user_types,
            &sequences,
            &synonyms,
            &partition_functions,
            &partition_schemes,
            &security_policies,
            &xml_schema_collections,
        );
        let unsupported = read_unsupported_objects(client, strategy, selected_schemas).await?;
        validate_supported_metadata(
            &unsupported,
            &views,
            &routines,
            &triggers,
            &user_types,
            &dependencies,
        )?;
        Ok(Self {
            schemas,
            principals,
            tables,
            columns,
            constraints,
            indexes,
            views,
            routines,
            parameters,
            triggers,
            user_types,
            sequences,
            synonyms,
            dependencies,
            partition_functions,
            partition_schemes,
            partitions,
            security_policies,
            xml_schema_collections,
            extended_properties,
        })
    }
}

async fn read_principals(client: &mut TdsClient) -> Result<Vec<RawPrincipal>, CatalogError> {
    rows(
        client,
        "
        SELECT principal_id,
               name,
               RTRIM(type),
               type_desc,
               default_schema_name,
               authentication_type_desc,
               is_fixed_role,
               owning_principal_id
        FROM sys.database_principals
        WHERE principal_id > 0
          AND name IS NOT NULL
        ORDER BY principal_id
        ",
    )
    .await?
    .into_iter()
    .map(|row| {
        Ok(RawPrincipal {
            id: required_value(&row, 0, "principal id")?,
            name: required_string(&row, 1, "principal name")?,
            type_code: required_string(&row, 2, "principal type")?,
            type_desc: required_string(&row, 3, "principal type description")?,
            default_schema: optional_string(&row, 4)?,
            authentication_type: required_string(&row, 5, "authentication type")?,
            fixed_role: required_value(&row, 6, "fixed role flag")?,
            owning_principal_id: optional_value(&row, 7)?,
        })
    })
    .collect()
}

async fn read_tables(
    client: &mut TdsClient,
    strategy: SqlServerCatalogVersion,
    selected_schemas: &BTreeSet<String>,
) -> Result<Vec<RawTable>, CatalogError> {
    let ledger_column = strategy.ledger_expression();
    let sql = format!(
        "
        SELECT t.object_id,
               s.name,
               t.name,
               t.principal_id,
               t.lob_data_space_id,
               NULLIF(t.filestream_data_space_id, 0),
               t.is_replicated,
               t.is_merge_published,
               t.is_sync_tran_subscribed,
               t.is_tracked_by_cdc,
               t.lock_on_bulk_load,
               t.is_filetable,
               t.is_memory_optimized,
               t.durability_desc,
               t.temporal_type_desc,
               hs.name,
               ht.name,
               t.is_remote_data_archive_enabled,
               CAST(0 AS bit),
               t.is_node,
               t.is_edge,
               {ledger_column}
        FROM sys.tables t
        JOIN sys.schemas s ON s.schema_id = t.schema_id
        LEFT JOIN sys.tables ht ON ht.object_id = t.history_table_id
        LEFT JOIN sys.schemas hs ON hs.schema_id = ht.schema_id
        WHERE t.is_ms_shipped = 0
        ORDER BY t.object_id
        "
    );
    rows(client, &sql)
        .await?
        .into_iter()
        .map(|row| {
            Ok(RawTable {
                id: required_value(&row, 0, "table id")?,
                schema: required_string(&row, 1, "table schema")?,
                name: required_string(&row, 2, "table name")?,
                principal_id: optional_value(&row, 3)?,
                lob_data_space_id: required_value(&row, 4, "LOB data space")?,
                filestream_data_space_id: optional_value(&row, 5)?,
                replicated: required_value(&row, 6, "replication flag")?,
                merge_published: required_value(&row, 7, "merge publication flag")?,
                sync_tran_subscribed: required_value(
                    &row,
                    8,
                    "sync transaction subscription flag",
                )?,
                cdc_tracked: required_value(&row, 9, "CDC flag")?,
                lock_on_bulk_load: required_value(&row, 10, "bulk-load lock flag")?,
                file_table: required_value(&row, 11, "FileTable flag")?,
                memory_optimized: required_value(&row, 12, "memory optimized flag")?,
                durability: required_string(&row, 13, "durability")?,
                temporal_type: required_string(&row, 14, "temporal type")?,
                history_schema: optional_string(&row, 15)?,
                history_table: optional_string(&row, 16)?,
                remote_data_archive: required_value(&row, 17, "remote archive flag")?,
                external: required_value(&row, 18, "external table flag")?,
                node: required_value(&row, 19, "graph node flag")?,
                edge: required_value(&row, 20, "graph edge flag")?,
                ledger_type: required_string(&row, 21, "ledger type")?,
            })
        })
        .collect::<Result<Vec<_>, CatalogError>>()
        .map(|tables| {
            tables
                .into_iter()
                .filter(|table| selected_schemas.contains(&table.schema))
                .collect()
        })
}

async fn read_columns(
    client: &mut TdsClient,
    selected_schemas: &BTreeSet<String>,
) -> Result<Vec<RawColumn>, CatalogError> {
    let sql = format!(
        "
        SELECT c.object_id,
               RTRIM(o.type),
               s.name,
               COALESCE(tt.name, o.name),
               c.column_id,
               c.name,
               c.user_type_id,
               ts.name,
               ty.name,
               c.max_length,
               c.precision,
               c.scale,
               c.collation_name,
               c.is_nullable,
               c.is_ansi_padded,
               c.is_rowguidcol,
               c.is_identity,
               CONVERT(nvarchar(128), ic.seed_value),
               CONVERT(nvarchar(128), ic.increment_value),
               c.is_computed,
               CASE WHEN DATALENGTH(cc.definition) <= {MAX_DEFINITION_BYTES} THEN cc.definition END,
               CAST(COALESCE(DATALENGTH(cc.definition), 0) AS int),
               cc.is_persisted,
               CASE WHEN DATALENGTH(dc.definition) <= {MAX_DEFINITION_BYTES} THEN dc.definition END,
               CAST(COALESCE(DATALENGTH(dc.definition), 0) AS int),
               c.is_filestream,
               c.is_replicated,
               c.is_non_sql_subscribed,
               c.is_merge_published,
               c.is_dts_replicated,
               c.is_xml_document,
               c.xml_collection_id,
               c.is_sparse,
               c.is_column_set,
               c.generated_always_type_desc,
               c.encryption_type_desc,
               c.is_hidden,
               COALESCE(mc.is_masked, CAST(0 AS bit)),
               mc.masking_function,
               c.graph_type_desc,
               c.default_object_id
        FROM sys.columns c
        JOIN sys.objects o ON o.object_id = c.object_id
        LEFT JOIN sys.table_types tt ON tt.type_table_object_id = o.object_id
        JOIN sys.schemas s ON s.schema_id = COALESCE(tt.schema_id, o.schema_id)
        JOIN sys.types ty ON ty.user_type_id = c.user_type_id
        JOIN sys.schemas ts ON ts.schema_id = ty.schema_id
        LEFT JOIN sys.identity_columns ic
          ON ic.object_id = c.object_id AND ic.column_id = c.column_id
        LEFT JOIN sys.computed_columns cc
          ON cc.object_id = c.object_id AND cc.column_id = c.column_id
        LEFT JOIN sys.default_constraints dc ON dc.object_id = c.default_object_id
        LEFT JOIN sys.masked_columns mc
          ON mc.object_id = c.object_id AND mc.column_id = c.column_id
        WHERE (o.is_ms_shipped = 0 OR o.type = 'TT')
          AND o.type IN ('U', 'V', 'TT')
        ORDER BY c.object_id, c.column_id
        "
    );
    rows(client, &sql)
        .await?
        .into_iter()
        .map(|row| {
            Ok(RawColumn {
                object_id: required_value(&row, 0, "column object id")?,
                object_type: required_string(&row, 1, "column object type")?,
                schema: required_string(&row, 2, "column schema")?,
                relation: required_string(&row, 3, "column relation")?,
                id: required_value(&row, 4, "column id")?,
                name: required_string(&row, 5, "column name")?,
                type_id: required_value(&row, 6, "column type id")?,
                type_schema: required_string(&row, 7, "column type schema")?,
                type_name: required_string(&row, 8, "column type name")?,
                max_length: required_value(&row, 9, "column max length")?,
                precision: required_value(&row, 10, "column precision")?,
                scale: required_value(&row, 11, "column scale")?,
                collation: optional_string(&row, 12)?,
                nullable: required_value(&row, 13, "column nullable flag")?,
                ansi_padded: required_value(&row, 14, "column ANSI padded flag")?,
                rowguid: required_value(&row, 15, "column rowguid flag")?,
                identity: required_value(&row, 16, "column identity flag")?,
                identity_seed: optional_string(&row, 17)?,
                identity_increment: optional_string(&row, 18)?,
                computed: required_value(&row, 19, "column computed flag")?,
                computed_definition: optional_string(&row, 20)?,
                computed_definition_bytes: required_value(&row, 21, "computed definition bytes")?,
                persisted: optional_value(&row, 22)?,
                default_definition: optional_string(&row, 23)?,
                default_definition_bytes: required_value(&row, 24, "default definition bytes")?,
                filestream: required_value(&row, 25, "column FILESTREAM flag")?,
                replicated: required_value(&row, 26, "column replicated flag")?,
                non_sql_subscribed: required_value(&row, 27, "column non-SQL subscriber flag")?,
                merge_published: required_value(&row, 28, "column merge publication flag")?,
                dts_replicated: required_value(&row, 29, "column DTS replication flag")?,
                xml_document: required_value(&row, 30, "XML document flag")?,
                xml_collection_id: required_value(&row, 31, "XML collection id")?,
                sparse: required_value(&row, 32, "sparse flag")?,
                column_set: required_value(&row, 33, "column set flag")?,
                generated_always: required_string(&row, 34, "generated always type")?,
                encryption_type: optional_string(&row, 35)?,
                hidden: required_value(&row, 36, "hidden column flag")?,
                masked: required_value(&row, 37, "masked column flag")?,
                masking_function: optional_string(&row, 38)?,
                graph_type: optional_string(&row, 39)?,
                default_object_id: required_value(&row, 40, "default object id")?,
            })
        })
        .collect::<Result<Vec<_>, CatalogError>>()
        .and_then(|columns| {
            for column in &columns {
                ensure_definition_size(
                    "computed column",
                    &format!("{}.{}.{}", column.schema, column.relation, column.name),
                    column.computed_definition_bytes,
                )?;
                ensure_definition_size(
                    "default",
                    &format!("{}.{}.{}", column.schema, column.relation, column.name),
                    column.default_definition_bytes,
                )?;
            }
            Ok(columns
                .into_iter()
                .filter(|column| selected_schemas.contains(&column.schema))
                .collect())
        })
}

async fn read_constraints(
    client: &mut TdsClient,
    selected_schemas: &BTreeSet<String>,
) -> Result<Vec<RawConstraint>, CatalogError> {
    let mut constraints = BTreeMap::<i32, RawConstraint>::new();
    let key_rows = rows(
        client,
        "
        SELECT kc.object_id,
               s.name,
               COALESCE(tt.name, t.name),
               t.object_id,
               kc.name,
               RTRIM(kc.type),
               ic.key_ordinal,
               c.column_id,
               c.name
        FROM sys.key_constraints kc
        JOIN sys.objects t ON t.object_id = kc.parent_object_id
        LEFT JOIN sys.table_types tt ON tt.type_table_object_id = t.object_id
        JOIN sys.schemas s ON s.schema_id = COALESCE(tt.schema_id, t.schema_id)
        JOIN sys.index_columns ic
          ON ic.object_id = kc.parent_object_id
         AND ic.index_id = kc.unique_index_id
         AND ic.key_ordinal > 0
        JOIN sys.columns c
          ON c.object_id = ic.object_id AND c.column_id = ic.column_id
        WHERE (t.is_ms_shipped = 0 OR t.type = 'TT')
          AND t.type IN ('U', 'TT')
          AND kc.type IN ('PK', 'UQ')
        ORDER BY kc.object_id, ic.key_ordinal
        ",
    )
    .await?;
    for row in key_rows {
        let id: i32 = required_value(&row, 0, "key constraint id")?;
        let schema = required_string(&row, 1, "key constraint schema")?;
        if !selected_schemas.contains(&schema) {
            continue;
        }
        let type_code = required_string(&row, 5, "key constraint type")?;
        let kind = if type_code == "PK" {
            ConstraintKind::PrimaryKey
        } else if type_code == "UQ" {
            ConstraintKind::Unique
        } else {
            return Err(CatalogError::Mapping(format!(
                "key constraint {id} has unsupported type '{type_code}'"
            )));
        };
        let table = required_string(&row, 2, "key constraint table")?;
        let table_id = required_value(&row, 3, "key constraint table id")?;
        let name = required_string(&row, 4, "key constraint name")?;
        let entry = constraints.entry(id).or_insert_with(|| RawConstraint {
            id,
            schema: schema.clone(),
            table: table.clone(),
            table_id,
            name: name.clone(),
            kind,
            columns: Vec::new(),
            referenced_schema: None,
            referenced_table: None,
            referenced_table_id: None,
            delete_action: None,
            update_action: None,
            disabled: false,
            not_trusted: false,
            not_for_replication: false,
            expression: None,
            expression_bytes: 0,
        });
        if entry.schema != schema
            || entry.table != table
            || entry.table_id != table_id
            || entry.name != name
            || entry.kind != kind
        {
            return Err(CatalogError::Mapping(format!(
                "key constraint {id} has inconsistent catalog rows"
            )));
        }
        entry.columns.push(RawConstraintColumn {
            ordinal: i32::from(required_value::<u8>(&row, 6, "constraint column ordinal")?),
            column_id: required_value(&row, 7, "constraint column id")?,
            name: required_string(&row, 8, "constraint column name")?,
            referenced_column_id: None,
            referenced_name: None,
        });
    }

    let fk_rows = rows(
        client,
        "
        SELECT fk.object_id,
               ps.name,
               pt.name,
               pt.object_id,
               fk.name,
               rs.name,
               rt.name,
               rt.object_id,
               fk.delete_referential_action_desc,
               fk.update_referential_action_desc,
               fk.is_disabled,
               fk.is_not_trusted,
               fk.is_not_for_replication,
               fkc.constraint_column_id,
               pc.column_id,
               pc.name,
               rc.column_id,
               rc.name
        FROM sys.foreign_keys fk
        JOIN sys.tables pt ON pt.object_id = fk.parent_object_id
        JOIN sys.schemas ps ON ps.schema_id = pt.schema_id
        JOIN sys.tables rt ON rt.object_id = fk.referenced_object_id
        JOIN sys.schemas rs ON rs.schema_id = rt.schema_id
        JOIN sys.foreign_key_columns fkc ON fkc.constraint_object_id = fk.object_id
        JOIN sys.columns pc
          ON pc.object_id = fkc.parent_object_id AND pc.column_id = fkc.parent_column_id
        JOIN sys.columns rc
          ON rc.object_id = fkc.referenced_object_id AND rc.column_id = fkc.referenced_column_id
        WHERE pt.is_ms_shipped = 0
        ORDER BY fk.object_id, fkc.constraint_column_id
        ",
    )
    .await?;
    for row in fk_rows {
        let id: i32 = required_value(&row, 0, "foreign key id")?;
        let schema = required_string(&row, 1, "foreign key schema")?;
        if !selected_schemas.contains(&schema) {
            continue;
        }
        let table = required_string(&row, 2, "foreign key table")?;
        let table_id = required_value(&row, 3, "foreign key table id")?;
        let name = required_string(&row, 4, "foreign key name")?;
        let referenced_schema = required_string(&row, 5, "referenced schema")?;
        let referenced_table = required_string(&row, 6, "referenced table")?;
        let referenced_table_id = required_value(&row, 7, "referenced table id")?;
        let delete_action = required_string(&row, 8, "delete action")?;
        let update_action = required_string(&row, 9, "update action")?;
        let disabled = required_value(&row, 10, "foreign key disabled flag")?;
        let not_trusted = required_value(&row, 11, "foreign key trust flag")?;
        let not_for_replication = required_value(&row, 12, "foreign key replication flag")?;
        let entry = constraints.entry(id).or_insert_with(|| RawConstraint {
            id,
            schema: schema.clone(),
            table: table.clone(),
            table_id,
            name: name.clone(),
            kind: ConstraintKind::ForeignKey,
            columns: Vec::new(),
            referenced_schema: Some(referenced_schema.clone()),
            referenced_table: Some(referenced_table.clone()),
            referenced_table_id: Some(referenced_table_id),
            delete_action: Some(delete_action.clone()),
            update_action: Some(update_action.clone()),
            disabled,
            not_trusted,
            not_for_replication,
            expression: None,
            expression_bytes: 0,
        });
        if entry.schema != schema
            || entry.table != table
            || entry.table_id != table_id
            || entry.name != name
            || entry.kind != ConstraintKind::ForeignKey
            || entry.referenced_schema.as_ref() != Some(&referenced_schema)
            || entry.referenced_table.as_ref() != Some(&referenced_table)
            || entry.referenced_table_id != Some(referenced_table_id)
            || entry.delete_action.as_ref() != Some(&delete_action)
            || entry.update_action.as_ref() != Some(&update_action)
            || entry.disabled != disabled
            || entry.not_trusted != not_trusted
            || entry.not_for_replication != not_for_replication
        {
            return Err(CatalogError::Mapping(format!(
                "foreign key {id} has inconsistent catalog rows"
            )));
        }
        entry.columns.push(RawConstraintColumn {
            ordinal: required_value(&row, 13, "foreign key column ordinal")?,
            column_id: required_value(&row, 14, "foreign key column id")?,
            name: required_string(&row, 15, "foreign key column name")?,
            referenced_column_id: Some(required_value(
                &row,
                16,
                "referenced foreign key column id",
            )?),
            referenced_name: Some(required_string(
                &row,
                17,
                "referenced foreign key column name",
            )?),
        });
    }

    let check_sql = format!(
        "
        SELECT cc.object_id,
               s.name,
               COALESCE(tt.name, t.name),
               t.object_id,
               cc.name,
               cc.parent_column_id,
               c.name,
               cc.is_disabled,
               cc.is_not_trusted,
               cc.is_not_for_replication,
               CASE WHEN DATALENGTH(cc.definition) <= {MAX_DEFINITION_BYTES} THEN cc.definition END,
               CAST(COALESCE(DATALENGTH(cc.definition), 0) AS int)
        FROM sys.check_constraints cc
        JOIN sys.objects t ON t.object_id = cc.parent_object_id
        LEFT JOIN sys.table_types tt ON tt.type_table_object_id = t.object_id
        JOIN sys.schemas s ON s.schema_id = COALESCE(tt.schema_id, t.schema_id)
        LEFT JOIN sys.columns c
          ON c.object_id = cc.parent_object_id AND c.column_id = cc.parent_column_id
        WHERE (t.is_ms_shipped = 0 OR t.type = 'TT')
          AND t.type IN ('U', 'TT')
        ORDER BY cc.object_id
        "
    );
    for row in rows(client, &check_sql).await? {
        let id: i32 = required_value(&row, 0, "check constraint id")?;
        let schema = required_string(&row, 1, "check constraint schema")?;
        if !selected_schemas.contains(&schema) {
            continue;
        }
        let parent_column_id: i32 = required_value(&row, 5, "check parent column id")?;
        let expression_bytes: i32 = required_value(&row, 11, "check definition bytes")?;
        ensure_definition_size("check constraint", &id.to_string(), expression_bytes)?;
        let mut columns = Vec::new();
        if parent_column_id > 0 {
            columns.push(RawConstraintColumn {
                ordinal: 1,
                column_id: parent_column_id,
                name: required_string(&row, 6, "check constraint column")?,
                referenced_column_id: None,
                referenced_name: None,
            });
        }
        constraints.insert(
            id,
            RawConstraint {
                id,
                schema,
                table: required_string(&row, 2, "check constraint table")?,
                table_id: required_value(&row, 3, "check constraint table id")?,
                name: required_string(&row, 4, "check constraint name")?,
                kind: ConstraintKind::Check,
                columns,
                referenced_schema: None,
                referenced_table: None,
                referenced_table_id: None,
                delete_action: None,
                update_action: None,
                disabled: required_value(&row, 7, "check disabled flag")?,
                not_trusted: required_value(&row, 8, "check trust flag")?,
                not_for_replication: required_value(&row, 9, "check replication flag")?,
                expression: optional_string(&row, 10)?,
                expression_bytes,
            },
        );
    }
    Ok(constraints.into_values().collect())
}

async fn read_indexes(
    client: &mut TdsClient,
    selected_schemas: &BTreeSet<String>,
) -> Result<Vec<RawIndex>, CatalogError> {
    let sql = format!(
        "
        SELECT i.object_id,
               s.name,
               COALESCE(tt.name, o.name),
               RTRIM(o.type),
               i.index_id,
               i.name,
               i.type,
               i.type_desc,
               i.is_unique,
               i.is_primary_key,
               i.is_unique_constraint,
               i.is_disabled,
               i.is_hypothetical,
               i.is_padded,
               i.fill_factor,
               i.ignore_dup_key,
               i.allow_row_locks,
               i.allow_page_locks,
               i.auto_created,
               CASE WHEN DATALENGTH(i.filter_definition) <= {MAX_DEFINITION_BYTES} THEN i.filter_definition END,
               CAST(COALESCE(DATALENGTH(i.filter_definition), 0) AS int),
               i.data_space_id
        FROM sys.indexes i
        JOIN sys.objects o ON o.object_id = i.object_id
        LEFT JOIN sys.table_types tt ON tt.type_table_object_id = o.object_id
        JOIN sys.schemas s ON s.schema_id = COALESCE(tt.schema_id, o.schema_id)
        WHERE (o.is_ms_shipped = 0 OR o.type = 'TT')
          AND o.type IN ('U', 'V', 'TT')
          AND i.index_id > 0
          AND i.name IS NOT NULL
        ORDER BY i.object_id, i.index_id
        "
    );
    let mut indexes = BTreeMap::<(i32, i32), RawIndex>::new();
    for row in rows(client, &sql).await? {
        let schema = required_string(&row, 1, "index schema")?;
        if !selected_schemas.contains(&schema) {
            continue;
        }
        let object_id = required_value(&row, 0, "index object id")?;
        let id = required_value(&row, 4, "index id")?;
        let filter_bytes = required_value(&row, 20, "index filter bytes")?;
        ensure_definition_size("filtered index", &format!("{object_id}:{id}"), filter_bytes)?;
        indexes.insert(
            (object_id, id),
            RawIndex {
                object_id,
                schema,
                relation: required_string(&row, 2, "index relation")?,
                relation_type: required_string(&row, 3, "index relation type")?,
                id,
                name: required_string(&row, 5, "index name")?,
                type_code: required_value(&row, 6, "index type")?,
                type_desc: required_string(&row, 7, "index type description")?,
                unique: required_value(&row, 8, "index unique flag")?,
                primary: required_value(&row, 9, "index primary flag")?,
                unique_constraint: required_value(&row, 10, "index constraint flag")?,
                disabled: required_value(&row, 11, "index disabled flag")?,
                hypothetical: required_value(&row, 12, "index hypothetical flag")?,
                padded: required_value(&row, 13, "index padded flag")?,
                fill_factor: required_value(&row, 14, "index fill factor")?,
                ignore_duplicate_key: required_value(&row, 15, "index duplicate-key flag")?,
                allow_row_locks: required_value(&row, 16, "index row-lock flag")?,
                allow_page_locks: required_value(&row, 17, "index page-lock flag")?,
                auto_created: required_value(&row, 18, "index auto-created flag")?,
                filter: optional_string(&row, 19)?,
                filter_bytes,
                data_space_id: required_value(&row, 21, "index data space")?,
                columns: Vec::new(),
            },
        );
    }
    let column_rows = rows(
        client,
        "
        SELECT ic.object_id,
               ic.index_id,
               ic.index_column_id,
               ic.column_id,
               c.name,
               ic.key_ordinal,
               ic.partition_ordinal,
               ic.is_descending_key,
               ic.is_included_column
        FROM sys.index_columns ic
        JOIN sys.columns c
          ON c.object_id = ic.object_id AND c.column_id = ic.column_id
        JOIN sys.objects o ON o.object_id = ic.object_id
        WHERE (o.is_ms_shipped = 0 OR o.type = 'TT')
          AND o.type IN ('U', 'V', 'TT')
        ORDER BY ic.object_id, ic.index_id, ic.index_column_id
        ",
    )
    .await?;
    for row in column_rows {
        let identity = (
            required_value(&row, 0, "index column object id")?,
            required_value(&row, 1, "index column index id")?,
        );
        if let Some(index) = indexes.get_mut(&identity) {
            index.columns.push(RawIndexColumn {
                index_column_id: required_value(&row, 2, "index column id")?,
                column_id: required_value(&row, 3, "indexed column id")?,
                name: required_string(&row, 4, "indexed column name")?,
                key_ordinal: i32::from(required_value::<u8>(&row, 5, "index key ordinal")?),
                partition_ordinal: i32::from(required_value::<u8>(&row, 6, "partition ordinal")?),
                descending: required_value(&row, 7, "descending index flag")?,
                included: required_value(&row, 8, "included column flag")?,
            });
        }
    }
    for index in indexes.values() {
        if index.columns.is_empty() {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "index '{}.{}.{}' has no catalog-resolved columns",
                index.schema, index.relation, index.name
            )));
        }
    }
    Ok(indexes.into_values().collect())
}

async fn read_views(
    client: &mut TdsClient,
    selected_schemas: &BTreeSet<String>,
) -> Result<Vec<RawView>, CatalogError> {
    let sql = format!(
        "
        SELECT v.object_id,
               s.name,
               v.name,
               v.principal_id,
               v.is_replicated,
               v.has_replication_filter,
               COALESCE(m.is_schema_bound, CAST(0 AS bit)),
               COALESCE(m.uses_ansi_nulls, CAST(0 AS bit)),
               COALESCE(m.uses_quoted_identifier, CAST(0 AS bit)),
               m.execute_as_principal_id,
               CASE WHEN DATALENGTH(m.definition) <= {MAX_DEFINITION_BYTES} THEN m.definition END,
               CAST(COALESCE(DATALENGTH(m.definition), 0) AS int),
               CASE WHEN EXISTS (
                   SELECT 1 FROM sys.indexes i
                   WHERE i.object_id = v.object_id
                     AND i.index_id > 0
                     AND i.is_hypothetical = 0
               ) THEN CAST(1 AS bit) ELSE CAST(0 AS bit) END
        FROM sys.views v
        JOIN sys.schemas s ON s.schema_id = v.schema_id
        LEFT JOIN sys.sql_modules m ON m.object_id = v.object_id
        WHERE v.is_ms_shipped = 0
        ORDER BY v.object_id
        "
    );
    rows(client, &sql)
        .await?
        .into_iter()
        .map(|row| {
            Ok(RawView {
                id: required_value(&row, 0, "view id")?,
                schema: required_string(&row, 1, "view schema")?,
                name: required_string(&row, 2, "view name")?,
                principal_id: optional_value(&row, 3)?,
                replicated: required_value(&row, 4, "view replicated flag")?,
                replication_filter: required_value(&row, 5, "view replication filter flag")?,
                schema_bound: required_value(&row, 6, "view schema-bound flag")?,
                ansi_nulls: required_value(&row, 7, "view ANSI NULL flag")?,
                quoted_identifier: required_value(&row, 8, "view quoted identifier flag")?,
                execute_as_principal_id: optional_value(&row, 9)?,
                definition: optional_string(&row, 10)?,
                definition_bytes: required_value(&row, 11, "view definition bytes")?,
                indexed: required_value(&row, 12, "indexed view flag")?,
            })
        })
        .collect::<Result<Vec<_>, CatalogError>>()
        .and_then(|views| {
            for view in &views {
                ensure_definition_size(
                    "view",
                    &format!("{}.{}", view.schema, view.name),
                    view.definition_bytes,
                )?;
            }
            Ok(views
                .into_iter()
                .filter(|view| selected_schemas.contains(&view.schema))
                .collect())
        })
}

async fn read_routines(
    client: &mut TdsClient,
    strategy: SqlServerCatalogVersion,
    selected_schemas: &BTreeSet<String>,
) -> Result<Vec<RawRoutine>, CatalogError> {
    let (inlineable, inline_type) = strategy.routine_inline_expressions();
    let sql = format!(
        "
        SELECT o.object_id,
               s.name,
               o.name,
               RTRIM(o.type),
               o.type_desc,
               o.principal_id,
               COALESCE(m.is_schema_bound, CAST(0 AS bit)),
               COALESCE(m.is_recompiled, CAST(0 AS bit)),
               COALESCE(m.uses_native_compilation, CAST(0 AS bit)),
               COALESCE(m.uses_ansi_nulls, CAST(0 AS bit)),
               COALESCE(m.uses_quoted_identifier, CAST(0 AS bit)),
               m.execute_as_principal_id,
               COALESCE(m.null_on_null_input, CAST(0 AS bit)),
               {inlineable},
               {inline_type},
               COALESCE(p.is_auto_executed, CAST(0 AS bit)),
               COALESCE(p.is_execution_replicated, CAST(0 AS bit)),
               CASE WHEN DATALENGTH(m.definition) <= {MAX_DEFINITION_BYTES} THEN m.definition END,
               CAST(COALESCE(DATALENGTH(m.definition), 0) AS int)
        FROM sys.objects o
        JOIN sys.schemas s ON s.schema_id = o.schema_id
        LEFT JOIN sys.sql_modules m ON m.object_id = o.object_id
        LEFT JOIN sys.procedures p ON p.object_id = o.object_id
        WHERE o.is_ms_shipped = 0
          AND o.type IN ('P', 'PC', 'FN', 'IF', 'TF', 'FS', 'FT', 'AF')
        ORDER BY o.object_id
        "
    );
    rows(client, &sql)
        .await?
        .into_iter()
        .map(|row| {
            Ok(RawRoutine {
                id: required_value(&row, 0, "routine id")?,
                schema: required_string(&row, 1, "routine schema")?,
                name: required_string(&row, 2, "routine name")?,
                type_code: required_string(&row, 3, "routine type")?,
                type_desc: required_string(&row, 4, "routine type description")?,
                principal_id: optional_value(&row, 5)?,
                schema_bound: required_value(&row, 6, "routine schema-bound flag")?,
                recompiled: required_value(&row, 7, "routine recompile flag")?,
                native_compilation: required_value(&row, 8, "native compilation flag")?,
                ansi_nulls: required_value(&row, 9, "routine ANSI NULL flag")?,
                quoted_identifier: required_value(&row, 10, "routine quoted identifier flag")?,
                execute_as_principal_id: optional_value(&row, 11)?,
                null_on_null_input: required_value(&row, 12, "null-on-null flag")?,
                inlineable: required_value(&row, 13, "routine inlineable flag")?,
                inline_type: required_value(&row, 14, "routine inline type")?,
                startup: required_value(&row, 15, "startup procedure flag")?,
                replication: required_value(&row, 16, "routine replication flag")?,
                definition: optional_string(&row, 17)?,
                definition_bytes: required_value(&row, 18, "routine definition bytes")?,
            })
        })
        .collect::<Result<Vec<_>, CatalogError>>()
        .and_then(|routines| {
            for routine in &routines {
                ensure_definition_size(
                    "routine",
                    &format!("{}.{}", routine.schema, routine.name),
                    routine.definition_bytes,
                )?;
            }
            Ok(routines
                .into_iter()
                .filter(|routine| selected_schemas.contains(&routine.schema))
                .collect())
        })
}

async fn read_parameters(
    client: &mut TdsClient,
    routines: &[RawRoutine],
) -> Result<Vec<RawParameter>, CatalogError> {
    let routine_ids = routines
        .iter()
        .map(|routine| routine.id)
        .collect::<BTreeSet<_>>();
    rows(
        client,
        "
        SELECT p.object_id,
               p.parameter_id,
               COALESCE(NULLIF(p.name, N''), CASE WHEN p.parameter_id = 0 THEN N'return' ELSE N'unnamed' END),
               p.user_type_id,
               ts.name,
               ty.name,
               p.max_length,
               p.precision,
               p.scale,
               p.is_output,
               p.is_readonly,
               p.is_nullable,
               CONVERT(nvarchar(4000), p.default_value),
               p.xml_collection_id
        FROM sys.parameters p
        JOIN sys.objects o ON o.object_id = p.object_id
        JOIN sys.types ty ON ty.user_type_id = p.user_type_id
        JOIN sys.schemas ts ON ts.schema_id = ty.schema_id
        WHERE o.is_ms_shipped = 0
          AND o.type IN ('P', 'PC', 'FN', 'IF', 'TF', 'FS', 'FT', 'AF')
        ORDER BY p.object_id, p.parameter_id
        ",
    )
    .await?
    .into_iter()
    .map(|row| {
        Ok(RawParameter {
            object_id: required_value(&row, 0, "parameter object id")?,
            id: required_value(&row, 1, "parameter id")?,
            name: required_string(&row, 2, "parameter name")?,
            type_id: required_value(&row, 3, "parameter type id")?,
            type_schema: required_string(&row, 4, "parameter type schema")?,
            type_name: required_string(&row, 5, "parameter type name")?,
            max_length: required_value(&row, 6, "parameter max length")?,
            precision: required_value(&row, 7, "parameter precision")?,
            scale: required_value(&row, 8, "parameter scale")?,
            output: required_value(&row, 9, "parameter output flag")?,
            readonly: required_value(&row, 10, "parameter readonly flag")?,
            nullable: required_value(&row, 11, "parameter nullable flag")?,
            default_value: optional_string(&row, 12)?,
            xml_collection_id: required_value(&row, 13, "parameter XML collection id")?,
        })
    })
    .collect::<Result<Vec<_>, CatalogError>>()
    .map(|parameters| {
        parameters
            .into_iter()
            .filter(|parameter| routine_ids.contains(&parameter.object_id))
            .collect()
    })
}

async fn read_triggers(
    client: &mut TdsClient,
    selected_schemas: &BTreeSet<String>,
) -> Result<Vec<RawTrigger>, CatalogError> {
    let sql = format!(
        "
        SELECT tr.object_id,
               tr.name,
               tr.parent_class,
               tr.parent_id,
               ps.name,
               po.name,
               RTRIM(po.type),
               tr.is_instead_of_trigger,
               tr.is_disabled,
               tr.is_not_for_replication,
               COALESCE(m.is_schema_bound, CAST(0 AS bit)),
               m.execute_as_principal_id,
               CASE WHEN DATALENGTH(m.definition) <= {MAX_DEFINITION_BYTES} THEN m.definition END,
               CAST(COALESCE(DATALENGTH(m.definition), 0) AS int),
               CASE WHEN OBJECTPROPERTYEX(tr.object_id, 'ExecIsInsertTrigger') = 1 THEN CAST(1 AS bit) ELSE CAST(0 AS bit) END,
               CASE WHEN OBJECTPROPERTYEX(tr.object_id, 'ExecIsUpdateTrigger') = 1 THEN CAST(1 AS bit) ELSE CAST(0 AS bit) END,
               CASE WHEN OBJECTPROPERTYEX(tr.object_id, 'ExecIsDeleteTrigger') = 1 THEN CAST(1 AS bit) ELSE CAST(0 AS bit) END
        FROM sys.triggers tr
        LEFT JOIN sys.objects po ON po.object_id = tr.parent_id
        LEFT JOIN sys.schemas ps ON ps.schema_id = po.schema_id
        LEFT JOIN sys.sql_modules m ON m.object_id = tr.object_id
        WHERE tr.is_ms_shipped = 0
        ORDER BY tr.object_id
        "
    );
    let mut triggers = BTreeMap::<i32, RawTrigger>::new();
    for row in rows(client, &sql).await? {
        let parent_class = i32::from(required_value::<u8>(&row, 2, "trigger parent class")?);
        let parent_schema = optional_string(&row, 4)?;
        if parent_class == 1
            && !parent_schema
                .as_ref()
                .is_some_and(|schema| selected_schemas.contains(schema))
        {
            continue;
        }
        let id: i32 = required_value(&row, 0, "trigger id")?;
        let definition_bytes: i32 = required_value(&row, 13, "trigger definition bytes")?;
        ensure_definition_size("trigger", &id.to_string(), definition_bytes)?;
        triggers.insert(
            id,
            RawTrigger {
                id,
                name: required_string(&row, 1, "trigger name")?,
                parent_class,
                parent_id: required_value(&row, 3, "trigger parent id")?,
                parent_schema,
                parent_name: optional_string(&row, 5)?,
                parent_type: optional_string(&row, 6)?,
                instead_of: required_value(&row, 7, "instead-of trigger flag")?,
                disabled: required_value(&row, 8, "trigger disabled flag")?,
                not_for_replication: required_value(&row, 9, "trigger replication flag")?,
                schema_bound: required_value(&row, 10, "trigger schema-bound flag")?,
                execute_as_principal_id: optional_value(&row, 11)?,
                definition: optional_string(&row, 12)?,
                definition_bytes,
                insert_event: required_value(&row, 14, "insert trigger event")?,
                update_event: required_value(&row, 15, "update trigger event")?,
                delete_event: required_value(&row, 16, "delete trigger event")?,
                events: Vec::new(),
            },
        );
    }
    for row in rows(
        client,
        "
        SELECT object_id, type_desc
        FROM sys.trigger_events
        ORDER BY object_id, type
        ",
    )
    .await?
    {
        let id: i32 = required_value(&row, 0, "trigger event object id")?;
        if let Some(trigger) = triggers.get_mut(&id) {
            trigger
                .events
                .push(required_string(&row, 1, "trigger event type")?);
        }
    }
    for trigger in triggers.values_mut() {
        if trigger.insert_event {
            trigger.events.push("INSERT".to_owned());
        }
        if trigger.update_event {
            trigger.events.push("UPDATE".to_owned());
        }
        if trigger.delete_event {
            trigger.events.push("DELETE".to_owned());
        }
        trigger.events.sort();
        trigger.events.dedup();
        if trigger.events.is_empty() {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "trigger '{}' has no catalog-visible event",
                trigger.name
            )));
        }
    }
    Ok(triggers.into_values().collect())
}

async fn read_user_types(
    client: &mut TdsClient,
    selected_schemas: &BTreeSet<String>,
) -> Result<Vec<RawUserType>, CatalogError> {
    rows(
        client,
        "
        SELECT ty.user_type_id,
               s.name,
               ty.name,
               ty.system_type_id,
               COALESCE(bt.name, ty.name),
               ty.max_length,
               ty.precision,
               ty.scale,
               ty.collation_name,
               ty.is_nullable,
               ty.is_user_defined,
               ty.is_assembly_type,
               ty.is_table_type,
               tt.type_table_object_id,
               COALESCE(tt.is_memory_optimized, CAST(0 AS bit)),
               ty.default_object_id,
               ty.rule_object_id
        FROM sys.types ty
        JOIN sys.schemas s ON s.schema_id = ty.schema_id
        LEFT JOIN sys.table_types tt ON tt.user_type_id = ty.user_type_id
        LEFT JOIN sys.types bt
          ON bt.user_type_id = bt.system_type_id
         AND bt.system_type_id = ty.system_type_id
        WHERE ty.is_user_defined = 1
        ORDER BY ty.user_type_id
        ",
    )
    .await?
    .into_iter()
    .map(|row| {
        Ok(RawUserType {
            id: required_value(&row, 0, "user type id")?,
            schema: required_string(&row, 1, "user type schema")?,
            name: required_string(&row, 2, "user type name")?,
            system_type_id: required_value(&row, 3, "system type id")?,
            base_type: required_string(&row, 4, "base type")?,
            max_length: required_value(&row, 5, "type max length")?,
            precision: required_value(&row, 6, "type precision")?,
            scale: required_value(&row, 7, "type scale")?,
            collation: optional_string(&row, 8)?,
            nullable: required_value(&row, 9, "type nullable flag")?,
            user_defined: required_value(&row, 10, "user-defined flag")?,
            assembly: required_value(&row, 11, "assembly type flag")?,
            table_type: required_value(&row, 12, "table type flag")?,
            table_object_id: optional_value(&row, 13)?,
            memory_optimized: required_value(&row, 14, "memory optimized table type flag")?,
            default_object_id: required_value(&row, 15, "type default object id")?,
            rule_object_id: required_value(&row, 16, "type rule object id")?,
        })
    })
    .collect::<Result<Vec<_>, CatalogError>>()
    .map(|types| {
        types
            .into_iter()
            .filter(|data_type| selected_schemas.contains(&data_type.schema))
            .collect()
    })
}

async fn read_sequences(
    client: &mut TdsClient,
    selected_schemas: &BTreeSet<String>,
) -> Result<Vec<RawSequence>, CatalogError> {
    rows(
        client,
        "
        SELECT seq.object_id,
               s.name,
               seq.name,
               seq.principal_id,
               seq.user_type_id,
               ts.name,
               ty.name,
               seq.precision,
               seq.scale,
               CONVERT(nvarchar(128), seq.start_value),
               CONVERT(nvarchar(128), seq.increment),
               CONVERT(nvarchar(128), seq.minimum_value),
               CONVERT(nvarchar(128), seq.maximum_value),
               seq.is_cycling,
               seq.cache_size,
               seq.is_exhausted
        FROM sys.sequences seq
        JOIN sys.schemas s ON s.schema_id = seq.schema_id
        JOIN sys.types ty ON ty.user_type_id = seq.user_type_id
        JOIN sys.schemas ts ON ts.schema_id = ty.schema_id
        ORDER BY seq.object_id
        ",
    )
    .await?
    .into_iter()
    .map(|row| {
        Ok(RawSequence {
            id: required_value(&row, 0, "sequence id")?,
            schema: required_string(&row, 1, "sequence schema")?,
            name: required_string(&row, 2, "sequence name")?,
            principal_id: optional_value(&row, 3)?,
            type_id: required_value(&row, 4, "sequence type id")?,
            type_schema: required_string(&row, 5, "sequence type schema")?,
            type_name: required_string(&row, 6, "sequence type name")?,
            precision: required_value(&row, 7, "sequence precision")?,
            scale: required_value(&row, 8, "sequence scale")?,
            start_value: required_string(&row, 9, "sequence start")?,
            increment: required_string(&row, 10, "sequence increment")?,
            minimum_value: required_string(&row, 11, "sequence minimum")?,
            maximum_value: required_string(&row, 12, "sequence maximum")?,
            cyclic: required_value(&row, 13, "sequence cycle flag")?,
            cache_size: optional_value(&row, 14)?,
            exhausted: required_value(&row, 15, "sequence exhausted flag")?,
        })
    })
    .collect::<Result<Vec<_>, CatalogError>>()
    .map(|sequences| {
        sequences
            .into_iter()
            .filter(|sequence| selected_schemas.contains(&sequence.schema))
            .collect()
    })
}

async fn read_synonyms(
    client: &mut TdsClient,
    selected_schemas: &BTreeSet<String>,
) -> Result<Vec<RawSynonym>, CatalogError> {
    rows(
        client,
        "
        SELECT sn.object_id,
               s.name,
               sn.name,
               sn.principal_id,
               sn.base_object_name,
               PARSENAME(sn.base_object_name, 4),
               PARSENAME(sn.base_object_name, 3),
               PARSENAME(sn.base_object_name, 2),
               PARSENAME(sn.base_object_name, 1)
        FROM sys.synonyms sn
        JOIN sys.schemas s ON s.schema_id = sn.schema_id
        ORDER BY sn.object_id
        ",
    )
    .await?
    .into_iter()
    .map(|row| {
        Ok(RawSynonym {
            id: required_value(&row, 0, "synonym id")?,
            schema: required_string(&row, 1, "synonym schema")?,
            name: required_string(&row, 2, "synonym name")?,
            principal_id: optional_value(&row, 3)?,
            base_object_name: required_string(&row, 4, "synonym target")?,
            server: optional_string(&row, 5)?,
            database: optional_string(&row, 6)?,
            target_schema: optional_string(&row, 7)?,
            target_entity: optional_string(&row, 8)?,
        })
    })
    .collect::<Result<Vec<_>, CatalogError>>()
    .map(|synonyms| {
        synonyms
            .into_iter()
            .filter(|synonym| selected_schemas.contains(&synonym.schema))
            .collect()
    })
}

async fn read_dependencies(
    client: &mut TdsClient,
    selected_schemas: &BTreeSet<String>,
) -> Result<Vec<RawDependency>, CatalogError> {
    rows(
        client,
        "
        SELECT sed.referencing_class,
               sed.referencing_id,
               sed.referencing_minor_id,
               sed.referenced_class,
               sed.referenced_server_name,
               sed.referenced_database_name,
               sed.referenced_schema_name,
               sed.referenced_entity_name,
               sed.referenced_id,
               sed.referenced_minor_id,
               sed.is_schema_bound_reference,
               sed.is_caller_dependent,
               sed.is_ambiguous,
               CASE
                   WHEN sed.referencing_class = 12 THEN N'__database__'
                   ELSE OBJECT_SCHEMA_NAME(sed.referencing_id)
               END
        FROM sys.sql_expression_dependencies sed
        ORDER BY sed.referencing_class,
                 sed.referencing_id,
                 sed.referencing_minor_id,
                 sed.referenced_class,
                 sed.referenced_server_name,
                 sed.referenced_database_name,
                 sed.referenced_schema_name,
                 sed.referenced_entity_name,
                 sed.referenced_minor_id
        ",
    )
    .await?
    .into_iter()
    .map(|row| {
        let dependency = RawDependency {
            referencing_class: i32::from(required_value::<u8>(&row, 0, "referencing class")?),
            referencing_id: required_value(&row, 1, "referencing id")?,
            referencing_minor_id: required_value(&row, 2, "referencing minor id")?,
            referenced_class: i32::from(required_value::<u8>(&row, 3, "referenced class")?),
            referenced_server: optional_string(&row, 4)?,
            referenced_database: optional_string(&row, 5)?,
            referenced_schema: optional_string(&row, 6)?,
            referenced_entity: required_string(&row, 7, "referenced entity")?,
            referenced_id: optional_value(&row, 8)?,
            referenced_minor_id: required_value(&row, 9, "referenced minor id")?,
            schema_bound: required_value(&row, 10, "schema-bound dependency flag")?,
            caller_dependent: required_value(&row, 11, "caller-dependent flag")?,
            ambiguous: required_value(&row, 12, "ambiguous dependency flag")?,
        };
        let source_schema = required_string(&row, 13, "dependency source schema")?;
        Ok((source_schema, dependency))
    })
    .collect::<Result<Vec<_>, CatalogError>>()
    .map(|dependencies| {
        dependencies
            .into_iter()
            .filter(|(schema, _)| schema == "__database__" || selected_schemas.contains(schema))
            .map(|(_, dependency)| dependency)
            .collect()
    })
}

async fn read_partition_functions(
    client: &mut TdsClient,
) -> Result<Vec<RawPartitionFunction>, CatalogError> {
    let mut functions = BTreeMap::<i32, RawPartitionFunction>::new();
    for row in rows(
        client,
        "
        SELECT function_id, name, fanout, boundary_value_on_right, is_system
        FROM sys.partition_functions
        ORDER BY function_id
        ",
    )
    .await?
    {
        let id = required_value(&row, 0, "partition function id")?;
        functions.insert(
            id,
            RawPartitionFunction {
                id,
                name: required_string(&row, 1, "partition function name")?,
                fanout: required_value(&row, 2, "partition function fanout")?,
                boundary_on_right: required_value(&row, 3, "partition boundary side")?,
                system: required_value(&row, 4, "system partition function flag")?,
                values: Vec::new(),
            },
        );
    }
    for row in rows(
        client,
        "
        SELECT function_id, boundary_id, CONVERT(nvarchar(4000), value)
        FROM sys.partition_range_values
        ORDER BY function_id, boundary_id
        ",
    )
    .await?
    {
        let function_id: i32 = required_value(&row, 0, "partition value function id")?;
        let function = functions.get_mut(&function_id).ok_or_else(|| {
            CatalogError::Mapping(format!(
                "partition range value references missing function {function_id}"
            ))
        })?;
        function.values.push(RawPartitionValue {
            boundary_id: required_value(&row, 1, "partition boundary id")?,
            value: optional_string(&row, 2)?,
        });
    }
    Ok(functions
        .into_values()
        .filter(|function| !function.system)
        .collect())
}

async fn read_partition_schemes(
    client: &mut TdsClient,
) -> Result<Vec<RawPartitionScheme>, CatalogError> {
    rows(
        client,
        "
        SELECT data_space_id, name, function_id
        FROM sys.partition_schemes
        ORDER BY data_space_id
        ",
    )
    .await?
    .into_iter()
    .map(|row| {
        Ok(RawPartitionScheme {
            id: required_value(&row, 0, "partition scheme id")?,
            name: required_string(&row, 1, "partition scheme name")?,
            function_id: required_value(&row, 2, "partition scheme function id")?,
        })
    })
    .collect()
}

async fn read_partitions(
    client: &mut TdsClient,
    strategy: SqlServerCatalogVersion,
    tables: &[RawTable],
    views: &[RawView],
) -> Result<Vec<RawPartition>, CatalogError> {
    let object_ids = tables
        .iter()
        .map(|table| table.id)
        .chain(views.iter().map(|view| view.id))
        .collect::<BTreeSet<_>>();
    let xml_column = strategy.xml_compression_expression();
    let sql = format!(
        "
        SELECT p.object_id,
               p.index_id,
               p.partition_number,
               p.data_compression_desc,
               {xml_column}
        FROM sys.partitions p
        JOIN sys.objects o ON o.object_id = p.object_id
        WHERE o.is_ms_shipped = 0
          AND o.type IN ('U', 'V')
        ORDER BY p.object_id, p.index_id, p.partition_number
        "
    );
    rows(client, &sql)
        .await?
        .into_iter()
        .map(|row| {
            Ok(RawPartition {
                object_id: required_value(&row, 0, "partition object id")?,
                index_id: required_value(&row, 1, "partition index id")?,
                partition_number: required_value(&row, 2, "partition number")?,
                data_compression: required_string(&row, 3, "partition compression")?,
                xml_compression: required_string(&row, 4, "partition XML compression")?,
            })
        })
        .collect::<Result<Vec<_>, CatalogError>>()
        .map(|partitions| {
            partitions
                .into_iter()
                .filter(|partition| object_ids.contains(&partition.object_id))
                .collect()
        })
}

async fn read_security_policies(
    client: &mut TdsClient,
    selected_schemas: &BTreeSet<String>,
) -> Result<Vec<RawSecurityPolicy>, CatalogError> {
    let mut policies = BTreeMap::<i32, RawSecurityPolicy>::new();
    for row in rows(
        client,
        "
        SELECT sp.object_id,
               s.name,
               sp.name,
               sp.principal_id,
               sp.is_enabled,
               sp.is_schema_bound
        FROM sys.security_policies sp
        JOIN sys.schemas s ON s.schema_id = sp.schema_id
        ORDER BY sp.object_id
        ",
    )
    .await?
    {
        let schema = required_string(&row, 1, "security policy schema")?;
        if !selected_schemas.contains(&schema) {
            continue;
        }
        let id = required_value(&row, 0, "security policy id")?;
        policies.insert(
            id,
            RawSecurityPolicy {
                id,
                schema,
                name: required_string(&row, 2, "security policy name")?,
                principal_id: optional_value(&row, 3)?,
                enabled: required_value(&row, 4, "security policy enabled flag")?,
                schema_bound: required_value(&row, 5, "security policy schema-bound flag")?,
                predicates: Vec::new(),
            },
        );
    }
    let predicate_sql = format!(
        "
        SELECT object_id,
               security_predicate_id,
               target_object_id,
               predicate_type_desc,
               operation_desc,
               CASE WHEN DATALENGTH(predicate_definition) <= {MAX_DEFINITION_BYTES} THEN predicate_definition END,
               CAST(COALESCE(DATALENGTH(predicate_definition), 0) AS int)
        FROM sys.security_predicates
        ORDER BY object_id, security_predicate_id
        "
    );
    for row in rows(client, &predicate_sql).await? {
        let policy_id: i32 = required_value(&row, 0, "predicate policy id")?;
        if let Some(policy) = policies.get_mut(&policy_id) {
            let id: i32 = required_value(&row, 1, "predicate id")?;
            let definition_bytes: i32 = required_value(&row, 6, "predicate definition bytes")?;
            ensure_definition_size("security predicate", &id.to_string(), definition_bytes)?;
            policy.predicates.push(RawSecurityPredicate {
                id,
                target_object_id: required_value(&row, 2, "predicate target")?,
                predicate_type: required_string(&row, 3, "predicate type")?,
                operation: optional_string(&row, 4)?,
                definition: required_string(&row, 5, "predicate definition")?,
                definition_bytes,
            });
        }
    }
    Ok(policies.into_values().collect())
}

async fn read_xml_schema_collections(
    client: &mut TdsClient,
    selected_schemas: &BTreeSet<String>,
) -> Result<Vec<RawXmlSchemaCollection>, CatalogError> {
    let mut collections = BTreeMap::<i32, RawXmlSchemaCollection>::new();
    for row in rows(
        client,
        "
        SELECT xsc.xml_collection_id,
               s.name,
               xsc.name,
               xsc.principal_id,
               CONVERT(nvarchar(33), xsc.create_date, 126),
               CONVERT(nvarchar(33), xsc.modify_date, 126),
               xsn.xml_namespace_id,
               xsn.name
        FROM sys.xml_schema_collections xsc
        JOIN sys.schemas s ON s.schema_id = xsc.schema_id
        LEFT JOIN sys.xml_schema_namespaces xsn
          ON xsn.xml_collection_id = xsc.xml_collection_id
        ORDER BY xsc.xml_collection_id, xsn.xml_namespace_id
        ",
    )
    .await?
    {
        let schema = required_string(&row, 1, "XML schema collection schema")?;
        if !selected_schemas.contains(&schema) {
            continue;
        }
        let id = required_value(&row, 0, "XML schema collection id")?;
        let name = required_string(&row, 2, "XML schema collection name")?;
        let principal_id = optional_value(&row, 3)?;
        let created_at = required_string(&row, 4, "XML schema collection creation time")?;
        let modified_at = required_string(&row, 5, "XML schema collection modification time")?;
        let entry = collections
            .entry(id)
            .or_insert_with(|| RawXmlSchemaCollection {
                id,
                schema: schema.clone(),
                name: name.clone(),
                principal_id,
                created_at: created_at.clone(),
                modified_at: modified_at.clone(),
                namespaces: Vec::new(),
            });
        if entry.schema != schema
            || entry.name != name
            || entry.principal_id != principal_id
            || entry.created_at != created_at
            || entry.modified_at != modified_at
        {
            return Err(CatalogError::Mapping(format!(
                "XML schema collection {id} has inconsistent catalog rows"
            )));
        }
        if let Some(namespace_id) = optional_value(&row, 6)? {
            let namespace_name = optional_value::<&str>(&row, 7)?
                .ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "XML schema collection {id} namespace {namespace_id} has no name"
                    ))
                })?
                .to_owned();
            if namespace_name.len() > MAX_PROPERTY_STRING_BYTES {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "XML namespace exceeds the {MAX_PROPERTY_STRING_BYTES}-byte property limit"
                )));
            }
            entry.namespaces.push(RawXmlSchemaNamespace {
                id: namespace_id,
                name: namespace_name,
            });
        }
    }
    Ok(collections.into_values().collect())
}

async fn read_extended_properties(
    client: &mut TdsClient,
) -> Result<Vec<RawExtendedProperty>, CatalogError> {
    rows(
        client,
        "
        SELECT ep.class,
               ep.class_desc,
               ep.major_id,
               ep.minor_id,
               ep.name,
               CONVERT(nvarchar(128), SQL_VARIANT_PROPERTY(ep.value, 'BaseType')),
               TRY_CONVERT(int, SQL_VARIANT_PROPERTY(ep.value, 'Precision')),
               TRY_CONVERT(int, SQL_VARIANT_PROPERTY(ep.value, 'Scale')),
               TRY_CONVERT(int, SQL_VARIANT_PROPERTY(ep.value, 'MaxLength')),
               CONVERT(nvarchar(128), SQL_VARIANT_PROPERTY(ep.value, 'Collation')),
               CASE
                 WHEN ep.value IS NULL THEN NULL
                 WHEN CONVERT(nvarchar(128), SQL_VARIANT_PROPERTY(ep.value, 'BaseType'))
                      IN (N'binary', N'varbinary')
                   THEN CONVERT(nvarchar(max), CONVERT(varchar(max), CONVERT(varbinary(8000), ep.value), 2))
                 ELSE CONVERT(nvarchar(max), ep.value, 126)
               END,
               CASE WHEN ep.value IS NULL THEN NULL
                    ELSE CONVERT(nvarchar(max), CONVERT(varchar(max), CONVERT(varbinary(8000), ep.value), 2))
               END
        FROM sys.extended_properties ep
        ORDER BY ep.class, ep.major_id, ep.minor_id, ep.name
        ",
    )
    .await?
    .into_iter()
    .map(|row| {
        Ok(RawExtendedProperty {
            class: required_value(&row, 0, "extended property class")?,
            class_description: required_string(&row, 1, "extended property class description")?,
            major_id: required_value(&row, 2, "extended property major id")?,
            minor_id: required_value(&row, 3, "extended property minor id")?,
            name: required_string(&row, 4, "extended property name")?,
            value_type: optional_string(&row, 5)?,
            value_precision: optional_value(&row, 6)?,
            value_scale: optional_value(&row, 7)?,
            value_max_length: optional_value(&row, 8)?,
            value_collation: optional_string(&row, 9)?,
            display_value: optional_string(&row, 10)?,
            value_hex: optional_string(&row, 11)?,
        })
    })
    .collect()
}

#[allow(clippy::too_many_arguments)]
fn select_extended_properties(
    properties: Vec<RawExtendedProperty>,
    schemas: &[RawSchema],
    principals: &[RawPrincipal],
    tables: &[RawTable],
    columns: &[RawColumn],
    constraints: &[RawConstraint],
    indexes: &[RawIndex],
    views: &[RawView],
    routines: &[RawRoutine],
    parameters: &[RawParameter],
    triggers: &[RawTrigger],
    user_types: &[RawUserType],
    sequences: &[RawSequence],
    synonyms: &[RawSynonym],
    partition_functions: &[RawPartitionFunction],
    partition_schemes: &[RawPartitionScheme],
    security_policies: &[RawSecurityPolicy],
    xml_schema_collections: &[RawXmlSchemaCollection],
) -> Vec<RawExtendedProperty> {
    let mut object_ids = tables
        .iter()
        .map(|object| object.id)
        .collect::<BTreeSet<_>>();
    object_ids.extend(views.iter().map(|object| object.id));
    object_ids.extend(routines.iter().map(|object| object.id));
    object_ids.extend(triggers.iter().map(|object| object.id));
    object_ids.extend(sequences.iter().map(|object| object.id));
    object_ids.extend(synonyms.iter().map(|object| object.id));
    object_ids.extend(security_policies.iter().map(|object| object.id));
    object_ids.extend(constraints.iter().map(|object| object.id));
    let column_ids = columns
        .iter()
        .map(|column| (column.object_id, column.id))
        .collect::<BTreeSet<_>>();
    let parameter_ids = parameters
        .iter()
        .map(|parameter| (parameter.object_id, parameter.id))
        .collect::<BTreeSet<_>>();
    let schema_ids = schemas
        .iter()
        .map(|schema| schema.id)
        .collect::<BTreeSet<_>>();
    let principal_ids = principals
        .iter()
        .map(|principal| principal.id)
        .collect::<BTreeSet<_>>();
    let type_ids = user_types
        .iter()
        .map(|data_type| data_type.id)
        .collect::<BTreeSet<_>>();
    let index_ids = indexes
        .iter()
        .map(|index| (index.object_id, index.id))
        .collect::<BTreeSet<_>>();
    let table_type_column_ids = user_types
        .iter()
        .filter_map(|data_type| {
            data_type
                .table_object_id
                .map(|object_id| (data_type.id, object_id))
        })
        .flat_map(|(user_type_id, object_id)| {
            columns.iter().filter_map(move |column| {
                (column.object_id == object_id).then_some((user_type_id, column.id))
            })
        })
        .collect::<BTreeSet<_>>();
    let xml_collection_ids = xml_schema_collections
        .iter()
        .map(|collection| collection.id)
        .collect::<BTreeSet<_>>();
    let partition_function_ids = partition_functions
        .iter()
        .map(|function| function.id)
        .collect::<BTreeSet<_>>();
    let partition_scheme_ids = partition_schemes
        .iter()
        .map(|scheme| scheme.id)
        .collect::<BTreeSet<_>>();

    properties
        .into_iter()
        .filter(|property| match property.class {
            0 => property.major_id == 0 && property.minor_id == 0,
            1 if property.minor_id == 0 => object_ids.contains(&property.major_id),
            1 => column_ids.contains(&(property.major_id, property.minor_id)),
            2 => parameter_ids.contains(&(property.major_id, property.minor_id)),
            3 => schema_ids.contains(&property.major_id),
            4 => principal_ids.contains(&property.major_id),
            6 => type_ids.contains(&property.major_id),
            7 => index_ids.contains(&(property.major_id, property.minor_id)),
            8 => table_type_column_ids.contains(&(property.major_id, property.minor_id)),
            10 => xml_collection_ids.contains(&property.major_id),
            20 => partition_scheme_ids.contains(&property.major_id),
            21 => partition_function_ids.contains(&property.major_id),
            _ => false,
        })
        .collect()
}

async fn read_unsupported_objects(
    client: &mut TdsClient,
    strategy: SqlServerCatalogVersion,
    selected_schemas: &BTreeSet<String>,
) -> Result<Vec<RawUnsupportedObject>, CatalogError> {
    let edge_constraints = strategy.edge_constraint_union();
    let sql = format!(
        "
        SELECT s.name, o.name, RTRIM(o.type), o.type_desc
        FROM sys.objects o
        LEFT JOIN sys.schemas s ON s.schema_id = o.schema_id
        WHERE o.is_ms_shipped = 0
          AND ((o.type = 'D' AND o.parent_object_id = 0) OR o.type IN ('R', 'TA'))
        UNION ALL
        SELECT s.name, et.name, N'ET', N'EXTERNAL_TABLE'
        FROM sys.external_tables et
        JOIN sys.schemas s ON s.schema_id = et.schema_id
        {edge_constraints}
        UNION ALL
        SELECT s.name, p.name, N'NP', N'NUMBERED_PROCEDURE'
        FROM sys.numbered_procedures np
        JOIN sys.procedures p ON p.object_id = np.object_id
        JOIN sys.schemas s ON s.schema_id = p.schema_id
        WHERE np.procedure_number > 1
        ORDER BY 1, 2, 3
        ",
    );
    rows(client, &sql)
        .await?
        .into_iter()
        .map(|row| {
            Ok(RawUnsupportedObject {
                schema: optional_string(&row, 0)?,
                name: required_string(&row, 1, "unsupported object name")?,
                type_code: required_string(&row, 2, "unsupported object type")?,
                type_desc: required_string(&row, 3, "unsupported object type description")?,
            })
        })
        .collect::<Result<Vec<_>, CatalogError>>()
        .map(|objects| {
            objects
                .into_iter()
                .filter(|object| {
                    object
                        .schema
                        .as_ref()
                        .is_none_or(|schema| selected_schemas.contains(schema))
                })
                .collect()
        })
}

fn validate_supported_metadata(
    unsupported: &[RawUnsupportedObject],
    views: &[RawView],
    routines: &[RawRoutine],
    triggers: &[RawTrigger],
    user_types: &[RawUserType],
    dependencies: &[RawDependency],
) -> Result<(), CatalogError> {
    if let Some(object) = unsupported.first() {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "SQL Server object '{}.{}' has unsupported catalog type {} ({})",
            object.schema.as_deref().unwrap_or("database"),
            object.name,
            object.type_code,
            object.type_desc
        )));
    }
    for view in views {
        require_visible_definition(
            "view",
            &format!("{}.{}", view.schema, view.name),
            view.definition.as_deref(),
        )?;
    }
    for routine in routines {
        if matches!(routine.type_code.as_str(), "PC" | "FS" | "FT" | "AF") {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "CLR routine '{}.{}' has no authoritative SQL dependency body",
                routine.schema, routine.name
            )));
        }
        let definition = require_visible_definition(
            "routine",
            &format!("{}.{}", routine.schema, routine.name),
            routine.definition.as_deref(),
        )?;
        reject_dynamic_sql(
            "routine",
            &format!("{}.{}", routine.schema, routine.name),
            definition,
        )?;
    }
    for trigger in triggers {
        let definition =
            require_visible_definition("trigger", &trigger.name, trigger.definition.as_deref())?;
        reject_dynamic_sql("trigger", &trigger.name, definition)?;
    }
    if let Some(data_type) = user_types.iter().find(|data_type| data_type.assembly) {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "CLR user-defined type '{}.{}' requires assembly metadata mapping",
            data_type.schema, data_type.name
        )));
    }
    if let Some(data_type) = user_types.iter().find(|data_type| {
        data_type.table_type != data_type.table_object_id.is_some()
            || (!data_type.table_type && data_type.memory_optimized)
    }) {
        return Err(CatalogError::Mapping(format!(
            "user-defined type '{}.{}' has inconsistent table-type catalog identity",
            data_type.schema, data_type.name
        )));
    }
    if let Some(data_type) = user_types
        .iter()
        .find(|data_type| data_type.default_object_id != 0 || data_type.rule_object_id != 0)
    {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "user-defined type '{}.{}' uses a legacy bound default or rule whose dependencies are not catalog-maintained",
            data_type.schema, data_type.name
        )));
    }
    if let Some(dependency) = dependencies
        .iter()
        .find(|dependency| dependency.caller_dependent || dependency.ambiguous)
    {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "dependency from object {} is resolved only at runtime (caller_dependent={}, ambiguous={})",
            dependency.referencing_id, dependency.caller_dependent, dependency.ambiguous
        )));
    }
    Ok(())
}

fn require_visible_definition<'a>(
    kind: &str,
    name: &str,
    definition: Option<&'a str>,
) -> Result<&'a str, CatalogError> {
    definition
        .filter(|definition| !definition.trim().is_empty())
        .ok_or_else(|| {
            CatalogError::UnsupportedMetadata(format!(
                "{kind} '{name}' has a hidden, encrypted, or unavailable definition"
            ))
        })
}

fn reject_dynamic_sql(kind: &str, name: &str, definition: &str) -> Result<(), CatalogError> {
    let dialect = MsSqlDialect {};
    let tokens = Tokenizer::new(&dialect, definition)
        .tokenize()
        .map_err(|error| {
            CatalogError::UnsupportedMetadata(format!(
                "{kind} '{name}' cannot be tokenized for dynamic SQL validation: {error}"
            ))
        })?;
    let tokens = tokens
        .iter()
        .filter(|token| !matches!(token, Token::Whitespace(_)))
        .collect::<Vec<_>>();
    for (index, token) in tokens.iter().enumerate() {
        let Token::Word(word) = token else {
            continue;
        };
        if !matches!(word.keyword, Keyword::EXEC | Keyword::EXECUTE)
            && !word.value.eq_ignore_ascii_case("EXEC")
            && !word.value.eq_ignore_ascii_case("EXECUTE")
        {
            continue;
        }
        let rest = &tokens[index + 1..];
        if rest
            .first()
            .is_some_and(|token| matches!(token, Token::Word(word) if word.keyword == Keyword::AS))
        {
            continue;
        }
        if execute_target_is_dynamic(rest) {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "{kind} '{name}' executes dynamic SQL whose dependencies are not catalog-maintained"
            )));
        }
    }
    Ok(())
}

fn execute_target_is_dynamic(tokens: &[&Token]) -> bool {
    let Some(first) = tokens.first() else {
        return true;
    };
    if is_string_token(first) {
        return true;
    }
    if matches!(first, Token::LParen) {
        return tokens
            .get(1)
            .is_none_or(|token| is_variable_token(token) || is_string_token(token));
    }
    if is_variable_token(first) {
        if !tokens
            .get(1)
            .is_some_and(|token| matches!(token, Token::Eq))
        {
            return true;
        }
        return tokens
            .get(2)
            .is_none_or(|token| is_variable_token(token) || is_string_token(token));
    }
    tokens.iter().take(7).any(|token| {
        matches!(token, Token::Word(word) if word.value.eq_ignore_ascii_case("sp_executesql"))
    })
}

fn is_variable_token(token: &Token) -> bool {
    matches!(token, Token::AtSign)
        || matches!(token, Token::Word(word) if word.value.starts_with('@'))
}

fn is_string_token(token: &Token) -> bool {
    matches!(
        token,
        Token::SingleQuotedString(_)
            | Token::NationalStringLiteral(_)
            | Token::DoubleQuotedString(_)
    )
}

fn ensure_definition_size(kind: &str, name: &str, bytes: i32) -> Result<(), CatalogError> {
    if bytes > MAX_DEFINITION_BYTES {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "{kind} '{name}' definition is {bytes} bytes; limit is {MAX_DEFINITION_BYTES}"
        )));
    }
    Ok(())
}

async fn rows(client: &mut TdsClient, sql: &str) -> Result<Vec<Row>, CatalogError> {
    Ok(client.simple_query(sql).await?.into_first_result().await?)
}

async fn query_one(client: &mut TdsClient, sql: &str) -> Result<Row, CatalogError> {
    client
        .simple_query(sql)
        .await?
        .into_row()
        .await?
        .ok_or_else(|| CatalogError::Mapping("required catalog query returned no row".to_owned()))
}

fn required_value<'a, T>(row: &'a Row, index: usize, field: &str) -> Result<T, CatalogError>
where
    T: FromSql<'a>,
{
    row.try_get(index)
        .map_err(|error| CatalogError::Mapping(format!("cannot read {field}: {error}")))?
        .ok_or_else(|| CatalogError::Mapping(format!("required {field} is NULL")))
}

fn optional_value<'a, T>(row: &'a Row, index: usize) -> Result<Option<T>, CatalogError>
where
    T: FromSql<'a>,
{
    row.try_get(index).map_err(|error| {
        CatalogError::Mapping(format!(
            "cannot read optional catalog field at column {index}: {error}"
        ))
    })
}

fn required_string(row: &Row, index: usize, field: &str) -> Result<String, CatalogError> {
    let value = required_value::<&str>(row, index, field)?.to_owned();
    if value.is_empty() {
        return Err(CatalogError::Mapping(format!("required {field} is empty")));
    }
    if value.len() > MAX_PROPERTY_STRING_BYTES {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "{field} exceeds the {MAX_PROPERTY_STRING_BYTES}-byte property limit"
        )));
    }
    Ok(value)
}

fn optional_string(row: &Row, index: usize) -> Result<Option<String>, CatalogError> {
    let value = optional_value::<&str>(row, index)?.map(str::to_owned);
    if value
        .as_ref()
        .is_some_and(|value| value.len() > MAX_PROPERTY_STRING_BYTES)
    {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "catalog property exceeds the {MAX_PROPERTY_STRING_BYTES}-byte limit"
        )));
    }
    Ok(value)
}

struct SqlServerSnapshotMapper {
    connection_alias: String,
    facts: ServerFacts,
    strategy: SqlServerCatalogVersion,
}

