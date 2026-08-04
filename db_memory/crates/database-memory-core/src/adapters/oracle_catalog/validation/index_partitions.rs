fn read_index_partitions(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawIndexPartition>, CatalogError> {
    type Row = (
        String,
        String,
        String,
        String,
        i64,
        Option<String>,
        i64,
        i64,
        String,
        Option<String>,
        String,
        String,
        String,
    );
    let mut partitions = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let (view, owner_expression, owner_filter) = match scope.mode {
            DictionaryScopeMode::User => (
                "USER_IND_PARTITIONS",
                ":1",
                "WHERE INDEX_NAME IN (SELECT INDEX_NAME FROM USER_INDEXES WHERE INDEX_TYPE <> 'LOB')",
            ),
            DictionaryScopeMode::Dba => (
                "DBA_IND_PARTITIONS",
                "INDEX_OWNER",
                "WHERE INDEX_OWNER = :1 AND INDEX_NAME IN (SELECT INDEX_NAME FROM DBA_INDEXES WHERE OWNER = :1 AND INDEX_TYPE <> 'LOB')",
            ),
        };
        let sql = format!(
            "
            SELECT {owner_expression},
                   INDEX_NAME,
                   COMPOSITE,
                   PARTITION_NAME,
                   SUBPARTITION_COUNT,
                   HIGH_VALUE_CLOB,
                   HIGH_VALUE_LENGTH,
                   PARTITION_POSITION,
                   STATUS,
                   TABLESPACE_NAME,
                   COMPRESSION,
                   INTERVAL,
                   SEGMENT_CREATED
            FROM {view}
            {owner_filter}
            ORDER BY INDEX_NAME, PARTITION_POSITION
            "
        );
        for row in connection.query_as::<Row>(&sql, &[owner])? {
            let (
                owner,
                index,
                composite,
                name,
                subpartition_count,
                high_value,
                high_value_length,
                position,
                status,
                tablespace,
                compression,
                interval,
                segment_created,
            ) = row?;
            let high_value = normalize_partition_high_value(
                &owner,
                &index,
                &name,
                high_value_length,
                high_value,
            )?;
            partitions.push(RawIndexPartition {
                owner,
                index,
                composite: composite.trim().to_owned(),
                name,
                subpartition_count,
                high_value,
                high_value_length,
                position,
                status: status.trim().to_owned(),
                tablespace: normalize_optional_token(tablespace),
                compression: compression.trim().to_owned(),
                interval: interval.trim().to_owned(),
                segment_created: segment_created.trim().to_owned(),
            });
        }
    }
    partitions.sort_by(|left, right| {
        (&left.owner, &left.index, left.position).cmp(&(&right.owner, &right.index, right.position))
    });
    Ok(partitions)
}

fn read_index_subpartitions(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawIndexSubpartition>, CatalogError> {
    type Row = (
        String,
        String,
        String,
        String,
        Option<String>,
        i64,
        i64,
        i64,
        String,
        Option<String>,
        String,
        String,
        String,
    );
    let mut subpartitions = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let (view, owner_expression, owner_filter) = match scope.mode {
            DictionaryScopeMode::User => (
                "USER_IND_SUBPARTITIONS",
                ":1",
                "WHERE INDEX_NAME IN (SELECT INDEX_NAME FROM USER_INDEXES WHERE INDEX_TYPE <> 'LOB')",
            ),
            DictionaryScopeMode::Dba => (
                "DBA_IND_SUBPARTITIONS",
                "INDEX_OWNER",
                "WHERE INDEX_OWNER = :1 AND INDEX_NAME IN (SELECT INDEX_NAME FROM DBA_INDEXES WHERE OWNER = :1 AND INDEX_TYPE <> 'LOB')",
            ),
        };
        let sql = format!(
            "
            SELECT {owner_expression},
                   INDEX_NAME,
                   PARTITION_NAME,
                   SUBPARTITION_NAME,
                   HIGH_VALUE_CLOB,
                   HIGH_VALUE_LENGTH,
                   PARTITION_POSITION,
                   SUBPARTITION_POSITION,
                   STATUS,
                   TABLESPACE_NAME,
                   COMPRESSION,
                   INTERVAL,
                   SEGMENT_CREATED
            FROM {view}
            {owner_filter}
            ORDER BY INDEX_NAME, PARTITION_POSITION, SUBPARTITION_POSITION
            "
        );
        for row in connection.query_as::<Row>(&sql, &[owner])? {
            let (
                owner,
                index,
                partition,
                name,
                high_value,
                high_value_length,
                partition_position,
                position,
                status,
                tablespace,
                compression,
                interval,
                segment_created,
            ) = row?;
            let high_value = normalize_partition_high_value(
                &owner,
                &index,
                &name,
                high_value_length,
                high_value,
            )?;
            subpartitions.push(RawIndexSubpartition {
                owner,
                index,
                partition,
                name,
                high_value,
                high_value_length,
                partition_position,
                position,
                status: status.trim().to_owned(),
                tablespace: normalize_optional_token(tablespace),
                compression: compression.trim().to_owned(),
                interval: interval.trim().to_owned(),
                segment_created: segment_created.trim().to_owned(),
            });
        }
    }
    subpartitions.sort_by(|left, right| {
        (
            &left.owner,
            &left.index,
            left.partition_position,
            left.position,
        )
            .cmp(&(
                &right.owner,
                &right.index,
                right.partition_position,
                right.position,
            ))
    });
    Ok(subpartitions)
}

