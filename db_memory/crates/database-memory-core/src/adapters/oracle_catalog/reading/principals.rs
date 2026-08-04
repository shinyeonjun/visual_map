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
