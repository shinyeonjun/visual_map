#[derive(Debug, Clone, PartialEq, Eq)]
struct StaticLiteral {
    value: String,
    start: usize,
    end: usize,
}

const EXECUTION_CALLS: &[&str] = &[
    ".executenonqueryasync",
    ".executenonquery",
    ".executereaderasync",
    ".executereader",
    ".executescalarasync",
    ".executescalar",
    ".executesqlrawasync",
    ".executesqlraw",
    ".executequery",
    ".executeasync",
    ".execute",
    ".queryfirstordefaultasync",
    ".queryfirstordefault",
    ".queryfirstasync",
    ".queryfirst",
    ".querysingleordefaultasync",
    ".querysingleordefault",
    ".querysingleasync",
    ".querysingle",
    ".querymultipleasync",
    ".querymultiple",
    ".queryforobject",
    ".queryforlist",
    ".querycontext",
    ".queryasync",
    ".query",
    ".sqlqueryraw",
    ".batchupdate",
    ".update",
    ".execcontext",
    ".execasync",
    ".exec",
    ".createnativequery",
    ".preparestatement",
    ".preparecall",
    ".fromsqlraw",
    ".fromsql",
    ".rawquery",
    ".raw",
    "->query",
    "sqlite3_exec",
    "sqlite3_prepare_v2",
    "mysql_query",
    "pqexec",
    ".$executeraw",
    ".$queryraw",
    "sqlx::query",
    "@delete",
    "@insert",
    "@select",
    "@update",
];

fn literal_is_executed(source: &str, literal: &StaticLiteral) -> bool {
    direct_execution_call_accepts_literal(source, literal)
        || (literal_assignment_is_static(source, literal.end)
            && assigned_identifier(source, literal.start).is_some_and(|identifier| {
                execution_call_uses_identifier(&source[literal.end..], &identifier)
            }))
}

fn direct_execution_call_accepts_literal(source: &str, literal: &StaticLiteral) -> bool {
    let start = source[..literal.start]
        .char_indices()
        .rev()
        .nth(512)
        .map_or(0, |(index, _)| index);
    let suffix_end = source[literal.end..]
        .char_indices()
        .nth(8192)
        .map_or(source.len(), |(index, _)| literal.end + index);
    let window = &source[start..suffix_end];
    let lower = window.to_ascii_lowercase();
    let literal_start = literal.start - start;
    let literal_end = literal.end - start;
    EXECUTION_CALLS.iter().any(|marker| {
        lower.match_indices(marker).any(|(position, _)| {
            if position >= literal_start || !trusted_execution_marker(&lower, position, marker) {
                return false;
            }
            let Some(open) = execution_open_paren(&lower, position, marker) else {
                return false;
            };
            let Some(close) = matching_paren(&lower, open) else {
                return false;
            };
            if open >= literal_start || literal_end > close {
                return false;
            }
            match call_depth_at(&lower, open, literal_start) {
                Some(1) => {
                    literal_is_execution_argument(&lower, open, literal_start, marker)
                        && literal_ends_as_direct_argument(&lower, literal_end, close)
                }
                Some(2) => static_sql_wrapper_accepts_literal(
                    &lower,
                    open,
                    literal_start,
                    literal_end,
                    close,
                ),
                _ => false,
            }
        })
    })
}

fn static_sql_wrapper_accepts_literal(
    source: &str,
    call_open: usize,
    literal_start: usize,
    literal_end: usize,
    call_close: usize,
) -> bool {
    let prefix = source[call_open + 1..literal_start]
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    if !matches!(prefix.as_slice(), b"text(" | b"sqlalchemy.text(") {
        return false;
    }
    let Some(after_wrapper) = source[literal_end..call_close]
        .trim_start()
        .strip_prefix(')')
    else {
        return false;
    };
    let after_wrapper = after_wrapper.trim_start();
    after_wrapper.is_empty() || after_wrapper.starts_with(',')
}

fn literal_ends_as_direct_argument(source: &str, literal_end: usize, call_close: usize) -> bool {
    let suffix = source[literal_end..call_close].trim_start();
    suffix.is_empty() || suffix.starts_with(',')
}

