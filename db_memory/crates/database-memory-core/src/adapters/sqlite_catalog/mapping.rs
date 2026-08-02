fn table_annotation(table_key: ObjectKey, relation: &RawRelation) -> ObjectAnnotation {
    let mut properties = BTreeMap::new();
    properties.insert("strict".to_owned(), MetadataValue::Boolean(relation.strict));
    properties.insert(
        "without_rowid".to_owned(),
        MetadataValue::Boolean(relation.without_rowid),
    );
    properties.insert(
        "sqlite_relation_type".to_owned(),
        MetadataValue::String(
            match relation.kind {
                RawRelationKind::Table(TableKind::Virtual) => "virtual",
                RawRelationKind::Table(TableKind::Shadow) => "shadow",
                _ => "table",
            }
            .to_owned(),
        ),
    );
    ObjectAnnotation {
        object_key: table_key,
        definition: relation.sql.clone(),
        properties,
    }
}

fn lookup_key(
    keys: &BTreeMap<String, ObjectKey>,
    name: &str,
    object_type: &str,
) -> Result<ObjectKey, SqliteAdapterError> {
    keys.get(&fold_identifier(name)).cloned().ok_or_else(|| {
        SqliteAdapterError::mapping(
            format!("{object_type} {name}"),
            "catalog relationship points outside the selected metadata scope",
        )
    })
}

struct MappedConstraints {
    constraints: Vec<ConstraintObject>,
    check_dependency_count: u64,
}

