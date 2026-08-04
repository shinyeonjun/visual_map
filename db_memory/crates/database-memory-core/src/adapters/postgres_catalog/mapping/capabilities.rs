fn pg_catalog_complete_capabilities(
    strategy: PgCatalogStrategy,
    raw: &RawPostgresCatalog,
) -> AdapterCapabilities {
    let opaque_routines = raw
        .routines
        .iter()
        .filter(|routine| !routine.body_catalog_tracked)
        .count();
    let mut limitations = Vec::new();
    let (routines, dependencies) = if opaque_routines == 0 {
        (CapabilitySupport::Supported, CapabilitySupport::Supported)
    } else {
        limitations.push(format!(
            "{} routine body or dependency path(s) are opaque; only catalog-proven routine edges are emitted",
            opaque_routines
        ));
        (CapabilitySupport::Partial, CapabilitySupport::Partial)
    };
    AdapterCapabilities {
        source_kind: strategy.source_kind().to_owned(),
        metadata_only: true,
        schemas: true,
        tables: true,
        columns: true,
        constraints: true,
        indexes: true,
        views: CapabilitySupport::Supported,
        triggers: CapabilitySupport::Supported,
        routines,
        dependencies,
        limitations,
        notes: vec![
            format!(
                "Reads {} pg_catalog metadata in one read-only repeatable-read transaction; application rows are never queried.",
                strategy.product_name()
            ),
            "Only pg_catalog-proven routine dependencies are emitted; opaque routine bodies remain structural boundary objects."
                .to_owned(),
            "System-schema implementation dependencies are outside the declared application schema scope."
                .to_owned(),
        ],
    }
}