fn read_partition_key_columns(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawPartitionKeyColumn>, CatalogError> {
    let mut columns = Vec::new();
    for subpartition in [false, true] {
        for owner in &scope.owners {
            prepare_call(connection, deadline)?;
            let (view, owner_expression, owner_filter) = match (scope.mode, subpartition) {
                (DictionaryScopeMode::User, false) => (
                    "USER_PART_KEY_COLUMNS",
                    ":1",
                    "WHERE OBJECT_TYPE <> 'INDEX' OR NAME NOT IN (SELECT INDEX_NAME FROM USER_INDEXES WHERE INDEX_TYPE = 'LOB')",
                ),
                (DictionaryScopeMode::User, true) => (
                    "USER_SUBPART_KEY_COLUMNS",
                    ":1",
                    "WHERE OBJECT_TYPE <> 'INDEX' OR NAME NOT IN (SELECT INDEX_NAME FROM USER_INDEXES WHERE INDEX_TYPE = 'LOB')",
                ),
                (DictionaryScopeMode::Dba, false) => (
                    "DBA_PART_KEY_COLUMNS",
                    "OWNER",
                    "WHERE OWNER = :1 AND (OBJECT_TYPE <> 'INDEX' OR NAME NOT IN (SELECT INDEX_NAME FROM DBA_INDEXES WHERE OWNER = :1 AND INDEX_TYPE = 'LOB'))",
                ),
                (DictionaryScopeMode::Dba, true) => (
                    "DBA_SUBPART_KEY_COLUMNS",
                    "OWNER",
                    "WHERE OWNER = :1 AND (OBJECT_TYPE <> 'INDEX' OR NAME NOT IN (SELECT INDEX_NAME FROM DBA_INDEXES WHERE OWNER = :1 AND INDEX_TYPE = 'LOB'))",
                ),
            };
            let sql = format!(
                "
                SELECT {owner_expression},
                       NAME,
                       OBJECT_TYPE,
                       COLUMN_NAME,
                       COLUMN_POSITION,
                       COLLATED_COLUMN_ID
                FROM {view}
                {owner_filter}
                ORDER BY NAME, OBJECT_TYPE, COLUMN_POSITION
                "
            );
            let rows = connection
                .query_as::<(String, String, String, String, i64, Option<i64>)>(&sql, &[owner])?;
            for row in rows {
                let (owner, name, object_type, column, position, collated_column_id) = row?;
                columns.push(RawPartitionKeyColumn {
                    owner,
                    name,
                    object_type: object_type.trim().to_owned(),
                    column,
                    position,
                    collated_column_id,
                    subpartition,
                });
            }
        }
    }
    columns.sort_by(|left, right| {
        (
            &left.owner,
            &left.name,
            &left.object_type,
            left.subpartition,
            left.position,
        )
            .cmp(&(
                &right.owner,
                &right.name,
                &right.object_type,
                right.subpartition,
                right.position,
            ))
    });
    Ok(columns)
}
