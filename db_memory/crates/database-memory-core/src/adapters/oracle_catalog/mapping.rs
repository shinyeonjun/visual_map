impl NamedCatalogColumn for RawConstraintColumn {
    fn name(&self) -> &str {
        &self.name
    }
}

impl NamedCatalogColumn for RawIndexColumn {
    fn name(&self) -> &str {
        &self.name
    }
}

fn resolve_named_columns<T: NamedCatalogColumn>(
    owner: &str,
    table: &str,
    raw_columns: &[T],
    column_keys: &BTreeMap<(String, String, String), ObjectKey>,
    subject: &str,
) -> Result<Vec<ObjectKey>, CatalogError> {
    raw_columns
        .iter()
        .map(|column| {
            required(
                column_keys.get(&(owner.to_owned(), table.to_owned(), column.name().to_owned())),
                format!(
                    "Oracle column {}.{}.{} for {}",
                    owner,
                    table,
                    column.name(),
                    subject
                ),
            )
            .cloned()
        })
        .collect()
}

fn inventory_properties(object: &RawInventoryObject) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_i64(&mut properties, "oracle_object_id", object.object_id);
    insert_optional_i64(
        &mut properties,
        "oracle_data_object_id",
        object.data_object_id,
    );
    insert_string(&mut properties, "object_status", &object.status);
    insert_bool(&mut properties, "temporary", object.temporary);
    insert_bool(&mut properties, "generated", object.generated);
    insert_bool(&mut properties, "secondary", object.secondary);
    insert_i64(&mut properties, "namespace", object.namespace);
    insert_optional_string(
        &mut properties,
        "edition_name",
        object.edition_name.as_deref(),
    );
    insert_optional_string(
        &mut properties,
        "editionable",
        object.editionable.as_deref(),
    );
    insert_optional_string(
        &mut properties,
        "default_collation",
        object.default_collation.as_deref(),
    );
    properties
}

fn oracle_column_properties(column: &RawColumn) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_optional_i64(&mut properties, "column_id", column.column_id);
    insert_i64(
        &mut properties,
        "internal_column_id",
        column.internal_column_id,
    );
    insert_i64(&mut properties, "data_length", column.data_length);
    insert_optional_i64(&mut properties, "data_precision", column.data_precision);
    insert_optional_i64(&mut properties, "data_scale", column.data_scale);
    insert_optional_i64(&mut properties, "char_length", column.char_length);
    insert_optional_string(&mut properties, "char_used", column.char_used.as_deref());
    insert_optional_string(&mut properties, "collation", column.collation.as_deref());
    insert_optional_string(
        &mut properties,
        "data_type_owner",
        column.data_type_owner.as_deref(),
    );
    insert_bool(&mut properties, "hidden", column.hidden);
    insert_bool(&mut properties, "virtual", column.virtual_column);
    insert_bool(&mut properties, "user_generated", column.user_generated);
    insert_bool(&mut properties, "default_on_null", column.default_on_null);
    insert_bool(&mut properties, "identity", column.identity);
    properties
}

fn oracle_index_properties(
    index: &RawIndex,
    inventory_object: &RawInventoryObject,
) -> BTreeMap<String, MetadataValue> {
    let mut properties = inventory_properties(inventory_object);
    insert_string(&mut properties, "index_type", &index.index_type);
    insert_string(&mut properties, "index_status", &index.status);
    insert_bool(&mut properties, "temporary", index.temporary);
    insert_bool(&mut properties, "generated", index.generated);
    insert_string(&mut properties, "visibility", &index.visibility);
    insert_optional_string(
        &mut properties,
        "function_status",
        index.function_status.as_deref(),
    );
    insert_bool(&mut properties, "constraint_index", index.constraint_index);
    properties.insert(
        "key_parts".to_owned(),
        MetadataValue::StringList(
            index
                .columns
                .iter()
                .map(|column| {
                    let value = column.expression.as_deref().unwrap_or(&column.name);
                    if column.descending {
                        format!("{value} DESC")
                    } else {
                        value.to_owned()
                    }
                })
                .collect(),
        ),
    );
    properties.insert(
        "descending_columns".to_owned(),
        MetadataValue::StringList(
            index
                .columns
                .iter()
                .filter(|column| column.descending)
                .map(|column| column.name.clone())
                .collect(),
        ),
    );
    properties
}

fn oracle_index_expression(index: &RawIndex) -> Option<String> {
    let expressions = index
        .columns
        .iter()
        .filter_map(|column| {
            column.expression.as_ref().map(|expression| {
                if column.descending {
                    format!("{expression} DESC")
                } else {
                    expression.clone()
                }
            })
        })
        .collect::<Vec<_>>();
    (!expressions.is_empty()).then(|| expressions.join(", "))
}

fn add_oracle_partitioned_table_properties(
    properties: &mut BTreeMap<String, MetadataValue>,
    table: &RawPartitionedTable,
    key_columns: &[RawPartitionKeyColumn],
) {
    insert_bool(properties, "partitioned", true);
    insert_string(properties, "partitioning_type", &table.partitioning_type);
    insert_string(
        properties,
        "subpartitioning_type",
        &table.subpartitioning_type,
    );
    insert_i64(properties, "partition_count", table.partition_count);
    insert_i64(
        properties,
        "default_subpartition_count",
        table.default_subpartition_count,
    );
    insert_optional_string(
        properties,
        "default_partition_tablespace",
        table.default_tablespace.as_deref(),
    );
    insert_optional_string(properties, "partition_interval", table.interval.as_deref());
    insert_optional_string(properties, "autolist", table.autolist.as_deref());
    insert_optional_string(
        properties,
        "subpartition_interval",
        table.interval_subpartition.as_deref(),
    );
    insert_optional_string(
        properties,
        "subpartition_autolist",
        table.autolist_subpartition.as_deref(),
    );
    insert_optional_string(properties, "automatic", table.automatic.as_deref());
    properties.insert(
        "partition_key_columns".to_owned(),
        MetadataValue::StringList(oracle_partition_key_names(
            key_columns,
            &table.owner,
            &table.table,
            "TABLE",
            false,
        )),
    );
    properties.insert(
        "subpartition_key_columns".to_owned(),
        MetadataValue::StringList(oracle_partition_key_names(
            key_columns,
            &table.owner,
            &table.table,
            "TABLE",
            true,
        )),
    );
    let collated =
        oracle_partition_collated_columns(key_columns, &table.owner, &table.table, "TABLE");
    if !collated.is_empty() {
        properties.insert(
            "collated_partition_key_columns".to_owned(),
            MetadataValue::StringList(collated),
        );
    }
}