fn discovery_counts_from_catalog(
    raw: &RawPostgresCatalog,
    snapshot: &CanonicalSchemaSnapshot,
) -> Result<DiscoveryCounts, CatalogError> {
    let relation_kinds = raw
        .relations
        .iter()
        .map(|relation| (relation.oid, relation.relkind))
        .collect::<BTreeMap<_, _>>();
    let mut objects = ObjectCategory::ALL
        .into_iter()
        .map(|category| (category, 0_u64))
        .collect::<BTreeMap<_, _>>();
    objects.insert(ObjectCategory::Database, 1);
    objects.insert(ObjectCategory::Schema, raw.schemas.len() as u64);
    objects.insert(
        ObjectCategory::Table,
        raw.relations
            .iter()
            .filter(|relation| matches!(relation.relkind, 'r' | 'p' | 'f'))
            .count() as u64,
    );
    objects.insert(
        ObjectCategory::Column,
        raw.columns
            .iter()
            .filter(|column| matches!(column.relation_kind, 'r' | 'p' | 'f'))
            .count() as u64,
    );
    objects.insert(
        ObjectCategory::PrimaryKey,
        raw.constraints
            .iter()
            .filter(|constraint| constraint.kind == 'p')
            .count() as u64,
    );
    objects.insert(
        ObjectCategory::ForeignKey,
        raw.constraints
            .iter()
            .filter(|constraint| constraint.kind == 'f')
            .count() as u64,
    );
    objects.insert(
        ObjectCategory::UniqueConstraint,
        raw.constraints
            .iter()
            .filter(|constraint| constraint.kind == 'u')
            .count() as u64,
    );
    objects.insert(
        ObjectCategory::CheckConstraint,
        raw.constraints
            .iter()
            .filter(|constraint| constraint.kind == 'c')
            .count() as u64,
    );
    objects.insert(ObjectCategory::Index, raw.indexes.len() as u64);
    objects.insert(
        ObjectCategory::View,
        raw.relations
            .iter()
            .filter(|relation| relation.relkind == 'v')
            .count() as u64,
    );
    objects.insert(
        ObjectCategory::ViewColumn,
        raw.columns
            .iter()
            .filter(|column| matches!(column.relation_kind, 'v' | 'm'))
            .count() as u64,
    );
    objects.insert(ObjectCategory::Trigger, raw.triggers.len() as u64);
    objects.insert(ObjectCategory::Routine, raw.routines.len() as u64);
    objects.insert(
        ObjectCategory::MaterializedView,
        raw.relations
            .iter()
            .filter(|relation| relation.relkind == 'm')
            .count() as u64,
    );
    objects.insert(ObjectCategory::Sequence, raw.sequences.len() as u64);
    objects.insert(
        ObjectCategory::RoutineParameter,
        raw.routine_parameters.len() as u64,
    );
    objects.insert(
        ObjectCategory::UserDefinedType,
        raw.types
            .iter()
            .filter(|raw_type| raw_type.kind != 'd')
            .count() as u64,
    );
    objects.insert(
        ObjectCategory::Domain,
        raw.types
            .iter()
            .filter(|raw_type| raw_type.kind == 'd')
            .count() as u64,
    );
    objects.insert(ObjectCategory::EnumValue, raw.enum_values.len() as u64);
    objects.insert(
        ObjectCategory::ExclusionConstraint,
        raw.constraints
            .iter()
            .filter(|constraint| constraint.kind == 'x')
            .count() as u64,
    );
    objects.insert(ObjectCategory::Event, raw.event_triggers.len() as u64);
    objects.insert(ObjectCategory::Principal, raw.principals.len() as u64);
    objects.insert(ObjectCategory::Policy, raw.policies.len() as u64);
    objects.insert(
        ObjectCategory::Extension,
        (raw.extensions.len()
            + raw
                .columns
                .iter()
                .filter(|column| column.relation_kind == 'c')
                .count()
            + raw
                .yugabyte
                .as_ref()
                .map(|catalog| catalog.tablegroups.len() + catalog.tablespaces.len())
                .unwrap_or_default()) as u64,
    );

    let emitted_objects = emitted_object_counts(snapshot);
    for category in ObjectCategory::ALL {
        let discovered = objects.get(&category).copied().unwrap_or_default();
        let emitted = emitted_objects.get(&category).copied().unwrap_or_default();
        if discovered != emitted {
            return Err(CatalogError::Mapping(format!(
                "{} raw/emitted object count mismatch for {category:?}: discovered={discovered}, emitted={emitted}",
                raw.strategy.product_name()
            )));
        }
    }

    let mut relationships = emitted_relationship_counts(snapshot);
    relationships.insert(
        RelationshipCategory::DatabaseHasSchema,
        raw.schemas.len() as u64,
    );
    relationships.insert(
        RelationshipCategory::SchemaHasTable,
        raw.relations
            .iter()
            .filter(|relation| matches!(relation.relkind, 'r' | 'p' | 'f'))
            .count() as u64,
    );
    relationships.insert(
        RelationshipCategory::TableHasColumn,
        raw.columns
            .iter()
            .filter(|column| matches!(column.relation_kind, 'r' | 'p' | 'f'))
            .count() as u64,
    );
    relationships.insert(
        RelationshipCategory::TableHasConstraint,
        raw.constraints
            .iter()
            .filter(|constraint| constraint.relation_oid.is_some() && constraint.kind != 'x')
            .count() as u64,
    );
    relationships.insert(
        RelationshipCategory::ConstraintColumn,
        raw.constraints
            .iter()
            .filter(|constraint| {
                constraint.relation_oid.is_some() && matches!(constraint.kind, 'p' | 'u' | 'c')
            })
            .map(|constraint| constraint.columns.len() as u64)
            .sum(),
    );
    relationships.insert(
        RelationshipCategory::ForeignKeyColumnPair,
        raw.constraints
            .iter()
            .filter(|constraint| constraint.kind == 'f')
            .map(|constraint| constraint.columns.len() as u64)
            .sum(),
    );
    relationships.insert(
        RelationshipCategory::TableHasIndex,
        raw.indexes
            .iter()
            .filter(|index| {
                relation_kinds
                    .get(&index.relation_oid)
                    .map(|kind| matches!(kind, 'r' | 'p' | 'f'))
                    .unwrap_or(false)
            })
            .count() as u64,
    );
    let base_index_oids = raw
        .indexes
        .iter()
        .filter(|index| {
            relation_kinds
                .get(&index.relation_oid)
                .map(|kind| matches!(kind, 'r' | 'p' | 'f'))
                .unwrap_or(false)
        })
        .map(|index| index.oid)
        .collect::<BTreeSet<_>>();
    let unique_index_columns = raw
        .index_terms
        .iter()
        .filter(|term| {
            base_index_oids.contains(&term.index_oid) && term.is_key && term.column_number > 0
        })
        .map(|term| (term.index_oid, term.column_number))
        .collect::<BTreeSet<_>>();
    relationships.insert(
        RelationshipCategory::IndexColumn,
        unique_index_columns.len() as u64,
    );
    relationships.insert(
        RelationshipCategory::SchemaHasView,
        raw.relations
            .iter()
            .filter(|relation| relation.relkind == 'v')
            .count() as u64,
    );
    relationships.insert(
        RelationshipCategory::ViewDependency,
        raw.view_dependencies
            .iter()
            .filter(|dependency| {
                !is_system_schema(&dependency.target_schema)
                    && relation_kinds.get(&dependency.view_oid) == Some(&'v')
                    && relation_kinds
                        .get(&dependency.target_relation_oid)
                        .map(|kind| {
                            if dependency.target_column_number > 0 {
                                matches!(kind, 'r' | 'p' | 'f')
                            } else {
                                matches!(kind, 'r' | 'p' | 'f' | 'v')
                            }
                        })
                        .unwrap_or(false)
            })
            .map(|dependency| {
                (
                    dependency.view_oid,
                    dependency.target_relation_oid,
                    dependency.target_column_number,
                )
            })
            .collect::<BTreeSet<_>>()
            .len() as u64,
    );
    relationships.insert(
        RelationshipCategory::TriggerTarget,
        raw.triggers.len() as u64,
    );
    relationships.insert(
        RelationshipCategory::TriggerRoutine,
        raw.triggers.len() as u64,
    );
    relationships.insert(
        RelationshipCategory::SchemaHasRoutine,
        raw.routines.len() as u64,
    );
    relationships.insert(
        RelationshipCategory::RoutineDependency,
        raw.routine_dependencies
            .iter()
            .filter(|dependency| {
                !dependency
                    .target_schema
                    .as_deref()
                    .map(is_system_schema)
                    .unwrap_or(true)
            })
            .map(|dependency| {
                (
                    dependency.owner_oid,
                    dependency.target_class.clone(),
                    dependency.target_oid,
                    dependency.target_sub_id,
                )
            })
            .collect::<BTreeSet<_>>()
            .len() as u64,
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
                raw.strategy.product_name()
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
                            "{} pg_catalog raw object inventory for {category:?} in the declared schema scope",
                            raw.strategy.product_name()
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
                            "{} pg_catalog relationship ledger for {category:?} in the declared schema scope",
                            raw.strategy.product_name()
                        ),
                    },
                )
            })
            .collect(),
    })
}
