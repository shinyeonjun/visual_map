async fn read_dependencies(
    client: &mut TdsClient,
    selected_schemas: &BTreeSet<String>,
) -> Result<Vec<RawDependency>, CatalogError> {
    rows(
        client,
        "
        SELECT sed.referencing_class,
               sed.referencing_id,
               sed.referencing_minor_id,
               sed.referenced_class,
               sed.referenced_server_name,
               sed.referenced_database_name,
               sed.referenced_schema_name,
               sed.referenced_entity_name,
               sed.referenced_id,
               sed.referenced_minor_id,
               sed.is_schema_bound_reference,
               sed.is_caller_dependent,
               sed.is_ambiguous,
               CASE
                   WHEN sed.referencing_class = 12 THEN N'__database__'
                   ELSE OBJECT_SCHEMA_NAME(sed.referencing_id)
               END
        FROM sys.sql_expression_dependencies sed
        ORDER BY sed.referencing_class,
                 sed.referencing_id,
                 sed.referencing_minor_id,
                 sed.referenced_class,
                 sed.referenced_server_name,
                 sed.referenced_database_name,
                 sed.referenced_schema_name,
                 sed.referenced_entity_name,
                 sed.referenced_minor_id
        ",
    )
    .await?
    .into_iter()
    .map(|row| {
        let dependency = RawDependency {
            referencing_class: i32::from(required_value::<u8>(&row, 0, "referencing class")?),
            referencing_id: required_value(&row, 1, "referencing id")?,
            referencing_minor_id: required_value(&row, 2, "referencing minor id")?,
            referenced_class: i32::from(required_value::<u8>(&row, 3, "referenced class")?),
            referenced_server: optional_string(&row, 4)?,
            referenced_database: optional_string(&row, 5)?,
            referenced_schema: optional_string(&row, 6)?,
            referenced_entity: required_string(&row, 7, "referenced entity")?,
            referenced_id: optional_value(&row, 8)?,
            referenced_minor_id: required_value(&row, 9, "referenced minor id")?,
            schema_bound: required_value(&row, 10, "schema-bound dependency flag")?,
            caller_dependent: required_value(&row, 11, "caller-dependent flag")?,
            ambiguous: required_value(&row, 12, "ambiguous dependency flag")?,
        };
        let source_schema = required_string(&row, 13, "dependency source schema")?;
        Ok((source_schema, dependency))
    })
    .collect::<Result<Vec<_>, CatalogError>>()
    .map(|dependencies| {
        dependencies
            .into_iter()
            .filter(|(schema, _)| schema == "__database__" || selected_schemas.contains(schema))
            .map(|(_, dependency)| dependency)
            .collect()
    })
}

async fn read_partition_functions(
    client: &mut TdsClient,
) -> Result<Vec<RawPartitionFunction>, CatalogError> {
    let mut functions = BTreeMap::<i32, RawPartitionFunction>::new();
    for row in rows(
        client,
        "
        SELECT function_id, name, fanout, boundary_value_on_right, is_system
        FROM sys.partition_functions
        ORDER BY function_id
        ",
    )
    .await?
    {
        let id = required_value(&row, 0, "partition function id")?;
        functions.insert(
            id,
            RawPartitionFunction {
                id,
                name: required_string(&row, 1, "partition function name")?,
                fanout: required_value(&row, 2, "partition function fanout")?,
                boundary_on_right: required_value(&row, 3, "partition boundary side")?,
                system: required_value(&row, 4, "system partition function flag")?,
                values: Vec::new(),
            },
        );
    }
    for row in rows(
        client,
        "
        SELECT function_id, boundary_id, CONVERT(nvarchar(4000), value)
        FROM sys.partition_range_values
        ORDER BY function_id, boundary_id
        ",
    )
    .await?
    {
        let function_id: i32 = required_value(&row, 0, "partition value function id")?;
        let function = functions.get_mut(&function_id).ok_or_else(|| {
            CatalogError::Mapping(format!(
                "partition range value references missing function {function_id}"
            ))
        })?;
        function.values.push(RawPartitionValue {
            boundary_id: required_value(&row, 1, "partition boundary id")?,
            value: optional_string(&row, 2)?,
        });
    }
    Ok(functions
        .into_values()
        .filter(|function| !function.system)
        .collect())
}

