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

