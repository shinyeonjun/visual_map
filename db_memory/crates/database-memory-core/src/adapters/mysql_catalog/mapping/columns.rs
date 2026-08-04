#[allow(clippy::too_many_arguments)]
fn map_columns(
    metadata: &mut CanonicalMetadata,
    source_kind: &str,
    connection_alias: &str,
    database: &str,
    lower_case_table_names: u64,
    raw_columns: &[RawColumn],
    table_keys: &BTreeMap<String, ObjectKey>,
    view_keys: &BTreeMap<String, ObjectKey>,
    sequence_keys: &BTreeMap<String, ObjectKey>,
    table_types: &BTreeMap<String, String>,
) -> Result<MappedColumns, CatalogError> {
    let mut columns = Vec::new();
    let mut column_keys = BTreeMap::new();
    for column in raw_columns {
        let relation_name = normalize_object_name(&column.table, lower_case_table_names);
        let column_name = normalize_column_name(&column.name);
        let table_type = table_types.get(&relation_name).ok_or_else(|| {
            CatalogError::Mapping(format!(
                "column '{}.{}' references a missing table-like object",
                column.table, column.name
            ))
        })?;
        match table_type.to_ascii_uppercase().as_str() {
            "BASE TABLE" => {
                let table_key = table_keys.get(&relation_name).cloned().ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "column '{}.{}' lost its table key",
                        column.table, column.name
                    ))
                })?;
                let key = family_key(
                    source_kind,
                    connection_alias,
                    database,
                    ObjectKind::Column,
                    &column.table,
                    Some(column.name.clone()),
                );
                if column_keys
                    .insert((relation_name.clone(), column_name), key.clone())
                    .is_some()
                {
                    return Err(CatalogError::Mapping(format!(
                        "duplicate column '{}.{}'",
                        column.table, column.name
                    )));
                }
                columns.push(ColumnObject {
                    key: key.clone(),
                    table_key,
                    name: column.name.clone(),
                    ordinal_position: column.ordinal,
                    data_type: column.column_type.clone(),
                    is_nullable: column.nullable,
                    default_value: column.default_value.clone(),
                    is_generated: column
                        .generation_expression
                        .as_deref()
                        .is_some_and(|value| !value.is_empty())
                        || column.extra.to_ascii_uppercase().contains("GENERATED"),
                });
                add_column_annotation(metadata, &key, column);
            }
            "VIEW" => {
                let view_key = view_keys.get(&relation_name).cloned().ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "view column '{}.{}' lost its view key",
                        column.table, column.name
                    ))
                })?;
                let key = family_key(
                    source_kind,
                    connection_alias,
                    database,
                    ObjectKind::ViewColumn,
                    &column.table,
                    Some(column.name.clone()),
                );
                let mut properties = column_properties(column);
                insert_u64(&mut properties, "ordinal_position", column.ordinal as u64);
                metadata.objects.push(MetadataObject {
                    key,
                    parent_key: Some(view_key),
                    name: column.name.clone(),
                    extension_kind: None,
                    definition: column.generation_expression.clone(),
                    properties,
                });
            }
            "SEQUENCE" => {
                let sequence_key = sequence_keys.get(&relation_name).cloned().ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "sequence column '{}.{}' lost its sequence key",
                        column.table, column.name
                    ))
                })?;
                let key = family_key(
                    source_kind,
                    connection_alias,
                    database,
                    ObjectKind::Extension,
                    &column.table,
                    Some(format!("sequence_column:{}", column.name)),
                );
                let mut properties = column_properties(column);
                insert_u64(&mut properties, "ordinal_position", column.ordinal as u64);
                metadata.objects.push(MetadataObject {
                    key,
                    parent_key: Some(sequence_key),
                    name: column.name.clone(),
                    extension_kind: Some("mariadb_sequence_column".to_owned()),
                    definition: column.generation_expression.clone(),
                    properties,
                });
            }
            unsupported => {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "column '{}.{}' belongs to unsupported TABLE_TYPE '{unsupported}'",
                    column.table, column.name
                )));
            }
        }
    }
    Ok(MappedColumns {
        objects: columns,
        keys: column_keys,
    })
}

