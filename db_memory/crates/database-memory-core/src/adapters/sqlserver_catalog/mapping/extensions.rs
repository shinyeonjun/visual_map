#[allow(clippy::too_many_arguments)]
fn map_xml_schema_collections(
    metadata: &mut CanonicalMetadata,
    connection_alias: &str,
    database: &str,
    collections: &[RawXmlSchemaCollection],
    schema_keys: &BTreeMap<String, ObjectKey>,
    schema_owner_ids: &BTreeMap<String, i32>,
    principal_keys: &BTreeMap<i32, ObjectKey>,
) -> Result<BTreeMap<i32, ObjectKey>, CatalogError> {
    let mut collection_keys = BTreeMap::<i32, ObjectKey>::new();
    for collection in collections {
        let schema_key = required_key(
            schema_keys,
            &collection.schema,
            "XML schema collection schema",
        )?;
        let key = sqlserver_key(
            connection_alias,
            database,
            &collection.schema,
            ObjectKind::Extension,
            &collection.name,
            Some("xml-schema-collection".to_owned()),
        );
        insert_unique_id(
            &mut collection_keys,
            collection.id,
            &key,
            "XML schema collection",
        )?;
        let mut properties = BTreeMap::new();
        insert_i64(
            &mut properties,
            "xml_collection_id",
            i64::from(collection.id),
        );
        insert_string(&mut properties, "created_at", &collection.created_at);
        insert_string(&mut properties, "modified_at", &collection.modified_at);
        metadata.objects.push(MetadataObject {
            key: key.clone(),
            parent_key: Some(schema_key.clone()),
            name: collection.name.clone(),
            extension_kind: Some("sqlserver_xml_schema_collection".to_owned()),
            definition: None,
            properties,
        });
        add_effective_owner(
            metadata,
            &key,
            collection.principal_id,
            &collection.schema,
            schema_owner_ids,
            principal_keys,
        )?;

        for namespace in &collection.namespaces {
            let namespace_key = sqlserver_key(
                connection_alias,
                database,
                &collection.schema,
                ObjectKind::Extension,
                &collection.name,
                Some(format!("xml-schema-namespace:{}", namespace.id)),
            );
            let mut namespace_properties = BTreeMap::new();
            insert_i64(
                &mut namespace_properties,
                "xml_namespace_id",
                i64::from(namespace.id),
            );
            insert_string(&mut namespace_properties, "namespace", &namespace.name);
            metadata.objects.push(MetadataObject {
                key: namespace_key,
                parent_key: Some(key.clone()),
                name: if namespace.name.is_empty() {
                    "default namespace".to_owned()
                } else {
                    namespace.name.clone()
                },
                extension_kind: Some("sqlserver_xml_schema_namespace".to_owned()),
                definition: None,
                properties: namespace_properties,
            });
        }
    }
    Ok(collection_keys)
}

struct ExtendedPropertyTargetRegistry<'a> {
    database: &'a ObjectKey,
    schemas: &'a BTreeMap<i32, ObjectKey>,
    principals: &'a BTreeMap<i32, ObjectKey>,
    objects: &'a BTreeMap<i32, ObjectKey>,
    columns: &'a BTreeMap<(i32, i32), ObjectKey>,
    table_type_columns: &'a BTreeMap<(i32, i32), ObjectKey>,
    parameters: &'a BTreeMap<(i32, i32), ObjectKey>,
    user_types: &'a BTreeMap<i32, ObjectKey>,
    indexes: &'a BTreeMap<(i32, i32), ObjectKey>,
    xml_collections: &'a BTreeMap<i32, ObjectKey>,
    partition_schemes: &'a BTreeMap<i32, ObjectKey>,
    partition_functions: &'a BTreeMap<i32, ObjectKey>,
}

impl ExtendedPropertyTargetRegistry<'_> {
    fn resolve(&self, property: &RawExtendedProperty) -> Option<&ObjectKey> {
        match property.class {
            0 if property.major_id == 0 && property.minor_id == 0 => Some(self.database),
            1 if property.minor_id == 0 => self.objects.get(&property.major_id),
            1 => self.columns.get(&(property.major_id, property.minor_id)),
            2 => self.parameters.get(&(property.major_id, property.minor_id)),
            3 => self.schemas.get(&property.major_id),
            4 => self.principals.get(&property.major_id),
            6 => self.user_types.get(&property.major_id),
            7 => self.indexes.get(&(property.major_id, property.minor_id)),
            8 => self
                .table_type_columns
                .get(&(property.major_id, property.minor_id)),
            10 => self.xml_collections.get(&property.major_id),
            20 => self.partition_schemes.get(&property.major_id),
            21 => self.partition_functions.get(&property.major_id),
            _ => None,
        }
    }
}

