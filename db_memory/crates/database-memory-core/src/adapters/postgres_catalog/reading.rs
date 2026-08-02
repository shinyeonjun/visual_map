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

fn read_server_facts(client: &mut impl GenericClient) -> Result<ServerFacts, CatalogError> {
    let row = client.query_one(
        "
        SELECT current_database()::text,
               current_setting('server_version'),
               current_setting('server_version_num')::integer,
               current_user::text,
               session_user::text,
               current_setting('transaction_read_only') = 'on',
               current_setting('transaction_isolation'),
               COALESCE((SELECT ssl FROM pg_catalog.pg_stat_ssl WHERE pid = pg_catalog.pg_backend_pid()), false),
               (SELECT version FROM pg_catalog.pg_stat_ssl WHERE pid = pg_catalog.pg_backend_pid()),
               (SELECT cipher FROM pg_catalog.pg_stat_ssl WHERE pid = pg_catalog.pg_backend_pid()),
               pg_catalog.version()
        ",
        &[],
    )?;
    Ok(ServerFacts {
        database: row.get(0),
        version: row.get(1),
        version_num: row.get(2),
        current_user: row.get(3),
        session_user: row.get(4),
        transaction_read_only: row.get(5),
        transaction_isolation: row.get(6),
        tls: row.get(7),
        tls_version: row.get(8),
        tls_cipher: row.get(9),
        version_banner: row.get(10),
    })
}

fn read_schemas(client: &mut impl GenericClient) -> Result<Vec<RawSchema>, CatalogError> {
    Ok(client
        .query(
            "
            SELECT ns.oid::bigint,
                   ns.nspname,
                   ns.nspowner::bigint,
                   pg_catalog.has_schema_privilege(ns.oid, 'USAGE'),
                   pg_catalog.obj_description(ns.oid, 'pg_namespace')
            FROM pg_catalog.pg_namespace ns
            WHERE ns.nspname <> 'information_schema'
              AND ns.nspname NOT LIKE 'pg\\_%' ESCAPE '\\'
            ORDER BY ns.nspname
            ",
            &[],
        )?
        .into_iter()
        .map(|row| RawSchema {
            oid: row.get(0),
            name: row.get(1),
            owner_oid: row.get(2),
            has_usage: row.get(3),
            comment: row.get(4),
        })
        .collect())
}

fn read_principals(client: &mut impl GenericClient) -> Result<Vec<RawPrincipal>, CatalogError> {
    Ok(client
        .query(
            "
            SELECT oid::bigint,
                   rolname,
                   rolsuper,
                   rolinherit,
                   rolcreaterole,
                   rolcreatedb,
                   rolcanlogin,
                   rolreplication,
                   rolbypassrls,
                   rolvaliduntil::text
            FROM pg_catalog.pg_roles
            ORDER BY rolname
            ",
            &[],
        )?
        .into_iter()
        .map(|row| RawPrincipal {
            oid: row.get(0),
            name: row.get(1),
            superuser: row.get(2),
            inherit: row.get(3),
            create_role: row.get(4),
            create_database: row.get(5),
            can_login: row.get(6),
            replication: row.get(7),
            bypass_rls: row.get(8),
            valid_until: row.get(9),
        })
        .collect())
}

fn reject_unsupported_relations(
    client: &mut impl GenericClient,
    schemas: &[String],
    product_name: &str,
) -> Result<(), CatalogError> {
    let rows = client.query(
        "
        SELECT ns.nspname, cls.relname, cls.relkind::text
        FROM pg_catalog.pg_class cls
        JOIN pg_catalog.pg_namespace ns ON ns.oid = cls.relnamespace
        WHERE ns.nspname = ANY($1::text[])
          AND cls.relkind NOT IN ('r', 'p', 'f', 'v', 'm', 'S', 'c', 'i', 'I')
        ORDER BY ns.nspname, cls.relname
        ",
        &[&schemas],
    )?;
    if let Some(row) = rows.first() {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "unsupported {product_name} pg_catalog relation kind '{}' discovered at {}.{}",
            row.get::<_, String>(2),
            row.get::<_, String>(0),
            row.get::<_, String>(1)
        )));
    }
    Ok(())
}

fn read_relations(
    client: &mut impl GenericClient,
    schemas: &[String],
) -> Result<Vec<RawRelation>, CatalogError> {
    Ok(client
        .query(
            "
            SELECT cls.oid::bigint,
                   cls.reltype::bigint,
                   ns.nspname,
                   cls.relname,
                   cls.relkind::text,
                   cls.relpersistence::text,
                   cls.relowner::bigint,
                   cls.relispartition,
                   cls.relrowsecurity,
                   cls.relforcerowsecurity,
                   cls.relreplident::text,
                   CASE WHEN cls.relispartition
                        THEN pg_catalog.pg_get_expr(cls.relpartbound, cls.oid, true)
                        ELSE NULL END,
                   CASE WHEN cls.relkind IN ('v', 'm')
                             AND pg_catalog.octet_length(pg_catalog.pg_get_viewdef(cls.oid, true)) <= $2
                        THEN pg_catalog.pg_get_viewdef(cls.oid, true)
                        ELSE NULL END,
                   CASE WHEN cls.relkind IN ('v', 'm')
                        THEN pg_catalog.octet_length(pg_catalog.pg_get_viewdef(cls.oid, true)) > $2
                        ELSE false END,
                   pg_catalog.obj_description(cls.oid, 'pg_class')
            FROM pg_catalog.pg_class cls
            JOIN pg_catalog.pg_namespace ns ON ns.oid = cls.relnamespace
            WHERE ns.nspname = ANY($1::text[])
              AND cls.relkind IN ('r', 'p', 'f', 'v', 'm', 'S', 'c')
            ORDER BY ns.nspname, cls.relname, cls.oid
            ",
            &[&schemas, &MAX_DEFINITION_BYTES],
        )?
        .into_iter()
        .map(|row| RawRelation {
            oid: row.get(0),
            row_type_oid: row.get(1),
            schema: row.get(2),
            name: row.get(3),
            relkind: one_char(&row.get::<_, String>(4)),
            persistence: one_char(&row.get::<_, String>(5)),
            owner_oid: row.get(6),
            is_partition: row.get(7),
            row_security: row.get(8),
            force_row_security: row.get(9),
            replica_identity: one_char(&row.get::<_, String>(10)),
            partition_bound: row.get(11),
            definition: row.get(12),
            definition_too_large: row.get(13),
            comment: row.get(14),
        })
        .collect())
}

fn read_columns(
    client: &mut impl GenericClient,
    schemas: &[String],
    catalog_version: PostgresCatalogVersion,
) -> Result<Vec<RawColumn>, CatalogError> {
    client
        .query(
            "
            SELECT cls.oid::bigint,
                   cls.relkind::text,
                   ns.nspname,
                   cls.relname,
                   att.attnum,
                   att.attname,
                   att.atttypid::bigint,
                   type_ns.nspname,
                   pg_catalog.format_type(att.atttypid, att.atttypmod),
                   NOT att.attnotnull,
                   def.oid::bigint,
                   CASE WHEN def.oid IS NOT NULL
                             AND pg_catalog.octet_length(pg_catalog.pg_get_expr(def.adbin, def.adrelid, true)) <= $2
                        THEN pg_catalog.pg_get_expr(def.adbin, def.adrelid, true)
                        ELSE NULL END,
                   CASE WHEN def.oid IS NOT NULL
                        THEN pg_catalog.octet_length(pg_catalog.pg_get_expr(def.adbin, def.adrelid, true)) > $2
                        ELSE false END,
                   att.attgenerated::text,
                   att.attidentity::text,
                   CASE WHEN att.attcollation = 0 THEN NULL
                        ELSE coll_ns.nspname || '.' || coll.collname END,
                   NULLIF(pg_catalog.to_jsonb(att)->>'attcompression', ''),
                   att.attstattarget::integer,
                   pg_catalog.col_description(att.attrelid, att.attnum)
            FROM pg_catalog.pg_attribute att
            JOIN pg_catalog.pg_class cls ON cls.oid = att.attrelid
            JOIN pg_catalog.pg_namespace ns ON ns.oid = cls.relnamespace
            LEFT JOIN pg_catalog.pg_attrdef def
              ON def.adrelid = att.attrelid AND def.adnum = att.attnum
            JOIN pg_catalog.pg_type data_type ON data_type.oid = att.atttypid
            JOIN pg_catalog.pg_namespace type_ns ON type_ns.oid = data_type.typnamespace
            LEFT JOIN pg_catalog.pg_collation coll ON coll.oid = att.attcollation
            LEFT JOIN pg_catalog.pg_namespace coll_ns ON coll_ns.oid = coll.collnamespace
            WHERE ns.nspname = ANY($1::text[])
              AND cls.relkind IN ('r', 'p', 'f', 'v', 'm', 'c')
              AND att.attnum > 0
              AND NOT att.attisdropped
            ORDER BY ns.nspname, cls.relname, att.attnum
            ",
            &[&schemas, &MAX_DEFINITION_BYTES],
        )?
        .into_iter()
        .map(|row| {
            let raw_statistics_target = row.try_get(17)?;
            Ok(RawColumn {
                relation_oid: row.get(0),
                relation_kind: one_char(&row.get::<_, String>(1)),
                schema: row.get(2),
                relation: row.get(3),
                attnum: row.get(4),
                name: row.get(5),
                type_oid: row.get(6),
                type_schema: row.get(7),
                data_type: row.get(8),
                nullable: row.get(9),
                default_oid: row.get(10),
                default_expression: row.get(11),
                default_too_large: row.get(12),
                generated: one_char(&row.get::<_, String>(13)),
                identity: one_char(&row.get::<_, String>(14)),
                collation: row.get(15),
                compression: row.get(16),
                statistics_target: catalog_version.statistics_target(raw_statistics_target)?,
                comment: row.get(18),
            })
        })
        .collect()
}

fn one_char(value: &str) -> char {
    value.chars().next().unwrap_or('\0')
}

