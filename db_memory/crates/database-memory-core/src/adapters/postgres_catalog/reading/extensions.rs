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
