impl RawPostgresCatalog {
    fn read(
        client: &mut impl GenericClient,
        request: &IntrospectionRequest,
        expected_product: PgWireProduct,
    ) -> Result<Self, CatalogError> {
        let server = read_server_facts(client)?;
        let strategy = PgCatalogStrategy::detect(expected_product, &server)?;
        let catalog_version = strategy.catalog_version();
        if !request.requested_catalogs.is_empty()
            && request.requested_catalogs != [server.database.clone()]
        {
            return Err(CatalogError::InvalidScope(format!(
                "this {} connection can certify only current database '{}', requested {:?}",
                strategy.product_name(),
                server.database,
                request.requested_catalogs
            )));
        }

        let available_schemas = read_schemas(client)?;
        let requested = request
            .requested_schemas
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let available_names = available_schemas
            .iter()
            .map(|schema| schema.name.clone())
            .collect::<BTreeSet<_>>();
        let missing = requested
            .difference(&available_names)
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(CatalogError::InvalidScope(format!(
                "requested {} schemas do not exist or are system schemas: {}",
                strategy.product_name(),
                missing.join(", ")
            )));
        }
        let schemas = if requested.is_empty() {
            available_schemas
        } else {
            available_schemas
                .into_iter()
                .filter(|schema| requested.contains(&schema.name))
                .collect()
        };
        if schemas.is_empty() {
            return Err(CatalogError::InvalidScope(format!(
                "{} scope contains no non-system schemas",
                strategy.product_name()
            )));
        }
        let inaccessible = schemas
            .iter()
            .filter(|schema| !schema.has_usage)
            .map(|schema| schema.name.clone())
            .collect::<Vec<_>>();
        if !inaccessible.is_empty() {
            return Err(CatalogError::PermissionDenied(format!(
                "current principal lacks USAGE on requested schema(s): {}",
                inaccessible.join(", ")
            )));
        }
        let schema_names = schemas
            .iter()
            .map(|schema| schema.name.clone())
            .collect::<Vec<_>>();

        reject_unsupported_relations(client, &schema_names, strategy.product_name())?;
        let yugabyte = match strategy {
            PgCatalogStrategy::YugabyteDb2025_2_3_2 => {
                Some(read_yugabyte_catalog(client, &schema_names)?)
            }
            PgCatalogStrategy::PostgreSql(_) => None,
        };
        let extension_routine_oids = read_extension_routine_oids(client)?;
        let routines = read_routines(client, &schema_names)?
            .into_iter()
            .filter(|routine| !extension_routine_oids.contains(&routine.oid))
            .collect();
        let routine_parameters = read_routine_parameters(client, &schema_names)?
            .into_iter()
            .filter(|parameter| !extension_routine_oids.contains(&parameter.routine_oid))
            .collect();
        let routine_dependencies = read_routine_dependencies(client, &schema_names)?
            .into_iter()
            .filter(|dependency| {
                !(extension_routine_oids.contains(&dependency.owner_oid)
                    || dependency.target_class == "routine"
                        && extension_routine_oids.contains(&dependency.target_oid))
            })
            .collect();

        Ok(Self {
            server,
            strategy,
            schemas,
            principals: read_principals(client)?,
            relations: read_relations(client, &schema_names)?,
            columns: read_columns(client, &schema_names, catalog_version)?,
            constraints: read_constraints(client, &schema_names)?,
            indexes: read_indexes(client, &schema_names)?,
            index_terms: read_index_terms(client, &schema_names)?,
            types: read_types(client, &schema_names)?,
            enum_values: read_enum_values(client, &schema_names)?,
            sequences: read_sequences(client, &schema_names)?,
            routines,
            routine_parameters,
            triggers: read_triggers(client, &schema_names)?,
            inheritance: read_inheritance(client, &schema_names)?,
            view_dependencies: read_view_dependencies(client, &schema_names)?,
            routine_dependencies,
            sequence_usages: read_sequence_usages(client, &schema_names)?,
            policies: read_policies(client, &schema_names)?,
            extensions: read_extensions(client)?,
            event_triggers: read_event_triggers(client)?,
            extension_routine_oids,
            yugabyte,
        })
    }
}