impl SqliteSnapshotMapper<'_> {
    #[allow(clippy::too_many_arguments)]
    fn map_constraints(
        &self,
        relation: &RawRelation,
        raw_foreign_keys: &[RawForeignKey],
        raw_indexes: &[RawIndex],
        table_keys: &BTreeMap<String, ObjectKey>,
        column_keys: &BTreeMap<(String, String), ObjectKey>,
        primary_key_columns: &BTreeMap<String, Vec<ObjectKey>>,
        metadata: &mut CanonicalMetadata,
    ) -> Result<MappedConstraints, SqliteAdapterError> {
        let table_key = lookup_key(table_keys, &relation.name, "table")?;
        let parsed_constraints = relation
            .parsed_table
            .as_ref()
            .map(|table| table.constraints.as_slice())
            .unwrap_or_default();
        let parsed_primary = parsed_constraints
            .iter()
            .filter(|constraint| constraint.kind == ParsedConstraintKind::PrimaryKey)
            .collect::<Vec<_>>();
        if parsed_primary.len() > 1 {
            return Err(SqliteAdapterError::mapping(
                format!("table {}", relation.name),
                "CREATE TABLE contains more than one PRIMARY KEY",
            ));
        }
        let raw_primary = primary_key_columns
            .get(&fold_identifier(&relation.name))
            .cloned()
            .unwrap_or_default();
        if relation.parsed_table.is_some() {
            match (parsed_primary.first(), raw_primary.is_empty()) {
                (Some(parsed), false) => {
                    let parsed_columns =
                        resolve_named_columns(&relation.name, &parsed.columns, column_keys)?;
                    if !same_key_sequence(&parsed_columns, &raw_primary) {
                        return Err(SqliteAdapterError::mapping(
                            format!("primary key on {}", relation.name),
                            "CREATE TABLE columns disagree with table_xinfo primary-key ordinals",
                        ));
                    }
                }
                (Some(_), true) | (None, false) => {
                    return Err(SqliteAdapterError::mapping(
                        format!("primary key on {}", relation.name),
                        "CREATE TABLE and table_xinfo disagree about primary-key presence",
                    ));
                }
                (None, true) => {}
            }
        }

        let mut names = ConstraintNameAllocator::default();
        let mut constraints = Vec::new();
        if !raw_primary.is_empty() {
            let parsed = parsed_primary.first().copied();
            let (name, declared_name) = names.allocate(
                parsed.and_then(|constraint| constraint.name.as_deref()),
                &format!("pk_{}", relation.name),
            );
            let mut properties = parsed
                .map(|constraint| constraint.properties.clone())
                .unwrap_or_default();
            preserve_declared_name(&mut properties, declared_name);
            let constraint = ConstraintObject {
                key: self.key(ObjectKind::PrimaryKey, &relation.name, Some(name.clone())),
                table_key: table_key.clone(),
                name,
                kind: ConstraintKind::PrimaryKey,
                columns: raw_primary,
                referenced_table_key: None,
                referenced_columns: vec![],
                expression: None,
            };
            push_annotation_if_needed(metadata, &constraint.key, None, properties);
            constraints.push(constraint);
        }

        let table_indexes = raw_indexes
            .iter()
            .filter(|index| same_identifier(&index.table_name, &relation.name))
            .collect::<Vec<_>>();
        let raw_unique_indexes = table_indexes
            .iter()
            .copied()
            .filter(|index| index.origin == "u")
            .collect::<Vec<_>>();
        let parsed_unique = parsed_constraints
            .iter()
            .filter(|constraint| constraint.kind == ParsedConstraintKind::Unique)
            .collect::<Vec<_>>();
        if relation.parsed_table.is_some() && parsed_unique.len() != raw_unique_indexes.len() {
            return Err(SqliteAdapterError::mapping(
                format!("unique constraints on {}", relation.name),
                format!(
                    "CREATE TABLE has {} UNIQUE constraint(s), but index_list reports {}",
                    parsed_unique.len(),
                    raw_unique_indexes.len()
                ),
            ));
        }
        let mut matched_unique = BTreeSet::new();
        for (ordinal, raw_index) in raw_unique_indexes.iter().enumerate() {
            let raw_columns = direct_index_column_names(raw_index)?;
            let parsed_match = parsed_unique
                .iter()
                .enumerate()
                .find(|(index, constraint)| {
                    !matched_unique.contains(index)
                        && same_identifier_sequence(&constraint.columns, &raw_columns)
                });
            let parsed = match (relation.parsed_table.is_some(), parsed_match) {
                (true, Some((index, parsed))) => {
                    matched_unique.insert(index);
                    Some(*parsed)
                }
                (true, None) => {
                    return Err(SqliteAdapterError::mapping(
                        format!("unique index {}", raw_index.name),
                        "index_xinfo columns do not match any parsed UNIQUE constraint",
                    ));
                }
                (false, _) => None,
            };
            let columns = resolve_named_columns(&relation.name, &raw_columns, column_keys)?;
            let (name, declared_name) = names.allocate(
                parsed.and_then(|constraint| constraint.name.as_deref()),
                &format!("uq_{}_{}", relation.name, ordinal + 1),
            );
            let mut properties = parsed
                .map(|constraint| constraint.properties.clone())
                .unwrap_or_default();
            properties.insert(
                "backing_index".to_owned(),
                MetadataValue::String(raw_index.name.clone()),
            );
            preserve_declared_name(&mut properties, declared_name);
            let constraint = ConstraintObject {
                key: self.key(
                    ObjectKind::UniqueConstraint,
                    &relation.name,
                    Some(name.clone()),
                ),
                table_key: table_key.clone(),
                name,
                kind: ConstraintKind::Unique,
                columns,
                referenced_table_key: None,
                referenced_columns: vec![],
                expression: None,
            };
            push_annotation_if_needed(metadata, &constraint.key, None, properties);
            constraints.push(constraint);
        }

        let mut check_dependency_count = 0_u64;
        for (ordinal, parsed) in parsed_constraints
            .iter()
            .filter(|constraint| constraint.kind == ParsedConstraintKind::Check)
            .enumerate()
        {
            let expression = parsed.expression.as_deref().ok_or_else(|| {
                SqliteAdapterError::mapping(
                    format!("check constraint on {}", relation.name),
                    "parsed CHECK constraint has no expression",
                )
            })?;
            let dependencies = self.expression_dependencies(
                &relation.name,
                &[expression],
                table_keys,
                &BTreeMap::new(),
                column_keys,
            )?;
            let columns = dependencies
                .into_iter()
                .filter(|key| {
                    key.object_kind == ObjectKind::Column
                        && same_identifier(&key.object_name, &relation.name)
                })
                .collect::<Vec<_>>();
            check_dependency_count += columns.len() as u64;
            let (name, declared_name) = names.allocate(
                parsed.name.as_deref(),
                &format!("ck_{}_{}", relation.name, ordinal + 1),
            );
            let mut properties = parsed.properties.clone();
            preserve_declared_name(&mut properties, declared_name);
            let constraint = ConstraintObject {
                key: self.key(
                    ObjectKind::CheckConstraint,
                    &relation.name,
                    Some(name.clone()),
                ),
                table_key: table_key.clone(),
                name,
                kind: ConstraintKind::Check,
                columns,
                referenced_table_key: None,
                referenced_columns: vec![],
                expression: Some(expression.to_owned()),
            };
            push_annotation_if_needed(metadata, &constraint.key, None, properties);
            constraints.push(constraint);
        }

        let parsed_foreign_keys = parsed_constraints
            .iter()
            .filter(|constraint| constraint.kind == ParsedConstraintKind::ForeignKey)
            .collect::<Vec<_>>();
        if relation.parsed_table.is_some() && parsed_foreign_keys.len() != raw_foreign_keys.len() {
            return Err(SqliteAdapterError::mapping(
                format!("foreign keys on {}", relation.name),
                format!(
                    "CREATE TABLE has {} FOREIGN KEY constraint(s), but foreign_key_list reports {}",
                    parsed_foreign_keys.len(),
                    raw_foreign_keys.len()
                ),
            ));
        }
        let mut matched_foreign_keys = BTreeSet::new();
        for raw_foreign_key in raw_foreign_keys {
            let mapped = resolve_raw_foreign_key(
                &relation.name,
                raw_foreign_key,
                table_keys,
                column_keys,
                primary_key_columns,
            )?;
            let parsed_match = parsed_foreign_keys
                .iter()
                .enumerate()
                .find(|(index, parsed)| {
                    !matched_foreign_keys.contains(index)
                        && parsed_foreign_key_matches(parsed, &mapped, raw_foreign_key)
                });
            let parsed = match (relation.parsed_table.is_some(), parsed_match) {
                (true, Some((index, parsed))) => {
                    matched_foreign_keys.insert(index);
                    Some(*parsed)
                }
                (true, None) => {
                    return Err(SqliteAdapterError::mapping(
                        format!("foreign key {}.{}", relation.name, raw_foreign_key.id),
                        "foreign_key_list does not match any parsed FOREIGN KEY constraint",
                    ));
                }
                (false, _) => None,
            };
            let (name, declared_name) = names.allocate(
                parsed.and_then(|constraint| constraint.name.as_deref()),
                &format!("fk_{}_{}", relation.name, raw_foreign_key.id),
            );
            let mut properties = parsed
                .map(|constraint| constraint.properties.clone())
                .unwrap_or_default();
            properties.insert(
                "on_update".to_owned(),
                MetadataValue::String(raw_foreign_key.on_update.clone()),
            );
            properties.insert(
                "on_delete".to_owned(),
                MetadataValue::String(raw_foreign_key.on_delete.clone()),
            );
            properties.insert(
                "match".to_owned(),
                MetadataValue::String(raw_foreign_key.match_name.clone()),
            );
            preserve_declared_name(&mut properties, declared_name);
            let constraint = ConstraintObject {
                key: self.key(ObjectKind::ForeignKey, &relation.name, Some(name.clone())),
                table_key: table_key.clone(),
                name,
                kind: ConstraintKind::ForeignKey,
                columns: mapped.source_columns,
                referenced_table_key: Some(mapped.referenced_table),
                referenced_columns: mapped.referenced_columns,
                expression: None,
            };
            push_annotation_if_needed(metadata, &constraint.key, None, properties);
            constraints.push(constraint);
        }

        Ok(MappedConstraints {
            constraints,
            check_dependency_count,
        })
    }
}

