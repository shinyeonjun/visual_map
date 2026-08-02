fn validate_raw_catalog(raw: &RawPostgresCatalog) -> Result<(), CatalogError> {
    let strategy = raw.strategy;
    if raw.server.major() != strategy.catalog_version().major() {
        return Err(CatalogError::Mapping(format!(
            "{} server major {} does not match selected catalog strategy {}",
            strategy.product_name(),
            raw.server.major(),
            strategy.strategy_name()
        )));
    }
    match (strategy, &raw.yugabyte) {
        (PgCatalogStrategy::YugabyteDb2025_2_3_2, Some(yugabyte)) => {
            validate_yugabyte_catalog(raw, yugabyte)?;
        }
        (PgCatalogStrategy::YugabyteDb2025_2_3_2, None) => {
            return Err(CatalogError::UnsupportedMetadata(
                "certified YugabyteDB strategy did not collect YugabyteDB catalog metadata"
                    .to_owned(),
            ));
        }
        (PgCatalogStrategy::PostgreSql(_), Some(_)) => {
            return Err(CatalogError::Mapping(
                "PostgreSQL strategy unexpectedly contains YugabyteDB catalog metadata".to_owned(),
            ));
        }
        (PgCatalogStrategy::PostgreSql(_), None) => {}
    }
    if !raw.server.transaction_read_only || raw.server.transaction_isolation != "repeatable read" {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "{} metadata transaction is not read-only repeatable-read (read_only={}, isolation={})",
            strategy.product_name(),
            raw.server.transaction_read_only,
            raw.server.transaction_isolation
        )));
    }
    for relation in &raw.relations {
        if relation.definition_too_large {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "definition for {}.{} exceeds {MAX_DEFINITION_BYTES} bytes",
                relation.schema, relation.name
            )));
        }
        validate_property_text(
            &format!(
                "relation {}.{} partition bound",
                relation.schema, relation.name
            ),
            relation.partition_bound.as_deref(),
        )?;
        validate_property_text(
            &format!("relation {}.{} comment", relation.schema, relation.name),
            relation.comment.as_deref(),
        )?;
    }
    for schema in &raw.schemas {
        validate_property_text(
            &format!("schema {} comment", schema.name),
            schema.comment.as_deref(),
        )?;
    }
    for column in &raw.columns {
        if column.default_too_large {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "default/generated expression for {}.{}.{} exceeds {MAX_DEFINITION_BYTES} bytes",
                column.schema, column.relation, column.name
            )));
        }
        validate_property_text(
            &format!(
                "column {}.{}.{} comment",
                column.schema, column.relation, column.name
            ),
            column.comment.as_deref(),
        )?;
    }
    for constraint in &raw.constraints {
        if constraint.definition_too_large {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "constraint definition {} exceeds {MAX_DEFINITION_BYTES} bytes",
                constraint.name
            )));
        }
    }
    for index in &raw.indexes {
        if index.definition_too_large {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "index definition {}.{}.{} exceeds {MAX_DEFINITION_BYTES} bytes",
                index.schema, index.relation, index.name
            )));
        }
    }
    for term in &raw.index_terms {
        validate_property_text("index term definition", Some(&term.definition))?;
    }
    for raw_type in &raw.types {
        if raw_type.default_too_large {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "type default {}.{} exceeds {MAX_PROPERTY_STRING_BYTES} bytes",
                raw_type.schema, raw_type.name
            )));
        }
        validate_property_text(
            &format!("type {}.{} comment", raw_type.schema, raw_type.name),
            raw_type.comment.as_deref(),
        )?;
    }
    for routine in &raw.routines {
        if routine.definition_too_large {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "routine definition {}.{}({}) exceeds {MAX_DEFINITION_BYTES} bytes",
                routine.schema, routine.name, routine.identity_arguments
            )));
        }
        validate_property_text(
            &format!("routine {} arguments", routine.name),
            Some(&routine.arguments_definition),
        )?;
    }
    for trigger in &raw.triggers {
        if trigger.definition_too_large {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "trigger definition {} exceeds {MAX_DEFINITION_BYTES} bytes",
                trigger.name
            )));
        }
        validate_property_text(
            &format!("trigger {} WHEN expression", trigger.name),
            trigger.when_expression.as_deref(),
        )?;
    }
    for policy in &raw.policies {
        validate_property_text(
            &format!("policy {} USING expression", policy.name),
            policy.using_expression.as_deref(),
        )?;
        validate_property_text(
            &format!("policy {} WITH CHECK expression", policy.name),
            policy.check_expression.as_deref(),
        )?;
    }
    Ok(())
}

