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
