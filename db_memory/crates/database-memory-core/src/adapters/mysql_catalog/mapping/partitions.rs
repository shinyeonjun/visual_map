#[allow(clippy::too_many_arguments)]
fn map_partitions(
    metadata: &mut CanonicalMetadata,
    source_kind: &str,
    connection_alias: &str,
    database: &str,
    lower_case_table_names: u64,
    raw_partitions: &[RawPartition],
    table_keys: &BTreeMap<String, ObjectKey>,
) -> Result<(), CatalogError> {
    let mut partition_keys = BTreeMap::<(String, String), ObjectKey>::new();
    let mut subpartitions = BTreeSet::new();
    for partition in raw_partitions {
        let table_name = normalize_object_name(&partition.table, lower_case_table_names);
        let table_key = table_keys.get(&table_name).cloned().ok_or_else(|| {
            CatalogError::Mapping(format!(
                "partition '{}.{}' targets missing base table",
                partition.table, partition.partition
            ))
        })?;
        let partition_identity = (table_name.clone(), partition.partition.clone());
        let partition_key = match partition_keys.get(&partition_identity) {
            Some(key) => key.clone(),
            None => {
                let key = family_key(
                    source_kind,
                    connection_alias,
                    database,
                    ObjectKind::Extension,
                    &partition.table,
                    Some(format!("partition:{}", partition.partition)),
                );
                let mut properties = BTreeMap::new();
                insert_u64(
                    &mut properties,
                    "ordinal_position",
                    partition.partition_ordinal as u64,
                );
                insert_optional_string(&mut properties, "method", partition.method.as_deref());
                insert_optional_string(
                    &mut properties,
                    "expression",
                    partition.expression.as_deref(),
                );
                insert_optional_string(
                    &mut properties,
                    "description",
                    partition.description.as_deref(),
                );
                insert_string(&mut properties, "comment", &partition.comment);
                insert_optional_string(
                    &mut properties,
                    "tablespace",
                    partition.tablespace.as_deref(),
                );
                metadata.objects.push(MetadataObject {
                    key: key.clone(),
                    parent_key: Some(table_key),
                    name: partition.partition.clone(),
                    extension_kind: Some("mysql_partition".to_owned()),
                    definition: partition.expression.clone(),
                    properties,
                });
                partition_keys.insert(partition_identity.clone(), key.clone());
                key
            }
        };
        if let Some(subpartition) = partition.subpartition.as_deref() {
            let identity = (
                table_name,
                partition.partition.clone(),
                subpartition.to_owned(),
            );
            if !subpartitions.insert(identity) {
                return Err(CatalogError::Mapping(format!(
                    "duplicate subpartition '{}.{}.{}'",
                    partition.table, partition.partition, subpartition
                )));
            }
            let key = family_key(
                source_kind,
                connection_alias,
                database,
                ObjectKind::Extension,
                &partition.table,
                Some(format!(
                    "partition:{}:subpartition:{subpartition}",
                    partition.partition
                )),
            );
            let mut properties = BTreeMap::new();
            if let Some(ordinal) = partition.subpartition_ordinal {
                insert_u64(&mut properties, "ordinal_position", ordinal as u64);
            }
            insert_optional_string(
                &mut properties,
                "method",
                partition.subpartition_method.as_deref(),
            );
            insert_optional_string(
                &mut properties,
                "expression",
                partition.subpartition_expression.as_deref(),
            );
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(partition_key),
                name: subpartition.to_owned(),
                extension_kind: Some("mysql_subpartition".to_owned()),
                definition: partition.subpartition_expression.clone(),
                properties,
            });
        }
    }
    Ok(())
}