#[derive(Default)]
struct ConstraintNameAllocator {
    uses: BTreeMap<String, u32>,
}

impl ConstraintNameAllocator {
    fn allocate(&mut self, declared: Option<&str>, fallback: &str) -> (String, Option<String>) {
        let base = declared.unwrap_or(fallback).to_owned();
        let count = self.uses.entry(fold_identifier(&base)).or_default();
        *count += 1;
        if *count == 1 {
            (base, None)
        } else {
            (format!("{base}#{}", *count), declared.map(str::to_owned))
        }
    }
}

fn preserve_declared_name(
    properties: &mut BTreeMap<String, MetadataValue>,
    declared_name: Option<String>,
) {
    if let Some(declared_name) = declared_name {
        properties.insert(
            "declared_name".to_owned(),
            MetadataValue::String(declared_name),
        );
    }
}

fn push_annotation_if_needed(
    metadata: &mut CanonicalMetadata,
    object_key: &ObjectKey,
    definition: Option<String>,
    properties: BTreeMap<String, MetadataValue>,
) {
    if definition.is_some() || !properties.is_empty() {
        metadata.annotations.push(ObjectAnnotation {
            object_key: object_key.clone(),
            definition,
            properties,
        });
    }
}

fn resolve_named_columns(
    relation_name: &str,
    column_names: &[String],
    column_keys: &BTreeMap<(String, String), ObjectKey>,
) -> Result<Vec<ObjectKey>, SqliteAdapterError> {
    column_names
        .iter()
        .map(|column_name| {
            column_keys
                .get(&(fold_identifier(relation_name), fold_identifier(column_name)))
                .cloned()
                .ok_or_else(|| {
                    SqliteAdapterError::mapping(
                        format!("column {relation_name}.{column_name}"),
                        "schema relationship references a column absent from table_xinfo",
                    )
                })
        })
        .collect()
}

