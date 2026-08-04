fn validate_raw_catalog(raw: &RawPostgresCatalog) -> Result<(), CatalogError> {
    let strategy = raw.strategy;
    if raw.server.major() != strategy.catalog_version().major() {
        return Err(CatalogError::Mapping(format!(
            "{} server major {} does not match selected catalog strategy {}",
            strategy.product_name(),
            raw.server.major(),
            strategy.strategy_name()
        )));
    }
    match (strategy, &raw.yugabyte) {
        (PgCatalogStrategy::YugabyteDb2025_2_3_2, Some(yugabyte)) => {
            validate_yugabyte_catalog(raw, yugabyte)?;
        }
        (PgCatalogStrategy::YugabyteDb2025_2_3_2, None) => {
            return Err(CatalogError::UnsupportedMetadata(
                "certified YugabyteDB strategy did not collect YugabyteDB catalog metadata"
                    .to_owned(),
            ));
        }
        (PgCatalogStrategy::PostgreSql(_), Some(_)) => {
            return Err(CatalogError::Mapping(
                "PostgreSQL strategy unexpectedly contains YugabyteDB catalog metadata".to_owned(),
            ));
        }
        (PgCatalogStrategy::PostgreSql(_), None) => {}
    }
    if !raw.server.transaction_read_only || raw.server.transaction_isolation != "repeatable read" {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "{} metadata transaction is not read-only repeatable-read (read_only={}, isolation={})",
            strategy.product_name(),
            raw.server.transaction_read_only,
            raw.server.transaction_isolation
        )));
    }
    for relation in &raw.relations {
        if relation.definition_too_large {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "definition for {}.{} exceeds {MAX_DEFINITION_BYTES} bytes",
                relation.schema, relation.name
            )));
        }
        validate_property_text(
            &format!(
                "relation {}.{} partition bound",
                relation.schema, relation.name
            ),
            relation.partition_bound.as_deref(),
        )?;
        validate_property_text(
            &format!("relation {}.{} comment", relation.schema, relation.name),
            relation.comment.as_deref(),
        )?;
    }
    for schema in &raw.schemas {
        validate_property_text(
            &format!("schema {} comment", schema.name),
            schema.comment.as_deref(),
        )?;
    }
    for column in &raw.columns {
        if column.default_too_large {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "default/generated expression for {}.{}.{} exceeds {MAX_DEFINITION_BYTES} bytes",
                column.schema, column.relation, column.name
            )));
        }
        validate_property_text(
            &format!(
                "column {}.{}.{} comment",
                column.schema, column.relation, column.name
            ),
            column.comment.as_deref(),
        )?;
    }
    for constraint in &raw.constraints {
        if constraint.definition_too_large {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "constraint definition {} exceeds {MAX_DEFINITION_BYTES} bytes",
                constraint.name
            )));
        }
    }
    for index in &raw.indexes {
        if index.definition_too_large {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "index definition {}.{}.{} exceeds {MAX_DEFINITION_BYTES} bytes",
                index.schema, index.relation, index.name
            )));
        }
    }
    for term in &raw.index_terms {
        validate_property_text("index term definition", Some(&term.definition))?;
    }
    for raw_type in &raw.types {
        if raw_type.default_too_large {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "type default {}.{} exceeds {MAX_PROPERTY_STRING_BYTES} bytes",
                raw_type.schema, raw_type.name
            )));
        }
        validate_property_text(
            &format!("type {}.{} comment", raw_type.schema, raw_type.name),
            raw_type.comment.as_deref(),
        )?;
    }
    for routine in &raw.routines {
        if routine.definition_too_large {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "routine definition {}.{}({}) exceeds {MAX_DEFINITION_BYTES} bytes",
                routine.schema, routine.name, routine.identity_arguments
            )));
        }
        validate_property_text(
            &format!("routine {} arguments", routine.name),
            Some(&routine.arguments_definition),
        )?;
    }
    for trigger in &raw.triggers {
        if trigger.definition_too_large {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "trigger definition {} exceeds {MAX_DEFINITION_BYTES} bytes",
                trigger.name
            )));
        }
        validate_property_text(
            &format!("trigger {} WHEN expression", trigger.name),
            trigger.when_expression.as_deref(),
        )?;
    }
    for policy in &raw.policies {
        validate_property_text(
            &format!("policy {} USING expression", policy.name),
            policy.using_expression.as_deref(),
        )?;
        validate_property_text(
            &format!("policy {} WITH CHECK expression", policy.name),
            policy.check_expression.as_deref(),
        )?;
    }
    Ok(())
}

