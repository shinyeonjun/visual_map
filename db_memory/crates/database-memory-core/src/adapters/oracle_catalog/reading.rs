fn read_principals(
    connection: &Connection,
    mode: DictionaryScopeMode,
    owners: &[String],
    deadline: Instant,
) -> Result<Vec<RawPrincipal>, CatalogError> {
    prepare_call(connection, deadline)?;
    let mut principals = Vec::new();
    match mode {
        DictionaryScopeMode::User => {
            let rows = connection
                .query_as::<(String, i64, String, String, String, Option<String>)>(
                    "
                SELECT USERNAME,
                       USER_ID,
                       ACCOUNT_STATUS,
                       COMMON,
                       ORACLE_MAINTAINED,
                       DEFAULT_COLLATION
                FROM USER_USERS
                ",
                    &[],
                )?;
            for row in rows {
                let (name, user_id, account_status, common, maintained, collation) = row?;
                principals.push(RawPrincipal {
                    name,
                    user_id,
                    account_status,
                    common: common == "YES",
                    oracle_maintained: maintained == "Y",
                    default_collation: collation,
                });
            }
        }
        DictionaryScopeMode::Dba => {
            for owner in owners {
                prepare_call(connection, deadline)?;
                let rows = connection
                    .query_as::<(String, i64, String, String, String, Option<String>)>(
                        "
                    SELECT USERNAME,
                           USER_ID,
                           ACCOUNT_STATUS,
                           COMMON,
                           ORACLE_MAINTAINED,
                           DEFAULT_COLLATION
                    FROM DBA_USERS
                    WHERE USERNAME = :1
                    ",
                        &[owner],
                    )?;
                for row in rows {
                    let (name, user_id, account_status, common, maintained, collation) = row?;
                    principals.push(RawPrincipal {
                        name,
                        user_id,
                        account_status,
                        common: common == "YES",
                        oracle_maintained: maintained == "Y",
                        default_collation: collation,
                    });
                }
            }
        }
    }
    principals.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(principals)
}

fn reject_database_links(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<(), CatalogError> {
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => "SELECT :1, DB_LINK FROM USER_DB_LINKS ORDER BY DB_LINK",
            DictionaryScopeMode::Dba => {
                "SELECT OWNER, DB_LINK FROM DBA_DB_LINKS WHERE OWNER = :1 ORDER BY OWNER, DB_LINK"
            }
        };
        let mut rows = connection.query_as::<(String, String)>(sql, &[owner])?;
        if let Some(row) = rows.next() {
            let (link_owner, link_name) = row?;
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle schema {link_owner} contains unsupported database link '{link_name}'"
            )));
        }
    }
    Ok(())
}

fn read_recycle_bin(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<BTreeSet<(String, String)>, CatalogError> {
    let mut recycle = BTreeSet::new();
    match scope.mode {
        DictionaryScopeMode::User => {
            prepare_call(connection, deadline)?;
            let rows = connection.query_as::<String>(
                "SELECT OBJECT_NAME FROM USER_RECYCLEBIN ORDER BY OBJECT_NAME",
                &[],
            )?;
            for row in rows {
                recycle.insert((scope.owners[0].clone(), row?));
            }
        }
        DictionaryScopeMode::Dba => {
            for owner in &scope.owners {
                prepare_call(connection, deadline)?;
                let rows = connection.query_as::<(String, String)>(
                    "
                    SELECT OWNER, OBJECT_NAME
                    FROM DBA_RECYCLEBIN
                    WHERE OWNER = :1
                    ORDER BY OWNER, OBJECT_NAME
                    ",
                    &[owner],
                )?;
                for row in rows {
                    recycle.insert(row?);
                }
            }
        }
    }
    Ok(recycle)
}

fn read_inventory(
    connection: &Connection,
    scope: &DictionaryScope,
    recycle: &BTreeSet<(String, String)>,
    deadline: Instant,
) -> Result<Vec<RawInventoryObject>, CatalogError> {
    type InventoryTuple = (
        String,
        String,
        Option<String>,
        i64,
        Option<i64>,
        String,
        String,
        String,
        String,
        String,
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let mut inventory = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       OBJECT_NAME,
                       SUBOBJECT_NAME,
                       OBJECT_ID,
                       DATA_OBJECT_ID,
                       OBJECT_TYPE,
                       STATUS,
                       TEMPORARY,
                       GENERATED,
                       SECONDARY,
                       NAMESPACE,
                       EDITION_NAME,
                       EDITIONABLE,
                       DEFAULT_COLLATION
                FROM USER_OBJECTS
                WHERE ORACLE_MAINTAINED = 'N'
                ORDER BY OBJECT_TYPE, OBJECT_NAME, SUBOBJECT_NAME
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER,
                       OBJECT_NAME,
                       SUBOBJECT_NAME,
                       OBJECT_ID,
                       DATA_OBJECT_ID,
                       OBJECT_TYPE,
                       STATUS,
                       TEMPORARY,
                       GENERATED,
                       SECONDARY,
                       NAMESPACE,
                       EDITION_NAME,
                       EDITIONABLE,
                       DEFAULT_COLLATION
                FROM DBA_OBJECTS
                WHERE OWNER = :1
                  AND ORACLE_MAINTAINED = 'N'
                ORDER BY OBJECT_TYPE, OBJECT_NAME, SUBOBJECT_NAME
                "
            }
        };
        let rows = connection.query_as::<InventoryTuple>(sql, &[owner])?;
        for row in rows {
            let (
                owner,
                name,
                subobject,
                object_id,
                data_object_id,
                object_type,
                status,
                temporary,
                generated,
                secondary,
                namespace,
                edition_name,
                editionable,
                default_collation,
            ) = row?;
            if recycle.contains(&(owner.clone(), name.clone())) {
                continue;
            }
            inventory.push(RawInventoryObject {
                owner,
                name,
                subobject,
                object_id,
                data_object_id,
                object_type,
                status,
                temporary: temporary == "Y",
                generated: generated == "Y",
                secondary: secondary == "Y",
                namespace,
                edition_name,
                editionable,
                default_collation,
            });
        }
    }
    inventory.sort_by(|left, right| {
        (&left.owner, &left.object_type, &left.name, &left.subobject).cmp(&(
            &right.owner,
            &right.object_type,
            &right.name,
            &right.subobject,
        ))
    });
    Ok(inventory)
}

fn read_tables(
    connection: &Connection,
    scope: &DictionaryScope,
    recycle: &BTreeSet<(String, String)>,
    deadline: Instant,
) -> Result<Vec<RawTable>, CatalogError> {
    type TableTuple = (
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        String,
        String,
        String,
        Option<String>,
        String,
    );
    let mut tables = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       TABLE_NAME,
                       STATUS,
                       TEMPORARY,
                       PARTITIONED,
                       IOT_TYPE,
                       NESTED,
                       READ_ONLY,
                       HAS_IDENTITY,
                       DURATION,
                       EXTERNAL
                FROM USER_TABLES
                WHERE SECONDARY = 'N'
                  AND DROPPED = 'NO'
                ORDER BY TABLE_NAME
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER,
                       TABLE_NAME,
                       STATUS,
                       TEMPORARY,
                       PARTITIONED,
                       IOT_TYPE,
                       NESTED,
                       READ_ONLY,
                       HAS_IDENTITY,
                       DURATION,
                       EXTERNAL
                FROM DBA_TABLES
                WHERE OWNER = :1
                  AND SECONDARY = 'N'
                  AND DROPPED = 'NO'
                ORDER BY OWNER, TABLE_NAME
                "
            }
        };
        let rows = connection.query_as::<TableTuple>(sql, &[owner])?;
        for row in rows {
            let (
                owner,
                name,
                status,
                temporary,
                partitioned,
                iot_type,
                nested,
                read_only,
                has_identity,
                duration,
                external,
            ) = row?;
            if recycle.contains(&(owner.clone(), name.clone())) {
                continue;
            }
            tables.push(RawTable {
                owner,
                name,
                status,
                temporary: temporary == "Y",
                partitioned: partitioned == "YES",
                iot_type,
                nested: nested == "YES",
                read_only: read_only == "YES",
                has_identity: has_identity == "YES",
                duration,
                external: external == "YES",
            });
        }
    }
    tables.sort_by(|left, right| (&left.owner, &left.name).cmp(&(&right.owner, &right.name)));
    Ok(tables)
}

