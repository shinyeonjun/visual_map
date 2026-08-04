fn add_included_column_relationships(
    metadata: &mut CanonicalMetadata,
    index_key: &ObjectKey,
    index: &RawIndex,
    column_keys: &BTreeMap<(i32, i32), ObjectKey>,
) -> Result<(), CatalogError> {
    for column in index.columns.iter().filter(|column| column.included) {
        let column_key = column_keys
            .get(&(index.object_id, column.column_id))
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "included index column '{}.{}' is not mapped",
                    index.name, column.name
                ))
            })?;
        let mut properties = BTreeMap::new();
        insert_i64(
            &mut properties,
            "index_column_id",
            i64::from(column.index_column_id),
        );
        add_relationship(
            metadata,
            MetadataRelationshipKind::IncludesColumn,
            index_key,
            column_key,
            Some(positive_u32(
                column.index_column_id,
                "index column ordinal",
            )?),
            properties,
        );
    }
    Ok(())
}

fn routine_kind(type_code: &str) -> Result<RoutineKind, CatalogError> {
    match type_code {
        "P" => Ok(RoutineKind::Procedure),
        "FN" | "IF" | "TF" => Ok(RoutineKind::Function),
        unsupported => Err(CatalogError::UnsupportedMetadata(format!(
            "routine type '{unsupported}' is not SQL-backed"
        ))),
    }
}

fn constraint_object_kind(kind: ConstraintKind) -> ObjectKind {
    match kind {
        ConstraintKind::PrimaryKey => ObjectKind::PrimaryKey,
        ConstraintKind::ForeignKey => ObjectKind::ForeignKey,
        ConstraintKind::Unique => ObjectKind::UniqueConstraint,
        ConstraintKind::Check => ObjectKind::CheckConstraint,
    }
}

fn qualified_type_name(schema: &str, name: &str) -> String {
    if schema.eq_ignore_ascii_case("sys") {
        name.to_owned()
    } else {
        format!("{schema}.{name}")
    }
}

fn positive_u32(value: i32, subject: &str) -> Result<u32, CatalogError> {
    u32::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| CatalogError::Mapping(format!("{subject} must be a positive u32")))
}

fn positive_u32_or_return(value: i32) -> Result<u32, CatalogError> {
    if value == 0 {
        Ok(1)
    } else {
        positive_u32(value + 1, "parameter relationship ordinal")
    }
}

fn require_contiguous_ordinals(
    ordinals: impl IntoIterator<Item = i32>,
    subject: &str,
) -> Result<(), CatalogError> {
    for (position, ordinal) in ordinals.into_iter().enumerate() {
        let expected = i32::try_from(position + 1)
            .map_err(|_| CatalogError::Mapping(format!("{subject} ordinal overflow")))?;
        if ordinal != expected {
            return Err(CatalogError::Mapping(format!(
                "{subject} has ordinal {ordinal}, expected {expected}"
            )));
        }
    }
    Ok(())
}

fn add_effective_owner(
    metadata: &mut CanonicalMetadata,
    object_key: &ObjectKey,
    principal_id: Option<i32>,
    schema: &str,
    schema_owner_ids: &BTreeMap<String, i32>,
    principal_keys: &BTreeMap<i32, ObjectKey>,
) -> Result<(), CatalogError> {
    let owner = principal_id
        .or_else(|| schema_owner_ids.get(schema).copied())
        .ok_or_else(|| {
            CatalogError::Mapping(format!(
                "object '{}' has no effective owner",
                object_key.object_name
            ))
        })?;
    add_owned_by(metadata, object_key, owner, principal_keys)
}

fn add_owned_by(
    metadata: &mut CanonicalMetadata,
    object_key: &ObjectKey,
    principal_id: i32,
    principal_keys: &BTreeMap<i32, ObjectKey>,
) -> Result<(), CatalogError> {
    let principal_key = principal_keys.get(&principal_id).ok_or_else(|| {
        CatalogError::Mapping(format!(
            "object '{}' references missing principal {principal_id}",
            object_key.object_name
        ))
    })?;
    add_relationship(
        metadata,
        MetadataRelationshipKind::OwnedBy,
        object_key,
        principal_key,
        None,
        BTreeMap::new(),
    );
    Ok(())
}

fn add_principal_memberships(
    metadata: &mut CanonicalMetadata,
    principals: &[RawPrincipal],
    principal_keys: &BTreeMap<i32, ObjectKey>,
) -> Result<(), CatalogError> {
    for principal in principals {
        let Some(owner_id) = principal.owning_principal_id else {
            continue;
        };
        let source = principal_keys.get(&principal.id).ok_or_else(|| {
            CatalogError::Mapping(format!("principal {} lost its key", principal.id))
        })?;
        let owner = principal_keys.get(&owner_id).ok_or_else(|| {
            CatalogError::Mapping(format!(
                "principal '{}' references missing owner {owner_id}",
                principal.name
            ))
        })?;
        add_relationship(
            metadata,
            MetadataRelationshipKind::OwnedBy,
            source,
            owner,
            None,
            BTreeMap::new(),
        );
    }
    Ok(())
}

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

fn add_relationship(
    metadata: &mut CanonicalMetadata,
    kind: MetadataRelationshipKind,
    from_key: &ObjectKey,
    to_key: &ObjectKey,
    ordinal: Option<u32>,
    properties: BTreeMap<String, MetadataValue>,
) {
    metadata.relationships.push(MetadataRelationship {
        kind,
        from_key: from_key.clone(),
        to_key: to_key.clone(),
        ordinal,
        properties,
    });
}

fn insert_string(properties: &mut BTreeMap<String, MetadataValue>, key: &str, value: &str) {
    properties.insert(key.to_owned(), MetadataValue::String(value.to_owned()));
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
