fn map_view_routine_relationships(
    metadata: &mut CanonicalMetadata,
    raw: &RawMysqlFamilyCatalog,
    view_keys: &BTreeMap<String, ObjectKey>,
    routine_keys: &BTreeMap<String, ObjectKey>,
) -> Result<(), CatalogError> {
    for usage in &raw.view_routine_usage {
        if normalize_object_name(&usage.routine_schema, raw.facts.lower_case_table_names)
            != normalize_object_name(&raw.facts.database, raw.facts.lower_case_table_names)
        {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "view '{}' invokes out-of-scope routine '{}.{}'",
                usage.view, usage.routine_schema, usage.specific_name
            )));
        }
        let view = view_keys
            .get(&normalize_object_name(
                &usage.view,
                raw.facts.lower_case_table_names,
            ))
            .cloned()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "VIEW_ROUTINE_USAGE references missing view '{}'",
                    usage.view
                ))
            })?;
        let routine = routine_keys
            .get(&usage.specific_name.to_ascii_lowercase())
            .cloned()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "VIEW_ROUTINE_USAGE references missing routine '{}'",
                    usage.specific_name
                ))
            })?;
        metadata.relationships.push(MetadataRelationship {
            kind: MetadataRelationshipKind::Invokes,
            from_key: view,
            to_key: routine,
            ordinal: None,
            properties: BTreeMap::new(),
        });
    }
    Ok(())
}

fn validate_relationship_uniqueness(
    relationships: &[MetadataRelationship],
) -> Result<(), CatalogError> {
    let mut seen = BTreeSet::new();
    for relationship in relationships.iter() {
        let identity = (
            relationship.kind.clone(),
            relationship.from_key.to_string(),
            relationship.to_key.to_string(),
            relationship.ordinal,
        );
        if !seen.insert(identity) {
            return Err(CatalogError::Mapping(format!(
                "duplicate metadata relationship {} -> {}",
                relationship.from_key, relationship.to_key
            )));
        }
    }
    Ok(())
}

fn mysql_family_capabilities(
    source_kind: &str,
    raw: &RawMysqlFamilyCatalog,
) -> AdapterCapabilities {
    let has_routines = !raw.routines.is_empty();
    let has_triggers = !raw.triggers.is_empty();
    let has_events = !raw.events.is_empty();
    let has_opaque_procedural_metadata = raw
        .routines
        .iter()
        .any(|routine| routine.definition.is_none())
        || raw
            .triggers
            .iter()
            .any(|trigger| trigger.statement.is_none())
        || raw.events.iter().any(|event| event.definition.is_none());
    let mut limitations = Vec::new();
    if has_routines {
        limitations.push(format!(
            "{} routine body dependency path(s) are not catalog-proven; only direct catalog relationships are emitted",
            raw.routines.len()
        ));
    }
    if has_triggers {
        limitations.push(format!(
            "{} trigger body dependency path(s) are not catalog-proven; trigger target relationships are emitted",
            raw.triggers.len()
        ));
    }
    if has_events {
        limitations.push(format!(
            "{} scheduled event body dependency path(s) are not catalog-proven",
            raw.events.len()
        ));
    }
    if has_opaque_procedural_metadata {
        limitations.push(
            "one or more procedural definitions are hidden; structural metadata is retained without guessed SQL"
                .to_owned(),
        );
    }
    AdapterCapabilities {
        source_kind: source_kind.to_owned(),
        metadata_only: true,
        schemas: true,
        tables: true,
        columns: true,
        constraints: true,
        indexes: true,
        views: CapabilitySupport::Supported,
        triggers: if has_triggers {
            CapabilitySupport::Partial
        } else {
            CapabilitySupport::Supported
        },
        routines: if has_routines {
            CapabilitySupport::Partial
        } else {
            CapabilitySupport::Supported
        },
        dependencies: if has_routines || has_triggers || has_events {
            CapabilitySupport::Partial
        } else {
            CapabilitySupport::Supported
        },
        limitations,
        notes: vec![
            "Reads INFORMATION_SCHEMA and SHOW CREATE metadata only; application table rows are never queried."
                .to_owned(),
            "The selected MySQL-family database is mapped to the common database and schema scope."
                .to_owned(),
            "Objects whose procedural dependencies cannot be proven remain structural boundary objects; no guessed dependency edge is emitted."
                .to_owned(),
        ],
    }
}