fn read_columns(
    connection: &Connection,
    scope: &DictionaryScope,
    recycle: &BTreeSet<(String, String)>,
    deadline: Instant,
) -> Result<Vec<RawColumn>, CatalogError> {
    type ColumnTuple = (
        String,
        String,
        String,
        Option<i64>,
        i64,
        String,
        Option<String>,
        i64,
        Option<i64>,
        Option<i64>,
        String,
        Option<String>,
        String,
        String,
        String,
        String,
        String,
        Option<i64>,
        Option<String>,
        Option<String>,
    );
    let mut columns = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       c.TABLE_NAME,
                       c.COLUMN_NAME,
                       c.COLUMN_ID,
                       c.INTERNAL_COLUMN_ID,
                       c.DATA_TYPE,
                       c.DATA_TYPE_OWNER,
                       c.DATA_LENGTH,
                       c.DATA_PRECISION,
                       c.DATA_SCALE,
                       c.NULLABLE,
                       c.DATA_DEFAULT,
                       c.HIDDEN_COLUMN,
                       c.VIRTUAL_COLUMN,
                       c.USER_GENERATED,
                       c.DEFAULT_ON_NULL,
                       c.IDENTITY_COLUMN,
                       c.CHAR_LENGTH,
                       c.CHAR_USED,
                       c.COLLATION
                FROM USER_TAB_COLS c
                JOIN USER_TABLES t ON t.TABLE_NAME = c.TABLE_NAME
                WHERE t.SECONDARY = 'N'
                  AND t.DROPPED = 'NO'
                ORDER BY c.TABLE_NAME, c.INTERNAL_COLUMN_ID
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT c.OWNER,
                       c.TABLE_NAME,
                       c.COLUMN_NAME,
                       c.COLUMN_ID,
                       c.INTERNAL_COLUMN_ID,
                       c.DATA_TYPE,
                       c.DATA_TYPE_OWNER,
                       c.DATA_LENGTH,
                       c.DATA_PRECISION,
                       c.DATA_SCALE,
                       c.NULLABLE,
                       c.DATA_DEFAULT,
                       c.HIDDEN_COLUMN,
                       c.VIRTUAL_COLUMN,
                       c.USER_GENERATED,
                       c.DEFAULT_ON_NULL,
                       c.IDENTITY_COLUMN,
                       c.CHAR_LENGTH,
                       c.CHAR_USED,
                       c.COLLATION
                FROM DBA_TAB_COLS c
                JOIN DBA_TABLES t
                  ON t.OWNER = c.OWNER
                 AND t.TABLE_NAME = c.TABLE_NAME
                WHERE c.OWNER = :1
                  AND t.SECONDARY = 'N'
                  AND t.DROPPED = 'NO'
                ORDER BY c.OWNER, c.TABLE_NAME, c.INTERNAL_COLUMN_ID
                "
            }
        };
        let rows = connection.query_as::<ColumnTuple>(sql, &[owner])?;
        for row in rows {
            let (
                owner,
                table,
                name,
                column_id,
                internal_column_id,
                data_type,
                data_type_owner,
                data_length,
                data_precision,
                data_scale,
                nullable,
                default_value,
                hidden,
                virtual_column,
                user_generated,
                default_on_null,
                identity,
                char_length,
                char_used,
                collation,
            ) = row?;
            if recycle.contains(&(owner.clone(), table.clone())) {
                continue;
            }
            columns.push(RawColumn {
                owner,
                table,
                name,
                column_id,
                internal_column_id,
                data_type,
                data_type_owner,
                data_length,
                data_precision,
                data_scale,
                nullable: nullable == "Y",
                default_value: normalize_definition(default_value)?,
                hidden: hidden == "YES",
                virtual_column: virtual_column == "YES",
                user_generated: user_generated == "YES",
                default_on_null: default_on_null == "YES",
                identity: identity == "YES",
                char_length,
                char_used,
                collation,
            });
        }
    }
    columns.sort_by(|left, right| {
        (&left.owner, &left.table, left.internal_column_id).cmp(&(
            &right.owner,
            &right.table,
            right.internal_column_id,
        ))
    });
    Ok(columns)
}

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

fn read_views(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawView>, CatalogError> {
    type ViewTuple = (
        String,
        String,
        Option<i64>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let mut views = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       VIEW_NAME,
                       TEXT_LENGTH,
                       TEXT,
                       VIEW_TYPE_OWNER,
                       VIEW_TYPE,
                       SUPERVIEW_NAME,
                       EDITIONING_VIEW,
                       READ_ONLY,
                       CONTAINER_DATA,
                       BEQUEATH,
                       DEFAULT_COLLATION,
                       HAS_SENSITIVE_COLUMN,
                       ADMIT_NULL,
                       PDB_LOCAL_ONLY,
                       DUALITY_VIEW
                FROM USER_VIEWS
                ORDER BY VIEW_NAME
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER,
                       VIEW_NAME,
                       TEXT_LENGTH,
                       TEXT,
                       VIEW_TYPE_OWNER,
                       VIEW_TYPE,
                       SUPERVIEW_NAME,
                       EDITIONING_VIEW,
                       READ_ONLY,
                       CONTAINER_DATA,
                       BEQUEATH,
                       DEFAULT_COLLATION,
                       HAS_SENSITIVE_COLUMN,
                       ADMIT_NULL,
                       PDB_LOCAL_ONLY,
                       DUALITY_VIEW
                FROM DBA_VIEWS
                WHERE OWNER = :1
                ORDER BY OWNER, VIEW_NAME
                "
            }
        };
        let rows = connection.query_as::<ViewTuple>(sql, &[owner])?;
        for row in rows {
            let (
                owner,
                name,
                text_length,
                definition,
                type_owner,
                view_type,
                superview,
                editioning,
                read_only,
                container_data,
                bequeath,
                default_collation,
                has_sensitive_column,
                admit_null,
                pdb_local_only,
                duality_view,
            ) = row?;
            if text_length.is_some_and(|length| length > MAX_DEFINITION_BYTES as i64) {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "Oracle view definition exceeds the {MAX_DEFINITION_BYTES}-byte safety limit for {owner}.{name}"
                )));
            }
            views.push(RawView {
                owner,
                name,
                text_length,
                definition: normalize_definition(definition)?,
                type_owner,
                view_type,
                superview,
                editioning,
                read_only,
                container_data,
                bequeath,
                default_collation,
                has_sensitive_column,
                admit_null,
                pdb_local_only,
                duality_view,
            });
        }
    }
    views.sort_by(|left, right| (&left.owner, &left.name).cmp(&(&right.owner, &right.name)));
    Ok(views)
}

