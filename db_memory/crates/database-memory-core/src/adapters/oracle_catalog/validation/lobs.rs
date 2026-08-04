fn read_lobs(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawLob>, CatalogError> {
    type Row = (
        String,
        String,
        String,
        String,
        Option<String>,
        String,
        i64,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        Option<i64>,
        Option<String>,
        Option<i64>,
    );
    let mut lobs = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let (view, owner_expression, owner_filter) = match scope.mode {
            DictionaryScopeMode::User => ("USER_LOBS", ":1", ""),
            DictionaryScopeMode::Dba => ("DBA_LOBS", "OWNER", "WHERE OWNER = :1"),
        };
        let sql = format!(
            "
            SELECT {owner_expression},
                   TABLE_NAME,
                   COLUMN_NAME,
                   SEGMENT_NAME,
                   TABLESPACE_NAME,
                   INDEX_NAME,
                   CHUNK,
                   PCTVERSION,
                   RETENTION,
                   FREEPOOLS,
                   CACHE,
                   LOGGING,
                   ENCRYPT,
                   COMPRESSION,
                   DEDUPLICATION,
                   IN_ROW,
                   FORMAT,
                   PARTITIONED,
                   SECUREFILE,
                   SEGMENT_CREATED,
                   RETENTION_TYPE,
                   RETENTION_VALUE,
                   VALUE_BASED,
                   MAX_INLINE
            FROM {view}
            {owner_filter}
            ORDER BY TABLE_NAME, COLUMN_NAME
            "
        );
        for row in connection.query_as::<Row>(&sql, &[owner])? {
            let (
                owner,
                table,
                column,
                segment_name,
                tablespace,
                index_name,
                chunk,
                pctversion,
                retention,
                freepools,
                cache,
                logging,
                encrypt,
                compression,
                deduplication,
                in_row,
                format,
                partitioned,
                securefile,
                segment_created,
                retention_type,
                retention_value,
                value_based,
                max_inline,
            ) = row?;
            lobs.push(RawLob {
                owner,
                table,
                column,
                segment_name,
                tablespace: normalize_optional_token(tablespace),
                index_name,
                chunk,
                pctversion,
                retention,
                freepools,
                cache: cache.trim().to_owned(),
                logging: logging.trim().to_owned(),
                encrypt: encrypt.trim().to_owned(),
                compression: compression.trim().to_owned(),
                deduplication: deduplication.trim().to_owned(),
                in_row: in_row.trim().to_owned(),
                format: format.trim().to_owned(),
                partitioned: partitioned.trim().to_owned(),
                securefile: securefile.trim().to_owned(),
                segment_created: segment_created.trim().to_owned(),
                retention_type: normalize_optional_token(retention_type),
                retention_value,
                value_based: normalize_optional_token(value_based),
                max_inline,
            });
        }
    }
    lobs.sort_by(|left, right| {
        (&left.owner, &left.table, &left.column).cmp(&(&right.owner, &right.table, &right.column))
    });
    Ok(lobs)
}

fn read_lob_partitions(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawLobPartition>, CatalogError> {
    type Row = (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        i64,
        Option<i64>,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        String,
        String,
        String,
        String,
        String,
        Option<i64>,
    );
    let mut partitions = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let (view, owner_expression, owner_filter) = match scope.mode {
            DictionaryScopeMode::User => ("USER_LOB_PARTITIONS", ":1", ""),
            DictionaryScopeMode::Dba => (
                "DBA_LOB_PARTITIONS",
                "TABLE_OWNER",
                "WHERE TABLE_OWNER = :1",
            ),
        };
        let sql = format!(
            "
            SELECT {owner_expression},
                   TABLE_NAME,
                   COLUMN_NAME,
                   LOB_NAME,
                   PARTITION_NAME,
                   LOB_PARTITION_NAME,
                   LOB_INDPART_NAME,
                   PARTITION_POSITION,
                   COMPOSITE,
                   CHUNK,
                   PCTVERSION,
                   CACHE,
                   IN_ROW,
                   TABLESPACE_NAME,
                   RETENTION,
                   LOGGING,
                   ENCRYPT,
                   COMPRESSION,
                   DEDUPLICATION,
                   SECUREFILE,
                   SEGMENT_CREATED,
                   MAX_INLINE
            FROM {view}
            {owner_filter}
            ORDER BY TABLE_NAME, COLUMN_NAME, PARTITION_POSITION
            "
        );
        for row in connection.query_as::<Row>(&sql, &[owner])? {
            let (
                owner,
                table,
                column,
                lob_name,
                table_partition,
                name,
                index_partition_name,
                position,
                composite,
                chunk,
                pctversion,
                cache,
                in_row,
                tablespace,
                retention,
                logging,
                encrypt,
                compression,
                deduplication,
                securefile,
                segment_created,
                max_inline,
            ) = row?;
            partitions.push(RawLobPartition {
                owner,
                table,
                column,
                lob_name,
                table_partition,
                name,
                index_partition_name,
                position,
                composite: composite.trim().to_owned(),
                chunk,
                pctversion,
                cache: cache.trim().to_owned(),
                in_row: in_row.trim().to_owned(),
                tablespace: normalize_optional_token(tablespace),
                retention: normalize_optional_token(retention),
                logging: logging.trim().to_owned(),
                encrypt: encrypt.trim().to_owned(),
                compression: compression.trim().to_owned(),
                deduplication: deduplication.trim().to_owned(),
                securefile: securefile.trim().to_owned(),
                segment_created: segment_created.trim().to_owned(),
                max_inline,
            });
        }
    }
    partitions.sort_by(|left, right| {
        (&left.owner, &left.table, &left.column, left.position).cmp(&(
            &right.owner,
            &right.table,
            &right.column,
            right.position,
        ))
    });
    Ok(partitions)
}

