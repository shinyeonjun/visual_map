#[allow(clippy::too_many_arguments)]
fn map_constraints(
    metadata: &mut CanonicalMetadata,
    source_kind: &str,
    connection_alias: &str,
    database: &str,
    lower_case_table_names: u64,
    raw: &RawMysqlFamilyCatalog,
    table_keys: &BTreeMap<String, ObjectKey>,
    column_keys: &BTreeMap<(String, String), ObjectKey>,
) -> Result<Vec<ConstraintObject>, CatalogError> {
    let mut key_usage = BTreeMap::<(String, String), Vec<&RawKeyUsage>>::new();
    for usage in &raw.key_usage {
        key_usage
            .entry((
                normalize_object_name(&usage.table, lower_case_table_names),
                usage.constraint.clone(),
            ))
            .or_default()
            .push(usage);
    }
    let mut checks = BTreeMap::new();
    for check in &raw.checks {
        let key = (
            normalize_object_name(&check.table, lower_case_table_names),
            check.constraint.clone(),
        );
        if checks.insert(key, check).is_some() {
            return Err(CatalogError::Mapping(format!(
                "duplicate check definition '{}.{}'",
                check.table, check.constraint
            )));
        }
    }
    let mut reference_rules = BTreeMap::new();
    for rule in &raw.reference_rules {
        let key = (
            normalize_object_name(&rule.table, lower_case_table_names),
            rule.constraint.clone(),
        );
        if reference_rules.insert(key, rule).is_some() {
            return Err(CatalogError::Mapping(format!(
                "duplicate referential rule '{}.{}'",
                rule.table, rule.constraint
            )));
        }
    }

    let mut constraints = Vec::new();
    let mut seen = BTreeSet::new();
    for raw_constraint in &raw.constraints {
        let table_name = normalize_object_name(&raw_constraint.table, lower_case_table_names);
        let identity = (table_name.clone(), raw_constraint.name.clone());
        if !seen.insert(identity.clone()) {
            return Err(CatalogError::Mapping(format!(
                "duplicate constraint '{}.{}'",
                raw_constraint.table, raw_constraint.name
            )));
        }
        let table_key = table_keys.get(&table_name).cloned().ok_or_else(|| {
            CatalogError::Mapping(format!(
                "constraint '{}.{}' targets a non-base or missing table",
                raw_constraint.table, raw_constraint.name
            ))
        })?;
        let (kind, object_kind) = match raw_constraint.constraint_type.as_str() {
            "PRIMARY KEY" => (ConstraintKind::PrimaryKey, ObjectKind::PrimaryKey),
            "FOREIGN KEY" => (ConstraintKind::ForeignKey, ObjectKind::ForeignKey),
            "UNIQUE" => (ConstraintKind::Unique, ObjectKind::UniqueConstraint),
            "CHECK" => (ConstraintKind::Check, ObjectKind::CheckConstraint),
            unsupported => {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "constraint '{}.{}' has unsupported type '{unsupported}'",
                    raw_constraint.table, raw_constraint.name
                )));
            }
        };
        let key = family_key(
            source_kind,
            connection_alias,
            database,
            object_kind,
            &raw_constraint.table,
            Some(raw_constraint.name.clone()),
        );
        let mut source_columns = Vec::new();
        let mut referenced_columns = Vec::new();
        let mut referenced_table_key = None;
        let mut uses = key_usage.remove(&identity).unwrap_or_default();
        uses.sort_by_key(|usage| usage.ordinal);
        if kind != ConstraintKind::Check {
            require_contiguous_ordinals(
                uses.iter().map(|usage| usage.ordinal),
                &format!(
                    "constraint '{}.{}'",
                    raw_constraint.table, raw_constraint.name
                ),
            )?;
            if uses.is_empty() {
                return Err(CatalogError::Mapping(format!(
                    "constraint '{}.{}' has no KEY_COLUMN_USAGE rows",
                    raw_constraint.table, raw_constraint.name
                )));
            }
        } else if !uses.is_empty() {
            return Err(CatalogError::Mapping(format!(
                "check constraint '{}.{}' unexpectedly has KEY_COLUMN_USAGE rows",
                raw_constraint.table, raw_constraint.name
            )));
        }
        for usage in uses {
            let source = column_keys
                .get(&(table_name.clone(), normalize_column_name(&usage.column)))
                .cloned()
                .ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "constraint '{}.{}' references missing source column '{}'",
                        raw_constraint.table, raw_constraint.name, usage.column
                    ))
                })?;
            source_columns.push(source);
            if kind == ConstraintKind::ForeignKey {
                let referenced_schema = usage.referenced_schema.as_deref().ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "foreign key '{}.{}' lacks referenced schema",
                        raw_constraint.table, raw_constraint.name
                    ))
                })?;
                if normalize_object_name(referenced_schema, lower_case_table_names)
                    != normalize_object_name(database, lower_case_table_names)
                {
                    return Err(CatalogError::UnsupportedMetadata(format!(
                        "foreign key '{}.{}' references out-of-scope database '{}'",
                        raw_constraint.table, raw_constraint.name, referenced_schema
                    )));
                }
                let referenced_table = usage.referenced_table.as_deref().ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "foreign key '{}.{}' lacks referenced table",
                        raw_constraint.table, raw_constraint.name
                    ))
                })?;
                let referenced_name =
                    normalize_object_name(referenced_table, lower_case_table_names);
                let candidate = table_keys.get(&referenced_name).cloned().ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "foreign key '{}.{}' references missing table '{}'",
                        raw_constraint.table, raw_constraint.name, referenced_table
                    ))
                })?;
                if referenced_table_key
                    .as_ref()
                    .is_some_and(|existing| existing != &candidate)
                {
                    return Err(CatalogError::Mapping(format!(
                        "foreign key '{}.{}' references multiple target tables",
                        raw_constraint.table, raw_constraint.name
                    )));
                }
                referenced_table_key = Some(candidate);
                let referenced_column = usage.referenced_column.as_deref().ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "foreign key '{}.{}' lacks referenced column",
                        raw_constraint.table, raw_constraint.name
                    ))
                })?;
                referenced_columns.push(
                    column_keys
                        .get(&(referenced_name, normalize_column_name(referenced_column)))
                        .cloned()
                        .ok_or_else(|| {
                            CatalogError::Mapping(format!(
                                "foreign key '{}.{}' references missing column '{}.{}'",
                                raw_constraint.table,
                                raw_constraint.name,
                                referenced_table,
                                referenced_column
                            ))
                        })?,
                );
            } else if usage.referenced_table.is_some()
                || usage.referenced_column.is_some()
                || usage.referenced_schema.is_some()
            {
                return Err(CatalogError::Mapping(format!(
                    "non-foreign constraint '{}.{}' has referenced target metadata",
                    raw_constraint.table, raw_constraint.name
                )));
            }
        }

        let expression = if kind == ConstraintKind::Check {
            Some(
                checks
                    .remove(&identity)
                    .ok_or_else(|| {
                        CatalogError::Mapping(format!(
                            "check constraint '{}.{}' has no CHECK_CONSTRAINTS row",
                            raw_constraint.table, raw_constraint.name
                        ))
                    })?
                    .clause
                    .clone(),
            )
        } else {
            None
        };
        let mut properties = BTreeMap::new();
        insert_bool(&mut properties, "enforced", raw_constraint.enforced);
        if kind == ConstraintKind::ForeignKey {
            let rule = reference_rules.remove(&identity).ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "foreign key '{}.{}' has no REFERENTIAL_CONSTRAINTS row",
                    raw_constraint.table, raw_constraint.name
                ))
            })?;
            insert_string(&mut properties, "match_option", &rule.match_option);
            insert_string(&mut properties, "update_rule", &rule.update_rule);
            insert_string(&mut properties, "delete_rule", &rule.delete_rule);
        }
        add_annotation(metadata, &key, None, properties);
        constraints.push(ConstraintObject {
            key,
            table_key,
            name: raw_constraint.name.clone(),
            kind,
            columns: source_columns,
            referenced_table_key,
            referenced_columns,
            expression,
        });
    }
    if let Some(((table, name), _)) = key_usage.into_iter().next() {
        return Err(CatalogError::Mapping(format!(
            "KEY_COLUMN_USAGE row '{table}.{name}' has no TABLE_CONSTRAINTS owner"
        )));
    }
    if let Some(((table, name), _)) = checks.into_iter().next() {
        return Err(CatalogError::Mapping(format!(
            "CHECK_CONSTRAINTS row '{table}.{name}' has no TABLE_CONSTRAINTS owner"
        )));
    }
    if let Some(((table, name), _)) = reference_rules.into_iter().next() {
        return Err(CatalogError::Mapping(format!(
            "REFERENTIAL_CONSTRAINTS row '{table}.{name}' has no foreign key owner"
        )));
    }
    Ok(constraints)
}
