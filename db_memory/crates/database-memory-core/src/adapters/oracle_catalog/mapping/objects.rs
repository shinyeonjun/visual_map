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
