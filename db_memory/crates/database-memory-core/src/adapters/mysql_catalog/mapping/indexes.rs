#[allow(clippy::too_many_arguments)]
fn map_indexes(
    metadata: &mut CanonicalMetadata,
    source_kind: &str,
    connection_alias: &str,
    database: &str,
    lower_case_table_names: u64,
    raw_parts: &[RawIndexPart],
    table_keys: &BTreeMap<String, ObjectKey>,
    column_keys: &BTreeMap<(String, String), ObjectKey>,
) -> Result<Vec<IndexObject>, CatalogError> {
    let mut grouped = BTreeMap::<(String, String), Vec<&RawIndexPart>>::new();
    for part in raw_parts {
        grouped
            .entry((
                normalize_object_name(&part.table, lower_case_table_names),
                part.index.clone(),
            ))
            .or_default()
            .push(part);
    }
    let mut indexes = Vec::new();
    for ((table_name, index_name), mut parts) in grouped {
        parts.sort_by_key(|part| part.ordinal);
        require_contiguous_ordinals(
            parts.iter().map(|part| part.ordinal),
            &format!("index '{table_name}.{index_name}'"),
        )?;
        let first = parts[0];
        if parts.iter().any(|part| {
            part.non_unique != first.non_unique
                || part.index_type != first.index_type
                || part.visible != first.visible
                || part.comment != first.comment
                || part.index_comment != first.index_comment
        }) {
            return Err(CatalogError::Mapping(format!(
                "index '{table_name}.{index_name}' has inconsistent part metadata"
            )));
        }
        let table_key = table_keys.get(&table_name).cloned().ok_or_else(|| {
            CatalogError::Mapping(format!(
                "index '{table_name}.{index_name}' targets a non-base or missing table"
            ))
        })?;
        let mut columns = Vec::new();
        let mut expressions = Vec::new();
        let mut part_descriptions = Vec::new();
        for part in parts {
            match (part.column.as_deref(), part.expression.as_deref()) {
                (Some(column), None) => {
                    columns.push(
                        column_keys
                            .get(&(
                                table_name.clone(),
                                normalize_column_name(column),
                            ))
                            .cloned()
                            .ok_or_else(|| {
                                CatalogError::Mapping(format!(
                                    "index '{table_name}.{index_name}' references missing column '{column}'"
                                ))
                            })?,
                    );
                    part_descriptions.push(format_index_part(part, column));
                }
                (None, Some(expression)) if !expression.trim().is_empty() => {
                    expressions.push(expression.to_owned());
                    part_descriptions.push(format_index_part(part, expression));
                }
                (Some(_), Some(_)) => {
                    return Err(CatalogError::Mapping(format!(
                        "index '{table_name}.{index_name}' part {} has both column and expression",
                        part.ordinal
                    )));
                }
                _ => {
                    return Err(CatalogError::Mapping(format!(
                        "index '{table_name}.{index_name}' part {} has neither column nor expression",
                        part.ordinal
                    )));
                }
            }
        }
        let key = family_key(
            source_kind,
            connection_alias,
            database,
            ObjectKind::Index,
            &first.table,
            Some(index_name.clone()),
        );
        let mut properties = BTreeMap::new();
        insert_string(&mut properties, "index_type", &first.index_type);
        insert_bool(&mut properties, "visible", first.visible);
        insert_string(&mut properties, "comment", &first.comment);
        insert_string(&mut properties, "index_comment", &first.index_comment);
        properties.insert(
            "parts".to_owned(),
            MetadataValue::StringList(part_descriptions),
        );
        add_annotation(metadata, &key, None, properties);
        indexes.push(IndexObject {
            key,
            table_key,
            name: index_name.clone(),
            columns,
            is_unique: !first.non_unique,
            is_primary: index_name == "PRIMARY",
            predicate: None,
            expression: (!expressions.is_empty()).then(|| expressions.join(", ")),
        });
    }
    Ok(indexes)
}

fn format_index_part(part: &RawIndexPart, value: &str) -> String {
    let mut description = format!("{}:{value}", part.ordinal);
    if let Some(prefix_length) = part.prefix_length {
        description.push_str(&format!(":prefix={prefix_length}"));
    }
    if let Some(collation) = part.collation.as_deref() {
        description.push_str(&format!(":order={collation}"));
    }
    description
}

fn require_contiguous_ordinals(
    ordinals: impl IntoIterator<Item = u32>,
    subject: &str,
) -> Result<(), CatalogError> {
    for (index, ordinal) in ordinals.into_iter().enumerate() {
        let expected = u32::try_from(index + 1)
            .map_err(|_| CatalogError::Mapping(format!("{subject} has too many terms")))?;
        if ordinal != expected {
            return Err(CatalogError::Mapping(format!(
                "{subject} ordinal {ordinal} is not contiguous; expected {expected}"
            )));
        }
    }
    Ok(())
}
