fn normalize_definition(value: Option<String>) -> Result<Option<String>, CatalogError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let normalized = value.trim().to_owned();
    if normalized.len() > MAX_DEFINITION_BYTES {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "Oracle metadata definition exceeds the {MAX_DEFINITION_BYTES}-byte safety limit"
        )));
    }
    Ok((!normalized.is_empty()).then_some(normalized))
}

fn normalize_optional_token(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn required_catalog_token(value: Option<String>, subject: &str) -> Result<String, CatalogError> {
    normalize_optional_token(value)
        .ok_or_else(|| CatalogError::Mapping(format!("Oracle catalog is missing {subject}")))
}

fn ensure_yes_no(value: &str, subject: &str) -> Result<(), CatalogError> {
    if matches!(value, "YES" | "NO") {
        Ok(())
    } else {
        Err(CatalogError::Mapping(format!(
            "{subject} has unrecognized value '{value}'"
        )))
    }
}

fn ensure_user_type_reference(
    scope: &DictionaryScope,
    user_types: &BTreeMap<(String, String), &RawUserType>,
    owner: Option<&str>,
    name: &str,
    subject: &str,
) -> Result<(), CatalogError> {
    if name.trim().is_empty() {
        return Err(CatalogError::Mapping(format!(
            "{subject} has no data type name"
        )));
    }
    let Some(owner) = owner else {
        return Ok(());
    };
    ensure_reference_owner(scope, owner, subject)?;
    if user_types.contains_key(&(owner.to_owned(), name.to_owned())) {
        Ok(())
    } else {
        Err(CatalogError::Mapping(format!(
            "{subject} references missing type {owner}.{name}"
        )))
    }
}

fn reject_dynamic_plsql(kind: &str, name: &str, definition: &str) -> Result<(), CatalogError> {
    let words = oracle_plsql_words(definition)?;
    let execute_immediate = words
        .windows(2)
        .any(|words| words == ["EXECUTE", "IMMEDIATE"]);
    let dbms_sql = words.iter().any(|word| word == "DBMS_SQL");
    let execute_ddl = words
        .windows(2)
        .any(|words| words == ["DBMS_UTILITY", "EXEC_DDL_STATEMENT"]);
    let dynamic_open = words.iter().enumerate().any(|(index, word)| {
        if word != "OPEN" {
            return false;
        }
        let Some(for_offset) = words[index + 1..]
            .iter()
            .take(3)
            .position(|word| word == "FOR")
        else {
            return false;
        };
        !matches!(
            words.get(index + for_offset + 2).map(String::as_str),
            Some("SELECT" | "WITH")
        )
    });
    if execute_immediate || dbms_sql || execute_ddl || dynamic_open {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "Oracle {kind} {name} contains dynamic PL/SQL that prevents complete dependency proof"
        )));
    }
    Ok(())
}

fn oracle_plsql_words(source: &str) -> Result<Vec<String>, CatalogError> {
    let chars = source.chars().collect::<Vec<_>>();
    let mut words = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '-' && chars.get(index + 1) == Some(&'-') {
            index += 2;
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            continue;
        }
        if chars[index] == '/' && chars.get(index + 1) == Some(&'*') {
            index += 2;
            while index + 1 < chars.len() && !(chars[index] == '*' && chars[index + 1] == '/') {
                index += 1;
            }
            if index + 1 >= chars.len() {
                return Err(CatalogError::UnsupportedMetadata(
                    "Oracle PL/SQL contains an unterminated block comment".to_owned(),
                ));
            }
            index += 2;
            continue;
        }
        let q_delimiter_index =
            if matches!(chars[index], 'q' | 'Q') && chars.get(index + 1) == Some(&'\'') {
                Some(index + 2)
            } else if matches!(chars[index], 'n' | 'N')
                && matches!(chars.get(index + 1), Some('q' | 'Q'))
                && chars.get(index + 2) == Some(&'\'')
            {
                Some(index + 3)
            } else {
                None
            };
        if let Some(delimiter_index) = q_delimiter_index {
            let Some(opening) = chars.get(delimiter_index).copied() else {
                return Err(CatalogError::UnsupportedMetadata(
                    "Oracle PL/SQL contains an incomplete alternative-quoted literal".to_owned(),
                ));
            };
            let closing = match opening {
                '[' => ']',
                '{' => '}',
                '(' => ')',
                '<' => '>',
                other => other,
            };
            index = delimiter_index + 1;
            while index + 1 < chars.len() && !(chars[index] == closing && chars[index + 1] == '\'')
            {
                index += 1;
            }
            if index + 1 >= chars.len() {
                return Err(CatalogError::UnsupportedMetadata(
                    "Oracle PL/SQL contains an unterminated alternative-quoted literal".to_owned(),
                ));
            }
            index += 2;
            continue;
        }
        if chars[index] == '\'' {
            index += 1;
            loop {
                let Some(character) = chars.get(index) else {
                    return Err(CatalogError::UnsupportedMetadata(
                        "Oracle PL/SQL contains an unterminated string literal".to_owned(),
                    ));
                };
                if *character != '\'' {
                    index += 1;
                    continue;
                }
                if chars.get(index + 1) == Some(&'\'') {
                    index += 2;
                    continue;
                }
                index += 1;
                break;
            }
            continue;
        }
        if chars[index] == '"' {
            index += 1;
            loop {
                let Some(character) = chars.get(index) else {
                    return Err(CatalogError::UnsupportedMetadata(
                        "Oracle PL/SQL contains an unterminated quoted identifier".to_owned(),
                    ));
                };
                if *character != '"' {
                    index += 1;
                    continue;
                }
                if chars.get(index + 1) == Some(&'"') {
                    index += 2;
                    continue;
                }
                index += 1;
                break;
            }
            continue;
        }
        if chars[index].is_ascii_alphabetic() || matches!(chars[index], '_' | '$' | '#') {
            let start = index;
            index += 1;
            while index < chars.len()
                && (chars[index].is_ascii_alphanumeric() || matches!(chars[index], '_' | '$' | '#'))
            {
                index += 1;
            }
            words.push(
                chars[start..index]
                    .iter()
                    .collect::<String>()
                    .to_uppercase(),
            );
            continue;
        }
        index += 1;
    }
    Ok(words)
}

