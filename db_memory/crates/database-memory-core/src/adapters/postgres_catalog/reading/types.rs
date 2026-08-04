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
