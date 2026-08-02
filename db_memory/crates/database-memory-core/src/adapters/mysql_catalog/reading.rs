impl RawMysqlFamilyCatalog {
    fn read<Q: Queryable>(
        connection: &mut Q,
        facts: &ServerFacts,
        strategy: MysqlFamilyVersion,
    ) -> Result<Self, CatalogError> {
        let active_roles = read_active_roles(connection, strategy)?;
        let grants = read_effective_privileges(connection, facts)?;
        require_metadata_privileges(&grants)?;
        // The mysql driver reached this reader only after the server accepted
        // SET TRANSACTION READ ONLY and REPEATABLE READ. The @@tx_read_only
        // variables expose session defaults, not the active transaction mode.
        let transaction_read_only = true;
        let transaction_isolation = "REPEATABLE-READ".to_owned();
        check_definition_sizes(connection, &facts.database)?;
        let before = CatalogSignature::read(connection, &facts.database, strategy)?;

        let tables = read_tables(connection, &facts.database)?;
        let columns = read_columns(connection, &facts.database, strategy)?;
        let constraints = read_constraints(connection, &facts.database, strategy)?;
        let key_usage = read_key_usage(connection, &facts.database)?;
        let reference_rules = read_reference_rules(connection, &facts.database)?;
        let checks = read_checks(connection, &facts.database, strategy)?;
        let index_parts = read_index_parts(connection, &facts.database, strategy)?;
        let views = read_views(connection, &facts.database, strategy)?;
        let view_table_usage = read_view_table_usage(connection, &facts.database, strategy)?;
        let view_routine_usage = read_view_routine_usage(connection, &facts.database, strategy)?;
        let routines = read_routines(connection, &facts.database)?;
        let parameters = read_parameters(connection, &facts.database, strategy)?;
        let triggers = read_triggers(connection, &facts.database)?;
        let events = read_events(connection, &facts.database)?;
        let partitions = read_partitions(connection, &facts.database)?;
        let sequences = read_sequences(connection, &facts.database, strategy, &tables)?;

        let after = CatalogSignature::read(connection, &facts.database, strategy)?;
        require_stable_signature(&before, &after)?;

        Ok(Self {
            facts: facts.clone(),
            strategy,
            grants,
            active_roles,
            transaction_read_only,
            transaction_isolation,
            tables,
            columns,
            constraints,
            key_usage,
            reference_rules,
            checks,
            index_parts,
            views,
            view_table_usage,
            view_routine_usage,
            routines,
            parameters,
            triggers,
            events,
            partitions,
            sequences,
        })
    }
}

fn require_stable_signature(
    before: &CatalogSignature,
    after: &CatalogSignature,
) -> Result<(), CatalogError> {
    if before == after {
        Ok(())
    } else {
        Err(CatalogError::ConcurrentDdl(
            "the selected database catalog changed during introspection".to_owned(),
        ))
    }
}

fn read_active_roles<Q: Queryable>(
    connection: &mut Q,
    strategy: MysqlFamilyVersion,
) -> Result<Vec<String>, CatalogError> {
    let sql = match strategy.product() {
        MysqlProduct::Mysql => {
            "SELECT CONCAT(ROLE_NAME, '@', ROLE_HOST) AS role_name \
             FROM INFORMATION_SCHEMA.ENABLED_ROLES ORDER BY ROLE_NAME, ROLE_HOST"
        }
        MysqlProduct::MariaDb => {
            "SELECT ROLE_NAME AS role_name FROM INFORMATION_SCHEMA.ENABLED_ROLES \
             WHERE ROLE_NAME IS NOT NULL ORDER BY ROLE_NAME"
        }
    };
    connection
        .query::<Row, _>(sql)?
        .into_iter()
        .map(|row| required(&row, "role_name"))
        .collect()
}

fn read_effective_privileges<Q: Queryable>(
    connection: &mut Q,
    facts: &ServerFacts,
) -> Result<BTreeSet<String>, CatalogError> {
    let rows = connection.query::<Row, _>("SHOW GRANTS")?;
    let mut privileges = BTreeSet::new();
    for row in rows {
        let grant: String = required_at(&row, 0)?;
        if let Some(parsed) = parse_schema_grant(&grant, &facts.database)? {
            privileges.extend(parsed);
        }
    }
    Ok(privileges)
}