fn map_extended_properties(
    metadata: &mut CanonicalMetadata,
    connection_alias: &str,
    database: &str,
    properties: &[RawExtendedProperty],
    targets: ExtendedPropertyTargetRegistry<'_>,
) -> Result<(), CatalogError> {
    for property in properties {
        let target = targets.resolve(property).ok_or_else(|| {
            CatalogError::Mapping(format!(
                "extended property '{}:{}:{}:{}' references an unmapped target",
                property.class, property.major_id, property.minor_id, property.name
            ))
        })?;
        let key = sqlserver_key(
            connection_alias,
            database,
            &target.schema,
            ObjectKind::Extension,
            &target.object_name,
            Some(format!(
                "extended-property:{}:{}:{}:{}",
                property.class, property.major_id, property.minor_id, property.name
            )),
        );
        let mut values = BTreeMap::new();
        insert_i64(&mut values, "class", i64::from(property.class));
        insert_string(
            &mut values,
            "class_description",
            &property.class_description,
        );
        insert_i64(&mut values, "major_id", i64::from(property.major_id));
        insert_i64(&mut values, "minor_id", i64::from(property.minor_id));
        insert_bool(&mut values, "value_is_null", property.value_type.is_none());
        insert_optional_string(&mut values, "value_type", property.value_type.as_deref());
        insert_optional_i64(
            &mut values,
            "value_precision",
            property.value_precision.map(i64::from),
        );
        insert_optional_i64(
            &mut values,
            "value_scale",
            property.value_scale.map(i64::from),
        );
        insert_optional_i64(
            &mut values,
            "value_max_length",
            property.value_max_length.map(i64::from),
        );
        insert_optional_string(
            &mut values,
            "value_collation",
            property.value_collation.as_deref(),
        );
        insert_optional_string(
            &mut values,
            "display_value",
            property.display_value.as_deref(),
        );
        insert_optional_string(&mut values, "value_hex", property.value_hex.as_deref());
        metadata.objects.push(MetadataObject {
            key,
            parent_key: Some(target.clone()),
            name: property.name.clone(),
            extension_kind: Some("sqlserver_extended_property".to_owned()),
            definition: None,
            properties: values,
        });
    }
    Ok(())
}

struct PartitionMappingResult {
    function_keys: BTreeMap<i32, ObjectKey>,
    scheme_keys: BTreeMap<i32, ObjectKey>,
}