fn add_oracle_partitioned_index_properties(
    properties: &mut BTreeMap<String, MetadataValue>,
    index: &RawPartitionedIndex,
    key_columns: &[RawPartitionKeyColumn],
) {
    insert_bool(properties, "partitioned", true);
    insert_string(properties, "partitioning_type", &index.partitioning_type);
    insert_string(
        properties,
        "subpartitioning_type",
        &index.subpartitioning_type,
    );
    insert_i64(properties, "partition_count", index.partition_count);
    insert_i64(
        properties,
        "default_subpartition_count",
        index.default_subpartition_count,
    );
    insert_string(properties, "locality", &index.locality);
    insert_string(properties, "alignment", &index.alignment);
    insert_optional_string(
        properties,
        "default_partition_tablespace",
        index.default_tablespace.as_deref(),
    );
    insert_optional_string(properties, "partition_interval", index.interval.as_deref());
    insert_optional_string(properties, "autolist", index.autolist.as_deref());
    insert_optional_string(
        properties,
        "subpartition_interval",
        index.interval_subpartition.as_deref(),
    );
    insert_optional_string(
        properties,
        "subpartition_autolist",
        index.autolist_subpartition.as_deref(),
    );
    properties.insert(
        "partition_key_columns".to_owned(),
        MetadataValue::StringList(oracle_partition_key_names(
            key_columns,
            &index.owner,
            &index.index,
            "INDEX",
            false,
        )),
    );
    properties.insert(
        "subpartition_key_columns".to_owned(),
        MetadataValue::StringList(oracle_partition_key_names(
            key_columns,
            &index.owner,
            &index.index,
            "INDEX",
            true,
        )),
    );
    let collated =
        oracle_partition_collated_columns(key_columns, &index.owner, &index.index, "INDEX");
    if !collated.is_empty() {
        properties.insert(
            "collated_partition_key_columns".to_owned(),
            MetadataValue::StringList(collated),
        );
    }
}

fn oracle_partition_key_names(
    key_columns: &[RawPartitionKeyColumn],
    owner: &str,
    name: &str,
    object_type: &str,
    subpartition: bool,
) -> Vec<String> {
    key_columns
        .iter()
        .filter(|column| {
            column.owner == owner
                && column.name == name
                && column.object_type == object_type
                && column.subpartition == subpartition
        })
        .map(|column| column.column.clone())
        .collect()
}

fn oracle_partition_collated_columns(
    key_columns: &[RawPartitionKeyColumn],
    owner: &str,
    name: &str,
    object_type: &str,
) -> Vec<String> {
    key_columns
        .iter()
        .filter(|column| {
            column.owner == owner && column.name == name && column.object_type == object_type
        })
        .filter_map(|column| Some(format!("{}={}", column.column, column.collated_column_id?)))
        .collect()
}

fn oracle_table_partition_properties(
    partition: &RawTablePartition,
    inventory: &RawInventoryObject,
) -> BTreeMap<String, MetadataValue> {
    let mut properties = inventory_properties(inventory);
    insert_i64(&mut properties, "position", partition.position);
    insert_bool(&mut properties, "composite", partition.composite == "YES");
    insert_i64(
        &mut properties,
        "subpartition_count",
        partition.subpartition_count,
    );
    insert_i64(
        &mut properties,
        "high_value_length",
        partition.high_value_length,
    );
    insert_optional_string(
        &mut properties,
        "tablespace",
        partition.tablespace.as_deref(),
    );
    insert_string(&mut properties, "compression", &partition.compression);
    insert_optional_string(
        &mut properties,
        "compress_for",
        partition.compress_for.as_deref(),
    );
    insert_string(&mut properties, "interval", &partition.interval);
    insert_string(
        &mut properties,
        "segment_created",
        &partition.segment_created,
    );
    insert_string(&mut properties, "indexing", &partition.indexing);
    insert_string(&mut properties, "read_only", &partition.read_only);
    properties
}

fn oracle_table_subpartition_properties(
    subpartition: &RawTableSubpartition,
    inventory: &RawInventoryObject,
) -> BTreeMap<String, MetadataValue> {
    let mut properties = inventory_properties(inventory);
    insert_string(&mut properties, "partition", &subpartition.partition);
    insert_i64(
        &mut properties,
        "partition_position",
        subpartition.partition_position,
    );
    insert_i64(&mut properties, "position", subpartition.position);
    insert_i64(
        &mut properties,
        "high_value_length",
        subpartition.high_value_length,
    );
    insert_optional_string(
        &mut properties,
        "tablespace",
        subpartition.tablespace.as_deref(),
    );
    insert_string(&mut properties, "compression", &subpartition.compression);
    insert_optional_string(
        &mut properties,
        "compress_for",
        subpartition.compress_for.as_deref(),
    );
    insert_string(&mut properties, "interval", &subpartition.interval);
    insert_string(
        &mut properties,
        "segment_created",
        &subpartition.segment_created,
    );
    insert_string(&mut properties, "indexing", &subpartition.indexing);
    insert_string(&mut properties, "read_only", &subpartition.read_only);
    properties
}

fn oracle_index_partition_properties(
    partition: &RawIndexPartition,
    inventory: &RawInventoryObject,
) -> BTreeMap<String, MetadataValue> {
    let mut properties = inventory_properties(inventory);
    insert_i64(&mut properties, "position", partition.position);
    insert_bool(&mut properties, "composite", partition.composite == "YES");
    insert_i64(
        &mut properties,
        "subpartition_count",
        partition.subpartition_count,
    );
    insert_i64(
        &mut properties,
        "high_value_length",
        partition.high_value_length,
    );
    insert_string(&mut properties, "partition_status", &partition.status);
    insert_optional_string(
        &mut properties,
        "tablespace",
        partition.tablespace.as_deref(),
    );
    insert_string(&mut properties, "compression", &partition.compression);
    insert_string(&mut properties, "interval", &partition.interval);
    insert_string(
        &mut properties,
        "segment_created",
        &partition.segment_created,
    );
    properties
}

fn oracle_index_subpartition_properties(
    subpartition: &RawIndexSubpartition,
    inventory: &RawInventoryObject,
) -> BTreeMap<String, MetadataValue> {
    let mut properties = inventory_properties(inventory);
    insert_string(&mut properties, "partition", &subpartition.partition);
    insert_i64(
        &mut properties,
        "partition_position",
        subpartition.partition_position,
    );
    insert_i64(&mut properties, "position", subpartition.position);
    insert_i64(
        &mut properties,
        "high_value_length",
        subpartition.high_value_length,
    );
    insert_string(&mut properties, "partition_status", &subpartition.status);
    insert_optional_string(
        &mut properties,
        "tablespace",
        subpartition.tablespace.as_deref(),
    );
    insert_string(&mut properties, "compression", &subpartition.compression);
    insert_string(&mut properties, "interval", &subpartition.interval);
    insert_string(
        &mut properties,
        "segment_created",
        &subpartition.segment_created,
    );
    properties
}

