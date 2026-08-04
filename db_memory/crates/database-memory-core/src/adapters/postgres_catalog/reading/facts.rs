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