async fn read_partition_schemes(
    client: &mut TdsClient,
) -> Result<Vec<RawPartitionScheme>, CatalogError> {
    rows(
        client,
        "
        SELECT data_space_id, name, function_id
        FROM sys.partition_schemes
        ORDER BY data_space_id
        ",
    )
    .await?
    .into_iter()
    .map(|row| {
        Ok(RawPartitionScheme {
            id: required_value(&row, 0, "partition scheme id")?,
            name: required_string(&row, 1, "partition scheme name")?,
            function_id: required_value(&row, 2, "partition scheme function id")?,
        })
    })
    .collect()
}

async fn read_partitions(
    client: &mut TdsClient,
    strategy: SqlServerCatalogVersion,
    tables: &[RawTable],
    views: &[RawView],
) -> Result<Vec<RawPartition>, CatalogError> {
    let object_ids = tables
        .iter()
        .map(|table| table.id)
        .chain(views.iter().map(|view| view.id))
        .collect::<BTreeSet<_>>();
    let xml_column = strategy.xml_compression_expression();
    let sql = format!(
        "
        SELECT p.object_id,
               p.index_id,
               p.partition_number,
               p.data_compression_desc,
               {xml_column}
        FROM sys.partitions p
        JOIN sys.objects o ON o.object_id = p.object_id
        WHERE o.is_ms_shipped = 0
          AND o.type IN ('U', 'V')
        ORDER BY p.object_id, p.index_id, p.partition_number
        "
    );
    rows(client, &sql)
        .await?
        .into_iter()
        .map(|row| {
            Ok(RawPartition {
                object_id: required_value(&row, 0, "partition object id")?,
                index_id: required_value(&row, 1, "partition index id")?,
                partition_number: required_value(&row, 2, "partition number")?,
                data_compression: required_string(&row, 3, "partition compression")?,
                xml_compression: required_string(&row, 4, "partition XML compression")?,
            })
        })
        .collect::<Result<Vec<_>, CatalogError>>()
        .map(|partitions| {
            partitions
                .into_iter()
                .filter(|partition| object_ids.contains(&partition.object_id))
                .collect()
        })
}

async fn read_security_policies(
    client: &mut TdsClient,
    selected_schemas: &BTreeSet<String>,
) -> Result<Vec<RawSecurityPolicy>, CatalogError> {
    let mut policies = BTreeMap::<i32, RawSecurityPolicy>::new();
    for row in rows(
        client,
        "
        SELECT sp.object_id,
               s.name,
               sp.name,
               sp.principal_id,
               sp.is_enabled,
               sp.is_schema_bound
        FROM sys.security_policies sp
        JOIN sys.schemas s ON s.schema_id = sp.schema_id
        ORDER BY sp.object_id
        ",
    )
    .await?
    {
        let schema = required_string(&row, 1, "security policy schema")?;
        if !selected_schemas.contains(&schema) {
            continue;
        }
        let id = required_value(&row, 0, "security policy id")?;
        policies.insert(
            id,
            RawSecurityPolicy {
                id,
                schema,
                name: required_string(&row, 2, "security policy name")?,
                principal_id: optional_value(&row, 3)?,
                enabled: required_value(&row, 4, "security policy enabled flag")?,
                schema_bound: required_value(&row, 5, "security policy schema-bound flag")?,
                predicates: Vec::new(),
            },
        );
    }
    let predicate_sql = format!(
        "
        SELECT object_id,
               security_predicate_id,
               target_object_id,
               predicate_type_desc,
               operation_desc,
               CASE WHEN DATALENGTH(predicate_definition) <= {MAX_DEFINITION_BYTES} THEN predicate_definition END,
               CAST(COALESCE(DATALENGTH(predicate_definition), 0) AS int)
        FROM sys.security_predicates
        ORDER BY object_id, security_predicate_id
        "
    );
    for row in rows(client, &predicate_sql).await? {
        let policy_id: i32 = required_value(&row, 0, "predicate policy id")?;
        if let Some(policy) = policies.get_mut(&policy_id) {
            let id: i32 = required_value(&row, 1, "predicate id")?;
            let definition_bytes: i32 = required_value(&row, 6, "predicate definition bytes")?;
            ensure_definition_size("security predicate", &id.to_string(), definition_bytes)?;
            policy.predicates.push(RawSecurityPredicate {
                id,
                target_object_id: required_value(&row, 2, "predicate target")?,
                predicate_type: required_string(&row, 3, "predicate type")?,
                operation: optional_string(&row, 4)?,
                definition: required_string(&row, 5, "predicate definition")?,
                definition_bytes,
            });
        }
    }
    Ok(policies.into_values().collect())
}