fn literal_assignment_is_static(source: &str, literal_end: usize) -> bool {
    let suffix = &source[literal_end..];
    let bytes = suffix.as_bytes();
    let mut index = 0usize;
    while bytes
        .get(index)
        .is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\r'))
    {
        index += 1;
    }
    match bytes.get(index).copied() {
        None | Some(b';' | b',' | b'}') => true,
        Some(b'\n') => {
            index += 1;
            while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
                index += 1;
            }
            !bytes
                .get(index)
                .is_some_and(|byte| matches!(byte, b'+' | b'.' | b'%' | b'&' | b'|' | b'\\'))
        }
        _ => false,
    }
}

fn assigned_identifier(source: &str, literal_start: usize) -> Option<String> {
    let prefix = &source[..literal_start];
    let statement_start = prefix
        .rfind(['\n', ';', '{', '}'])
        .map_or(0, |index| index + 1);
    let statement = &prefix[statement_start..];
    let equals = statement.rfind('=')?;
    let bytes = statement.as_bytes();
    if equals > 0 && matches!(bytes[equals - 1], b'=' | b'!' | b'<' | b'>') {
        return None;
    }
    if bytes.get(equals + 1) == Some(&b'=') {
        return None;
    }
    let rhs = statement[equals + 1..].trim();
    if !rhs
        .bytes()
        .all(|byte| byte.is_ascii_alphabetic() || matches!(byte, b'$' | b'@'))
    {
        return None;
    }
    let lhs = statement[..equals].trim_end_matches([' ', '\t', ':']);
    let lhs = lhs
        .rsplit_once(':')
        .map_or(lhs, |(value, _)| value.trim_end());
    let identifier = lhs
        .rsplit(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .next()?;
    (!identifier.is_empty()).then(|| identifier.to_string())
}

fn execution_call_uses_identifier(source: &str, identifier: &str) -> bool {
    let bounded_end = source
        .char_indices()
        .nth(8192)
        .map_or(source.len(), |(index, _)| index);
    let bounded = &source[..bounded_end];
    let lower = bounded.to_ascii_lowercase();
    EXECUTION_CALLS.iter().any(|marker| {
        lower.match_indices(marker).any(|(position, _)| {
            if !trusted_execution_marker(&lower, position, marker)
                || identifier_reassigned_before(&lower, identifier, position)
            {
                return false;
            }
            let Some(open) = execution_open_paren(&lower, position, marker) else {
                return false;
            };
            let Some(close) = matching_paren(&lower, open) else {
                return false;
            };
            first_argument_is_identifier(&bounded[open + 1..close], identifier)
        })
    })
}

fn trusted_execution_marker(source: &str, marker_start: usize, marker: &str) -> bool {
    if marker.starts_with('@') || marker.starts_with("sqlx::") {
        return true;
    }
    if matches!(
        marker,
        "sqlite3_exec" | "sqlite3_prepare_v2" | "mysql_query" | "pqexec"
    ) {
        return source[..marker_start]
            .chars()
            .next_back()
            .is_none_or(|character| {
                !(character.is_ascii_alphanumeric()
                    || matches!(character, '_' | '$' | '.' | ':' | '>'))
            });
    }
    let receiver = execution_receiver(source, marker_start);
    let Some(receiver) = receiver else {
        return false;
    };
    let receiver = receiver
        .trim_start_matches(['_', '$'])
        .replace('_', "")
        .to_ascii_lowercase();
    matches!(
        receiver.as_str(),
        "db" | "database"
            | "databaseclient"
            | "dbclient"
            | "dbconnection"
            | "connection"
            | "conn"
            | "cursor"
            | "jdbc"
            | "jdbctemplate"
            | "entitymanager"
            | "session"
            | "queryrunner"
            | "client"
            | "pool"
            | "sequelize"
            | "prisma"
            | "knex"
            | "sql"
            | "pdo"
            | "mysqli"
            | "dbal"
            | "doctrine"
    )
}

fn literal_is_execution_argument(
    source: &str,
    call_open: usize,
    literal_start: usize,
    marker: &str,
) -> bool {
    let prefix = &source[call_open + 1..literal_start];
    let mut depth = 0usize;
    let mut commas = 0usize;
    let bytes = prefix.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'\'' | b'"' | b'`' => index = skip_quoted(bytes, index, bytes.len()),
            b',' if depth == 0 => commas += 1,
            _ => {}
        }
        index += 1;
    }
    let expected = usize::from(matches!(
        marker,
        "sqlite3_exec" | "sqlite3_prepare_v2" | "mysql_query" | "pqexec"
    ));
    commas == expected && (expected > 0 || prefix.trim().is_empty())
}