fn oracle_trigger_timing(trigger_type: &str) -> Result<String, CatalogError> {
    for timing in ["INSTEAD OF", "BEFORE", "AFTER", "COMPOUND"] {
        if trigger_type.starts_with(timing) {
            return Ok(timing.to_owned());
        }
    }
    Err(CatalogError::UnsupportedMetadata(format!(
        "Oracle trigger type '{trigger_type}' has no covered timing"
    )))
}

fn oracle_trigger_events(triggering_event: &str) -> Result<Vec<String>, CatalogError> {
    let events = triggering_event
        .split(" OR ")
        .map(str::trim)
        .filter(|event| !event.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if events.is_empty() {
        Err(CatalogError::Mapping(
            "Oracle trigger has no triggering events".to_owned(),
        ))
    } else {
        Ok(events)
    }
}

fn read_constraints(
    connection: &Connection,
    scope: &DictionaryScope,
    recycle: &BTreeSet<(String, String)>,
    deadline: Instant,
) -> Result<Vec<RawConstraint>, CatalogError> {
    type ConstraintTuple = (
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let mut constraints = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       c.TABLE_NAME,
                       c.CONSTRAINT_NAME,
                       c.CONSTRAINT_TYPE,
                       c.SEARCH_CONDITION,
                       c.R_OWNER,
                       c.R_CONSTRAINT_NAME,
                       c.DELETE_RULE,
                       c.STATUS,
                       c.DEFERRABLE,
                       c.DEFERRED,
                       c.VALIDATED,
                       c.GENERATED,
                       c.INDEX_OWNER,
                       c.INDEX_NAME,
                       c.INVALID,
                       c.VIEW_RELATED
                FROM USER_CONSTRAINTS c
                JOIN USER_TABLES t ON t.TABLE_NAME = c.TABLE_NAME
                WHERE t.SECONDARY = 'N'
                  AND t.DROPPED = 'NO'
                ORDER BY c.TABLE_NAME, c.CONSTRAINT_NAME
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT c.OWNER,
                       c.TABLE_NAME,
                       c.CONSTRAINT_NAME,
                       c.CONSTRAINT_TYPE,
                       c.SEARCH_CONDITION,
                       c.R_OWNER,
                       c.R_CONSTRAINT_NAME,
                       c.DELETE_RULE,
                       c.STATUS,
                       c.DEFERRABLE,
                       c.DEFERRED,
                       c.VALIDATED,
                       c.GENERATED,
                       c.INDEX_OWNER,
                       c.INDEX_NAME,
                       c.INVALID,
                       c.VIEW_RELATED
                FROM DBA_CONSTRAINTS c
                JOIN DBA_TABLES t
                  ON t.OWNER = c.OWNER
                 AND t.TABLE_NAME = c.TABLE_NAME
                WHERE c.OWNER = :1
                  AND t.SECONDARY = 'N'
                  AND t.DROPPED = 'NO'
                ORDER BY c.OWNER, c.TABLE_NAME, c.CONSTRAINT_NAME
                "
            }
        };
        let rows = connection.query_as::<ConstraintTuple>(sql, &[owner])?;
        for row in rows {
            let (
                owner,
                table,
                name,
                constraint_type,
                search_condition,
                referenced_owner,
                referenced_constraint,
                delete_rule,
                status,
                deferrable,
                deferred,
                validated,
                generated,
                index_owner,
                index_name,
                invalid,
                view_related,
            ) = row?;
            if recycle.contains(&(owner.clone(), table.clone())) {
                continue;
            }
            constraints.push(RawConstraint {
                owner,
                table,
                name,
                constraint_type,
                search_condition: normalize_definition(search_condition)?,
                referenced_owner,
                referenced_constraint,
                delete_rule,
                status,
                deferrable,
                deferred,
                validated,
                generated,
                index_owner,
                index_name,
                invalid,
                view_related,
                columns: Vec::new(),
            });
        }
    }
    constraints.sort_by(|left, right| {
        (&left.owner, &left.table, &left.name).cmp(&(&right.owner, &right.table, &right.name))
    });
    Ok(constraints)
}

