fn set_object_count(
    counts: &mut BTreeMap<ObjectCategory, DiscoveredCount>,
    category: ObjectCategory,
    count: usize,
) {
    counts
        .get_mut(&category)
        .expect("all object categories exist")
        .count = count as u64;
}

fn set_relationship_count(
    counts: &mut BTreeMap<RelationshipCategory, DiscoveredCount>,
    category: RelationshipCategory,
    count: usize,
) {
    counts
        .get_mut(&category)
        .expect("all relationship categories exist")
        .count = count as u64;
}

fn oracle_key(
    connection_alias: &str,
    database: &str,
    schema: &str,
    kind: ObjectKind,
    object_name: &str,
    sub_object: Option<String>,
) -> ObjectKey {
    ObjectKey::new(
        ORACLE_SOURCE,
        connection_alias,
        database,
        schema,
        kind,
        object_name,
        sub_object,
    )
}

fn required<T>(value: Option<&T>, subject: impl Into<String>) -> Result<&T, CatalogError> {
    value.ok_or_else(|| {
        CatalogError::Mapping(format!("missing {subject}", subject = subject.into()))
    })
}

fn positive_u32(value: i64, subject: &str) -> Result<u32, CatalogError> {
    u32::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| CatalogError::Mapping(format!("invalid {subject}: {value}")))
}

fn insert_bool(properties: &mut BTreeMap<String, MetadataValue>, name: &str, value: bool) {
    properties.insert(name.to_owned(), MetadataValue::Boolean(value));
}

fn insert_i64(properties: &mut BTreeMap<String, MetadataValue>, name: &str, value: i64) {
    properties.insert(name.to_owned(), MetadataValue::Integer(value));
}

fn insert_optional_i64(
    properties: &mut BTreeMap<String, MetadataValue>,
    name: &str,
    value: Option<i64>,
) {
    if let Some(value) = value {
        insert_i64(properties, name, value);
    }
}

fn insert_string(
    properties: &mut BTreeMap<String, MetadataValue>,
    name: &str,
    value: impl ToString,
) {
    properties.insert(name.to_owned(), MetadataValue::String(value.to_string()));
}

fn insert_optional_string(
    properties: &mut BTreeMap<String, MetadataValue>,
    name: &str,
    value: Option<&str>,
) {
    if let Some(value) = value {
        insert_string(properties, name, value);
    }
}
