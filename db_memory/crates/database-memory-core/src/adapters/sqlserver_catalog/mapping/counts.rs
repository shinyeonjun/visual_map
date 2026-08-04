fn validate_relationship_uniqueness(
    relationships: &[MetadataRelationship],
) -> Result<(), CatalogError> {
    let mut seen = BTreeSet::new();
    for relationship in relationships {
        let identity = (
            relationship.kind.clone(),
            relationship.from_key.to_string(),
            relationship.to_key.to_string(),
            relationship.ordinal,
        );
        if !seen.insert(identity) {
            return Err(CatalogError::Mapping(format!(
                "duplicate metadata relationship {}:{}->{}",
                relationship.kind.graph_edge_type(),
                relationship.from_key,
                relationship.to_key
            )));
        }
    }
    Ok(())
}

fn discovery_counts_from_catalog(
    raw: &RawSqlServerCatalog,
    snapshot: &CanonicalSchemaSnapshot,
    projection: SqlServerProjectionLedger,
) -> Result<DiscoveryCounts, CatalogError> {
    let emitted_objects = emitted_object_counts(snapshot);
    let emitted_relationships = emitted_relationship_counts(snapshot);
    let table_ids = raw
        .tables
        .iter()
        .map(|table| table.id)
        .collect::<BTreeSet<_>>();
    let table_type_ids = raw
        .user_types
        .iter()
        .filter_map(|data_type| data_type.table_object_id)
        .collect::<BTreeSet<_>>();
    let mut expected_objects = ObjectCategory::ALL
        .into_iter()
        .map(|category| (category, 0_u64))
        .collect::<BTreeMap<_, _>>();
    expected_objects.insert(ObjectCategory::Database, 1);
    expected_objects.insert(ObjectCategory::Schema, raw.schemas.len() as u64);
    expected_objects.insert(ObjectCategory::Principal, raw.principals.len() as u64);
    expected_objects.insert(ObjectCategory::Table, raw.tables.len() as u64);
    expected_objects.insert(
        ObjectCategory::Column,
        raw.columns
            .iter()
            .filter(|column| column.object_type == "U")
            .count() as u64,
    );
    expected_objects.insert(
        ObjectCategory::ViewColumn,
        raw.columns
            .iter()
            .filter(|column| column.object_type == "V")
            .count() as u64,
    );
    expected_objects.insert(
        ObjectCategory::PrimaryKey,
        raw.constraints
            .iter()
            .filter(|constraint| {
                table_ids.contains(&constraint.table_id)
                    && constraint.kind == ConstraintKind::PrimaryKey
            })
            .count() as u64,
    );
    expected_objects.insert(
        ObjectCategory::ForeignKey,
        raw.constraints
            .iter()
            .filter(|constraint| {
                table_ids.contains(&constraint.table_id)
                    && constraint.kind == ConstraintKind::ForeignKey
            })
            .count() as u64,
    );
    expected_objects.insert(
        ObjectCategory::UniqueConstraint,
        raw.constraints
            .iter()
            .filter(|constraint| {
                table_ids.contains(&constraint.table_id)
                    && constraint.kind == ConstraintKind::Unique
            })
            .count() as u64,
    );
    expected_objects.insert(
        ObjectCategory::CheckConstraint,
        raw.constraints
            .iter()
            .filter(|constraint| {
                table_ids.contains(&constraint.table_id) && constraint.kind == ConstraintKind::Check
            })
            .count() as u64,
    );
    expected_objects.insert(
        ObjectCategory::Index,
        raw.indexes
            .iter()
            .filter(|index| index.relation_type != "TT")
            .count() as u64,
    );
    expected_objects.insert(
        ObjectCategory::View,
        raw.views.iter().filter(|view| !view.indexed).count() as u64,
    );
    expected_objects.insert(
        ObjectCategory::MaterializedView,
        raw.views.iter().filter(|view| view.indexed).count() as u64,
    );
    expected_objects.insert(ObjectCategory::Routine, raw.routines.len() as u64);
    expected_objects.insert(
        ObjectCategory::RoutineParameter,
        raw.parameters.len() as u64,
    );
    expected_objects.insert(ObjectCategory::Trigger, raw.triggers.len() as u64);
    expected_objects.insert(ObjectCategory::UserDefinedType, raw.user_types.len() as u64);
    expected_objects.insert(ObjectCategory::Sequence, raw.sequences.len() as u64);
    expected_objects.insert(ObjectCategory::Synonym, raw.synonyms.len() as u64);
    expected_objects.insert(ObjectCategory::Policy, raw.security_policies.len() as u64);
    expected_objects.insert(
        ObjectCategory::Extension,
        expected_extension_object_count(raw, &table_type_ids, projection),
    );
    if expected_objects != emitted_objects {
        return Err(CatalogError::Mapping(format!(
            "SQL Server raw/emitted object counts differ: raw={expected_objects:?}, emitted={emitted_objects:?}"
        )));
    }

    let mut expected_relationships = RelationshipCategory::ALL
        .into_iter()
        .map(|category| (category, 0_u64))
        .collect::<BTreeMap<_, _>>();
    expected_relationships.insert(
        RelationshipCategory::DatabaseHasSchema,
        raw.schemas.len() as u64,
    );
    expected_relationships.insert(
        RelationshipCategory::SchemaHasTable,
        raw.tables.len() as u64,
    );
    expected_relationships.insert(
        RelationshipCategory::TableHasColumn,
        raw.columns
            .iter()
            .filter(|column| column.object_type == "U")
            .count() as u64,
    );
    expected_relationships.insert(
        RelationshipCategory::TableHasConstraint,
        raw.constraints
            .iter()
            .filter(|constraint| table_ids.contains(&constraint.table_id))
            .count() as u64,
    );
    expected_relationships.insert(
        RelationshipCategory::ConstraintColumn,
        raw.constraints
            .iter()
            .filter(|constraint| {
                table_ids.contains(&constraint.table_id)
                    && constraint.kind != ConstraintKind::ForeignKey
            })
            .map(|constraint| constraint.columns.len() as u64)
            .sum(),
    );
    expected_relationships.insert(
        RelationshipCategory::ForeignKeyColumnPair,
        raw.constraints
            .iter()
            .filter(|constraint| {
                table_ids.contains(&constraint.table_id)
                    && constraint.kind == ConstraintKind::ForeignKey
            })
            .map(|constraint| constraint.columns.len() as u64)
            .sum(),
    );
    expected_relationships.insert(
        RelationshipCategory::TableHasIndex,
        raw.indexes
            .iter()
            .filter(|index| index.relation_type == "U")
            .count() as u64,
    );
    expected_relationships.insert(
        RelationshipCategory::IndexColumn,
        raw.indexes
            .iter()
            .filter(|index| index.relation_type == "U")
            .map(projected_index_column_count)
            .sum(),
    );
    expected_relationships.insert(
        RelationshipCategory::SchemaHasView,
        raw.views.iter().filter(|view| !view.indexed).count() as u64,
    );
    expected_relationships.insert(
        RelationshipCategory::ViewDependency,
        projection.view_dependencies,
    );
    expected_relationships.insert(
        RelationshipCategory::TriggerTarget,
        raw.triggers
            .iter()
            .filter(|trigger| {
                trigger.parent_class == 1
                    && trigger
                        .parent_type
                        .as_deref()
                        .is_some_and(|kind| kind == "U" || kind == "V")
                    && !raw
                        .views
                        .iter()
                        .any(|view| view.indexed && view.id == trigger.parent_id)
            })
            .count() as u64,
    );
    expected_relationships.insert(RelationshipCategory::TriggerRoutine, 0);
    expected_relationships.insert(
        RelationshipCategory::SchemaHasRoutine,
        raw.routines.len() as u64,
    );
    expected_relationships.insert(
        RelationshipCategory::RoutineDependency,
        projection.routine_dependencies,
    );
    expected_relationships.insert(
        RelationshipCategory::MetadataParent,
        expected_metadata_parent_count(raw, &table_type_ids, projection),
    );
    expected_relationships.insert(
        RelationshipCategory::MetadataRelationship,
        expected_metadata_relationship_count(raw, &table_type_ids, projection),
    );
    if expected_relationships != emitted_relationships {
        return Err(CatalogError::Mapping(format!(
            "SQL Server raw/emitted relationship counts differ: raw={expected_relationships:?}, emitted={emitted_relationships:?}"
        )));
    }

    Ok(DiscoveryCounts {
        objects: expected_objects
            .into_iter()
            .map(|(category, count)| {
                (
                    category,
                    DiscoveredCount {
                        count,
                        evidence: "SQL Server sys catalog raw inventory".to_owned(),
                    },
                )
            })
            .collect(),
        relationships: expected_relationships
            .into_iter()
            .map(|(category, count)| {
                (
                    category,
                    DiscoveredCount {
                        count,
                        evidence: "SQL Server catalog identity and dependency ledger".to_owned(),
                    },
                )
            })
            .collect(),
    })
}

