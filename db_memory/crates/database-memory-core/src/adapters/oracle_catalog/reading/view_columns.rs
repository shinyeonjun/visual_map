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