async fn read_xml_schema_collections(
    client: &mut TdsClient,
    selected_schemas: &BTreeSet<String>,
) -> Result<Vec<RawXmlSchemaCollection>, CatalogError> {
    let mut collections = BTreeMap::<i32, RawXmlSchemaCollection>::new();
    for row in rows(
        client,
        "
        SELECT xsc.xml_collection_id,
               s.name,
               xsc.name,
               xsc.principal_id,
               CONVERT(nvarchar(33), xsc.create_date, 126),
               CONVERT(nvarchar(33), xsc.modify_date, 126),
               xsn.xml_namespace_id,
               xsn.name
        FROM sys.xml_schema_collections xsc
        JOIN sys.schemas s ON s.schema_id = xsc.schema_id
        LEFT JOIN sys.xml_schema_namespaces xsn
          ON xsn.xml_collection_id = xsc.xml_collection_id
        ORDER BY xsc.xml_collection_id, xsn.xml_namespace_id
        ",
    )
    .await?
    {
        let schema = required_string(&row, 1, "XML schema collection schema")?;
        if !selected_schemas.contains(&schema) {
            continue;
        }
        let id = required_value(&row, 0, "XML schema collection id")?;
        let name = required_string(&row, 2, "XML schema collection name")?;
        let principal_id = optional_value(&row, 3)?;
        let created_at = required_string(&row, 4, "XML schema collection creation time")?;
        let modified_at = required_string(&row, 5, "XML schema collection modification time")?;
        let entry = collections
            .entry(id)
            .or_insert_with(|| RawXmlSchemaCollection {
                id,
                schema: schema.clone(),
                name: name.clone(),
                principal_id,
                created_at: created_at.clone(),
                modified_at: modified_at.clone(),
                namespaces: Vec::new(),
            });
        if entry.schema != schema
            || entry.name != name
            || entry.principal_id != principal_id
            || entry.created_at != created_at
            || entry.modified_at != modified_at
        {
            return Err(CatalogError::Mapping(format!(
                "XML schema collection {id} has inconsistent catalog rows"
            )));
        }
        if let Some(namespace_id) = optional_value(&row, 6)? {
            let namespace_name = optional_value::<&str>(&row, 7)?
                .ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "XML schema collection {id} namespace {namespace_id} has no name"
                    ))
                })?
                .to_owned();
            if namespace_name.len() > MAX_PROPERTY_STRING_BYTES {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "XML namespace exceeds the {MAX_PROPERTY_STRING_BYTES}-byte property limit"
                )));
            }
            entry.namespaces.push(RawXmlSchemaNamespace {
                id: namespace_id,
                name: namespace_name,
            });
        }
    }
    Ok(collections.into_values().collect())
}

