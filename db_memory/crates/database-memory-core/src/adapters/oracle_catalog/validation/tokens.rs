fn normalize_definition(value: Option<String>) -> Result<Option<String>, CatalogError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let normalized = value.trim().to_owned();
    if normalized.len() > MAX_DEFINITION_BYTES {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "Oracle metadata definition exceeds the {MAX_DEFINITION_BYTES}-byte safety limit"
        )));
    }
    Ok((!normalized.is_empty()).then_some(normalized))
}

fn normalize_optional_token(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn required_catalog_token(value: Option<String>, subject: &str) -> Result<String, CatalogError> {
    normalize_optional_token(value)
        .ok_or_else(|| CatalogError::Mapping(format!("Oracle catalog is missing {subject}")))
}

fn ensure_yes_no(value: &str, subject: &str) -> Result<(), CatalogError> {
    if matches!(value, "YES" | "NO") {
        Ok(())
    } else {
        Err(CatalogError::Mapping(format!(
            "{subject} has unrecognized value '{value}'"
        )))
    }
}

fn ensure_user_type_reference(
    scope: &DictionaryScope,
    user_types: &BTreeMap<(String, String), &RawUserType>,
    owner: Option<&str>,
    name: &str,
    subject: &str,
) -> Result<(), CatalogError> {
    if name.trim().is_empty() {
        return Err(CatalogError::Mapping(format!(
            "{subject} has no data type name"
        )));
    }
    let Some(owner) = owner else {
        return Ok(());
    };
    ensure_reference_owner(scope, owner, subject)?;
    if user_types.contains_key(&(owner.to_owned(), name.to_owned())) {
        Ok(())
    } else {
        Err(CatalogError::Mapping(format!(
            "{subject} references missing type {owner}.{name}"
        )))
    }
}

fn reject_dynamic_plsql(kind: &str, name: &str, definition: &str) -> Result<(), CatalogError> {
    let words = oracle_plsql_words(definition)?;
    let execute_immediate = words
        .windows(2)
        .any(|words| words == ["EXECUTE", "IMMEDIATE"]);
    let dbms_sql = words.iter().any(|word| word == "DBMS_SQL");
    let execute_ddl = words
        .windows(2)
        .any(|words| words == ["DBMS_UTILITY", "EXEC_DDL_STATEMENT"]);
    let dynamic_open = words.iter().enumerate().any(|(index, word)| {
        if word != "OPEN" {
            return false;
        }
        let Some(for_offset) = words[index + 1..]
            .iter()
            .take(3)
            .position(|word| word == "FOR")
        else {
            return false;
        };
        !matches!(
            words.get(index + for_offset + 2).map(String::as_str),
            Some("SELECT" | "WITH")
        )
    });
    if execute_immediate || dbms_sql || execute_ddl || dynamic_open {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "Oracle {kind} {name} contains dynamic PL/SQL that prevents complete dependency proof"
        )));
    }
    Ok(())
}

fn oracle_plsql_words(source: &str) -> Result<Vec<String>, CatalogError> {
    let chars = source.chars().collect::<Vec<_>>();
    let mut words = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '-' && chars.get(index + 1) == Some(&'-') {
            index += 2;
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            continue;
        }
        if chars[index] == '/' && chars.get(index + 1) == Some(&'*') {
            index += 2;
            while index + 1 < chars.len() && !(chars[index] == '*' && chars[index + 1] == '/') {
                index += 1;
            }
            if index + 1 >= chars.len() {
                return Err(CatalogError::UnsupportedMetadata(
                    "Oracle PL/SQL contains an unterminated block comment".to_owned(),
                ));
            }
            index += 2;
            continue;
        }
        let q_delimiter_index =
            if matches!(chars[index], 'q' | 'Q') && chars.get(index + 1) == Some(&'\'') {
                Some(index + 2)
            } else if matches!(chars[index], 'n' | 'N')
                && matches!(chars.get(index + 1), Some('q' | 'Q'))
                && chars.get(index + 2) == Some(&'\'')
            {
                Some(index + 3)
            } else {
                None
            };
        if let Some(delimiter_index) = q_delimiter_index {
            let Some(opening) = chars.get(delimiter_index).copied() else {
                return Err(CatalogError::UnsupportedMetadata(
                    "Oracle PL/SQL contains an incomplete alternative-quoted literal".to_owned(),
                ));
            };
            let closing = match opening {
                '[' => ']',
                '{' => '}',
                '(' => ')',
                '<' => '>',
                other => other,
            };
            index = delimiter_index + 1;
            while index + 1 < chars.len() && !(chars[index] == closing && chars[index + 1] == '\'')
            {
                index += 1;
            }
            if index + 1 >= chars.len() {
                return Err(CatalogError::UnsupportedMetadata(
                    "Oracle PL/SQL contains an unterminated alternative-quoted literal".to_owned(),
                ));
            }
            index += 2;
            continue;
        }
        if chars[index] == '\'' {
            index += 1;
            loop {
                let Some(character) = chars.get(index) else {
                    return Err(CatalogError::UnsupportedMetadata(
                        "Oracle PL/SQL contains an unterminated string literal".to_owned(),
                    ));
                };
                if *character != '\'' {
                    index += 1;
                    continue;
                }
                if chars.get(index + 1) == Some(&'\'') {
                    index += 2;
                    continue;
                }
                index += 1;
                break;
            }
            continue;
        }
        if chars[index] == '"' {
            index += 1;
            loop {
                let Some(character) = chars.get(index) else {
                    return Err(CatalogError::UnsupportedMetadata(
                        "Oracle PL/SQL contains an unterminated quoted identifier".to_owned(),
                    ));
                };
                if *character != '"' {
                    index += 1;
                    continue;
                }
                if chars.get(index + 1) == Some(&'"') {
                    index += 2;
                    continue;
                }
                index += 1;
                break;
            }
            continue;
        }
        if chars[index].is_ascii_alphabetic() || matches!(chars[index], '_' | '$' | '#') {
            let start = index;
            index += 1;
            while index < chars.len()
                && (chars[index].is_ascii_alphanumeric() || matches!(chars[index], '_' | '$' | '#'))
            {
                index += 1;
            }
            words.push(
                chars[start..index]
                    .iter()
                    .collect::<String>()
                    .to_uppercase(),
            );
            continue;
        }
        index += 1;
    }
    Ok(words)
}

fn oracle_trigger_timing(trigger_type: &str) -> Result<String, CatalogError> {
    for timing in ["INSTEAD OF", "BEFORE", "AFTER", "COMPOUND"] {
        if trigger_type.starts_with(timing) {
            return Ok(timing.to_owned());
        }
    }
    Err(CatalogError::UnsupportedMetadata(format!(
        "Oracle trigger type '{trigger_type}' has no covered timing"
    )))
}

fn oracle_trigger_events(triggering_event: &str) -> Result<Vec<String>, CatalogError> {
    let events = triggering_event
        .split(" OR ")
        .map(str::trim)
        .filter(|event| !event.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if events.is_empty() {
        Err(CatalogError::Mapping(
            "Oracle trigger has no triggering events".to_owned(),
        ))
    } else {
        Ok(events)
    }
}
