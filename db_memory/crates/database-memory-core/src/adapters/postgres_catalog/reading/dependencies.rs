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
