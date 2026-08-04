#[allow(clippy::too_many_arguments)]
fn map_synonym_targets(
    metadata: &mut CanonicalMetadata,
    database: &str,
    synonyms: &[RawSynonym],
    synonym_keys: &BTreeMap<i32, ObjectKey>,
    name_keys: &BTreeMap<(String, String), ObjectKey>,
    external_reference_keys: &mut BTreeMap<String, ObjectKey>,
    connection_alias: &str,
    database_key: &ObjectKey,
) -> Result<(), CatalogError> {
    for synonym in synonyms {
        let source = synonym_keys
            .get(&synonym.id)
            .ok_or_else(|| CatalogError::Mapping(format!("synonym {} lost its key", synonym.id)))?;
        let local_database = synonym
            .database
            .as_deref()
            .is_none_or(|target| target.eq_ignore_ascii_case(database));
        let local_server = synonym.server.is_none();
        let local_target = if local_database && local_server {
            synonym
                .target_schema
                .as_ref()
                .zip(synonym.target_entity.as_ref())
                .and_then(|(schema, entity)| name_keys.get(&(schema.clone(), entity.clone())))
                .cloned()
        } else {
            None
        };
        let target = match local_target {
            Some(target) => target,
            None => ensure_external_reference(
                metadata,
                external_reference_keys,
                connection_alias,
                database,
                database_key,
                &synonym.base_object_name,
                synonym.server.as_deref(),
                synonym.database.as_deref(),
                synonym.target_schema.as_deref(),
                synonym.target_entity.as_deref(),
                "synonym_target",
            )?,
        };
        add_relationship(
            metadata,
            MetadataRelationshipKind::SynonymFor,
            source,
            &target,
            None,
            BTreeMap::new(),
        );
    }
    Ok(())
}

#[derive(Default)]
struct DependencyMappingResult {
    view_dependencies: BTreeMap<i32, Vec<ObjectKey>>,
    routine_dependencies: BTreeMap<i32, Vec<ObjectKey>>,
    metadata_relationship_count: u64,
}

impl DependencyMappingResult {
    fn view_dependency_count(&self) -> u64 {
        self.view_dependencies
            .values()
            .map(|dependencies| dependencies.len() as u64)
            .sum()
    }

