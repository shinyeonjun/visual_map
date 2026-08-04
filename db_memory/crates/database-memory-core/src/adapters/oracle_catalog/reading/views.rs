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
