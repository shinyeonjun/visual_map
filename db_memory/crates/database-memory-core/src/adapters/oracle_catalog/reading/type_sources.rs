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