fn parse_schema_grant(
    grant: &str,
    database: &str,
) -> Result<Option<BTreeSet<String>>, CatalogError> {
    let Some(body) = grant.strip_prefix("GRANT ") else {
        return Ok(None);
    };
    let Some(on_offset) = body.find(" ON ") else {
        return Ok(None);
    };
    let privileges_text = &body[..on_offset];
    let scoped = &body[on_offset + 4..];
    let Some(to_offset) = scoped.find(" TO ") else {
        return Err(CatalogError::Mapping(format!(
            "SHOW GRANTS row has ON without TO: {grant}"
        )));
    };
    let scope = &scoped[..to_offset];
    if !grant_scope_matches(scope, database)? {
        return Ok(None);
    }
    let mut privileges = BTreeSet::new();
    if privileges_text.eq_ignore_ascii_case("ALL PRIVILEGES")
        || privileges_text.eq_ignore_ascii_case("ALL")
    {
        privileges.extend(
            ["SELECT", "SHOW VIEW", "EXECUTE", "EVENT", "TRIGGER"]
                .into_iter()
                .map(str::to_owned),
        );
        privileges.insert("ALL PRIVILEGES".to_owned());
        return Ok(Some(privileges));
    }
    for privilege in privileges_text.split(',') {
        let privilege = privilege.trim().to_ascii_uppercase();
        if privilege.is_empty()
            || privilege.chars().any(|character| {
                !(character.is_ascii_alphabetic() || character == ' ' || character == '_')
            })
        {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "SHOW GRANTS contains an unrecognized schema privilege token '{privilege}'"
            )));
        }
        privileges.insert(privilege);
    }
    Ok(Some(privileges))
}

fn grant_scope_matches(scope: &str, database: &str) -> Result<bool, CatalogError> {
    if scope == "*.*" {
        return Ok(true);
    }
    if !scope.starts_with('`') {
        return Ok(false);
    }
    let (identifier, rest) = parse_backtick_identifier(scope)?;
    if rest != ".*" {
        return Ok(false);
    }
    Ok(identifier == database)
}

fn parse_backtick_identifier(value: &str) -> Result<(String, &str), CatalogError> {
    let Some(mut remaining) = value.strip_prefix('`') else {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "SHOW GRANTS scope '{value}' is not a server-quoted identifier"
        )));
    };
    let mut identifier = String::new();
    loop {
        let Some(position) = remaining.find('`') else {
            return Err(CatalogError::Mapping(format!(
                "SHOW GRANTS scope '{value}' has an unterminated identifier"
            )));
        };
        identifier.push_str(&remaining[..position]);
        remaining = &remaining[position + 1..];
        if let Some(after_escape) = remaining.strip_prefix('`') {
            identifier.push('`');
            remaining = after_escape;
        } else {
            return Ok((identifier, remaining));
        }
    }
}

fn normalize_principal(value: &str) -> String {
    value
        .chars()
        .filter(|character| !matches!(character, '`' | '\'' | '"' | ' '))
        .collect::<String>()
        .to_ascii_lowercase()
}

fn require_metadata_privileges(privileges: &BTreeSet<String>) -> Result<(), CatalogError> {
    let required = ["SELECT", "SHOW VIEW", "EXECUTE", "EVENT", "TRIGGER"];
    let missing = required
        .iter()
        .filter(|privilege| !privileges.contains(**privilege))
        .copied()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(CatalogError::PermissionDenied(format!(
            "the selected database lacks schema-wide metadata visibility privileges: {}",
            missing.join(", ")
        )))
    }
}

fn check_definition_sizes<Q: Queryable>(
    connection: &mut Q,
    database: &str,
) -> Result<(), CatalogError> {
    let rows = connection.exec::<Row, _, _>(
        "SELECT 'view' AS object_kind, TABLE_NAME AS object_name, \
                OCTET_LENGTH(VIEW_DEFINITION) AS definition_bytes \
         FROM INFORMATION_SCHEMA.VIEWS WHERE TABLE_SCHEMA = ? \
           AND OCTET_LENGTH(VIEW_DEFINITION) > 1048576 \
         UNION ALL \
         SELECT 'routine', SPECIFIC_NAME, OCTET_LENGTH(ROUTINE_DEFINITION) \
         FROM INFORMATION_SCHEMA.ROUTINES WHERE ROUTINE_SCHEMA = ? \
           AND OCTET_LENGTH(ROUTINE_DEFINITION) > 1048576 \
         UNION ALL \
         SELECT 'trigger', TRIGGER_NAME, OCTET_LENGTH(ACTION_STATEMENT) \
         FROM INFORMATION_SCHEMA.TRIGGERS WHERE TRIGGER_SCHEMA = ? \
           AND OCTET_LENGTH(ACTION_STATEMENT) > 1048576 \
         UNION ALL \
         SELECT 'event', EVENT_NAME, OCTET_LENGTH(EVENT_DEFINITION) \
         FROM INFORMATION_SCHEMA.EVENTS WHERE EVENT_SCHEMA = ? \
           AND OCTET_LENGTH(EVENT_DEFINITION) > 1048576",
        (database, database, database, database),
    )?;
    if let Some(row) = rows.first() {
        let kind: String = required(row, "object_kind")?;
        let name: String = required(row, "object_name")?;
        let bytes: u64 = required(row, "definition_bytes")?;
        return Err(CatalogError::UnsupportedMetadata(format!(
            "{kind} '{name}' definition is {bytes} bytes and exceeds the {MAX_DEFINITION_BYTES}-byte safety limit"
        )));
    }
    Ok(())
}