fn oracle_lob_properties(
    lob: &RawLob,
    segment_inventory: &RawInventoryObject,
    index_inventory: &RawInventoryObject,
) -> BTreeMap<String, MetadataValue> {
    let mut properties = inventory_properties(segment_inventory);
    insert_string(&mut properties, "column", &lob.column);
    insert_string(&mut properties, "segment_name", &lob.segment_name);
    insert_string(&mut properties, "index_name", &lob.index_name);
    insert_optional_string(&mut properties, "tablespace", lob.tablespace.as_deref());
    insert_i64(&mut properties, "chunk", lob.chunk);
    insert_optional_i64(&mut properties, "pctversion", lob.pctversion);
    insert_optional_i64(&mut properties, "retention", lob.retention);
    insert_optional_i64(&mut properties, "freepools", lob.freepools);
    insert_string(&mut properties, "cache", &lob.cache);
    insert_string(&mut properties, "logging", &lob.logging);
    insert_string(&mut properties, "encrypt", &lob.encrypt);
    insert_string(&mut properties, "compression", &lob.compression);
    insert_string(&mut properties, "deduplication", &lob.deduplication);
    insert_string(&mut properties, "in_row", &lob.in_row);
    insert_string(&mut properties, "format", &lob.format);
    insert_bool(&mut properties, "partitioned", lob.partitioned == "YES");
    insert_bool(&mut properties, "securefile", lob.securefile == "YES");
    insert_string(&mut properties, "segment_created", &lob.segment_created);
    insert_optional_string(
        &mut properties,
        "retention_type",
        lob.retention_type.as_deref(),
    );
    insert_optional_i64(&mut properties, "retention_value", lob.retention_value);
    insert_optional_string(&mut properties, "value_based", lob.value_based.as_deref());
    insert_optional_i64(&mut properties, "max_inline", lob.max_inline);
    add_oracle_lob_index_inventory_properties(&mut properties, index_inventory);
    properties
}

fn oracle_lob_partition_properties(
    partition: &RawLobPartition,
    segment_inventory: &RawInventoryObject,
    index_inventory: &RawInventoryObject,
) -> BTreeMap<String, MetadataValue> {
    let mut properties = inventory_properties(segment_inventory);
    insert_string(
        &mut properties,
        "table_partition",
        &partition.table_partition,
    );
    insert_string(&mut properties, "lob_name", &partition.lob_name);
    insert_string(
        &mut properties,
        "lob_index_partition_name",
        &partition.index_partition_name,
    );
    insert_i64(&mut properties, "position", partition.position);
    insert_bool(&mut properties, "composite", partition.composite == "YES");
    insert_i64(&mut properties, "chunk", partition.chunk);
    insert_optional_i64(&mut properties, "pctversion", partition.pctversion);
    insert_string(&mut properties, "cache", &partition.cache);
    insert_string(&mut properties, "in_row", &partition.in_row);
    insert_optional_string(
        &mut properties,
        "tablespace",
        partition.tablespace.as_deref(),
    );
    insert_optional_string(&mut properties, "retention", partition.retention.as_deref());
    insert_string(&mut properties, "logging", &partition.logging);
    insert_string(&mut properties, "encrypt", &partition.encrypt);
    insert_string(&mut properties, "compression", &partition.compression);
    insert_string(&mut properties, "deduplication", &partition.deduplication);
    insert_string(&mut properties, "securefile", &partition.securefile);
    insert_string(
        &mut properties,
        "segment_created",
        &partition.segment_created,
    );
    insert_optional_i64(&mut properties, "max_inline", partition.max_inline);
    add_oracle_lob_index_inventory_properties(&mut properties, index_inventory);
    properties
}

fn oracle_lob_subpartition_properties(
    subpartition: &RawLobSubpartition,
    segment_inventory: &RawInventoryObject,
    index_inventory: &RawInventoryObject,
) -> BTreeMap<String, MetadataValue> {
    let mut properties = inventory_properties(segment_inventory);
    insert_string(
        &mut properties,
        "lob_partition_name",
        &subpartition.lob_partition_name,
    );
    insert_string(
        &mut properties,
        "table_subpartition",
        &subpartition.table_subpartition,
    );
    insert_string(
        &mut properties,
        "lob_index_subpartition_name",
        &subpartition.index_subpartition_name,
    );
    insert_i64(&mut properties, "position", subpartition.position);
    insert_i64(&mut properties, "chunk", subpartition.chunk);
    insert_optional_i64(&mut properties, "pctversion", subpartition.pctversion);
    insert_string(&mut properties, "cache", &subpartition.cache);
    insert_string(&mut properties, "in_row", &subpartition.in_row);
    insert_optional_string(
        &mut properties,
        "tablespace",
        subpartition.tablespace.as_deref(),
    );
    insert_optional_string(
        &mut properties,
        "retention",
        subpartition.retention.as_deref(),
    );
    insert_string(&mut properties, "logging", &subpartition.logging);
    insert_string(&mut properties, "encrypt", &subpartition.encrypt);
    insert_string(&mut properties, "compression", &subpartition.compression);
    insert_string(
        &mut properties,
        "deduplication",
        &subpartition.deduplication,
    );
    insert_string(&mut properties, "securefile", &subpartition.securefile);
    insert_string(
        &mut properties,
        "segment_created",
        &subpartition.segment_created,
    );
    insert_optional_i64(&mut properties, "max_inline", subpartition.max_inline);
    add_oracle_lob_index_inventory_properties(&mut properties, index_inventory);
    properties
}

fn add_oracle_lob_index_inventory_properties(
    properties: &mut BTreeMap<String, MetadataValue>,
    inventory: &RawInventoryObject,
) {
    insert_i64(properties, "lob_index_object_id", inventory.object_id);
    insert_optional_i64(
        properties,
        "lob_index_data_object_id",
        inventory.data_object_id,
    );
    insert_string(properties, "lob_index_status", &inventory.status);
    insert_bool(properties, "lob_index_generated", inventory.generated);
}

