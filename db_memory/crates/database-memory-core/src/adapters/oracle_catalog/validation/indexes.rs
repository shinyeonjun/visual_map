fn read_indexes(
    connection: &Connection,
    scope: &DictionaryScope,
    recycle: &BTreeSet<(String, String)>,
    deadline: Instant,
) -> Result<Vec<RawIndex>, CatalogError> {
    type IndexTuple = (
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
        String,
        String,
        Option<String>,
        String,
    );
    let mut indexes = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       i.TABLE_OWNER,
                       i.TABLE_NAME,
                       i.INDEX_NAME,
                       i.INDEX_TYPE,
                       i.UNIQUENESS,
                       i.STATUS,
                       i.PARTITIONED,
                       i.TEMPORARY,
                       i.GENERATED,
                       i.SECONDARY,
                       i.VISIBILITY,
                       i.FUNCIDX_STATUS,
                       i.CONSTRAINT_INDEX
                FROM USER_INDEXES i
                JOIN USER_TABLES t ON t.TABLE_NAME = i.TABLE_NAME
                WHERE t.SECONDARY = 'N'
                  AND t.DROPPED = 'NO'
                  AND i.INDEX_TYPE <> 'LOB'
                ORDER BY i.TABLE_NAME, i.INDEX_NAME
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT i.OWNER,
                       i.TABLE_OWNER,
                       i.TABLE_NAME,
                       i.INDEX_NAME,
                       i.INDEX_TYPE,
                       i.UNIQUENESS,
                       i.STATUS,
                       i.PARTITIONED,
                       i.TEMPORARY,
                       i.GENERATED,
                       i.SECONDARY,
                       i.VISIBILITY,
                       i.FUNCIDX_STATUS,
                       i.CONSTRAINT_INDEX
                FROM DBA_INDEXES i
                JOIN DBA_TABLES t
                  ON t.OWNER = i.TABLE_OWNER
                 AND t.TABLE_NAME = i.TABLE_NAME
                WHERE i.TABLE_OWNER = :1
                  AND t.SECONDARY = 'N'
                  AND t.DROPPED = 'NO'
                  AND i.INDEX_TYPE <> 'LOB'
                ORDER BY i.OWNER, i.TABLE_NAME, i.INDEX_NAME
                "
            }
        };
        let rows = connection.query_as::<IndexTuple>(sql, &[owner])?;
        for row in rows {
            let (
                index_owner,
                table_owner,
                table,
                name,
                index_type,
                uniqueness,
                status,
                partitioned,
                temporary,
                generated,
                secondary,
                visibility,
                function_status,
                constraint_index,
            ) = row?;
            if recycle.contains(&(table_owner.clone(), table.clone())) {
                continue;
            }
            indexes.push(RawIndex {
                owner: index_owner,
                table_owner,
                table,
                name,
                index_type,
                unique: uniqueness == "UNIQUE",
                status,
                partitioned: partitioned == "YES",
                temporary: temporary == "Y",
                generated: generated == "Y",
                secondary: secondary == "Y",
                visibility,
                function_status,
                constraint_index: constraint_index == "YES",
                columns: Vec::new(),
            });
        }
    }
    indexes.sort_by(|left, right| {
        (&left.owner, &left.table_owner, &left.table, &left.name).cmp(&(
            &right.owner,
            &right.table_owner,
            &right.table,
            &right.name,
        ))
    });
    Ok(indexes)
}

fn attach_index_columns(
    connection: &Connection,
    scope: &DictionaryScope,
    indexes: &mut [RawIndex],
    deadline: Instant,
) -> Result<(), CatalogError> {
    let mut positions = BTreeMap::new();
    for (position, index) in indexes.iter().enumerate() {
        let identity = (index.owner.clone(), index.name.clone());
        if positions.insert(identity.clone(), position).is_some() {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle index identity {}.{}",
                identity.0, identity.1
            )));
        }
    }

    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       ic.INDEX_NAME,
                       :1,
                       ic.TABLE_NAME,
                       ic.COLUMN_NAME,
                       ic.COLUMN_POSITION,
                       ic.DESCEND
                FROM USER_IND_COLUMNS ic
                JOIN USER_INDEXES i
                  ON i.INDEX_NAME = ic.INDEX_NAME
                 AND i.TABLE_NAME = ic.TABLE_NAME
                JOIN USER_TABLES t ON t.TABLE_NAME = ic.TABLE_NAME
                WHERE t.SECONDARY = 'N'
                  AND t.DROPPED = 'NO'
                  AND i.INDEX_TYPE <> 'LOB'
                ORDER BY ic.TABLE_NAME, ic.INDEX_NAME, ic.COLUMN_POSITION
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT ic.INDEX_OWNER,
                       ic.INDEX_NAME,
                       ic.TABLE_OWNER,
                       ic.TABLE_NAME,
                       ic.COLUMN_NAME,
                       ic.COLUMN_POSITION,
                       ic.DESCEND
                FROM DBA_IND_COLUMNS ic
                JOIN DBA_INDEXES i
                  ON i.OWNER = ic.INDEX_OWNER
                 AND i.INDEX_NAME = ic.INDEX_NAME
                 AND i.TABLE_OWNER = ic.TABLE_OWNER
                 AND i.TABLE_NAME = ic.TABLE_NAME
                JOIN DBA_TABLES t
                  ON t.OWNER = ic.TABLE_OWNER
                 AND t.TABLE_NAME = ic.TABLE_NAME
                WHERE ic.TABLE_OWNER = :1
                  AND t.SECONDARY = 'N'
                  AND t.DROPPED = 'NO'
                  AND i.INDEX_TYPE <> 'LOB'
                ORDER BY ic.INDEX_OWNER, ic.TABLE_NAME, ic.INDEX_NAME, ic.COLUMN_POSITION
                "
            }
        };
        let rows = connection
            .query_as::<(String, String, String, String, String, i64, String)>(sql, &[owner])?;
        for row in rows {
            let (index_owner, index_name, table_owner, table_name, column_name, position, descend) =
                row?;
            let index = positions
                .get(&(index_owner.clone(), index_name.clone()))
                .copied()
                .ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "Oracle index column references missing header {}.{}",
                        index_owner, index_name
                    ))
                })?;
            if indexes[index].table_owner != table_owner || indexes[index].table != table_name {
                return Err(CatalogError::Mapping(format!(
                    "Oracle index column table mismatch for {}.{}",
                    index_owner, index_name
                )));
            }
            indexes[index].columns.push(RawIndexColumn {
                name: column_name,
                position,
                descending: descend == "DESC",
                expression: None,
            });
        }
    }
    for index in indexes {
        index.columns.sort_by_key(|column| column.position);
    }
    Ok(())
}