fn same_key_sequence(left: &[ObjectKey], right: &[ObjectKey]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.sub_object
                .as_deref()
                .zip(right.sub_object.as_deref())
                .is_some_and(|(left, right)| same_identifier(left, right))
        })
}

fn same_identifier_sequence(left: &[String], right: &[String]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| same_identifier(left, right))
}

fn direct_index_column_names(index: &RawIndex) -> Result<Vec<String>, SqliteAdapterError> {
    index
        .terms
        .iter()
        .filter(|term| term.key)
        .map(|term| {
            if term.cid < 0 {
                return Err(SqliteAdapterError::mapping(
                    format!("implicit index {}", index.name),
                    "UNIQUE/PRIMARY KEY backing index contains a non-column key term",
                ));
            }
            term.column_name.clone().ok_or_else(|| {
                SqliteAdapterError::mapping(
                    format!("index {}", index.name),
                    "index_xinfo omitted a direct column name",
                )
            })
        })
        .collect()
}

struct ResolvedForeignKey {
    source_columns: Vec<ObjectKey>,
    referenced_table: ObjectKey,
    referenced_columns: Vec<ObjectKey>,
}

fn resolve_raw_foreign_key(
    source_table: &str,
    raw: &RawForeignKey,
    table_keys: &BTreeMap<String, ObjectKey>,
    column_keys: &BTreeMap<(String, String), ObjectKey>,
    primary_key_columns: &BTreeMap<String, Vec<ObjectKey>>,
) -> Result<ResolvedForeignKey, SqliteAdapterError> {
    let referenced_table_name = raw
        .parts
        .first()
        .map(|part| part.referenced_table.as_str())
        .ok_or_else(|| {
            SqliteAdapterError::mapping(
                format!("foreign key {source_table}.{}", raw.id),
                "foreign key has no column parts",
            )
        })?;
    if raw
        .parts
        .iter()
        .any(|part| !same_identifier(&part.referenced_table, referenced_table_name))
    {
        return Err(SqliteAdapterError::mapping(
            format!("foreign key {source_table}.{}", raw.id),
            "foreign_key_list returned multiple target tables for one key",
        ));
    }
    let source_names = raw
        .parts
        .iter()
        .map(|part| part.source_column.clone())
        .collect::<Vec<_>>();
    let source_columns = resolve_named_columns(source_table, &source_names, column_keys)?;
    let referenced_table = lookup_key(table_keys, referenced_table_name, "referenced table")?;
    let referenced_columns = if raw
        .parts
        .iter()
        .all(|part| part.referenced_column.is_none())
    {
        primary_key_columns
            .get(&fold_identifier(referenced_table_name))
            .cloned()
            .filter(|columns| columns.len() == raw.parts.len())
            .ok_or_else(|| {
                SqliteAdapterError::mapping(
                    format!("foreign key {source_table}.{}", raw.id),
                    "implicit referenced columns do not resolve to a target primary key of equal cardinality",
                )
            })?
    } else {
        if raw
            .parts
            .iter()
            .any(|part| part.referenced_column.is_none())
        {
            return Err(SqliteAdapterError::mapping(
                format!("foreign key {source_table}.{}", raw.id),
                "foreign_key_list mixed explicit and implicit referenced columns",
            ));
        }
        resolve_named_columns(
            referenced_table_name,
            &raw.parts
                .iter()
                .filter_map(|part| part.referenced_column.clone())
                .collect::<Vec<_>>(),
            column_keys,
        )?
    };
    Ok(ResolvedForeignKey {
        source_columns,
        referenced_table,
        referenced_columns,
    })
}

