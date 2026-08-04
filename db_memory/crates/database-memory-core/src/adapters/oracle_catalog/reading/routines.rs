fn read_triggers(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawTrigger>, CatalogError> {
    type TriggerTuple = (
        String,
        String,
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let mut triggers = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       TRIGGER_NAME,
                       TRIGGER_TYPE,
                       TRIGGERING_EVENT,
                       TABLE_OWNER,
                       BASE_OBJECT_TYPE,
                       TABLE_NAME,
                       COLUMN_NAME,
                       REFERENCING_NAMES,
                       WHEN_CLAUSE,
                       STATUS,
                       DESCRIPTION,
                       ACTION_TYPE,
                       TRIGGER_BODY,
                       CROSSEDITION,
                       FIRE_ONCE,
                       APPLY_SERVER_ONLY
                FROM USER_TRIGGERS
                ORDER BY TRIGGER_NAME
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER,
                       TRIGGER_NAME,
                       TRIGGER_TYPE,
                       TRIGGERING_EVENT,
                       TABLE_OWNER,
                       BASE_OBJECT_TYPE,
                       TABLE_NAME,
                       COLUMN_NAME,
                       REFERENCING_NAMES,
                       WHEN_CLAUSE,
                       STATUS,
                       DESCRIPTION,
                       ACTION_TYPE,
                       TRIGGER_BODY,
                       CROSSEDITION,
                       FIRE_ONCE,
                       APPLY_SERVER_ONLY
                FROM DBA_TRIGGERS
                WHERE OWNER = :1
                ORDER BY OWNER, TRIGGER_NAME
                "
            }
        };
        let rows = connection.query_as::<TriggerTuple>(sql, &[owner])?;
        for row in rows {
            let (
                owner,
                name,
                trigger_type,
                triggering_event,
                table_owner,
                base_object_type,
                table_name,
                column_name,
                referencing_names,
                when_clause,
                status,
                description,
                action_type,
                body,
                crossedition,
                fire_once,
                apply_server_only,
            ) = row?;
            triggers.push(RawTrigger {
                owner,
                name,
                trigger_type: trigger_type.trim().to_owned(),
                triggering_event: triggering_event.trim().to_owned(),
                table_owner: normalize_optional_token(table_owner),
                base_object_type: base_object_type.trim().to_owned(),
                table_name: normalize_optional_token(table_name),
                column_name: normalize_optional_token(column_name),
                referencing_names: normalize_optional_token(referencing_names),
                when_clause: normalize_optional_token(when_clause),
                status: status.trim().to_owned(),
                description: normalize_definition(description)?,
                action_type: action_type.trim().to_owned(),
                body: normalize_definition(body)?,
                crossedition: normalize_optional_token(crossedition),
                fire_once: normalize_optional_token(fire_once),
                apply_server_only: normalize_optional_token(apply_server_only),
            });
        }
    }
    triggers.sort_by(|left, right| (&left.owner, &left.name).cmp(&(&right.owner, &right.name)));
    Ok(triggers)
}

