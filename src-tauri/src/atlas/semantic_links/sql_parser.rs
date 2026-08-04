use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum QueryOperation {
    Select,
    Insert,
    Update,
    Delete,
    Merge,
}

impl QueryOperation {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Select => "SELECT",
            Self::Insert => "INSERT",
            Self::Update => "UPDATE",
            Self::Delete => "DELETE",
            Self::Merge => "MERGE",
        }
    }

    pub(super) fn edge_kind(self) -> &'static str {
        match self {
            Self::Select => "code_db_read",
            _ => "code_db_write",
        }
    }

    pub(super) fn edge_type(self) -> &'static str {
        match self {
            Self::Select => "READS",
            _ => "WRITES",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct QueryEvidence {
    pub(super) operation: QueryOperation,
    pub(super) columns: BTreeSet<String>,
    pub(super) line_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct QueryTableAccess {
    pub(super) token: String,
    pub(super) alias: Option<String>,
    pub(super) operation: QueryOperation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ParsedQuery {
    pub(super) accesses: Vec<QueryTableAccess>,
    pub(super) identifiers: BTreeSet<String>,
    pub(super) line_offset: usize,
}

pub(super) fn analyze_source(
    source: &str,
    table: &str,
    schema: Option<&str>,
    columns: &[&str],
    schema_ambiguous: bool,
) -> Vec<QueryEvidence> {
    let mut evidence = Vec::new();
    for query in parse_source(source) {
        for access in query.accesses.iter().filter(|access| {
            table_reference_matches(&access.token, table, schema, schema_ambiguous)
        }) {
            evidence.push(QueryEvidence {
                operation: access.operation,
                columns: matched_target_columns(&query, access, columns),
                line_offset: query.line_offset,
            });
        }
    }
    evidence.sort_by(|left, right| {
        left.line_offset
            .cmp(&right.line_offset)
            .then_with(|| left.operation.as_str().cmp(right.operation.as_str()))
    });
    evidence.dedup();
    evidence
}

pub(super) fn parse_source(source: &str) -> Vec<ParsedQuery> {
    let sanitized = strip_comments(source);
    let mut queries = extract_static_literals(&sanitized)
        .into_iter()
        .filter(|literal| literal_is_executed(&sanitized, literal))
        .filter_map(|literal| {
            let mut query = parse_sql_literal(&literal.value)?;
            query.line_offset = sanitized[..literal.start]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count();
            Some(query)
        })
        .collect::<Vec<_>>();
    queries.sort_by_key(|query| query.line_offset);
    queries.dedup();
    queries
}

fn parse_sql_literal(literal: &str) -> Option<ParsedQuery> {
    if literal.contains('#') {
        return None;
    }
    let tokens = sql_tokens(literal);
    if tokens.first().is_some_and(|token| token == "with") {
        return None;
    }
    let statement_count = tokens
        .split(|token| token == ";")
        .filter(|statement| statement.iter().any(|token| is_sql_operation(token)))
        .count();
    if statement_count != 1 || has_legacy_comma_table_list(&tokens) {
        return None;
    }
    let (operation_index, operation) = tokens.iter().enumerate().find_map(|(index, token)| {
        let operation = match token.as_str() {
            "select" => QueryOperation::Select,
            "insert" => QueryOperation::Insert,
            "update" => QueryOperation::Update,
            "delete" => QueryOperation::Delete,
            "merge" => QueryOperation::Merge,
            _ => return None,
        };
        (index <= 16).then_some((index, operation))
    })?;
    if has_unsupported_projection_syntax(&tokens, operation_index, operation) {
        return None;
    }
    let accesses = table_accesses(&tokens, operation_index, operation);
    if accesses.is_empty() {
        return None;
    }
    let identifiers = sql_column_identifiers(&tokens, &accesses, operation_index, operation);
    Some(ParsedQuery {
        accesses,
        identifiers,
        line_offset: 0,
    })
}

fn has_unsupported_projection_syntax(
    tokens: &[String],
    operation_index: usize,
    operation: QueryOperation,
) -> bool {
    if operation != QueryOperation::Select {
        return false;
    }
    let projection = tokens
        .iter()
        .skip(operation_index + 1)
        .take_while(|token| token.as_str() != "from")
        .map(String::as_str)
        .collect::<Vec<_>>();
    if projection.windows(2).any(|pair| pair == ["distinct", "on"]) {
        return true;
    }
    let top_index =
        usize::from(projection.first() == Some(&"all") || projection.first() == Some(&"distinct"));
    projection.get(top_index) == Some(&"top")
        && projection.get(top_index + 1).is_some_and(|next| {
            *next == "(" || *next == "<parameter>" || next.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn sql_column_identifiers(
    tokens: &[String],
    accesses: &[QueryTableAccess],
    operation_index: usize,
    operation: QueryOperation,
) -> BTreeSet<String> {
    let access_names = accesses
        .iter()
        .flat_map(|access| [Some(access.token.as_str()), access.alias.as_deref()])
        .flatten()
        .map(str::to_ascii_lowercase)
        .collect::<HashSet<_>>();
    let projection_end = (operation == QueryOperation::Select).then(|| {
        tokens
            .iter()
            .enumerate()
            .skip(operation_index + 1)
            .find_map(|(index, token)| (token == "from").then_some(index))
            .unwrap_or(tokens.len())
    });

    tokens
        .iter()
        .enumerate()
        .filter_map(|(index, token)| {
            if !is_sql_identifier(token)
                || access_names.contains(token)
                || index
                    .checked_sub(1)
                    .and_then(|previous| tokens.get(previous))
                    .is_some_and(|previous| matches!(previous.as_str(), "as" | "collate"))
                || tokens.get(index + 1).is_some_and(|next| next == "(")
                || projection_end.is_some_and(|end| {
                    index < end && implicit_projection_alias(tokens, index, operation_index)
                })
            {
                return None;
            }
            Some(token.clone())
        })
        .collect()
}

fn implicit_projection_alias(tokens: &[String], index: usize, operation_index: usize) -> bool {
    if index <= operation_index + 1 {
        return false;
    }
    let Some(previous) = tokens.get(index - 1) else {
        return false;
    };
    previous == ")"
        || previous == "end"
        || previous == "<value>"
        || previous == "null"
        || previous == "true"
        || previous == "false"
        || is_sql_identifier(previous)
}

fn is_sql_operation(value: &str) -> bool {
    matches!(value, "select" | "insert" | "update" | "delete" | "merge")
}

fn has_legacy_comma_table_list(tokens: &[String]) -> bool {
    tokens.iter().enumerate().any(|(index, token)| {
        if !matches!(token.as_str(), "from" | "using") {
            return false;
        }
        tokens
            .iter()
            .skip(index + 1)
            .take_while(|candidate| {
                !matches!(
                    candidate.as_str(),
                    ";" | "group"
                        | "having"
                        | "join"
                        | "limit"
                        | "offset"
                        | "on"
                        | "order"
                        | "returning"
                        | "set"
                        | "union"
                        | "values"
                        | "when"
                        | "where"
                )
            })
            .any(|candidate| candidate == ",")
    })
}

pub(super) fn resolve_table<'a>(
    token: &str,
    tables_by_name: &HashMap<String, Vec<&'a InventoryItem>>,
) -> Vec<&'a InventoryItem> {
    let parts = token.split('.').collect::<Vec<_>>();
    let name = parts.last().copied().unwrap_or(token);
    let schema = (parts.len() > 1).then(|| parts[parts.len() - 2]);
    tables_by_name
        .get(&name.to_ascii_lowercase())
        .into_iter()
        .flatten()
        .copied()
        .filter(|table| table.name.eq_ignore_ascii_case(name))
        .filter(|table| {
            schema.is_none_or(|schema| {
                table
                    .group_id
                    .as_deref()
                    .is_some_and(|group| group.eq_ignore_ascii_case(schema))
            })
        })
        .collect()
}

pub(super) fn matched_columns(
    query: &ParsedQuery,
    access: &QueryTableAccess,
    table: &InventoryItem,
    columns: &[InventoryItem],
    resolved_accesses: &[(&QueryTableAccess, &InventoryItem)],
    columns_by_table: &HashMap<String, Vec<InventoryItem>>,
) -> BTreeSet<String> {
    query
        .identifiers
        .iter()
        .filter_map(|token| {
            let name = unqualified_identifier(token);
            let column = columns
                .iter()
                .find(|column| column.name.eq_ignore_ascii_case(name))
                .map(|column| column.name.clone())?;
            if token.contains('.') {
                return identifier_has_unique_access_owner(token, &query.accesses, access)
                    .then_some(column);
            }
            if resolved_accesses.len() != query.accesses.len() {
                return None;
            }
            let owners = resolved_accesses
                .iter()
                .filter(|(_, owner)| {
                    columns_by_table
                        .get(owner.id.as_str())
                        .is_some_and(|items| {
                            items
                                .iter()
                                .any(|candidate| candidate.name.eq_ignore_ascii_case(name))
                        })
                })
                .map(|(_, owner)| owner.id.as_str())
                .collect::<BTreeSet<_>>();
            (owners.len() == 1 && owners.contains(table.id.as_str())).then_some(column)
        })
        .collect()
}

fn matched_target_columns(
    query: &ParsedQuery,
    access: &QueryTableAccess,
    columns: &[&str],
) -> BTreeSet<String> {
    let unique_tables = query
        .accesses
        .iter()
        .map(|item| item.token.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    query
        .identifiers
        .iter()
        .filter_map(|token| {
            let name = unqualified_identifier(token);
            let column = columns
                .iter()
                .find(|column| column.eq_ignore_ascii_case(name))?;
            if token.contains('.') {
                return identifier_has_unique_access_owner(token, &query.accesses, access)
                    .then(|| (*column).to_string());
            }
            (unique_tables.len() == 1).then(|| (*column).to_string())
        })
        .collect()
}

fn table_accesses(
    tokens: &[String],
    operation_index: usize,
    operation: QueryOperation,
) -> Vec<QueryTableAccess> {
    let mut accesses = Vec::new();
    match operation {
        QueryOperation::Select => {
            push_marker_accesses(
                tokens,
                operation_index,
                &["from", "join"],
                QueryOperation::Select,
                &mut accesses,
            );
        }
        QueryOperation::Insert => {
            push_marker_accesses(
                tokens,
                operation_index,
                &["into"],
                QueryOperation::Insert,
                &mut accesses,
            );
            push_marker_accesses(
                tokens,
                operation_index,
                &["from", "join"],
                QueryOperation::Select,
                &mut accesses,
            );
        }
        QueryOperation::Update => {
            if let Some(access) =
                table_access_after(tokens, operation_index + 1, QueryOperation::Update)
            {
                accesses.push(access);
            }
            push_marker_accesses(
                tokens,
                operation_index + 1,
                &["from", "join"],
                QueryOperation::Select,
                &mut accesses,
            );
        }
        QueryOperation::Delete => {
            push_first_marker_access(
                tokens,
                operation_index,
                "from",
                QueryOperation::Delete,
                &mut accesses,
            );
            push_marker_accesses(
                tokens,
                operation_index,
                &["using", "join"],
                QueryOperation::Select,
                &mut accesses,
            );
        }
        QueryOperation::Merge => {
            push_first_marker_access(
                tokens,
                operation_index,
                "into",
                QueryOperation::Merge,
                &mut accesses,
            );
            push_first_marker_access(
                tokens,
                operation_index,
                "using",
                QueryOperation::Select,
                &mut accesses,
            );
        }
    }
    accesses.sort_by(|left, right| {
        left.operation
            .as_str()
            .cmp(right.operation.as_str())
            .then_with(|| left.token.cmp(&right.token))
            .then_with(|| left.alias.cmp(&right.alias))
    });
    accesses.dedup();
    accesses
}

fn push_marker_accesses(
    tokens: &[String],
    start: usize,
    markers: &[&str],
    operation: QueryOperation,
    accesses: &mut Vec<QueryTableAccess>,
) {
    for (index, _) in tokens
        .iter()
        .enumerate()
        .skip(start)
        .filter(|(_, token)| markers.contains(&token.as_str()))
    {
        if let Some(access) = table_access_after(tokens, index + 1, operation) {
            accesses.push(access);
        }
    }
}

fn push_first_marker_access(
    tokens: &[String],
    start: usize,
    marker: &str,
    operation: QueryOperation,
    accesses: &mut Vec<QueryTableAccess>,
) {
    let Some(index) = tokens
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(index, token)| (token == marker).then_some(index))
    else {
        return;
    };
    if let Some(access) = table_access_after(tokens, index + 1, operation) {
        accesses.push(access);
    }
}

fn table_access_after(
    tokens: &[String],
    table_index: usize,
    operation: QueryOperation,
) -> Option<QueryTableAccess> {
    let token = tokens.get(table_index)?;
    if !is_sql_identifier(token) {
        return None;
    }
    if operation == QueryOperation::Select
        && tokens.get(table_index + 1).is_some_and(|next| next == "(")
    {
        return None;
    }
    let alias = match tokens.get(table_index + 1).map(String::as_str) {
        Some("as") => tokens
            .get(table_index + 2)
            .filter(|candidate| is_sql_identifier(candidate))
            .cloned(),
        Some(candidate) if is_sql_identifier(candidate) => Some(candidate.to_string()),
        _ => None,
    };
    Some(QueryTableAccess {
        token: token.clone(),
        alias,
        operation,
    })
}

fn is_sql_identifier(value: &str) -> bool {
    !value.is_empty()
        && !is_sql_keyword(value)
        && !matches!(
            value,
            "(" | ")"
                | ","
                | ";"
                | "+"
                | "-"
                | "*"
                | "/"
                | "%"
                | "="
                | "<"
                | ">"
                | "<value>"
                | "<parameter>"
        )
}

fn is_sql_keyword(value: &str) -> bool {
    matches!(
        value,
        "all"
            | "and"
            | "asc"
            | "as"
            | "by"
            | "case"
            | "collate"
            | "cross"
            | "delete"
            | "desc"
            | "distinct"
            | "else"
            | "end"
            | "false"
            | "from"
            | "full"
            | "group"
            | "having"
            | "inner"
            | "insert"
            | "into"
            | "join"
            | "left"
            | "limit"
            | "lateral"
            | "merge"
            | "natural"
            | "not"
            | "null"
            | "offset"
            | "on"
            | "only"
            | "or"
            | "order"
            | "outer"
            | "returning"
            | "right"
            | "select"
            | "set"
            | "then"
            | "true"
            | "union"
            | "update"
            | "using"
            | "values"
            | "when"
            | "where"
            | "with"
    )
}

fn table_reference_matches(
    value: &str,
    table: &str,
    schema: Option<&str>,
    require_schema: bool,
) -> bool {
    let parts = value.split('.').collect::<Vec<_>>();
    if !parts
        .last()
        .is_some_and(|name| name.eq_ignore_ascii_case(table))
    {
        return false;
    }
    if require_schema {
        return schema.is_some_and(|schema| {
            parts.len() > 1 && parts[parts.len() - 2].eq_ignore_ascii_case(schema)
        });
    }
    match (schema, parts.len()) {
        (Some(schema), length) if length > 1 => parts[length - 2].eq_ignore_ascii_case(schema),
        _ => true,
    }
}

fn identifier_has_unique_access_owner(
    identifier: &str,
    accesses: &[QueryTableAccess],
    expected: &QueryTableAccess,
) -> bool {
    let owners = accesses
        .iter()
        .filter(|access| identifier_belongs_to_query_access(identifier, access))
        .map(|access| {
            (
                access.token.to_ascii_lowercase(),
                access
                    .alias
                    .as_deref()
                    .unwrap_or_default()
                    .to_ascii_lowercase(),
            )
        })
        .collect::<BTreeSet<_>>();
    owners.len() == 1
        && owners.contains(&(
            expected.token.to_ascii_lowercase(),
            expected
                .alias
                .as_deref()
                .unwrap_or_default()
                .to_ascii_lowercase(),
        ))
}

fn identifier_belongs_to_query_access(identifier: &str, access: &QueryTableAccess) -> bool {
    let Some((qualifier, _)) = identifier.rsplit_once('.') else {
        return false;
    };
    if access
        .alias
        .as_deref()
        .is_some_and(|alias| qualifier.eq_ignore_ascii_case(alias))
    {
        return true;
    }
    qualifier.eq_ignore_ascii_case(&access.token)
        || access
            .token
            .rsplit('.')
            .next()
            .is_some_and(|table| qualifier.eq_ignore_ascii_case(table))
}

fn unqualified_identifier(value: &str) -> &str {
    value.rsplit('.').next().unwrap_or(value)
}

fn sql_tokens(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes.get(index..index + 2) == Some(b"--") {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"/*") {
            index += 2;
            while index < bytes.len() && bytes.get(index..index + 2) != Some(b"*/") {
                index += 1;
            }
            index = (index + 2).min(bytes.len());
            continue;
        }
        match bytes[index] {
            b'\'' => {
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == b'\'' {
                        if bytes.get(index + 1) == Some(&b'\'') {
                            index += 2;
                        } else {
                            index += 1;
                            break;
                        }
                    } else {
                        index += 1;
                    }
                }
                tokens.push("<value>".to_string());
            }
            b'"' | b'`' | b'[' => {
                let (identifier, next) = sql_identifier_chain(bytes, index);
                if !identifier.is_empty() {
                    tokens.push(identifier.to_ascii_lowercase());
                }
                index = next;
            }
            byte if byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$' => {
                let start = index;
                let parameter =
                    start > 0 && matches!(bytes[start - 1], b':' | b'@') || bytes[start] == b'$';
                let (identifier, next) = sql_identifier_chain(bytes, index);
                if parameter {
                    tokens.push("<parameter>".to_string());
                } else {
                    tokens.push(identifier.to_ascii_lowercase());
                }
                index = next;
            }
            b'(' | b')' | b',' | b';' | b'+' | b'-' | b'*' | b'/' | b'%' | b'=' | b'<' | b'>' => {
                tokens.push((bytes[index] as char).to_string());
                index += 1;
            }
            b'#' => {
                let start = index;
                index += 1;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || matches!(bytes[index], b'_' | b'#'))
                {
                    index += 1;
                }
                tokens.push(String::from_utf8_lossy(&bytes[start..index]).to_ascii_lowercase());
            }
            _ => index += 1,
        }
    }
    tokens
}

