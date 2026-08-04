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