fn execution_receiver(source: &str, marker_start: usize) -> Option<&str> {
    let prefix = source.get(..marker_start)?.trim_end();
    let start = prefix
        .char_indices()
        .rev()
        .take_while(|(_, character)| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '$')
        })
        .last()
        .map_or(prefix.len(), |(index, _)| index);
    (start < prefix.len()).then(|| &prefix[start..])
}

fn identifier_reassigned_before(source: &str, identifier: &str, before: usize) -> bool {
    let identifier = identifier.to_ascii_lowercase();
    source[..before]
        .match_indices(&identifier)
        .any(|(index, _)| {
            let token_end = index + identifier.len();
            let token_is_bounded =
                source[..index].chars().next_back().is_none_or(|character| {
                    !(character.is_ascii_alphanumeric() || character == '_')
                }) && source[token_end..].chars().next().is_none_or(|character| {
                    !(character.is_ascii_alphanumeric() || character == '_')
                });
            token_is_bounded && assignment_follows(&source[token_end..])
        })
}

fn assignment_follows(source: &str) -> bool {
    let source = source.trim_start();
    if source.starts_with(":=") {
        return true;
    }
    if source
        .as_bytes()
        .first()
        .is_some_and(|byte| matches!(byte, b'+' | b'-' | b'*' | b'/' | b'%' | b'&' | b'|' | b'^'))
        && source.as_bytes().get(1) == Some(&b'=')
    {
        return true;
    }
    source.starts_with('=') && !source.starts_with("==") && !source.starts_with("=>")
}

fn execution_open_paren(source: &str, marker_start: usize, marker: &str) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut index = marker_start + marker.len();
    if bytes
        .get(index)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        return None;
    }
    while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
        index += 1;
    }
    if bytes.get(index) == Some(&b'!') {
        index += 1;
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
    }
    if bytes.get(index) == Some(&b'<') {
        let mut depth = 0usize;
        while index < bytes.len() {
            match bytes[index] {
                b'<' => depth += 1,
                b'>' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        index += 1;
                        break;
                    }
                }
                _ => {}
            }
            index += 1;
        }
        if depth != 0 {
            return None;
        }
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
    }
    (bytes.get(index) == Some(&b'(')).then_some(index)
}

fn call_depth_at(source: &str, open: usize, end: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    let mut index = open;
    while index < end {
        match bytes[index] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return None;
                }
            }
            b'\'' | b'"' | b'`' => index = skip_quoted(bytes, index, end),
            _ => {}
        }
        index += 1;
    }
    (depth > 0).then_some(depth)
}

fn matching_paren(source: &str, open: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    let mut index = open;
    while index < bytes.len() {
        match bytes[index] {
            b'(' => depth += 1,
            b')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            b'\'' | b'"' | b'`' => index = skip_quoted(bytes, index, bytes.len()),
            _ => {}
        }
        index += 1;
    }
    None
}

fn skip_quoted(bytes: &[u8], quote_index: usize, end: usize) -> usize {
    let quote = bytes[quote_index];
    let mut index = quote_index + 1;
    while index < end {
        if bytes[index] == b'\\' {
            index = (index + 2).min(end);
            continue;
        }
        if bytes[index] == quote {
            return index;
        }
        index += 1;
    }
    end.saturating_sub(1)
}

fn first_argument_is_identifier(source: &str, identifier: &str) -> bool {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'\'' | b'"' | b'`' => index = skip_quoted(bytes, index, bytes.len()),
            b',' if depth == 0 => break,
            _ => {}
        }
        index += 1;
    }
    source[..index].trim() == identifier
}

