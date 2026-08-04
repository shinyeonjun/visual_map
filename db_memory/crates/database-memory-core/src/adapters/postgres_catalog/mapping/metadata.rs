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