fn attach_constraint_columns(
    connection: &Connection,
    scope: &DictionaryScope,
    constraints: &mut [RawConstraint],
    deadline: Instant,
) -> Result<(), CatalogError> {
    let mut positions = BTreeMap::new();
    for (position, constraint) in constraints.iter().enumerate() {
        let identity = (constraint.owner.clone(), constraint.name.clone());
        if positions.insert(identity.clone(), position).is_some() {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle constraint identity {}.{}",
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
                       cc.CONSTRAINT_NAME,
                       cc.TABLE_NAME,
                       cc.COLUMN_NAME,
                       cc.POSITION
                FROM USER_CONS_COLUMNS cc
                JOIN USER_CONSTRAINTS c
                  ON c.CONSTRAINT_NAME = cc.CONSTRAINT_NAME
                 AND c.TABLE_NAME = cc.TABLE_NAME
                JOIN USER_TABLES t ON t.TABLE_NAME = cc.TABLE_NAME
                WHERE t.SECONDARY = 'N'
                  AND t.DROPPED = 'NO'
                ORDER BY cc.TABLE_NAME, cc.CONSTRAINT_NAME, cc.POSITION
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT cc.OWNER,
                       cc.CONSTRAINT_NAME,
                       cc.TABLE_NAME,
                       cc.COLUMN_NAME,
                       cc.POSITION
                FROM DBA_CONS_COLUMNS cc
                JOIN DBA_CONSTRAINTS c
                  ON c.OWNER = cc.OWNER
                 AND c.CONSTRAINT_NAME = cc.CONSTRAINT_NAME
                 AND c.TABLE_NAME = cc.TABLE_NAME
                JOIN DBA_TABLES t
                  ON t.OWNER = cc.OWNER
                 AND t.TABLE_NAME = cc.TABLE_NAME
                WHERE cc.OWNER = :1
                  AND t.SECONDARY = 'N'
                  AND t.DROPPED = 'NO'
                ORDER BY cc.OWNER, cc.TABLE_NAME, cc.CONSTRAINT_NAME, cc.POSITION
                "
            }
        };
        let rows =
            connection.query_as::<(String, String, String, String, Option<i64>)>(sql, &[owner])?;
        for row in rows {
            let (column_owner, constraint_name, table_name, column_name, position) = row?;
            let index = positions
                .get(&(column_owner.clone(), constraint_name.clone()))
                .copied()
                .ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "Oracle constraint column references missing header {}.{}",
                        column_owner, constraint_name
                    ))
                })?;
            if constraints[index].table != table_name {
                return Err(CatalogError::Mapping(format!(
                    "Oracle constraint column table mismatch for {}.{}",
                    column_owner, constraint_name
                )));
            }
            constraints[index].columns.push(RawConstraintColumn {
                name: column_name,
                position,
            });
        }
    }
    for constraint in constraints {
        constraint.columns.sort_by(|left, right| {
            left.position
                .cmp(&right.position)
                .then_with(|| left.name.cmp(&right.name))
        });
    }
    Ok(())
}

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

fn normalize_partition_high_value(
    owner: &str,
    object: &str,
    partition: &str,
    length: i64,
    value: Option<String>,
) -> Result<Option<String>, CatalogError> {
    if length < 0 {
        return Err(CatalogError::Mapping(format!(
            "Oracle partition {owner}.{object}.{partition} has negative high-value length"
        )));
    }
    if length > MAX_DEFINITION_BYTES as i64 {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "Oracle partition boundary exceeds the {MAX_DEFINITION_BYTES}-byte safety limit for {owner}.{object}.{partition}"
        )));
    }
    normalize_definition(value)
}