fn oracle_trigger_definition(trigger: &RawTrigger) -> Result<String, CatalogError> {
    let description = trigger.description.as_deref().ok_or_else(|| {
        CatalogError::Mapping(format!(
            "Oracle trigger {}.{} has no complete description",
            trigger.owner, trigger.name
        ))
    })?;
    let body = trigger.body.as_deref().ok_or_else(|| {
        CatalogError::Mapping(format!(
            "Oracle trigger {}.{} has no complete body",
            trigger.owner, trigger.name
        ))
    })?;
    let definition = format!("CREATE OR REPLACE TRIGGER {description}\n{body}");
    if definition.len() > MAX_DEFINITION_BYTES {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "Oracle trigger definition exceeds the {MAX_DEFINITION_BYTES}-byte safety limit for {}.{}",
            trigger.owner, trigger.name
        )));
    }
    Ok(definition)
}

fn oracle_trigger_properties(
    trigger: &RawTrigger,
    inventory_object: &RawInventoryObject,
) -> BTreeMap<String, MetadataValue> {
    let mut properties = inventory_properties(inventory_object);
    insert_string(&mut properties, "trigger_type", &trigger.trigger_type);
    insert_string(
        &mut properties,
        "triggering_event",
        &trigger.triggering_event,
    );
    insert_optional_string(
        &mut properties,
        "table_owner",
        trigger.table_owner.as_deref(),
    );
    insert_string(
        &mut properties,
        "base_object_type",
        &trigger.base_object_type,
    );
    insert_optional_string(&mut properties, "table_name", trigger.table_name.as_deref());
    insert_optional_string(
        &mut properties,
        "column_name",
        trigger.column_name.as_deref(),
    );
    insert_optional_string(
        &mut properties,
        "referencing_names",
        trigger.referencing_names.as_deref(),
    );
    insert_optional_string(
        &mut properties,
        "when_clause",
        trigger.when_clause.as_deref(),
    );
    insert_string(&mut properties, "status", &trigger.status);
    insert_string(&mut properties, "action_type", &trigger.action_type);
    insert_optional_string(
        &mut properties,
        "crossedition",
        trigger.crossedition.as_deref(),
    );
    insert_optional_string(&mut properties, "fire_once", trigger.fire_once.as_deref());
    insert_optional_string(
        &mut properties,
        "apply_server_only",
        trigger.apply_server_only.as_deref(),
    );
    properties
}

fn oracle_routine_properties(
    routine: &RawRoutine,
    inventory_object: &RawInventoryObject,
) -> BTreeMap<String, MetadataValue> {
    let mut properties = inventory_properties(inventory_object);
    insert_i64(&mut properties, "object_id", routine.object_id);
    insert_i64(&mut properties, "subprogram_id", routine.subprogram_id);
    insert_optional_string(&mut properties, "overload", routine.overload.as_deref());
    insert_string(&mut properties, "object_type", &routine.object_type);
    insert_bool(&mut properties, "aggregate", routine.aggregate);
    insert_bool(&mut properties, "pipelined", routine.pipelined);
    insert_bool(&mut properties, "parallel", routine.parallel);
    insert_bool(&mut properties, "interface", routine.interface);
    insert_bool(&mut properties, "deterministic", routine.deterministic);
    insert_string(&mut properties, "authid", &routine.authid);
    insert_optional_string(
        &mut properties,
        "polymorphic",
        routine.polymorphic.as_deref(),
    );
    properties
}

fn oracle_routine_argument_properties(
    argument: &RawRoutineArgument,
) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_i64(&mut properties, "position", argument.position);
    insert_i64(&mut properties, "sequence", argument.sequence);
    insert_i64(&mut properties, "data_level", argument.data_level);
    insert_string(
        &mut properties,
        "data_type",
        format_oracle_argument_type(argument),
    );
    insert_string(&mut properties, "mode", &argument.mode);
    insert_bool(&mut properties, "defaulted", argument.defaulted);
    insert_optional_i64(&mut properties, "default_length", argument.default_length);
    insert_optional_string(
        &mut properties,
        "default_value",
        argument.default_value.as_deref(),
    );
    insert_optional_i64(&mut properties, "data_length", argument.data_length);
    insert_optional_i64(&mut properties, "data_precision", argument.data_precision);
    insert_optional_i64(&mut properties, "data_scale", argument.data_scale);
    insert_optional_string(&mut properties, "pls_type", argument.pls_type.as_deref());
    insert_optional_i64(&mut properties, "char_length", argument.char_length);
    insert_optional_string(&mut properties, "char_used", argument.char_used.as_deref());
    properties
}

fn validate_package_argument_order(
    routine: &RawPackageRoutine,
    arguments: &[&RawRoutineArgument],
) -> Result<(), CatalogError> {
    let return_count = arguments
        .iter()
        .filter(|argument| argument.position == 0)
        .count();
    if return_count > 1 {
        return Err(CatalogError::Mapping(format!(
            "Oracle package routine {}.{}.{} has {return_count} return rows",
            routine.owner, routine.package, routine.name
        )));
    }
    for (offset, argument) in arguments.iter().enumerate() {
        let expected_sequence = i64::try_from(offset + 1)
            .map_err(|_| CatalogError::Mapping("too many Oracle package arguments".to_owned()))?;
        if argument.sequence != expected_sequence {
            return Err(CatalogError::Mapping(format!(
                "Oracle package argument sequence gap for {}.{}.{}: expected {expected_sequence}, found {}",
                routine.owner, routine.package, routine.name, argument.sequence
            )));
        }
        let expected_position = if return_count == 1 {
            i64::try_from(offset).map_err(|_| {
                CatalogError::Mapping("too many Oracle package arguments".to_owned())
            })?
        } else {
            expected_sequence
        };
        if argument.position != expected_position {
            return Err(CatalogError::Mapping(format!(
                "Oracle package argument position mismatch for {}.{}.{}: expected {expected_position}, found {}",
                routine.owner, routine.package, routine.name, argument.position
            )));
        }
        if argument.position == 0 && (argument.name.is_some() || argument.mode != "OUT") {
            return Err(CatalogError::Mapping(format!(
                "Oracle package function return metadata is malformed for {}.{}.{}",
                routine.owner, routine.package, routine.name
            )));
        }
    }
    Ok(())
}

fn oracle_package_definition(package: &RawPackage) -> Result<String, CatalogError> {
    let specification = package.specification.as_deref().ok_or_else(|| {
        CatalogError::Mapping(format!(
            "Oracle package {}.{} has no specification",
            package.owner, package.name
        ))
    })?;
    let definition = package
        .body
        .as_deref()
        .map(|body| format!("{specification}\n\n{body}"))
        .unwrap_or_else(|| specification.to_owned());
    if definition.len() > MAX_DEFINITION_BYTES {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "combined Oracle package definition exceeds the {MAX_DEFINITION_BYTES}-byte safety limit for {}.{}",
            package.owner, package.name
        )));
    }
    Ok(definition)
}

