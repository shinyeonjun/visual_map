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