fn mysql_family_capability_checks(raw: &RawMysqlFamilyCatalog) -> Vec<CapabilityCheck> {
    vec![
        CapabilityCheck {
            name: "catalog_stability".to_owned(),
            evidence: "ordered metadata signatures matched before and after catalog discovery"
                .to_owned(),
        },
        CapabilityCheck {
            name: "metadata_only_catalog_queries".to_owned(),
            evidence: "adapter queried INFORMATION_SCHEMA, session/server facts, and SHOW CREATE SEQUENCE only; no application relation appears in a SELECT FROM clause"
                .to_owned(),
        },
        CapabilityCheck {
            name: "metadata_visibility".to_owned(),
            evidence: format!(
                "effective schema/global privilege proof includes SELECT, SHOW VIEW, EXECUTE, EVENT, and TRIGGER ({} privilege entries)",
                raw.grants.len()
            ),
        },
        CapabilityCheck {
            name: "principal_context".to_owned(),
            evidence: format!(
                "current_user={} session_user={} active_roles={}",
                raw.facts.current_user,
                raw.facts.session_user,
                raw.active_roles.len()
            ),
        },
        CapabilityCheck {
            name: "read_only_repeatable_read_transaction".to_owned(),
            evidence: format!(
                "transaction_read_only={} transaction_isolation={}",
                raw.transaction_read_only, raw.transaction_isolation
            ),
        },
        CapabilityCheck {
            name: "supported_server_version".to_owned(),
            evidence: format!(
                "server version {} maps to certified strategy {}",
                raw.facts.version,
                raw.strategy.label()
            ),
        },
        CapabilityCheck {
            name: "transport_security".to_owned(),
            evidence: raw
                .facts
                .tls_cipher
                .as_deref()
                .map(|cipher| format!("TLS enabled with cipher {cipher}"))
                .unwrap_or_else(|| {
                    "plaintext transport is accepted only by the connection policy for a loopback/local endpoint"
                        .to_owned()
                }),
        },
        CapabilityCheck {
            name: "view_dependency_proof".to_owned(),
            evidence: match raw.strategy.product() {
                MysqlProduct::Mysql => format!(
                    "{} VIEW_TABLE_USAGE and {} VIEW_ROUTINE_USAGE rows reconciled to canonical dependencies",
                    raw.view_table_usage.len(),
                    raw.view_routine_usage.len()
                ),
                MysqlProduct::MariaDb => format!(
                    "all {} frozen MariaDB view definitions were parsed with the MySQL SQL AST dialect",
                    raw.views.len()
                ),
            },
        },
    ]
}