fn read_lob_subpartitions(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawLobSubpartition>, CatalogError> {
    type Row = (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        i64,
        i64,
        Option<i64>,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        String,
        String,
        String,
        String,
        String,
        Option<i64>,
    );
    let mut subpartitions = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let (view, owner_expression, owner_filter) = match scope.mode {
            DictionaryScopeMode::User => ("USER_LOB_SUBPARTITIONS", ":1", ""),
            DictionaryScopeMode::Dba => (
                "DBA_LOB_SUBPARTITIONS",
                "TABLE_OWNER",
                "WHERE TABLE_OWNER = :1",
            ),
        };
        let sql = format!(
            "
            SELECT {owner_expression},
                   TABLE_NAME,
                   COLUMN_NAME,
                   LOB_NAME,
                   LOB_PARTITION_NAME,
                   SUBPARTITION_NAME,
                   LOB_SUBPARTITION_NAME,
                   LOB_INDSUBPART_NAME,
                   SUBPARTITION_POSITION,
                   CHUNK,
                   PCTVERSION,
                   CACHE,
                   IN_ROW,
                   TABLESPACE_NAME,
                   RETENTION,
                   LOGGING,
                   ENCRYPT,
                   COMPRESSION,
                   DEDUPLICATION,
                   SECUREFILE,
                   SEGMENT_CREATED,
                   MAX_INLINE
            FROM {view}
            {owner_filter}
            ORDER BY TABLE_NAME, COLUMN_NAME, LOB_PARTITION_NAME, SUBPARTITION_POSITION
            "
        );
        for row in connection.query_as::<Row>(&sql, &[owner])? {
            let (
                owner,
                table,
                column,
                lob_name,
                lob_partition_name,
                table_subpartition,
                name,
                index_subpartition_name,
                position,
                chunk,
                pctversion,
                cache,
                in_row,
                tablespace,
                retention,
                logging,
                encrypt,
                compression,
                deduplication,
                securefile,
                segment_created,
                max_inline,
            ) = row?;
            subpartitions.push(RawLobSubpartition {
                owner,
                table,
                column,
                lob_name,
                lob_partition_name,
                table_subpartition,
                name,
                index_subpartition_name,
                position,
                chunk,
                pctversion,
                cache: cache.trim().to_owned(),
                in_row: in_row.trim().to_owned(),
                tablespace: normalize_optional_token(tablespace),
                retention: normalize_optional_token(retention),
                logging: logging.trim().to_owned(),
                encrypt: encrypt.trim().to_owned(),
                compression: compression.trim().to_owned(),
                deduplication: deduplication.trim().to_owned(),
                securefile: securefile.trim().to_owned(),
                segment_created: segment_created.trim().to_owned(),
                max_inline,
            });
        }
    }
    subpartitions.sort_by(|left, right| {
        (
            &left.owner,
            &left.table,
            &left.column,
            &left.lob_partition_name,
            left.position,
        )
            .cmp(&(
                &right.owner,
                &right.table,
                &right.column,
                &right.lob_partition_name,
                right.position,
            ))
    });
    Ok(subpartitions)
}
