fn sqlserver_key(
    connection_alias: &str,
    database: &str,
    schema: &str,
    object_kind: ObjectKind,
    object_name: &str,
    sub_object: Option<String>,
) -> ObjectKey {
    ObjectKey::new(
        SQLSERVER_SOURCE,
        connection_alias,
        database,
        schema,
        object_kind,
        object_name,
        sub_object,
    )
}

#[allow(clippy::too_many_arguments)]
fn insert_object_identity(
    kind_keys: &mut BTreeMap<i32, ObjectKey>,
    object_keys: &mut BTreeMap<i32, ObjectKey>,
    name_keys: &mut BTreeMap<(String, String), ObjectKey>,
    id: i32,
    schema: &str,
    name: &str,
    key: &ObjectKey,
    kind: &str,
) -> Result<(), CatalogError> {
    insert_unique_id(kind_keys, id, key, kind)?;
    if object_keys.insert(id, key.clone()).is_some() {
        return Err(CatalogError::Mapping(format!(
            "object id {id} is shared by multiple mapped objects"
        )));
    }
    if name_keys
        .insert((schema.to_owned(), name.to_owned()), key.clone())
        .is_some()
    {
        return Err(CatalogError::Mapping(format!(
            "duplicate schema object name '{schema}.{name}'"
        )));
    }
    Ok(())
}

fn insert_view_identity(
    view_keys: &mut BTreeMap<i32, ObjectKey>,
    object_keys: &mut BTreeMap<i32, ObjectKey>,
    name_keys: &mut BTreeMap<(String, String), ObjectKey>,
    view: &RawView,
    key: &ObjectKey,
) -> Result<(), CatalogError> {
    insert_object_identity(
        view_keys,
        object_keys,
        name_keys,
        view.id,
        &view.schema,
        &view.name,
        key,
        "view",
    )
}

fn insert_unique_id(
    keys: &mut BTreeMap<i32, ObjectKey>,
    id: i32,
    key: &ObjectKey,
    subject: &str,
) -> Result<(), CatalogError> {
    if keys.insert(id, key.clone()).is_some() {
        return Err(CatalogError::Mapping(format!(
            "duplicate {subject} id {id}"
        )));
    }
    Ok(())
}

fn required_key<'a>(
    keys: &'a BTreeMap<String, ObjectKey>,
    name: &str,
    subject: &str,
) -> Result<&'a ObjectKey, CatalogError> {
    keys.get(name)
        .ok_or_else(|| CatalogError::Mapping(format!("{subject} '{name}' is not mapped")))
}

fn add_database_annotation(
    metadata: &mut CanonicalMetadata,
    database_key: &ObjectKey,
    facts: &ServerFacts,
    strategy: SqlServerCatalogVersion,
) {
    let mut properties = BTreeMap::new();
    insert_string(&mut properties, "server_version", &facts.version);
    insert_i64(&mut properties, "server_major", i64::from(facts.major));
    insert_i64(
        &mut properties,
        "engine_edition",
        i64::from(facts.engine_edition),
    );
    insert_string(&mut properties, "edition", &facts.edition);
    insert_string(&mut properties, "current_user", &facts.current_user);
    insert_string(&mut properties, "login", &facts.login);
    insert_string(&mut properties, "original_login", &facts.original_login);
    insert_string(&mut properties, "collation", &facts.collation);
    insert_i64(
        &mut properties,
        "compatibility_level",
        i64::from(facts.compatibility_level),
    );
    insert_bool(
        &mut properties,
        "database_read_only",
        facts.database_read_only,
    );
    insert_string(&mut properties, "containment", &facts.containment);
    insert_bool(
        &mut properties,
        "encrypted_transport",
        facts.encrypted_transport,
    );
    insert_string(
        &mut properties,
        "catalog_strategy",
        strategy.strategy_name(),
    );
    add_annotation(metadata, database_key, None, properties);
}
