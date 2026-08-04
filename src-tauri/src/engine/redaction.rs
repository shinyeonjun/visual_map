const SECRET_KEYS: &[&str] = &[
    "password",
    "passwd",
    "pwd",
    "token",
    "secret",
    "api_key",
    "apikey",
    "access_token",
];

fn redact_json_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(fields) => {
            for (key, value) in fields.iter_mut() {
                if SECRET_KEYS
                    .iter()
                    .any(|secret_key| key.eq_ignore_ascii_case(secret_key))
                {
                    *value = serde_json::Value::String("[REDACTED]".to_string());
                } else {
                    redact_json_value(value);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_json_value(item);
            }
        }
        serde_json::Value::String(text) => {
            *text = redact_unstructured_secrets(text);
        }
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {}
    }
}

fn redact_unstructured_secrets(input: &str) -> String {
    let mut redacted = redact_key_values(input);
    redacted = redact_url_passwords(&redacted);
    redacted = redact_oracle_connect_strings(&redacted);
    redacted
}

fn redact_key_values(input: &str) -> String {
    let mut output = input.to_string();

    for key in SECRET_KEYS {
        let mut search_start = 0;
        loop {
            let lower = output.to_ascii_lowercase();
            let Some(offset) = lower[search_start..].find(key) else {
                break;
            };
            let key_start = search_start + offset;
            let key_end = key_start + key.len();
            let Some((value_start, value_end)) = secret_value_range(&output, key_start, key_end)
            else {
                search_start = key_end;
                continue;
            };

            output.replace_range(value_start..value_end, "[REDACTED]");
            search_start = value_start + "[REDACTED]".len();
        }
    }

    output
}

fn secret_value_range(input: &str, key_start: usize, key_end: usize) -> Option<(usize, usize)> {
    let bytes = input.as_bytes();
    if key_start > 0 {
        let previous = bytes[key_start - 1];
        if previous.is_ascii_alphanumeric() || previous == b'_' {
            return None;
        }
    }

    let mut cursor = key_end;
    if matches!(bytes.get(cursor), Some(b'"' | b'\'')) {
        cursor += 1;
    }
    while matches!(bytes.get(cursor), Some(byte) if byte.is_ascii_whitespace()) {
        cursor += 1;
    }
    if !matches!(bytes.get(cursor), Some(b'=' | b':')) {
        return None;
    }
    cursor += 1;
    while matches!(bytes.get(cursor), Some(byte) if byte.is_ascii_whitespace()) {
        cursor += 1;
    }

    let quote = match bytes.get(cursor) {
        Some(b'"') => Some(b'"'),
        Some(b'\'') => Some(b'\''),
        _ => None,
    };
    let value_start = cursor + usize::from(quote.is_some());
    let value_end = if let Some(quote) = quote {
        input[value_start..]
            .find(quote as char)
            .map(|offset| value_start + offset)
            .unwrap_or(input.len())
    } else {
        input[value_start..]
            .find(|character: char| {
                character.is_whitespace() || matches!(character, '&' | ';' | ',' | '"' | '\'')
            })
            .map(|offset| value_start + offset)
            .unwrap_or(input.len())
    };

    Some((value_start, value_end))
}

fn redact_url_passwords(input: &str) -> String {
    let mut output = input.to_string();
    let mut search_from = 0;

    while let Some(scheme_offset) = output[search_from..].find("://") {
        let auth_start = search_from + scheme_offset + 3;
        let rest = &output[auth_start..];
        let Some(at_offset) = rest.find('@') else {
            break;
        };
        let at = auth_start + at_offset;
        let userinfo = &output[auth_start..at];
        if let Some(colon_offset) = userinfo.rfind(':') {
            let password_start = auth_start + colon_offset + 1;
            output.replace_range(password_start..at, "[REDACTED]");
            search_from = password_start + "[REDACTED]".len();
        } else {
            search_from = at + 1;
        }
    }

    output
}

fn redact_oracle_connect_strings(input: &str) -> String {
    let mut output = input.to_string();
    let mut search_from = 0;

    while let Some(at_offset) = output[search_from..].find('@') {
        let at = search_from + at_offset;
        let token_start = output[..at]
            .rfind(|character: char| {
                character.is_whitespace() || matches!(character, '"' | '\'' | ';' | ',' | '(' | ')')
            })
            .map(|offset| offset + 1)
            .unwrap_or(0);
        let token = &output[token_start..at];

        if token.contains("://") {
            search_from = at + 1;
            continue;
        }

        if let Some(slash_offset) = token.rfind('/') {
            let password_start = token_start + slash_offset + 1;
            if password_start < at && slash_offset > 0 {
                output.replace_range(password_start..at, "[REDACTED]");
                search_from = password_start + "[REDACTED]".len() + 1;
                continue;
            }
        }

        search_from = at + 1;
    }

    output
}

fn timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}