fn read_tables<Q: Queryable>(
    connection: &mut Q,
    database: &str,
) -> Result<Vec<RawTable>, CatalogError> {
    connection
        .exec::<Row, _, _>(
            "SELECT TABLE_NAME, TABLE_TYPE, ENGINE, ROW_FORMAT, TABLE_COLLATION, \
                    CREATE_OPTIONS, COALESCE(TABLE_COMMENT, '') AS TABLE_COMMENT \
             FROM INFORMATION_SCHEMA.TABLES WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME",
            (database,),
        )?
        .into_iter()
        .map(|row| {
            Ok(RawTable {
                name: required(&row, "TABLE_NAME")?,
                table_type: required(&row, "TABLE_TYPE")?,
                engine: optional(&row, "ENGINE")?,
                row_format: optional(&row, "ROW_FORMAT")?,
                collation: optional(&row, "TABLE_COLLATION")?,
                create_options: optional(&row, "CREATE_OPTIONS")?,
                comment: required(&row, "TABLE_COMMENT")?,
            })
        })
        .collect()
}

fn read_columns<Q: Queryable>(
    connection: &mut Q,
    database: &str,
    strategy: MysqlFamilyVersion,
) -> Result<Vec<RawColumn>, CatalogError> {
    let (spatial, period_start, period_end) = match strategy {
        MysqlFamilyVersion::Mysql80 | MysqlFamilyVersion::Mysql84 | MysqlFamilyVersion::Mysql97 => {
            (
                "SRS_ID",
                "'NO' AS IS_SYSTEM_TIME_PERIOD_START",
                "'NO' AS IS_SYSTEM_TIME_PERIOD_END",
            )
        }
        MysqlFamilyVersion::MariaDb123 => (
            "NULL AS SRS_ID",
            "IS_SYSTEM_TIME_PERIOD_START",
            "IS_SYSTEM_TIME_PERIOD_END",
        ),
        MysqlFamilyVersion::MariaDb1011
        | MysqlFamilyVersion::MariaDb114
        | MysqlFamilyVersion::MariaDb118 => (
            "NULL AS SRS_ID",
            "'NO' AS IS_SYSTEM_TIME_PERIOD_START",
            "'NO' AS IS_SYSTEM_TIME_PERIOD_END",
        ),
    };
    let sql = format!(
        "SELECT TABLE_NAME, COLUMN_NAME, ORDINAL_POSITION, DATA_TYPE, COLUMN_TYPE, \
                IS_NULLABLE, COLUMN_DEFAULT, CHARACTER_SET_NAME, COLLATION_NAME, EXTRA, \
                PRIVILEGES, COALESCE(COLUMN_COMMENT, '') AS COLUMN_COMMENT, \
                GENERATION_EXPRESSION, {spatial}, {period_start}, {period_end} \
         FROM INFORMATION_SCHEMA.COLUMNS WHERE TABLE_SCHEMA = ? \
         ORDER BY TABLE_NAME, ORDINAL_POSITION"
    );
    connection
        .exec::<Row, _, _>(sql, (database,))?
        .into_iter()
        .map(|row| {
            let nullable: String = required(&row, "IS_NULLABLE")?;
            let period_start: String = required(&row, "IS_SYSTEM_TIME_PERIOD_START")?;
            let period_end: String = required(&row, "IS_SYSTEM_TIME_PERIOD_END")?;
            Ok(RawColumn {
                table: required(&row, "TABLE_NAME")?,
                name: required(&row, "COLUMN_NAME")?,
                ordinal: u32_from_u64(required(&row, "ORDINAL_POSITION")?, "column ordinal")?,
                data_type: required(&row, "DATA_TYPE")?,
                column_type: required(&row, "COLUMN_TYPE")?,
                nullable: nullable.eq_ignore_ascii_case("YES"),
                default_value: optional(&row, "COLUMN_DEFAULT")?,
                character_set: optional(&row, "CHARACTER_SET_NAME")?,
                collation: optional(&row, "COLLATION_NAME")?,
                extra: required(&row, "EXTRA")?,
                privileges: required(&row, "PRIVILEGES")?,
                comment: required(&row, "COLUMN_COMMENT")?,
                generation_expression: optional(&row, "GENERATION_EXPRESSION")?,
                spatial_reference_id: optional(&row, "SRS_ID")?,
                system_period_start: period_start.eq_ignore_ascii_case("YES"),
                system_period_end: period_end.eq_ignore_ascii_case("YES"),
            })
        })
        .collect()
}