fn discovery_counts_from_catalog(
    raw: &RawMysqlFamilyCatalog,
    snapshot: &CanonicalSchemaSnapshot,
) -> Result<DiscoveryCounts, CatalogError> {
    let table_type_by_name = raw
        .tables
        .iter()
        .map(|table| {
            (
                normalize_object_name(&table.name, raw.facts.lower_case_table_names),
                table.table_type.to_ascii_uppercase(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let base_table_count = raw
        .tables
        .iter()
        .filter(|table| table.table_type.eq_ignore_ascii_case("BASE TABLE"))
        .count() as u64;
    let base_column_count = raw
        .columns
        .iter()
        .filter(|column| {
            table_type_by_name
                .get(&normalize_object_name(
                    &column.table,
                    raw.facts.lower_case_table_names,
                ))
                .is_some_and(|table_type| table_type == "BASE TABLE")
        })
        .count() as u64;
    let view_column_count = raw
        .columns
        .iter()
        .filter(|column| {
            table_type_by_name
                .get(&normalize_object_name(
                    &column.table,
                    raw.facts.lower_case_table_names,
                ))
                .is_some_and(|table_type| table_type == "VIEW")
        })
        .count() as u64;
    let sequence_column_count = raw
        .columns
        .iter()
        .filter(|column| {
            table_type_by_name
                .get(&normalize_object_name(
                    &column.table,
                    raw.facts.lower_case_table_names,
                ))
                .is_some_and(|table_type| table_type == "SEQUENCE")
        })
        .count() as u64;
    let index_identities = raw
        .index_parts
        .iter()
        .map(|part| {
            (
                normalize_object_name(&part.table, raw.facts.lower_case_table_names),
                part.index.clone(),
            )
        })
        .collect::<BTreeSet<_>>();
    let partition_identities = raw
        .partitions
        .iter()
        .map(|partition| {
            (
                normalize_object_name(&partition.table, raw.facts.lower_case_table_names),
                partition.partition.clone(),
            )
        })
        .collect::<BTreeSet<_>>();
    let subpartition_count = raw
        .partitions
        .iter()
        .filter(|partition| partition.subpartition.is_some())
        .count() as u64;

    let mut objects = ObjectCategory::ALL
        .into_iter()
        .map(|category| (category, 0_u64))
        .collect::<BTreeMap<_, _>>();
    objects.insert(ObjectCategory::Database, 1);
    objects.insert(ObjectCategory::Schema, 1);
    objects.insert(ObjectCategory::Table, base_table_count);
    objects.insert(ObjectCategory::Column, base_column_count);
    for (constraint_type, category) in [
        ("PRIMARY KEY", ObjectCategory::PrimaryKey),
        ("FOREIGN KEY", ObjectCategory::ForeignKey),
        ("UNIQUE", ObjectCategory::UniqueConstraint),
        ("CHECK", ObjectCategory::CheckConstraint),
    ] {
        objects.insert(
            category,
            raw.constraints
                .iter()
                .filter(|constraint| constraint.constraint_type == constraint_type)
                .count() as u64,
        );
    }
    objects.insert(ObjectCategory::Index, index_identities.len() as u64);
    objects.insert(ObjectCategory::View, raw.views.len() as u64);
    objects.insert(ObjectCategory::ViewColumn, view_column_count);
    objects.insert(ObjectCategory::Trigger, raw.triggers.len() as u64);
    objects.insert(ObjectCategory::Routine, raw.routines.len() as u64);
    objects.insert(ObjectCategory::Sequence, raw.sequences.len() as u64);
    objects.insert(
        ObjectCategory::RoutineParameter,
        raw.parameters.len() as u64,
    );
    objects.insert(ObjectCategory::Event, raw.events.len() as u64);
    objects.insert(
        ObjectCategory::Principal,
        1_u64 + raw.active_roles.len() as u64,
    );
    objects.insert(
        ObjectCategory::Extension,
        sequence_column_count + partition_identities.len() as u64 + subpartition_count,
    );

    let emitted_objects = emitted_object_counts(snapshot);
    for category in ObjectCategory::ALL {
        let discovered = objects.get(&category).copied().unwrap_or_default();
        let emitted = emitted_objects.get(&category).copied().unwrap_or_default();
        if discovered != emitted {
            return Err(CatalogError::Mapping(format!(
                "{} raw/emitted object count mismatch for {category:?}: discovered={discovered}, emitted={emitted}",
                raw.strategy.label()
            )));
        }
    }

    let constraint_types = raw
        .constraints
        .iter()
        .map(|constraint| {
            (
                (
                    normalize_object_name(&constraint.table, raw.facts.lower_case_table_names),
                    constraint.name.clone(),
                ),
                constraint.constraint_type.as_str(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let constraint_columns = raw
        .key_usage
        .iter()
        .filter(|usage| {
            constraint_types.get(&(
                normalize_object_name(&usage.table, raw.facts.lower_case_table_names),
                usage.constraint.clone(),
            )) != Some(&"FOREIGN KEY")
        })
        .count() as u64;
    let foreign_key_pairs = raw
        .key_usage
        .iter()
        .filter(|usage| {
            constraint_types.get(&(
                normalize_object_name(&usage.table, raw.facts.lower_case_table_names),
                usage.constraint.clone(),
            )) == Some(&"FOREIGN KEY")
        })
        .count() as u64;
    let index_column_count = raw
        .index_parts
        .iter()
        .filter(|part| part.column.is_some())
        .count() as u64;

    let mut relationships = RelationshipCategory::ALL
        .into_iter()
        .map(|category| (category, 0_u64))
        .collect::<BTreeMap<_, _>>();
    relationships.insert(RelationshipCategory::DatabaseHasSchema, 1);
    relationships.insert(RelationshipCategory::SchemaHasTable, base_table_count);
    relationships.insert(RelationshipCategory::TableHasColumn, base_column_count);
    relationships.insert(
        RelationshipCategory::TableHasConstraint,
        raw.constraints.len() as u64,
    );
    relationships.insert(RelationshipCategory::ConstraintColumn, constraint_columns);
    relationships.insert(
        RelationshipCategory::ForeignKeyColumnPair,
        foreign_key_pairs,
    );
    relationships.insert(
        RelationshipCategory::TableHasIndex,
        index_identities.len() as u64,
    );
    relationships.insert(RelationshipCategory::IndexColumn, index_column_count);
    relationships.insert(RelationshipCategory::SchemaHasView, raw.views.len() as u64);
    relationships.insert(
        RelationshipCategory::ViewDependency,
        snapshot
            .schema
            .views
            .iter()
            .map(|view| view.depends_on.len() as u64)
            .sum(),
    );
    relationships.insert(
        RelationshipCategory::TriggerTarget,
        raw.triggers.len() as u64,
    );
    relationships.insert(RelationshipCategory::TriggerRoutine, 0);
    relationships.insert(
        RelationshipCategory::SchemaHasRoutine,
        raw.routines.len() as u64,
    );
    relationships.insert(RelationshipCategory::RoutineDependency, 0);
    relationships.insert(
        RelationshipCategory::MetadataParent,
        snapshot
            .metadata
            .objects
            .iter()
            .filter(|object| object.parent_key.is_some())
            .count() as u64,
    );
    relationships.insert(
        RelationshipCategory::MetadataRelationship,
        snapshot.metadata.relationships.len() as u64,
    );

    let emitted_relationships = emitted_relationship_counts(snapshot);
    for category in RelationshipCategory::ALL {
        let discovered = relationships.get(&category).copied().unwrap_or_default();
        let emitted = emitted_relationships
            .get(&category)
            .copied()
            .unwrap_or_default();
        if discovered != emitted {
            return Err(CatalogError::Mapping(format!(
                "{} raw/emitted relationship count mismatch for {category:?}: discovered={discovered}, emitted={emitted}",
                raw.strategy.label()
            )));
        }
    }

    Ok(DiscoveryCounts {
        objects: objects
            .into_iter()
            .map(|(category, count)| {
                (
                    category,
                    DiscoveredCount {
                        count,
                        evidence: format!(
                            "{} INFORMATION_SCHEMA raw object inventory for {category:?}",
                            raw.strategy.label()
                        ),
                    },
                )
            })
            .collect(),
        relationships: relationships
            .into_iter()
            .map(|(category, count)| {
                (
                    category,
                    DiscoveredCount {
                        count,
                        evidence: format!(
                            "{} strict relationship ledger for {category:?}",
                            raw.strategy.label()
                        ),
                    },
                )
            })
            .collect(),
    })
}