fn expected_extension_object_count(
    raw: &RawSqlServerCatalog,
    table_type_ids: &BTreeSet<i32>,
    projection: SqlServerProjectionLedger,
) -> u64 {
    let table_type_columns = raw
        .columns
        .iter()
        .filter(|column| table_type_ids.contains(&column.object_id))
        .count() as u64;
    let table_type_constraints = raw
        .constraints
        .iter()
        .filter(|constraint| table_type_ids.contains(&constraint.table_id))
        .count() as u64;
    let table_type_indexes = raw
        .indexes
        .iter()
        .filter(|index| table_type_ids.contains(&index.object_id))
        .count() as u64;
    let security_predicates = raw
        .security_policies
        .iter()
        .map(|policy| policy.predicates.len() as u64)
        .sum::<u64>();
    let partition_boundaries = raw
        .partition_functions
        .iter()
        .map(|function| function.values.len() as u64)
        .sum::<u64>();
    let xml_namespaces = raw
        .xml_schema_collections
        .iter()
        .map(|collection| collection.namespaces.len() as u64)
        .sum::<u64>();

    table_type_columns
        + table_type_constraints
        + table_type_indexes
        + security_predicates
        + raw.partition_functions.len() as u64
        + partition_boundaries
        + raw.partition_schemes.len() as u64
        + raw.partitions.len() as u64
        + raw.xml_schema_collections.len() as u64
        + xml_namespaces
        + raw.extended_properties.len() as u64
        + projection.external_reference_objects
}

