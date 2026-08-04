fn validate_raw_table_inventory(raw: &RawMysqlFamilyCatalog) -> Result<(), CatalogError> {
    let mut table_names = BTreeSet::new();
    let mut catalog_views = BTreeSet::new();
    let mut catalog_sequences = BTreeSet::new();
    for table in &raw.tables {
        let name = normalize_object_name(&table.name, raw.facts.lower_case_table_names);
        if !table_names.insert(name.clone()) {
            return Err(CatalogError::Mapping(format!(
                "TABLES contains duplicate name '{}'",
                table.name
            )));
        }
        match table.table_type.to_ascii_uppercase().as_str() {
            "BASE TABLE" => {}
            "VIEW" => {
                catalog_views.insert(name);
            }
            "SEQUENCE" if raw.strategy.product() == MysqlProduct::MariaDb => {
                catalog_sequences.insert(name);
            }
            unsupported => {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "table-like object '{}' has unsupported TABLE_TYPE '{unsupported}'",
                    table.name
                )));
            }
        }
    }
    let views = raw
        .views
        .iter()
        .map(|view| normalize_object_name(&view.name, raw.facts.lower_case_table_names))
        .collect::<BTreeSet<_>>();
    if catalog_views != views || views.len() != raw.views.len() {
        return Err(CatalogError::Mapping(
            "TABLES view inventory does not reconcile with VIEWS".to_owned(),
        ));
    }
    let sequences = raw
        .sequences
        .iter()
        .map(|sequence| normalize_object_name(&sequence.name, raw.facts.lower_case_table_names))
        .collect::<BTreeSet<_>>();
    if catalog_sequences != sequences || sequences.len() != raw.sequences.len() {
        return Err(CatalogError::Mapping(
            "TABLES sequence inventory does not reconcile with sequence metadata".to_owned(),
        ));
    }
    Ok(())
}

fn family_key(
    source_kind: &str,
    connection_alias: &str,
    database: &str,
    object_kind: ObjectKind,
    object_name: &str,
    sub_object: Option<String>,
) -> ObjectKey {
    ObjectKey::new(
        source_kind,
        connection_alias,
        database,
        database,
        object_kind,
        object_name,
        sub_object,
    )
}

fn normalize_object_name(value: &str, lower_case_table_names: u64) -> String {
    if lower_case_table_names == 0 {
        value.to_owned()
    } else {
        value.to_ascii_lowercase()
    }
}

fn normalize_column_name(value: &str) -> String {
    value.to_ascii_lowercase()
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

fn insert_u64(properties: &mut BTreeMap<String, MetadataValue>, key: &str, value: u64) {
    properties.insert(key.to_owned(), MetadataValue::Unsigned(value));
}