fn read_materialized_views(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawMaterializedView>, CatalogError> {
    type MaterializedViewTuple = (
        String,
        String,
        String,
        Option<i64>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let mut materialized_views = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let view = match scope.mode {
            DictionaryScopeMode::User => "USER_MVIEWS",
            DictionaryScopeMode::Dba => "DBA_MVIEWS",
        };
        let sql = format!(
            "
            SELECT OWNER,
                   MVIEW_NAME,
                   CONTAINER_NAME,
                   QUERY_LEN,
                   QUERY,
                   UPDATABLE,
                   MASTER_LINK,
                   REWRITE_ENABLED,
                   REWRITE_CAPABILITY,
                   REFRESH_MODE,
                   REFRESH_METHOD,
                   BUILD_MODE,
                   FAST_REFRESHABLE,
                   COMPILE_STATE,
                   USE_NO_INDEX,
                   SEGMENT_CREATED,
                   DEFAULT_COLLATION,
                   ON_QUERY_COMPUTATION,
                   AUTO,
                   CONCURRENT_REFRESH_ENABLED
            FROM {view}
            WHERE OWNER = :1
            ORDER BY OWNER, MVIEW_NAME
            "
        );
        let rows = connection.query_as::<MaterializedViewTuple>(&sql, &[owner])?;
        for row in rows {
            let (
                owner,
                name,
                container_name,
                query_length,
                definition,
                updatable,
                master_link,
                rewrite_enabled,
                rewrite_capability,
                refresh_mode,
                refresh_method,
                build_mode,
                fast_refreshable,
                compile_state,
                use_no_index,
                segment_created,
                default_collation,
                on_query_computation,
                automatic,
                concurrent_refresh,
            ) = row?;
            if query_length.is_some_and(|length| length > MAX_DEFINITION_BYTES as i64) {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "Oracle materialized-view definition exceeds the {MAX_DEFINITION_BYTES}-byte safety limit for {owner}.{name}"
                )));
            }
            materialized_views.push(RawMaterializedView {
                owner,
                name,
                container_name,
                query_length,
                definition: normalize_definition(definition)?,
                updatable,
                master_link,
                rewrite_enabled,
                rewrite_capability,
                refresh_mode,
                refresh_method,
                build_mode,
                fast_refreshable,
                compile_state,
                use_no_index,
                segment_created,
                default_collation,
                on_query_computation,
                automatic,
                concurrent_refresh,
            });
        }
    }
    materialized_views
        .sort_by(|left, right| (&left.owner, &left.name).cmp(&(&right.owner, &right.name)));
    Ok(materialized_views)
}

fn read_synonyms(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawSynonym>, CatalogError> {
    let mut synonyms = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       SYNONYM_NAME,
                       TABLE_OWNER,
                       TABLE_NAME,
                       DB_LINK,
                       ORIGIN_CON_ID
                FROM USER_SYNONYMS
                ORDER BY SYNONYM_NAME
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER,
                       SYNONYM_NAME,
                       TABLE_OWNER,
                       TABLE_NAME,
                       DB_LINK,
                       ORIGIN_CON_ID
                FROM DBA_SYNONYMS
                WHERE OWNER = :1
                ORDER BY OWNER, SYNONYM_NAME
                "
            }
        };
        let rows = connection
            .query_as::<(String, String, String, String, Option<String>, i64)>(sql, &[owner])?;
        for row in rows {
            let (owner, name, target_owner, target_name, database_link, origin_container_id) = row?;
            synonyms.push(RawSynonym {
                owner,
                name,
                target_owner,
                target_name,
                database_link: normalize_optional_token(database_link),
                origin_container_id,
            });
        }
    }
    synonyms.sort_by(|left, right| (&left.owner, &left.name).cmp(&(&right.owner, &right.name)));
    Ok(synonyms)
}

fn read_user_types(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawUserType>, CatalogError> {
    type TypeTuple = (
        String,
        String,
        String,
        Option<String>,
        Option<i64>,
        Option<i64>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<i64>,
        Option<i64>,
        Option<String>,
    );
    let mut types = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       TYPE_NAME,
                       RAWTOHEX(TYPE_OID),
                       TYPECODE,
                       ATTRIBUTES,
                       METHODS,
                       PREDEFINED,
                       INCOMPLETE,
                       FINAL,
                       INSTANTIABLE,
                       PERSISTABLE,
                       SUPERTYPE_OWNER,
                       SUPERTYPE_NAME,
                       LOCAL_ATTRIBUTES,
                       LOCAL_METHODS,
                       RAWTOHEX(TYPEID)
                FROM USER_TYPES
                ORDER BY TYPE_NAME
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER,
                       TYPE_NAME,
                       RAWTOHEX(TYPE_OID),
                       TYPECODE,
                       ATTRIBUTES,
                       METHODS,
                       PREDEFINED,
                       INCOMPLETE,
                       FINAL,
                       INSTANTIABLE,
                       PERSISTABLE,
                       SUPERTYPE_OWNER,
                       SUPERTYPE_NAME,
                       LOCAL_ATTRIBUTES,
                       LOCAL_METHODS,
                       RAWTOHEX(TYPEID)
                FROM DBA_TYPES
                WHERE OWNER = :1
                ORDER BY OWNER, TYPE_NAME
                "
            }
        };
        let rows = connection.query_as::<TypeTuple>(sql, &[owner])?;
        for row in rows {
            let (
                owner,
                name,
                oid,
                typecode,
                attribute_count,
                method_count,
                predefined,
                incomplete,
                final_type,
                instantiable,
                persistable,
                supertype_owner,
                supertype_name,
                local_attribute_count,
                local_method_count,
                type_id,
            ) = row?;
            types.push(RawUserType {
                owner: owner.clone(),
                name: name.clone(),
                oid,
                typecode: required_catalog_token(
                    typecode,
                    &format!("typecode for {owner}.{name}"),
                )?,
                attribute_count: attribute_count.ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "Oracle type {owner}.{name} has no attribute count"
                    ))
                })?,
                method_count: method_count.ok_or_else(|| {
                    CatalogError::Mapping(format!("Oracle type {owner}.{name} has no method count"))
                })?,
                predefined: required_catalog_token(
                    predefined,
                    &format!("predefined flag for {owner}.{name}"),
                )?,
                incomplete: required_catalog_token(
                    incomplete,
                    &format!("incomplete flag for {owner}.{name}"),
                )?,
                final_type: required_catalog_token(
                    final_type,
                    &format!("final flag for {owner}.{name}"),
                )?,
                instantiable: required_catalog_token(
                    instantiable,
                    &format!("instantiable flag for {owner}.{name}"),
                )?,
                persistable: required_catalog_token(
                    persistable,
                    &format!("persistable flag for {owner}.{name}"),
                )?,
                supertype_owner: normalize_optional_token(supertype_owner),
                supertype_name: normalize_optional_token(supertype_name),
                local_attribute_count,
                local_method_count,
                type_id: normalize_optional_token(type_id),
                specification: None,
                body: None,
            });
        }
    }
    types.sort_by(|left, right| (&left.owner, &left.name).cmp(&(&right.owner, &right.name)));
    Ok(types)
}

