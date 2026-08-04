fn read_partitioned_tables(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawPartitionedTable>, CatalogError> {
    type Row = (
        String,
        String,
        String,
        String,
        i64,
        i64,
        i64,
        i64,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let mut tables = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let (view, owner_expression, owner_filter) = match scope.mode {
            DictionaryScopeMode::User => ("USER_PART_TABLES", ":1", ""),
            DictionaryScopeMode::Dba => ("DBA_PART_TABLES", "OWNER", "WHERE OWNER = :1"),
        };
        let sql = format!(
            "
            SELECT {owner_expression},
                   TABLE_NAME,
                   PARTITIONING_TYPE,
                   SUBPARTITIONING_TYPE,
                   PARTITION_COUNT,
                   DEF_SUBPARTITION_COUNT,
                   PARTITIONING_KEY_COUNT,
                   SUBPARTITIONING_KEY_COUNT,
                   STATUS,
                   DEF_TABLESPACE_NAME,
                   INTERVAL,
                   AUTOLIST,
                   INTERVAL_SUBPARTITION,
                   AUTOLIST_SUBPARTITION,
                   AUTO
            FROM {view}
            {owner_filter}
            ORDER BY TABLE_NAME
            "
        );
        for row in connection.query_as::<Row>(&sql, &[owner])? {
            let (
                owner,
                table,
                partitioning_type,
                subpartitioning_type,
                partition_count,
                default_subpartition_count,
                partitioning_key_count,
                subpartitioning_key_count,
                status,
                default_tablespace,
                interval,
                autolist,
                interval_subpartition,
                autolist_subpartition,
                automatic,
            ) = row?;
            tables.push(RawPartitionedTable {
                owner,
                table,
                partitioning_type: partitioning_type.trim().to_owned(),
                subpartitioning_type: subpartitioning_type.trim().to_owned(),
                partition_count,
                default_subpartition_count,
                partitioning_key_count,
                subpartitioning_key_count,
                status: status.trim().to_owned(),
                default_tablespace: normalize_optional_token(default_tablespace),
                interval: normalize_definition(interval)?,
                autolist: normalize_optional_token(autolist),
                interval_subpartition: normalize_definition(interval_subpartition)?,
                autolist_subpartition: normalize_optional_token(autolist_subpartition),
                automatic: normalize_optional_token(automatic),
            });
        }
    }
    tables.sort_by(|left, right| (&left.owner, &left.table).cmp(&(&right.owner, &right.table)));
    Ok(tables)
}

fn read_table_partitions(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawTablePartition>, CatalogError> {
    type Row = (
        String,
        String,
        String,
        String,
        i64,
        Option<String>,
        i64,
        i64,
        Option<String>,
        String,
        Option<String>,
        String,
        String,
        String,
        String,
    );
    let mut partitions = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let (view, owner_expression, owner_filter) = match scope.mode {
            DictionaryScopeMode::User => ("USER_TAB_PARTITIONS", ":1", ""),
            DictionaryScopeMode::Dba => (
                "DBA_TAB_PARTITIONS",
                "TABLE_OWNER",
                "WHERE TABLE_OWNER = :1",
            ),
        };
        let sql = format!(
            "
            SELECT {owner_expression},
                   TABLE_NAME,
                   COMPOSITE,
                   PARTITION_NAME,
                   SUBPARTITION_COUNT,
                   HIGH_VALUE_CLOB,
                   HIGH_VALUE_LENGTH,
                   PARTITION_POSITION,
                   TABLESPACE_NAME,
                   COMPRESSION,
                   COMPRESS_FOR,
                   INTERVAL,
                   SEGMENT_CREATED,
                   INDEXING,
                   READ_ONLY
            FROM {view}
            {owner_filter}
            ORDER BY TABLE_NAME, PARTITION_POSITION
            "
        );
        for row in connection.query_as::<Row>(&sql, &[owner])? {
            let (
                owner,
                table,
                composite,
                name,
                subpartition_count,
                high_value,
                high_value_length,
                position,
                tablespace,
                compression,
                compress_for,
                interval,
                segment_created,
                indexing,
                read_only,
            ) = row?;
            let high_value = normalize_partition_high_value(
                &owner,
                &table,
                &name,
                high_value_length,
                high_value,
            )?;
            partitions.push(RawTablePartition {
                owner,
                table,
                composite: composite.trim().to_owned(),
                name,
                subpartition_count,
                high_value,
                high_value_length,
                position,
                tablespace: normalize_optional_token(tablespace),
                compression: compression.trim().to_owned(),
                compress_for: normalize_optional_token(compress_for),
                interval: interval.trim().to_owned(),
                segment_created: segment_created.trim().to_owned(),
                indexing: indexing.trim().to_owned(),
                read_only: read_only.trim().to_owned(),
            });
        }
    }
    partitions.sort_by(|left, right| {
        (&left.owner, &left.table, left.position).cmp(&(&right.owner, &right.table, right.position))
    });
    Ok(partitions)
}

fn read_table_subpartitions(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawTableSubpartition>, CatalogError> {
    type Row = (
        String,
        String,
        String,
        String,
        Option<String>,
        i64,
        i64,
        i64,
        Option<String>,
        String,
        Option<String>,
        String,
        String,
        String,
        String,
    );
    let mut subpartitions = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let (view, owner_expression, owner_filter) = match scope.mode {
            DictionaryScopeMode::User => ("USER_TAB_SUBPARTITIONS", ":1", ""),
            DictionaryScopeMode::Dba => (
                "DBA_TAB_SUBPARTITIONS",
                "TABLE_OWNER",
                "WHERE TABLE_OWNER = :1",
            ),
        };
        let sql = format!(
            "
            SELECT {owner_expression},
                   TABLE_NAME,
                   PARTITION_NAME,
                   SUBPARTITION_NAME,
                   HIGH_VALUE_CLOB,
                   HIGH_VALUE_LENGTH,
                   PARTITION_POSITION,
                   SUBPARTITION_POSITION,
                   TABLESPACE_NAME,
                   COMPRESSION,
                   COMPRESS_FOR,
                   INTERVAL,
                   SEGMENT_CREATED,
                   INDEXING,
                   READ_ONLY
            FROM {view}
            {owner_filter}
            ORDER BY TABLE_NAME, PARTITION_POSITION, SUBPARTITION_POSITION
            "
        );
        for row in connection.query_as::<Row>(&sql, &[owner])? {
            let (
                owner,
                table,
                partition,
                name,
                high_value,
                high_value_length,
                partition_position,
                position,
                tablespace,
                compression,
                compress_for,
                interval,
                segment_created,
                indexing,
                read_only,
            ) = row?;
            let high_value = normalize_partition_high_value(
                &owner,
                &table,
                &name,
                high_value_length,
                high_value,
            )?;
            subpartitions.push(RawTableSubpartition {
                owner,
                table,
                partition,
                name,
                high_value,
                high_value_length,
                partition_position,
                position,
                tablespace: normalize_optional_token(tablespace),
                compression: compression.trim().to_owned(),
                compress_for: normalize_optional_token(compress_for),
                interval: interval.trim().to_owned(),
                segment_created: segment_created.trim().to_owned(),
                indexing: indexing.trim().to_owned(),
                read_only: read_only.trim().to_owned(),
            });
        }
    }
    subpartitions.sort_by(|left, right| {
        (
            &left.owner,
            &left.table,
            left.partition_position,
            left.position,
        )
            .cmp(&(
                &right.owner,
                &right.table,
                right.partition_position,
                right.position,
            ))
    });
    Ok(subpartitions)
}

fn read_partitioned_indexes(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawPartitionedIndex>, CatalogError> {
    type Row = (
        String,
        String,
        String,
        String,
        String,
        i64,
        i64,
        i64,
        i64,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let mut indexes = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let (view, owner_expression, owner_filter) = match scope.mode {
            DictionaryScopeMode::User => (
                "USER_PART_INDEXES",
                ":1",
                "WHERE INDEX_NAME IN (SELECT INDEX_NAME FROM USER_INDEXES WHERE INDEX_TYPE <> 'LOB')",
            ),
            DictionaryScopeMode::Dba => (
                "DBA_PART_INDEXES",
                "OWNER",
                "WHERE OWNER = :1 AND INDEX_NAME IN (SELECT INDEX_NAME FROM DBA_INDEXES WHERE OWNER = :1 AND INDEX_TYPE <> 'LOB')",
            ),
        };
        let sql = format!(
            "
            SELECT {owner_expression},
                   INDEX_NAME,
                   TABLE_NAME,
                   PARTITIONING_TYPE,
                   SUBPARTITIONING_TYPE,
                   PARTITION_COUNT,
                   DEF_SUBPARTITION_COUNT,
                   PARTITIONING_KEY_COUNT,
                   SUBPARTITIONING_KEY_COUNT,
                   LOCALITY,
                   ALIGNMENT,
                   DEF_TABLESPACE_NAME,
                   INTERVAL,
                   AUTOLIST,
                   INTERVAL_SUBPARTITION,
                   AUTOLIST_SUBPARTITION
            FROM {view}
            {owner_filter}
            ORDER BY INDEX_NAME
            "
        );
        for row in connection.query_as::<Row>(&sql, &[owner])? {
            let (
                owner,
                index,
                table,
                partitioning_type,
                subpartitioning_type,
                partition_count,
                default_subpartition_count,
                partitioning_key_count,
                subpartitioning_key_count,
                locality,
                alignment,
                default_tablespace,
                interval,
                autolist,
                interval_subpartition,
                autolist_subpartition,
            ) = row?;
            indexes.push(RawPartitionedIndex {
                owner,
                index,
                table,
                partitioning_type: partitioning_type.trim().to_owned(),
                subpartitioning_type: subpartitioning_type.trim().to_owned(),
                partition_count,
                default_subpartition_count,
                partitioning_key_count,
                subpartitioning_key_count,
                locality: locality.trim().to_owned(),
                alignment: alignment.trim().to_owned(),
                default_tablespace: normalize_optional_token(default_tablespace),
                interval: normalize_definition(interval)?,
                autolist: normalize_optional_token(autolist),
                interval_subpartition: normalize_definition(interval_subpartition)?,
                autolist_subpartition: normalize_optional_token(autolist_subpartition),
            });
        }
    }
    indexes.sort_by(|left, right| (&left.owner, &left.index).cmp(&(&right.owner, &right.index)));
    Ok(indexes)
}
