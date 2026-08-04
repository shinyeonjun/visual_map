fn pg_key(
    source_kind: &str,
    connection_alias: &str,
    database: &str,
    schema: &str,
    object_kind: ObjectKind,
    object_name: &str,
    sub_object: Option<String>,
) -> ObjectKey {
    ObjectKey::new(
        source_kind,
        connection_alias,
        database,
        schema,
        object_kind,
        object_name,
        sub_object,
    )
}

fn required<T>(value: Option<&T>, subject: impl Into<String>) -> Result<&T, CatalogError> {
    value.ok_or_else(|| CatalogError::Mapping(format!("unresolved {0}", subject.into())))
}

fn insert_bool(properties: &mut BTreeMap<String, MetadataValue>, key: &str, value: bool) {
    properties.insert(key.to_owned(), MetadataValue::Boolean(value));
}

fn insert_i64(properties: &mut BTreeMap<String, MetadataValue>, key: &str, value: i64) {
    properties.insert(key.to_owned(), MetadataValue::Integer(value));
}

fn insert_optional_i64(
    properties: &mut BTreeMap<String, MetadataValue>,
    key: &str,
    value: Option<i64>,
) {
    if let Some(value) = value {
        insert_i64(properties, key, value);
    }
}

fn insert_u64(properties: &mut BTreeMap<String, MetadataValue>, key: &str, value: u64) {
    properties.insert(key.to_owned(), MetadataValue::Unsigned(value));
}

fn insert_string(
    properties: &mut BTreeMap<String, MetadataValue>,
    key: &str,
    value: impl AsRef<str>,
) {
    properties.insert(
        key.to_owned(),
        MetadataValue::String(value.as_ref().to_owned()),
    );
}

fn insert_optional_string(
    properties: &mut BTreeMap<String, MetadataValue>,
    key: &str,
    value: Option<&str>,
) {
    if let Some(value) = value {
        insert_string(properties, key, value);
    }
}

fn insert_optional_bool(
    properties: &mut BTreeMap<String, MetadataValue>,
    key: &str,
    value: Option<bool>,
) {
    if let Some(value) = value {
        insert_bool(properties, key, value);
    }
}

fn positive_u32(value: i16, subject: &str) -> Result<u32, CatalogError> {
    u32::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| CatalogError::Mapping(format!("{subject} must be positive, got {value}")))
}

fn type_kind_name(kind: char) -> &'static str {
    match kind {
        'b' => "base",
        'c' => "composite",
        'd' => "domain",
        'e' => "enum",
        'r' => "range",
        'm' => "multirange",
        _ => "unrecognized",
    }
}

fn table_kind(relation: &RawRelation) -> TableKind {
    if relation.is_partition {
        TableKind::Partition
    } else {
        match relation.relkind {
            'p' => TableKind::Partitioned,
            'f' => TableKind::Foreign,
            _ => TableKind::BaseTable,
        }
    }
}

fn relation_properties(relation: &RawRelation) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_i64(&mut properties, "postgres_oid", relation.oid);
    insert_string(
        &mut properties,
        "relation_kind",
        relation.relkind.to_string(),
    );
    insert_string(
        &mut properties,
        "persistence",
        match relation.persistence {
            'u' => "unlogged",
            't' => "temporary",
            _ => "permanent",
        },
    );
    insert_bool(&mut properties, "partition", relation.is_partition);
    insert_bool(&mut properties, "row_security", relation.row_security);
    insert_bool(
        &mut properties,
        "force_row_security",
        relation.force_row_security,
    );
    insert_string(
        &mut properties,
        "replica_identity",
        relation.replica_identity.to_string(),
    );
    insert_optional_string(
        &mut properties,
        "partition_bound",
        relation.partition_bound.as_deref(),
    );
    insert_optional_string(&mut properties, "comment", relation.comment.as_deref());
    properties
}

fn relation_annotation(relation: &RawRelation, key: &ObjectKey) -> ObjectAnnotation {
    ObjectAnnotation {
        object_key: key.clone(),
        definition: None,
        properties: relation_properties(relation),
    }
}