fn attach_type_sources(
    connection: &Connection,
    scope: &DictionaryScope,
    types: &mut [RawUserType],
    deadline: Instant,
) -> Result<(), CatalogError> {
    let positions = types
        .iter()
        .enumerate()
        .map(|(position, user_type)| ((user_type.owner.clone(), user_type.name.clone()), position))
        .collect::<BTreeMap<_, _>>();
    let mut sources = BTreeMap::<(usize, String), String>::new();
    let mut last_lines = BTreeMap::<(usize, String), i64>::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1, NAME, TYPE, LINE, TEXT
                FROM USER_SOURCE
                WHERE TYPE IN ('TYPE', 'TYPE BODY')
                ORDER BY NAME, TYPE, LINE
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER, NAME, TYPE, LINE, TEXT
                FROM DBA_SOURCE
                WHERE OWNER = :1
                  AND TYPE IN ('TYPE', 'TYPE BODY')
                ORDER BY OWNER, NAME, TYPE, LINE
                "
            }
        };
        let rows =
            connection.query_as::<(String, String, String, i64, Option<String>)>(sql, &[owner])?;
        for row in rows {
            let (source_owner, name, object_type, line, text) = row?;
            let position = positions
                .get(&(source_owner.clone(), name.clone()))
                .copied()
                .ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "Oracle type source {source_owner}.{name} ({object_type}) has no type header"
                    ))
                })?;
            let source_key = (position, object_type.clone());
            let expected_line = last_lines.get(&source_key).copied().unwrap_or(0) + 1;
            if line != expected_line {
                return Err(CatalogError::Mapping(format!(
                    "Oracle type source {source_owner}.{name} ({object_type}) expected line {expected_line}, found {line}"
                )));
            }
            last_lines.insert(source_key.clone(), line);
            let source = sources.entry(source_key).or_default();
            source.push_str(text.as_deref().unwrap_or_default());
            if source.len() > MAX_DEFINITION_BYTES {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "Oracle type definition exceeds the {MAX_DEFINITION_BYTES}-byte safety limit for {source_owner}.{name} ({object_type})"
                )));
            }
        }
    }
    for (position, user_type) in types.iter_mut().enumerate() {
        user_type.specification =
            normalize_definition(sources.remove(&(position, "TYPE".to_owned())))?;
        user_type.body = normalize_definition(sources.remove(&(position, "TYPE BODY".to_owned())))?;
        if user_type.specification.is_none() {
            return Err(CatalogError::Mapping(format!(
                "Oracle type {}.{} has no complete specification",
                user_type.owner, user_type.name
            )));
        }
    }
    Ok(())
}

fn read_type_attributes(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawTypeAttribute>, CatalogError> {
    type AttributeTuple = (
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        Option<String>,
        i64,
        Option<String>,
        Option<String>,
    );
    let mut attributes = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       TYPE_NAME,
                       ATTR_NAME,
                       ATTR_TYPE_MOD,
                       ATTR_TYPE_OWNER,
                       ATTR_TYPE_NAME,
                       LENGTH,
                       PRECISION,
                       SCALE,
                       CHARACTER_SET_NAME,
                       ATTR_NO,
                       INHERITED,
                       CHAR_USED
                FROM USER_TYPE_ATTRS
                ORDER BY TYPE_NAME, ATTR_NO
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER,
                       TYPE_NAME,
                       ATTR_NAME,
                       ATTR_TYPE_MOD,
                       ATTR_TYPE_OWNER,
                       ATTR_TYPE_NAME,
                       LENGTH,
                       PRECISION,
                       SCALE,
                       CHARACTER_SET_NAME,
                       ATTR_NO,
                       INHERITED,
                       CHAR_USED
                FROM DBA_TYPE_ATTRS
                WHERE OWNER = :1
                ORDER BY OWNER, TYPE_NAME, ATTR_NO
                "
            }
        };
        let rows = connection.query_as::<AttributeTuple>(sql, &[owner])?;
        for row in rows {
            let (
                owner,
                type_name,
                name,
                type_modifier,
                data_type_owner,
                data_type_name,
                length,
                precision,
                scale,
                character_set,
                position,
                inherited,
                char_used,
            ) = row?;
            attributes.push(RawTypeAttribute {
                owner: owner.clone(),
                type_name: type_name.clone(),
                name: name.clone(),
                type_modifier: normalize_optional_token(type_modifier),
                data_type_owner: normalize_optional_token(data_type_owner),
                data_type_name: required_catalog_token(
                    data_type_name,
                    &format!("attribute type for {owner}.{type_name}.{name}"),
                )?,
                length,
                precision,
                scale,
                character_set: normalize_optional_token(character_set),
                position,
                inherited: required_catalog_token(
                    inherited,
                    &format!("inherited flag for {owner}.{type_name}.{name}"),
                )?,
                char_used: normalize_optional_token(char_used),
            });
        }
    }
    attributes.sort_by(|left, right| {
        (&left.owner, &left.type_name, left.position).cmp(&(
            &right.owner,
            &right.type_name,
            right.position,
        ))
    });
    Ok(attributes)
}

fn read_collection_types(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawCollectionType>, CatalogError> {
    type CollectionTuple = (
        String,
        String,
        String,
        Option<i64>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let mut collections = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       TYPE_NAME,
                       COLL_TYPE,
                       UPPER_BOUND,
                       ELEM_TYPE_MOD,
                       ELEM_TYPE_OWNER,
                       ELEM_TYPE_NAME,
                       LENGTH,
                       PRECISION,
                       SCALE,
                       CHARACTER_SET_NAME,
                       ELEM_STORAGE,
                       NULLS_STORED,
                       CHAR_USED
                FROM USER_COLL_TYPES
                ORDER BY TYPE_NAME
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER,
                       TYPE_NAME,
                       COLL_TYPE,
                       UPPER_BOUND,
                       ELEM_TYPE_MOD,
                       ELEM_TYPE_OWNER,
                       ELEM_TYPE_NAME,
                       LENGTH,
                       PRECISION,
                       SCALE,
                       CHARACTER_SET_NAME,
                       ELEM_STORAGE,
                       NULLS_STORED,
                       CHAR_USED
                FROM DBA_COLL_TYPES
                WHERE OWNER = :1
                ORDER BY OWNER, TYPE_NAME
                "
            }
        };
        let rows = connection.query_as::<CollectionTuple>(sql, &[owner])?;
        for row in rows {
            let (
                owner,
                type_name,
                collection_type,
                upper_bound,
                element_type_modifier,
                element_type_owner,
                element_type_name,
                length,
                precision,
                scale,
                character_set,
                element_storage,
                nulls_stored,
                char_used,
            ) = row?;
            collections.push(RawCollectionType {
                owner: owner.clone(),
                type_name: type_name.clone(),
                collection_type: collection_type.trim().to_owned(),
                upper_bound,
                element_type_modifier: normalize_optional_token(element_type_modifier),
                element_type_owner: normalize_optional_token(element_type_owner),
                element_type_name: required_catalog_token(
                    element_type_name,
                    &format!("collection element type for {owner}.{type_name}"),
                )?,
                length,
                precision,
                scale,
                character_set: normalize_optional_token(character_set),
                element_storage: normalize_optional_token(element_storage),
                nulls_stored: normalize_optional_token(nulls_stored),
                char_used: normalize_optional_token(char_used),
            });
        }
    }
    collections.sort_by(|left, right| {
        (&left.owner, &left.type_name).cmp(&(&right.owner, &right.type_name))
    });
    Ok(collections)
}