fn parsed_foreign_key_matches(
    parsed: &ParsedConstraint,
    mapped: &ResolvedForeignKey,
    raw: &RawForeignKey,
) -> bool {
    let source_names = mapped
        .source_columns
        .iter()
        .filter_map(|key| key.sub_object.clone())
        .collect::<Vec<_>>();
    let referenced_names = mapped
        .referenced_columns
        .iter()
        .filter_map(|key| key.sub_object.clone())
        .collect::<Vec<_>>();
    let parsed_referenced_names = if parsed.referenced_columns.is_empty() {
        referenced_names.clone()
    } else {
        parsed.referenced_columns.clone()
    };
    let table_matches = parsed
        .referenced_table
        .as_deref()
        .is_some_and(|name| same_identifier(name, &mapped.referenced_table.object_name));
    let actions_match = [
        ("on_update", raw.on_update.as_str(), "no_action"),
        ("on_delete", raw.on_delete.as_str(), "no_action"),
        ("match", raw.match_name.as_str(), "none"),
    ]
    .into_iter()
    .all(|(property, actual, default)| {
        parsed
            .properties
            .get(property)
            .and_then(metadata_string)
            .unwrap_or(default)
            == actual
    });
    table_matches
        && same_identifier_sequence(&parsed.columns, &source_names)
        && same_identifier_sequence(&parsed_referenced_names, &referenced_names)
        && actions_match
}

fn metadata_string(value: &MetadataValue) -> Option<&str> {
    match value {
        MetadataValue::String(value) => Some(value),
        _ => None,
    }
}

struct MappedIndexes {
    indexes: Vec<IndexObject>,
    direct_column_count: u64,
}

