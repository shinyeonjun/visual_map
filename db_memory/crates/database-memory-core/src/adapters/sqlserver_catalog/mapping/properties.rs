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
