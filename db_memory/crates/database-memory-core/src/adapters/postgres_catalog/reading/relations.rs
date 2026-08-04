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