async fn read_extended_properties(
    client: &mut TdsClient,
) -> Result<Vec<RawExtendedProperty>, CatalogError> {
    rows(
        client,
        "
        SELECT ep.class,
               ep.class_desc,
               ep.major_id,
               ep.minor_id,
               ep.name,
               CONVERT(nvarchar(128), SQL_VARIANT_PROPERTY(ep.value, 'BaseType')),
               TRY_CONVERT(int, SQL_VARIANT_PROPERTY(ep.value, 'Precision')),
               TRY_CONVERT(int, SQL_VARIANT_PROPERTY(ep.value, 'Scale')),
               TRY_CONVERT(int, SQL_VARIANT_PROPERTY(ep.value, 'MaxLength')),
               CONVERT(nvarchar(128), SQL_VARIANT_PROPERTY(ep.value, 'Collation')),
               CASE
                 WHEN ep.value IS NULL THEN NULL
                 WHEN CONVERT(nvarchar(128), SQL_VARIANT_PROPERTY(ep.value, 'BaseType'))
                      IN (N'binary', N'varbinary')
                   THEN CONVERT(nvarchar(max), CONVERT(varchar(max), CONVERT(varbinary(8000), ep.value), 2))
                 ELSE CONVERT(nvarchar(max), ep.value, 126)
               END,
               CASE WHEN ep.value IS NULL THEN NULL
                    ELSE CONVERT(nvarchar(max), CONVERT(varchar(max), CONVERT(varbinary(8000), ep.value), 2))
               END
        FROM sys.extended_properties ep
        ORDER BY ep.class, ep.major_id, ep.minor_id, ep.name
        ",
    )
    .await?
    .into_iter()
    .map(|row| {
        Ok(RawExtendedProperty {
            class: required_value(&row, 0, "extended property class")?,
            class_description: required_string(&row, 1, "extended property class description")?,
            major_id: required_value(&row, 2, "extended property major id")?,
            minor_id: required_value(&row, 3, "extended property minor id")?,
            name: required_string(&row, 4, "extended property name")?,
            value_type: optional_string(&row, 5)?,
            value_precision: optional_value(&row, 6)?,
            value_scale: optional_value(&row, 7)?,
            value_max_length: optional_value(&row, 8)?,
            value_collation: optional_string(&row, 9)?,
            display_value: optional_string(&row, 10)?,
            value_hex: optional_string(&row, 11)?,
        })
    })
    .collect()
}