    fn routine_dependency_count(&self) -> u64 {
        self.routine_dependencies
            .values()
            .map(|dependencies| dependencies.len() as u64)
            .sum()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct SqlServerProjectionLedger {
    external_reference_objects: u64,
    view_dependencies: u64,
    routine_dependencies: u64,
    dependency_metadata_relationships: u64,
}

#[allow(clippy::too_many_arguments)]
fn map_dependencies(
    metadata: &mut CanonicalMetadata,
    dependencies: &[RawDependency],
    object_keys: &BTreeMap<i32, ObjectKey>,
    column_keys: &BTreeMap<(i32, i32), ObjectKey>,
    dependency_source_keys: &BTreeMap<i32, ObjectKey>,
    type_keys: &BTreeMap<i32, ObjectKey>,
    xml_collection_keys: &BTreeMap<i32, ObjectKey>,
    index_keys: &BTreeMap<(i32, i32), ObjectKey>,
    partition_function_keys: &BTreeMap<i32, ObjectKey>,
    name_keys: &BTreeMap<(String, String), ObjectKey>,
    external_reference_keys: &mut BTreeMap<String, ObjectKey>,
    connection_alias: &str,
    database: &str,
    database_key: &ObjectKey,
) -> Result<DependencyMappingResult, CatalogError> {
    let mut result = DependencyMappingResult::default();
    for dependency in dependencies {
        let source = if dependency.referencing_minor_id > 0 {
            column_keys
                .get(&(dependency.referencing_id, dependency.referencing_minor_id))
                .or_else(|| object_keys.get(&dependency.referencing_id))
                .cloned()
        } else {
            object_keys
                .get(&dependency.referencing_id)
                .or_else(|| dependency_source_keys.get(&dependency.referencing_id))
                .cloned()
        }
        .ok_or_else(|| {
            CatalogError::UnsupportedMetadata(format!(
                "dependency source class {} identity {}:{} has no canonical representation",
                dependency.referencing_class,
                dependency.referencing_id,
                dependency.referencing_minor_id
            ))
        })?;

        let target = resolve_dependency_target(
            metadata,
            dependency,
            object_keys,
            column_keys,
            type_keys,
            xml_collection_keys,
            index_keys,
            partition_function_keys,
            name_keys,
            external_reference_keys,
            connection_alias,
            database,
            database_key,
        )?;
        if source == target {
            continue;
        }
        match source.object_kind {
            ObjectKind::Column if target.object_kind == ObjectKind::Sequence => {
                add_relationship(
                    metadata,
                    MetadataRelationshipKind::UsesSequence,
                    &source,
                    &target,
                    None,
                    dependency_properties(dependency),
                );
                result.metadata_relationship_count += 1;
            }
            ObjectKind::View if is_legacy_schema_object_kind(target.object_kind) => {
                push_unique_dependency(
                    result
                        .view_dependencies
                        .entry(dependency.referencing_id)
                        .or_default(),
                    target,
                );
            }
            ObjectKind::Routine if is_legacy_schema_object_kind(target.object_kind) => {
                push_unique_dependency(
                    result
                        .routine_dependencies
                        .entry(dependency.referencing_id)
                        .or_default(),
                    target,
                );
            }
            ObjectKind::MaterializedView
                if matches!(target.object_kind, ObjectKind::Table | ObjectKind::View) =>
            {
                add_relationship(
                    metadata,
                    MetadataRelationshipKind::Materializes,
                    &source,
                    &target,
                    None,
                    dependency_properties(dependency),
                );
                result.metadata_relationship_count += 1;
            }
            ObjectKind::Trigger if target.object_kind == ObjectKind::Routine => {
                add_relationship(
                    metadata,
                    MetadataRelationshipKind::Invokes,
                    &source,
                    &target,
                    None,
                    dependency_properties(dependency),
                );
                result.metadata_relationship_count += 1;
            }
            _ => {
                add_relationship(
                    metadata,
                    MetadataRelationshipKind::DependsOn,
                    &source,
                    &target,
                    None,
                    dependency_properties(dependency),
                );
                result.metadata_relationship_count += 1;
            }
        }
    }
    for dependencies in result.view_dependencies.values_mut() {
        dependencies.sort_by_key(ObjectKey::to_string);
        dependencies.dedup();
    }
    for dependencies in result.routine_dependencies.values_mut() {
        dependencies.sort_by_key(ObjectKey::to_string);
        dependencies.dedup();
    }
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn resolve_dependency_target(
    metadata: &mut CanonicalMetadata,
    dependency: &RawDependency,
    object_keys: &BTreeMap<i32, ObjectKey>,
    column_keys: &BTreeMap<(i32, i32), ObjectKey>,
    type_keys: &BTreeMap<i32, ObjectKey>,
    xml_collection_keys: &BTreeMap<i32, ObjectKey>,
    index_keys: &BTreeMap<(i32, i32), ObjectKey>,
    partition_function_keys: &BTreeMap<i32, ObjectKey>,
    name_keys: &BTreeMap<(String, String), ObjectKey>,
    external_reference_keys: &mut BTreeMap<String, ObjectKey>,
    connection_alias: &str,
    database: &str,
    database_key: &ObjectKey,
) -> Result<ObjectKey, CatalogError> {
    let direct = match dependency.referenced_class {
        1 => dependency.referenced_id.and_then(|id| {
            if dependency.referenced_minor_id > 0 {
                column_keys
                    .get(&(id, dependency.referenced_minor_id))
                    .cloned()
            } else {
                object_keys.get(&id).cloned()
            }
        }),
        6 => dependency
            .referenced_id
            .and_then(|id| type_keys.get(&id).cloned()),
        7 => dependency.referenced_id.and_then(|object_id| {
            index_keys
                .get(&(object_id, dependency.referenced_minor_id))
                .cloned()
        }),
        10 => dependency
            .referenced_id
            .and_then(|id| xml_collection_keys.get(&id).cloned()),
        21 => dependency
            .referenced_id
            .and_then(|id| partition_function_keys.get(&id).cloned()),
        other => {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "dependency uses unsupported referenced class {other}"
            )))
        }
    };
    if let Some(key) = direct {
        return Ok(key);
    }