struct MappedColumns {
    objects: Vec<ColumnObject>,
    keys: BTreeMap<(String, String), ObjectKey>,
}

fn column_properties(column: &RawColumn) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_string(&mut properties, "data_type", &column.data_type);
    insert_string(&mut properties, "column_type", &column.column_type);
    insert_optional_string(
        &mut properties,
        "character_set",
        column.character_set.as_deref(),
    );
    insert_optional_string(&mut properties, "collation", column.collation.as_deref());
    insert_string(&mut properties, "extra", &column.extra);
    insert_string(&mut properties, "privileges", &column.privileges);
    insert_string(&mut properties, "comment", &column.comment);
    if let Some(spatial_reference_id) = column.spatial_reference_id {
        insert_u64(
            &mut properties,
            "spatial_reference_id",
            spatial_reference_id,
        );
    }
    insert_bool(
        &mut properties,
        "system_period_start",
        column.system_period_start,
    );
    insert_bool(
        &mut properties,
        "system_period_end",
        column.system_period_end,
    );
    properties
}

fn add_column_annotation(
    metadata: &mut CanonicalMetadata,
    column_key: &ObjectKey,
    column: &RawColumn,
) {
    add_annotation(
        metadata,
        column_key,
        column.generation_expression.clone(),
        column_properties(column),
    );
}

