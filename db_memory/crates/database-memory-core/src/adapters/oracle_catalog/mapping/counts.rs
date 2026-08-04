fn discovery_counts_from_catalog(
    raw: &RawOracleCatalog,
    scope: &DictionaryScope,
) -> DiscoveryCounts {
    let object_evidence =
        "Oracle USER/DBA dictionary inventory after explicit application-scope filtering";
    let relationship_evidence =
        "Oracle USER/DBA dictionary parent and ordered-column reconciliation";
    let mut objects = ObjectCategory::ALL
        .into_iter()
        .map(|category| {
            (
                category,
                DiscoveredCount {
                    count: 0,
                    evidence: object_evidence.to_owned(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut relationships = RelationshipCategory::ALL
        .into_iter()
        .map(|category| {
            (
                category,
                DiscoveredCount {
                    count: 0,
                    evidence: relationship_evidence.to_owned(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let materialized_view_names = raw
        .materialized_views
        .iter()
        .map(|view| (view.owner.as_str(), view.name.as_str()))
        .collect::<BTreeSet<_>>();
    let base_table_count = raw
        .tables
        .iter()
        .filter(|table| {
            !materialized_view_names.contains(&(table.owner.as_str(), table.name.as_str()))
        })
        .count();
    let base_column_count = raw
        .columns
        .iter()
        .filter(|column| {
            !materialized_view_names.contains(&(column.owner.as_str(), column.table.as_str()))
        })
        .count();
    let materialized_view_column_count = raw.columns.len() - base_column_count;
    let base_constraint_count = raw
        .constraints
        .iter()
        .filter(|constraint| {
            !materialized_view_names
                .contains(&(constraint.owner.as_str(), constraint.table.as_str()))
        })
        .count();
    let materialized_view_constraint_count = raw.constraints.len() - base_constraint_count;
    let base_index_count = raw
        .indexes
        .iter()
        .filter(|index| {
            !materialized_view_names.contains(&(index.table_owner.as_str(), index.table.as_str()))
        })
        .count();
    let materialized_view_index_count = raw.indexes.len() - base_index_count;
    let materialized_view_dependency_count = raw
        .dependencies
        .iter()
        .filter(|dependency| dependency.object_type == "MATERIALIZED VIEW")
        .filter(|dependency| !dependency.referenced_owner_oracle_maintained)
        .filter(|dependency| {
            !(dependency.referenced_type == "TABLE"
                && dependency.owner == dependency.referenced_owner
                && dependency.name == dependency.referenced_name)
        })
        .count();
    let trigger_targets = raw
        .triggers
        .iter()
        .filter_map(|trigger| {
            if !matches!(trigger.base_object_type.as_str(), "TABLE" | "VIEW") {
                return None;
            }
            Some((
                (trigger.owner.as_str(), trigger.name.as_str()),
                (
                    trigger.table_owner.as_deref()?,
                    trigger.table_name.as_deref()?,
                    trigger.base_object_type.as_str(),
                ),
            ))
        })
        .collect::<BTreeMap<_, _>>();
    let trigger_dependency_count = raw
        .dependencies
        .iter()
        .filter(|dependency| dependency.object_type == "TRIGGER")
        .filter(|dependency| !dependency.referenced_owner_oracle_maintained)
        .filter(|dependency| {
            trigger_targets
                .get(&(dependency.owner.as_str(), dependency.name.as_str()))
                .is_none_or(|target| {
                    !(dependency.referenced_owner == target.0
                        && dependency.referenced_name == target.1
                        && dependency.referenced_type == target.2)
                })
        })
        .count();
    let routine_dependency_count = raw
        .dependencies
        .iter()
        .filter(|dependency| matches!(dependency.object_type.as_str(), "FUNCTION" | "PROCEDURE"))
        .filter(|dependency| !dependency.referenced_owner_oracle_maintained)
        .filter(|dependency| dependency.referenced_type != "TYPE")
        .count();
    let metadata_only_type_dependency_count = raw
        .dependencies
        .iter()
        .filter(|dependency| {
            matches!(
                dependency.object_type.as_str(),
                "VIEW" | "FUNCTION" | "PROCEDURE"
            ) && dependency.referenced_type == "TYPE"
        })
        .filter(|dependency| !dependency.referenced_owner_oracle_maintained)
        .count();
    let package_dependency_count = oracle_package_dependency_groups(&raw.dependencies).len();
    let type_dependency_count = oracle_type_dependency_groups(&raw.dependencies).len();
    let synonym_dependency_count = raw
        .dependencies
        .iter()
        .filter(|dependency| dependency.object_type == "SYNONYM")
        .filter(|dependency| !dependency.referenced_owner_oracle_maintained)
        .count();
    let type_reference_count = raw
        .type_attributes
        .iter()
        .filter(|attribute| attribute.data_type_owner.is_some())
        .count()
        + raw
            .collection_types
            .iter()
            .filter(|collection| collection.element_type_owner.is_some())
            .count()
        + raw
            .type_method_parameters
            .iter()
            .filter(|parameter| parameter.data_type_owner.is_some())
            .count()
        + raw
            .columns
            .iter()
            .filter(|column| column.data_type_owner.is_some())
            .count()
        + raw
            .view_columns
            .iter()
            .filter(|column| column.data_type_owner.is_some())
            .count()
        + raw
            .routine_arguments
            .iter()
            .filter(|argument| argument.type_owner.is_some())
            .count()
        + raw
            .package_arguments
            .iter()
            .filter(|argument| argument.type_owner.is_some())
            .count();
    let type_inheritance_count = raw
        .user_types
        .iter()
        .filter(|user_type| user_type.supertype_owner.is_some())
        .count();

    set_object_count(&mut objects, ObjectCategory::Database, 1);
    set_object_count(&mut objects, ObjectCategory::Schema, scope.owners.len());
    set_object_count(&mut objects, ObjectCategory::Table, base_table_count);
    set_object_count(&mut objects, ObjectCategory::Column, base_column_count);
    set_object_count(&mut objects, ObjectCategory::Index, raw.indexes.len());
    set_object_count(&mut objects, ObjectCategory::Sequence, raw.sequences.len());
    set_object_count(&mut objects, ObjectCategory::View, raw.views.len());
    set_object_count(&mut objects, ObjectCategory::Synonym, raw.synonyms.len());
    set_object_count(
        &mut objects,
        ObjectCategory::UserDefinedType,
        raw.user_types.len(),
    );
    set_object_count(
        &mut objects,
        ObjectCategory::Extension,
        raw.type_attributes.len()
            + raw.table_partitions.len()
            + raw.table_subpartitions.len()
            + raw.index_partitions.len()
            + raw.index_subpartitions.len()
            + raw.lobs.len()
            + raw.lob_partitions.len()
            + raw.lob_subpartitions.len(),
    );
    set_object_count(&mut objects, ObjectCategory::Trigger, raw.triggers.len());
    set_object_count(
        &mut objects,
        ObjectCategory::Routine,
        raw.routines.len() + raw.package_routines.len() + raw.type_methods.len(),
    );
    set_object_count(
        &mut objects,
        ObjectCategory::RoutineParameter,
        raw.routine_arguments.len()
            + raw.package_arguments.len()
            + raw.type_method_parameters.len(),
    );
    set_object_count(&mut objects, ObjectCategory::Package, raw.packages.len());
    set_object_count(
        &mut objects,
        ObjectCategory::ViewColumn,
        raw.view_columns.len() + materialized_view_column_count,
    );
    set_object_count(
        &mut objects,
        ObjectCategory::MaterializedView,
        raw.materialized_views.len(),
    );
    set_object_count(
        &mut objects,
        ObjectCategory::Principal,
        scope.principals.len(),
    );
    for constraint in &raw.constraints {
        let category = match constraint.constraint_type.as_str() {
            "P" => ObjectCategory::PrimaryKey,
            "R" => ObjectCategory::ForeignKey,
            "U" => ObjectCategory::UniqueConstraint,
            "C" => ObjectCategory::CheckConstraint,
            _ => continue,
        };
        objects.entry(category).and_modify(|count| count.count += 1);
    }

    set_relationship_count(
        &mut relationships,
        RelationshipCategory::DatabaseHasSchema,
        scope.owners.len(),
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::SchemaHasTable,
        base_table_count,
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::TableHasColumn,
        base_column_count,
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::TableHasConstraint,
        base_constraint_count,
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::ConstraintColumn,
        raw.constraints
            .iter()
            .filter(|constraint| {
                !materialized_view_names
                    .contains(&(constraint.owner.as_str(), constraint.table.as_str()))
            })
            .filter(|constraint| constraint.constraint_type != "R")
            .map(|constraint| constraint.columns.len())
            .sum(),
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::ForeignKeyColumnPair,
        raw.constraints
            .iter()
            .filter(|constraint| {
                !materialized_view_names
                    .contains(&(constraint.owner.as_str(), constraint.table.as_str()))
            })
            .filter(|constraint| constraint.constraint_type == "R")
            .map(|constraint| constraint.columns.len())
            .sum(),
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::TableHasIndex,
        base_index_count,
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::IndexColumn,
        raw.indexes
            .iter()
            .filter(|index| {
                !materialized_view_names
                    .contains(&(index.table_owner.as_str(), index.table.as_str()))
            })
            .map(|index| {
                index
                    .columns
                    .iter()
                    .filter(|column| column.expression.is_none())
                    .count()
            })
            .sum(),
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::SchemaHasView,
        raw.views.len(),
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::ViewDependency,
        raw.dependencies
            .iter()
            .filter(|dependency| dependency.object_type == "VIEW")
            .filter(|dependency| !dependency.referenced_owner_oracle_maintained)
            .filter(|dependency| dependency.referenced_type != "TYPE")
            .count(),
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::TriggerTarget,
        trigger_targets.len(),
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::SchemaHasRoutine,
        raw.routines.len(),
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::RoutineDependency,
        routine_dependency_count,
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::MetadataParent,
        scope.principals.len()
            + raw.sequences.len()
            + raw.synonyms.len()
            + raw.user_types.len()
            + raw.type_attributes.len()
            + raw.table_partitions.len()
            + raw.table_subpartitions.len()
            + raw.index_partitions.len()
            + raw.index_subpartitions.len()
            + raw.lobs.len()
            + raw.lob_partitions.len()
            + raw.lob_subpartitions.len()
            + raw.type_methods.len()
            + raw.type_method_parameters.len()
            + raw.view_columns.len()
            + raw.materialized_views.len()
            + materialized_view_column_count
            + materialized_view_constraint_count
            + materialized_view_index_count
            + raw.routine_arguments.len()
            + raw.packages.len()
            + raw.package_routines.len()
            + raw.package_arguments.len()
            + raw
                .triggers
                .iter()
                .filter(|trigger| {
                    matches!(trigger.base_object_type.as_str(), "SCHEMA" | "DATABASE")
                })
                .count(),
    );
    set_relationship_count(
        &mut relationships,
        RelationshipCategory::MetadataRelationship,
        raw.identity_columns.len()
            + materialized_view_dependency_count
            + trigger_dependency_count
            + synonym_dependency_count
            + type_dependency_count
            + type_reference_count
            + type_inheritance_count
            + metadata_only_type_dependency_count
            + raw.routine_arguments.len()
            + raw.package_arguments.len()
            + raw.type_method_parameters.len()
            + package_dependency_count
            + raw.lob_partitions.len()
            + raw.lob_subpartitions.len()
            + raw
                .constraints
                .iter()
                .filter(|constraint| {
                    materialized_view_names
                        .contains(&(constraint.owner.as_str(), constraint.table.as_str()))
                })
                .map(|constraint| constraint.columns.len())
                .sum::<usize>()
            + raw
                .indexes
                .iter()
                .filter(|index| {
                    materialized_view_names
                        .contains(&(index.table_owner.as_str(), index.table.as_str()))
                })
                .map(|index| {
                    index
                        .columns
                        .iter()
                        .filter(|column| column.expression.is_none())
                        .count()
                })
                .sum::<usize>(),
    );

    DiscoveryCounts {
        objects,
        relationships,
    }
}
