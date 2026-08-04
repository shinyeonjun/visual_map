fn read_type_methods(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawTypeMethod>, CatalogError> {
    type MethodTuple = (
        String,
        String,
        String,
        i64,
        Option<String>,
        i64,
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let mut methods = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       TYPE_NAME,
                       METHOD_NAME,
                       METHOD_NO,
                       METHOD_TYPE,
                       PARAMETERS,
                       RESULTS,
                       FINAL,
                       INSTANTIABLE,
                       OVERRIDING,
                       INHERITED
                FROM USER_TYPE_METHODS
                ORDER BY TYPE_NAME, METHOD_NO
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER,
                       TYPE_NAME,
                       METHOD_NAME,
                       METHOD_NO,
                       METHOD_TYPE,
                       PARAMETERS,
                       RESULTS,
                       FINAL,
                       INSTANTIABLE,
                       OVERRIDING,
                       INHERITED
                FROM DBA_TYPE_METHODS
                WHERE OWNER = :1
                ORDER BY OWNER, TYPE_NAME, METHOD_NO
                "
            }
        };
        let rows = connection.query_as::<MethodTuple>(sql, &[owner])?;
        for row in rows {
            let (
                owner,
                type_name,
                name,
                method_number,
                method_type,
                parameter_count,
                result_count,
                final_method,
                instantiable,
                overriding,
                inherited,
            ) = row?;
            methods.push(RawTypeMethod {
                owner: owner.clone(),
                type_name: type_name.clone(),
                name: name.clone(),
                method_number,
                method_type: required_catalog_token(
                    method_type,
                    &format!("method type for {owner}.{type_name}.{name}"),
                )?,
                parameter_count,
                result_count,
                final_method: required_catalog_token(
                    final_method,
                    &format!("final flag for {owner}.{type_name}.{name}"),
                )?,
                instantiable: required_catalog_token(
                    instantiable,
                    &format!("instantiable flag for {owner}.{type_name}.{name}"),
                )?,
                overriding: required_catalog_token(
                    overriding,
                    &format!("overriding flag for {owner}.{type_name}.{name}"),
                )?,
                inherited: required_catalog_token(
                    inherited,
                    &format!("inherited flag for {owner}.{type_name}.{name}"),
                )?,
            });
        }
    }
    methods.sort_by(|left, right| {
        (&left.owner, &left.type_name, left.method_number).cmp(&(
            &right.owner,
            &right.type_name,
            right.method_number,
        ))
    });
    Ok(methods)
}

fn read_type_method_parameters(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawTypeMethodParameter>, CatalogError> {
    type ParameterTuple = (
        String,
        String,
        String,
        i64,
        String,
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    type ResultTuple = (
        String,
        String,
        String,
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let mut parameters = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let parameter_sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       TYPE_NAME,
                       METHOD_NAME,
                       METHOD_NO,
                       PARAM_NAME,
                       PARAM_NO,
                       PARAM_MODE,
                       PARAM_TYPE_MOD,
                       PARAM_TYPE_OWNER,
                       PARAM_TYPE_NAME,
                       CHARACTER_SET_NAME
                FROM USER_METHOD_PARAMS
                ORDER BY TYPE_NAME, METHOD_NO, PARAM_NO
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER,
                       TYPE_NAME,
                       METHOD_NAME,
                       METHOD_NO,
                       PARAM_NAME,
                       PARAM_NO,
                       PARAM_MODE,
                       PARAM_TYPE_MOD,
                       PARAM_TYPE_OWNER,
                       PARAM_TYPE_NAME,
                       CHARACTER_SET_NAME
                FROM DBA_METHOD_PARAMS
                WHERE OWNER = :1
                ORDER BY OWNER, TYPE_NAME, METHOD_NO, PARAM_NO
                "
            }
        };
        let rows = connection.query_as::<ParameterTuple>(parameter_sql, &[owner])?;
        for row in rows {
            let (
                owner,
                type_name,
                method_name,
                method_number,
                name,
                position,
                mode,
                type_modifier,
                data_type_owner,
                data_type_name,
                character_set,
            ) = row?;
            parameters.push(RawTypeMethodParameter {
                owner: owner.clone(),
                type_name: type_name.clone(),
                method_name: method_name.clone(),
                method_number,
                name,
                position,
                mode: required_catalog_token(
                    mode,
                    &format!("method parameter mode for {owner}.{type_name}.{method_name}"),
                )?,
                type_modifier: normalize_optional_token(type_modifier),
                data_type_owner: normalize_optional_token(data_type_owner),
                data_type_name: required_catalog_token(
                    data_type_name,
                    &format!("method parameter type for {owner}.{type_name}.{method_name}"),
                )?,
                character_set: normalize_optional_token(character_set),
                return_value: false,
            });
        }

        prepare_call(connection, deadline)?;
        let result_sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       TYPE_NAME,
                       METHOD_NAME,
                       METHOD_NO,
                       RESULT_TYPE_MOD,
                       RESULT_TYPE_OWNER,
                       RESULT_TYPE_NAME,
                       CHARACTER_SET_NAME
                FROM USER_METHOD_RESULTS
                ORDER BY TYPE_NAME, METHOD_NO
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER,
                       TYPE_NAME,
                       METHOD_NAME,
                       METHOD_NO,
                       RESULT_TYPE_MOD,
                       RESULT_TYPE_OWNER,
                       RESULT_TYPE_NAME,
                       CHARACTER_SET_NAME
                FROM DBA_METHOD_RESULTS
                WHERE OWNER = :1
                ORDER BY OWNER, TYPE_NAME, METHOD_NO
                "
            }
        };
        let rows = connection.query_as::<ResultTuple>(result_sql, &[owner])?;
        for row in rows {
            let (
                owner,
                type_name,
                method_name,
                method_number,
                type_modifier,
                data_type_owner,
                data_type_name,
                character_set,
            ) = row?;
            parameters.push(RawTypeMethodParameter {
                owner: owner.clone(),
                type_name: type_name.clone(),
                method_name: method_name.clone(),
                method_number,
                name: "RETURN".to_owned(),
                position: 0,
                mode: "OUT".to_owned(),
                type_modifier: normalize_optional_token(type_modifier),
                data_type_owner: normalize_optional_token(data_type_owner),
                data_type_name: required_catalog_token(
                    data_type_name,
                    &format!("method result type for {owner}.{type_name}.{method_name}"),
                )?,
                character_set: normalize_optional_token(character_set),
                return_value: true,
            });
        }
    }
    parameters.sort_by(|left, right| {
        (
            &left.owner,
            &left.type_name,
            left.method_number,
            left.position,
            &left.name,
        )
            .cmp(&(
                &right.owner,
                &right.type_name,
                right.method_number,
                right.position,
                &right.name,
            ))
    });
    Ok(parameters)
}