fn oracle_type_definition(user_type: &RawUserType) -> Result<String, CatalogError> {
    let specification = user_type.specification.as_deref().ok_or_else(|| {
        CatalogError::Mapping(format!(
            "Oracle type {}.{} has no specification",
            user_type.owner, user_type.name
        ))
    })?;
    let definition = user_type
        .body
        .as_deref()
        .map(|body| format!("{specification}\n\n{body}"))
        .unwrap_or_else(|| specification.to_owned());
    if definition.len() > MAX_DEFINITION_BYTES {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "combined Oracle type definition exceeds the {MAX_DEFINITION_BYTES}-byte safety limit for {}.{}",
            user_type.owner, user_type.name
        )));
    }
    Ok(definition)
}

fn oracle_type_properties(
    user_type: &RawUserType,
    inventory_object: &RawInventoryObject,
    body_inventory: Option<&RawInventoryObject>,
    collection: Option<&RawCollectionType>,
) -> BTreeMap<String, MetadataValue> {
    let mut properties = inventory_properties(inventory_object);
    insert_string(&mut properties, "type_oid", &user_type.oid);
    insert_string(&mut properties, "typecode", &user_type.typecode);
    insert_i64(
        &mut properties,
        "attribute_count",
        user_type.attribute_count,
    );
    insert_i64(&mut properties, "method_count", user_type.method_count);
    insert_string(&mut properties, "predefined", &user_type.predefined);
    insert_string(&mut properties, "incomplete", &user_type.incomplete);
    insert_string(&mut properties, "final", &user_type.final_type);
    insert_string(&mut properties, "instantiable", &user_type.instantiable);
    insert_string(&mut properties, "persistable", &user_type.persistable);
    insert_optional_string(
        &mut properties,
        "supertype_owner",
        user_type.supertype_owner.as_deref(),
    );
    insert_optional_string(
        &mut properties,
        "supertype_name",
        user_type.supertype_name.as_deref(),
    );
    insert_optional_i64(
        &mut properties,
        "local_attribute_count",
        user_type.local_attribute_count,
    );
    insert_optional_i64(
        &mut properties,
        "local_method_count",
        user_type.local_method_count,
    );
    insert_optional_string(&mut properties, "type_id", user_type.type_id.as_deref());
    insert_bool(&mut properties, "has_body", user_type.body.is_some());
    if let Some(body_inventory) = body_inventory {
        insert_i64(&mut properties, "body_object_id", body_inventory.object_id);
        insert_string(&mut properties, "body_status", &body_inventory.status);
    }
    if let Some(collection) = collection {
        insert_string(
            &mut properties,
            "collection_type",
            &collection.collection_type,
        );
        insert_optional_i64(&mut properties, "upper_bound", collection.upper_bound);
        insert_optional_string(
            &mut properties,
            "element_type_modifier",
            collection.element_type_modifier.as_deref(),
        );
        insert_optional_string(
            &mut properties,
            "element_type_owner",
            collection.element_type_owner.as_deref(),
        );
        insert_string(
            &mut properties,
            "element_type_name",
            &collection.element_type_name,
        );
        insert_optional_i64(&mut properties, "element_length", collection.length);
        insert_optional_i64(&mut properties, "element_precision", collection.precision);
        insert_optional_i64(&mut properties, "element_scale", collection.scale);
        insert_optional_string(
            &mut properties,
            "element_character_set",
            collection.character_set.as_deref(),
        );
        insert_optional_string(
            &mut properties,
            "element_storage",
            collection.element_storage.as_deref(),
        );
        insert_optional_string(
            &mut properties,
            "nulls_stored",
            collection.nulls_stored.as_deref(),
        );
        insert_optional_string(
            &mut properties,
            "element_char_used",
            collection.char_used.as_deref(),
        );
    }
    properties
}

fn oracle_type_attribute_properties(
    attribute: &RawTypeAttribute,
) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_i64(&mut properties, "position", attribute.position);
    insert_string(&mut properties, "data_type", &attribute.data_type_name);
    insert_optional_string(
        &mut properties,
        "type_modifier",
        attribute.type_modifier.as_deref(),
    );
    insert_optional_string(
        &mut properties,
        "data_type_owner",
        attribute.data_type_owner.as_deref(),
    );
    insert_optional_i64(&mut properties, "length", attribute.length);
    insert_optional_i64(&mut properties, "precision", attribute.precision);
    insert_optional_i64(&mut properties, "scale", attribute.scale);
    insert_optional_string(
        &mut properties,
        "character_set",
        attribute.character_set.as_deref(),
    );
    insert_bool(&mut properties, "inherited", attribute.inherited == "YES");
    insert_optional_string(&mut properties, "char_used", attribute.char_used.as_deref());
    properties
}

fn oracle_type_method_properties(method: &RawTypeMethod) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_i64(&mut properties, "method_number", method.method_number);
    insert_string(&mut properties, "method_type", &method.method_type);
    insert_i64(&mut properties, "parameter_count", method.parameter_count);
    insert_i64(&mut properties, "result_count", method.result_count);
    insert_bool(&mut properties, "final", method.final_method == "YES");
    insert_bool(
        &mut properties,
        "instantiable",
        method.instantiable == "YES",
    );
    insert_bool(&mut properties, "overriding", method.overriding == "YES");
    insert_bool(&mut properties, "inherited", method.inherited == "YES");
    properties
}

fn oracle_type_method_parameter_properties(
    parameter: &RawTypeMethodParameter,
) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_i64(&mut properties, "position", parameter.position);
    insert_string(&mut properties, "mode", &parameter.mode);
    insert_string(&mut properties, "data_type", &parameter.data_type_name);
    insert_optional_string(
        &mut properties,
        "type_modifier",
        parameter.type_modifier.as_deref(),
    );
    insert_optional_string(
        &mut properties,
        "data_type_owner",
        parameter.data_type_owner.as_deref(),
    );
    insert_optional_string(
        &mut properties,
        "character_set",
        parameter.character_set.as_deref(),
    );
    insert_bool(&mut properties, "return_value", parameter.return_value);
    properties
}

fn oracle_package_properties(
    package: &RawPackage,
    inventory_object: &RawInventoryObject,
    body_inventory: Option<&RawInventoryObject>,
) -> BTreeMap<String, MetadataValue> {
    let mut properties = inventory_properties(inventory_object);
    insert_string(&mut properties, "authid", &package.authid);
    insert_bool(&mut properties, "has_body", package.body.is_some());
    insert_i64(
        &mut properties,
        "specification_bytes",
        package
            .specification
            .as_ref()
            .map_or(0, |definition| definition.len()) as i64,
    );
    insert_i64(
        &mut properties,
        "body_bytes",
        package
            .body
            .as_ref()
            .map_or(0, |definition| definition.len()) as i64,
    );
    if let Some(body) = body_inventory {
        insert_i64(&mut properties, "body_object_id", body.object_id);
        insert_optional_i64(&mut properties, "body_data_object_id", body.data_object_id);
        insert_string(&mut properties, "body_status", &body.status);
        insert_bool(&mut properties, "body_generated", body.generated);
    }
    properties
}