fn read_routines(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawRoutine>, CatalogError> {
    type RoutineTuple = (
        String,
        String,
        i64,
        i64,
        Option<String>,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        Option<String>,
    );
    let mut routines = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       OBJECT_NAME,
                       OBJECT_ID,
                       SUBPROGRAM_ID,
                       OVERLOAD,
                       OBJECT_TYPE,
                       AGGREGATE,
                       PIPELINED,
                       PARALLEL,
                       INTERFACE,
                       DETERMINISTIC,
                       AUTHID,
                       POLYMORPHIC,
                       PROCEDURE_NAME
                FROM USER_PROCEDURES
                WHERE PROCEDURE_NAME IS NULL
                  AND OBJECT_TYPE IN ('FUNCTION', 'PROCEDURE')
                ORDER BY OBJECT_NAME, SUBPROGRAM_ID
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER,
                       OBJECT_NAME,
                       OBJECT_ID,
                       SUBPROGRAM_ID,
                       OVERLOAD,
                       OBJECT_TYPE,
                       AGGREGATE,
                       PIPELINED,
                       PARALLEL,
                       INTERFACE,
                       DETERMINISTIC,
                       AUTHID,
                       POLYMORPHIC,
                       PROCEDURE_NAME
                FROM DBA_PROCEDURES
                WHERE OWNER = :1
                  AND PROCEDURE_NAME IS NULL
                  AND OBJECT_TYPE IN ('FUNCTION', 'PROCEDURE')
                ORDER BY OWNER, OBJECT_NAME, SUBPROGRAM_ID
                "
            }
        };
        let rows = connection.query_as::<RoutineTuple>(sql, &[owner])?;
        for row in rows {
            let (
                owner,
                name,
                object_id,
                subprogram_id,
                overload,
                object_type,
                aggregate,
                pipelined,
                parallel,
                interface,
                deterministic,
                authid,
                polymorphic,
                procedure_name,
            ) = row?;
            if procedure_name.is_some() {
                return Err(CatalogError::Mapping(format!(
                    "Oracle standalone routine {}.{} unexpectedly has PROCEDURE_NAME metadata",
                    owner, name
                )));
            }
            routines.push(RawRoutine {
                owner,
                name,
                object_id,
                subprogram_id,
                overload: normalize_optional_token(overload),
                object_type: object_type.trim().to_owned(),
                aggregate: aggregate.trim() == "YES",
                pipelined: pipelined.trim() == "YES",
                parallel: parallel.trim() == "YES",
                interface: interface.trim() == "YES",
                deterministic: deterministic.trim() == "YES",
                authid: authid.trim().to_owned(),
                polymorphic: match polymorphic.trim() {
                    "" | "NULL" => None,
                    value => Some(value.to_owned()),
                },
                definition: None,
            });
        }
    }
    routines.sort_by(|left, right| {
        (&left.owner, &left.name, left.subprogram_id).cmp(&(
            &right.owner,
            &right.name,
            right.subprogram_id,
        ))
    });
    Ok(routines)
}

fn attach_routine_sources(
    connection: &Connection,
    scope: &DictionaryScope,
    routines: &mut [RawRoutine],
    deadline: Instant,
) -> Result<(), CatalogError> {
    let positions = routines
        .iter()
        .enumerate()
        .map(|(position, routine)| {
            (
                (
                    routine.owner.clone(),
                    routine.name.clone(),
                    routine.object_type.clone(),
                ),
                position,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut sources = BTreeMap::<usize, String>::new();
    let mut last_lines = BTreeMap::<usize, i64>::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1, NAME, TYPE, LINE, TEXT
                FROM USER_SOURCE
                WHERE TYPE IN ('FUNCTION', 'PROCEDURE')
                ORDER BY NAME, TYPE, LINE
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER, NAME, TYPE, LINE, TEXT
                FROM DBA_SOURCE
                WHERE OWNER = :1
                  AND TYPE IN ('FUNCTION', 'PROCEDURE')
                ORDER BY OWNER, NAME, TYPE, LINE
                "
            }
        };
        let rows =
            connection.query_as::<(String, String, String, i64, Option<String>)>(sql, &[owner])?;
        for row in rows {
            let (source_owner, name, object_type, line, text) = row?;
            let position = positions
                .get(&(source_owner.clone(), name.clone(), object_type.clone()))
                .copied()
                .ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "Oracle source {}.{} ({object_type}) has no routine header",
                        source_owner, name
                    ))
                })?;
            let expected_line = last_lines.get(&position).copied().unwrap_or(0) + 1;
            if line != expected_line {
                return Err(CatalogError::Mapping(format!(
                    "Oracle routine source {}.{} expected line {expected_line}, found {line}",
                    source_owner, name
                )));
            }
            last_lines.insert(position, line);
            let source = sources.entry(position).or_default();
            source.push_str(text.as_deref().unwrap_or_default());
            if source.len() > MAX_DEFINITION_BYTES {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "Oracle routine definition exceeds the {MAX_DEFINITION_BYTES}-byte safety limit for {}.{}",
                    source_owner, name
                )));
            }
        }
    }
    for (position, routine) in routines.iter_mut().enumerate() {
        routine.definition = normalize_definition(sources.remove(&position))?;
        if routine.definition.is_none() {
            return Err(CatalogError::Mapping(format!(
                "Oracle routine {}.{} has no complete source",
                routine.owner, routine.name
            )));
        }
    }
    Ok(())
}
