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
