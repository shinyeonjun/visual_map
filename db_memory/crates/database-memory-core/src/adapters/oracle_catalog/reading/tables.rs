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
