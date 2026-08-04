fn validate_partition_catalog(
    raw: &RawOracleCatalog,
    scope: &DictionaryScope,
    inventory_subobject_keys: &BTreeSet<(String, String, String, String)>,
    tables: &BTreeSet<(String, String)>,
    column_keys: &BTreeSet<(String, String, String)>,
    indexes: &BTreeSet<(String, String)>,
) -> Result<(), CatalogError> {
    let lob_index_names = raw
        .lobs
        .iter()
        .map(|lob| (lob.owner.clone(), lob.index_name.clone()))
        .collect::<BTreeSet<_>>();
    let raw_tables = raw
        .tables
        .iter()
        .map(|table| ((table.owner.clone(), table.name.clone()), table))
        .collect::<BTreeMap<_, _>>();
    let expected_partitioned_tables = raw
        .tables
        .iter()
        .filter(|table| table.partitioned)
        .map(|table| (table.owner.clone(), table.name.clone()))
        .collect::<BTreeSet<_>>();
    let mut partitioned_tables = BTreeMap::new();
    for table in &raw.partitioned_tables {
        ensure_owner(scope, &table.owner, "partitioned table")?;
        if !tables.contains(&(table.owner.clone(), table.table.clone()))
            || !raw_tables
                .get(&(table.owner.clone(), table.table.clone()))
                .is_some_and(|raw_table| raw_table.partitioned)
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle partition metadata references non-partitioned table {}.{}",
                table.owner, table.table
            )));
        }
        ensure_partitioning_type(
            &table.partitioning_type,
            false,
            &format!("Oracle table {}.{}", table.owner, table.table),
        )?;
        ensure_partitioning_type(
            &table.subpartitioning_type,
            true,
            &format!("Oracle table {}.{}", table.owner, table.table),
        )?;
        if table.status != "VALID"
            || table.partition_count <= 0
            || table.partitioning_key_count <= 0
            || table.default_subpartition_count < 0
            || table.subpartitioning_key_count < 0
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle partition header is malformed for {}.{}",
                table.owner, table.table
            )));
        }
        let has_subpartitions = table.subpartitioning_type != "NONE";
        if has_subpartitions
            != (table.default_subpartition_count > 0 && table.subpartitioning_key_count > 0)
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle subpartition header is inconsistent for {}.{}",
                table.owner, table.table
            )));
        }
        for (name, value) in [
            ("autolist", table.autolist.as_deref()),
            (
                "autolist_subpartition",
                table.autolist_subpartition.as_deref(),
            ),
            ("auto", table.automatic.as_deref()),
        ] {
            if let Some(value) = value {
                ensure_yes_no(
                    value,
                    &format!("Oracle table {}.{} {name}", table.owner, table.table),
                )?;
            }
        }
        let identity = (table.owner.clone(), table.table.clone());
        if partitioned_tables.insert(identity.clone(), table).is_some() {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle partitioned-table header {}.{}",
                identity.0, identity.1
            )));
        }
    }
    if partitioned_tables.keys().cloned().collect::<BTreeSet<_>>() != expected_partitioned_tables {
        return Err(CatalogError::Mapping(
            "Oracle USER/DBA_PART_TABLES does not exactly match partitioned USER/DBA_TABLES rows"
                .to_owned(),
        ));
    }

    let mut table_partitions_by_table =
        BTreeMap::<(String, String), Vec<&RawTablePartition>>::new();
    let mut table_partition_identities = BTreeMap::new();
    for partition in &raw.table_partitions {
        ensure_owner(scope, &partition.owner, "table partition")?;
        let header = partitioned_tables
            .get(&(partition.owner.clone(), partition.table.clone()))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle table partition {}.{}.{} has no partitioned-table header",
                    partition.owner, partition.table, partition.name
                ))
            })?;
        positive_u32(partition.position, "Oracle table partition position")?;
        if partition.subpartition_count < 0
            || !matches!(partition.composite.as_str(), "YES" | "NO")
            || (partition.composite == "YES") != (header.subpartitioning_type != "NONE")
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle table partition metadata is malformed for {}.{}.{}",
                partition.owner, partition.table, partition.name
            )));
        }
        if !inventory_subobject_keys.contains(&(
            partition.owner.clone(),
            "TABLE PARTITION".to_owned(),
            partition.table.clone(),
            partition.name.clone(),
        )) {
            return Err(CatalogError::Mapping(format!(
                "Oracle table partition {}.{}.{} is missing from the independent object inventory",
                partition.owner, partition.table, partition.name
            )));
        }
        let identity = (
            partition.owner.clone(),
            partition.table.clone(),
            partition.name.clone(),
        );
        if table_partition_identities
            .insert(identity.clone(), partition)
            .is_some()
        {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle table partition {}.{}.{}",
                identity.0, identity.1, identity.2
            )));
        }
        table_partitions_by_table
            .entry((partition.owner.clone(), partition.table.clone()))
            .or_default()
            .push(partition);
    }
    for (identity, header) in &partitioned_tables {
        let partitions = table_partitions_by_table
            .get(identity)
            .map(Vec::as_slice)
            .unwrap_or_default();
        if partitions.len() != header.partition_count as usize {
            return Err(CatalogError::Mapping(format!(
                "Oracle table partition count mismatch for {}.{}",
                identity.0, identity.1
            )));
        }
        ensure_contiguous_positions(
            partitions.iter().map(|partition| partition.position),
            &format!("Oracle table partitions {}.{}", identity.0, identity.1),
        )?;
    }
    let inventory_table_partition_count = inventory_subobject_keys
        .iter()
        .filter(|key| key.1 == "TABLE PARTITION")
        .count();
    if inventory_table_partition_count != raw.table_partitions.len() {
        return Err(CatalogError::Mapping(format!(
            "Oracle table-partition inventory mismatch: USER/DBA_OBJECTS reports {inventory_table_partition_count}, USER/DBA_TAB_PARTITIONS reports {}",
            raw.table_partitions.len()
        )));
    }

    let mut table_subpartitions_by_partition =
        BTreeMap::<(String, String, String), Vec<&RawTableSubpartition>>::new();
    let mut table_subpartition_identities = BTreeSet::new();
    for subpartition in &raw.table_subpartitions {
        ensure_owner(scope, &subpartition.owner, "table subpartition")?;
        let parent = table_partition_identities
            .get(&(
                subpartition.owner.clone(),
                subpartition.table.clone(),
                subpartition.partition.clone(),
            ))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle table subpartition {}.{}.{} has no parent partition {}",
                    subpartition.owner,
                    subpartition.table,
                    subpartition.name,
                    subpartition.partition
                ))
            })?;
        positive_u32(subpartition.position, "Oracle table subpartition position")?;
        if subpartition.partition_position != parent.position {
            return Err(CatalogError::Mapping(format!(
                "Oracle table subpartition parent position mismatch for {}.{}.{}",
                subpartition.owner, subpartition.table, subpartition.name
            )));
        }
        if !inventory_subobject_keys.contains(&(
            subpartition.owner.clone(),
            "TABLE SUBPARTITION".to_owned(),
            subpartition.table.clone(),
            subpartition.name.clone(),
        )) {
            return Err(CatalogError::Mapping(format!(
                "Oracle table subpartition {}.{}.{} is missing from the independent object inventory",
                subpartition.owner, subpartition.table, subpartition.name
            )));
        }
        let identity = (
            subpartition.owner.clone(),
            subpartition.table.clone(),
            subpartition.name.clone(),
        );
        if !table_subpartition_identities.insert(identity.clone()) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle table subpartition {}.{}.{}",
                identity.0, identity.1, identity.2
            )));
        }
        table_subpartitions_by_partition
            .entry((
                subpartition.owner.clone(),
                subpartition.table.clone(),
                subpartition.partition.clone(),
            ))
            .or_default()
            .push(subpartition);
    }
    for (identity, parent) in &table_partition_identities {
        let subpartitions = table_subpartitions_by_partition
            .get(identity)
            .map(Vec::as_slice)
            .unwrap_or_default();
        if subpartitions.len() != parent.subpartition_count as usize {
            return Err(CatalogError::Mapping(format!(
                "Oracle table subpartition count mismatch for {}.{}.{}",
                identity.0, identity.1, identity.2
            )));
        }
        ensure_contiguous_positions(
            subpartitions
                .iter()
                .map(|subpartition| subpartition.position),
            &format!(
                "Oracle table subpartitions {}.{}.{}",
                identity.0, identity.1, identity.2
            ),
        )?;
    }
    let inventory_table_subpartition_count = inventory_subobject_keys
        .iter()
        .filter(|key| key.1 == "TABLE SUBPARTITION")
        .count();
    if inventory_table_subpartition_count != raw.table_subpartitions.len() {
        return Err(CatalogError::Mapping(format!(
            "Oracle table-subpartition inventory mismatch: USER/DBA_OBJECTS reports {inventory_table_subpartition_count}, USER/DBA_TAB_SUBPARTITIONS reports {}",
            raw.table_subpartitions.len()
        )));
    }

    let raw_indexes = raw
        .indexes
        .iter()
        .map(|index| ((index.owner.clone(), index.name.clone()), index))
        .collect::<BTreeMap<_, _>>();
    let expected_partitioned_indexes = raw
        .indexes
        .iter()
        .filter(|index| index.partitioned)
        .map(|index| (index.owner.clone(), index.name.clone()))
        .collect::<BTreeSet<_>>();
    let mut partitioned_indexes = BTreeMap::new();
    for index in &raw.partitioned_indexes {
        ensure_owner(scope, &index.owner, "partitioned index")?;
        if !indexes.contains(&(index.owner.clone(), index.index.clone())) {
            return Err(CatalogError::Mapping(format!(
                "Oracle partition metadata references missing index {}.{}",
                index.owner, index.index
            )));
        }
        let raw_index = raw_indexes
            .get(&(index.owner.clone(), index.index.clone()))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle partitioned-index header has no index {}.{}",
                    index.owner, index.index
                ))
            })?;
        if !raw_index.partitioned || raw_index.table != index.table {
            return Err(CatalogError::Mapping(format!(
                "Oracle partitioned-index header disagrees with USER/DBA_INDEXES for {}.{}",
                index.owner, index.index
            )));
        }
        ensure_partitioning_type(
            &index.partitioning_type,
            false,
            &format!("Oracle index {}.{}", index.owner, index.index),
        )?;
        ensure_partitioning_type(
            &index.subpartitioning_type,
            true,
            &format!("Oracle index {}.{}", index.owner, index.index),
        )?;
        if index.partition_count <= 0
            || index.partitioning_key_count <= 0
            || index.default_subpartition_count < 0
            || index.subpartitioning_key_count < 0
            || !matches!(index.locality.as_str(), "LOCAL" | "GLOBAL")
            || !matches!(index.alignment.as_str(), "PREFIXED" | "NON_PREFIXED")
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle partitioned-index header is malformed for {}.{}",
                index.owner, index.index
            )));
        }
        let has_subpartitions = index.subpartitioning_type != "NONE";
        if has_subpartitions
            != (index.default_subpartition_count > 0 && index.subpartitioning_key_count > 0)
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle index subpartition header is inconsistent for {}.{}",
                index.owner, index.index
            )));
        }
        for (name, value) in [
            ("autolist", index.autolist.as_deref()),
            (
                "autolist_subpartition",
                index.autolist_subpartition.as_deref(),
            ),
        ] {
            if let Some(value) = value {
                ensure_yes_no(
                    value,
                    &format!("Oracle index {}.{} {name}", index.owner, index.index),
                )?;
            }
        }
        let identity = (index.owner.clone(), index.index.clone());
        if partitioned_indexes
            .insert(identity.clone(), index)
            .is_some()
        {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle partitioned-index header {}.{}",
                identity.0, identity.1
            )));
        }
    }
    if partitioned_indexes.keys().cloned().collect::<BTreeSet<_>>() != expected_partitioned_indexes
    {
        return Err(CatalogError::Mapping(
            "Oracle USER/DBA_PART_INDEXES does not exactly match partitioned USER/DBA_INDEXES rows"
                .to_owned(),
        ));
    }

    let mut index_partitions_by_index =
        BTreeMap::<(String, String), Vec<&RawIndexPartition>>::new();
    let mut index_partition_identities = BTreeMap::new();
    for partition in &raw.index_partitions {
        ensure_owner(scope, &partition.owner, "index partition")?;
        let header = partitioned_indexes
            .get(&(partition.owner.clone(), partition.index.clone()))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle index partition {}.{}.{} has no partitioned-index header",
                    partition.owner, partition.index, partition.name
                ))
            })?;
        positive_u32(partition.position, "Oracle index partition position")?;
        if partition.subpartition_count < 0
            || !matches!(partition.composite.as_str(), "YES" | "NO")
            || (partition.composite == "YES") != (header.subpartitioning_type != "NONE")
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle index partition metadata is malformed for {}.{}.{}",
                partition.owner, partition.index, partition.name
            )));
        }
        if !inventory_subobject_keys.contains(&(
            partition.owner.clone(),
            "INDEX PARTITION".to_owned(),
            partition.index.clone(),
            partition.name.clone(),
        )) {
            return Err(CatalogError::Mapping(format!(
                "Oracle index partition {}.{}.{} is missing from the independent object inventory",
                partition.owner, partition.index, partition.name
            )));
        }
        let identity = (
            partition.owner.clone(),
            partition.index.clone(),
            partition.name.clone(),
        );
        if index_partition_identities
            .insert(identity.clone(), partition)
            .is_some()
        {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle index partition {}.{}.{}",
                identity.0, identity.1, identity.2
            )));
        }
        index_partitions_by_index
            .entry((partition.owner.clone(), partition.index.clone()))
            .or_default()
            .push(partition);
    }
    for (identity, header) in &partitioned_indexes {
        let partitions = index_partitions_by_index
            .get(identity)
            .map(Vec::as_slice)
            .unwrap_or_default();
        if partitions.len() != header.partition_count as usize {
            return Err(CatalogError::Mapping(format!(
                "Oracle index partition count mismatch for {}.{}",
                identity.0, identity.1
            )));
        }
        ensure_contiguous_positions(
            partitions.iter().map(|partition| partition.position),
            &format!("Oracle index partitions {}.{}", identity.0, identity.1),
        )?;
    }
    let inventory_index_partition_count = inventory_subobject_keys
        .iter()
        .filter(|key| {
            key.1 == "INDEX PARTITION" && !lob_index_names.contains(&(key.0.clone(), key.2.clone()))
        })
        .count();
    if inventory_index_partition_count != raw.index_partitions.len() {
        return Err(CatalogError::Mapping(format!(
            "Oracle index-partition inventory mismatch: USER/DBA_OBJECTS reports {inventory_index_partition_count}, USER/DBA_IND_PARTITIONS reports {}",
            raw.index_partitions.len()
        )));
    }

    let mut index_subpartitions_by_partition =
        BTreeMap::<(String, String, String), Vec<&RawIndexSubpartition>>::new();
    let mut index_subpartition_identities = BTreeSet::new();
    for subpartition in &raw.index_subpartitions {
        ensure_owner(scope, &subpartition.owner, "index subpartition")?;
        let parent = index_partition_identities
            .get(&(
                subpartition.owner.clone(),
                subpartition.index.clone(),
                subpartition.partition.clone(),
            ))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle index subpartition {}.{}.{} has no parent partition {}",
                    subpartition.owner,
                    subpartition.index,
                    subpartition.name,
                    subpartition.partition
                ))
            })?;
        positive_u32(subpartition.position, "Oracle index subpartition position")?;
        if subpartition.partition_position != parent.position {
            return Err(CatalogError::Mapping(format!(
                "Oracle index subpartition parent position mismatch for {}.{}.{}",
                subpartition.owner, subpartition.index, subpartition.name
            )));
        }
        if !inventory_subobject_keys.contains(&(
            subpartition.owner.clone(),
            "INDEX SUBPARTITION".to_owned(),
            subpartition.index.clone(),
            subpartition.name.clone(),
        )) {
            return Err(CatalogError::Mapping(format!(
                "Oracle index subpartition {}.{}.{} is missing from the independent object inventory",
                subpartition.owner, subpartition.index, subpartition.name
            )));
        }
        let identity = (
            subpartition.owner.clone(),
            subpartition.index.clone(),
            subpartition.name.clone(),
        );
        if !index_subpartition_identities.insert(identity.clone()) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle index subpartition {}.{}.{}",
                identity.0, identity.1, identity.2
            )));
        }
        index_subpartitions_by_partition
            .entry((
                subpartition.owner.clone(),
                subpartition.index.clone(),
                subpartition.partition.clone(),
            ))
            .or_default()
            .push(subpartition);
    }
    for (identity, parent) in &index_partition_identities {
        let subpartitions = index_subpartitions_by_partition
            .get(identity)
            .map(Vec::as_slice)
            .unwrap_or_default();
        if subpartitions.len() != parent.subpartition_count as usize {
            return Err(CatalogError::Mapping(format!(
                "Oracle index subpartition count mismatch for {}.{}.{}",
                identity.0, identity.1, identity.2
            )));
        }
        ensure_contiguous_positions(
            subpartitions
                .iter()
                .map(|subpartition| subpartition.position),
            &format!(
                "Oracle index subpartitions {}.{}.{}",
                identity.0, identity.1, identity.2
            ),
        )?;
    }
    let inventory_index_subpartition_count = inventory_subobject_keys
        .iter()
        .filter(|key| {
            key.1 == "INDEX SUBPARTITION"
                && !lob_index_names.contains(&(key.0.clone(), key.2.clone()))
        })
        .count();
    if inventory_index_subpartition_count != raw.index_subpartitions.len() {
        return Err(CatalogError::Mapping(format!(
            "Oracle index-subpartition inventory mismatch: USER/DBA_OBJECTS reports {inventory_index_subpartition_count}, USER/DBA_IND_SUBPARTITIONS reports {}",
            raw.index_subpartitions.len()
        )));
    }

    let mut keys_by_object =
        BTreeMap::<(String, String, String, bool), Vec<&RawPartitionKeyColumn>>::new();
    let mut key_identities = BTreeSet::new();
    for key_column in &raw.partition_key_columns {
        ensure_owner(scope, &key_column.owner, "partition key column")?;
        if !matches!(key_column.object_type.as_str(), "TABLE" | "INDEX") {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle partition key {}.{} has unsupported object type '{}'",
                key_column.owner, key_column.name, key_column.object_type
            )));
        }
        positive_u32(key_column.position, "Oracle partition key column position")?;
        if key_column.collated_column_id.is_some_and(|id| id <= 0) {
            return Err(CatalogError::Mapping(format!(
                "Oracle partition key {}.{}.{} has invalid collated column id",
                key_column.owner, key_column.name, key_column.column
            )));
        }
        let target_table = if key_column.object_type == "TABLE" {
            key_column.name.as_str()
        } else {
            raw_indexes
                .get(&(key_column.owner.clone(), key_column.name.clone()))
                .map(|index| index.table.as_str())
                .ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "Oracle index partition key references missing index {}.{}",
                        key_column.owner, key_column.name
                    ))
                })?
        };
        if !column_keys.contains(&(
            key_column.owner.clone(),
            target_table.to_owned(),
            key_column.column.clone(),
        )) {
            return Err(CatalogError::Mapping(format!(
                "Oracle partition key {}.{}.{} references a missing column",
                key_column.owner, key_column.name, key_column.column
            )));
        }
        let identity = (
            key_column.owner.clone(),
            key_column.name.clone(),
            key_column.object_type.clone(),
            key_column.subpartition,
            key_column.position,
        );
        if !key_identities.insert(identity) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle partition key position for {}.{}",
                key_column.owner, key_column.name
            )));
        }
        keys_by_object
            .entry((
                key_column.owner.clone(),
                key_column.name.clone(),
                key_column.object_type.clone(),
                key_column.subpartition,
            ))
            .or_default()
            .push(key_column);
    }
    for table in &raw.partitioned_tables {
        for (subpartition, expected) in [
            (false, table.partitioning_key_count),
            (true, table.subpartitioning_key_count),
        ] {
            let key = (
                table.owner.clone(),
                table.table.clone(),
                "TABLE".to_owned(),
                subpartition,
            );
            let columns = keys_by_object
                .get(&key)
                .map(Vec::as_slice)
                .unwrap_or_default();
            if columns.len() != expected as usize {
                return Err(CatalogError::Mapping(format!(
                    "Oracle table partition-key count mismatch for {}.{}",
                    table.owner, table.table
                )));
            }
            ensure_contiguous_positions(
                columns.iter().map(|column| column.position),
                &format!(
                    "Oracle table partition keys {}.{}",
                    table.owner, table.table
                ),
            )?;
        }
    }
    for index in &raw.partitioned_indexes {
        for (subpartition, expected) in [
            (false, index.partitioning_key_count),
            (true, index.subpartitioning_key_count),
        ] {
            let key = (
                index.owner.clone(),
                index.index.clone(),
                "INDEX".to_owned(),
                subpartition,
            );
            let columns = keys_by_object
                .get(&key)
                .map(Vec::as_slice)
                .unwrap_or_default();
            if columns.len() != expected as usize {
                return Err(CatalogError::Mapping(format!(
                    "Oracle index partition-key count mismatch for {}.{}",
                    index.owner, index.index
                )));
            }
            ensure_contiguous_positions(
                columns.iter().map(|column| column.position),
                &format!(
                    "Oracle index partition keys {}.{}",
                    index.owner, index.index
                ),
            )?;
        }
    }
    let expected_key_count = raw
        .partitioned_tables
        .iter()
        .map(|table| table.partitioning_key_count + table.subpartitioning_key_count)
        .chain(
            raw.partitioned_indexes
                .iter()
                .map(|index| index.partitioning_key_count + index.subpartitioning_key_count),
        )
        .sum::<i64>();
    if expected_key_count < 0 || raw.partition_key_columns.len() != expected_key_count as usize {
        return Err(CatalogError::Mapping(
            "Oracle partition-key catalogs contain unclaimed or missing rows".to_owned(),
        ));
    }

    Ok(())
}