fn extract_static_literals(source: &str) -> Vec<StaticLiteral> {
    let bytes = source.as_bytes();
    let mut literals = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if let Some((literal, next)) = rust_raw_literal_at(bytes, index) {
            if !contains_interpolation_marker(&literal.value) {
                literals.push(literal);
            }
            index = next;
            continue;
        }
        let quote = bytes[index];
        if !matches!(quote, b'\'' | b'"' | b'`') {
            index += 1;
            continue;
        }
        let triple = bytes.get(index..index + 3) == Some(&[quote, quote, quote]);
        let delimiter_len = if triple { 3 } else { 1 };
        let prefix_start = (0..index)
            .rev()
            .take_while(|position| {
                bytes[*position].is_ascii_alphabetic() || matches!(bytes[*position], b'$' | b'@')
            })
            .last()
            .unwrap_or(index);
        let prefix = &bytes[prefix_start..index];
        let dynamic_prefix = prefix.iter().any(|byte| matches!(byte, b'f' | b'F' | b'$'));
        let expression_start = static_literal_prefix_start(quote, prefix, prefix_start, index);
        let start = index + delimiter_len;
        index = start;
        while index < bytes.len() {
            if !triple && bytes[index] == b'\\' {
                index = (index + 2).min(bytes.len());
                continue;
            }
            let closes = if triple {
                bytes.get(index..index + 3) == Some(&[quote, quote, quote])
            } else {
                bytes[index] == quote
            };
            if closes {
                let value = String::from_utf8_lossy(&bytes[start..index]).into_owned();
                let dynamic = dynamic_prefix || contains_interpolation_marker(&value);
                if let (false, Some(expression_start)) = (dynamic, expression_start) {
                    literals.push(StaticLiteral {
                        value,
                        start: expression_start,
                        end: index + delimiter_len,
                    });
                }
                index += delimiter_len;
                break;
            }
            index += 1;
        }
    }
    literals
}

fn rust_raw_literal_at(bytes: &[u8], start: usize) -> Option<(StaticLiteral, usize)> {
    if bytes.get(start) != Some(&b'r')
        || start.checked_sub(1).is_some_and(|previous| {
            bytes[previous].is_ascii_alphanumeric() || bytes[previous] == b'_'
        })
    {
        return None;
    }
    let mut quote = start + 1;
    while bytes.get(quote) == Some(&b'#') {
        quote += 1;
    }
    let hashes = quote.saturating_sub(start + 1);
    if hashes == 0 || bytes.get(quote) != Some(&b'"') {
        return None;
    }
    let value_start = quote + 1;
    let mut close = value_start;
    while close < bytes.len() {
        if bytes[close] == b'"'
            && bytes
                .get(close + 1..close + 1 + hashes)
                .is_some_and(|suffix| suffix.iter().all(|byte| *byte == b'#'))
        {
            let end = close + 1 + hashes;
            return Some((
                StaticLiteral {
                    value: String::from_utf8_lossy(&bytes[value_start..close]).into_owned(),
                    start,
                    end,
                },
                end,
            ));
        }
        close += 1;
    }
    None
}

fn static_literal_prefix_start(
    quote: u8,
    prefix: &[u8],
    prefix_start: usize,
    literal_start: usize,
) -> Option<usize> {
    if prefix.is_empty() {
        return Some(literal_start);
    }
    (quote != b'`' && matches!(prefix, b"r" | b"R" | b"@")).then_some(prefix_start)
}

fn contains_interpolation_marker(value: &str) -> bool {
    value.contains("${")
        || value.contains("#{")
        || value.as_bytes().windows(2).any(|window| {
            window[0] == b'$' && (window[1].is_ascii_alphabetic() || window[1] == b'_')
        })
}

fn strip_comments(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if let Some((_, next)) = rust_raw_literal_at(bytes, index) {
            output.extend_from_slice(&bytes[index..next]);
            index = next;
        } else if bytes.get(index..index + 2) == Some(b"//") {
            while index < bytes.len() && bytes[index] != b'\n' {
                output.push(b' ');
                index += 1;
            }
        } else if bytes.get(index..index + 2) == Some(b"/*") {
            output.extend_from_slice(b"  ");
            index += 2;
            while index < bytes.len() && bytes.get(index..index + 2) != Some(b"*/") {
                output.push(if bytes[index] == b'\n' { b'\n' } else { b' ' });
                index += 1;
            }
            if index < bytes.len() {
                output.extend_from_slice(b"  ");
                index += 2;
            }
        } else if bytes[index] == b'#' {
            while index < bytes.len() && bytes[index] != b'\n' {
                output.push(b' ');
                index += 1;
            }
        } else if matches!(bytes[index], b'\'' | b'"' | b'`') {
            let quote = bytes[index];
            output.push(quote);
            index += 1;
            while index < bytes.len() {
                output.push(bytes[index]);
                if bytes[index] == b'\\' {
                    index += 1;
                    if index < bytes.len() {
                        output.push(bytes[index]);
                    }
                } else if bytes[index] == quote {
                    index += 1;
                    break;
                }
                index += 1;
            }
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&output).into_owned()
}