fn validate_yugabyte_catalog(
    raw: &RawPostgresCatalog,
    yugabyte: &RawYugabyteCatalog,
) -> Result<(), CatalogError> {
    if yugabyte.database_default_tablespace_oid <= 0 {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "YugabyteDB database default tablespace oid must be positive, got {}",
            yugabyte.database_default_tablespace_oid
        )));
    }

    let mut tablespace_oids = BTreeSet::new();
    let mut tablespace_names = BTreeSet::new();
    for tablespace in &yugabyte.tablespaces {
        if tablespace.oid <= 0
            || tablespace.name.trim().is_empty()
            || !tablespace_oids.insert(tablespace.oid)
            || !tablespace_names.insert(tablespace.name.clone())
        {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "invalid or duplicate YugabyteDB tablespace oid={} name='{}'",
                tablespace.oid, tablespace.name
            )));
        }
        validate_property_text(
            &format!("YugabyteDB tablespace {} comment", tablespace.name),
            tablespace.comment.as_deref(),
        )?;
        validate_string_list(
            &format!("YugabyteDB tablespace {} ACL", tablespace.name),
            &tablespace.acl,
        )?;
        validate_string_list(
            &format!(
                "YugabyteDB tablespace {} placement options",
                tablespace.name
            ),
            &tablespace.options,
        )?;
    }
    if !tablespace_oids.contains(&yugabyte.database_default_tablespace_oid) {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "YugabyteDB default tablespace oid {} is absent from pg_tablespace",
            yugabyte.database_default_tablespace_oid
        )));
    }

    let mut tablegroup_oids = BTreeSet::new();
    let mut tablegroup_names = BTreeSet::new();
    for tablegroup in &yugabyte.tablegroups {
        if tablegroup.oid <= 0
            || tablegroup.name.trim().is_empty()
            || !tablegroup_oids.insert(tablegroup.oid)
            || !tablegroup_names.insert(tablegroup.name.clone())
        {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "invalid or duplicate YugabyteDB tablegroup oid={} name='{}'",
                tablegroup.oid, tablegroup.name
            )));
        }
        let effective_tablespace = effective_yugabyte_tablespace_oid(
            tablegroup.tablespace_oid,
            yugabyte.database_default_tablespace_oid,
        );
        if !tablespace_oids.contains(&effective_tablespace) {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "YugabyteDB tablegroup {} references missing tablespace oid {}",
                tablegroup.name, effective_tablespace
            )));
        }
        validate_string_list(
            &format!("YugabyteDB tablegroup {} ACL", tablegroup.name),
            &tablegroup.acl,
        )?;
        validate_string_list(
            &format!("YugabyteDB tablegroup {} options", tablegroup.name),
            &tablegroup.options,
        )?;
    }

    let mut expected_relations = BTreeMap::new();
    for relation in &raw.relations {
        if matches!(relation.relkind, 'r' | 'p' | 'f' | 'm' | 'S')
            && expected_relations
                .insert(relation.oid, Some(relation.relkind))
                .is_some()
        {
            return Err(CatalogError::Mapping(format!(
                "duplicate YugabyteDB relation oid {}",
                relation.oid
            )));
        }
    }
    for index in &raw.indexes {
        if expected_relations.insert(index.oid, None).is_some() {
            return Err(CatalogError::Mapping(format!(
                "duplicate YugabyteDB index oid {}",
                index.oid
            )));
        }
    }

    let mut discovered_relations = BTreeSet::new();
    for relation in &yugabyte.relation_properties {
        let Some(expected_kind) = expected_relations.get(&relation.relation_oid) else {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "yb_table_properties returned out-of-scope relation oid {}",
                relation.relation_oid
            )));
        };
        if !discovered_relations.insert(relation.relation_oid) {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "yb_table_properties returned duplicate relation oid {}",
                relation.relation_oid
            )));
        }
        match expected_kind {
            Some(kind) if *kind != relation.relation_kind => {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "YugabyteDB relation oid {} changed kind from '{}' to '{}' during discovery",
                    relation.relation_oid, kind, relation.relation_kind
                )));
            }
            None if !matches!(relation.relation_kind, 'i' | 'I') => {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "YugabyteDB index oid {} reports relation kind '{}'",
                    relation.relation_oid, relation.relation_kind
                )));
            }
            _ => {}
        }
        if relation.tablespace_oid < 0 {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "YugabyteDB relation oid {} has negative tablespace oid {}",
                relation.relation_oid, relation.tablespace_oid
            )));
        }
        validate_property_text(
            &format!(
                "YugabyteDB relation oid {} range split clause",
                relation.relation_oid
            ),
            relation.range_split_clause.as_deref(),
        )?;

        match (
            relation.num_tablets,
            relation.num_hash_key_columns,
            relation.is_colocated,
        ) {
            (Some(num_tablets), Some(num_hash_columns), Some(_))
                if num_tablets > 0 && num_hash_columns >= 0 => {}
            (None, None, None)
                if relation.tablegroup_oid.is_none()
                    && relation.colocation_id.is_none()
                    && relation.range_split_clause.is_none() =>
            {
                continue;
            }
            values => {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "incoherent yb_table_properties for relation oid {}: {:?}",
                    relation.relation_oid, values
                )));
            }
        }
        if relation.range_split_clause.is_some() && relation.num_hash_key_columns != Some(0) {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "YugabyteDB relation oid {} has a range split clause with hash key columns",
                relation.relation_oid
            )));
        }
        match relation.is_colocated {
            Some(true) if relation.tablegroup_oid.is_some() && relation.colocation_id.is_some() => {
            }
            Some(false)
                if relation.tablegroup_oid.is_none() && relation.colocation_id.is_none() => {}
            Some(is_colocated) => {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "YugabyteDB relation oid {} has inconsistent colocation fields (is_colocated={is_colocated})",
                    relation.relation_oid
                )));
            }
            None => unreachable!("the non-storage-backed case continued above"),
        }
        if let Some(tablegroup_oid) = relation.tablegroup_oid {
            if !tablegroup_oids.contains(&tablegroup_oid) {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "YugabyteDB relation oid {} references missing tablegroup oid {}",
                    relation.relation_oid, tablegroup_oid
                )));
            }
        }
        let effective_tablespace = effective_yugabyte_tablespace_oid(
            relation.tablespace_oid,
            yugabyte.database_default_tablespace_oid,
        );
        if !tablespace_oids.contains(&effective_tablespace) {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "YugabyteDB relation oid {} references missing tablespace oid {}",
                relation.relation_oid, effective_tablespace
            )));
        }
    }
    let expected_oids = expected_relations.keys().copied().collect::<BTreeSet<_>>();
    if expected_oids != discovered_relations {
        let missing = expected_oids
            .difference(&discovered_relations)
            .copied()
            .collect::<Vec<_>>();
        return Err(CatalogError::UnsupportedMetadata(format!(
            "YugabyteDB physical metadata is incomplete; missing relation oids {missing:?}"
        )));
    }

    Ok(())
}