fn expected_metadata_parent_count(
    raw: &RawSqlServerCatalog,
    table_type_ids: &BTreeSet<i32>,
    projection: SqlServerProjectionLedger,
) -> u64 {
    let indexed_view_ids = raw
        .views
        .iter()
        .filter(|view| view.indexed)
        .map(|view| view.id)
        .collect::<BTreeSet<_>>();
    let metadata_triggers = raw
        .triggers
        .iter()
        .filter(|trigger| {
            trigger.parent_class == 0
                || (trigger.parent_class == 1 && indexed_view_ids.contains(&trigger.parent_id))
        })
        .count() as u64;

    raw.principals.len() as u64
        + raw.user_types.len() as u64
        + raw.sequences.len() as u64
        + indexed_view_ids.len() as u64
        + metadata_triggers
        + raw
            .columns
            .iter()
            .filter(|column| column.object_type == "V")
            .count() as u64
        + raw
            .indexes
            .iter()
            .filter(|index| index.relation_type == "V")
            .count() as u64
        + raw.parameters.len() as u64
        + raw.synonyms.len() as u64
        + raw.security_policies.len() as u64
        + expected_extension_object_count(raw, table_type_ids, projection)
}

fn expected_metadata_relationship_count(
    raw: &RawSqlServerCatalog,
    table_type_ids: &BTreeSet<i32>,
    projection: SqlServerProjectionLedger,
) -> u64 {
    let user_type_ids = raw
        .user_types
        .iter()
        .map(|data_type| data_type.id)
        .collect::<BTreeSet<_>>();
    let ownerships = raw.schemas.len()
        + raw.sequences.len()
        + raw.tables.len()
        + raw.views.len()
        + raw.routines.len()
        + raw.synonyms.len()
        + raw.security_policies.len()
        + raw
            .principals
            .iter()
            .filter(|principal| principal.owning_principal_id.is_some())
            .count();
    let sequence_types = raw
        .sequences
        .iter()
        .filter(|sequence| user_type_ids.contains(&sequence.type_id))
        .count() as u64;
    let column_types = raw
        .columns
        .iter()
        .filter(|column| user_type_ids.contains(&column.type_id))
        .count() as u64;
    let table_type_constraint_columns = raw
        .constraints
        .iter()
        .filter(|constraint| table_type_ids.contains(&constraint.table_id))
        .map(|constraint| constraint.columns.len() as u64)
        .sum::<u64>();
    let table_type_index_columns = raw
        .indexes
        .iter()
        .filter(|index| table_type_ids.contains(&index.object_id))
        .map(|index| index.columns.len() as u64)
        .sum::<u64>();
    let parameter_types = raw
        .parameters
        .iter()
        .filter(|parameter| user_type_ids.contains(&parameter.type_id))
        .count() as u64;
    let security_predicates = raw
        .security_policies
        .iter()
        .map(|policy| policy.predicates.len() as u64)
        .sum::<u64>();
    let included_columns = raw
        .indexes
        .iter()
        .filter(|index| index.relation_type == "U" || index.relation_type == "V")
        .flat_map(|index| &index.columns)
        .filter(|column| column.included)
        .count() as u64;
    let partition_scheme_ids = raw
        .partition_schemes
        .iter()
        .map(|scheme| scheme.id)
        .collect::<BTreeSet<_>>();
    let index_data_spaces = raw
        .indexes
        .iter()
        .map(|index| ((index.object_id, index.id), index.data_space_id))
        .collect::<BTreeMap<_, _>>();
    let partition_scheme_uses = raw
        .partitions
        .iter()
        .filter(|partition| {
            index_data_spaces
                .get(&(partition.object_id, partition.index_id))
                .is_some_and(|id| partition_scheme_ids.contains(id))
        })
        .count() as u64;
    let temporal_histories = raw
        .tables
        .iter()
        .filter(|table| table.history_schema.is_some() && table.history_table.is_some())
        .count() as u64;
    let typed_xml_columns = raw
        .columns
        .iter()
        .filter(|column| column.xml_collection_id > 0)
        .count() as u64;
    let typed_xml_parameters = raw
        .parameters
        .iter()
        .filter(|parameter| parameter.xml_collection_id > 0)
        .count() as u64;

    ownerships as u64
        + sequence_types
        + column_types
        + table_type_constraint_columns
        + table_type_index_columns
        + raw.parameters.len() as u64
        + parameter_types
        + security_predicates
        + included_columns
        + raw.partition_schemes.len() as u64
        + partition_scheme_uses
        + temporal_histories
        + raw.synonyms.len() as u64
        + raw.xml_schema_collections.len() as u64
        + typed_xml_columns
        + typed_xml_parameters
        + projection.dependency_metadata_relationships
}