fn read_dependencies(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawDependency>, CatalogError> {
    let mut dependencies = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       D.NAME,
                       D.TYPE,
                       D.REFERENCED_OWNER,
                       D.REFERENCED_NAME,
                       D.REFERENCED_TYPE,
                       D.REFERENCED_LINK_NAME,
                       D.DEPENDENCY_TYPE,
                       U.ORACLE_MAINTAINED
                FROM USER_DEPENDENCIES D
                LEFT JOIN ALL_USERS U ON U.USERNAME = D.REFERENCED_OWNER
                ORDER BY D.NAME, D.TYPE, D.REFERENCED_OWNER, D.REFERENCED_NAME, D.REFERENCED_TYPE
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT D.OWNER,
                       D.NAME,
                       D.TYPE,
                       D.REFERENCED_OWNER,
                       D.REFERENCED_NAME,
                       D.REFERENCED_TYPE,
                       D.REFERENCED_LINK_NAME,
                       D.DEPENDENCY_TYPE,
                       U.ORACLE_MAINTAINED
                FROM DBA_DEPENDENCIES D
                LEFT JOIN DBA_USERS U ON U.USERNAME = D.REFERENCED_OWNER
                WHERE D.OWNER = :1
                ORDER BY D.OWNER, D.NAME, D.TYPE, D.REFERENCED_OWNER, D.REFERENCED_NAME, D.REFERENCED_TYPE
                "
            }
        };
        let rows = connection.query_as::<(
            String,
            String,
            String,
            String,
            String,
            String,
            Option<String>,
            String,
            Option<String>,
        )>(sql, &[owner])?;
        for row in rows {
            let (
                owner,
                name,
                object_type,
                referenced_owner,
                referenced_name,
                referenced_type,
                referenced_link,
                dependency_type,
                referenced_owner_oracle_maintained,
            ) = row?;
            let referenced_owner_oracle_maintained = match referenced_owner_oracle_maintained
                .as_deref()
            {
                Some("Y") => true,
                Some("N") => false,
                value => {
                    return Err(CatalogError::Mapping(format!(
                        "Oracle dependency target owner {referenced_owner} has unprovable ORACLE_MAINTAINED state '{}'",
                        value.unwrap_or("missing")
                    )));
                }
            };
            dependencies.push(RawDependency {
                owner,
                name,
                object_type,
                referenced_owner,
                referenced_name,
                referenced_type,
                referenced_link,
                dependency_type,
                referenced_owner_oracle_maintained,
            });
        }
    }
    dependencies.sort_by(|left, right| {
        (
            &left.owner,
            &left.name,
            &left.object_type,
            &left.referenced_owner,
            &left.referenced_name,
            &left.referenced_type,
        )
            .cmp(&(
                &right.owner,
                &right.name,
                &right.object_type,
                &right.referenced_owner,
                &right.referenced_name,
                &right.referenced_type,
            ))
    });
    dependencies.dedup();
    Ok(dependencies)
}

fn oracle_package_dependency_groups(
    dependencies: &[RawDependency],
) -> BTreeMap<CollapsedDependencyIdentity, CollapsedDependencyEvidence> {
    let mut groups = BTreeMap::<CollapsedDependencyIdentity, CollapsedDependencyEvidence>::new();
    for dependency in dependencies.iter().filter(|dependency| {
        matches!(dependency.object_type.as_str(), "PACKAGE" | "PACKAGE BODY")
            && !dependency.referenced_owner_oracle_maintained
            && !(dependency.object_type == "PACKAGE BODY"
                && dependency.referenced_type == "PACKAGE"
                && dependency.owner == dependency.referenced_owner
                && dependency.name == dependency.referenced_name)
    }) {
        let evidence = groups
            .entry((
                dependency.owner.clone(),
                dependency.name.clone(),
                dependency.referenced_owner.clone(),
                dependency.referenced_name.clone(),
                dependency.referenced_type.clone(),
            ))
            .or_default();
        evidence
            .source_object_types
            .insert(dependency.object_type.clone());
        evidence
            .dependency_types
            .insert(dependency.dependency_type.clone());
    }
    groups
}

fn oracle_type_dependency_groups(
    dependencies: &[RawDependency],
) -> BTreeMap<CollapsedDependencyIdentity, CollapsedDependencyEvidence> {
    let mut groups = BTreeMap::<CollapsedDependencyIdentity, CollapsedDependencyEvidence>::new();
    for dependency in dependencies.iter().filter(|dependency| {
        matches!(dependency.object_type.as_str(), "TYPE" | "TYPE BODY")
            && !dependency.referenced_owner_oracle_maintained
            && !(dependency.object_type == "TYPE BODY"
                && dependency.referenced_type == "TYPE"
                && dependency.owner == dependency.referenced_owner
                && dependency.name == dependency.referenced_name)
    }) {
        let evidence = groups
            .entry((
                dependency.owner.clone(),
                dependency.name.clone(),
                dependency.referenced_owner.clone(),
                dependency.referenced_name.clone(),
                dependency.referenced_type.clone(),
            ))
            .or_default();
        evidence
            .source_object_types
            .insert(dependency.object_type.clone());
        evidence
            .dependency_types
            .insert(dependency.dependency_type.clone());
    }
    groups
}