fn validate_string_list(subject: &str, values: &[String]) -> Result<(), CatalogError> {
    for value in values {
        validate_property_text(subject, Some(value))?;
    }
    Ok(())
}

fn validate_property_text(subject: &str, value: Option<&str>) -> Result<(), CatalogError> {
    if value
        .map(|value| value.len() > MAX_PROPERTY_STRING_BYTES as usize)
        .unwrap_or(false)
    {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "{subject} exceeds {MAX_PROPERTY_STRING_BYTES} bytes"
        )));
    }
    Ok(())
}

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

fn merge_metadata_properties(
    metadata: &mut CanonicalMetadata,
    object_key: &ObjectKey,
    properties: BTreeMap<String, MetadataValue>,
) -> Result<(), CatalogError> {
    if let Some(object) = metadata
        .objects
        .iter_mut()
        .find(|object| object.key == *object_key)
    {
        merge_property_maps(&mut object.properties, properties, object_key)?;
        return Ok(());
    }
    if let Some(annotation) = metadata
        .annotations
        .iter_mut()
        .find(|annotation| annotation.object_key == *object_key)
    {
        merge_property_maps(&mut annotation.properties, properties, object_key)?;
        return Ok(());
    }
    metadata.annotations.push(ObjectAnnotation {
        object_key: object_key.clone(),
        definition: None,
        properties,
    });
    Ok(())
}

fn merge_property_maps(
    target: &mut BTreeMap<String, MetadataValue>,
    properties: BTreeMap<String, MetadataValue>,
    object_key: &ObjectKey,
) -> Result<(), CatalogError> {
    for (name, value) in properties {
        if target.insert(name.clone(), value).is_some() {
            return Err(CatalogError::Mapping(format!(
                "duplicate metadata property '{name}' for {object_key}"
            )));
        }
    }
    Ok(())
}

fn column_properties(column: &RawColumn) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_i64(
        &mut properties,
        "postgres_attribute_number",
        i64::from(column.attnum),
    );
    insert_string(
        &mut properties,
        "generated",
        match column.generated {
            's' => "stored",
            'v' => "virtual",
            _ => "none",
        },
    );
    insert_string(
        &mut properties,
        "identity",
        match column.identity {
            'a' => "always",
            'd' => "by_default",
            _ => "none",
        },
    );
    insert_optional_string(&mut properties, "collation", column.collation.as_deref());
    insert_optional_string(
        &mut properties,
        "compression",
        column.compression.as_deref(),
    );
    let statistics_target_mode = match column.statistics_target {
        PostgresStatisticsTarget::Default => "default",
        PostgresStatisticsTarget::Disabled => "disabled",
        PostgresStatisticsTarget::Custom(_) => "custom",
    };
    insert_string(
        &mut properties,
        "statistics_target_mode",
        statistics_target_mode,
    );
    if let PostgresStatisticsTarget::Custom(value) = column.statistics_target {
        insert_i64(&mut properties, "statistics_target", i64::from(value));
    } else if column.statistics_target == PostgresStatisticsTarget::Disabled {
        insert_i64(&mut properties, "statistics_target", 0);
    }
    insert_optional_string(&mut properties, "comment", column.comment.as_deref());
    if column.generated != '\0' {
        insert_optional_string(
            &mut properties,
            "generation_expression",
            column.default_expression.as_deref(),
        );
    }
    if let Some(default_oid) = column.default_oid {
        insert_i64(&mut properties, "postgres_default_oid", default_oid);
    }
    properties
}

fn column_annotation(column: &RawColumn, key: &ObjectKey) -> ObjectAnnotation {
    ObjectAnnotation {
        object_key: key.clone(),
        definition: None,
        properties: column_properties(column),
    }
}

fn add_owned_by(
    relationships: &mut Vec<MetadataRelationship>,
    object: &ObjectKey,
    owner_oid: i64,
    principals: &BTreeMap<i64, ObjectKey>,
    subject: &str,
) -> Result<(), CatalogError> {
    let owner = required(
        principals.get(&owner_oid),
        format!("owner principal oid {owner_oid} for {subject} {object}"),
    )?;
    relationships.push(MetadataRelationship {
        kind: MetadataRelationshipKind::OwnedBy,
        from_key: object.clone(),
        to_key: owner.clone(),
        ordinal: None,
        properties: BTreeMap::new(),
    });
    Ok(())
}