fn attach_index_expressions(
    connection: &Connection,
    scope: &DictionaryScope,
    indexes: &mut [RawIndex],
    deadline: Instant,
) -> Result<(), CatalogError> {
    let mut positions = BTreeMap::new();
    for (index_position, index) in indexes.iter().enumerate() {
        for (column_position, column) in index.columns.iter().enumerate() {
            let identity = (index.owner.clone(), index.name.clone(), column.position);
            if positions
                .insert(identity.clone(), (index_position, column_position))
                .is_some()
            {
                return Err(CatalogError::Mapping(format!(
                    "duplicate Oracle index-expression position {} for {}.{}",
                    identity.2, identity.0, identity.1
                )));
            }
        }
    }

    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       e.INDEX_NAME,
                       :1,
                       e.TABLE_NAME,
                       e.COLUMN_EXPRESSION,
                       e.COLUMN_POSITION
                FROM USER_IND_EXPRESSIONS e
                JOIN USER_INDEXES i
                  ON i.INDEX_NAME = e.INDEX_NAME
                 AND i.TABLE_NAME = e.TABLE_NAME
                JOIN USER_TABLES t ON t.TABLE_NAME = e.TABLE_NAME
                WHERE t.SECONDARY = 'N'
                  AND t.DROPPED = 'NO'
                  AND i.INDEX_TYPE <> 'LOB'
                ORDER BY e.TABLE_NAME, e.INDEX_NAME, e.COLUMN_POSITION
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT e.INDEX_OWNER,
                       e.INDEX_NAME,
                       e.TABLE_OWNER,
                       e.TABLE_NAME,
                       e.COLUMN_EXPRESSION,
                       e.COLUMN_POSITION
                FROM DBA_IND_EXPRESSIONS e
                JOIN DBA_INDEXES i
                  ON i.OWNER = e.INDEX_OWNER
                 AND i.INDEX_NAME = e.INDEX_NAME
                 AND i.TABLE_OWNER = e.TABLE_OWNER
                 AND i.TABLE_NAME = e.TABLE_NAME
                JOIN DBA_TABLES t
                  ON t.OWNER = e.TABLE_OWNER
                 AND t.TABLE_NAME = e.TABLE_NAME
                WHERE e.TABLE_OWNER = :1
                  AND t.SECONDARY = 'N'
                  AND t.DROPPED = 'NO'
                  AND i.INDEX_TYPE <> 'LOB'
                ORDER BY e.INDEX_OWNER, e.TABLE_NAME, e.INDEX_NAME, e.COLUMN_POSITION
                "
            }
        };
        let rows = connection
            .query_as::<(String, String, String, String, Option<String>, i64)>(sql, &[owner])?;
        for row in rows {
            let (index_owner, index_name, table_owner, table_name, expression, position) = row?;
            let (index_position, column_position) = positions
                .get(&(index_owner.clone(), index_name.clone(), position))
                .copied()
                .ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "Oracle index expression references missing key position {position} for {index_owner}.{index_name}"
                    ))
                })?;
            let index = &mut indexes[index_position];
            if index.table_owner != table_owner || index.table != table_name {
                return Err(CatalogError::Mapping(format!(
                    "Oracle index expression table mismatch for {index_owner}.{index_name}"
                )));
            }
            let expression = normalize_definition(expression)?.ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle index expression is empty for {index_owner}.{index_name} position {position}"
                ))
            })?;
            let column = &mut index.columns[column_position];
            if column.expression.replace(expression).is_some() {
                return Err(CatalogError::Mapping(format!(
                    "duplicate Oracle index expression for {index_owner}.{index_name} position {position}"
                )));
            }
        }
    }
    Ok(())
}