fn read_constraints<Q: Queryable>(
    connection: &mut Q,
    database: &str,
    strategy: MysqlFamilyVersion,
) -> Result<Vec<RawConstraint>, CatalogError> {
    let enforced = match strategy.product() {
        MysqlProduct::Mysql => "ENFORCED",
        MysqlProduct::MariaDb => "'YES' AS ENFORCED",
    };
    let sql = format!(
        "SELECT TABLE_NAME, CONSTRAINT_NAME, CONSTRAINT_TYPE, {enforced} \
         FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS WHERE TABLE_SCHEMA = ? \
         ORDER BY TABLE_NAME, CONSTRAINT_NAME"
    );
    connection
        .exec::<Row, _, _>(sql, (database,))?
        .into_iter()
        .map(|row| {
            let enforced: String = required(&row, "ENFORCED")?;
            Ok(RawConstraint {
                table: required(&row, "TABLE_NAME")?,
                name: required(&row, "CONSTRAINT_NAME")?,
                constraint_type: required(&row, "CONSTRAINT_TYPE")?,
                enforced: enforced.eq_ignore_ascii_case("YES"),
            })
        })
        .collect()
}

fn read_key_usage<Q: Queryable>(
    connection: &mut Q,
    database: &str,
) -> Result<Vec<RawKeyUsage>, CatalogError> {
    connection
        .exec::<Row, _, _>(
            "SELECT TABLE_NAME, CONSTRAINT_NAME, COLUMN_NAME, ORDINAL_POSITION, \
                    REFERENCED_TABLE_SCHEMA, REFERENCED_TABLE_NAME, REFERENCED_COLUMN_NAME \
             FROM INFORMATION_SCHEMA.KEY_COLUMN_USAGE WHERE TABLE_SCHEMA = ? \
             ORDER BY TABLE_NAME, CONSTRAINT_NAME, ORDINAL_POSITION",
            (database,),
        )?
        .into_iter()
        .map(|row| {
            Ok(RawKeyUsage {
                table: required(&row, "TABLE_NAME")?,
                constraint: required(&row, "CONSTRAINT_NAME")?,
                column: required(&row, "COLUMN_NAME")?,
                ordinal: u32_from_u64(required(&row, "ORDINAL_POSITION")?, "key ordinal")?,
                referenced_schema: optional(&row, "REFERENCED_TABLE_SCHEMA")?,
                referenced_table: optional(&row, "REFERENCED_TABLE_NAME")?,
                referenced_column: optional(&row, "REFERENCED_COLUMN_NAME")?,
            })
        })
        .collect()
}

fn read_reference_rules<Q: Queryable>(
    connection: &mut Q,
    database: &str,
) -> Result<Vec<RawReferenceRule>, CatalogError> {
    connection
        .exec::<Row, _, _>(
            "SELECT TABLE_NAME, CONSTRAINT_NAME, MATCH_OPTION, UPDATE_RULE, DELETE_RULE \
             FROM INFORMATION_SCHEMA.REFERENTIAL_CONSTRAINTS WHERE CONSTRAINT_SCHEMA = ? \
             ORDER BY TABLE_NAME, CONSTRAINT_NAME",
            (database,),
        )?
        .into_iter()
        .map(|row| {
            Ok(RawReferenceRule {
                table: required(&row, "TABLE_NAME")?,
                constraint: required(&row, "CONSTRAINT_NAME")?,
                match_option: required(&row, "MATCH_OPTION")?,
                update_rule: required(&row, "UPDATE_RULE")?,
                delete_rule: required(&row, "DELETE_RULE")?,
            })
        })
        .collect()
}

fn read_checks<Q: Queryable>(
    connection: &mut Q,
    database: &str,
    strategy: MysqlFamilyVersion,
) -> Result<Vec<RawCheck>, CatalogError> {
    let sql = match strategy.product() {
        MysqlProduct::Mysql => {
            "SELECT tc.TABLE_NAME, cc.CONSTRAINT_NAME, cc.CHECK_CLAUSE \
             FROM INFORMATION_SCHEMA.CHECK_CONSTRAINTS cc \
             JOIN INFORMATION_SCHEMA.TABLE_CONSTRAINTS tc \
               ON tc.CONSTRAINT_SCHEMA = cc.CONSTRAINT_SCHEMA \
              AND tc.CONSTRAINT_NAME = cc.CONSTRAINT_NAME \
              AND tc.CONSTRAINT_TYPE = 'CHECK' \
             WHERE cc.CONSTRAINT_SCHEMA = ? ORDER BY tc.TABLE_NAME, cc.CONSTRAINT_NAME"
        }
        MysqlProduct::MariaDb => {
            "SELECT TABLE_NAME, CONSTRAINT_NAME, CHECK_CLAUSE \
             FROM INFORMATION_SCHEMA.CHECK_CONSTRAINTS WHERE CONSTRAINT_SCHEMA = ? \
             ORDER BY TABLE_NAME, CONSTRAINT_NAME"
        }
    };
    connection
        .exec::<Row, _, _>(sql, (database,))?
        .into_iter()
        .map(|row| {
            Ok(RawCheck {
                table: required(&row, "TABLE_NAME")?,
                constraint: required(&row, "CONSTRAINT_NAME")?,
                clause: required(&row, "CHECK_CLAUSE")?,
            })
        })
        .collect()
}

