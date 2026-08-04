fn read_sequences(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawSequence>, CatalogError> {
    type SequenceTuple = (
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let mut sequences = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       SEQUENCE_NAME,
                       TO_CHAR(MIN_VALUE, 'TM9'),
                       TO_CHAR(MAX_VALUE, 'TM9'),
                       TO_CHAR(INCREMENT_BY, 'TM9'),
                       CYCLE_FLAG,
                       ORDER_FLAG,
                       TO_CHAR(CACHE_SIZE, 'TM9'),
                       SCALE_FLAG,
                       EXTEND_FLAG,
                       SHARDED_FLAG,
                       SESSION_FLAG,
                       KEEP_VALUE
                FROM USER_SEQUENCES
                ORDER BY SEQUENCE_NAME
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT SEQUENCE_OWNER,
                       SEQUENCE_NAME,
                       TO_CHAR(MIN_VALUE, 'TM9'),
                       TO_CHAR(MAX_VALUE, 'TM9'),
                       TO_CHAR(INCREMENT_BY, 'TM9'),
                       CYCLE_FLAG,
                       ORDER_FLAG,
                       TO_CHAR(CACHE_SIZE, 'TM9'),
                       SCALE_FLAG,
                       EXTEND_FLAG,
                       SHARDED_FLAG,
                       SESSION_FLAG,
                       KEEP_VALUE
                FROM DBA_SEQUENCES
                WHERE SEQUENCE_OWNER = :1
                ORDER BY SEQUENCE_OWNER, SEQUENCE_NAME
                "
            }
        };
        let rows = connection.query_as::<SequenceTuple>(sql, &[owner])?;
        for row in rows {
            let (
                owner,
                name,
                min_value,
                max_value,
                increment_by,
                cycle,
                ordered,
                cache_size,
                scale,
                extend,
                sharded,
                session,
                keep_value,
            ) = row?;
            sequences.push(RawSequence {
                owner,
                name,
                min_value,
                max_value,
                increment_by,
                cycle,
                ordered,
                cache_size,
                scale,
                extend,
                sharded,
                session,
                keep_value,
            });
        }
    }
    sequences.sort_by(|left, right| (&left.owner, &left.name).cmp(&(&right.owner, &right.name)));
    Ok(sequences)
}

fn read_identity_columns(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawIdentityColumn>, CatalogError> {
    let mut identities = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       TABLE_NAME,
                       COLUMN_NAME,
                       GENERATION_TYPE,
                       SEQUENCE_NAME,
                       IDENTITY_OPTIONS
                FROM USER_TAB_IDENTITY_COLS
                ORDER BY TABLE_NAME, COLUMN_NAME
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER,
                       TABLE_NAME,
                       COLUMN_NAME,
                       GENERATION_TYPE,
                       SEQUENCE_NAME,
                       IDENTITY_OPTIONS
                FROM DBA_TAB_IDENTITY_COLS
                WHERE OWNER = :1
                ORDER BY OWNER, TABLE_NAME, COLUMN_NAME
                "
            }
        };
        let rows = connection.query_as::<(
            String,
            String,
            String,
            Option<String>,
            String,
            Option<String>,
        )>(sql, &[owner])?;
        for row in rows {
            let (owner, table, column, generation_type, sequence_name, options) = row?;
            identities.push(RawIdentityColumn {
                owner,
                table,
                column,
                generation_type,
                sequence_name,
                options: normalize_definition(options)?,
            });
        }
    }
    identities.sort_by(|left, right| {
        (&left.owner, &left.table, &left.column).cmp(&(&right.owner, &right.table, &right.column))
    });
    Ok(identities)
}