fn resolve_view_dependencies(
    raw: &RawMysqlFamilyCatalog,
    table_keys: &BTreeMap<String, ObjectKey>,
    view_keys: &BTreeMap<String, ObjectKey>,
) -> Result<BTreeMap<String, Vec<ObjectKey>>, CatalogError> {
    let mut grouped = raw
        .views
        .iter()
        .map(|view| {
            (
                normalize_object_name(&view.name, raw.facts.lower_case_table_names),
                BTreeSet::<String>::new(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut keys = BTreeMap::new();
    for key in table_keys.values().chain(view_keys.values()) {
        keys.insert(key.to_string(), key.clone());
    }

    match raw.strategy.product() {
        MysqlProduct::Mysql => {
            for usage in &raw.view_table_usage {
                let view = normalize_object_name(&usage.view, raw.facts.lower_case_table_names);
                let dependencies = grouped.get_mut(&view).ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "VIEW_TABLE_USAGE references missing view '{}'",
                        usage.view
                    ))
                })?;
                if normalize_object_name(&usage.target_schema, raw.facts.lower_case_table_names)
                    != normalize_object_name(&raw.facts.database, raw.facts.lower_case_table_names)
                {
                    return Err(CatalogError::UnsupportedMetadata(format!(
                        "view '{}' depends on out-of-scope database '{}.{}'",
                        usage.view, usage.target_schema, usage.target_name
                    )));
                }
                let target =
                    normalize_object_name(&usage.target_name, raw.facts.lower_case_table_names);
                let key = table_keys
                    .get(&target)
                    .or_else(|| view_keys.get(&target))
                    .ok_or_else(|| {
                        CatalogError::Mapping(format!(
                            "view '{}' dependency '{}.{}' is absent from the selected catalog",
                            usage.view, usage.target_schema, usage.target_name
                        ))
                    })?;
                dependencies.insert(key.to_string());
            }
        }
        MysqlProduct::MariaDb => {
            for view in &raw.views {
                let definition = view.definition.as_deref().ok_or_else(|| {
                    CatalogError::PermissionDenied(format!(
                        "view '{}' definition is hidden; SHOW VIEW is not effective",
                        view.name
                    ))
                })?;
                let relations =
                    parse_mariadb_view_relations(definition, raw.facts.lower_case_table_names)?;
                let view_name = normalize_object_name(&view.name, raw.facts.lower_case_table_names);
                let dependencies = grouped.get_mut(&view_name).ok_or_else(|| {
                    CatalogError::Mapping(format!("view '{}' has no dependency ledger", view.name))
                })?;
                for (schema, relation) in relations {
                    if schema.as_deref().is_some_and(|schema| {
                        normalize_object_name(schema, raw.facts.lower_case_table_names)
                            != normalize_object_name(
                                &raw.facts.database,
                                raw.facts.lower_case_table_names,
                            )
                    }) {
                        return Err(CatalogError::UnsupportedMetadata(format!(
                            "view '{}' depends on out-of-scope relation '{}.{}'",
                            view.name,
                            schema.unwrap_or_default(),
                            relation
                        )));
                    }
                    let target = normalize_object_name(&relation, raw.facts.lower_case_table_names);
                    let key = table_keys
                        .get(&target)
                        .or_else(|| view_keys.get(&target))
                        .ok_or_else(|| {
                            CatalogError::Mapping(format!(
                                "MariaDB view '{}' AST dependency '{}' is absent from the selected catalog",
                                view.name, relation
                            ))
                        })?;
                    dependencies.insert(key.to_string());
                }
            }
        }
    }

    grouped
        .into_iter()
        .map(|(view, dependency_ids)| {
            let dependencies = dependency_ids
                .into_iter()
                .map(|id| {
                    keys.get(&id).cloned().ok_or_else(|| {
                        CatalogError::Mapping(format!(
                            "view dependency stable key '{id}' was not registered"
                        ))
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok((view, dependencies))
        })
        .collect()
}

#[derive(Default)]
struct CteAliasCollector {
    aliases: BTreeSet<String>,
    lower_case_table_names: u64,
}

impl Visitor for CteAliasCollector {
    type Break = ();

    fn pre_visit_query(&mut self, query: &Query) -> ControlFlow<Self::Break> {
        if let Some(with) = &query.with {
            for cte in &with.cte_tables {
                self.aliases.insert(normalize_object_name(
                    &cte.alias.name.value,
                    self.lower_case_table_names,
                ));
            }
        }
        ControlFlow::Continue(())
    }
}

fn parse_mariadb_view_relations(
    definition: &str,
    lower_case_table_names: u64,
) -> Result<BTreeSet<(Option<String>, String)>, CatalogError> {
    let statements = Parser::parse_sql(&MySqlDialect {}, definition).map_err(|error| {
        CatalogError::UnsupportedMetadata(format!(
            "MariaDB view definition cannot be parsed as SQL AST: {error}"
        ))
    })?;
    if statements.len() != 1 {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "MariaDB view definition parsed into {} statements instead of one",
            statements.len()
        )));
    }
    let mut ctes = CteAliasCollector {
        aliases: BTreeSet::new(),
        lower_case_table_names,
    };
    let _ = statements.visit(&mut ctes);

    let mut relations = BTreeSet::new();
    let mut failure = None;
    let _: ControlFlow<()> = visit_relations(&statements, |relation| {
        if failure.is_some() {
            return ControlFlow::Continue(());
        }
        match object_name_identifiers(relation) {
            Ok(parts) if parts.len() == 1 => {
                let name = parts[0].clone();
                let normalized = normalize_object_name(&name, lower_case_table_names);
                if !ctes.aliases.contains(&normalized) && !name.eq_ignore_ascii_case("dual") {
                    relations.insert((None, name));
                }
            }
            Ok(parts) if parts.len() == 2 => {
                relations.insert((Some(parts[0].clone()), parts[1].clone()));
            }
            Ok(parts) => {
                failure = Some(CatalogError::UnsupportedMetadata(format!(
                    "MariaDB view relation '{}' uses unsupported {}-part qualification",
                    relation,
                    parts.len()
                )));
            }
            Err(error) => failure = Some(error),
        }
        ControlFlow::Continue(())
    });
    match failure {
        Some(error) => Err(error),
        None => Ok(relations),
    }
}

fn object_name_identifiers(name: &ObjectName) -> Result<Vec<String>, CatalogError> {
    name.0
        .iter()
        .map(|part| match part {
            ObjectNamePart::Identifier(identifier) => Ok(identifier.value.clone()),
            ObjectNamePart::Function(_) => Err(CatalogError::UnsupportedMetadata(format!(
                "dynamic relation identifier '{name}' cannot be proven"
            ))),
        })
        .collect()
}