fn add_type_use(
    relationships: &mut Vec<MetadataRelationship>,
    object: &ObjectKey,
    type_oid: i64,
    type_schema: &str,
    types: &BTreeMap<i64, ObjectKey>,
) -> Result<(), CatalogError> {
    if let Some(target) = types.get(&type_oid) {
        relationships.push(MetadataRelationship {
            kind: MetadataRelationshipKind::UsesType,
            from_key: object.clone(),
            to_key: target.clone(),
            ordinal: None,
            properties: BTreeMap::new(),
        });
    } else if !is_system_schema(type_schema) {
        return Err(CatalogError::Mapping(format!(
            "{} uses type outside the certified schema scope ({}. oid {})",
            object, type_schema, type_oid
        )));
    }
    Ok(())
}

fn relation_row_type_oid(
    relations: &[RawRelation],
    relation_oid: i64,
) -> Result<i64, CatalogError> {
    relations
        .iter()
        .find(|relation| relation.oid == relation_oid)
        .map(|relation| relation.row_type_oid)
        .ok_or_else(|| {
            CatalogError::Mapping(format!("unresolved relation oid {relation_oid} row type"))
        })
}

fn constraint_properties(constraint: &RawConstraint) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_i64(&mut properties, "postgres_oid", constraint.oid);
    insert_bool(&mut properties, "deferrable", constraint.deferrable);
    insert_bool(
        &mut properties,
        "initially_deferred",
        constraint.initially_deferred,
    );
    insert_bool(&mut properties, "validated", constraint.validated);
    insert_bool(&mut properties, "no_inherit", constraint.no_inherit);
    if constraint.kind == 'f' {
        insert_string(
            &mut properties,
            "on_delete",
            foreign_key_action(constraint.delete_action),
        );
        insert_string(
            &mut properties,
            "on_update",
            foreign_key_action(constraint.update_action),
        );
        insert_string(
            &mut properties,
            "match_type",
            foreign_key_match(constraint.match_type),
        );
    }
    properties
}

fn foreign_key_action(value: char) -> &'static str {
    match value {
        'a' => "no_action",
        'r' => "restrict",
        'c' => "cascade",
        'n' => "set_null",
        'd' => "set_default",
        _ => "not_applicable",
    }
}

fn foreign_key_match(value: char) -> &'static str {
    match value {
        'f' => "full",
        'p' => "partial",
        's' => "simple",
        _ => "not_applicable",
    }
}

fn resolve_columns(
    relation_oid: i64,
    column_numbers: &[i16],
    columns: &BTreeMap<(i64, i32), ObjectKey>,
    subject: &str,
) -> Result<Vec<ObjectKey>, CatalogError> {
    column_numbers
        .iter()
        .enumerate()
        .map(|(position, column_number)| {
            if *column_number <= 0 {
                return Err(CatalogError::Mapping(format!(
                    "{subject} contains expression/system column number {column_number} at ordinal {}",
                    position + 1
                )));
            }
            required(
                columns.get(&(relation_oid, i32::from(*column_number))),
                format!("{subject} column number {column_number}"),
            )
            .cloned()
        })
        .collect()
}

fn group_index_terms(index_terms: &[RawIndexTerm]) -> BTreeMap<i64, Vec<&RawIndexTerm>> {
    let mut grouped = BTreeMap::<i64, Vec<&RawIndexTerm>>::new();
    for term in index_terms {
        grouped.entry(term.index_oid).or_default().push(term);
    }
    grouped
}

fn index_properties(index: &RawIndex, terms: &[&RawIndexTerm]) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_i64(&mut properties, "postgres_oid", index.oid);
    insert_string(&mut properties, "access_method", &index.access_method);
    insert_bool(&mut properties, "unique", index.unique);
    insert_bool(&mut properties, "primary", index.primary);
    insert_bool(&mut properties, "exclusion", index.exclusion);
    insert_bool(&mut properties, "immediate", index.immediate);
    insert_bool(&mut properties, "clustered", index.clustered);
    insert_bool(&mut properties, "valid", index.valid);
    insert_bool(&mut properties, "ready", index.ready);
    insert_bool(&mut properties, "live", index.live);
    insert_bool(&mut properties, "replica_identity", index.replica_identity);
    insert_bool(
        &mut properties,
        "nulls_not_distinct",
        index.nulls_not_distinct,
    );
    insert_i64(
        &mut properties,
        "key_term_count",
        i64::from(index.key_count),
    );
    properties.insert(
        "terms".to_owned(),
        MetadataValue::StringList(
            terms
                .iter()
                .map(|term| {
                    format!(
                        "{}|{}|{}|{}|{}|{}|{}|{}",
                        term.ordinal,
                        if term.is_key { "key" } else { "include" },
                        term.column_name.as_deref().unwrap_or_default(),
                        term.definition,
                        if term.descending { "desc" } else { "asc" },
                        if term.nulls_first {
                            "nulls_first"
                        } else {
                            "nulls_last"
                        },
                        term.operator_class.as_deref().unwrap_or_default(),
                        term.collation.as_deref().unwrap_or_default()
                    )
                })
                .collect(),
        ),
    );
    properties
}