fn oracle_package_routine_signature(
    routine: &RawPackageRoutine,
    arguments: &[&RawRoutineArgument],
) -> Result<String, CatalogError> {
    let parameters = arguments
        .iter()
        .filter(|argument| argument.position > 0)
        .map(|argument| {
            format!(
                "{} {}",
                argument.mode,
                format_oracle_argument_type(argument)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let return_type = arguments
        .iter()
        .find(|argument| argument.position == 0)
        .map(|argument| format!("->{}", format_oracle_argument_type(argument)))
        .unwrap_or_default();
    let signature = format!("{}({parameters}){return_type}", routine.name);
    if signature.len() > MAX_ROUTINE_SIGNATURE_BYTES {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "Oracle package routine signature exceeds {MAX_ROUTINE_SIGNATURE_BYTES} bytes for {}.{}.{}",
            routine.owner, routine.package, routine.name
        )));
    }
    Ok(signature)
}

fn oracle_package_routine_properties(
    routine: &RawPackageRoutine,
    signature: &str,
) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_i64(&mut properties, "object_id", routine.object_id);
    insert_i64(&mut properties, "subprogram_id", routine.subprogram_id);
    insert_optional_string(&mut properties, "overload", routine.overload.as_deref());
    insert_string(&mut properties, "signature", signature);
    insert_bool(&mut properties, "aggregate", routine.aggregate);
    insert_bool(&mut properties, "pipelined", routine.pipelined);
    insert_bool(&mut properties, "parallel", routine.parallel);
    insert_bool(&mut properties, "interface", routine.interface);
    insert_bool(&mut properties, "deterministic", routine.deterministic);
    insert_string(&mut properties, "authid", &routine.authid);
    insert_optional_string(
        &mut properties,
        "polymorphic",
        routine.polymorphic.as_deref(),
    );
    properties
}

fn format_oracle_argument_type(argument: &RawRoutineArgument) -> String {
    let data_type = argument
        .data_type
        .as_deref()
        .unwrap_or("UNSPECIFIED")
        .to_owned();
    match data_type.as_str() {
        "NUMBER" => match (argument.data_precision, argument.data_scale) {
            (Some(precision), Some(scale)) => format!("{data_type}({precision},{scale})"),
            (Some(precision), None) => format!("{data_type}({precision})"),
            _ => data_type,
        },
        "CHAR" | "VARCHAR2" | "NCHAR" | "NVARCHAR2" => argument
            .char_length
            .map(|length| {
                let unit = match argument.char_used.as_deref() {
                    Some("C") => " CHAR",
                    Some("B") => " BYTE",
                    _ => "",
                };
                format!("{data_type}({length}{unit})")
            })
            .unwrap_or(data_type),
        _ => data_type,
    }
}

fn constraint_properties(constraint: &RawConstraint) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_string(&mut properties, "status", &constraint.status);
    insert_string(&mut properties, "deferrable", &constraint.deferrable);
    insert_string(&mut properties, "deferred", &constraint.deferred);
    insert_string(&mut properties, "validated", &constraint.validated);
    insert_string(&mut properties, "generated", &constraint.generated);
    insert_optional_string(
        &mut properties,
        "delete_rule",
        constraint.delete_rule.as_deref(),
    );
    insert_optional_string(
        &mut properties,
        "index_owner",
        constraint.index_owner.as_deref(),
    );
    insert_optional_string(
        &mut properties,
        "index_name",
        constraint.index_name.as_deref(),
    );
    insert_optional_string(&mut properties, "invalid", constraint.invalid.as_deref());
    insert_optional_string(
        &mut properties,
        "view_related",
        constraint.view_related.as_deref(),
    );
    properties
}

fn format_oracle_data_type(column: &RawColumn) -> String {
    let type_name = column
        .data_type_owner
        .as_deref()
        .map(|owner| format!("{owner}.{}", column.data_type))
        .unwrap_or_else(|| column.data_type.clone());
    match column.data_type.as_str() {
        "NUMBER" => match (column.data_precision, column.data_scale) {
            (Some(precision), Some(scale)) => format!("{type_name}({precision},{scale})"),
            (Some(precision), None) => format!("{type_name}({precision})"),
            _ => type_name,
        },
        "FLOAT" => column
            .data_precision
            .map(|precision| format!("{type_name}({precision})"))
            .unwrap_or(type_name),
        "CHAR" | "VARCHAR2" | "NCHAR" | "NVARCHAR2" => {
            let unit = match column.char_used.as_deref() {
                Some("C") => " CHAR",
                Some("B") => " BYTE",
                _ => "",
            };
            format!(
                "{type_name}({}{unit})",
                column.char_length.unwrap_or(column.data_length)
            )
        }
        "RAW" | "UROWID" => format!("{type_name}({})", column.data_length),
        _ => type_name,
    }
}

fn oracle_complete_capabilities(scope: &DictionaryScope) -> AdapterCapabilities {
    AdapterCapabilities {
        source_kind: ORACLE_SOURCE.to_owned(),
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
        notes: vec![format!(
            "{}; unsupported Oracle object shapes fail the analysis instead of producing a partial snapshot",
            scope.mode.label()
        )],
    }
}