fn sql_identifier_chain(bytes: &[u8], start: usize) -> (String, usize) {
    let mut parts = Vec::new();
    let mut index = start;
    while let Some(&opening) = bytes.get(index) {
        let mut part = Vec::new();
        if matches!(opening, b'"' | b'`' | b'[') {
            let closing = if opening == b'[' { b']' } else { opening };
            index += 1;
            while index < bytes.len() {
                if bytes[index] == closing {
                    if bytes.get(index + 1) == Some(&closing) {
                        part.push(closing);
                        index += 2;
                        continue;
                    }
                    index += 1;
                    break;
                }
                part.push(bytes[index]);
                index += 1;
            }
            for byte in &mut part {
                if *byte == b'.' {
                    *byte = 0;
                }
            }
        } else if opening.is_ascii_alphanumeric() || matches!(opening, b'_' | b'$') {
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || matches!(bytes[index], b'_' | b'$'))
            {
                part.push(bytes[index]);
                index += 1;
            }
        } else {
            break;
        }
        parts.push(String::from_utf8_lossy(&part).into_owned());

        let mut next = index;
        while bytes.get(next).is_some_and(u8::is_ascii_whitespace) {
            next += 1;
        }
        if bytes.get(next) != Some(&b'.') {
            index = next;
            break;
        }
        next += 1;
        while bytes.get(next).is_some_and(u8::is_ascii_whitespace) {
            next += 1;
        }
        if !bytes.get(next).is_some_and(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$' | b'"' | b'`' | b'[')
        }) {
            break;
        }
        index = next;
    }
    (parts.join("."), index)
}

include!("sql_parser/static_literals.rs");