fn add_included_columns(
    relationships: &mut Vec<MetadataRelationship>,
    index_key: &ObjectKey,
    index: &RawIndex,
    terms: &[&RawIndexTerm],
    columns: &BTreeMap<(i64, i32), ObjectKey>,
    _materialized_view: bool,
) -> Result<(), CatalogError> {
    for term in terms {
        if term.column_number <= 0 {
            continue;
        }
        let column = required(
            columns.get(&(index.relation_oid, i32::from(term.column_number))),
            format!("index {} term ordinal {}", index.name, term.ordinal),
        )?;
        let mut properties = BTreeMap::new();
        insert_string(
            &mut properties,
            "role",
            if term.is_key { "key" } else { "include" },
        );
        insert_string(&mut properties, "definition", &term.definition);
        insert_bool(&mut properties, "descending", term.descending);
        insert_bool(&mut properties, "nulls_first", term.nulls_first);
        insert_optional_string(
            &mut properties,
            "operator_class",
            term.operator_class.as_deref(),
        );
        insert_optional_string(&mut properties, "collation", term.collation.as_deref());
        relationships.push(MetadataRelationship {
            kind: MetadataRelationshipKind::IncludesColumn,
            from_key: index_key.clone(),
            to_key: column.clone(),
            ordinal: Some(positive_u32(term.ordinal, "index term ordinal")?),
            properties,
        });
    }
    Ok(())
}

fn resolve_relation_dependency(
    relation_oid: i64,
    column_number: i32,
    target_schema: &str,
    relations: &BTreeMap<i64, ObjectKey>,
    columns: &BTreeMap<(i64, i32), ObjectKey>,
) -> Result<Option<ObjectKey>, CatalogError> {
    if is_system_schema(target_schema) {
        return Ok(None);
    }
    if column_number > 0 {
        return required(
            columns.get(&(relation_oid, column_number)),
            format!(
                "dependency target column outside the certified schema scope ({}. oid {}:{})",
                target_schema, relation_oid, column_number
            ),
        )
        .cloned()
        .map(Some);
    }
    required(
        relations.get(&relation_oid),
        format!(
            "dependency target relation outside the certified schema scope ({}. oid {})",
            target_schema, relation_oid
        ),
    )
    .cloned()
    .map(Some)
}

fn routine_properties(routine: &RawRoutine) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_i64(&mut properties, "postgres_oid", routine.oid);
    insert_string(
        &mut properties,
        "routine_kind",
        match routine.kind {
            'p' => "procedure",
            'a' => "aggregate",
            'w' => "window",
            _ => "function",
        },
    );
    insert_string(&mut properties, "language", &routine.language);
    insert_string(
        &mut properties,
        "identity_arguments",
        &routine.identity_arguments,
    );
    insert_string(&mut properties, "arguments", &routine.arguments_definition);
    insert_optional_string(
        &mut properties,
        "return_type",
        routine.return_type.as_deref(),
    );
    insert_bool(&mut properties, "returns_set", routine.returns_set);
    insert_bool(
        &mut properties,
        "security_definer",
        routine.security_definer,
    );
    insert_bool(&mut properties, "leakproof", routine.leakproof);
    insert_bool(&mut properties, "strict", routine.strict);
    insert_string(
        &mut properties,
        "volatility",
        match routine.volatility {
            'i' => "immutable",
            's' => "stable",
            _ => "volatile",
        },
    );
    insert_string(
        &mut properties,
        "parallel",
        match routine.parallel {
            's' => "safe",
            'r' => "restricted",
            _ => "unsafe",
        },
    );
    insert_bool(
        &mut properties,
        "body_catalog_tracked",
        routine.body_catalog_tracked,
    );
    properties
}

fn routine_parameter_mode(mode: char) -> &'static str {
    match mode {
        'o' => "out",
        'b' => "inout",
        'v' => "variadic",
        't' => "table",
        _ => "in",
    }
}

fn resolve_routine_dependency(
    dependency: &RawDependency,
    relations: &BTreeMap<i64, ObjectKey>,
    columns: &BTreeMap<(i64, i32), ObjectKey>,
    routines: &BTreeMap<i64, ObjectKey>,
    types: &BTreeMap<i64, ObjectKey>,
) -> Result<Option<ObjectKey>, CatalogError> {
    let schema = dependency.target_schema.as_deref().unwrap_or_default();
    if is_system_schema(schema) {
        return Ok(None);
    }
    let target = match dependency.target_class.as_str() {
        "relation" if dependency.target_sub_id > 0 => columns
            .get(&(dependency.target_oid, dependency.target_sub_id))
            .cloned(),
        "relation" => relations.get(&dependency.target_oid).cloned(),
        "routine" => routines.get(&dependency.target_oid).cloned(),
        "type" => types.get(&dependency.target_oid).cloned(),
        other => {
            return Err(CatalogError::Mapping(format!(
                "unsupported routine dependency target class '{other}'"
            )));
        }
    };
    target.map(Some).ok_or_else(|| {
        CatalogError::Mapping(format!(
            "routine dependency points outside the certified schema scope (class={}, schema={}, oid={}, subid={})",
            dependency.target_class,
            schema,
            dependency.target_oid,
            dependency.target_sub_id
        ))
    })
}

