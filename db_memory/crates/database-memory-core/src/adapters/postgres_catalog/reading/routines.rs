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