impl SqliteSnapshotMapper<'_> {
    fn map_generated_dependencies(
        &self,
        relations: &[RawRelation],
        table_keys: &BTreeMap<String, ObjectKey>,
        column_keys: &BTreeMap<(String, String), ObjectKey>,
        metadata: &mut CanonicalMetadata,
    ) -> Result<(), SqliteAdapterError> {
        for relation in relations.iter().filter(|relation| relation.kind.is_table()) {
            let Some(parsed) = &relation.parsed_table else {
                continue;
            };
            for column in parsed
                .columns
                .iter()
                .filter(|column| column.generated_expression.is_some())
            {
                let expression = column.generated_expression.as_deref().unwrap_or_default();
                let generated_key = column_keys
                    .get(&(
                        fold_identifier(&relation.name),
                        fold_identifier(&column.name),
                    ))
                    .cloned()
                    .ok_or_else(|| {
                        SqliteAdapterError::mapping(
                            format!("generated column {}.{}", relation.name, column.name),
                            "generated column is absent from table_xinfo",
                        )
                    })?;
                for dependency in self
                    .expression_dependencies(
                        &relation.name,
                        &[expression],
                        table_keys,
                        &BTreeMap::new(),
                        column_keys,
                    )?
                    .into_iter()
                    .filter(|key| {
                        key.object_kind == ObjectKind::Column
                            && same_identifier(&key.object_name, &relation.name)
                    })
                {
                    metadata.relationships.push(MetadataRelationship {
                        kind: MetadataRelationshipKind::DependsOn,
                        from_key: generated_key.clone(),
                        to_key: dependency,
                        ordinal: None,
                        properties: BTreeMap::new(),
                    });
                }
            }
        }
        Ok(())
    }

    fn map_indexes(
        &self,
        raw_indexes: &[RawIndex],
        table_keys: &BTreeMap<String, ObjectKey>,
        column_keys: &BTreeMap<(String, String), ObjectKey>,
        metadata: &mut CanonicalMetadata,
    ) -> Result<MappedIndexes, SqliteAdapterError> {
        let mut indexes = Vec::new();
        let mut direct_column_count = 0_u64;
        for raw in raw_indexes {
            let table_key = lookup_key(table_keys, &raw.table_name, "index table")?;
            let index_key = self.key(ObjectKind::Index, &raw.table_name, Some(raw.name.clone()));
            let key_terms = raw.terms.iter().filter(|term| term.key).collect::<Vec<_>>();
            let mut columns = Vec::new();
            let mut expression_terms = Vec::new();
            let mut probe_expressions = Vec::<String>::new();
            for (ordinal, term) in key_terms.iter().enumerate() {
                let parsed_term = raw
                    .parsed
                    .as_ref()
                    .and_then(|index| index.terms.get(ordinal));
                if term.cid >= 0 {
                    let column_name = term.column_name.as_deref().ok_or_else(|| {
                        SqliteAdapterError::mapping(
                            format!("index {}", raw.name),
                            "direct index term has no column name",
                        )
                    })?;
                    if parsed_term
                        .and_then(|term| term.column_name.as_deref())
                        .is_some_and(|parsed| !same_identifier(parsed, column_name))
                    {
                        return Err(SqliteAdapterError::mapping(
                            format!("index {}", raw.name),
                            "CREATE INDEX term disagrees with index_xinfo column",
                        ));
                    }
                    columns.extend(resolve_named_columns(
                        &raw.table_name,
                        &[column_name.to_owned()],
                        column_keys,
                    )?);
                    direct_column_count += 1;
                } else if term.cid == -2 {
                    let expression =
                        parsed_term
                            .map(|term| term.expression.clone())
                            .ok_or_else(|| {
                                SqliteAdapterError::mapping(
                                    format!("index {}", raw.name),
                                    "expression index term has no parsed CREATE INDEX expression",
                                )
                            })?;
                    expression_terms.push(expression.clone());
                    probe_expressions.push(expression);
                } else {
                    let expression = parsed_term
                        .map(|term| term.expression.clone())
                        .unwrap_or_else(|| "rowid".to_owned());
                    expression_terms.push(expression.clone());
                    probe_expressions.push(expression);
                }
                if let Some(parsed_term) = parsed_term {
                    let expected_descending = parsed_term.order.as_deref() == Some("DESC");
                    if expected_descending != term.descending {
                        return Err(SqliteAdapterError::mapping(
                            format!("index {} term {}", raw.name, ordinal + 1),
                            "CREATE INDEX ordering disagrees with index_xinfo",
                        ));
                    }
                }
            }
            if let Some(predicate) = raw
                .parsed
                .as_ref()
                .and_then(|index| index.predicate.clone())
            {
                probe_expressions.push(predicate);
            }
            if !probe_expressions.is_empty() {
                for dependency in self.expression_dependencies(
                    &raw.table_name,
                    &probe_expressions
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>(),
                    table_keys,
                    &BTreeMap::new(),
                    column_keys,
                )? {
                    metadata.relationships.push(MetadataRelationship {
                        kind: MetadataRelationshipKind::DependsOn,
                        from_key: index_key.clone(),
                        to_key: dependency,
                        ordinal: None,
                        properties: BTreeMap::new(),
                    });
                }
            }
            let mut auxiliary_columns = Vec::new();
            for term in raw.terms.iter().filter(|term| !term.key && term.cid >= 0) {
                let column_name = term.column_name.as_deref().ok_or_else(|| {
                    SqliteAdapterError::mapping(
                        format!("index {}", raw.name),
                        "auxiliary index term has no column name",
                    )
                })?;
                let column_key =
                    resolve_named_columns(&raw.table_name, &[column_name.to_owned()], column_keys)?
                        .pop()
                        .ok_or_else(|| {
                            SqliteAdapterError::mapping(
                                format!("index {}", raw.name),
                                "auxiliary index column did not resolve",
                            )
                        })?;
                auxiliary_columns.push(column_name.to_owned());
                let mut properties = BTreeMap::new();
                properties.insert(
                    "role".to_owned(),
                    MetadataValue::String("sqlite_auxiliary".to_owned()),
                );
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::IncludesColumn,
                    from_key: index_key.clone(),
                    to_key: column_key,
                    ordinal: Some(term.sequence.saturating_add(1)),
                    properties,
                });
            }
            let mut properties = BTreeMap::new();
            properties.insert(
                "origin".to_owned(),
                MetadataValue::String(raw.origin.clone()),
            );
            properties.insert("partial".to_owned(), MetadataValue::Boolean(raw.partial));
            properties.insert(
                "term_collations".to_owned(),
                MetadataValue::StringList(
                    key_terms
                        .iter()
                        .map(|term| term.collation.clone().unwrap_or_default())
                        .collect(),
                ),
            );
            properties.insert(
                "term_orders".to_owned(),
                MetadataValue::StringList(
                    key_terms
                        .iter()
                        .map(|term| {
                            if term.descending {
                                "DESC".to_owned()
                            } else {
                                "ASC".to_owned()
                            }
                        })
                        .collect(),
                ),
            );
            if let Some(parsed) = &raw.parsed {
                properties.insert(
                    "term_nulls".to_owned(),
                    MetadataValue::StringList(
                        parsed
                            .terms
                            .iter()
                            .map(|term| term.nulls.clone().unwrap_or_default())
                            .collect(),
                    ),
                );
            }
            if !auxiliary_columns.is_empty() {
                properties.insert(
                    "auxiliary_columns".to_owned(),
                    MetadataValue::StringList(auxiliary_columns),
                );
            }
            push_annotation_if_needed(metadata, &index_key, raw.sql.clone(), properties);
            indexes.push(IndexObject {
                key: index_key,
                table_key,
                name: raw.name.clone(),
                columns,
                is_unique: raw.unique,
                is_primary: raw.origin == "pk",
                predicate: raw
                    .parsed
                    .as_ref()
                    .and_then(|index| index.predicate.clone()),
                expression: (!expression_terms.is_empty()).then(|| expression_terms.join(", ")),
            });
        }
        Ok(MappedIndexes {
            indexes,
            direct_column_count,
        })
    }

    fn view_dependencies(
        &self,
        view: &RawRelation,
        table_keys: &BTreeMap<String, ObjectKey>,
        view_keys: &BTreeMap<String, ObjectKey>,
        column_keys: &BTreeMap<(String, String), ObjectKey>,
    ) -> Result<Vec<ObjectKey>, SqliteAdapterError> {
        let projection = if view.columns.is_empty() {
            "1".to_owned()
        } else {
            view.columns
                .iter()
                .map(|column| quote_identifier(&column.name))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let sql = format!(
            "EXPLAIN SELECT {projection} FROM {} LIMIT 0",
            quote_identifier(&view.name)
        );
        let accesses =
            capture_prepare_accesses(self.conn, &sql, AccessorFilter::Exact(view.name.clone()))?;
        map_accesses(accesses, table_keys, view_keys, column_keys)
    }

    fn expression_dependencies(
        &self,
        relation_name: &str,
        expressions: &[&str],
        table_keys: &BTreeMap<String, ObjectKey>,
        view_keys: &BTreeMap<String, ObjectKey>,
        column_keys: &BTreeMap<(String, String), ObjectKey>,
    ) -> Result<Vec<ObjectKey>, SqliteAdapterError> {
        if expressions.is_empty() {
            return Ok(vec![]);
        }
        let projection = expressions
            .iter()
            .map(|expression| format!("({expression})"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "EXPLAIN SELECT {projection} FROM {} LIMIT 0",
            quote_identifier(relation_name)
        );
        let accesses = capture_prepare_accesses(self.conn, &sql, AccessorFilter::Any)?;
        map_accesses(accesses, table_keys, view_keys, column_keys)
    }

    #[allow(clippy::too_many_arguments)]
    fn map_triggers(
        &self,
        raw_triggers: &[RawTrigger],
        relations: &[RawRelation],
        table_keys: &BTreeMap<String, ObjectKey>,
        view_keys: &BTreeMap<String, ObjectKey>,
        column_keys: &BTreeMap<(String, String), ObjectKey>,
        metadata: &mut CanonicalMetadata,
    ) -> Result<Vec<TriggerObject>, SqliteAdapterError> {
        let relation_by_name = relations
            .iter()
            .map(|relation| (fold_identifier(&relation.name), relation))
            .collect::<BTreeMap<_, _>>();
        let mut triggers = Vec::new();
        for raw in raw_triggers {
            let owner = table_keys
                .get(&fold_identifier(&raw.owner_name))
                .or_else(|| view_keys.get(&fold_identifier(&raw.owner_name)))
                .cloned()
                .ok_or_else(|| {
                    SqliteAdapterError::mapping(
                        format!("trigger {}", raw.name),
                        "trigger owner is absent from the selected relation inventory",
                    )
                })?;
            let trigger_key =
                self.key(ObjectKind::Trigger, &raw.owner_name, Some(raw.name.clone()));
            let owner_relation = relation_by_name
                .get(&fold_identifier(&raw.owner_name))
                .copied()
                .ok_or_else(|| {
                    SqliteAdapterError::mapping(
                        format!("trigger {}", raw.name),
                        "trigger owner metadata is unavailable",
                    )
                })?;
            let probe_sql = trigger_probe_sql(raw, owner_relation)?;
            let accesses = capture_prepare_accesses(
                self.conn,
                &probe_sql,
                AccessorFilter::Exact(raw.name.clone()),
            )?;
            let mut dependencies = map_accesses(accesses, table_keys, view_keys, column_keys)?;
            for update_column in &raw.parsed.update_columns {
                let column_key = column_keys
                    .get(&(
                        fold_identifier(&raw.owner_name),
                        fold_identifier(update_column),
                    ))
                    .cloned()
                    .ok_or_else(|| {
                        SqliteAdapterError::mapping(
                            format!("trigger {}", raw.name),
                            format!(
                                "UPDATE OF column '{update_column}' is not present on trigger owner"
                            ),
                        )
                    })?;
                dependencies.push(column_key);
            }
            deduplicate_keys(&mut dependencies);
            for dependency in dependencies {
                let mut properties = BTreeMap::new();
                properties.insert(
                    "access".to_owned(),
                    MetadataValue::String("trigger_prepare".to_owned()),
                );
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::DependsOn,
                    from_key: trigger_key.clone(),
                    to_key: dependency,
                    ordinal: None,
                    properties,
                });
            }
            let mut properties = BTreeMap::new();
            if !raw.parsed.update_columns.is_empty() {
                properties.insert(
                    "update_columns".to_owned(),
                    MetadataValue::StringList(raw.parsed.update_columns.clone()),
                );
            }
            if let Some(when_expression) = &raw.parsed.when_expression {
                properties.insert(
                    "when_expression".to_owned(),
                    MetadataValue::String(when_expression.clone()),
                );
            }
            push_annotation_if_needed(metadata, &trigger_key, None, properties);
            triggers.push(TriggerObject {
                key: trigger_key,
                table_key: owner,
                name: raw.name.clone(),
                timing: Some(raw.parsed.timing.clone()),
                events: vec![raw.parsed.event.clone()],
                definition: Some(raw.sql.clone()),
                executes_routine_key: None,
            });
        }
        Ok(triggers)
    }
}

