const SECRET_KEYS: &[&str] = &[
    "aws_secret_access_key",
    "awsSecretAccessKey",
    "secret_access_key",
    "secretAccessKey",
    "connection_string",
    "connectionString",
    "database_url",
    "databaseUrl",
    "refresh_token",
    "refreshToken",
    "client_secret",
    "clientSecret",
    "private_key",
    "privateKey",
    "authorization",
    "access_token",
    "password",
    "passwd",
    "pwd",
    "token",
    "secret",
    "api_key",
    "apikey",
];

const PREFIXED_SECRET_SHAPES: &[(&[u8], usize)] = &[
    (b"github_pat_", 24),
    (b"ghp_", 20),
    (b"gho_", 20),
    (b"ghu_", 20),
    (b"ghs_", 20),
    (b"ghr_", 20),
    (b"sk-proj-", 24),
    (b"sk-live-", 20),
    (b"sk_live_", 20),
    (b"sk_test_", 20),
    (b"sk-", 20),
    (b"xoxb-", 20),
    (b"xoxp-", 20),
    (b"xapp-", 20),
    (b"AIza", 24),
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
    let mut redacted = redact_private_keys(input);
    redacted = redact_bearer_tokens(&redacted);
    redacted = redact_key_values(&redacted);
    redacted = redact_url_passwords(&redacted);
    redacted = redact_oracle_connect_strings(&redacted);
    redacted = redact_known_token_shapes(&redacted);
    redacted
}

fn redact_private_keys(input: &str) -> String {
    let mut output = input.to_string();
    let mut search_from = 0;
    while let Some(offset) = output[search_from..].find("-----BEGIN ") {
        let begin = search_from + offset;
        let header_end = output[begin..]
            .find('\n')
            .map(|offset| begin + offset)
            .unwrap_or(output.len());
        if !output[begin..header_end].contains("PRIVATE KEY-----") {
            search_from = header_end.min(output.len());
            continue;
        }
        let end = output[header_end..]
            .find("-----END ")
            .map(|offset| header_end + offset)
            .and_then(|footer_start| {
                let footer_end = output[footer_start..]
                    .find('\n')
                    .map(|offset| footer_start + offset)
                    .unwrap_or(output.len());
                output[footer_start..footer_end]
                    .contains("PRIVATE KEY-----")
                    .then_some(footer_end)
            })
            // An unterminated private-key block is still secret material.
            .unwrap_or(output.len());
        output.replace_range(begin..end, "[REDACTED_PRIVATE_KEY]");
        search_from = begin + "[REDACTED_PRIVATE_KEY]".len();
    }
    output
}

fn redact_bearer_tokens(input: &str) -> String {
    let mut output = input.to_string();
    let mut search_from = 0;
    loop {
        let lower = output.to_ascii_lowercase();
        let Some(offset) = lower[search_from..].find("bearer ") else {
            break;
        };
        let token_start = search_from + offset + "bearer ".len();
        let token_end = output[token_start..]
            .find(|character: char| {
                character.is_whitespace() || matches!(character, '"' | '\'' | ',' | ';')
            })
            .map(|offset| token_start + offset)
            .unwrap_or(output.len());
        if token_end > token_start {
            output.replace_range(token_start..token_end, "[REDACTED]");
            search_from = token_start + "[REDACTED]".len();
        } else {
            search_from = token_start;
        }
    }
    output
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
        if previous.is_ascii_alphanumeric() || matches!(previous, b'_' | b'-' | b'/' | b'\\') {
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
    let separator = *bytes.get(cursor)?;
    if !matches!(separator, b'=' | b':') {
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
                character.is_whitespace()
                    || matches!(character, '&' | ';' | ',' | '"' | '\'' | ')' | ']' | '}')
            })
            .map(|offset| value_start + offset)
            .unwrap_or(input.len())
    };

    if separator == b':'
        && quote.is_none()
        && looks_like_source_type_annotation(&input[value_start..value_end])
    {
        return None;
    }

    Some((value_start, value_end))
}

fn looks_like_source_type_annotation(value: &str) -> bool {
    matches!(
        value,
        "str"
            | "string"
            | "String"
            | "bool"
            | "boolean"
            | "int"
            | "integer"
            | "float"
            | "double"
            | "number"
            | "bytes"
            | "object"
            | "unknown"
            | "any"
    ) || value
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_uppercase())
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
            let username = &token[..slash_offset];
            let host_starts_valid = output[at + 1..]
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_alphanumeric() || character == '[');
            if password_start < at
                && !username.is_empty()
                && !username.contains('/')
                && host_starts_valid
            {
                output.replace_range(password_start..at, "[REDACTED]");
                search_from = password_start + "[REDACTED]".len() + 1;
                continue;
            }
        }

        search_from = at + 1;
    }

    output
}

fn redact_known_token_shapes(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut spans = Vec::<(usize, usize)>::new();
    let mut index = 0;
    while index < bytes.len() {
        if let Some((_, minimum_len)) = PREFIXED_SECRET_SHAPES
            .iter()
            .find(|(prefix, _)| bytes[index..].starts_with(prefix))
        {
            let end = ascii_secret_token_end(bytes, index);
            if end.saturating_sub(index) >= *minimum_len && token_boundary(bytes, index, end) {
                spans.push((index, end));
                index = end;
                continue;
            }
        }
        if is_aws_access_key(bytes, index) {
            spans.push((index, index + 20));
            index += 20;
            continue;
        }
        if bytes[index..].starts_with(b"eyJ") {
            let end = ascii_secret_token_end(bytes, index);
            let candidate = &bytes[index..end];
            if candidate.len() >= 40
                && candidate.iter().filter(|byte| **byte == b'.').count() == 2
                && token_boundary(bytes, index, end)
            {
                spans.push((index, end));
                index = end;
                continue;
            }
        }
        index += 1;
    }

    let mut output = input.to_string();
    for (start, end) in spans.into_iter().rev() {
        output.replace_range(start..end, "[REDACTED_TOKEN]");
    }
    output
}

fn ascii_secret_token_end(bytes: &[u8], start: usize) -> usize {
    let mut end = start;
    while matches!(bytes.get(end), Some(byte) if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')) {
        end += 1;
    }
    end
}

fn token_boundary(bytes: &[u8], start: usize, end: usize) -> bool {
    let valid_left = start == 0
        || !matches!(bytes[start - 1], byte if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'));
    let valid_right = end == bytes.len()
        || !matches!(bytes[end], byte if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'));
    valid_left && valid_right
}

fn is_aws_access_key(bytes: &[u8], start: usize) -> bool {
    const PREFIXES: &[&[u8; 4]] = &[
        b"AKIA", b"ASIA", b"AIDA", b"AROA", b"AIPA", b"AGPA", b"ANPA", b"ANVA",
    ];
    let end = start.saturating_add(20);
    end <= bytes.len()
        && PREFIXES
            .iter()
            .any(|prefix| bytes[start..].starts_with(prefix.as_slice()))
        && bytes[start + 4..end]
            .iter()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
        && token_boundary(bytes, start, end)
}

fn timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}