fn read_constraints(
    client: &mut impl GenericClient,
    schemas: &[String],
) -> Result<Vec<RawConstraint>, CatalogError> {
    Ok(client
        .query(
            "
            SELECT con.oid::bigint,
                   ns.nspname,
                   NULLIF(con.conrelid, 0)::bigint,
                   NULLIF(con.contypid, 0)::bigint,
                   con.conname,
                   con.contype::text,
                   COALESCE(con.conkey, ARRAY[]::smallint[]),
                   NULLIF(con.confrelid, 0)::bigint,
                   COALESCE(con.confkey, ARRAY[]::smallint[]),
                   CASE WHEN pg_catalog.octet_length(pg_catalog.pg_get_constraintdef(con.oid, true)) <= $2
                        THEN pg_catalog.pg_get_constraintdef(con.oid, true)
                        ELSE NULL END,
                   pg_catalog.octet_length(pg_catalog.pg_get_constraintdef(con.oid, true)) > $2,
                   con.condeferrable,
                   con.condeferred,
                   con.convalidated,
                   con.connoinherit,
                   con.confdeltype::text,
                   con.confupdtype::text,
                   con.confmatchtype::text
            FROM pg_catalog.pg_constraint con
            LEFT JOIN pg_catalog.pg_class rel ON rel.oid = con.conrelid
            LEFT JOIN pg_catalog.pg_type typ ON typ.oid = con.contypid
            JOIN pg_catalog.pg_namespace ns
              ON ns.oid = COALESCE(rel.relnamespace, typ.typnamespace)
            WHERE ns.nspname = ANY($1::text[])
              AND con.contype IN ('p', 'u', 'f', 'c', 'x')
            ORDER BY ns.nspname, COALESCE(rel.relname, typ.typname), con.conname, con.oid
            ",
            &[&schemas, &MAX_DEFINITION_BYTES],
        )?
        .into_iter()
        .map(|row| RawConstraint {
            oid: row.get(0),
            schema: row.get(1),
            relation_oid: row.get(2),
            domain_type_oid: row.get(3),
            name: row.get(4),
            kind: one_char(&row.get::<_, String>(5)),
            columns: row.get(6),
            referenced_relation_oid: row.get(7),
            referenced_columns: row.get(8),
            definition: row.get(9),
            definition_too_large: row.get(10),
            deferrable: row.get(11),
            initially_deferred: row.get(12),
            validated: row.get(13),
            no_inherit: row.get(14),
            delete_action: one_char(&row.get::<_, String>(15)),
            update_action: one_char(&row.get::<_, String>(16)),
            match_type: one_char(&row.get::<_, String>(17)),
        })
        .collect())
}

fn read_indexes(
    client: &mut impl GenericClient,
    schemas: &[String],
) -> Result<Vec<RawIndex>, CatalogError> {
    Ok(client
        .query(
            "
            SELECT idx_cls.oid::bigint,
                   tbl.oid::bigint,
                   ns.nspname,
                   tbl.relname,
                   idx_cls.relname,
                   am.amname,
                   idx.indisunique,
                   idx.indisprimary,
                   idx.indisexclusion,
                   idx.indimmediate,
                   idx.indisclustered,
                   idx.indisvalid,
                   idx.indisready,
                   idx.indislive,
                   idx.indisreplident,
                   COALESCE((pg_catalog.to_jsonb(idx)->>'indnullsnotdistinct')::boolean, false),
                   idx.indnkeyatts,
                   CASE WHEN pg_catalog.octet_length(pg_catalog.pg_get_indexdef(idx.indexrelid)) <= $2
                        THEN pg_catalog.pg_get_expr(idx.indpred, idx.indrelid, true)
                        ELSE NULL END,
                   CASE WHEN pg_catalog.octet_length(pg_catalog.pg_get_indexdef(idx.indexrelid)) <= $2
                        THEN pg_catalog.pg_get_expr(idx.indexprs, idx.indrelid, true)
                        ELSE NULL END,
                   CASE WHEN pg_catalog.octet_length(pg_catalog.pg_get_indexdef(idx.indexrelid)) <= $2
                        THEN pg_catalog.pg_get_indexdef(idx.indexrelid)
                        ELSE NULL END,
                   pg_catalog.octet_length(pg_catalog.pg_get_indexdef(idx.indexrelid)) > $2
            FROM pg_catalog.pg_index idx
            JOIN pg_catalog.pg_class tbl ON tbl.oid = idx.indrelid
            JOIN pg_catalog.pg_namespace ns ON ns.oid = tbl.relnamespace
            JOIN pg_catalog.pg_class idx_cls ON idx_cls.oid = idx.indexrelid
            JOIN pg_catalog.pg_am am ON am.oid = idx_cls.relam
            WHERE ns.nspname = ANY($1::text[])
            ORDER BY ns.nspname, tbl.relname, idx_cls.relname, idx_cls.oid
            ",
            &[&schemas, &MAX_DEFINITION_BYTES],
        )?
        .into_iter()
        .map(|row| RawIndex {
            oid: row.get(0),
            relation_oid: row.get(1),
            schema: row.get(2),
            relation: row.get(3),
            name: row.get(4),
            access_method: row.get(5),
            unique: row.get(6),
            primary: row.get(7),
            exclusion: row.get(8),
            immediate: row.get(9),
            clustered: row.get(10),
            valid: row.get(11),
            ready: row.get(12),
            live: row.get(13),
            replica_identity: row.get(14),
            nulls_not_distinct: row.get(15),
            key_count: row.get(16),
            predicate: row.get(17),
            expression: row.get(18),
            definition: row.get(19),
            definition_too_large: row.get(20),
        })
        .collect())
}

fn read_index_terms(
    client: &mut impl GenericClient,
    schemas: &[String],
) -> Result<Vec<RawIndexTerm>, CatalogError> {
    Ok(client
        .query(
            "
            SELECT idx.indexrelid::bigint,
                   key_part.ordinality::smallint,
                   key_part.attnum,
                   att.attname,
                   pg_catalog.pg_get_indexdef(
                       idx.indexrelid,
                       key_part.ordinality::integer,
                       true
                   ),
                   key_part.ordinality <= idx.indnkeyatts,
                   COALESCE((option_part.option_value & 1) <> 0, false),
                   COALESCE((option_part.option_value & 2) <> 0, false),
                   CASE WHEN opclass.oid IS NULL THEN NULL
                        ELSE opclass_ns.nspname || '.' || opclass.opcname END,
                   CASE WHEN coll.oid IS NULL OR coll.oid = 0 THEN NULL
                        ELSE coll_ns.nspname || '.' || coll.collname END
            FROM pg_catalog.pg_index idx
            JOIN pg_catalog.pg_class tbl ON tbl.oid = idx.indrelid
            JOIN pg_catalog.pg_namespace ns ON ns.oid = tbl.relnamespace
            CROSS JOIN LATERAL pg_catalog.unnest(idx.indkey) WITH ORDINALITY
                AS key_part(attnum, ordinality)
            LEFT JOIN pg_catalog.pg_attribute att
              ON att.attrelid = idx.indrelid AND att.attnum = key_part.attnum
            LEFT JOIN LATERAL pg_catalog.unnest(idx.indoption::smallint[]) WITH ORDINALITY
                AS option_part(option_value, ordinality)
              ON option_part.ordinality = key_part.ordinality
            LEFT JOIN LATERAL pg_catalog.unnest(idx.indclass::oid[]) WITH ORDINALITY
                AS class_part(opclass_oid, ordinality)
              ON class_part.ordinality = key_part.ordinality
            LEFT JOIN pg_catalog.pg_opclass opclass ON opclass.oid = class_part.opclass_oid
            LEFT JOIN pg_catalog.pg_namespace opclass_ns ON opclass_ns.oid = opclass.opcnamespace
            LEFT JOIN LATERAL pg_catalog.unnest(idx.indcollation::oid[]) WITH ORDINALITY
                AS coll_part(collation_oid, ordinality)
              ON coll_part.ordinality = key_part.ordinality
            LEFT JOIN pg_catalog.pg_collation coll ON coll.oid = coll_part.collation_oid
            LEFT JOIN pg_catalog.pg_namespace coll_ns ON coll_ns.oid = coll.collnamespace
            WHERE ns.nspname = ANY($1::text[])
            ORDER BY idx.indexrelid, key_part.ordinality
            ",
            &[&schemas],
        )?
        .into_iter()
        .map(|row| RawIndexTerm {
            index_oid: row.get(0),
            ordinal: row.get(1),
            column_number: row.get(2),
            column_name: row.get(3),
            definition: row.get(4),
            is_key: row.get(5),
            descending: row.get(6),
            nulls_first: row.get(7),
            operator_class: row.get(8),
            collation: row.get(9),
        })
        .collect())
}

fn read_types(
    client: &mut impl GenericClient,
    schemas: &[String],
) -> Result<Vec<RawType>, CatalogError> {
    Ok(client
        .query(
            "
            SELECT typ.oid::bigint,
                   ns.nspname,
                   typ.typname,
                   typ.typtype::text,
                   typ.typowner::bigint,
                   typ.typcategory::text,
                   NULLIF(typ.typrelid, 0)::bigint,
                   NULLIF(typ.typbasetype, 0)::bigint,
                   base_ns.nspname,
                   NULLIF(typ.typelem, 0)::bigint,
                   element_ns.nspname,
                   typ.typnotnull,
                   CASE WHEN typ.typdefault IS NOT NULL
                             AND pg_catalog.octet_length(typ.typdefault) <= $2
                        THEN typ.typdefault
                        ELSE NULL END,
                   CASE WHEN typ.typdefault IS NOT NULL
                        THEN pg_catalog.octet_length(typ.typdefault) > $2
                        ELSE false END,
                   CASE WHEN typ.typcollation = 0 THEN NULL
                        ELSE coll_ns.nspname || '.' || coll.collname END,
                   rng.rngsubtype::bigint,
                   range_subtype_ns.nspname,
                   NULLIF(rng.rngmultitypid, 0)::bigint,
                   multirange_ns.nspname,
                   pg_catalog.obj_description(typ.oid, 'pg_type')
            FROM pg_catalog.pg_type typ
            JOIN pg_catalog.pg_namespace ns ON ns.oid = typ.typnamespace
            LEFT JOIN pg_catalog.pg_class rel ON rel.oid = typ.typrelid
            LEFT JOIN pg_catalog.pg_type base_type ON base_type.oid = typ.typbasetype
            LEFT JOIN pg_catalog.pg_namespace base_ns ON base_ns.oid = base_type.typnamespace
            LEFT JOIN pg_catalog.pg_type element_type ON element_type.oid = typ.typelem
            LEFT JOIN pg_catalog.pg_namespace element_ns ON element_ns.oid = element_type.typnamespace
            LEFT JOIN pg_catalog.pg_range rng ON rng.rngtypid = typ.oid
            LEFT JOIN pg_catalog.pg_type range_subtype ON range_subtype.oid = rng.rngsubtype
            LEFT JOIN pg_catalog.pg_namespace range_subtype_ns
              ON range_subtype_ns.oid = range_subtype.typnamespace
            LEFT JOIN pg_catalog.pg_type multirange_type ON multirange_type.oid = rng.rngmultitypid
            LEFT JOIN pg_catalog.pg_namespace multirange_ns
              ON multirange_ns.oid = multirange_type.typnamespace
            LEFT JOIN pg_catalog.pg_collation coll ON coll.oid = typ.typcollation
            LEFT JOIN pg_catalog.pg_namespace coll_ns ON coll_ns.oid = coll.collnamespace
            WHERE ns.nspname = ANY($1::text[])
              AND typ.typisdefined
              AND typ.typtype IN ('b', 'c', 'd', 'e', 'r', 'm')
            ORDER BY ns.nspname, typ.typname, typ.oid
            ",
            &[&schemas, &MAX_PROPERTY_STRING_BYTES],
        )?
        .into_iter()
        .map(|row| RawType {
            oid: row.get(0),
            schema: row.get(1),
            name: row.get(2),
            kind: one_char(&row.get::<_, String>(3)),
            owner_oid: row.get(4),
            category: one_char(&row.get::<_, String>(5)),
            relation_oid: row.get(6),
            base_type_oid: row.get(7),
            base_type_schema: row.get(8),
            element_type_oid: row.get(9),
            element_type_schema: row.get(10),
            not_null: row.get(11),
            default_value: row.get(12),
            default_too_large: row.get(13),
            collation: row.get(14),
            range_subtype_oid: row.get(15),
            range_subtype_schema: row.get(16),
            multirange_type_oid: row.get(17),
            multirange_type_schema: row.get(18),
            comment: row.get(19),
        })
        .collect())
}