fn validate_yugabyte_catalog(
    raw: &RawPostgresCatalog,
    yugabyte: &RawYugabyteCatalog,
) -> Result<(), CatalogError> {
    if yugabyte.database_default_tablespace_oid <= 0 {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "YugabyteDB database default tablespace oid must be positive, got {}",
            yugabyte.database_default_tablespace_oid
        )));
    }

    let mut tablespace_oids = BTreeSet::new();
    let mut tablespace_names = BTreeSet::new();
    for tablespace in &yugabyte.tablespaces {
        if tablespace.oid <= 0
            || tablespace.name.trim().is_empty()
            || !tablespace_oids.insert(tablespace.oid)
            || !tablespace_names.insert(tablespace.name.clone())
        {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "invalid or duplicate YugabyteDB tablespace oid={} name='{}'",
                tablespace.oid, tablespace.name
            )));
        }
        validate_property_text(
            &format!("YugabyteDB tablespace {} comment", tablespace.name),
            tablespace.comment.as_deref(),
        )?;
        validate_string_list(
            &format!("YugabyteDB tablespace {} ACL", tablespace.name),
            &tablespace.acl,
        )?;
        validate_string_list(
            &format!(
                "YugabyteDB tablespace {} placement options",
                tablespace.name
            ),
            &tablespace.options,
        )?;
    }
    if !tablespace_oids.contains(&yugabyte.database_default_tablespace_oid) {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "YugabyteDB default tablespace oid {} is absent from pg_tablespace",
            yugabyte.database_default_tablespace_oid
        )));
    }

    let mut tablegroup_oids = BTreeSet::new();
    let mut tablegroup_names = BTreeSet::new();
    for tablegroup in &yugabyte.tablegroups {
        if tablegroup.oid <= 0
            || tablegroup.name.trim().is_empty()
            || !tablegroup_oids.insert(tablegroup.oid)
            || !tablegroup_names.insert(tablegroup.name.clone())
        {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "invalid or duplicate YugabyteDB tablegroup oid={} name='{}'",
                tablegroup.oid, tablegroup.name
            )));
        }
        let effective_tablespace = effective_yugabyte_tablespace_oid(
            tablegroup.tablespace_oid,
            yugabyte.database_default_tablespace_oid,
        );
        if !tablespace_oids.contains(&effective_tablespace) {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "YugabyteDB tablegroup {} references missing tablespace oid {}",
                tablegroup.name, effective_tablespace
            )));
        }
        validate_string_list(
            &format!("YugabyteDB tablegroup {} ACL", tablegroup.name),
            &tablegroup.acl,
        )?;
        validate_string_list(
            &format!("YugabyteDB tablegroup {} options", tablegroup.name),
            &tablegroup.options,
        )?;
    }

    let mut expected_relations = BTreeMap::new();
    for relation in &raw.relations {
        if matches!(relation.relkind, 'r' | 'p' | 'f' | 'm' | 'S')
            && expected_relations
                .insert(relation.oid, Some(relation.relkind))
                .is_some()
        {
            return Err(CatalogError::Mapping(format!(
                "duplicate YugabyteDB relation oid {}",
                relation.oid
            )));
        }
    }
    for index in &raw.indexes {
        if expected_relations.insert(index.oid, None).is_some() {
            return Err(CatalogError::Mapping(format!(
                "duplicate YugabyteDB index oid {}",
                index.oid
            )));
        }
    }

    let mut discovered_relations = BTreeSet::new();
    for relation in &yugabyte.relation_properties {
        let Some(expected_kind) = expected_relations.get(&relation.relation_oid) else {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "yb_table_properties returned out-of-scope relation oid {}",
                relation.relation_oid
            )));
        };
        if !discovered_relations.insert(relation.relation_oid) {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "yb_table_properties returned duplicate relation oid {}",
                relation.relation_oid
            )));
        }
        match expected_kind {
            Some(kind) if *kind != relation.relation_kind => {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "YugabyteDB relation oid {} changed kind from '{}' to '{}' during discovery",
                    relation.relation_oid, kind, relation.relation_kind
                )));
            }
            None if !matches!(relation.relation_kind, 'i' | 'I') => {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "YugabyteDB index oid {} reports relation kind '{}'",
                    relation.relation_oid, relation.relation_kind
                )));
            }
            _ => {}
        }
        if relation.tablespace_oid < 0 {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "YugabyteDB relation oid {} has negative tablespace oid {}",
                relation.relation_oid, relation.tablespace_oid
            )));
        }
        validate_property_text(
            &format!(
                "YugabyteDB relation oid {} range split clause",
                relation.relation_oid
            ),
            relation.range_split_clause.as_deref(),
        )?;

        match (
            relation.num_tablets,
            relation.num_hash_key_columns,
            relation.is_colocated,
        ) {
            (Some(num_tablets), Some(num_hash_columns), Some(_))
                if num_tablets > 0 && num_hash_columns >= 0 => {}
            (None, None, None)
                if relation.tablegroup_oid.is_none()
                    && relation.colocation_id.is_none()
                    && relation.range_split_clause.is_none() =>
            {
                continue;
            }
            values => {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "incoherent yb_table_properties for relation oid {}: {:?}",
                    relation.relation_oid, values
                )));
            }
        }
        if relation.range_split_clause.is_some() && relation.num_hash_key_columns != Some(0) {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "YugabyteDB relation oid {} has a range split clause with hash key columns",
                relation.relation_oid
            )));
        }
        match relation.is_colocated {
            Some(true) if relation.tablegroup_oid.is_some() && relation.colocation_id.is_some() => {
            }
            Some(false)
                if relation.tablegroup_oid.is_none() && relation.colocation_id.is_none() => {}
            Some(is_colocated) => {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "YugabyteDB relation oid {} has inconsistent colocation fields (is_colocated={is_colocated})",
                    relation.relation_oid
                )));
            }
            None => unreachable!("the non-storage-backed case continued above"),
        }
        if let Some(tablegroup_oid) = relation.tablegroup_oid {
            if !tablegroup_oids.contains(&tablegroup_oid) {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "YugabyteDB relation oid {} references missing tablegroup oid {}",
                    relation.relation_oid, tablegroup_oid
                )));
            }
        }
        let effective_tablespace = effective_yugabyte_tablespace_oid(
            relation.tablespace_oid,
            yugabyte.database_default_tablespace_oid,
        );
        if !tablespace_oids.contains(&effective_tablespace) {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "YugabyteDB relation oid {} references missing tablespace oid {}",
                relation.relation_oid, effective_tablespace
            )));
        }
    }
    let expected_oids = expected_relations.keys().copied().collect::<BTreeSet<_>>();
    if expected_oids != discovered_relations {
        let missing = expected_oids
            .difference(&discovered_relations)
            .copied()
            .collect::<Vec<_>>();
        return Err(CatalogError::UnsupportedMetadata(format!(
            "YugabyteDB physical metadata is incomplete; missing relation oids {missing:?}"
        )));
    }

    Ok(())
}

fn validate_string_list(subject: &str, values: &[String]) -> Result<(), CatalogError> {
    for value in values {
        validate_property_text(subject, Some(value))?;
    }
    Ok(())
}

fn validate_property_text(subject: &str, value: Option<&str>) -> Result<(), CatalogError> {
    if value
        .map(|value| value.len() > MAX_PROPERTY_STRING_BYTES as usize)
        .unwrap_or(false)
    {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "{subject} exceeds {MAX_PROPERTY_STRING_BYTES} bytes"
        )));
    }
    Ok(())
}