fn read_index_parts<Q: Queryable>(
    connection: &mut Q,
    database: &str,
    strategy: MysqlFamilyVersion,
) -> Result<Vec<RawIndexPart>, CatalogError> {
    let (visible, expression) = match strategy.product() {
        MysqlProduct::Mysql => ("IS_VISIBLE", "EXPRESSION"),
        MysqlProduct::MariaDb => (
            "CASE WHEN IGNORED = 'YES' THEN 'NO' ELSE 'YES' END AS IS_VISIBLE",
            "NULL AS EXPRESSION",
        ),
    };
    let sql = format!(
        "SELECT TABLE_NAME, INDEX_NAME, NON_UNIQUE, SEQ_IN_INDEX, COLUMN_NAME, COLLATION, \
                SUB_PART, INDEX_TYPE, COMMENT, INDEX_COMMENT, {visible}, {expression} \
         FROM INFORMATION_SCHEMA.STATISTICS WHERE TABLE_SCHEMA = ? \
         ORDER BY TABLE_NAME, INDEX_NAME, SEQ_IN_INDEX"
    );
    connection
        .exec::<Row, _, _>(sql, (database,))?
        .into_iter()
        .map(|row| {
            let non_unique: u64 = required(&row, "NON_UNIQUE")?;
            let visible: String = required(&row, "IS_VISIBLE")?;
            Ok(RawIndexPart {
                table: required(&row, "TABLE_NAME")?,
                index: required(&row, "INDEX_NAME")?,
                non_unique: non_unique != 0,
                ordinal: u32_from_u64(required(&row, "SEQ_IN_INDEX")?, "index ordinal")?,
                column: optional(&row, "COLUMN_NAME")?,
                collation: optional(&row, "COLLATION")?,
                prefix_length: optional(&row, "SUB_PART")?,
                index_type: required(&row, "INDEX_TYPE")?,
                comment: required(&row, "COMMENT")?,
                index_comment: required(&row, "INDEX_COMMENT")?,
                visible: visible.eq_ignore_ascii_case("YES"),
                expression: optional(&row, "EXPRESSION")?,
            })
        })
        .collect()
}

fn read_views<Q: Queryable>(
    connection: &mut Q,
    database: &str,
    strategy: MysqlFamilyVersion,
) -> Result<Vec<RawView>, CatalogError> {
    let algorithm = match strategy.product() {
        MysqlProduct::Mysql => "NULL AS ALGORITHM",
        MysqlProduct::MariaDb => "ALGORITHM",
    };
    let sql = format!(
        "SELECT TABLE_NAME, VIEW_DEFINITION, CHECK_OPTION, IS_UPDATABLE, DEFINER, SECURITY_TYPE, \
                CHARACTER_SET_CLIENT, COLLATION_CONNECTION, {algorithm} \
         FROM INFORMATION_SCHEMA.VIEWS WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME"
    );
    connection
        .exec::<Row, _, _>(sql, (database,))?
        .into_iter()
        .map(|row| {
            let updatable: String = required(&row, "IS_UPDATABLE")?;
            Ok(RawView {
                name: required(&row, "TABLE_NAME")?,
                definition: optional(&row, "VIEW_DEFINITION")?,
                check_option: required(&row, "CHECK_OPTION")?,
                updatable: updatable.eq_ignore_ascii_case("YES"),
                definer: required(&row, "DEFINER")?,
                security_type: required(&row, "SECURITY_TYPE")?,
                character_set: required(&row, "CHARACTER_SET_CLIENT")?,
                collation: required(&row, "COLLATION_CONNECTION")?,
                algorithm: optional(&row, "ALGORITHM")?,
            })
        })
        .collect()
}

fn read_view_table_usage<Q: Queryable>(
    connection: &mut Q,
    database: &str,
    strategy: MysqlFamilyVersion,
) -> Result<Vec<RawViewTableUsage>, CatalogError> {
    if strategy.product() == MysqlProduct::MariaDb {
        return Ok(Vec::new());
    }
    connection
        .exec::<Row, _, _>(
            "SELECT VIEW_NAME, TABLE_SCHEMA, TABLE_NAME \
             FROM INFORMATION_SCHEMA.VIEW_TABLE_USAGE WHERE VIEW_SCHEMA = ? \
             ORDER BY VIEW_NAME, TABLE_SCHEMA, TABLE_NAME",
            (database,),
        )?
        .into_iter()
        .map(|row| {
            Ok(RawViewTableUsage {
                view: required(&row, "VIEW_NAME")?,
                target_schema: required(&row, "TABLE_SCHEMA")?,
                target_name: required(&row, "TABLE_NAME")?,
            })
        })
        .collect()
}