#[allow(clippy::too_many_arguments)]
fn select_extended_properties(
    properties: Vec<RawExtendedProperty>,
    schemas: &[RawSchema],
    principals: &[RawPrincipal],
    tables: &[RawTable],
    columns: &[RawColumn],
    constraints: &[RawConstraint],
    indexes: &[RawIndex],
    views: &[RawView],
    routines: &[RawRoutine],
    parameters: &[RawParameter],
    triggers: &[RawTrigger],
    user_types: &[RawUserType],
    sequences: &[RawSequence],
    synonyms: &[RawSynonym],
    partition_functions: &[RawPartitionFunction],
    partition_schemes: &[RawPartitionScheme],
    security_policies: &[RawSecurityPolicy],
    xml_schema_collections: &[RawXmlSchemaCollection],
) -> Vec<RawExtendedProperty> {
    let mut object_ids = tables
        .iter()
        .map(|object| object.id)
        .collect::<BTreeSet<_>>();
    object_ids.extend(views.iter().map(|object| object.id));
    object_ids.extend(routines.iter().map(|object| object.id));
    object_ids.extend(triggers.iter().map(|object| object.id));
    object_ids.extend(sequences.iter().map(|object| object.id));
    object_ids.extend(synonyms.iter().map(|object| object.id));
    object_ids.extend(security_policies.iter().map(|object| object.id));
    object_ids.extend(constraints.iter().map(|object| object.id));
    let column_ids = columns
        .iter()
        .map(|column| (column.object_id, column.id))
        .collect::<BTreeSet<_>>();
    let parameter_ids = parameters
        .iter()
        .map(|parameter| (parameter.object_id, parameter.id))
        .collect::<BTreeSet<_>>();
    let schema_ids = schemas
        .iter()
        .map(|schema| schema.id)
        .collect::<BTreeSet<_>>();
    let principal_ids = principals
        .iter()
        .map(|principal| principal.id)
        .collect::<BTreeSet<_>>();
    let type_ids = user_types
        .iter()
        .map(|data_type| data_type.id)
        .collect::<BTreeSet<_>>();
    let index_ids = indexes
        .iter()
        .map(|index| (index.object_id, index.id))
        .collect::<BTreeSet<_>>();
    let table_type_column_ids = user_types
        .iter()
        .filter_map(|data_type| {
            data_type
                .table_object_id
                .map(|object_id| (data_type.id, object_id))
        })
        .flat_map(|(user_type_id, object_id)| {
            columns.iter().filter_map(move |column| {
                (column.object_id == object_id).then_some((user_type_id, column.id))
            })
        })
        .collect::<BTreeSet<_>>();
    let xml_collection_ids = xml_schema_collections
        .iter()
        .map(|collection| collection.id)
        .collect::<BTreeSet<_>>();
    let partition_function_ids = partition_functions
        .iter()
        .map(|function| function.id)
        .collect::<BTreeSet<_>>();
    let partition_scheme_ids = partition_schemes
        .iter()
        .map(|scheme| scheme.id)
        .collect::<BTreeSet<_>>();

    properties
        .into_iter()
        .filter(|property| match property.class {
            0 => property.major_id == 0 && property.minor_id == 0,
            1 if property.minor_id == 0 => object_ids.contains(&property.major_id),
            1 => column_ids.contains(&(property.major_id, property.minor_id)),
            2 => parameter_ids.contains(&(property.major_id, property.minor_id)),
            3 => schema_ids.contains(&property.major_id),
            4 => principal_ids.contains(&property.major_id),
            6 => type_ids.contains(&property.major_id),
            7 => index_ids.contains(&(property.major_id, property.minor_id)),
            8 => table_type_column_ids.contains(&(property.major_id, property.minor_id)),
            10 => xml_collection_ids.contains(&property.major_id),
            20 => partition_scheme_ids.contains(&property.major_id),
            21 => partition_function_ids.contains(&property.major_id),
            _ => false,
        })
        .collect()
}

async fn read_unsupported_objects(
    client: &mut TdsClient,
    strategy: SqlServerCatalogVersion,
    selected_schemas: &BTreeSet<String>,
) -> Result<Vec<RawUnsupportedObject>, CatalogError> {
    let edge_constraints = strategy.edge_constraint_union();
    let sql = format!(
        "
        SELECT s.name, o.name, RTRIM(o.type), o.type_desc
        FROM sys.objects o
        LEFT JOIN sys.schemas s ON s.schema_id = o.schema_id
        WHERE o.is_ms_shipped = 0
          AND ((o.type = 'D' AND o.parent_object_id = 0) OR o.type IN ('R', 'TA'))
        UNION ALL
        SELECT s.name, et.name, N'ET', N'EXTERNAL_TABLE'
        FROM sys.external_tables et
        JOIN sys.schemas s ON s.schema_id = et.schema_id
        {edge_constraints}
        UNION ALL
        SELECT s.name, p.name, N'NP', N'NUMBERED_PROCEDURE'
        FROM sys.numbered_procedures np
        JOIN sys.procedures p ON p.object_id = np.object_id
        JOIN sys.schemas s ON s.schema_id = p.schema_id
        WHERE np.procedure_number > 1
        ORDER BY 1, 2, 3
        ",
    );
    rows(client, &sql)
        .await?
        .into_iter()
        .map(|row| {
            Ok(RawUnsupportedObject {
                schema: optional_string(&row, 0)?,
                name: required_string(&row, 1, "unsupported object name")?,
                type_code: required_string(&row, 2, "unsupported object type")?,
                type_desc: required_string(&row, 3, "unsupported object type description")?,
            })
        })
        .collect::<Result<Vec<_>, CatalogError>>()
        .map(|objects| {
            objects
                .into_iter()
                .filter(|object| {
                    object
                        .schema
                        .as_ref()
                        .is_none_or(|schema| selected_schemas.contains(schema))
                })
                .collect()
        })
}