fn trigger_properties(
    trigger: &RawTrigger,
    columns: &BTreeMap<(i64, i32), ObjectKey>,
) -> Result<BTreeMap<String, MetadataValue>, CatalogError> {
    let mut properties = BTreeMap::new();
    insert_i64(&mut properties, "postgres_oid", trigger.oid);
    insert_string(&mut properties, "orientation", &trigger.orientation);
    insert_string(&mut properties, "enabled", trigger.enabled.to_string());
    insert_optional_string(
        &mut properties,
        "when_expression",
        trigger.when_expression.as_deref(),
    );
    let update_columns = trigger
        .update_columns
        .iter()
        .map(|column_number| {
            required(
                columns.get(&(trigger.relation_oid, i32::from(*column_number))),
                format!(
                    "trigger {} UPDATE OF column number {}",
                    trigger.name, column_number
                ),
            )
            .map(|key| {
                key.sub_object
                    .clone()
                    .unwrap_or_else(|| key.object_name.clone())
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if !update_columns.is_empty() {
        properties.insert(
            "update_columns".to_owned(),
            MetadataValue::StringList(update_columns),
        );
    }
    Ok(properties)
}

fn add_type_relationships(
    raw: &RawPostgresCatalog,
    relationships: &mut Vec<MetadataRelationship>,
    types: &BTreeMap<i64, ObjectKey>,
    relations: &BTreeMap<i64, ObjectKey>,
) -> Result<(), CatalogError> {
    for raw_type in &raw.types {
        let source = required(
            types.get(&raw_type.oid),
            format!("type relationship source oid {}", raw_type.oid),
        )?;
        for (target_oid, target_schema, relation_name) in [
            (
                raw_type.base_type_oid,
                raw_type.base_type_schema.as_deref(),
                "domain_base_type",
            ),
            (
                raw_type.element_type_oid,
                raw_type.element_type_schema.as_deref(),
                "element_type",
            ),
            (
                raw_type.range_subtype_oid,
                raw_type.range_subtype_schema.as_deref(),
                "range_subtype",
            ),
            (
                raw_type.multirange_type_oid,
                raw_type.multirange_type_schema.as_deref(),
                "multirange_type",
            ),
        ] {
            let Some(target_oid) = target_oid else {
                continue;
            };
            if target_oid == raw_type.oid {
                continue;
            }
            if let Some(target) = types.get(&target_oid) {
                let mut properties = BTreeMap::new();
                insert_string(&mut properties, "role", relation_name);
                relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::DependsOn,
                    from_key: source.clone(),
                    to_key: target.clone(),
                    ordinal: None,
                    properties,
                });
            } else if !target_schema.map(is_system_schema).unwrap_or(true) {
                return Err(CatalogError::Mapping(format!(
                    "type {} depends on type outside the certified schema scope ({}. oid {})",
                    source,
                    target_schema.unwrap_or_default(),
                    target_oid
                )));
            }
        }
        if let Some(relation_oid) = raw_type.relation_oid {
            if let Some(relation) = relations.get(&relation_oid) {
                relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::DependsOn,
                    from_key: source.clone(),
                    to_key: relation.clone(),
                    ordinal: None,
                    properties: BTreeMap::new(),
                });
            }
        }
    }
    Ok(())
}

fn policy_command(command: char) -> &'static str {
    match command {
        'r' => "select",
        'a' => "insert",
        'w' => "update",
        'd' => "delete",
        _ => "all",
    }
}

fn is_system_schema(schema: &str) -> bool {
    schema == "information_schema" || schema.starts_with("pg_")
}

fn is_base_snapshot_kind(kind: ObjectKind) -> bool {
    matches!(
        kind,
        ObjectKind::Database
            | ObjectKind::Schema
            | ObjectKind::Table
            | ObjectKind::Column
            | ObjectKind::PrimaryKey
            | ObjectKind::ForeignKey
            | ObjectKind::UniqueConstraint
            | ObjectKind::CheckConstraint
            | ObjectKind::Index
            | ObjectKind::View
            | ObjectKind::Trigger
            | ObjectKind::Routine
    )
}

fn deduplicate_metadata_relationships(
    relationships: &mut [MetadataRelationship],
) -> Result<(), CatalogError> {
    relationships.sort_by_key(|relationship| {
        (
            relationship.kind.clone(),
            relationship.from_key.to_string(),
            relationship.to_key.to_string(),
            relationship.ordinal,
        )
    });
    for pair in relationships.windows(2) {
        let left = &pair[0];
        let right = &pair[1];
        if left.kind == right.kind
            && left.from_key == right.from_key
            && left.to_key == right.to_key
            && left.ordinal == right.ordinal
        {
            return Err(CatalogError::Mapping(format!(
                "duplicate canonical metadata relationship {}:{}->{}",
                left.kind.graph_edge_type(),
                left.from_key,
                left.to_key
            )));
        }
    }
    Ok(())
}

fn pg_catalog_complete_capabilities(
    strategy: PgCatalogStrategy,
    raw: &RawPostgresCatalog,
) -> AdapterCapabilities {
    let opaque_routines = raw
        .routines
        .iter()
        .filter(|routine| !routine.body_catalog_tracked)
        .count();
    let mut limitations = Vec::new();
    let (routines, dependencies) = if opaque_routines == 0 {
        (CapabilitySupport::Supported, CapabilitySupport::Supported)
    } else {
        limitations.push(format!(
            "{} routine body or dependency path(s) are opaque; only catalog-proven routine edges are emitted",
            opaque_routines
        ));
        (CapabilitySupport::Partial, CapabilitySupport::Partial)
    };
    AdapterCapabilities {
        source_kind: strategy.source_kind().to_owned(),
        metadata_only: true,
        schemas: true,
        tables: true,
        columns: true,
        constraints: true,
        indexes: true,
        views: CapabilitySupport::Supported,
        triggers: CapabilitySupport::Supported,
        routines,
        dependencies,
        limitations,
        notes: vec![
            format!(
                "Reads {} pg_catalog metadata in one read-only repeatable-read transaction; application rows are never queried.",
                strategy.product_name()
            ),
            "Only pg_catalog-proven routine dependencies are emitted; opaque routine bodies remain structural boundary objects."
                .to_owned(),
            "System-schema implementation dependencies are outside the declared application schema scope."
                .to_owned(),
        ],
    }
}