fn map_partitions(
    metadata: &mut CanonicalMetadata,
    connection_alias: &str,
    database: &str,
    database_key: &ObjectKey,
    raw: &RawSqlServerCatalog,
    object_keys: &BTreeMap<i32, ObjectKey>,
    index_keys: &BTreeMap<(i32, i32), ObjectKey>,
) -> Result<PartitionMappingResult, CatalogError> {
    let mut function_keys = BTreeMap::<i32, ObjectKey>::new();
    for function in &raw.partition_functions {
        let key = sqlserver_key(
            connection_alias,
            database,
            database,
            ObjectKind::Extension,
            &function.name,
            Some(format!("partition-function:{}", function.id)),
        );
        if function_keys.insert(function.id, key.clone()).is_some() {
            return Err(CatalogError::Mapping(format!(
                "duplicate partition function id {}",
                function.id
            )));
        }
        let mut properties = BTreeMap::new();
        insert_i64(&mut properties, "fanout", i64::from(function.fanout));
        insert_bool(
            &mut properties,
            "boundary_on_right",
            function.boundary_on_right,
        );
        metadata.objects.push(MetadataObject {
            key: key.clone(),
            parent_key: Some(database_key.clone()),
            name: function.name.clone(),
            extension_kind: Some("sqlserver_partition_function".to_owned()),
            definition: None,
            properties,
        });
        for value in &function.values {
            let value_key = sqlserver_key(
                connection_alias,
                database,
                database,
                ObjectKind::Extension,
                &function.name,
                Some(format!("partition-boundary:{}", value.boundary_id)),
            );
            let mut properties = BTreeMap::new();
            insert_i64(&mut properties, "boundary_id", i64::from(value.boundary_id));
            insert_optional_string(&mut properties, "value", value.value.as_deref());
            metadata.objects.push(MetadataObject {
                key: value_key,
                parent_key: Some(key.clone()),
                name: format!("{} boundary {}", function.name, value.boundary_id),
                extension_kind: Some("sqlserver_partition_boundary".to_owned()),
                definition: None,
                properties,
            });
        }
    }

    let mut scheme_keys = BTreeMap::<i32, ObjectKey>::new();
    for scheme in &raw.partition_schemes {
        let function_key = function_keys.get(&scheme.function_id).ok_or_else(|| {
            CatalogError::Mapping(format!(
                "partition scheme '{}' references missing function {}",
                scheme.name, scheme.function_id
            ))
        })?;
        let key = sqlserver_key(
            connection_alias,
            database,
            database,
            ObjectKind::Extension,
            &scheme.name,
            Some(format!("partition-scheme:{}", scheme.id)),
        );
        if scheme_keys.insert(scheme.id, key.clone()).is_some() {
            return Err(CatalogError::Mapping(format!(
                "duplicate partition scheme id {}",
                scheme.id
            )));
        }
        metadata.objects.push(MetadataObject {
            key: key.clone(),
            parent_key: Some(database_key.clone()),
            name: scheme.name.clone(),
            extension_kind: Some("sqlserver_partition_scheme".to_owned()),
            definition: None,
            properties: BTreeMap::new(),
        });
        add_relationship(
            metadata,
            MetadataRelationshipKind::Extension("partition_scheme_uses_function".to_owned()),
            &key,
            function_key,
            None,
            BTreeMap::new(),
        );
    }

    let index_data_spaces = raw
        .indexes
        .iter()
        .map(|index| ((index.object_id, index.id), index.data_space_id))
        .collect::<BTreeMap<_, _>>();
    for partition in &raw.partitions {
        let parent_key = if partition.index_id == 0 {
            object_keys.get(&partition.object_id)
        } else {
            index_keys.get(&(partition.object_id, partition.index_id))
        }
        .cloned()
        .ok_or_else(|| {
            CatalogError::Mapping(format!(
                "partition {}:{}:{} references missing parent",
                partition.object_id, partition.index_id, partition.partition_number
            ))
        })?;
        let key = sqlserver_key(
            connection_alias,
            database,
            &parent_key.schema,
            ObjectKind::Extension,
            &parent_key.object_name,
            Some(format!(
                "partition:{}:{}",
                partition.index_id, partition.partition_number
            )),
        );
        let mut properties = BTreeMap::new();
        insert_i64(&mut properties, "index_id", i64::from(partition.index_id));
        insert_i64(
            &mut properties,
            "partition_number",
            i64::from(partition.partition_number),
        );
        insert_string(
            &mut properties,
            "data_compression",
            &partition.data_compression,
        );
        insert_string(
            &mut properties,
            "xml_compression",
            &partition.xml_compression,
        );
        metadata.objects.push(MetadataObject {
            key: key.clone(),
            parent_key: Some(parent_key),
            name: format!("partition {}", partition.partition_number),
            extension_kind: Some("sqlserver_partition".to_owned()),
            definition: None,
            properties,
        });
        if let Some(data_space_id) = index_data_spaces
            .get(&(partition.object_id, partition.index_id))
            .and_then(|id| scheme_keys.get(id))
        {
            add_relationship(
                metadata,
                MetadataRelationshipKind::Extension("partition_uses_scheme".to_owned()),
                &key,
                data_space_id,
                None,
                BTreeMap::new(),
            );
        }
    }

    for table in &raw.tables {
        let (Some(history_schema), Some(history_table)) =
            (table.history_schema.as_ref(), table.history_table.as_ref())
        else {
            continue;
        };
        let source = object_keys.get(&table.id).ok_or_else(|| {
            CatalogError::Mapping(format!("temporal table {} lost its key", table.id))
        })?;
        let target = raw
            .tables
            .iter()
            .find(|candidate| {
                &candidate.schema == history_schema && &candidate.name == history_table
            })
            .and_then(|candidate| object_keys.get(&candidate.id))
            .ok_or_else(|| {
                CatalogError::InvalidScope(format!(
                    "temporal table '{}.{}' history table '{}.{}' is outside the selected schema scope",
                    table.schema, table.name, history_schema, history_table
                ))
            })?;
        add_relationship(
            metadata,
            MetadataRelationshipKind::Extension("temporal_history_table".to_owned()),
            source,
            target,
            None,
            BTreeMap::new(),
        );
    }
    Ok(PartitionMappingResult {
        function_keys,
        scheme_keys,
    })
}