fn read_view_routine_usage<Q: Queryable>(
    connection: &mut Q,
    database: &str,
    strategy: MysqlFamilyVersion,
) -> Result<Vec<RawViewRoutineUsage>, CatalogError> {
    if strategy.product() == MysqlProduct::MariaDb {
        return Ok(Vec::new());
    }
    connection
        .exec::<Row, _, _>(
            "SELECT TABLE_NAME, SPECIFIC_SCHEMA, SPECIFIC_NAME \
             FROM INFORMATION_SCHEMA.VIEW_ROUTINE_USAGE WHERE TABLE_SCHEMA = ? \
             ORDER BY TABLE_NAME, SPECIFIC_SCHEMA, SPECIFIC_NAME",
            (database,),
        )?
        .into_iter()
        .map(|row| {
            Ok(RawViewRoutineUsage {
                view: required(&row, "TABLE_NAME")?,
                routine_schema: required(&row, "SPECIFIC_SCHEMA")?,
                specific_name: required(&row, "SPECIFIC_NAME")?,
            })
        })
        .collect()
}

fn read_routines<Q: Queryable>(
    connection: &mut Q,
    database: &str,
) -> Result<Vec<RawRoutine>, CatalogError> {
    connection
        .exec::<Row, _, _>(
            "SELECT SPECIFIC_NAME, ROUTINE_NAME, ROUTINE_TYPE, DATA_TYPE, DTD_IDENTIFIER, \
                    ROUTINE_DEFINITION, IS_DETERMINISTIC, SQL_DATA_ACCESS, SECURITY_TYPE, \
                    SQL_MODE, COALESCE(ROUTINE_COMMENT, '') AS ROUTINE_COMMENT, DEFINER, \
                    CHARACTER_SET_CLIENT, COLLATION_CONNECTION, DATABASE_COLLATION \
             FROM INFORMATION_SCHEMA.ROUTINES WHERE ROUTINE_SCHEMA = ? ORDER BY SPECIFIC_NAME",
            (database,),
        )?
        .into_iter()
        .map(|row| {
            let deterministic: String = required(&row, "IS_DETERMINISTIC")?;
            Ok(RawRoutine {
                specific_name: required(&row, "SPECIFIC_NAME")?,
                name: required(&row, "ROUTINE_NAME")?,
                routine_type: required(&row, "ROUTINE_TYPE")?,
                data_type: required(&row, "DATA_TYPE")?,
                dtd_identifier: optional(&row, "DTD_IDENTIFIER")?,
                definition: optional(&row, "ROUTINE_DEFINITION")?,
                deterministic: deterministic.eq_ignore_ascii_case("YES"),
                sql_data_access: required(&row, "SQL_DATA_ACCESS")?,
                security_type: required(&row, "SECURITY_TYPE")?,
                sql_mode: required(&row, "SQL_MODE")?,
                comment: required(&row, "ROUTINE_COMMENT")?,
                definer: required(&row, "DEFINER")?,
                character_set: optional(&row, "CHARACTER_SET_CLIENT")?,
                collation: optional(&row, "COLLATION_CONNECTION")?,
                database_collation: required(&row, "DATABASE_COLLATION")?,
            })
        })
        .collect()
}

fn read_parameters<Q: Queryable>(
    connection: &mut Q,
    database: &str,
    strategy: MysqlFamilyVersion,
) -> Result<Vec<RawParameter>, CatalogError> {
    let default_value = match strategy {
        MysqlFamilyVersion::MariaDb123 => "PARAMETER_DEFAULT",
        _ => "NULL AS PARAMETER_DEFAULT",
    };
    let sql = format!(
        "SELECT SPECIFIC_NAME, ORDINAL_POSITION, PARAMETER_MODE, PARAMETER_NAME, DATA_TYPE, \
                DTD_IDENTIFIER, ROUTINE_TYPE, {default_value} \
         FROM INFORMATION_SCHEMA.PARAMETERS WHERE SPECIFIC_SCHEMA = ? \
         ORDER BY SPECIFIC_NAME, ORDINAL_POSITION"
    );
    connection
        .exec::<Row, _, _>(sql, (database,))?
        .into_iter()
        .map(|row| {
            Ok(RawParameter {
                specific_name: required(&row, "SPECIFIC_NAME")?,
                ordinal: u32_from_u64(required(&row, "ORDINAL_POSITION")?, "parameter ordinal")?,
                mode: optional(&row, "PARAMETER_MODE")?,
                name: optional(&row, "PARAMETER_NAME")?,
                data_type: required(&row, "DATA_TYPE")?,
                dtd_identifier: optional(&row, "DTD_IDENTIFIER")?,
                routine_type: required(&row, "ROUTINE_TYPE")?,
                default_value: optional(&row, "PARAMETER_DEFAULT")?,
            })
        })
        .collect()
}