fn read_type_methods(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawTypeMethod>, CatalogError> {
    type MethodTuple = (
        String,
        String,
        String,
        i64,
        Option<String>,
        i64,
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let mut methods = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       TYPE_NAME,
                       METHOD_NAME,
                       METHOD_NO,
                       METHOD_TYPE,
                       PARAMETERS,
                       RESULTS,
                       FINAL,
                       INSTANTIABLE,
                       OVERRIDING,
                       INHERITED
                FROM USER_TYPE_METHODS
                ORDER BY TYPE_NAME, METHOD_NO
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER,
                       TYPE_NAME,
                       METHOD_NAME,
                       METHOD_NO,
                       METHOD_TYPE,
                       PARAMETERS,
                       RESULTS,
                       FINAL,
                       INSTANTIABLE,
                       OVERRIDING,
                       INHERITED
                FROM DBA_TYPE_METHODS
                WHERE OWNER = :1
                ORDER BY OWNER, TYPE_NAME, METHOD_NO
                "
            }
        };
        let rows = connection.query_as::<MethodTuple>(sql, &[owner])?;
        for row in rows {
            let (
                owner,
                type_name,
                name,
                method_number,
                method_type,
                parameter_count,
                result_count,
                final_method,
                instantiable,
                overriding,
                inherited,
            ) = row?;
            methods.push(RawTypeMethod {
                owner: owner.clone(),
                type_name: type_name.clone(),
                name: name.clone(),
                method_number,
                method_type: required_catalog_token(
                    method_type,
                    &format!("method type for {owner}.{type_name}.{name}"),
                )?,
                parameter_count,
                result_count,
                final_method: required_catalog_token(
                    final_method,
                    &format!("final flag for {owner}.{type_name}.{name}"),
                )?,
                instantiable: required_catalog_token(
                    instantiable,
                    &format!("instantiable flag for {owner}.{type_name}.{name}"),
                )?,
                overriding: required_catalog_token(
                    overriding,
                    &format!("overriding flag for {owner}.{type_name}.{name}"),
                )?,
                inherited: required_catalog_token(
                    inherited,
                    &format!("inherited flag for {owner}.{type_name}.{name}"),
                )?,
            });
        }
    }
    methods.sort_by(|left, right| {
        (&left.owner, &left.type_name, left.method_number).cmp(&(
            &right.owner,
            &right.type_name,
            right.method_number,
        ))
    });
    Ok(methods)
}

fn read_type_method_parameters(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawTypeMethodParameter>, CatalogError> {
    type ParameterTuple = (
        String,
        String,
        String,
        i64,
        String,
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    type ResultTuple = (
        String,
        String,
        String,
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let mut parameters = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let parameter_sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       TYPE_NAME,
                       METHOD_NAME,
                       METHOD_NO,
                       PARAM_NAME,
                       PARAM_NO,
                       PARAM_MODE,
                       PARAM_TYPE_MOD,
                       PARAM_TYPE_OWNER,
                       PARAM_TYPE_NAME,
                       CHARACTER_SET_NAME
                FROM USER_METHOD_PARAMS
                ORDER BY TYPE_NAME, METHOD_NO, PARAM_NO
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER,
                       TYPE_NAME,
                       METHOD_NAME,
                       METHOD_NO,
                       PARAM_NAME,
                       PARAM_NO,
                       PARAM_MODE,
                       PARAM_TYPE_MOD,
                       PARAM_TYPE_OWNER,
                       PARAM_TYPE_NAME,
                       CHARACTER_SET_NAME
                FROM DBA_METHOD_PARAMS
                WHERE OWNER = :1
                ORDER BY OWNER, TYPE_NAME, METHOD_NO, PARAM_NO
                "
            }
        };
        let rows = connection.query_as::<ParameterTuple>(parameter_sql, &[owner])?;
        for row in rows {
            let (
                owner,
                type_name,
                method_name,
                method_number,
                name,
                position,
                mode,
                type_modifier,
                data_type_owner,
                data_type_name,
                character_set,
            ) = row?;
            parameters.push(RawTypeMethodParameter {
                owner: owner.clone(),
                type_name: type_name.clone(),
                method_name: method_name.clone(),
                method_number,
                name,
                position,
                mode: required_catalog_token(
                    mode,
                    &format!("method parameter mode for {owner}.{type_name}.{method_name}"),
                )?,
                type_modifier: normalize_optional_token(type_modifier),
                data_type_owner: normalize_optional_token(data_type_owner),
                data_type_name: required_catalog_token(
                    data_type_name,
                    &format!("method parameter type for {owner}.{type_name}.{method_name}"),
                )?,
                character_set: normalize_optional_token(character_set),
                return_value: false,
            });
        }

        prepare_call(connection, deadline)?;
        let result_sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       TYPE_NAME,
                       METHOD_NAME,
                       METHOD_NO,
                       RESULT_TYPE_MOD,
                       RESULT_TYPE_OWNER,
                       RESULT_TYPE_NAME,
                       CHARACTER_SET_NAME
                FROM USER_METHOD_RESULTS
                ORDER BY TYPE_NAME, METHOD_NO
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER,
                       TYPE_NAME,
                       METHOD_NAME,
                       METHOD_NO,
                       RESULT_TYPE_MOD,
                       RESULT_TYPE_OWNER,
                       RESULT_TYPE_NAME,
                       CHARACTER_SET_NAME
                FROM DBA_METHOD_RESULTS
                WHERE OWNER = :1
                ORDER BY OWNER, TYPE_NAME, METHOD_NO
                "
            }
        };
        let rows = connection.query_as::<ResultTuple>(result_sql, &[owner])?;
        for row in rows {
            let (
                owner,
                type_name,
                method_name,
                method_number,
                type_modifier,
                data_type_owner,
                data_type_name,
                character_set,
            ) = row?;
            parameters.push(RawTypeMethodParameter {
                owner: owner.clone(),
                type_name: type_name.clone(),
                method_name: method_name.clone(),
                method_number,
                name: "RETURN".to_owned(),
                position: 0,
                mode: "OUT".to_owned(),
                type_modifier: normalize_optional_token(type_modifier),
                data_type_owner: normalize_optional_token(data_type_owner),
                data_type_name: required_catalog_token(
                    data_type_name,
                    &format!("method result type for {owner}.{type_name}.{method_name}"),
                )?,
                character_set: normalize_optional_token(character_set),
                return_value: true,
            });
        }
    }
    parameters.sort_by(|left, right| {
        (
            &left.owner,
            &left.type_name,
            left.method_number,
            left.position,
            &left.name,
        )
            .cmp(&(
                &right.owner,
                &right.type_name,
                right.method_number,
                right.position,
                &right.name,
            ))
    });
    Ok(parameters)
}

fn read_triggers(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawTrigger>, CatalogError> {
    type TriggerTuple = (
        String,
        String,
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let mut triggers = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       TRIGGER_NAME,
                       TRIGGER_TYPE,
                       TRIGGERING_EVENT,
                       TABLE_OWNER,
                       BASE_OBJECT_TYPE,
                       TABLE_NAME,
                       COLUMN_NAME,
                       REFERENCING_NAMES,
                       WHEN_CLAUSE,
                       STATUS,
                       DESCRIPTION,
                       ACTION_TYPE,
                       TRIGGER_BODY,
                       CROSSEDITION,
                       FIRE_ONCE,
                       APPLY_SERVER_ONLY
                FROM USER_TRIGGERS
                ORDER BY TRIGGER_NAME
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER,
                       TRIGGER_NAME,
                       TRIGGER_TYPE,
                       TRIGGERING_EVENT,
                       TABLE_OWNER,
                       BASE_OBJECT_TYPE,
                       TABLE_NAME,
                       COLUMN_NAME,
                       REFERENCING_NAMES,
                       WHEN_CLAUSE,
                       STATUS,
                       DESCRIPTION,
                       ACTION_TYPE,
                       TRIGGER_BODY,
                       CROSSEDITION,
                       FIRE_ONCE,
                       APPLY_SERVER_ONLY
                FROM DBA_TRIGGERS
                WHERE OWNER = :1
                ORDER BY OWNER, TRIGGER_NAME
                "
            }
        };
        let rows = connection.query_as::<TriggerTuple>(sql, &[owner])?;
        for row in rows {
            let (
                owner,
                name,
                trigger_type,
                triggering_event,
                table_owner,
                base_object_type,
                table_name,
                column_name,
                referencing_names,
                when_clause,
                status,
                description,
                action_type,
                body,
                crossedition,
                fire_once,
                apply_server_only,
            ) = row?;
            triggers.push(RawTrigger {
                owner,
                name,
                trigger_type: trigger_type.trim().to_owned(),
                triggering_event: triggering_event.trim().to_owned(),
                table_owner: normalize_optional_token(table_owner),
                base_object_type: base_object_type.trim().to_owned(),
                table_name: normalize_optional_token(table_name),
                column_name: normalize_optional_token(column_name),
                referencing_names: normalize_optional_token(referencing_names),
                when_clause: normalize_optional_token(when_clause),
                status: status.trim().to_owned(),
                description: normalize_definition(description)?,
                action_type: action_type.trim().to_owned(),
                body: normalize_definition(body)?,
                crossedition: normalize_optional_token(crossedition),
                fire_once: normalize_optional_token(fire_once),
                apply_server_only: normalize_optional_token(apply_server_only),
            });
        }
    }
    triggers.sort_by(|left, right| (&left.owner, &left.name).cmp(&(&right.owner, &right.name)));
    Ok(triggers)
}

