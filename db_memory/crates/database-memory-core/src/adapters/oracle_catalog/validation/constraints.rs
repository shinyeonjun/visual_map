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