fn read_enum_values(
    client: &mut impl GenericClient,
    schemas: &[String],
) -> Result<Vec<RawEnumValue>, CatalogError> {
    Ok(client
        .query(
            "
            SELECT enum.enumtypid::bigint,
                   enum.enumlabel,
                   enum.enumsortorder::text
            FROM pg_catalog.pg_enum enum
            JOIN pg_catalog.pg_type typ ON typ.oid = enum.enumtypid
            JOIN pg_catalog.pg_namespace ns ON ns.oid = typ.typnamespace
            WHERE ns.nspname = ANY($1::text[])
            ORDER BY enum.enumtypid, enum.enumsortorder, enum.oid
            ",
            &[&schemas],
        )?
        .into_iter()
        .map(|row| RawEnumValue {
            type_oid: row.get(0),
            label: row.get(1),
            sort_order: row.get(2),
        })
        .collect())
}

fn read_sequences(
    client: &mut impl GenericClient,
    schemas: &[String],
) -> Result<Vec<RawSequence>, CatalogError> {
    Ok(client
        .query(
            "
            SELECT seq.seqrelid::bigint,
                   seq.seqtypid::bigint,
                   seq.seqstart,
                   seq.seqmin,
                   seq.seqmax,
                   seq.seqincrement,
                   seq.seqcycle,
                   seq.seqcache
            FROM pg_catalog.pg_sequence seq
            JOIN pg_catalog.pg_class cls ON cls.oid = seq.seqrelid
            JOIN pg_catalog.pg_namespace ns ON ns.oid = cls.relnamespace
            WHERE ns.nspname = ANY($1::text[])
            ORDER BY ns.nspname, cls.relname, cls.oid
            ",
            &[&schemas],
        )?
        .into_iter()
        .map(|row| RawSequence {
            relation_oid: row.get(0),
            type_oid: row.get(1),
            start_value: row.get(2),
            min_value: row.get(3),
            max_value: row.get(4),
            increment_by: row.get(5),
            cycle: row.get(6),
            cache_size: row.get(7),
        })
        .collect())
}

fn read_routines(
    client: &mut impl GenericClient,
    schemas: &[String],
) -> Result<Vec<RawRoutine>, CatalogError> {
    Ok(client
        .query(
            "
            SELECT proc.oid::bigint,
                   ns.nspname,
                   proc.proname,
                   pg_catalog.pg_get_function_identity_arguments(proc.oid),
                   proc.prokind::text,
                   proc.proowner::bigint,
                   lang.lanname,
                   proc.prorettype::bigint,
                   return_ns.nspname,
                   pg_catalog.pg_get_function_result(proc.oid),
                   proc.proretset,
                   proc.prosecdef,
                   proc.proleakproof,
                   proc.proisstrict,
                   proc.provolatile::text,
                   proc.proparallel::text,
                   CASE WHEN proc.prokind IN ('f', 'p')
                             AND pg_catalog.octet_length(pg_catalog.pg_get_functiondef(proc.oid)) <= $2
                        THEN pg_catalog.pg_get_functiondef(proc.oid)
                        ELSE NULL END,
                   CASE WHEN proc.prokind IN ('f', 'p')
                        THEN pg_catalog.octet_length(pg_catalog.pg_get_functiondef(proc.oid)) > $2
                        ELSE false END,
                   pg_catalog.pg_get_function_arguments(proc.oid),
                   lang.lanname = 'sql' AND proc.prosqlbody IS NOT NULL
            FROM pg_catalog.pg_proc proc
            JOIN pg_catalog.pg_namespace ns ON ns.oid = proc.pronamespace
            JOIN pg_catalog.pg_language lang ON lang.oid = proc.prolang
            JOIN pg_catalog.pg_type return_type ON return_type.oid = proc.prorettype
            JOIN pg_catalog.pg_namespace return_ns ON return_ns.oid = return_type.typnamespace
            WHERE ns.nspname = ANY($1::text[])
            ORDER BY ns.nspname,
                     proc.proname,
                     pg_catalog.pg_get_function_identity_arguments(proc.oid),
                     proc.oid
            ",
            &[&schemas, &MAX_DEFINITION_BYTES],
        )?
        .into_iter()
        .map(|row| RawRoutine {
            oid: row.get(0),
            schema: row.get(1),
            name: row.get(2),
            identity_arguments: row.get(3),
            kind: one_char(&row.get::<_, String>(4)),
            owner_oid: row.get(5),
            language: row.get(6),
            return_type_oid: row.get(7),
            return_type_schema: row.get(8),
            return_type: row.get(9),
            returns_set: row.get(10),
            security_definer: row.get(11),
            leakproof: row.get(12),
            strict: row.get(13),
            volatility: one_char(&row.get::<_, String>(14)),
            parallel: one_char(&row.get::<_, String>(15)),
            definition: row.get(16),
            definition_too_large: row.get(17),
            arguments_definition: row.get(18),
            body_catalog_tracked: row.get(19),
        })
        .collect())
}