fn discovery_counts_from_catalog(
    raw: &RawPostgresCatalog,
    snapshot: &CanonicalSchemaSnapshot,
) -> Result<DiscoveryCounts, CatalogError> {
    let relation_kinds = raw
        .relations
        .iter()
        .map(|relation| (relation.oid, relation.relkind))
        .collect::<BTreeMap<_, _>>();
    let mut objects = ObjectCategory::ALL
        .into_iter()
        .map(|category| (category, 0_u64))
        .collect::<BTreeMap<_, _>>();
    objects.insert(ObjectCategory::Database, 1);
    objects.insert(ObjectCategory::Schema, raw.schemas.len() as u64);
    objects.insert(
        ObjectCategory::Table,
        raw.relations
            .iter()
            .filter(|relation| matches!(relation.relkind, 'r' | 'p' | 'f'))
            .count() as u64,
    );
    objects.insert(
        ObjectCategory::Column,
        raw.columns
            .iter()
            .filter(|column| matches!(column.relation_kind, 'r' | 'p' | 'f'))
            .count() as u64,
    );
    objects.insert(
        ObjectCategory::PrimaryKey,
        raw.constraints
            .iter()
            .filter(|constraint| constraint.kind == 'p')
            .count() as u64,
    );
    objects.insert(
        ObjectCategory::ForeignKey,
        raw.constraints
            .iter()
            .filter(|constraint| constraint.kind == 'f')
            .count() as u64,
    );
    objects.insert(
        ObjectCategory::UniqueConstraint,
        raw.constraints
            .iter()
            .filter(|constraint| constraint.kind == 'u')
            .count() as u64,
    );
    objects.insert(
        ObjectCategory::CheckConstraint,
        raw.constraints
            .iter()
            .filter(|constraint| constraint.kind == 'c')
            .count() as u64,
    );
    objects.insert(ObjectCategory::Index, raw.indexes.len() as u64);
    objects.insert(
        ObjectCategory::View,
        raw.relations
            .iter()
            .filter(|relation| relation.relkind == 'v')
            .count() as u64,
    );
    objects.insert(
        ObjectCategory::ViewColumn,
        raw.columns
            .iter()
            .filter(|column| matches!(column.relation_kind, 'v' | 'm'))
            .count() as u64,
    );
    objects.insert(ObjectCategory::Trigger, raw.triggers.len() as u64);
    objects.insert(ObjectCategory::Routine, raw.routines.len() as u64);
    objects.insert(
        ObjectCategory::MaterializedView,
        raw.relations
            .iter()
            .filter(|relation| relation.relkind == 'm')
            .count() as u64,
    );
    objects.insert(ObjectCategory::Sequence, raw.sequences.len() as u64);
    objects.insert(
        ObjectCategory::RoutineParameter,
        raw.routine_parameters.len() as u64,
    );
    objects.insert(
        ObjectCategory::UserDefinedType,
        raw.types
            .iter()
            .filter(|raw_type| raw_type.kind != 'd')
            .count() as u64,
    );
    objects.insert(
        ObjectCategory::Domain,
        raw.types
            .iter()
            .filter(|raw_type| raw_type.kind == 'd')
            .count() as u64,
    );
    objects.insert(ObjectCategory::EnumValue, raw.enum_values.len() as u64);
    objects.insert(
        ObjectCategory::ExclusionConstraint,
        raw.constraints
            .iter()
            .filter(|constraint| constraint.kind == 'x')
            .count() as u64,
    );
    objects.insert(ObjectCategory::Event, raw.event_triggers.len() as u64);
    objects.insert(ObjectCategory::Principal, raw.principals.len() as u64);
    objects.insert(ObjectCategory::Policy, raw.policies.len() as u64);
    objects.insert(
        ObjectCategory::Extension,
        (raw.extensions.len()
            + raw
                .columns
                .iter()
                .filter(|column| column.relation_kind == 'c')
                .count()
            + raw
                .yugabyte
                .as_ref()
                .map(|catalog| catalog.tablegroups.len() + catalog.tablespaces.len())
                .unwrap_or_default()) as u64,
    );

    let emitted_objects = emitted_object_counts(snapshot);
    for category in ObjectCategory::ALL {
        let discovered = objects.get(&category).copied().unwrap_or_default();
        let emitted = emitted_objects.get(&category).copied().unwrap_or_default();
        if discovered != emitted {
            return Err(CatalogError::Mapping(format!(
                "{} raw/emitted object count mismatch for {category:?}: discovered={discovered}, emitted={emitted}",
                raw.strategy.product_name()
            )));
        }
    }

    let mut relationships = emitted_relationship_counts(snapshot);
    relationships.insert(
        RelationshipCategory::DatabaseHasSchema,
        raw.schemas.len() as u64,
    );
    relationships.insert(
        RelationshipCategory::SchemaHasTable,
        raw.relations
            .iter()
            .filter(|relation| matches!(relation.relkind, 'r' | 'p' | 'f'))
            .count() as u64,
    );
    relationships.insert(
        RelationshipCategory::TableHasColumn,
        raw.columns
            .iter()
            .filter(|column| matches!(column.relation_kind, 'r' | 'p' | 'f'))
            .count() as u64,
    );
    relationships.insert(
        RelationshipCategory::TableHasConstraint,
        raw.constraints
            .iter()
            .filter(|constraint| constraint.relation_oid.is_some() && constraint.kind != 'x')
            .count() as u64,
    );
    relationships.insert(
        RelationshipCategory::ConstraintColumn,
        raw.constraints
            .iter()
            .filter(|constraint| {
                constraint.relation_oid.is_some() && matches!(constraint.kind, 'p' | 'u' | 'c')
            })
            .map(|constraint| constraint.columns.len() as u64)
            .sum(),
    );
    relationships.insert(
        RelationshipCategory::ForeignKeyColumnPair,
        raw.constraints
            .iter()
            .filter(|constraint| constraint.kind == 'f')
            .map(|constraint| constraint.columns.len() as u64)
            .sum(),
    );
    relationships.insert(
        RelationshipCategory::TableHasIndex,
        raw.indexes
            .iter()
            .filter(|index| {
                relation_kinds
                    .get(&index.relation_oid)
                    .map(|kind| matches!(kind, 'r' | 'p' | 'f'))
                    .unwrap_or(false)
            })
            .count() as u64,
    );
    let base_index_oids = raw
        .indexes
        .iter()
        .filter(|index| {
            relation_kinds
                .get(&index.relation_oid)
                .map(|kind| matches!(kind, 'r' | 'p' | 'f'))
                .unwrap_or(false)
        })
        .map(|index| index.oid)
        .collect::<BTreeSet<_>>();
    let unique_index_columns = raw
        .index_terms
        .iter()
        .filter(|term| {
            base_index_oids.contains(&term.index_oid) && term.is_key && term.column_number > 0
        })
        .map(|term| (term.index_oid, term.column_number))
        .collect::<BTreeSet<_>>();
    relationships.insert(
        RelationshipCategory::IndexColumn,
        unique_index_columns.len() as u64,
    );
    relationships.insert(
        RelationshipCategory::SchemaHasView,
        raw.relations
            .iter()
            .filter(|relation| relation.relkind == 'v')
            .count() as u64,
    );
    relationships.insert(
        RelationshipCategory::ViewDependency,
        raw.view_dependencies
            .iter()
            .filter(|dependency| {
                !is_system_schema(&dependency.target_schema)
                    && relation_kinds.get(&dependency.view_oid) == Some(&'v')
                    && relation_kinds
                        .get(&dependency.target_relation_oid)
                        .map(|kind| {
                            if dependency.target_column_number > 0 {
                                matches!(kind, 'r' | 'p' | 'f')
                            } else {
                                matches!(kind, 'r' | 'p' | 'f' | 'v')
                            }
                        })
                        .unwrap_or(false)
            })
            .map(|dependency| {
                (
                    dependency.view_oid,
                    dependency.target_relation_oid,
                    dependency.target_column_number,
                )
            })
            .collect::<BTreeSet<_>>()
            .len() as u64,
    );
    relationships.insert(
        RelationshipCategory::TriggerTarget,
        raw.triggers.len() as u64,
    );
    relationships.insert(
        RelationshipCategory::TriggerRoutine,
        raw.triggers.len() as u64,
    );
    relationships.insert(
        RelationshipCategory::SchemaHasRoutine,
        raw.routines.len() as u64,
    );
    relationships.insert(
        RelationshipCategory::RoutineDependency,
        raw.routine_dependencies
            .iter()
            .filter(|dependency| {
                !dependency
                    .target_schema
                    .as_deref()
                    .map(is_system_schema)
                    .unwrap_or(true)
            })
            .map(|dependency| {
                (
                    dependency.owner_oid,
                    dependency.target_class.clone(),
                    dependency.target_oid,
                    dependency.target_sub_id,
                )
            })
            .collect::<BTreeSet<_>>()
            .len() as u64,
    );

    let emitted_relationships = emitted_relationship_counts(snapshot);
    for category in RelationshipCategory::ALL {
        let discovered = relationships.get(&category).copied().unwrap_or_default();
        let emitted = emitted_relationships
            .get(&category)
            .copied()
            .unwrap_or_default();
        if discovered != emitted {
            return Err(CatalogError::Mapping(format!(
                "{} raw/emitted relationship count mismatch for {category:?}: discovered={discovered}, emitted={emitted}",
                raw.strategy.product_name()
            )));
        }
    }

    Ok(DiscoveryCounts {
        objects: objects
            .into_iter()
            .map(|(category, count)| {
                (
                    category,
                    DiscoveredCount {
                        count,
                        evidence: format!(
                            "{} pg_catalog raw object inventory for {category:?} in the declared schema scope",
                            raw.strategy.product_name()
                        ),
                    },
                )
            })
            .collect(),
        relationships: relationships
            .into_iter()
            .map(|(category, count)| {
                (
                    category,
                    DiscoveredCount {
                        count,
                        evidence: format!(
                            "{} pg_catalog relationship ledger for {category:?} in the declared schema scope",
                            raw.strategy.product_name()
                        ),
                    },
                )
            })
            .collect(),
    })
}