fn projected_index_column_count(index: &RawIndex) -> u64 {
    let key_columns = index
        .columns
        .iter()
        .filter(|column| column.key_ordinal > 0)
        .count();
    if key_columns == 0 {
        index.columns.len() as u64
    } else {
        key_columns as u64
    }
}

fn sqlserver_capabilities() -> AdapterCapabilities {
    AdapterCapabilities {
        source_kind: SQLSERVER_SOURCE.to_owned(),
        metadata_only: true,
        schemas: true,
        tables: true,
        columns: true,
        constraints: true,
        indexes: true,
        views: CapabilitySupport::Supported,
        triggers: CapabilitySupport::Supported,
        routines: CapabilitySupport::Supported,
        dependencies: CapabilitySupport::Supported,
        limitations: Vec::new(),
        notes: vec![
            "Reads SQL Server sys catalog metadata and module definitions only; application table rows are never queried.".to_owned(),
            "Dynamic SQL, encrypted definitions, runtime-bound dependencies, and unsupported CLR or legacy objects fail closed.".to_owned(),
        ],
    }
}

fn sqlserver_capability_checks(
    facts: &ServerFacts,
    strategy: SqlServerCatalogVersion,
) -> Vec<CapabilityCheck> {
    vec![
        CapabilityCheck {
            name: "catalog_version_strategy".to_owned(),
            evidence: strategy.strategy_name().to_owned(),
        },
        CapabilityCheck {
            name: "metadata_visibility".to_owned(),
            evidence: "database VIEW DEFINITION and dependency SELECT effective".to_owned(),
        },
        CapabilityCheck {
            name: "catalog_stability".to_owned(),
            evidence: "two exact ordered raw catalog reads matched under READ COMMITTED".to_owned(),
        },
        CapabilityCheck {
            name: "metadata_only".to_owned(),
            evidence: "adapter queries sys catalogs, SERVERPROPERTY, and metadata functions only"
                .to_owned(),
        },
        CapabilityCheck {
            name: "transport".to_owned(),
            evidence: if facts.encrypted_transport {
                "TDS transport reported encrypted".to_owned()
            } else {
                "loopback TDS transport reported unencrypted".to_owned()
            },
        },
        CapabilityCheck {
            name: "module_dependency_policy".to_owned(),
            evidence: "dynamic, encrypted, CLR, caller-dependent, and ambiguous modules reject certification"
                .to_owned(),
        },
        CapabilityCheck {
            name: "xml_schema_collections".to_owned(),
            evidence: "typed XML columns and parameters resolve to sys.xml_schema_collections"
                .to_owned(),
        },
        CapabilityCheck {
            name: "extended_properties".to_owned(),
            evidence: "supported sys.extended_properties targets preserve sql_variant type, display, and raw hex values"
                .to_owned(),
        },
    ]
}