fn read_yugabyte_catalog(
    client: &mut impl GenericClient,
    schemas: &[String],
) -> Result<RawYugabyteCatalog, CatalogError> {
    let database = client.query_one(
        "
        SELECT pg_catalog.yb_is_database_colocated(),
               db.dattablespace::bigint
        FROM pg_catalog.pg_database db
        WHERE db.datname = pg_catalog.current_database()
        ",
        &[],
    )?;

    let relation_properties = client
        .query(
            "
            SELECT cls.oid::bigint,
                   cls.relkind::text,
                   cls.reltablespace::bigint,
                   properties.num_tablets,
                   properties.num_hash_key_columns,
                   properties.is_colocated,
                   properties.tablegroup_oid::bigint,
                   properties.colocation_id::bigint,
                   CASE WHEN properties.num_hash_key_columns = 0
                        THEN NULLIF(pg_catalog.yb_get_range_split_clause(cls.oid)::text, '')
                        ELSE NULL END
            FROM pg_catalog.pg_class cls
            JOIN pg_catalog.pg_namespace ns ON ns.oid = cls.relnamespace
            LEFT JOIN LATERAL pg_catalog.yb_table_properties(cls.oid) properties ON true
            WHERE ns.nspname = ANY($1::text[])
              AND cls.relkind IN ('r', 'p', 'f', 'm', 'S', 'i', 'I')
            ORDER BY ns.nspname, cls.relname, cls.oid
            ",
            &[&schemas],
        )?
        .into_iter()
        .map(|row| RawYugabyteRelationProperties {
            relation_oid: row.get(0),
            relation_kind: one_char(&row.get::<_, String>(1)),
            tablespace_oid: row.get(2),
            num_tablets: row.get(3),
            num_hash_key_columns: row.get(4),
            is_colocated: row.get(5),
            tablegroup_oid: row.get(6),
            colocation_id: row.get(7),
            range_split_clause: row.get(8),
        })
        .collect();

    let tablegroups = client
        .query(
            "
            SELECT grp.oid::bigint,
                   grp.grpname::text,
                   grp.grpowner::bigint,
                   grp.grptablespace::bigint,
                   ARRAY(
                       SELECT acl::text
                       FROM pg_catalog.unnest(grp.grpacl) acl
                   ),
                   COALESCE(grp.grpoptions, ARRAY[]::text[])
            FROM pg_catalog.pg_yb_tablegroup grp
            ORDER BY grp.grpname, grp.oid
            ",
            &[],
        )?
        .into_iter()
        .map(|row| RawYugabyteTablegroup {
            oid: row.get(0),
            name: row.get(1),
            owner_oid: row.get(2),
            tablespace_oid: row.get(3),
            acl: row.get(4),
            options: row.get(5),
        })
        .collect();

    let tablespaces = client
        .query(
            "
            SELECT spc.oid::bigint,
                   spc.spcname::text,
                   spc.spcowner::bigint,
                   ARRAY(
                       SELECT acl::text
                       FROM pg_catalog.unnest(spc.spcacl) acl
                   ),
                   COALESCE(spc.spcoptions, ARRAY[]::text[]),
                   pg_catalog.shobj_description(spc.oid, 'pg_tablespace')
            FROM pg_catalog.pg_tablespace spc
            ORDER BY spc.spcname, spc.oid
            ",
            &[],
        )?
        .into_iter()
        .map(|row| RawYugabyteTablespace {
            oid: row.get(0),
            name: row.get(1),
            owner_oid: row.get(2),
            acl: row.get(3),
            options: row.get(4),
            comment: row.get(5),
        })
        .collect();

    Ok(RawYugabyteCatalog {
        database_colocated: database.get(0),
        database_default_tablespace_oid: database.get(1),
        relation_properties,
        tablegroups,
        tablespaces,
    })
}
