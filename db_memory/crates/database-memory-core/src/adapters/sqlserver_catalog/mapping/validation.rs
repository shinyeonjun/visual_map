fn validate_raw_inventory(raw: &RawSqlServerCatalog) -> Result<(), CatalogError> {
    require_unique(raw.schemas.iter().map(|schema| schema.id), "schema id")?;
    require_unique(
        raw.principals.iter().map(|principal| principal.id),
        "principal id",
    )?;
    require_unique(raw.tables.iter().map(|table| table.id), "table id")?;
    require_unique(
        raw.columns
            .iter()
            .map(|column| (column.object_id, column.id)),
        "column identity",
    )?;
    require_unique(raw.views.iter().map(|view| view.id), "view id")?;
    require_unique(raw.routines.iter().map(|routine| routine.id), "routine id")?;
    require_unique(
        raw.parameters
            .iter()
            .map(|parameter| (parameter.object_id, parameter.id)),
        "routine parameter identity",
    )?;
    require_unique(raw.triggers.iter().map(|trigger| trigger.id), "trigger id")?;
    require_unique(
        raw.constraints.iter().map(|constraint| constraint.id),
        "constraint id",
    )?;
    require_unique(
        raw.user_types.iter().map(|data_type| data_type.id),
        "user-defined type id",
    )?;
    require_unique(
        raw.sequences.iter().map(|sequence| sequence.id),
        "sequence id",
    )?;
    require_unique(raw.synonyms.iter().map(|synonym| synonym.id), "synonym id")?;
    require_unique(
        raw.indexes.iter().map(|index| (index.object_id, index.id)),
        "index identity",
    )?;
    require_unique(
        raw.partition_functions.iter().map(|function| function.id),
        "partition function id",
    )?;
    require_unique(
        raw.partition_schemes.iter().map(|scheme| scheme.id),
        "partition scheme id",
    )?;
    require_unique(
        raw.security_policies.iter().map(|policy| policy.id),
        "security policy id",
    )?;
    require_unique(
        raw.security_policies.iter().flat_map(|policy| {
            policy
                .predicates
                .iter()
                .map(move |predicate| (policy.id, predicate.id))
        }),
        "security predicate identity",
    )?;
    require_unique(
        raw.xml_schema_collections
            .iter()
            .map(|collection| collection.id),
        "XML schema collection id",
    )?;
    require_unique(
        raw.xml_schema_collections.iter().flat_map(|collection| {
            collection
                .namespaces
                .iter()
                .map(move |namespace| (collection.id, namespace.id))
        }),
        "XML schema namespace identity",
    )?;
    require_unique(
        raw.extended_properties.iter().map(|property| {
            (
                property.class,
                property.major_id,
                property.minor_id,
                property.name.clone(),
            )
        }),
        "extended property identity",
    )?;
    for property in &raw.extended_properties {
        let value_is_null = property.value_type.is_none();
        let typed_fields_are_empty = property.value_precision.is_none()
            && property.value_scale.is_none()
            && property.value_max_length.is_none()
            && property.value_collation.is_none()
            && property.display_value.is_none()
            && property.value_hex.is_none();
        if value_is_null != typed_fields_are_empty {
            return Err(CatalogError::Mapping(format!(
                "extended property '{}:{}:{}:{}' has inconsistent sql_variant metadata",
                property.class, property.major_id, property.minor_id, property.name
            )));
        }
    }
    Ok(())
}

fn require_unique<T: Ord>(
    values: impl IntoIterator<Item = T>,
    subject: &str,
) -> Result<(), CatalogError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(CatalogError::Mapping(format!(
                "duplicate {subject} in raw catalog"
            )));
        }
    }
    Ok(())
}
