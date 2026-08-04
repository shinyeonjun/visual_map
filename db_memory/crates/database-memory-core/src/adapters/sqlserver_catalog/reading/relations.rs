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

