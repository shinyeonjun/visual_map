impl RawSqlServerCatalog {
    async fn read(
        client: &mut TdsClient,
        strategy: SqlServerCatalogVersion,
        selected_schemas: &BTreeSet<String>,
    ) -> Result<Self, CatalogError> {
        let schemas = read_schemas(client)
            .await?
            .into_iter()
            .filter(|schema| selected_schemas.contains(&schema.name))
            .collect::<Vec<_>>();
        let principals = read_principals(client).await?;
        let tables = read_tables(client, strategy, selected_schemas).await?;
        let columns = read_columns(client, selected_schemas).await?;
        let constraints = read_constraints(client, selected_schemas).await?;
        let indexes = read_indexes(client, selected_schemas).await?;
        let views = read_views(client, selected_schemas).await?;
        let routines = read_routines(client, strategy, selected_schemas).await?;
        let parameters = read_parameters(client, &routines).await?;
        let triggers = read_triggers(client, selected_schemas).await?;
        let user_types = read_user_types(client, selected_schemas).await?;
        let sequences = read_sequences(client, selected_schemas).await?;
        let synonyms = read_synonyms(client, selected_schemas).await?;
        let dependencies = read_dependencies(client, selected_schemas).await?;
        let partition_functions = read_partition_functions(client).await?;
        let partition_schemes = read_partition_schemes(client).await?;
        let partitions = read_partitions(client, strategy, &tables, &views).await?;
        let security_policies = read_security_policies(client, selected_schemas).await?;
        let xml_schema_collections = read_xml_schema_collections(client, selected_schemas).await?;
        let extended_properties = select_extended_properties(
            read_extended_properties(client).await?,
            &schemas,
            &principals,
            &tables,
            &columns,
            &constraints,
            &indexes,
            &views,
            &routines,
            &parameters,
            &triggers,
            &user_types,
            &sequences,
            &synonyms,
            &partition_functions,
            &partition_schemes,
            &security_policies,
            &xml_schema_collections,
        );
        let unsupported = read_unsupported_objects(client, strategy, selected_schemas).await?;
        validate_supported_metadata(
            &unsupported,
            &views,
            &routines,
            &triggers,
            &user_types,
            &dependencies,
        )?;
        Ok(Self {
            schemas,
            principals,
            tables,
            columns,
            constraints,
            indexes,
            views,
            routines,
            parameters,
            triggers,
            user_types,
            sequences,
            synonyms,
            dependencies,
            partition_functions,
            partition_schemes,
            partitions,
            security_policies,
            xml_schema_collections,
            extended_properties,
        })
    }
}

