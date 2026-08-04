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