fn read_routines(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawRoutine>, CatalogError> {
    type RoutineTuple = (
        String,
        String,
        i64,
        i64,
        Option<String>,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        Option<String>,
    );
    let mut routines = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       OBJECT_NAME,
                       OBJECT_ID,
                       SUBPROGRAM_ID,
                       OVERLOAD,
                       OBJECT_TYPE,
                       AGGREGATE,
                       PIPELINED,
                       PARALLEL,
                       INTERFACE,
                       DETERMINISTIC,
                       AUTHID,
                       POLYMORPHIC,
                       PROCEDURE_NAME
                FROM USER_PROCEDURES
                WHERE PROCEDURE_NAME IS NULL
                  AND OBJECT_TYPE IN ('FUNCTION', 'PROCEDURE')
                ORDER BY OBJECT_NAME, SUBPROGRAM_ID
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER,
                       OBJECT_NAME,
                       OBJECT_ID,
                       SUBPROGRAM_ID,
                       OVERLOAD,
                       OBJECT_TYPE,
                       AGGREGATE,
                       PIPELINED,
                       PARALLEL,
                       INTERFACE,
                       DETERMINISTIC,
                       AUTHID,
                       POLYMORPHIC,
                       PROCEDURE_NAME
                FROM DBA_PROCEDURES
                WHERE OWNER = :1
                  AND PROCEDURE_NAME IS NULL
                  AND OBJECT_TYPE IN ('FUNCTION', 'PROCEDURE')
                ORDER BY OWNER, OBJECT_NAME, SUBPROGRAM_ID
                "
            }
        };
        let rows = connection.query_as::<RoutineTuple>(sql, &[owner])?;
        for row in rows {
            let (
                owner,
                name,
                object_id,
                subprogram_id,
                overload,
                object_type,
                aggregate,
                pipelined,
                parallel,
                interface,
                deterministic,
                authid,
                polymorphic,
                procedure_name,
            ) = row?;
            if procedure_name.is_some() {
                return Err(CatalogError::Mapping(format!(
                    "Oracle standalone routine {}.{} unexpectedly has PROCEDURE_NAME metadata",
                    owner, name
                )));
            }
            routines.push(RawRoutine {
                owner,
                name,
                object_id,
                subprogram_id,
                overload: normalize_optional_token(overload),
                object_type: object_type.trim().to_owned(),
                aggregate: aggregate.trim() == "YES",
                pipelined: pipelined.trim() == "YES",
                parallel: parallel.trim() == "YES",
                interface: interface.trim() == "YES",
                deterministic: deterministic.trim() == "YES",
                authid: authid.trim().to_owned(),
                polymorphic: match polymorphic.trim() {
                    "" | "NULL" => None,
                    value => Some(value.to_owned()),
                },
                definition: None,
            });
        }
    }
    routines.sort_by(|left, right| {
        (&left.owner, &left.name, left.subprogram_id).cmp(&(
            &right.owner,
            &right.name,
            right.subprogram_id,
        ))
    });
    Ok(routines)
}

fn attach_routine_sources(
    connection: &Connection,
    scope: &DictionaryScope,
    routines: &mut [RawRoutine],
    deadline: Instant,
) -> Result<(), CatalogError> {
    let positions = routines
        .iter()
        .enumerate()
        .map(|(position, routine)| {
            (
                (
                    routine.owner.clone(),
                    routine.name.clone(),
                    routine.object_type.clone(),
                ),
                position,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut sources = BTreeMap::<usize, String>::new();
    let mut last_lines = BTreeMap::<usize, i64>::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1, NAME, TYPE, LINE, TEXT
                FROM USER_SOURCE
                WHERE TYPE IN ('FUNCTION', 'PROCEDURE')
                ORDER BY NAME, TYPE, LINE
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER, NAME, TYPE, LINE, TEXT
                FROM DBA_SOURCE
                WHERE OWNER = :1
                  AND TYPE IN ('FUNCTION', 'PROCEDURE')
                ORDER BY OWNER, NAME, TYPE, LINE
                "
            }
        };
        let rows =
            connection.query_as::<(String, String, String, i64, Option<String>)>(sql, &[owner])?;
        for row in rows {
            let (source_owner, name, object_type, line, text) = row?;
            let position = positions
                .get(&(source_owner.clone(), name.clone(), object_type.clone()))
                .copied()
                .ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "Oracle source {}.{} ({object_type}) has no routine header",
                        source_owner, name
                    ))
                })?;
            let expected_line = last_lines.get(&position).copied().unwrap_or(0) + 1;
            if line != expected_line {
                return Err(CatalogError::Mapping(format!(
                    "Oracle routine source {}.{} expected line {expected_line}, found {line}",
                    source_owner, name
                )));
            }
            last_lines.insert(position, line);
            let source = sources.entry(position).or_default();
            source.push_str(text.as_deref().unwrap_or_default());
            if source.len() > MAX_DEFINITION_BYTES {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "Oracle routine definition exceeds the {MAX_DEFINITION_BYTES}-byte safety limit for {}.{}",
                    source_owner, name
                )));
            }
        }
    }
    for (position, routine) in routines.iter_mut().enumerate() {
        routine.definition = normalize_definition(sources.remove(&position))?;
        if routine.definition.is_none() {
            return Err(CatalogError::Mapping(format!(
                "Oracle routine {}.{} has no complete source",
                routine.owner, routine.name
            )));
        }
    }
    Ok(())
}