fn read_triggers<Q: Queryable>(
    connection: &mut Q,
    database: &str,
) -> Result<Vec<RawTrigger>, CatalogError> {
    connection
        .exec::<Row, _, _>(
            "SELECT TRIGGER_NAME, EVENT_MANIPULATION, EVENT_OBJECT_TABLE, ACTION_ORDER, \
                    ACTION_CONDITION, ACTION_STATEMENT, ACTION_ORIENTATION, ACTION_TIMING, \
                    SQL_MODE, DEFINER, CHARACTER_SET_CLIENT, COLLATION_CONNECTION, DATABASE_COLLATION \
             FROM INFORMATION_SCHEMA.TRIGGERS WHERE TRIGGER_SCHEMA = ? ORDER BY TRIGGER_NAME",
            (database,),
        )?
        .into_iter()
        .map(|row| {
            Ok(RawTrigger {
                name: required(&row, "TRIGGER_NAME")?,
                event: required(&row, "EVENT_MANIPULATION")?,
                table: required(&row, "EVENT_OBJECT_TABLE")?,
                action_order: required(&row, "ACTION_ORDER")?,
                condition: optional(&row, "ACTION_CONDITION")?,
                statement: optional(&row, "ACTION_STATEMENT")?,
                orientation: required(&row, "ACTION_ORIENTATION")?,
                timing: required(&row, "ACTION_TIMING")?,
                sql_mode: required(&row, "SQL_MODE")?,
                definer: required(&row, "DEFINER")?,
                character_set: required(&row, "CHARACTER_SET_CLIENT")?,
                collation: required(&row, "COLLATION_CONNECTION")?,
                database_collation: required(&row, "DATABASE_COLLATION")?,
            })
        })
        .collect()
}

fn read_events<Q: Queryable>(
    connection: &mut Q,
    database: &str,
) -> Result<Vec<RawEvent>, CatalogError> {
    connection
        .exec::<Row, _, _>(
            "SELECT EVENT_NAME, DEFINER, TIME_ZONE, EVENT_BODY, EVENT_DEFINITION, EVENT_TYPE, \
                    CAST(EXECUTE_AT AS CHAR) AS EXECUTE_AT_TEXT, INTERVAL_VALUE, INTERVAL_FIELD, \
                    SQL_MODE, CAST(STARTS AS CHAR) AS STARTS_TEXT, CAST(ENDS AS CHAR) AS ENDS_TEXT, \
                    STATUS, ON_COMPLETION, COALESCE(EVENT_COMMENT, '') AS EVENT_COMMENT \
             FROM INFORMATION_SCHEMA.EVENTS WHERE EVENT_SCHEMA = ? ORDER BY EVENT_NAME",
            (database,),
        )?
        .into_iter()
        .map(|row| {
            Ok(RawEvent {
                name: required(&row, "EVENT_NAME")?,
                definer: required(&row, "DEFINER")?,
                time_zone: required(&row, "TIME_ZONE")?,
                body: required(&row, "EVENT_BODY")?,
                definition: optional(&row, "EVENT_DEFINITION")?,
                event_type: required(&row, "EVENT_TYPE")?,
                execute_at: optional(&row, "EXECUTE_AT_TEXT")?,
                interval_value: optional(&row, "INTERVAL_VALUE")?,
                interval_field: optional(&row, "INTERVAL_FIELD")?,
                sql_mode: required(&row, "SQL_MODE")?,
                starts: optional(&row, "STARTS_TEXT")?,
                ends: optional(&row, "ENDS_TEXT")?,
                status: required(&row, "STATUS")?,
                on_completion: required(&row, "ON_COMPLETION")?,
                comment: required(&row, "EVENT_COMMENT")?,
            })
        })
        .collect()
}

fn read_partitions<Q: Queryable>(
    connection: &mut Q,
    database: &str,
) -> Result<Vec<RawPartition>, CatalogError> {
    connection
        .exec::<Row, _, _>(
            "SELECT TABLE_NAME, PARTITION_NAME, SUBPARTITION_NAME, PARTITION_ORDINAL_POSITION, \
                    SUBPARTITION_ORDINAL_POSITION, PARTITION_METHOD, SUBPARTITION_METHOD, \
                    PARTITION_EXPRESSION, SUBPARTITION_EXPRESSION, PARTITION_DESCRIPTION, \
                    COALESCE(PARTITION_COMMENT, '') AS PARTITION_COMMENT, TABLESPACE_NAME \
             FROM INFORMATION_SCHEMA.PARTITIONS \
             WHERE TABLE_SCHEMA = ? AND PARTITION_NAME IS NOT NULL \
             ORDER BY TABLE_NAME, PARTITION_ORDINAL_POSITION, SUBPARTITION_ORDINAL_POSITION",
            (database,),
        )?
        .into_iter()
        .map(|row| {
            Ok(RawPartition {
                table: required(&row, "TABLE_NAME")?,
                partition: required(&row, "PARTITION_NAME")?,
                subpartition: optional(&row, "SUBPARTITION_NAME")?,
                partition_ordinal: u32_from_u64(
                    required(&row, "PARTITION_ORDINAL_POSITION")?,
                    "partition ordinal",
                )?,
                subpartition_ordinal: optional::<u64>(&row, "SUBPARTITION_ORDINAL_POSITION")?
                    .map(|value| u32_from_u64(value, "subpartition ordinal"))
                    .transpose()?,
                method: optional(&row, "PARTITION_METHOD")?,
                subpartition_method: optional(&row, "SUBPARTITION_METHOD")?,
                expression: optional(&row, "PARTITION_EXPRESSION")?,
                subpartition_expression: optional(&row, "SUBPARTITION_EXPRESSION")?,
                description: optional(&row, "PARTITION_DESCRIPTION")?,
                comment: required(&row, "PARTITION_COMMENT")?,
                tablespace: optional(&row, "TABLESPACE_NAME")?,
            })
        })
        .collect()
}