    let local_database = dependency
        .referenced_database
        .as_deref()
        .is_none_or(|name| name.eq_ignore_ascii_case(database));
    if dependency.referenced_server.is_none() && local_database {
        if let Some(key) = dependency
            .referenced_schema
            .as_ref()
            .and_then(|schema| {
                name_keys.get(&(schema.clone(), dependency.referenced_entity.clone()))
            })
            .cloned()
        {
            return Ok(key);
        }
    }
    let full_name = [
        dependency.referenced_server.as_deref(),
        dependency.referenced_database.as_deref(),
        dependency.referenced_schema.as_deref(),
        Some(dependency.referenced_entity.as_str()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(".");
    ensure_external_reference(
        metadata,
        external_reference_keys,
        connection_alias,
        database,
        database_key,
        &full_name,
        dependency.referenced_server.as_deref(),
        dependency.referenced_database.as_deref(),
        dependency.referenced_schema.as_deref(),
        Some(&dependency.referenced_entity),
        if dependency.referenced_id.is_some() {
            "unmodeled_local_reference"
        } else {
            "symbolic_reference"
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn ensure_external_reference(
    metadata: &mut CanonicalMetadata,
    keys: &mut BTreeMap<String, ObjectKey>,
    connection_alias: &str,
    database: &str,
    database_key: &ObjectKey,
    full_name: &str,
    server: Option<&str>,
    target_database: Option<&str>,
    schema: Option<&str>,
    entity: Option<&str>,
    reference_kind: &str,
) -> Result<ObjectKey, CatalogError> {
    if full_name.trim().is_empty() {
        return Err(CatalogError::Mapping(
            "external dependency has no stable name".to_owned(),
        ));
    }
    if let Some(key) = keys.get(full_name) {
        return Ok(key.clone());
    }
    let key = sqlserver_key(
        connection_alias,
        database,
        "external",
        ObjectKind::Extension,
        full_name,
        Some("reference".to_owned()),
    );
    let mut properties = BTreeMap::new();
    insert_string(&mut properties, "reference_kind", reference_kind);
    insert_optional_string(&mut properties, "server", server);
    insert_optional_string(&mut properties, "database", target_database);
    insert_optional_string(&mut properties, "schema", schema);
    insert_optional_string(&mut properties, "entity", entity);
    metadata.objects.push(MetadataObject {
        key: key.clone(),
        parent_key: Some(database_key.clone()),
        name: full_name.to_owned(),
        extension_kind: Some("sqlserver_external_reference".to_owned()),
        definition: None,
        properties,
    });
    keys.insert(full_name.to_owned(), key.clone());
    Ok(key)
}

fn dependency_properties(dependency: &RawDependency) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_i64(
        &mut properties,
        "referencing_class",
        i64::from(dependency.referencing_class),
    );
    insert_i64(
        &mut properties,
        "referenced_class",
        i64::from(dependency.referenced_class),
    );
    insert_i64(
        &mut properties,
        "referencing_minor_id",
        i64::from(dependency.referencing_minor_id),
    );
    insert_i64(
        &mut properties,
        "referenced_minor_id",
        i64::from(dependency.referenced_minor_id),
    );
    insert_bool(&mut properties, "schema_bound", dependency.schema_bound);
    properties
}

fn push_unique_dependency(dependencies: &mut Vec<ObjectKey>, key: ObjectKey) {
    if !dependencies.contains(&key) {
        dependencies.push(key);
    }
}

fn is_legacy_schema_object_kind(kind: ObjectKind) -> bool {
    matches!(
        kind,
        ObjectKind::Database
            | ObjectKind::Schema
            | ObjectKind::Table
            | ObjectKind::Column
            | ObjectKind::PrimaryKey
            | ObjectKind::ForeignKey
            | ObjectKind::UniqueConstraint
            | ObjectKind::CheckConstraint
            | ObjectKind::Index
            | ObjectKind::View
            | ObjectKind::Trigger
            | ObjectKind::Routine
    )
}
