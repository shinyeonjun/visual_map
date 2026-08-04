fn validate_supported_metadata(
    unsupported: &[RawUnsupportedObject],
    views: &[RawView],
    routines: &[RawRoutine],
    triggers: &[RawTrigger],
    user_types: &[RawUserType],
    dependencies: &[RawDependency],
) -> Result<(), CatalogError> {
    if let Some(object) = unsupported.first() {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "SQL Server object '{}.{}' has unsupported catalog type {} ({})",
            object.schema.as_deref().unwrap_or("database"),
            object.name,
            object.type_code,
            object.type_desc
        )));
    }
    for view in views {
        require_visible_definition(
            "view",
            &format!("{}.{}", view.schema, view.name),
            view.definition.as_deref(),
        )?;
    }
    for routine in routines {
        if matches!(routine.type_code.as_str(), "PC" | "FS" | "FT" | "AF") {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "CLR routine '{}.{}' has no authoritative SQL dependency body",
                routine.schema, routine.name
            )));
        }
        let definition = require_visible_definition(
            "routine",
            &format!("{}.{}", routine.schema, routine.name),
            routine.definition.as_deref(),
        )?;
        reject_dynamic_sql(
            "routine",
            &format!("{}.{}", routine.schema, routine.name),
            definition,
        )?;
    }
    for trigger in triggers {
        let definition =
            require_visible_definition("trigger", &trigger.name, trigger.definition.as_deref())?;
        reject_dynamic_sql("trigger", &trigger.name, definition)?;
    }
    if let Some(data_type) = user_types.iter().find(|data_type| data_type.assembly) {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "CLR user-defined type '{}.{}' requires assembly metadata mapping",
            data_type.schema, data_type.name
        )));
    }
    if let Some(data_type) = user_types.iter().find(|data_type| {
        data_type.table_type != data_type.table_object_id.is_some()
            || (!data_type.table_type && data_type.memory_optimized)
    }) {
        return Err(CatalogError::Mapping(format!(
            "user-defined type '{}.{}' has inconsistent table-type catalog identity",
            data_type.schema, data_type.name
        )));
    }
    if let Some(data_type) = user_types
        .iter()
        .find(|data_type| data_type.default_object_id != 0 || data_type.rule_object_id != 0)
    {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "user-defined type '{}.{}' uses a legacy bound default or rule whose dependencies are not catalog-maintained",
            data_type.schema, data_type.name
        )));
    }
    if let Some(dependency) = dependencies
        .iter()
        .find(|dependency| dependency.caller_dependent || dependency.ambiguous)
    {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "dependency from object {} is resolved only at runtime (caller_dependent={}, ambiguous={})",
            dependency.referencing_id, dependency.caller_dependent, dependency.ambiguous
        )));
    }
    Ok(())
}

fn require_visible_definition<'a>(
    kind: &str,
    name: &str,
    definition: Option<&'a str>,
) -> Result<&'a str, CatalogError> {
    definition
        .filter(|definition| !definition.trim().is_empty())
        .ok_or_else(|| {
            CatalogError::UnsupportedMetadata(format!(
                "{kind} '{name}' has a hidden, encrypted, or unavailable definition"
            ))
        })
}

fn reject_dynamic_sql(kind: &str, name: &str, definition: &str) -> Result<(), CatalogError> {
    let dialect = MsSqlDialect {};
    let tokens = Tokenizer::new(&dialect, definition)
        .tokenize()
        .map_err(|error| {
            CatalogError::UnsupportedMetadata(format!(
                "{kind} '{name}' cannot be tokenized for dynamic SQL validation: {error}"
            ))
        })?;
    let tokens = tokens
        .iter()
        .filter(|token| !matches!(token, Token::Whitespace(_)))
        .collect::<Vec<_>>();
    for (index, token) in tokens.iter().enumerate() {
        let Token::Word(word) = token else {
            continue;
        };
        if !matches!(word.keyword, Keyword::EXEC | Keyword::EXECUTE)
            && !word.value.eq_ignore_ascii_case("EXEC")
            && !word.value.eq_ignore_ascii_case("EXECUTE")
        {
            continue;
        }
        let rest = &tokens[index + 1..];
        if rest
            .first()
            .is_some_and(|token| matches!(token, Token::Word(word) if word.keyword == Keyword::AS))
        {
            continue;
        }
        if execute_target_is_dynamic(rest) {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "{kind} '{name}' executes dynamic SQL whose dependencies are not catalog-maintained"
            )));
        }
    }
    Ok(())
}

fn execute_target_is_dynamic(tokens: &[&Token]) -> bool {
    let Some(first) = tokens.first() else {
        return true;
    };
    if is_string_token(first) {
        return true;
    }
    if matches!(first, Token::LParen) {
        return tokens
            .get(1)
            .is_none_or(|token| is_variable_token(token) || is_string_token(token));
    }
    if is_variable_token(first) {
        if !tokens
            .get(1)
            .is_some_and(|token| matches!(token, Token::Eq))
        {
            return true;
        }
        return tokens
            .get(2)
            .is_none_or(|token| is_variable_token(token) || is_string_token(token));
    }
    tokens.iter().take(7).any(|token| {
        matches!(token, Token::Word(word) if word.value.eq_ignore_ascii_case("sp_executesql"))
    })
}

fn is_variable_token(token: &Token) -> bool {
    matches!(token, Token::AtSign)
        || matches!(token, Token::Word(word) if word.value.starts_with('@'))
}

fn is_string_token(token: &Token) -> bool {
    matches!(
        token,
        Token::SingleQuotedString(_)
            | Token::NationalStringLiteral(_)
            | Token::DoubleQuotedString(_)
    )
}

fn ensure_definition_size(kind: &str, name: &str, bytes: i32) -> Result<(), CatalogError> {
    if bytes > MAX_DEFINITION_BYTES {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "{kind} '{name}' definition is {bytes} bytes; limit is {MAX_DEFINITION_BYTES}"
        )));
    }
    Ok(())
}