fn read_sequences<Q: Queryable>(
    connection: &mut Q,
    database: &str,
    strategy: MysqlFamilyVersion,
    tables: &[RawTable],
) -> Result<Vec<RawSequence>, CatalogError> {
    if strategy.product() == MysqlProduct::Mysql {
        return Ok(Vec::new());
    }
    let sequence_names = tables
        .iter()
        .filter(|table| table.table_type.eq_ignore_ascii_case("SEQUENCE"))
        .map(|table| table.name.clone())
        .collect::<Vec<_>>();
    let mut definitions = BTreeMap::new();
    for name in &sequence_names {
        let statement = format!(
            "SHOW CREATE SEQUENCE {}.{}",
            quote_identifier(database),
            quote_identifier(name)
        );
        let row = connection
            .query_first::<Row, _>(statement)?
            .ok_or_else(|| CatalogError::Mapping(format!("sequence '{name}' has no definition")))?;
        let definition = optional_at::<String>(&row, 1)?.ok_or_else(|| {
            CatalogError::Mapping(format!("sequence '{name}' has a hidden definition"))
        })?;
        if definition.len() as u64 > MAX_DEFINITION_BYTES {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "sequence '{name}' definition exceeds the {MAX_DEFINITION_BYTES}-byte safety limit"
            )));
        }
        definitions.insert(name.clone(), definition);
    }

    if matches!(
        strategy,
        MysqlFamilyVersion::MariaDb118 | MysqlFamilyVersion::MariaDb123
    ) {
        let rows = connection.exec::<Row, _, _>(
            "SELECT SEQUENCE_NAME, DATA_TYPE, CAST(START_VALUE AS CHAR) AS START_VALUE_TEXT, \
                    CAST(MINIMUM_VALUE AS CHAR) AS MINIMUM_VALUE_TEXT, \
                    CAST(MAXIMUM_VALUE AS CHAR) AS MAXIMUM_VALUE_TEXT, \
                    CAST(INCREMENT AS CHAR) AS INCREMENT_TEXT, CYCLE_OPTION \
             FROM INFORMATION_SCHEMA.SEQUENCES WHERE SEQUENCE_SCHEMA = ? ORDER BY SEQUENCE_NAME",
            (database,),
        )?;
        let mut sequences = Vec::new();
        for row in rows {
            let name: String = required(&row, "SEQUENCE_NAME")?;
            let cycle: String = required(&row, "CYCLE_OPTION")?;
            sequences.push(RawSequence {
                definition: definitions.remove(&name),
                name,
                data_type: optional(&row, "DATA_TYPE")?,
                start_value: optional(&row, "START_VALUE_TEXT")?,
                minimum_value: optional(&row, "MINIMUM_VALUE_TEXT")?,
                maximum_value: optional(&row, "MAXIMUM_VALUE_TEXT")?,
                increment: optional(&row, "INCREMENT_TEXT")?,
                cycles: Some(cycle.eq_ignore_ascii_case("YES")),
            });
        }
        if !definitions.is_empty() || sequences.len() != sequence_names.len() {
            return Err(CatalogError::Mapping(
                "MariaDB SEQUENCES rows do not reconcile with TABLES sequence rows".to_owned(),
            ));
        }
        Ok(sequences)
    } else {
        Ok(sequence_names
            .into_iter()
            .map(|name| RawSequence {
                definition: definitions.remove(&name),
                name,
                data_type: None,
                start_value: None,
                minimum_value: None,
                maximum_value: None,
                increment: None,
                cycles: None,
            })
            .collect())
    }
}

fn quote_identifier(value: &str) -> String {
    format!("`{}`", value.replace('`', "``"))
}

fn u32_from_u64(value: u64, subject: &str) -> Result<u32, CatalogError> {
    u32::try_from(value)
        .map_err(|_| CatalogError::Mapping(format!("{subject} {value} exceeds u32 range")))
}

struct MysqlFamilySnapshotMapper {
    connection_alias: String,
    strategy: MysqlFamilyVersion,
}