fn read_packages(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawPackage>, CatalogError> {
    let mut packages = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       OBJECT_NAME,
                       OBJECT_ID,
                       SUBPROGRAM_ID,
                       OVERLOAD,
                       OBJECT_TYPE,
                       AUTHID,
                       PROCEDURE_NAME
                FROM USER_PROCEDURES
                WHERE PROCEDURE_NAME IS NULL
                  AND OBJECT_TYPE = 'PACKAGE'
                ORDER BY OBJECT_NAME
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER,
                       OBJECT_NAME,
                       OBJECT_ID,
                       SUBPROGRAM_ID,
                       OVERLOAD,
                       OBJECT_TYPE,
                       AUTHID,
                       PROCEDURE_NAME
                FROM DBA_PROCEDURES
                WHERE OWNER = :1
                  AND PROCEDURE_NAME IS NULL
                  AND OBJECT_TYPE = 'PACKAGE'
                ORDER BY OWNER, OBJECT_NAME
                "
            }
        };
        let rows = connection.query_as::<(
            String,
            String,
            i64,
            i64,
            Option<String>,
            String,
            String,
            Option<String>,
        )>(sql, &[owner])?;
        for row in rows {
            let (
                owner,
                name,
                object_id,
                subprogram_id,
                overload,
                object_type,
                authid,
                procedure_name,
            ) = row?;
            if subprogram_id != 0
                || overload.is_some()
                || object_type.trim() != "PACKAGE"
                || procedure_name.is_some()
            {
                return Err(CatalogError::Mapping(format!(
                    "Oracle package header metadata is malformed for {}.{}",
                    owner, name
                )));
            }
            packages.push(RawPackage {
                owner,
                name,
                object_id,
                authid: authid.trim().to_owned(),
                specification: None,
                body: None,
            });
        }
    }
    packages.sort_by(|left, right| (&left.owner, &left.name).cmp(&(&right.owner, &right.name)));
    Ok(packages)
}

fn attach_package_sources(
    connection: &Connection,
    scope: &DictionaryScope,
    packages: &mut [RawPackage],
    deadline: Instant,
) -> Result<(), CatalogError> {
    let positions = packages
        .iter()
        .enumerate()
        .map(|(position, package)| ((package.owner.clone(), package.name.clone()), position))
        .collect::<BTreeMap<_, _>>();
    let mut sources = BTreeMap::<(usize, String), String>::new();
    let mut last_lines = BTreeMap::<(usize, String), i64>::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1, NAME, TYPE, LINE, TEXT
                FROM USER_SOURCE
                WHERE TYPE IN ('PACKAGE', 'PACKAGE BODY')
                ORDER BY NAME, TYPE, LINE
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER, NAME, TYPE, LINE, TEXT
                FROM DBA_SOURCE
                WHERE OWNER = :1
                  AND TYPE IN ('PACKAGE', 'PACKAGE BODY')
                ORDER BY OWNER, NAME, TYPE, LINE
                "
            }
        };
        let rows =
            connection.query_as::<(String, String, String, i64, Option<String>)>(sql, &[owner])?;
        for row in rows {
            let (source_owner, name, object_type, line, text) = row?;
            let position = positions
                .get(&(source_owner.clone(), name.clone()))
                .copied()
                .ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "Oracle package source {}.{} ({object_type}) has no package header",
                        source_owner, name
                    ))
                })?;
            let source_key = (position, object_type.clone());
            let expected_line = last_lines.get(&source_key).copied().unwrap_or(0) + 1;
            if line != expected_line {
                return Err(CatalogError::Mapping(format!(
                    "Oracle package source {}.{} ({object_type}) expected line {expected_line}, found {line}",
                    source_owner, name
                )));
            }
            last_lines.insert(source_key.clone(), line);
            let source = sources.entry(source_key).or_default();
            source.push_str(text.as_deref().unwrap_or_default());
            if source.len() > MAX_DEFINITION_BYTES {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "Oracle package definition exceeds the {MAX_DEFINITION_BYTES}-byte safety limit for {}.{} ({object_type})",
                    source_owner, name
                )));
            }
        }
    }
    for (position, package) in packages.iter_mut().enumerate() {
        package.specification =
            normalize_definition(sources.remove(&(position, "PACKAGE".to_owned())))?;
        package.body =
            normalize_definition(sources.remove(&(position, "PACKAGE BODY".to_owned())))?;
        if package.specification.is_none() {
            return Err(CatalogError::Mapping(format!(
                "Oracle package {}.{} has no complete specification",
                package.owner, package.name
            )));
        }
    }
    Ok(())
}

fn read_package_routines(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawPackageRoutine>, CatalogError> {
    type PackageRoutineTuple = (
        String,
        String,
        String,
        i64,
        i64,
        Option<String>,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    );
    let mut routines = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       OBJECT_NAME,
                       PROCEDURE_NAME,
                       OBJECT_ID,
                       SUBPROGRAM_ID,
                       OVERLOAD,
                       AGGREGATE,
                       PIPELINED,
                       PARALLEL,
                       INTERFACE,
                       DETERMINISTIC,
                       AUTHID,
                       POLYMORPHIC
                FROM USER_PROCEDURES
                WHERE PROCEDURE_NAME IS NOT NULL
                  AND OBJECT_TYPE = 'PACKAGE'
                ORDER BY OBJECT_NAME, SUBPROGRAM_ID
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER,
                       OBJECT_NAME,
                       PROCEDURE_NAME,
                       OBJECT_ID,
                       SUBPROGRAM_ID,
                       OVERLOAD,
                       AGGREGATE,
                       PIPELINED,
                       PARALLEL,
                       INTERFACE,
                       DETERMINISTIC,
                       AUTHID,
                       POLYMORPHIC
                FROM DBA_PROCEDURES
                WHERE OWNER = :1
                  AND PROCEDURE_NAME IS NOT NULL
                  AND OBJECT_TYPE = 'PACKAGE'
                ORDER BY OWNER, OBJECT_NAME, SUBPROGRAM_ID
                "
            }
        };
        let rows = connection.query_as::<PackageRoutineTuple>(sql, &[owner])?;
        for row in rows {
            let (
                owner,
                package,
                name,
                object_id,
                subprogram_id,
                overload,
                aggregate,
                pipelined,
                parallel,
                interface,
                deterministic,
                authid,
                polymorphic,
            ) = row?;
            routines.push(RawPackageRoutine {
                owner,
                package,
                name,
                object_id,
                subprogram_id,
                overload: normalize_optional_token(overload),
                aggregate: aggregate.trim() == "YES",
                pipelined: pipelined.trim() == "YES",
                parallel: parallel.trim() == "YES",
                interface: interface.trim() == "YES",
                deterministic: deterministic.trim() == "YES",
                authid: authid.trim().to_owned(),
                polymorphic: match polymorphic.trim() {
                    "" | "NULL" => None,
                    value => Some(value.to_owned()),
                },
            });
        }
    }
    routines.sort_by(|left, right| {
        (&left.owner, &left.package, left.subprogram_id).cmp(&(
            &right.owner,
            &right.package,
            right.subprogram_id,
        ))
    });
    Ok(routines)
}

