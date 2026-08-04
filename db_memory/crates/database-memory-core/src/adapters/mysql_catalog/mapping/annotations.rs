fn add_annotation(
    metadata: &mut CanonicalMetadata,
    object_key: &ObjectKey,
    definition: Option<String>,
    properties: BTreeMap<String, MetadataValue>,
) {
    if definition.is_some() || !properties.is_empty() {
        metadata.annotations.push(ObjectAnnotation {
            object_key: object_key.clone(),
            definition,
            properties,
        });
    }
}

fn add_database_annotation(
    metadata: &mut CanonicalMetadata,
    database_key: &ObjectKey,
    raw: &RawMysqlFamilyCatalog,
) {
    let mut properties = BTreeMap::new();
    insert_string(&mut properties, "server_version", &raw.facts.version);
    insert_string(
        &mut properties,
        "server_version_comment",
        &raw.facts.version_comment,
    );
    insert_string(&mut properties, "current_user", &raw.facts.current_user);
    insert_string(&mut properties, "session_user", &raw.facts.session_user);
    insert_u64(
        &mut properties,
        "lower_case_table_names",
        raw.facts.lower_case_table_names,
    );
    insert_string(&mut properties, "catalog_strategy", raw.strategy.label());
    add_annotation(metadata, database_key, None, properties);
}

#[derive(Clone, Debug)]
struct PrincipalKeys {
    current: ObjectKey,
    roles: Vec<ObjectKey>,
}

fn map_principals(
    metadata: &mut CanonicalMetadata,
    source_kind: &str,
    connection_alias: &str,
    database: &str,
    database_key: &ObjectKey,
    raw: &RawMysqlFamilyCatalog,
) -> Result<PrincipalKeys, CatalogError> {
    let current_key = family_key(
        source_kind,
        connection_alias,
        database,
        ObjectKind::Principal,
        &raw.facts.current_user,
        None,
    );
    let mut properties = BTreeMap::new();
    insert_string(&mut properties, "principal_kind", "current_user");
    properties.insert(
        "schema_privileges".to_owned(),
        MetadataValue::StringList(raw.grants.iter().cloned().collect()),
    );
    metadata.objects.push(MetadataObject {
        key: current_key.clone(),
        parent_key: Some(database_key.clone()),
        name: raw.facts.current_user.clone(),
        extension_kind: None,
        definition: None,
        properties,
    });

    let mut roles = Vec::new();
    let mut seen = BTreeSet::new();
    for role in &raw.active_roles {
        let normalized = normalize_principal(role);
        if !seen.insert(normalized) {
            return Err(CatalogError::Mapping(format!(
                "duplicate active role '{role}'"
            )));
        }
        let key = family_key(
            source_kind,
            connection_alias,
            database,
            ObjectKind::Principal,
            role,
            Some("active_role".to_owned()),
        );
        let mut properties = BTreeMap::new();
        insert_string(&mut properties, "principal_kind", "active_role");
        metadata.objects.push(MetadataObject {
            key: key.clone(),
            parent_key: Some(database_key.clone()),
            name: role.clone(),
            extension_kind: None,
            definition: None,
            properties,
        });
        roles.push(key);
    }
    Ok(PrincipalKeys {
        current: current_key,
        roles,
    })
}

fn add_principal_relationships(
    metadata: &mut CanonicalMetadata,
    principals: &PrincipalKeys,
) -> Result<(), CatalogError> {
    for role in &principals.roles {
        metadata.relationships.push(MetadataRelationship {
            kind: MetadataRelationshipKind::Extension("active_role".to_owned()),
            from_key: principals.current.clone(),
            to_key: role.clone(),
            ordinal: None,
            properties: BTreeMap::new(),
        });
    }
    Ok(())
}

fn map_sequences(
    metadata: &mut CanonicalMetadata,
    source_kind: &str,
    connection_alias: &str,
    database: &str,
    schema_key: &ObjectKey,
    lower_case_table_names: u64,
    raw_sequences: &[RawSequence],
) -> Result<BTreeMap<String, ObjectKey>, CatalogError> {
    let mut keys = BTreeMap::new();
    for sequence in raw_sequences {
        let normalized = normalize_object_name(&sequence.name, lower_case_table_names);
        let key = family_key(
            source_kind,
            connection_alias,
            database,
            ObjectKind::Sequence,
            &sequence.name,
            None,
        );
        if keys.insert(normalized, key.clone()).is_some() {
            return Err(CatalogError::Mapping(format!(
                "duplicate sequence '{}'",
                sequence.name
            )));
        }
        let mut properties = BTreeMap::new();
        insert_optional_string(&mut properties, "data_type", sequence.data_type.as_deref());
        insert_optional_string(
            &mut properties,
            "start_value",
            sequence.start_value.as_deref(),
        );
        insert_optional_string(
            &mut properties,
            "minimum_value",
            sequence.minimum_value.as_deref(),
        );
        insert_optional_string(
            &mut properties,
            "maximum_value",
            sequence.maximum_value.as_deref(),
        );
        insert_optional_string(&mut properties, "increment", sequence.increment.as_deref());
        if let Some(cycles) = sequence.cycles {
            insert_bool(&mut properties, "cycles", cycles);
        }
        metadata.objects.push(MetadataObject {
            key,
            parent_key: Some(schema_key.clone()),
            name: sequence.name.clone(),
            extension_kind: None,
            definition: sequence.definition.clone(),
            properties,
        });
    }
    Ok(keys)
}

fn add_table_annotation(metadata: &mut CanonicalMetadata, table_key: &ObjectKey, table: &RawTable) {
    let mut properties = BTreeMap::new();
    insert_optional_string(&mut properties, "engine", table.engine.as_deref());
    insert_optional_string(&mut properties, "row_format", table.row_format.as_deref());
    insert_optional_string(&mut properties, "collation", table.collation.as_deref());
    insert_optional_string(
        &mut properties,
        "create_options",
        table.create_options.as_deref(),
    );
    insert_string(&mut properties, "comment", &table.comment);
    add_annotation(metadata, table_key, None, properties);
}

fn add_view_annotation(
    metadata: &mut CanonicalMetadata,
    view_key: &ObjectKey,
    view: &RawView,
    _definition: String,
) {
    let mut properties = BTreeMap::new();
    insert_string(&mut properties, "check_option", &view.check_option);
    insert_bool(&mut properties, "updatable", view.updatable);
    insert_string(&mut properties, "definer", &view.definer);
    insert_string(&mut properties, "security_type", &view.security_type);
    insert_string(&mut properties, "character_set_client", &view.character_set);
    insert_string(&mut properties, "collation_connection", &view.collation);
    insert_optional_string(&mut properties, "algorithm", view.algorithm.as_deref());
    add_annotation(metadata, view_key, None, properties);
}