fn discovery_counts_from_catalog(
    raw: &RawOracleCatalog,
    scope: &DictionaryScope,
) -> DiscoveryCounts {
    let object_evidence =
        "Oracle USER/DBA dictionary inventory after explicit application-scope filtering";
    let relationship_evidence =
        "Oracle USER/DBA dictionary parent and ordered-column reconciliation";
    let mut objects = ObjectCategory::ALL
        .into_iter()
        .map(|category| {
            (
                category,
                DiscoveredCount {
                    count: 0,
                    evidence: object_evidence.to_owned(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut relationships = RelationshipCategory::ALL
        .into_iter()
        .map(|category| {
            (
                category,
                DiscoveredCount {
                    count: 0,
                    evidence: relationship_evidence.to_owned(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let materialized_view_names = raw
        .materialized_views
        .iter()
        .map(|view| (view.owner.as_str(), view.name.as_str()))
        .collect::<BTreeSet<_>>();
    let base_table_count = raw
        .tables
        .iter()
        .filter(|table| {
            !materialized_view_names.contains(&(table.owner.as_str(), table.name.as_str()))
        })
        .count();
    let base_column_count = raw
        .columns
        .iter()
        .filter(|column| {
            !materialized_view_names.contains(&(column.owner.as_str(), column.table.as_str()))
        })
        .count();
    let materialized_view_column_count = raw.columns.len() - base_column_count;
    let base_constraint_count = raw
        .constraints
        .iter()
        .filter(|constraint| {
            !materialized_view_names
                .contains(&(constraint.owner.as_str(), constraint.table.as_str()))
        })
        .count();
    let materialized_view_constraint_count = raw.constraints.len() - base_constraint_count;
    let base_index_count = raw
        .indexes
        .iter()
        .filter(|index| {
            !materialized_view_names.contains(&(index.table_owner.as_str(), index.table.as_str()))
        })
        .count();
    let materialized_view_index_count = raw.indexes.len() - base_index_count;
    let materialized_view_dependency_count = raw
        .dependencies
        .iter()
        .filter(|dependency| dependency.object_type == "MATERIALIZED VIEW")
        .filter(|dependency| !dependency.referenced_owner_oracle_maintained)
        .filter(|dependency| {
            !(dependency.referenced_type == "TABLE"
                && dependency.owner == dependency.referenced_owner
                && dependency.name == dependency.referenced_name)
        })
        .count();
    let trigger_targets = raw
        .triggers
        .iter()
        .filter_map(|trigger| {
            if !matches!(trigger.base_object_type.as_str(), "TABLE" | "VIEW") {
                return None;
            }
            Some((
                (trigger.owner.as_str(), trigger.name.as_str()),
                (
                    trigger.table_owner.as_deref()?,
                    trigger.table_name.as_deref()?,
                    trigger.base_object_type.as_str(),
                ),
            ))
        })
        .collect::<BTreeMap<_, _>>();
    let trigger_dependency_count = raw
        .dependencies
        .iter()
        .filter(|dependency| dependency.object_type == "TRIGGER")
        .filter(|dependency| !dependency.referenced_owner_oracle_maintained)
        .filter(|dependency| {
            trigger_targets
                .get(&(dependency.owner.as_str(), dependency.name.as_str()))
                .is_none_or(|target| {
                    !(dependency.referenced_owner == target.0
                        && dependency.referenced_name == target.1
                        && dependency.referenced_type == target.2)
                })
        })
        .count();
    let routine_dependency_count = raw
        .dependencies
        .iter()
        .filter(|dependency| matches!(dependency.object_type.as_str(), "FUNCTION" | "PROCEDURE"))
        .filter(|dependency| !dependency.referenced_owner_oracle_maintained)
        .filter(|dependency| dependency.referenced_type != "TYPE")
        .count();
    let metadata_only_type_dependency_count = raw
        .dependencies
        .iter()
        .filter(|dependency| {
            matches!(
                dependency.object_type.as_str(),
                "VIEW" | "FUNCTION" | "PROCEDURE"
            ) && dependency.referenced_type == "TYPE"
        })
        .filter(|dependency| !dependency.referenced_owner_oracle_maintained)
        .count();
    let package_dependency_count = oracle_package_dependency_groups(&raw.dependencies).len();
    let type_dependency_count = oracle_type_dependency_groups(&raw.dependencies).len();
    let synonym_dependency_count = raw
        .dependencies
        .iter()
        .filter(|dependency| dependency.object_type == "SYNONYM")
        .filter(|dependency| !dependency.referenced_owner_oracle_maintained)
        .count();
    let type_reference_count = raw
        .type_attributes
        .iter()
        .filter(|attribute| attribute.data_type_owner.is_some())
        .count()
        + raw
            .collection_types
            .iter()
            .filter(|collection| collection.element_type_owner.is_some())
            .count()
        + raw
            .type_method_parameters
            .iter()
            .filter(|parameter| parameter.data_type_owner.is_some())
            .count()
        + raw
            .columns
            .iter()
            .filter(|column| column.data_type_owner.is_some())
            .count()
        + raw
            .view_columns
            .iter()
            .filter(|column| column.data_type_owner.is_some())
            .count()
        + raw
            .routine_arguments
            .iter()
            .filter(|argument| argument.type_owner.is_some())
            .count()
        + raw
            .package_arguments
            .iter()
            .filter(|argument| argument.type_owner.is_some())
            .count();
    let type_inheritance_count = raw
        .user_types
        .iter()
        .filter(|user_type| user_type.supertype_owner.is_some())
        .count();

    set_object_count(&mut objects, ObjectCategory::Database, 1);
    set_object_count(&mut objects, ObjectCategory::Schema, scope.owners.len());
    set_object_count(&mut objects, ObjectCategory::Table, base_table_count);
    set_object_count(&mut objects, ObjectCategory::Column, base_column_count);
    set_object_count(&mut objects, ObjectCategory::Index, raw.indexes.len());
    set_object_count(&mut objects, ObjectCategory::Sequence, raw.sequences.len());
    set_object_count(&mut objects, ObjectCategory::View, raw.views.len());
    set_object_count(&mut objects, ObjectCategory::Synonym, raw.synonyms.len());
    set_object_count(
        &mut objects,
        ObjectCategory::UserDefinedType,
        raw.user_types.len(),
    );
    set_object_count(
        &mut objects,
        ObjectCategory::Extension,
        raw.type_attributes.len()
            + raw.table_partitions.len()
            + raw.table_subpartitions.len()
            + raw.index_partitions.len()
            + raw.index_subpartitions.len()
            + raw.lobs.len()
            + raw.lob_partitions.len()
            + raw.lob_subpartitions.len(),
    );
    set_object_count(&mut objects, ObjectCategory::Trigger, raw.triggers.len());
    set_object_count(
        &mut objects,
        ObjectCategory::Routine,
        raw.routines.len() + raw.package_routines.len() + raw.type_methods.len(),
    );
    set_object_count(
        &mut objects,
        ObjectCategory::RoutineParameter,
        raw.routine_arguments.len()
            + raw.package_arguments.len()
            + raw.type_method_parameters.len(),
    );
    set_object_count(&mut objects, ObjectCategory::Package, raw.packages.len());
    set_object_count(
        &mut objects,
        ObjectCategory::ViewColumn,
        raw.view_columns.len() + materialized_view_column_count,
    );
    set_object_count(
        &mut objects,
        ObjectCategory::MaterializedView,
        raw.materialized_views.len(),
    );
    set_object_count(
        &mut objects,
        ObjectCategory::Principal,
        scope.principals.len(),
    );
    for constraint in &raw.constraints {
        let category = match constraint.constraint_type.as_str() {
            "P" => ObjectCategory::PrimaryKey,
            "R" => ObjectCategory::ForeignKey,
            "U" => ObjectCategory::UniqueConstraint,
            "C" => ObjectCategory::CheckConstraint,
            _ => continue,
        };
        objects.entry(category).and_modify(|count| count.count += 1);
    }

    set_relationship_count(
        &mut relationships,
        RelationshipCategory::DatabaseHasSchema,
        scope.owners.len(),
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::SchemaHasTable,
        base_table_count,
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::TableHasColumn,
        base_column_count,
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::TableHasConstraint,
        base_constraint_count,
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::ConstraintColumn,
        raw.constraints
            .iter()
            .filter(|constraint| {
                !materialized_view_names
                    .contains(&(constraint.owner.as_str(), constraint.table.as_str()))
            })
            .filter(|constraint| constraint.constraint_type != "R")
            .map(|constraint| constraint.columns.len())
            .sum(),
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::ForeignKeyColumnPair,
        raw.constraints
            .iter()
            .filter(|constraint| {
                !materialized_view_names
                    .contains(&(constraint.owner.as_str(), constraint.table.as_str()))
            })
            .filter(|constraint| constraint.constraint_type == "R")
            .map(|constraint| constraint.columns.len())
            .sum(),
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::TableHasIndex,
        base_index_count,
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::IndexColumn,
        raw.indexes
            .iter()
            .filter(|index| {
                !materialized_view_names
                    .contains(&(index.table_owner.as_str(), index.table.as_str()))
            })
            .map(|index| {
                index
                    .columns
                    .iter()
                    .filter(|column| column.expression.is_none())
                    .count()
            })
            .sum(),
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::SchemaHasView,
        raw.views.len(),
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::ViewDependency,
        raw.dependencies
            .iter()
            .filter(|dependency| dependency.object_type == "VIEW")
            .filter(|dependency| !dependency.referenced_owner_oracle_maintained)
            .filter(|dependency| dependency.referenced_type != "TYPE")
            .count(),
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::TriggerTarget,
        trigger_targets.len(),
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::SchemaHasRoutine,
        raw.routines.len(),
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::RoutineDependency,
        routine_dependency_count,
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::MetadataParent,
        scope.principals.len()
            + raw.sequences.len()
            + raw.synonyms.len()
            + raw.user_types.len()
            + raw.type_attributes.len()
            + raw.table_partitions.len()
            + raw.table_subpartitions.len()
            + raw.index_partitions.len()
            + raw.index_subpartitions.len()
            + raw.lobs.len()
            + raw.lob_partitions.len()
            + raw.lob_subpartitions.len()
            + raw.type_methods.len()
            + raw.type_method_parameters.len()
            + raw.view_columns.len()
            + raw.materialized_views.len()
            + materialized_view_column_count
            + materialized_view_constraint_count
            + materialized_view_index_count
            + raw.routine_arguments.len()
            + raw.packages.len()
            + raw.package_routines.len()
            + raw.package_arguments.len()
            + raw
                .triggers
                .iter()
                .filter(|trigger| {
                    matches!(trigger.base_object_type.as_str(), "SCHEMA" | "DATABASE")
                })
                .count(),
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::MetadataRelationship,
        raw.identity_columns.len()
            + materialized_view_dependency_count
            + trigger_dependency_count
            + synonym_dependency_count
            + type_dependency_count
            + type_reference_count
            + type_inheritance_count
            + metadata_only_type_dependency_count
            + raw.routine_arguments.len()
            + raw.package_arguments.len()
            + raw.type_method_parameters.len()
            + package_dependency_count
            + raw.lob_partitions.len()
            + raw.lob_subpartitions.len()
            + raw
                .constraints
                .iter()
                .filter(|constraint| {
                    materialized_view_names
                        .contains(&(constraint.owner.as_str(), constraint.table.as_str()))
                })
                .map(|constraint| constraint.columns.len())
                .sum::<usize>()
            + raw
                .indexes
                .iter()
                .filter(|index| {
                    materialized_view_names
                        .contains(&(index.table_owner.as_str(), index.table.as_str()))
                })
                .map(|index| {
                    index
                        .columns
                        .iter()
                        .filter(|column| column.expression.is_none())
                        .count()
                })
                .sum::<usize>(),
    );

    DiscoveryCounts {
        objects,
        relationships,
    }
}

fn set_object_count(
    counts: &mut BTreeMap<ObjectCategory, DiscoveredCount>,
    category: ObjectCategory,
    count: usize,
) {
    counts
        .get_mut(&category)
        .expect("all object categories exist")
        .count = count as u64;
}

fn set_relationship_count(
    counts: &mut BTreeMap<RelationshipCategory, DiscoveredCount>,
    category: RelationshipCategory,
    count: usize,
) {
    counts
        .get_mut(&category)
        .expect("all relationship categories exist")
        .count = count as u64;
}

fn oracle_key(
    connection_alias: &str,
    database: &str,
    schema: &str,
    kind: ObjectKind,
    object_name: &str,
    sub_object: Option<String>,
) -> ObjectKey {
    ObjectKey::new(
        ORACLE_SOURCE,
        connection_alias,
        database,
        schema,
        kind,
        object_name,
        sub_object,
    )
}

fn required<T>(value: Option<&T>, subject: impl Into<String>) -> Result<&T, CatalogError> {
    value.ok_or_else(|| {
        CatalogError::Mapping(format!("missing {subject}", subject = subject.into()))
    })
}

fn positive_u32(value: i64, subject: &str) -> Result<u32, CatalogError> {
    u32::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| CatalogError::Mapping(format!("invalid {subject}: {value}")))
}

fn insert_bool(properties: &mut BTreeMap<String, MetadataValue>, name: &str, value: bool) {
    properties.insert(name.to_owned(), MetadataValue::Boolean(value));
}

fn insert_i64(properties: &mut BTreeMap<String, MetadataValue>, name: &str, value: i64) {
    properties.insert(name.to_owned(), MetadataValue::Integer(value));
}

fn insert_optional_i64(
    properties: &mut BTreeMap<String, MetadataValue>,
    name: &str,
    value: Option<i64>,
) {
    if let Some(value) = value {
        insert_i64(properties, name, value);
    }
}

fn insert_string(
    properties: &mut BTreeMap<String, MetadataValue>,
    name: &str,
    value: impl ToString,
) {
    properties.insert(name.to_owned(), MetadataValue::String(value.to_string()));
}

fn insert_optional_string(
    properties: &mut BTreeMap<String, MetadataValue>,
    name: &str,
    value: Option<&str>,
) {
    if let Some(value) = value {
        insert_string(properties, name, value);
    }
}

