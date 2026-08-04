fn validate_lob_catalog(
    raw: &RawOracleCatalog,
    scope: &DictionaryScope,
    inventory_keys: &BTreeSet<(String, String, String)>,
    inventory_subobject_keys: &BTreeSet<(String, String, String, String)>,
    tables: &BTreeSet<(String, String)>,
    column_keys: &BTreeSet<(String, String, String)>,
) -> Result<(), CatalogError> {
    let raw_tables = raw
        .tables
        .iter()
        .map(|table| ((table.owner.clone(), table.name.clone()), table))
        .collect::<BTreeMap<_, _>>();
    let mut lobs = BTreeMap::new();
    let mut segment_names = BTreeSet::new();
    let mut index_names = BTreeSet::new();
    for lob in &raw.lobs {
        ensure_owner(scope, &lob.owner, "LOB")?;
        let table = raw_tables
            .get(&(lob.owner.clone(), lob.table.clone()))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle LOB {}.{}.{} has no parent table",
                    lob.owner, lob.table, lob.column
                ))
            })?;
        if !tables.contains(&(lob.owner.clone(), lob.table.clone()))
            || !column_keys.contains(&(lob.owner.clone(), lob.table.clone(), lob.column.clone()))
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle LOB {}.{}.{} has no parent column",
                lob.owner, lob.table, lob.column
            )));
        }
        ensure_yes_no(
            &lob.partitioned,
            &format!(
                "Oracle LOB {}.{}.{} partitioned",
                lob.owner, lob.table, lob.column
            ),
        )?;
        ensure_yes_no(
            &lob.securefile,
            &format!(
                "Oracle LOB {}.{}.{} securefile",
                lob.owner, lob.table, lob.column
            ),
        )?;
        if (lob.partitioned == "YES") != table.partitioned
            || lob.chunk <= 0
            || lob.pctversion.is_some_and(|value| value < 0)
            || lob.retention.is_some_and(|value| value < 0)
            || lob.freepools.is_some_and(|value| value < 0)
            || lob.retention_value.is_some_and(|value| value < 0)
            || lob.max_inline.is_some_and(|value| value < 0)
            || [
                lob.cache.as_str(),
                lob.logging.as_str(),
                lob.encrypt.as_str(),
                lob.compression.as_str(),
                lob.deduplication.as_str(),
                lob.in_row.as_str(),
                lob.format.as_str(),
                lob.segment_created.as_str(),
            ]
            .iter()
            .any(|value| value.is_empty())
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle LOB metadata is malformed for {}.{}.{}",
                lob.owner, lob.table, lob.column
            )));
        }
        if !inventory_keys.contains(&(
            lob.owner.clone(),
            "LOB".to_owned(),
            lob.segment_name.clone(),
        )) || !inventory_keys.contains(&(
            lob.owner.clone(),
            "INDEX".to_owned(),
            lob.index_name.clone(),
        )) {
            return Err(CatalogError::Mapping(format!(
                "Oracle LOB {}.{}.{} is missing its segment or index inventory row",
                lob.owner, lob.table, lob.column
            )));
        }
        if !segment_names.insert((lob.owner.clone(), lob.segment_name.clone()))
            || !index_names.insert((lob.owner.clone(), lob.index_name.clone()))
        {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle LOB segment or index identity for {}.{}.{}",
                lob.owner, lob.table, lob.column
            )));
        }
        let identity = (lob.owner.clone(), lob.table.clone(), lob.column.clone());
        if lobs.insert(identity.clone(), lob).is_some() {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle LOB column {}.{}.{}",
                identity.0, identity.1, identity.2
            )));
        }
    }
    let inventory_lob_count = inventory_keys.iter().filter(|key| key.1 == "LOB").count();
    if inventory_lob_count != raw.lobs.len() {
        return Err(CatalogError::Mapping(format!(
            "Oracle LOB inventory mismatch: USER/DBA_OBJECTS reports {inventory_lob_count}, USER/DBA_LOBS reports {}",
            raw.lobs.len()
        )));
    }

    let table_partitions = raw
        .table_partitions
        .iter()
        .map(|partition| {
            (
                (
                    partition.owner.clone(),
                    partition.table.clone(),
                    partition.name.clone(),
                ),
                partition,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut lob_partitions = BTreeMap::new();
    let mut lob_partitions_by_lob =
        BTreeMap::<(String, String, String), Vec<&RawLobPartition>>::new();
    let mut lob_index_partition_names = BTreeSet::new();
    for partition in &raw.lob_partitions {
        ensure_owner(scope, &partition.owner, "LOB partition")?;
        let lob = lobs
            .get(&(
                partition.owner.clone(),
                partition.table.clone(),
                partition.column.clone(),
            ))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle LOB partition {}.{}.{} has no parent LOB",
                    partition.owner, partition.table, partition.name
                ))
            })?;
        let table_partition = table_partitions
            .get(&(
                partition.owner.clone(),
                partition.table.clone(),
                partition.table_partition.clone(),
            ))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle LOB partition {}.{}.{} has no table partition {}",
                    partition.owner, partition.table, partition.name, partition.table_partition
                ))
            })?;
        positive_u32(partition.position, "Oracle LOB partition position")?;
        if lob.segment_name != partition.lob_name
            || partition.position != table_partition.position
            || partition.composite != table_partition.composite
            || partition.chunk <= 0
            || partition.pctversion.is_some_and(|value| value < 0)
            || partition.max_inline.is_some_and(|value| value < 0)
            || [
                partition.cache.as_str(),
                partition.in_row.as_str(),
                partition.logging.as_str(),
                partition.encrypt.as_str(),
                partition.compression.as_str(),
                partition.deduplication.as_str(),
                partition.securefile.as_str(),
                partition.segment_created.as_str(),
            ]
            .iter()
            .any(|value| value.is_empty())
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle LOB partition metadata is inconsistent for {}.{}.{}",
                partition.owner, partition.table, partition.name
            )));
        }
        if !inventory_subobject_keys.contains(&(
            partition.owner.clone(),
            "LOB PARTITION".to_owned(),
            partition.lob_name.clone(),
            partition.name.clone(),
        )) || !inventory_subobject_keys.contains(&(
            partition.owner.clone(),
            "INDEX PARTITION".to_owned(),
            lob.index_name.clone(),
            partition.index_partition_name.clone(),
        )) {
            return Err(CatalogError::Mapping(format!(
                "Oracle LOB partition {}.{}.{} is missing its segment or index inventory row",
                partition.owner, partition.table, partition.name
            )));
        }
        if !lob_index_partition_names.insert((
            partition.owner.clone(),
            lob.index_name.clone(),
            partition.index_partition_name.clone(),
        )) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle LOB index partition {}.{}",
                partition.owner, partition.index_partition_name
            )));
        }
        let identity = (
            partition.owner.clone(),
            partition.lob_name.clone(),
            partition.name.clone(),
        );
        if lob_partitions.insert(identity.clone(), partition).is_some() {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle LOB partition {}.{}.{}",
                identity.0, identity.1, identity.2
            )));
        }
        lob_partitions_by_lob
            .entry((
                partition.owner.clone(),
                partition.table.clone(),
                partition.column.clone(),
            ))
            .or_default()
            .push(partition);
    }
    for (identity, lob) in &lobs {
        let partitions = lob_partitions_by_lob
            .get(identity)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let expected = if lob.partitioned == "YES" {
            raw.table_partitions
                .iter()
                .filter(|partition| partition.owner == lob.owner && partition.table == lob.table)
                .count()
        } else {
            0
        };
        if partitions.len() != expected {
            return Err(CatalogError::Mapping(format!(
                "Oracle LOB partition count mismatch for {}.{}.{}",
                identity.0, identity.1, identity.2
            )));
        }
        ensure_contiguous_positions(
            partitions.iter().map(|partition| partition.position),
            &format!(
                "Oracle LOB partitions {}.{}.{}",
                identity.0, identity.1, identity.2
            ),
        )?;
    }
    let inventory_lob_partition_count = inventory_subobject_keys
        .iter()
        .filter(|key| key.1 == "LOB PARTITION")
        .count();
    let inventory_lob_index_partition_count = inventory_subobject_keys
        .iter()
        .filter(|key| {
            key.1 == "INDEX PARTITION" && index_names.contains(&(key.0.clone(), key.2.clone()))
        })
        .count();
    if inventory_lob_partition_count != raw.lob_partitions.len()
        || inventory_lob_index_partition_count != raw.lob_partitions.len()
        || lob_index_partition_names.len() != raw.lob_partitions.len()
    {
        return Err(CatalogError::Mapping(format!(
            "Oracle LOB-partition inventory mismatch: LOB={inventory_lob_partition_count}, INDEX={inventory_lob_index_partition_count}, catalog={}",
            lob_index_partition_names.len()
        )));
    }

    let table_subpartitions = raw
        .table_subpartitions
        .iter()
        .map(|subpartition| {
            (
                (
                    subpartition.owner.clone(),
                    subpartition.table.clone(),
                    subpartition.name.clone(),
                ),
                subpartition,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut lob_subpartition_identities = BTreeSet::new();
    let mut lob_subpartitions_by_partition =
        BTreeMap::<(String, String, String), Vec<&RawLobSubpartition>>::new();
    let mut lob_index_subpartition_names = BTreeSet::new();
    for subpartition in &raw.lob_subpartitions {
        ensure_owner(scope, &subpartition.owner, "LOB subpartition")?;
        let lob = lobs
            .get(&(
                subpartition.owner.clone(),
                subpartition.table.clone(),
                subpartition.column.clone(),
            ))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle LOB subpartition {}.{}.{} has no parent LOB",
                    subpartition.owner, subpartition.table, subpartition.name
                ))
            })?;
        let parent = lob_partitions
            .get(&(
                subpartition.owner.clone(),
                subpartition.lob_name.clone(),
                subpartition.lob_partition_name.clone(),
            ))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle LOB subpartition {}.{}.{} has no parent LOB partition",
                    subpartition.owner, subpartition.table, subpartition.name
                ))
            })?;
        let table_subpartition = table_subpartitions
            .get(&(
                subpartition.owner.clone(),
                subpartition.table.clone(),
                subpartition.table_subpartition.clone(),
            ))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle LOB subpartition {}.{}.{} has no table subpartition {}",
                    subpartition.owner,
                    subpartition.table,
                    subpartition.name,
                    subpartition.table_subpartition
                ))
            })?;
        positive_u32(subpartition.position, "Oracle LOB subpartition position")?;
        if subpartition.lob_name != lob.segment_name
            || table_subpartition.partition != parent.table_partition
            || subpartition.position != table_subpartition.position
            || subpartition.chunk <= 0
            || subpartition.pctversion.is_some_and(|value| value < 0)
            || subpartition.max_inline.is_some_and(|value| value < 0)
            || [
                subpartition.cache.as_str(),
                subpartition.in_row.as_str(),
                subpartition.logging.as_str(),
                subpartition.encrypt.as_str(),
                subpartition.compression.as_str(),
                subpartition.deduplication.as_str(),
                subpartition.securefile.as_str(),
                subpartition.segment_created.as_str(),
            ]
            .iter()
            .any(|value| value.is_empty())
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle LOB subpartition metadata is inconsistent for {}.{}.{}",
                subpartition.owner, subpartition.table, subpartition.name
            )));
        }
        if !inventory_subobject_keys.contains(&(
            subpartition.owner.clone(),
            "LOB SUBPARTITION".to_owned(),
            subpartition.lob_name.clone(),
            subpartition.name.clone(),
        )) || !inventory_subobject_keys.contains(&(
            subpartition.owner.clone(),
            "INDEX SUBPARTITION".to_owned(),
            lob.index_name.clone(),
            subpartition.index_subpartition_name.clone(),
        )) {
            return Err(CatalogError::Mapping(format!(
                "Oracle LOB subpartition {}.{}.{} is missing its segment or index inventory row",
                subpartition.owner, subpartition.table, subpartition.name
            )));
        }
        let identity = (
            subpartition.owner.clone(),
            subpartition.lob_name.clone(),
            subpartition.name.clone(),
        );
        if !lob_subpartition_identities.insert(identity.clone())
            || !lob_index_subpartition_names.insert((
                subpartition.owner.clone(),
                lob.index_name.clone(),
                subpartition.index_subpartition_name.clone(),
            ))
        {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle LOB subpartition {}.{}.{}",
                identity.0, identity.1, identity.2
            )));
        }
        lob_subpartitions_by_partition
            .entry((
                subpartition.owner.clone(),
                subpartition.lob_name.clone(),
                subpartition.lob_partition_name.clone(),
            ))
            .or_default()
            .push(subpartition);
    }
    for (identity, partition) in &lob_partitions {
        let subpartitions = lob_subpartitions_by_partition
            .get(identity)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let expected = table_partitions
            .get(&(
                partition.owner.clone(),
                partition.table.clone(),
                partition.table_partition.clone(),
            ))
            .map_or(0, |table_partition| {
                table_partition.subpartition_count as usize
            });
        if subpartitions.len() != expected {
            return Err(CatalogError::Mapping(format!(
                "Oracle LOB subpartition count mismatch for {}.{}.{}",
                identity.0, identity.1, identity.2
            )));
        }
        ensure_contiguous_positions(
            subpartitions
                .iter()
                .map(|subpartition| subpartition.position),
            &format!(
                "Oracle LOB subpartitions {}.{}.{}",
                identity.0, identity.1, identity.2
            ),
        )?;
    }
    let inventory_lob_subpartition_count = inventory_subobject_keys
        .iter()
        .filter(|key| key.1 == "LOB SUBPARTITION")
        .count();
    let inventory_lob_index_subpartition_count = inventory_subobject_keys
        .iter()
        .filter(|key| {
            key.1 == "INDEX SUBPARTITION" && index_names.contains(&(key.0.clone(), key.2.clone()))
        })
        .count();
    if inventory_lob_subpartition_count != raw.lob_subpartitions.len()
        || inventory_lob_index_subpartition_count != raw.lob_subpartitions.len()
        || lob_index_subpartition_names.len() != raw.lob_subpartitions.len()
    {
        return Err(CatalogError::Mapping(format!(
            "Oracle LOB-subpartition inventory mismatch: LOB={inventory_lob_subpartition_count}, INDEX={inventory_lob_index_subpartition_count}, catalog={}",
            lob_index_subpartition_names.len()
        )));
    }

    Ok(())
}