fn trigger_probe_sql(
    trigger: &RawTrigger,
    owner: &RawRelation,
) -> Result<String, SqliteAdapterError> {
    let owner_name = quote_identifier(&owner.name);
    match trigger.parsed.event.as_str() {
        "INSERT" => Ok(format!("EXPLAIN INSERT INTO {owner_name} DEFAULT VALUES")),
        "DELETE" => Ok(format!("EXPLAIN DELETE FROM {owner_name} WHERE 0")),
        "UPDATE" => {
            let column_name = trigger
                .parsed
                .update_columns
                .first()
                .cloned()
                .or_else(|| {
                    owner
                        .columns
                        .iter()
                        .find(|column| !matches!(column.hidden, 2 | 3))
                        .map(|column| column.name.clone())
                })
                .ok_or_else(|| {
                    SqliteAdapterError::mapping(
                        format!("trigger {}", trigger.name),
                        "cannot prepare an UPDATE event for an owner with no writable columns",
                    )
                })?;
            if !owner
                .columns
                .iter()
                .any(|column| same_identifier(&column.name, &column_name))
            {
                return Err(SqliteAdapterError::mapping(
                    format!("trigger {}", trigger.name),
                    format!("UPDATE OF column '{column_name}' is not present on the owner"),
                ));
            }
            let column = quote_identifier(&column_name);
            Ok(format!(
                "EXPLAIN UPDATE {owner_name} SET {column} = {column} WHERE 0"
            ))
        }
        event => Err(SqliteAdapterError::mapping(
            format!("trigger {}", trigger.name),
            format!("unsupported parsed trigger event '{event}'"),
        )),
    }
}

#[derive(Clone)]
enum AccessorFilter {
    Any,
    Exact(String),
}