fn read_routine_parameters(
    client: &mut impl GenericClient,
    schemas: &[String],
) -> Result<Vec<RawRoutineParameter>, CatalogError> {
    Ok(client
        .query(
            "
            SELECT proc.oid::bigint,
                   argument_type.ordinality::integer,
                   NULLIF(proc.proargnames[argument_type.ordinality], ''),
                   COALESCE(proc.proargmodes[argument_type.ordinality], 'i'::\"char\")::text,
                   argument_type.type_oid::bigint,
                   argument_type_ns.nspname,
                   pg_catalog.format_type(argument_type.type_oid, NULL)
            FROM pg_catalog.pg_proc proc
            JOIN pg_catalog.pg_namespace ns ON ns.oid = proc.pronamespace
            CROSS JOIN LATERAL pg_catalog.unnest(
                COALESCE(proc.proallargtypes, proc.proargtypes::oid[])
            ) WITH ORDINALITY AS argument_type(type_oid, ordinality)
            JOIN pg_catalog.pg_type argument_pg_type
              ON argument_pg_type.oid = argument_type.type_oid
            JOIN pg_catalog.pg_namespace argument_type_ns
              ON argument_type_ns.oid = argument_pg_type.typnamespace
            WHERE ns.nspname = ANY($1::text[])
            ORDER BY proc.oid, argument_type.ordinality
            ",
            &[&schemas],
        )?
        .into_iter()
        .map(|row| RawRoutineParameter {
            routine_oid: row.get(0),
            ordinal: row.get(1),
            name: row.get(2),
            mode: one_char(&row.get::<_, String>(3)),
            type_oid: row.get(4),
            type_schema: row.get(5),
            data_type: row.get(6),
        })
        .collect())
}

fn read_triggers(
    client: &mut impl GenericClient,
    schemas: &[String],
) -> Result<Vec<RawTrigger>, CatalogError> {
    Ok(client
        .query(
            "
            SELECT trg.oid::bigint,
                   trg.tgrelid::bigint,
                   trg.tgfoid::bigint,
                   trg.tgname,
                   CASE
                     WHEN (trg.tgtype::integer & 2) <> 0 THEN 'BEFORE'
                     WHEN (trg.tgtype::integer & 64) <> 0 THEN 'INSTEAD OF'
                     ELSE 'AFTER'
                   END,
                   (trg.tgtype::integer & 4) <> 0,
                   (trg.tgtype::integer & 8) <> 0,
                   (trg.tgtype::integer & 16) <> 0,
                   (trg.tgtype::integer & 32) <> 0,
                   CASE WHEN (trg.tgtype::integer & 1) <> 0 THEN 'ROW' ELSE 'STATEMENT' END,
                   trg.tgenabled::text,
                   COALESCE(trg.tgattr::smallint[], ARRAY[]::smallint[]),
                   CASE WHEN pg_catalog.octet_length(pg_catalog.pg_get_triggerdef(trg.oid, true)) <= $2
                        THEN pg_catalog.pg_get_expr(trg.tgqual, trg.tgrelid, true)
                        ELSE NULL END,
                   CASE WHEN pg_catalog.octet_length(pg_catalog.pg_get_triggerdef(trg.oid, true)) <= $2
                        THEN pg_catalog.pg_get_triggerdef(trg.oid, true)
                        ELSE NULL END,
                   pg_catalog.octet_length(pg_catalog.pg_get_triggerdef(trg.oid, true)) > $2
            FROM pg_catalog.pg_trigger trg
            JOIN pg_catalog.pg_class rel ON rel.oid = trg.tgrelid
            JOIN pg_catalog.pg_namespace ns ON ns.oid = rel.relnamespace
            WHERE ns.nspname = ANY($1::text[])
              AND NOT trg.tgisinternal
            ORDER BY ns.nspname, rel.relname, trg.tgname, trg.oid
            ",
            &[&schemas, &MAX_DEFINITION_BYTES],
        )?
        .into_iter()
        .map(|row| {
            let mut events = Vec::new();
            if row.get(5) {
                events.push("INSERT".to_owned());
            }
            if row.get(6) {
                events.push("DELETE".to_owned());
            }
            if row.get(7) {
                events.push("UPDATE".to_owned());
            }
            if row.get(8) {
                events.push("TRUNCATE".to_owned());
            }
            RawTrigger {
                oid: row.get(0),
                relation_oid: row.get(1),
                routine_oid: row.get(2),
                name: row.get(3),
                timing: row.get(4),
                events,
                orientation: row.get(9),
                enabled: one_char(&row.get::<_, String>(10)),
                update_columns: row.get(11),
                when_expression: row.get(12),
                definition: row.get(13),
                definition_too_large: row.get(14),
            }
        })
        .collect())
}

fn read_inheritance(
    client: &mut impl GenericClient,
    schemas: &[String],
) -> Result<Vec<RawInheritance>, CatalogError> {
    Ok(client
        .query(
            "
            SELECT inh.inhrelid::bigint,
                   inh.inhparent::bigint,
                   inh.inhseqno,
                   child.relispartition
            FROM pg_catalog.pg_inherits inh
            JOIN pg_catalog.pg_class child ON child.oid = inh.inhrelid
            JOIN pg_catalog.pg_namespace child_ns ON child_ns.oid = child.relnamespace
            JOIN pg_catalog.pg_class parent ON parent.oid = inh.inhparent
            WHERE child_ns.nspname = ANY($1::text[])
              AND child.relkind IN ('r', 'p', 'f')
              AND parent.relkind IN ('r', 'p', 'f')
            ORDER BY inh.inhrelid, inh.inhseqno, inh.inhparent
            ",
            &[&schemas],
        )?
        .into_iter()
        .map(|row| RawInheritance {
            child_oid: row.get(0),
            parent_oid: row.get(1),
            sequence_number: row.get(2),
            child_is_partition: row.get(3),
        })
        .collect())
}

fn read_view_dependencies(
    client: &mut impl GenericClient,
    schemas: &[String],
) -> Result<Vec<RawViewDependency>, CatalogError> {
    Ok(client
        .query(
            "
            SELECT view_cls.oid::bigint,
                   target_cls.oid::bigint,
                   dep.refobjsubid,
                   target_ns.nspname,
                   dep.deptype::text
            FROM pg_catalog.pg_rewrite rewrite
            JOIN pg_catalog.pg_class view_cls ON view_cls.oid = rewrite.ev_class
            JOIN pg_catalog.pg_namespace view_ns ON view_ns.oid = view_cls.relnamespace
            JOIN pg_catalog.pg_depend dep
              ON dep.classid = 'pg_catalog.pg_rewrite'::regclass
             AND dep.objid = rewrite.oid
            JOIN pg_catalog.pg_class target_cls
              ON dep.refclassid = 'pg_catalog.pg_class'::regclass
             AND dep.refobjid = target_cls.oid
            JOIN pg_catalog.pg_namespace target_ns ON target_ns.oid = target_cls.relnamespace
            WHERE view_ns.nspname = ANY($1::text[])
              AND view_cls.relkind IN ('v', 'm')
              AND rewrite.rulename = '_RETURN'
              AND dep.deptype IN ('n', 'a', 'i')
              AND target_cls.oid <> view_cls.oid
            GROUP BY view_cls.oid, target_cls.oid, dep.refobjsubid, target_ns.nspname, dep.deptype
            ORDER BY view_cls.oid, target_cls.oid, dep.refobjsubid
            ",
            &[&schemas],
        )?
        .into_iter()
        .map(|row| RawViewDependency {
            view_oid: row.get(0),
            target_relation_oid: row.get(1),
            target_column_number: row.get(2),
            target_schema: row.get(3),
            dependency_type: one_char(&row.get::<_, String>(4)),
        })
        .collect())
}

fn read_routine_dependencies(
    client: &mut impl GenericClient,
    schemas: &[String],
) -> Result<Vec<RawDependency>, CatalogError> {
    Ok(client
        .query(
            "
            SELECT proc.oid::bigint,
                   CASE dep.refclassid
                     WHEN 'pg_catalog.pg_class'::regclass THEN 'relation'
                     WHEN 'pg_catalog.pg_proc'::regclass THEN 'routine'
                     WHEN 'pg_catalog.pg_type'::regclass THEN 'type'
                   END,
                   dep.refobjid::bigint,
                   dep.refobjsubid,
                   COALESCE(rel_ns.nspname, proc_target_ns.nspname, type_ns.nspname),
                   dep.deptype::text
            FROM pg_catalog.pg_proc proc
            JOIN pg_catalog.pg_namespace proc_ns ON proc_ns.oid = proc.pronamespace
            JOIN pg_catalog.pg_depend dep
              ON dep.classid = 'pg_catalog.pg_proc'::regclass
             AND dep.objid = proc.oid
            LEFT JOIN pg_catalog.pg_class rel_target
              ON dep.refclassid = 'pg_catalog.pg_class'::regclass
             AND rel_target.oid = dep.refobjid
            LEFT JOIN pg_catalog.pg_namespace rel_ns ON rel_ns.oid = rel_target.relnamespace
            LEFT JOIN pg_catalog.pg_proc proc_target
              ON dep.refclassid = 'pg_catalog.pg_proc'::regclass
             AND proc_target.oid = dep.refobjid
            LEFT JOIN pg_catalog.pg_namespace proc_target_ns
              ON proc_target_ns.oid = proc_target.pronamespace
            LEFT JOIN pg_catalog.pg_type type_target
              ON dep.refclassid = 'pg_catalog.pg_type'::regclass
             AND type_target.oid = dep.refobjid
            LEFT JOIN pg_catalog.pg_namespace type_ns ON type_ns.oid = type_target.typnamespace
            WHERE proc_ns.nspname = ANY($1::text[])
              AND dep.refclassid IN (
                    'pg_catalog.pg_class'::regclass,
                    'pg_catalog.pg_proc'::regclass,
                    'pg_catalog.pg_type'::regclass
                  )
              AND NOT (
                    dep.refclassid = 'pg_catalog.pg_proc'::regclass
                AND dep.refobjid = proc.oid
              )
              AND dep.deptype IN ('n', 'a', 'i')
            ORDER BY proc.oid, dep.refclassid, dep.refobjid, dep.refobjsubid
            ",
            &[&schemas],
        )?
        .into_iter()
        .map(|row| RawDependency {
            owner_oid: row.get(0),
            target_class: row.get(1),
            target_oid: row.get(2),
            target_sub_id: row.get(3),
            target_schema: row.get(4),
            dependency_type: one_char(&row.get::<_, String>(5)),
        })
        .collect())
}

fn read_sequence_usages(
    client: &mut impl GenericClient,
    schemas: &[String],
) -> Result<Vec<RawSequenceUsage>, CatalogError> {
    Ok(client
        .query(
            "
            SELECT source_relation_oid,
                   source_column_number,
                   sequence_oid,
                   dependency_type
            FROM (
                SELECT dep.refobjid::bigint AS source_relation_oid,
                       dep.refobjsubid AS source_column_number,
                       seq.oid::bigint AS sequence_oid,
                       dep.deptype::text AS dependency_type
                FROM pg_catalog.pg_class seq
                JOIN pg_catalog.pg_namespace seq_ns ON seq_ns.oid = seq.relnamespace
                JOIN pg_catalog.pg_depend dep
                  ON dep.classid = 'pg_catalog.pg_class'::regclass
                 AND dep.objid = seq.oid
                 AND dep.refclassid = 'pg_catalog.pg_class'::regclass
                WHERE seq.relkind = 'S'
                  AND seq_ns.nspname = ANY($1::text[])
                  AND dep.refobjsubid > 0
                  AND dep.deptype IN ('a', 'i')

                UNION

                SELECT attrdef.adrelid::bigint,
                       attrdef.adnum,
                       seq.oid::bigint,
                       dep.deptype::text
                FROM pg_catalog.pg_attrdef attrdef
                JOIN pg_catalog.pg_class source_rel ON source_rel.oid = attrdef.adrelid
                JOIN pg_catalog.pg_namespace source_ns ON source_ns.oid = source_rel.relnamespace
                JOIN pg_catalog.pg_depend dep
                  ON dep.classid = 'pg_catalog.pg_attrdef'::regclass
                 AND dep.objid = attrdef.oid
                 AND dep.refclassid = 'pg_catalog.pg_class'::regclass
                JOIN pg_catalog.pg_class seq ON seq.oid = dep.refobjid AND seq.relkind = 'S'
                WHERE source_ns.nspname = ANY($1::text[])
                  AND dep.deptype IN ('n', 'a', 'i')
            ) usage
            ORDER BY source_relation_oid, source_column_number, sequence_oid
            ",
            &[&schemas],
        )?
        .into_iter()
        .map(|row| RawSequenceUsage {
            column_relation_oid: row.get(0),
            column_number: row.get(1),
            sequence_oid: row.get(2),
            dependency_type: one_char(&row.get::<_, String>(3)),
        })
        .collect())
}

fn read_policies(
    client: &mut impl GenericClient,
    schemas: &[String],
) -> Result<Vec<RawPolicy>, CatalogError> {
    Ok(client
        .query(
            "
            SELECT policy.oid::bigint,
                   policy.polrelid::bigint,
                   policy.polname,
                   policy.polcmd::text,
                   policy.polpermissive,
                   ARRAY(
                       SELECT role_oid::bigint
                       FROM pg_catalog.unnest(policy.polroles) AS role_oid
                       ORDER BY role_oid
                   ),
                   pg_catalog.pg_get_expr(policy.polqual, policy.polrelid, true),
                   pg_catalog.pg_get_expr(policy.polwithcheck, policy.polrelid, true)
            FROM pg_catalog.pg_policy policy
            JOIN pg_catalog.pg_class rel ON rel.oid = policy.polrelid
            JOIN pg_catalog.pg_namespace ns ON ns.oid = rel.relnamespace
            WHERE ns.nspname = ANY($1::text[])
            ORDER BY ns.nspname, rel.relname, policy.polname, policy.oid
            ",
            &[&schemas],
        )?
        .into_iter()
        .map(|row| RawPolicy {
            oid: row.get(0),
            relation_oid: row.get(1),
            name: row.get(2),
            command: one_char(&row.get::<_, String>(3)),
            permissive: row.get(4),
            role_oids: row.get(5),
            using_expression: row.get(6),
            check_expression: row.get(7),
        })
        .collect())
}

fn read_extensions(client: &mut impl GenericClient) -> Result<Vec<RawExtension>, CatalogError> {
    Ok(client
        .query(
            "
            SELECT ext.oid::bigint,
                   ext.extname,
                   ext.extowner::bigint,
                   ns.nspname,
                   ext.extrelocatable,
                   ext.extversion
            FROM pg_catalog.pg_extension ext
            LEFT JOIN pg_catalog.pg_namespace ns ON ns.oid = ext.extnamespace
            ORDER BY ext.extname, ext.oid
            ",
            &[],
        )?
        .into_iter()
        .map(|row| RawExtension {
            oid: row.get(0),
            name: row.get(1),
            owner_oid: row.get(2),
            schema: row.get(3),
            relocatable: row.get(4),
            version: row.get(5),
        })
        .collect())
}

fn read_extension_routine_oids(
    client: &mut impl GenericClient,
) -> Result<BTreeSet<i64>, CatalogError> {
    Ok(client
        .query(
            "
            SELECT dep.objid::bigint
            FROM pg_catalog.pg_depend dep
            WHERE dep.classid = 'pg_catalog.pg_proc'::regclass
              AND dep.refclassid = 'pg_catalog.pg_extension'::regclass
              AND dep.deptype = 'e'
            ORDER BY dep.objid
            ",
            &[],
        )?
        .into_iter()
        .map(|row| row.get::<_, i64>(0))
        .collect())
}

fn read_event_triggers(
    client: &mut impl GenericClient,
) -> Result<Vec<RawEventTrigger>, CatalogError> {
    Ok(client
        .query(
            "
            SELECT event.oid::bigint,
                   event.evtname,
                   event.evtevent,
                   event.evtowner::bigint,
                   event.evtfoid::bigint,
                   proc_ns.nspname,
                   event.evtenabled::text,
                   COALESCE(event.evttags, ARRAY[]::text[])
            FROM pg_catalog.pg_event_trigger event
            JOIN pg_catalog.pg_proc proc ON proc.oid = event.evtfoid
            JOIN pg_catalog.pg_namespace proc_ns ON proc_ns.oid = proc.pronamespace
            ORDER BY event.evtname, event.oid
            ",
            &[],
        )?
        .into_iter()
        .map(|row| RawEventTrigger {
            oid: row.get(0),
            name: row.get(1),
            event: row.get(2),
            owner_oid: row.get(3),
            routine_oid: row.get(4),
            routine_schema: row.get(5),
            enabled: one_char(&row.get::<_, String>(6)),
            tags: row.get(7),
        })
        .collect())
}

impl<'a> PostgresSnapshotMapper<'a> {
    fn new(connection_alias: &'a str, source_kind: &'static str) -> Self {
        Self {
            connection_alias,
            source_kind,
        }
    }

    fn map(&self, raw: RawPostgresCatalog) -> Result<CatalogDiscovery, CatalogError> {
        validate_raw_catalog(&raw)?;
        let strategy = raw.strategy;
        if self.source_kind != strategy.source_kind() {
            return Err(CatalogError::Mapping(format!(
                "mapper source '{}' does not match selected strategy {}",
                self.source_kind,
                strategy.strategy_name()
            )));
        }

        let database_name = raw.server.database.clone();
        let database_key = pg_key(
            self.source_kind,
            self.connection_alias,
            &database_name,
            &database_name,
            ObjectKind::Database,
            &database_name,
            None,
        );
        let database = DatabaseObject {
            key: database_key.clone(),
            name: database_name.clone(),
        };

        let schemas = raw
            .schemas
            .iter()
            .map(|schema| SchemaObject {
                key: pg_key(
                    self.source_kind,
                    self.connection_alias,
                    &database_name,
                    &schema.name,
                    ObjectKind::Schema,
                    &schema.name,
                    None,
                ),
                database_key: database_key.clone(),
                name: schema.name.clone(),
            })
            .collect::<Vec<_>>();
        let schema_keys = schemas
            .iter()
            .map(|schema| (schema.name.clone(), schema.key.clone()))
            .collect::<BTreeMap<_, _>>();

        let mut metadata = CanonicalMetadata::default();
        let mut principal_keys = BTreeMap::new();
        for principal in &raw.principals {
            let key = pg_key(
                self.source_kind,
                self.connection_alias,
                &database_name,
                &database_name,
                ObjectKind::Principal,
                &principal.name,
                None,
            );
            principal_keys.insert(principal.oid, key.clone());
            let mut properties = BTreeMap::new();
            insert_bool(&mut properties, "superuser", principal.superuser);
            insert_bool(&mut properties, "inherit", principal.inherit);
            insert_bool(&mut properties, "create_role", principal.create_role);
            insert_bool(
                &mut properties,
                "create_database",
                principal.create_database,
            );
            insert_bool(&mut properties, "can_login", principal.can_login);
            insert_bool(&mut properties, "replication", principal.replication);
            insert_bool(&mut properties, "bypass_rls", principal.bypass_rls);
            insert_optional_string(
                &mut properties,
                "valid_until",
                principal.valid_until.as_deref(),
            );
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(database_key.clone()),
                name: principal.name.clone(),
                extension_kind: None,
                definition: None,
                properties,
            });
        }

        for schema in &raw.schemas {
            let schema_key = required(
                schema_keys.get(&schema.name),
                format!("schema key for {}", schema.name),
            )?;
            let mut properties = BTreeMap::new();
            insert_i64(&mut properties, "postgres_oid", schema.oid);
            insert_optional_string(&mut properties, "comment", schema.comment.as_deref());
            metadata.annotations.push(ObjectAnnotation {
                object_key: schema_key.clone(),
                definition: None,
                properties,
            });
            add_owned_by(
                &mut metadata.relationships,
                schema_key,
                schema.owner_oid,
                &principal_keys,
                "schema",
            )?;
        }

        let mut type_keys = BTreeMap::new();
        for raw_type in &raw.types {
            let parent = required(
                schema_keys.get(&raw_type.schema),
                format!("schema key for pg_catalog type {}", raw_type.name),
            )?;
            let kind = if raw_type.kind == 'd' {
                ObjectKind::Domain
            } else {
                ObjectKind::UserDefinedType
            };
            let key = pg_key(
                self.source_kind,
                self.connection_alias,
                &database_name,
                &raw_type.schema,
                kind,
                &raw_type.name,
                None,
            );
            if type_keys.insert(raw_type.oid, key.clone()).is_some() {
                return Err(CatalogError::Mapping(format!(
                    "duplicate pg_catalog type oid {}",
                    raw_type.oid
                )));
            }
            let mut properties = BTreeMap::new();
            insert_i64(&mut properties, "postgres_oid", raw_type.oid);
            insert_string(
                &mut properties,
                "postgres_type_kind",
                type_kind_name(raw_type.kind),
            );
            insert_string(&mut properties, "category", raw_type.category.to_string());
            insert_bool(&mut properties, "not_null", raw_type.not_null);
            insert_optional_string(
                &mut properties,
                "default",
                raw_type.default_value.as_deref(),
            );
            insert_optional_string(&mut properties, "collation", raw_type.collation.as_deref());
            insert_optional_string(&mut properties, "comment", raw_type.comment.as_deref());
            insert_bool(
                &mut properties,
                "implicit_relation_type",
                raw_type.relation_oid.is_some(),
            );
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(parent.clone()),
                name: raw_type.name.clone(),
                extension_kind: None,
                definition: None,
                properties,
            });
            add_owned_by(
                &mut metadata.relationships,
                &key,
                raw_type.owner_oid,
                &principal_keys,
                "type",
            )?;
        }

        for enum_value in &raw.enum_values {
            let type_key = required(
                type_keys.get(&enum_value.type_oid),
                format!("enum parent type oid {}", enum_value.type_oid),
            )?;
            let key = pg_key(
                self.source_kind,
                self.connection_alias,
                &database_name,
                &type_key.schema,
                ObjectKind::EnumValue,
                &type_key.object_name,
                Some(enum_value.label.clone()),
            );
            let mut properties = BTreeMap::new();
            insert_string(&mut properties, "sort_order", &enum_value.sort_order);
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(type_key.clone()),
                name: enum_value.label.clone(),
                extension_kind: None,
                definition: None,
                properties,
            });
        }

        let sequence_facts = raw
            .sequences
            .iter()
            .map(|sequence| (sequence.relation_oid, sequence))
            .collect::<BTreeMap<_, _>>();
        let mut tables = Vec::new();
        let mut views = Vec::new();
        let mut table_keys = BTreeMap::new();
        let mut view_keys = BTreeMap::new();
        let mut materialized_view_keys = BTreeMap::new();
        let mut sequence_keys = BTreeMap::new();
        let mut relation_keys = BTreeMap::new();
        let mut relation_row_type_keys = BTreeMap::new();
        for relation in &raw.relations {
            if let Some(type_key) = type_keys.get(&relation.row_type_oid) {
                relation_row_type_keys.insert(relation.row_type_oid, type_key.clone());
            }
            let schema_key = required(
                schema_keys.get(&relation.schema),
                format!(
                    "schema key for relation {}.{}",
                    relation.schema, relation.name
                ),
            )?;
            match relation.relkind {
                'r' | 'p' | 'f' => {
                    let key = pg_key(
                        self.source_kind,
                        self.connection_alias,
                        &database_name,
                        &relation.schema,
                        ObjectKind::Table,
                        &relation.name,
                        None,
                    );
                    table_keys.insert(relation.oid, key.clone());
                    relation_keys.insert(relation.oid, key.clone());
                    tables.push(TableObject {
                        key: key.clone(),
                        schema_key: schema_key.clone(),
                        name: relation.name.clone(),
                        kind: table_kind(relation),
                    });
                    metadata
                        .annotations
                        .push(relation_annotation(relation, &key));
                    add_owned_by(
                        &mut metadata.relationships,
                        &key,
                        relation.owner_oid,
                        &principal_keys,
                        "table",
                    )?;
                }
                'v' => {
                    let key = pg_key(
                        self.source_kind,
                        self.connection_alias,
                        &database_name,
                        &relation.schema,
                        ObjectKind::View,
                        &relation.name,
                        None,
                    );
                    view_keys.insert(relation.oid, key.clone());
                    relation_keys.insert(relation.oid, key.clone());
                    views.push(ViewObject {
                        key: key.clone(),
                        schema_key: schema_key.clone(),
                        name: relation.name.clone(),
                        definition: relation.definition.clone(),
                        depends_on: Vec::new(),
                    });
                    metadata
                        .annotations
                        .push(relation_annotation(relation, &key));
                    add_owned_by(
                        &mut metadata.relationships,
                        &key,
                        relation.owner_oid,
                        &principal_keys,
                        "view",
                    )?;
                }
                'm' => {
                    let key = pg_key(
                        self.source_kind,
                        self.connection_alias,
                        &database_name,
                        &relation.schema,
                        ObjectKind::MaterializedView,
                        &relation.name,
                        None,
                    );
                    materialized_view_keys.insert(relation.oid, key.clone());
                    relation_keys.insert(relation.oid, key.clone());
                    metadata.objects.push(MetadataObject {
                        key: key.clone(),
                        parent_key: Some(schema_key.clone()),
                        name: relation.name.clone(),
                        extension_kind: None,
                        definition: relation.definition.clone(),
                        properties: relation_properties(relation),
                    });
                    add_owned_by(
                        &mut metadata.relationships,
                        &key,
                        relation.owner_oid,
                        &principal_keys,
                        "materialized view",
                    )?;
                }
                'S' => {
                    let key = pg_key(
                        self.source_kind,
                        self.connection_alias,
                        &database_name,
                        &relation.schema,
                        ObjectKind::Sequence,
                        &relation.name,
                        None,
                    );
                    sequence_keys.insert(relation.oid, key.clone());
                    relation_keys.insert(relation.oid, key.clone());
                    let sequence = required(
                        sequence_facts.get(&relation.oid).copied(),
                        format!("pg_sequence row for {}.{}", relation.schema, relation.name),
                    )?;
                    let mut properties = relation_properties(relation);
                    insert_i64(&mut properties, "type_oid", sequence.type_oid);
                    insert_i64(&mut properties, "start", sequence.start_value);
                    insert_i64(&mut properties, "minimum", sequence.min_value);
                    insert_i64(&mut properties, "maximum", sequence.max_value);
                    insert_i64(&mut properties, "increment", sequence.increment_by);
                    insert_bool(&mut properties, "cycle", sequence.cycle);
                    insert_i64(&mut properties, "cache", sequence.cache_size);
                    metadata.objects.push(MetadataObject {
                        key: key.clone(),
                        parent_key: Some(schema_key.clone()),
                        name: relation.name.clone(),
                        extension_kind: None,
                        definition: None,
                        properties,
                    });
                    add_owned_by(
                        &mut metadata.relationships,
                        &key,
                        relation.owner_oid,
                        &principal_keys,
                        "sequence",
                    )?;
                }
                'c' => {}
                other => {
                    return Err(CatalogError::Mapping(format!(
                        "unmapped pg_catalog relation kind '{other}' for {}.{}",
                        relation.schema, relation.name
                    )));
                }
            }
        }
        let mut physical_relation_keys = relation_keys.clone();

        let mut columns = Vec::new();
        let mut column_keys = BTreeMap::new();
        for column in &raw.columns {
            let parent_key = relation_keys.get(&column.relation_oid);
            match column.relation_kind {
                'r' | 'p' | 'f' => {
                    let table_key = required(
                        parent_key,
                        format!(
                            "table key for column {}.{}.{}",
                            column.schema, column.relation, column.name
                        ),
                    )?;
                    let key = pg_key(
                        self.source_kind,
                        self.connection_alias,
                        &database_name,
                        &column.schema,
                        ObjectKind::Column,
                        &column.relation,
                        Some(column.name.clone()),
                    );
                    column_keys.insert((column.relation_oid, column.attnum as i32), key.clone());
                    columns.push(ColumnObject {
                        key: key.clone(),
                        table_key: table_key.clone(),
                        name: column.name.clone(),
                        ordinal_position: positive_u32(column.attnum, "column ordinal")?,
                        data_type: column.data_type.clone(),
                        is_nullable: column.nullable,
                        default_value: column.default_expression.clone(),
                        is_generated: column.generated != '\0',
                    });
                    metadata.annotations.push(column_annotation(column, &key));
                    add_type_use(
                        &mut metadata.relationships,
                        &key,
                        column.type_oid,
                        &column.type_schema,
                        &type_keys,
                    )?;
                }
                'v' | 'm' => {
                    let view_key = required(
                        parent_key,
                        format!(
                            "view key for output column {}.{}.{}",
                            column.schema, column.relation, column.name
                        ),
                    )?;
                    let key = pg_key(
                        self.source_kind,
                        self.connection_alias,
                        &database_name,
                        &column.schema,
                        ObjectKind::ViewColumn,
                        &column.relation,
                        Some(column.name.clone()),
                    );
                    column_keys.insert((column.relation_oid, column.attnum as i32), key.clone());
                    let mut properties = column_properties(column);
                    insert_u64(
                        &mut properties,
                        "ordinal_position",
                        positive_u32(column.attnum, "view column ordinal")? as u64,
                    );
                    insert_string(&mut properties, "data_type", &column.data_type);
                    insert_bool(&mut properties, "nullable", column.nullable);
                    metadata.objects.push(MetadataObject {
                        key: key.clone(),
                        parent_key: Some(view_key.clone()),
                        name: column.name.clone(),
                        extension_kind: None,
                        definition: None,
                        properties,
                    });
                    add_type_use(
                        &mut metadata.relationships,
                        &key,
                        column.type_oid,
                        &column.type_schema,
                        &type_keys,
                    )?;
                }
                'c' => {
                    let parent_type = required(
                        type_keys.get(&relation_row_type_oid(&raw.relations, column.relation_oid)?),
                        format!(
                            "composite type for attribute {}.{}.{}",
                            column.schema, column.relation, column.name
                        ),
                    )?;
                    let key = pg_key(
                        self.source_kind,
                        self.connection_alias,
                        &database_name,
                        &column.schema,
                        ObjectKind::Extension,
                        &column.relation,
                        Some(column.name.clone()),
                    );
                    column_keys.insert((column.relation_oid, column.attnum as i32), key.clone());
                    let mut properties = column_properties(column);
                    insert_u64(
                        &mut properties,
                        "ordinal_position",
                        positive_u32(column.attnum, "composite attribute ordinal")? as u64,
                    );
                    insert_string(&mut properties, "data_type", &column.data_type);
                    metadata.objects.push(MetadataObject {
                        key: key.clone(),
                        parent_key: Some(parent_type.clone()),
                        name: column.name.clone(),
                        extension_kind: Some("postgres_composite_attribute".to_owned()),
                        definition: None,
                        properties,
                    });
                    if let Some(type_key) = type_keys.get(&column.type_oid) {
                        metadata.relationships.push(MetadataRelationship {
                            kind: MetadataRelationshipKind::DependsOn,
                            from_key: key,
                            to_key: type_key.clone(),
                            ordinal: None,
                            properties: BTreeMap::new(),
                        });
                    }
                }
                other => {
                    return Err(CatalogError::Mapping(format!(
                        "unmapped pg_catalog column relation kind '{other}' for {}.{}.{}",
                        column.schema, column.relation, column.name
                    )));
                }
            }
        }

        let mut constraints = Vec::new();
        for constraint in &raw.constraints {
            if let Some(domain_oid) = constraint.domain_type_oid {
                if constraint.kind != 'c' {
                    return Err(CatalogError::Mapping(format!(
                        "unsupported domain constraint kind '{}' for {}",
                        constraint.kind, constraint.name
                    )));
                }
                let domain_key = required(
                    type_keys.get(&domain_oid),
                    format!(
                        "domain parent oid {domain_oid} for constraint {}",
                        constraint.name
                    ),
                )?;
                let key = pg_key(
                    self.source_kind,
                    self.connection_alias,
                    &database_name,
                    &constraint.schema,
                    ObjectKind::CheckConstraint,
                    &domain_key.object_name,
                    Some(constraint.name.clone()),
                );
                metadata.objects.push(MetadataObject {
                    key,
                    parent_key: Some(domain_key.clone()),
                    name: constraint.name.clone(),
                    extension_kind: None,
                    definition: constraint.definition.clone(),
                    properties: constraint_properties(constraint),
                });
                continue;
            }

            let relation_oid = constraint.relation_oid.ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "constraint {} has neither relation nor domain parent",
                    constraint.name
                ))
            })?;
            let table_key = required(
                table_keys.get(&relation_oid),
                format!(
                    "table parent oid {relation_oid} for constraint {}",
                    constraint.name
                ),
            )?;
            if constraint.kind == 'x' {
                let key = pg_key(
                    self.source_kind,
                    self.connection_alias,
                    &database_name,
                    &constraint.schema,
                    ObjectKind::ExclusionConstraint,
                    &table_key.object_name,
                    Some(constraint.name.clone()),
                );
                metadata.objects.push(MetadataObject {
                    key: key.clone(),
                    parent_key: Some(table_key.clone()),
                    name: constraint.name.clone(),
                    extension_kind: None,
                    definition: constraint.definition.clone(),
                    properties: constraint_properties(constraint),
                });
                for (ordinal, column_number) in constraint.columns.iter().enumerate() {
                    if *column_number <= 0 {
                        continue;
                    }
                    let column_key = required(
                        column_keys.get(&(relation_oid, i32::from(*column_number))),
                        format!(
                            "exclusion constraint column {} at ordinal {}",
                            constraint.name,
                            ordinal + 1
                        ),
                    )?;
                    metadata.relationships.push(MetadataRelationship {
                        kind: MetadataRelationshipKind::ExcludesWith,
                        from_key: key.clone(),
                        to_key: column_key.clone(),
                        ordinal: Some((ordinal + 1) as u32),
                        properties: BTreeMap::new(),
                    });
                }
                continue;
            }

            let (kind, object_kind) = match constraint.kind {
                'p' => (ConstraintKind::PrimaryKey, ObjectKind::PrimaryKey),
                'u' => (ConstraintKind::Unique, ObjectKind::UniqueConstraint),
                'f' => (ConstraintKind::ForeignKey, ObjectKind::ForeignKey),
                'c' => (ConstraintKind::Check, ObjectKind::CheckConstraint),
                other => {
                    return Err(CatalogError::Mapping(format!(
                        "unmapped pg_catalog constraint kind '{other}' for {}",
                        constraint.name
                    )));
                }
            };
            let local_columns = resolve_columns(
                relation_oid,
                &constraint.columns,
                &column_keys,
                &constraint.name,
            )?;
            let (referenced_table_key, referenced_columns) = if kind == ConstraintKind::ForeignKey {
                let referenced_oid = constraint.referenced_relation_oid.ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "foreign key {} has no referenced relation",
                        constraint.name
                    ))
                })?;
                let referenced_table = required(
                    table_keys.get(&referenced_oid),
                    format!(
                        "foreign key {} references a table outside the certified schema scope (oid {referenced_oid})",
                        constraint.name
                    ),
                )?;
                let referenced = resolve_columns(
                    referenced_oid,
                    &constraint.referenced_columns,
                    &column_keys,
                    &constraint.name,
                )?;
                (Some(referenced_table.clone()), referenced)
            } else {
                (None, Vec::new())
            };
            let key = pg_key(
                self.source_kind,
                self.connection_alias,
                &database_name,
                &constraint.schema,
                object_kind,
                &table_key.object_name,
                Some(constraint.name.clone()),
            );
            constraints.push(ConstraintObject {
                key: key.clone(),
                table_key: table_key.clone(),
                name: constraint.name.clone(),
                kind,
                columns: local_columns,
                referenced_table_key,
                referenced_columns,
                expression: (kind == ConstraintKind::Check)
                    .then(|| constraint.definition.clone())
                    .flatten(),
            });
            metadata.annotations.push(ObjectAnnotation {
                object_key: key,
                definition: None,
                properties: constraint_properties(constraint),
            });
        }

        let terms_by_index = group_index_terms(&raw.index_terms);
        let relation_kind_by_oid = raw
            .relations
            .iter()
            .map(|relation| (relation.oid, relation.relkind))
            .collect::<BTreeMap<_, _>>();
        let mut indexes = Vec::new();
        for index in &raw.indexes {
            let terms = terms_by_index.get(&index.oid).cloned().unwrap_or_default();
            if terms.is_empty() {
                return Err(CatalogError::Mapping(format!(
                    "index {}.{}.{} has no catalog terms",
                    index.schema, index.relation, index.name
                )));
            }
            let relation_kind = required(
                relation_kind_by_oid.get(&index.relation_oid),
                format!("indexed relation oid {}", index.relation_oid),
            )?;
            match *relation_kind {
                'r' | 'p' | 'f' => {
                    let table_key = required(
                        table_keys.get(&index.relation_oid),
                        format!("indexed table oid {}", index.relation_oid),
                    )?;
                    let key = pg_key(
                        self.source_kind,
                        self.connection_alias,
                        &database_name,
                        &index.schema,
                        ObjectKind::Index,
                        &index.relation,
                        Some(index.name.clone()),
                    );
                    if physical_relation_keys
                        .insert(index.oid, key.clone())
                        .is_some()
                    {
                        return Err(CatalogError::Mapping(format!(
                            "duplicate physical relation oid {} for index {}",
                            index.oid, index.name
                        )));
                    }
                    let mut key_columns = Vec::new();
                    for term in &terms {
                        if term.is_key && term.column_number > 0 {
                            let column_key = required(
                                column_keys
                                    .get(&(index.relation_oid, i32::from(term.column_number))),
                                format!(
                                    "index key column for {} ordinal {}",
                                    index.name, term.ordinal
                                ),
                            )?
                            .clone();
                            if !key_columns.contains(&column_key) {
                                key_columns.push(column_key);
                            }
                        }
                    }
                    indexes.push(IndexObject {
                        key: key.clone(),
                        table_key: table_key.clone(),
                        name: index.name.clone(),
                        columns: key_columns,
                        is_unique: index.unique,
                        is_primary: index.primary,
                        predicate: index.predicate.clone(),
                        expression: index.expression.clone(),
                    });
                    metadata.annotations.push(ObjectAnnotation {
                        object_key: key.clone(),
                        definition: index.definition.clone(),
                        properties: index_properties(index, &terms),
                    });
                    add_included_columns(
                        &mut metadata.relationships,
                        &key,
                        index,
                        &terms,
                        &column_keys,
                        false,
                    )?;
                }
                'm' => {
                    let parent = required(
                        materialized_view_keys.get(&index.relation_oid),
                        format!("materialized view parent for index {}", index.name),
                    )?;
                    let key = pg_key(
                        self.source_kind,
                        self.connection_alias,
                        &database_name,
                        &index.schema,
                        ObjectKind::Index,
                        &index.relation,
                        Some(index.name.clone()),
                    );
                    if physical_relation_keys
                        .insert(index.oid, key.clone())
                        .is_some()
                    {
                        return Err(CatalogError::Mapping(format!(
                            "duplicate physical relation oid {} for index {}",
                            index.oid, index.name
                        )));
                    }
                    metadata.objects.push(MetadataObject {
                        key: key.clone(),
                        parent_key: Some(parent.clone()),
                        name: index.name.clone(),
                        extension_kind: None,
                        definition: index.definition.clone(),
                        properties: index_properties(index, &terms),
                    });
                    add_included_columns(
                        &mut metadata.relationships,
                        &key,
                        index,
                        &terms,
                        &column_keys,
                        true,
                    )?;
                }
                other => {
                    return Err(CatalogError::Mapping(format!(
                        "index {} belongs to unsupported relation kind '{other}'",
                        index.name
                    )));
                }
            }
        }

        if let Some(yugabyte) = &raw.yugabyte {
            map_yugabyte_metadata(
                &mut metadata,
                yugabyte,
                self.source_kind,
                self.connection_alias,
                &database_name,
                &database_key,
                &principal_keys,
                &physical_relation_keys,
            )?;
        }

        let view_position_by_oid = views
            .iter()
            .enumerate()
            .map(|(position, view)| {
                let oid = view_keys
                    .iter()
                    .find_map(|(oid, key)| (key == &view.key).then_some(*oid))
                    .expect("view key was inserted with its oid");
                (oid, position)
            })
            .collect::<BTreeMap<_, _>>();
        let mut view_dependency_ordinals = BTreeMap::<i64, u32>::new();
        for dependency in &raw.view_dependencies {
            let Some(target_key) = resolve_relation_dependency(
                dependency.target_relation_oid,
                dependency.target_column_number,
                &dependency.target_schema,
                &relation_keys,
                &column_keys,
            )?
            else {
                continue;
            };
            let owner_key = view_keys
                .get(&dependency.view_oid)
                .or_else(|| materialized_view_keys.get(&dependency.view_oid))
                .ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "view dependency owner oid {} is not mapped",
                        dependency.view_oid
                    ))
                })?;
            if let Some(position) = view_position_by_oid.get(&dependency.view_oid) {
                if is_base_snapshot_kind(target_key.object_kind) {
                    views[*position].depends_on.push(target_key.clone());
                } else {
                    metadata.relationships.push(MetadataRelationship {
                        kind: MetadataRelationshipKind::DependsOn,
                        from_key: owner_key.clone(),
                        to_key: target_key.clone(),
                        ordinal: None,
                        properties: BTreeMap::new(),
                    });
                }
            } else if let Some(materialized_key) = materialized_view_keys.get(&dependency.view_oid)
            {
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::DependsOn,
                    from_key: materialized_key.clone(),
                    to_key: target_key.clone(),
                    ordinal: None,
                    properties: BTreeMap::new(),
                });
            } else {
                return Err(CatalogError::Mapping(format!(
                    "view dependency owner oid {} is not mapped",
                    dependency.view_oid
                )));
            }
            let ordinal = view_dependency_ordinals
                .entry(dependency.view_oid)
                .and_modify(|value| *value += 1)
                .or_insert(1);
            let mut properties = BTreeMap::new();
            insert_string(
                &mut properties,
                "postgres_dependency_type",
                dependency.dependency_type.to_string(),
            );
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::Extension("postgres_catalog_dependency".to_owned()),
                from_key: owner_key.clone(),
                to_key: target_key,
                ordinal: Some(*ordinal),
                properties,
            });
        }

        let mut routine_keys = BTreeMap::new();
        for routine in &raw.routines {
            let key = pg_key(
                self.source_kind,
                self.connection_alias,
                &database_name,
                &routine.schema,
                ObjectKind::Routine,
                &routine.name,
                Some(routine.identity_arguments.clone()),
            );
            if routine_keys.insert(routine.oid, key).is_some() {
                return Err(CatalogError::Mapping(format!(
                    "duplicate pg_catalog routine oid {}",
                    routine.oid
                )));
            }
        }
        let mut routines = Vec::new();
        let mut routine_position_by_oid = BTreeMap::new();
        for routine in &raw.routines {
            let schema_key = required(
                schema_keys.get(&routine.schema),
                format!("schema key for routine {}.{}", routine.schema, routine.name),
            )?;
            let key = required(
                routine_keys.get(&routine.oid),
                format!("routine key oid {}", routine.oid),
            )?;
            routine_position_by_oid.insert(routine.oid, routines.len());
            routines.push(RoutineObject {
                key: key.clone(),
                schema_key: schema_key.clone(),
                name: routine.name.clone(),
                kind: if routine.kind == 'p' {
                    RoutineKind::Procedure
                } else {
                    RoutineKind::Function
                },
                definition: routine.definition.clone(),
                depends_on: Vec::new(),
            });
            metadata.annotations.push(ObjectAnnotation {
                object_key: key.clone(),
                definition: None,
                properties: routine_properties(routine),
            });
            add_owned_by(
                &mut metadata.relationships,
                key,
                routine.owner_oid,
                &principal_keys,
                "routine",
            )?;
            if let Some(return_type_key) = type_keys.get(&routine.return_type_oid) {
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::ReturnsType,
                    from_key: key.clone(),
                    to_key: return_type_key.clone(),
                    ordinal: None,
                    properties: BTreeMap::new(),
                });
            } else if !is_system_schema(&routine.return_type_schema) {
                return Err(CatalogError::Mapping(format!(
                    "routine {}.{} returns type outside the certified schema scope (type oid {})",
                    routine.schema, routine.name, routine.return_type_oid
                )));
            }
        }

        for parameter in &raw.routine_parameters {
            let routine_key = required(
                routine_keys.get(&parameter.routine_oid),
                format!("parameter parent routine oid {}", parameter.routine_oid),
            )?;
            let ordinal = u32::try_from(parameter.ordinal).map_err(|_| {
                CatalogError::Mapping(format!(
                    "routine parameter ordinal {} is invalid",
                    parameter.ordinal
                ))
            })?;
            if ordinal == 0 {
                return Err(CatalogError::Mapping(
                    "routine parameter ordinal cannot be zero".to_owned(),
                ));
            }
            let display_name = parameter
                .name
                .clone()
                .unwrap_or_else(|| format!("argument_{ordinal}"));
            let identity = format!(
                "{}#{}:{}",
                routine_key.sub_object.as_deref().unwrap_or_default(),
                ordinal,
                display_name
            );
            let key = pg_key(
                self.source_kind,
                self.connection_alias,
                &database_name,
                &routine_key.schema,
                ObjectKind::RoutineParameter,
                &routine_key.object_name,
                Some(identity),
            );
            let mut properties = BTreeMap::new();
            insert_u64(&mut properties, "ordinal_position", ordinal as u64);
            insert_string(
                &mut properties,
                "mode",
                routine_parameter_mode(parameter.mode),
            );
            insert_string(&mut properties, "data_type", &parameter.data_type);
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(routine_key.clone()),
                name: display_name,
                extension_kind: None,
                definition: None,
                properties,
            });
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::HasParameter,
                from_key: routine_key.clone(),
                to_key: key.clone(),
                ordinal: Some(ordinal),
                properties: BTreeMap::new(),
            });
            add_type_use(
                &mut metadata.relationships,
                &key,
                parameter.type_oid,
                &parameter.type_schema,
                &type_keys,
            )?;
        }

        let mut routine_dependency_ordinals = BTreeMap::<i64, u32>::new();
        for dependency in &raw.routine_dependencies {
            let position = required(
                routine_position_by_oid.get(&dependency.owner_oid),
                format!("routine dependency owner oid {}", dependency.owner_oid),
            )?;
            let Some(target) = resolve_routine_dependency(
                dependency,
                &relation_keys,
                &column_keys,
                &routine_keys,
                &type_keys,
            )?
            else {
                continue;
            };
            routines[*position].depends_on.push(target.clone());
            let ordinal = routine_dependency_ordinals
                .entry(dependency.owner_oid)
                .and_modify(|value| *value += 1)
                .or_insert(1);
            let mut properties = BTreeMap::new();
            insert_string(
                &mut properties,
                "postgres_dependency_type",
                dependency.dependency_type.to_string(),
            );
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::Extension("postgres_catalog_dependency".to_owned()),
                from_key: routines[*position].key.clone(),
                to_key: target,
                ordinal: Some(*ordinal),
                properties,
            });
        }

        let mut triggers = Vec::new();
        for trigger in &raw.triggers {
            let relation_key = required(
                relation_keys.get(&trigger.relation_oid),
                format!("trigger target relation oid {}", trigger.relation_oid),
            )?;
            if !matches!(
                relation_key.object_kind,
                ObjectKind::Table | ObjectKind::View
            ) {
                return Err(CatalogError::Mapping(format!(
                    "trigger {} target kind {} is unsupported",
                    trigger.name, relation_key.object_kind
                )));
            }
            let routine_key = routine_keys.get(&trigger.routine_oid).cloned();
            if routine_key.is_none() && !raw.extension_routine_oids.contains(&trigger.routine_oid) {
                return Err(CatalogError::Mapping(format!(
                    "trigger {} invokes routine outside the certified schema scope (oid {})",
                    trigger.name, trigger.routine_oid
                )));
            }
            let key = pg_key(
                self.source_kind,
                self.connection_alias,
                &database_name,
                &relation_key.schema,
                ObjectKind::Trigger,
                &relation_key.object_name,
                Some(trigger.name.clone()),
            );
            triggers.push(TriggerObject {
                key: key.clone(),
                table_key: relation_key.clone(),
                name: trigger.name.clone(),
                timing: Some(trigger.timing.clone()),
                events: trigger.events.clone(),
                definition: trigger.definition.clone(),
                executes_routine_key: routine_key,
            });
            metadata.annotations.push(ObjectAnnotation {
                object_key: key,
                definition: None,
                properties: trigger_properties(trigger, &column_keys)?,
            });
        }

        for inheritance in &raw.inheritance {
            let child = required(
                table_keys.get(&inheritance.child_oid),
                format!("inheritance child table oid {}", inheritance.child_oid),
            )?;
            let parent = required(
                table_keys.get(&inheritance.parent_oid),
                format!(
                    "table {} inherits from a parent outside the certified schema scope (oid {})",
                    child.object_name, inheritance.parent_oid
                ),
            )?;
            let mut properties = BTreeMap::new();
            insert_i64(
                &mut properties,
                "sequence_number",
                i64::from(inheritance.sequence_number),
            );
            metadata.relationships.push(MetadataRelationship {
                kind: if inheritance.child_is_partition {
                    MetadataRelationshipKind::PartitionOf
                } else {
                    MetadataRelationshipKind::InheritsFrom
                },
                from_key: child.clone(),
                to_key: parent.clone(),
                ordinal: None,
                properties,
            });
        }

        for usage in &raw.sequence_usages {
            let column = required(
                column_keys.get(&(usage.column_relation_oid, usage.column_number)),
                format!(
                    "sequence usage source column {}:{}",
                    usage.column_relation_oid, usage.column_number
                ),
            )?;
            if column.object_kind != ObjectKind::Column {
                return Err(CatalogError::Mapping(format!(
                    "sequence usage source {} is not a table column",
                    column
                )));
            }
            let sequence = required(
                sequence_keys.get(&usage.sequence_oid),
                format!(
                    "column {} uses sequence outside the certified schema scope (oid {})",
                    column, usage.sequence_oid
                ),
            )?;
            let mut properties = BTreeMap::new();
            insert_string(
                &mut properties,
                "postgres_dependency_type",
                usage.dependency_type.to_string(),
            );
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::UsesSequence,
                from_key: column.clone(),
                to_key: sequence.clone(),
                ordinal: None,
                properties,
            });
        }

        add_type_relationships(
            &raw,
            &mut metadata.relationships,
            &type_keys,
            &relation_keys,
        )?;

        for policy in &raw.policies {
            let parent = required(
                relation_keys.get(&policy.relation_oid),
                format!("policy target relation oid {}", policy.relation_oid),
            )?;
            let key = pg_key(
                self.source_kind,
                self.connection_alias,
                &database_name,
                &parent.schema,
                ObjectKind::Policy,
                &parent.object_name,
                Some(policy.name.clone()),
            );
            let mut properties = BTreeMap::new();
            insert_i64(&mut properties, "postgres_oid", policy.oid);
            insert_string(&mut properties, "command", policy_command(policy.command));
            insert_bool(&mut properties, "permissive", policy.permissive);
            insert_optional_string(
                &mut properties,
                "using_expression",
                policy.using_expression.as_deref(),
            );
            insert_optional_string(
                &mut properties,
                "check_expression",
                policy.check_expression.as_deref(),
            );
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(parent.clone()),
                name: policy.name.clone(),
                extension_kind: None,
                definition: None,
                properties,
            });
            for role_oid in &policy.role_oids {
                if *role_oid == 0 {
                    continue;
                }
                let role = required(
                    principal_keys.get(role_oid),
                    format!("policy {} role oid {role_oid}", policy.name),
                )?;
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::Extension("postgres_policy_role".to_owned()),
                    from_key: key.clone(),
                    to_key: role.clone(),
                    ordinal: None,
                    properties: BTreeMap::new(),
                });
            }
        }

        for extension in &raw.extensions {
            let key = pg_key(
                self.source_kind,
                self.connection_alias,
                &database_name,
                &database_name,
                ObjectKind::Extension,
                &extension.name,
                None,
            );
            let mut properties = BTreeMap::new();
            insert_i64(&mut properties, "postgres_oid", extension.oid);
            insert_optional_string(&mut properties, "schema", extension.schema.as_deref());
            insert_bool(&mut properties, "relocatable", extension.relocatable);
            insert_string(&mut properties, "version", &extension.version);
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(database_key.clone()),
                name: extension.name.clone(),
                extension_kind: Some("postgres_extension".to_owned()),
                definition: None,
                properties,
            });
            add_owned_by(
                &mut metadata.relationships,
                &key,
                extension.owner_oid,
                &principal_keys,
                "extension",
            )?;
        }

        for event in &raw.event_triggers {
            let routine = routine_keys.get(&event.routine_oid).cloned();
            if routine.is_none()
                && !raw.extension_routine_oids.contains(&event.routine_oid)
                && !is_system_schema(&event.routine_schema)
            {
                return Err(CatalogError::Mapping(format!(
                    "event trigger {} invokes routine outside the certified schema scope ({}. oid {})",
                    event.name, event.routine_schema, event.routine_oid
                )));
            }
            let key = pg_key(
                self.source_kind,
                self.connection_alias,
                &database_name,
                &database_name,
                ObjectKind::Event,
                &event.name,
                None,
            );
            let mut properties = BTreeMap::new();
            insert_i64(&mut properties, "postgres_oid", event.oid);
            insert_string(&mut properties, "event", &event.event);
            insert_string(&mut properties, "enabled", event.enabled.to_string());
            properties.insert(
                "tags".to_owned(),
                MetadataValue::StringList(event.tags.clone()),
            );
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(database_key.clone()),
                name: event.name.clone(),
                extension_kind: None,
                definition: None,
                properties,
            });
            add_owned_by(
                &mut metadata.relationships,
                &key,
                event.owner_oid,
                &principal_keys,
                "event trigger",
            )?;
            if let Some(routine) = routine {
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::Invokes,
                    from_key: key,
                    to_key: routine,
                    ordinal: None,
                    properties: BTreeMap::new(),
                });
            }
        }

        deduplicate_metadata_relationships(&mut metadata.relationships)?;
        let snapshot = CanonicalSchemaSnapshot {
            schema: SchemaSnapshot {
                source_kind: self.source_kind.to_owned(),
                connection_alias: self.connection_alias.to_owned(),
                database,
                schemas,
                tables,
                columns,
                constraints,
                indexes,
                views,
                triggers,
                routines,
                capabilities: pg_catalog_complete_capabilities(strategy, &raw),
            },
            metadata,
        };
        let discovered_counts = discovery_counts_from_catalog(&raw, &snapshot)?;
        let mut scope_schemas = raw
            .schemas
            .iter()
            .map(|schema| schema.name.clone())
            .collect::<Vec<_>>();
        scope_schemas.sort();
        let mut capability_checks = vec![
            CapabilityCheck {
                name: "supported_server_version".to_owned(),
                evidence: format!(
                    "server_version={} and server_version_num={} map to certified {} strategy {}",
                    raw.server.version,
                    raw.server.version_num,
                    strategy.product_name(),
                    strategy.strategy_name()
                ),
            },
            CapabilityCheck {
                name: "read_only_repeatable_read_transaction".to_owned(),
                evidence: format!(
                    "transaction_read_only={} and transaction_isolation={}",
                    raw.server.transaction_read_only, raw.server.transaction_isolation
                ),
            },
            CapabilityCheck {
                name: "schema_visibility".to_owned(),
                evidence: format!(
                    "has_schema_privilege(..., USAGE) succeeded for {} requested schema(s)",
                    scope_schemas.len()
                ),
            },
            CapabilityCheck {
                name: "metadata_only_catalog_queries".to_owned(),
                evidence: "adapter queried pg_catalog metadata and server information only; no application relation appears in a FROM clause"
                    .to_owned(),
            },
            CapabilityCheck {
                name: "routine_dependency_proof".to_owned(),
                evidence: format!(
                    "{} selected routine(s) have catalog-proven dependency bodies; {} opaque routine(s) remain boundary objects and emit no guessed edges",
                    raw.routines
                        .iter()
                        .filter(|routine| routine.body_catalog_tracked)
                        .count(),
                    raw.routines
                        .iter()
                        .filter(|routine| !routine.body_catalog_tracked)
                        .count()
                ),
            },
            CapabilityCheck {
                name: "principal_context".to_owned(),
                evidence: format!(
                    "current_user={} session_user={} and pg_roles inventory was readable",
                    raw.server.current_user, raw.server.session_user
                ),
            },
            CapabilityCheck {
                name: "transport_security".to_owned(),
                evidence: if raw.server.tls {
                    format!(
                        "TLS enabled version={} cipher={}",
                        raw.server.tls_version.as_deref().unwrap_or("reported"),
                        raw.server.tls_cipher.as_deref().unwrap_or("reported")
                    )
                } else {
                    "plaintext transport accepted only for a loopback/local connection".to_owned()
                },
            },
        ];
        if let Some(yugabyte) = &raw.yugabyte {
            capability_checks.push(CapabilityCheck {
                name: "yugabytedb_distributed_metadata".to_owned(),
                evidence: format!(
                    "yb_table_properties certified {} physical relation(s), including tablet count, hash-key count, colocation, tablegroup, and range split metadata",
                    yugabyte.relation_properties.len()
                ),
            });
            capability_checks.push(CapabilityCheck {
                name: "yugabytedb_placement_metadata".to_owned(),
                evidence: format!(
                    "pg_yb_tablegroup and pg_tablespace certified {} tablegroup(s), {} tablespace(s), database_colocated={}",
                    yugabyte.tablegroups.len(),
                    yugabyte.tablespaces.len(),
                    yugabyte.database_colocated
                ),
            });
        }

        Ok(CatalogDiscovery {
            snapshot,
            adapter: AdapterIdentity {
                name: strategy.adapter_name().to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
            },
            server: ServerIdentity {
                product: strategy.product_name().to_owned(),
                version: raw.server.version.clone(),
            },
            scope: IntrospectionScope {
                catalogs: vec![database_name],
                schemas: scope_schemas.clone(),
            },
            discovered_counts,
            capability_checks,
        })
    }
}

