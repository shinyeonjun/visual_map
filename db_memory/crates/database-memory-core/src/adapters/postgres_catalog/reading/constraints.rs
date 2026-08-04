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