fn read_routine_arguments(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawRoutineArgument>, CatalogError> {
    read_arguments(connection, scope, deadline, false)
}

fn read_package_arguments(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawRoutineArgument>, CatalogError> {
    read_arguments(connection, scope, deadline, true)
}

fn read_arguments(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
    packaged: bool,
) -> Result<Vec<RawRoutineArgument>, CatalogError> {
    type ArgumentTuple = (
        String,
        String,
        Option<String>,
        Option<String>,
        i64,
        i64,
        i64,
        Option<String>,
        String,
        Option<i64>,
        Option<String>,
        String,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<i64>,
        Option<String>,
        i64,
        Option<String>,
    );
    let mut arguments = Vec::new();
    let package_predicate = if packaged { "IS NOT NULL" } else { "IS NULL" };
    let user_package_inventory_predicate = if packaged {
        "AND EXISTS (
                    SELECT 1
                    FROM USER_OBJECTS package_object
                    WHERE package_object.OBJECT_ID = USER_ARGUMENTS.OBJECT_ID
                      AND package_object.OBJECT_TYPE = 'PACKAGE'
                 )"
    } else {
        ""
    };
    let dba_package_inventory_predicate = if packaged {
        "AND EXISTS (
                    SELECT 1
                    FROM DBA_OBJECTS package_object
                    WHERE package_object.OWNER = DBA_ARGUMENTS.OWNER
                      AND package_object.OBJECT_ID = DBA_ARGUMENTS.OBJECT_ID
                      AND package_object.OBJECT_TYPE = 'PACKAGE'
                 )"
    } else {
        ""
    };
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                format!(
                    "
                SELECT :1,
                       OBJECT_NAME,
                       PACKAGE_NAME,
                       ARGUMENT_NAME,
                       POSITION,
                       SEQUENCE,
                       DATA_LEVEL,
                       DATA_TYPE,
                       DEFAULTED,
                       DEFAULT_LENGTH,
                       DEFAULT_VALUE,
                       IN_OUT,
                       DATA_LENGTH,
                       DATA_PRECISION,
                       DATA_SCALE,
                       TYPE_OWNER,
                       TYPE_NAME,
                       TYPE_SUBNAME,
                       PLS_TYPE,
                       CHAR_LENGTH,
                       CHAR_USED,
                       SUBPROGRAM_ID,
                       OVERLOAD
                FROM USER_ARGUMENTS
                WHERE PACKAGE_NAME {package_predicate}
                  {user_package_inventory_predicate}
                ORDER BY OBJECT_NAME, SUBPROGRAM_ID, SEQUENCE
                "
                )
            }
            DictionaryScopeMode::Dba => {
                format!(
                    "
                SELECT OWNER,
                       OBJECT_NAME,
                       PACKAGE_NAME,
                       ARGUMENT_NAME,
                       POSITION,
                       SEQUENCE,
                       DATA_LEVEL,
                       DATA_TYPE,
                       DEFAULTED,
                       DEFAULT_LENGTH,
                       DEFAULT_VALUE,
                       IN_OUT,
                       DATA_LENGTH,
                       DATA_PRECISION,
                       DATA_SCALE,
                       TYPE_OWNER,
                       TYPE_NAME,
                       TYPE_SUBNAME,
                       PLS_TYPE,
                       CHAR_LENGTH,
                       CHAR_USED,
                       SUBPROGRAM_ID,
                       OVERLOAD
                FROM DBA_ARGUMENTS
                WHERE OWNER = :1
                  AND PACKAGE_NAME {package_predicate}
                  {dba_package_inventory_predicate}
                ORDER BY OWNER, OBJECT_NAME, SUBPROGRAM_ID, SEQUENCE
                "
                )
            }
        };
        let rows = connection.query_as::<ArgumentTuple>(&sql, &[owner])?;
        for row in rows {
            let (
                owner,
                routine,
                package_name,
                name,
                position,
                sequence,
                data_level,
                data_type,
                defaulted,
                default_length,
                default_value,
                mode,
                data_length,
                data_precision,
                data_scale,
                type_owner,
                type_name,
                type_subname,
                pls_type,
                char_length,
                char_used,
                subprogram_id,
                overload,
            ) = row?;
            arguments.push(RawRoutineArgument {
                owner,
                routine,
                package_name: normalize_optional_token(package_name),
                name: normalize_optional_token(name),
                position,
                sequence,
                data_level,
                data_type: normalize_optional_token(data_type),
                defaulted: defaulted.trim() == "Y",
                default_length,
                default_value: normalize_definition(default_value)?,
                mode: mode.trim().to_owned(),
                data_length,
                data_precision,
                data_scale,
                type_owner: normalize_optional_token(type_owner),
                type_name: normalize_optional_token(type_name),
                type_subname: normalize_optional_token(type_subname),
                pls_type: normalize_optional_token(pls_type),
                char_length,
                char_used: normalize_optional_token(char_used),
                subprogram_id,
                overload: normalize_optional_token(overload),
            });
        }
    }
    arguments.sort_by(|left, right| {
        (
            &left.owner,
            &left.routine,
            left.subprogram_id,
            left.sequence,
        )
            .cmp(&(
                &right.owner,
                &right.routine,
                right.subprogram_id,
                right.sequence,
            ))
    });
    Ok(arguments)
}

fn read_view_columns(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawColumn>, CatalogError> {
    type ColumnTuple = (
        String,
        String,
        String,
        Option<i64>,
        i64,
        String,
        Option<String>,
        i64,
        Option<i64>,
        Option<i64>,
        String,
        Option<String>,
        String,
        String,
        String,
        String,
        String,
        Option<i64>,
        Option<String>,
        Option<String>,
    );
    let mut columns = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       c.TABLE_NAME,
                       c.COLUMN_NAME,
                       c.COLUMN_ID,
                       c.INTERNAL_COLUMN_ID,
                       c.DATA_TYPE,
                       c.DATA_TYPE_OWNER,
                       c.DATA_LENGTH,
                       c.DATA_PRECISION,
                       c.DATA_SCALE,
                       c.NULLABLE,
                       c.DATA_DEFAULT,
                       c.HIDDEN_COLUMN,
                       c.VIRTUAL_COLUMN,
                       c.USER_GENERATED,
                       c.DEFAULT_ON_NULL,
                       c.IDENTITY_COLUMN,
                       c.CHAR_LENGTH,
                       c.CHAR_USED,
                       c.COLLATION
                FROM USER_TAB_COLS c
                JOIN USER_VIEWS v ON v.VIEW_NAME = c.TABLE_NAME
                ORDER BY c.TABLE_NAME, c.INTERNAL_COLUMN_ID
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT c.OWNER,
                       c.TABLE_NAME,
                       c.COLUMN_NAME,
                       c.COLUMN_ID,
                       c.INTERNAL_COLUMN_ID,
                       c.DATA_TYPE,
                       c.DATA_TYPE_OWNER,
                       c.DATA_LENGTH,
                       c.DATA_PRECISION,
                       c.DATA_SCALE,
                       c.NULLABLE,
                       c.DATA_DEFAULT,
                       c.HIDDEN_COLUMN,
                       c.VIRTUAL_COLUMN,
                       c.USER_GENERATED,
                       c.DEFAULT_ON_NULL,
                       c.IDENTITY_COLUMN,
                       c.CHAR_LENGTH,
                       c.CHAR_USED,
                       c.COLLATION
                FROM DBA_TAB_COLS c
                JOIN DBA_VIEWS v
                  ON v.OWNER = c.OWNER
                 AND v.VIEW_NAME = c.TABLE_NAME
                WHERE c.OWNER = :1
                ORDER BY c.OWNER, c.TABLE_NAME, c.INTERNAL_COLUMN_ID
                "
            }
        };
        let rows = connection.query_as::<ColumnTuple>(sql, &[owner])?;
        for row in rows {
            let (
                owner,
                table,
                name,
                column_id,
                internal_column_id,
                data_type,
                data_type_owner,
                data_length,
                data_precision,
                data_scale,
                nullable,
                default_value,
                hidden,
                virtual_column,
                user_generated,
                default_on_null,
                identity,
                char_length,
                char_used,
                collation,
            ) = row?;
            columns.push(RawColumn {
                owner,
                table,
                name,
                column_id,
                internal_column_id,
                data_type,
                data_type_owner,
                data_length,
                data_precision,
                data_scale,
                nullable: nullable == "Y",
                default_value: normalize_definition(default_value)?,
                hidden: hidden == "YES",
                virtual_column: virtual_column == "YES",
                user_generated: user_generated == "YES",
                default_on_null: default_on_null == "YES",
                identity: identity == "YES",
                char_length,
                char_used,
                collation,
            });
        }
    }
    columns.sort_by(|left, right| {
        (&left.owner, &left.table, left.internal_column_id).cmp(&(
            &right.owner,
            &right.table,
            right.internal_column_id,
        ))
    });
    Ok(columns)
}

