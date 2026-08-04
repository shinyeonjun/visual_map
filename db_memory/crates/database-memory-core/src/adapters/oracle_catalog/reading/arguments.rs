fn read_routine_arguments(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawRoutineArgument>, CatalogError> {
    read_arguments(connection, scope, deadline, false)
}

fn read_package_arguments(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawRoutineArgument>, CatalogError> {
    read_arguments(connection, scope, deadline, true)
}

fn read_arguments(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
    packaged: bool,
) -> Result<Vec<RawRoutineArgument>, CatalogError> {
    type ArgumentTuple = (
        String,
        String,
        Option<String>,
        Option<String>,
        i64,
        i64,
        i64,
        Option<String>,
        String,
        Option<i64>,
        Option<String>,
        String,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<i64>,
        Option<String>,
        i64,
        Option<String>,
    );
    let mut arguments = Vec::new();
    let package_predicate = if packaged { "IS NOT NULL" } else { "IS NULL" };
    let user_package_inventory_predicate = if packaged {
        "AND EXISTS (
                    SELECT 1
                    FROM USER_OBJECTS package_object
                    WHERE package_object.OBJECT_ID = USER_ARGUMENTS.OBJECT_ID
                      AND package_object.OBJECT_TYPE = 'PACKAGE'
                 )"
    } else {
        ""
    };
    let dba_package_inventory_predicate = if packaged {
        "AND EXISTS (
                    SELECT 1
                    FROM DBA_OBJECTS package_object
                    WHERE package_object.OWNER = DBA_ARGUMENTS.OWNER
                      AND package_object.OBJECT_ID = DBA_ARGUMENTS.OBJECT_ID
                      AND package_object.OBJECT_TYPE = 'PACKAGE'
                 )"
    } else {
        ""
    };
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                format!(
                    "
                SELECT :1,
                       OBJECT_NAME,
                       PACKAGE_NAME,
                       ARGUMENT_NAME,
                       POSITION,
                       SEQUENCE,
                       DATA_LEVEL,
                       DATA_TYPE,
                       DEFAULTED,
                       DEFAULT_LENGTH,
                       DEFAULT_VALUE,
                       IN_OUT,
                       DATA_LENGTH,
                       DATA_PRECISION,
                       DATA_SCALE,
                       TYPE_OWNER,
                       TYPE_NAME,
                       TYPE_SUBNAME,
                       PLS_TYPE,
                       CHAR_LENGTH,
                       CHAR_USED,
                       SUBPROGRAM_ID,
                       OVERLOAD
                FROM USER_ARGUMENTS
                WHERE PACKAGE_NAME {package_predicate}
                  {user_package_inventory_predicate}
                ORDER BY OBJECT_NAME, SUBPROGRAM_ID, SEQUENCE
                "
                )
            }
            DictionaryScopeMode::Dba => {
                format!(
                    "
                SELECT OWNER,
                       OBJECT_NAME,
                       PACKAGE_NAME,
                       ARGUMENT_NAME,
                       POSITION,
                       SEQUENCE,
                       DATA_LEVEL,
                       DATA_TYPE,
                       DEFAULTED,
                       DEFAULT_LENGTH,
                       DEFAULT_VALUE,
                       IN_OUT,
                       DATA_LENGTH,
                       DATA_PRECISION,
                       DATA_SCALE,
                       TYPE_OWNER,
                       TYPE_NAME,
                       TYPE_SUBNAME,
                       PLS_TYPE,
                       CHAR_LENGTH,
                       CHAR_USED,
                       SUBPROGRAM_ID,
                       OVERLOAD
                FROM DBA_ARGUMENTS
                WHERE OWNER = :1
                  AND PACKAGE_NAME {package_predicate}
                  {dba_package_inventory_predicate}
                ORDER BY OWNER, OBJECT_NAME, SUBPROGRAM_ID, SEQUENCE
                "
                )
            }
        };
        let rows = connection.query_as::<ArgumentTuple>(&sql, &[owner])?;
        for row in rows {
            let (
                owner,
                routine,
                package_name,
                name,
                position,
                sequence,
                data_level,
                data_type,
                defaulted,
                default_length,
                default_value,
                mode,
                data_length,
                data_precision,
                data_scale,
                type_owner,
                type_name,
                type_subname,
                pls_type,
                char_length,
                char_used,
                subprogram_id,
                overload,
            ) = row?;
            arguments.push(RawRoutineArgument {
                owner,
                routine,
                package_name: normalize_optional_token(package_name),
                name: normalize_optional_token(name),
                position,
                sequence,
                data_level,
                data_type: normalize_optional_token(data_type),
                defaulted: defaulted.trim() == "Y",
                default_length,
                default_value: normalize_definition(default_value)?,
                mode: mode.trim().to_owned(),
                data_length,
                data_precision,
                data_scale,
                type_owner: normalize_optional_token(type_owner),
                type_name: normalize_optional_token(type_name),
                type_subname: normalize_optional_token(type_subname),
                pls_type: normalize_optional_token(pls_type),
                char_length,
                char_used: normalize_optional_token(char_used),
                subprogram_id,
                overload: normalize_optional_token(overload),
            });
        }
    }
    arguments.sort_by(|left, right| {
        (
            &left.owner,
            &left.routine,
            left.subprogram_id,
            left.sequence,
        )
            .cmp(&(
                &right.owner,
                &right.routine,
                right.subprogram_id,
                right.sequence,
            ))
    });
    Ok(arguments)
}