#[allow(clippy::too_many_arguments)]
fn map_yugabyte_metadata(
    metadata: &mut CanonicalMetadata,
    raw: &RawYugabyteCatalog,
    source_kind: &str,
    connection_alias: &str,
    database_name: &str,
    database_key: &ObjectKey,
    principal_keys: &BTreeMap<i64, ObjectKey>,
    physical_relation_keys: &BTreeMap<i64, ObjectKey>,
) -> Result<(), CatalogError> {
    let mut tablespace_keys = BTreeMap::new();
    let mut tablespace_names = BTreeMap::new();
    for tablespace in &raw.tablespaces {
        let key = pg_key(
            source_kind,
            connection_alias,
            database_name,
            database_name,
            ObjectKind::Extension,
            &format!("tablespace:{}", tablespace.name),
            None,
        );
        if tablespace_keys
            .insert(tablespace.oid, key.clone())
            .is_some()
        {
            return Err(CatalogError::Mapping(format!(
                "duplicate YugabyteDB tablespace oid {}",
                tablespace.oid
            )));
        }
        tablespace_names.insert(tablespace.oid, tablespace.name.clone());
        let mut properties = BTreeMap::new();
        insert_i64(&mut properties, "yugabytedb_tablespace_oid", tablespace.oid);
        properties.insert(
            "acl".to_owned(),
            MetadataValue::StringList(tablespace.acl.clone()),
        );
        properties.insert(
            "placement_options".to_owned(),
            MetadataValue::StringList(tablespace.options.clone()),
        );
        insert_optional_string(&mut properties, "comment", tablespace.comment.as_deref());
        metadata.objects.push(MetadataObject {
            key: key.clone(),
            parent_key: Some(database_key.clone()),
            name: tablespace.name.clone(),
            extension_kind: Some("yugabytedb_tablespace".to_owned()),
            definition: None,
            properties,
        });
        add_owned_by(
            &mut metadata.relationships,
            &key,
            tablespace.owner_oid,
            principal_keys,
            "YugabyteDB tablespace",
        )?;
    }

    let default_tablespace = required(
        tablespace_keys.get(&raw.database_default_tablespace_oid),
        format!(
            "YugabyteDB database default tablespace oid {}",
            raw.database_default_tablespace_oid
        ),
    )?;
    let default_tablespace_name = required(
        tablespace_names.get(&raw.database_default_tablespace_oid),
        format!(
            "YugabyteDB database default tablespace name oid {}",
            raw.database_default_tablespace_oid
        ),
    )?;
    let mut database_properties = BTreeMap::new();
    insert_bool(
        &mut database_properties,
        "yugabytedb_database_colocated",
        raw.database_colocated,
    );
    insert_i64(
        &mut database_properties,
        "yugabytedb_default_tablespace_oid",
        raw.database_default_tablespace_oid,
    );
    insert_string(
        &mut database_properties,
        "yugabytedb_default_tablespace",
        default_tablespace_name,
    );
    merge_metadata_properties(metadata, database_key, database_properties)?;
    metadata.relationships.push(MetadataRelationship {
        kind: MetadataRelationshipKind::Extension("yugabytedb_default_tablespace".to_owned()),
        from_key: database_key.clone(),
        to_key: default_tablespace.clone(),
        ordinal: None,
        properties: BTreeMap::new(),
    });

    let mut tablegroup_keys = BTreeMap::new();
    for tablegroup in &raw.tablegroups {
        let key = pg_key(
            source_kind,
            connection_alias,
            database_name,
            database_name,
            ObjectKind::Extension,
            &format!("tablegroup:{}", tablegroup.name),
            None,
        );
        if tablegroup_keys
            .insert(tablegroup.oid, key.clone())
            .is_some()
        {
            return Err(CatalogError::Mapping(format!(
                "duplicate YugabyteDB tablegroup oid {}",
                tablegroup.oid
            )));
        }
        let effective_tablespace_oid = effective_yugabyte_tablespace_oid(
            tablegroup.tablespace_oid,
            raw.database_default_tablespace_oid,
        );
        let tablespace = required(
            tablespace_keys.get(&effective_tablespace_oid),
            format!(
                "tablespace oid {effective_tablespace_oid} for YugabyteDB tablegroup {}",
                tablegroup.name
            ),
        )?;
        let mut properties = BTreeMap::new();
        insert_i64(&mut properties, "yugabytedb_tablegroup_oid", tablegroup.oid);
        insert_i64(
            &mut properties,
            "yugabytedb_catalog_tablespace_oid",
            tablegroup.tablespace_oid,
        );
        properties.insert(
            "acl".to_owned(),
            MetadataValue::StringList(tablegroup.acl.clone()),
        );
        properties.insert(
            "options".to_owned(),
            MetadataValue::StringList(tablegroup.options.clone()),
        );
        metadata.objects.push(MetadataObject {
            key: key.clone(),
            parent_key: Some(database_key.clone()),
            name: tablegroup.name.clone(),
            extension_kind: Some("yugabytedb_tablegroup".to_owned()),
            definition: None,
            properties,
        });
        add_owned_by(
            &mut metadata.relationships,
            &key,
            tablegroup.owner_oid,
            principal_keys,
            "YugabyteDB tablegroup",
        )?;
        metadata.relationships.push(MetadataRelationship {
            kind: MetadataRelationshipKind::Extension("yugabytedb_uses_tablespace".to_owned()),
            from_key: key,
            to_key: tablespace.clone(),
            ordinal: None,
            properties: BTreeMap::new(),
        });
    }

    for relation in &raw.relation_properties {
        let key = required(
            physical_relation_keys.get(&relation.relation_oid),
            format!("YugabyteDB physical relation oid {}", relation.relation_oid),
        )?;
        let storage_backed = relation.num_tablets.is_some();
        let mut properties = BTreeMap::new();
        insert_bool(&mut properties, "yugabytedb_storage_backed", storage_backed);
        insert_string(
            &mut properties,
            "yugabytedb_relation_kind",
            relation.relation_kind.to_string(),
        );
        insert_i64(
            &mut properties,
            "yugabytedb_catalog_tablespace_oid",
            relation.tablespace_oid,
        );
        insert_optional_i64(
            &mut properties,
            "yugabytedb_num_tablets",
            relation.num_tablets,
        );
        insert_optional_i64(
            &mut properties,
            "yugabytedb_num_hash_key_columns",
            relation.num_hash_key_columns,
        );
        insert_optional_bool(
            &mut properties,
            "yugabytedb_is_colocated",
            relation.is_colocated,
        );
        insert_optional_i64(
            &mut properties,
            "yugabytedb_tablegroup_oid",
            relation.tablegroup_oid,
        );
        insert_optional_i64(
            &mut properties,
            "yugabytedb_colocation_id",
            relation.colocation_id,
        );
        insert_optional_string(
            &mut properties,
            "yugabytedb_range_split_clause",
            relation.range_split_clause.as_deref(),
        );

        if storage_backed {
            let effective_tablespace_oid = effective_yugabyte_tablespace_oid(
                relation.tablespace_oid,
                raw.database_default_tablespace_oid,
            );
            let tablespace = required(
                tablespace_keys.get(&effective_tablespace_oid),
                format!(
                    "tablespace oid {effective_tablespace_oid} for YugabyteDB relation {}",
                    key
                ),
            )?;
            let tablespace_name = required(
                tablespace_names.get(&effective_tablespace_oid),
                format!(
                    "tablespace name oid {effective_tablespace_oid} for YugabyteDB relation {}",
                    key
                ),
            )?;
            insert_string(
                &mut properties,
                "yugabytedb_effective_tablespace",
                tablespace_name,
            );
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::Extension("yugabytedb_uses_tablespace".to_owned()),
                from_key: key.clone(),
                to_key: tablespace.clone(),
                ordinal: None,
                properties: BTreeMap::new(),
            });
        }
        if let Some(tablegroup_oid) = relation.tablegroup_oid {
            let tablegroup = required(
                tablegroup_keys.get(&tablegroup_oid),
                format!(
                    "tablegroup oid {tablegroup_oid} for YugabyteDB relation {}",
                    key
                ),
            )?;
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::Extension(
                    "yugabytedb_member_of_tablegroup".to_owned(),
                ),
                from_key: key.clone(),
                to_key: tablegroup.clone(),
                ordinal: None,
                properties: BTreeMap::new(),
            });
        }
        merge_metadata_properties(metadata, key, properties)?;
    }

    Ok(())
}

fn effective_yugabyte_tablespace_oid(catalog_oid: i64, database_default_oid: i64) -> i64 {
    if catalog_oid == 0 {
        database_default_oid
    } else {
        catalog_oid
    }
}
