use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    sync::{Arc, Mutex, OnceLock},
};

use super::{
    linker::{record_code_search_gap, AppliedCodeMatch},
    model::{Evidence, InventoryItem, InventorySnapshot, SnapshotLink},
};

const MAX_SOURCE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_FUNCTION_LINES: usize = 240;
const MAX_SEMANTIC_CACHE_ENTRIES: usize = 64;

static SEMANTIC_LINK_CACHE: OnceLock<Mutex<HashMap<SemanticCacheKey, Arc<Vec<SnapshotLink>>>>> =
    OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SemanticCacheKey {
    workspace_id: String,
    saved_at: String,
    repo_path: String,
    code_ids: Vec<String>,
    source_signature: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QueryOperation {
    Select,
    Insert,
    Update,
    Delete,
    Merge,
}

impl QueryOperation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Select => "SELECT",
            Self::Insert => "INSERT",
            Self::Update => "UPDATE",
            Self::Delete => "DELETE",
            Self::Merge => "MERGE",
        }
    }

    fn edge_kind(self) -> &'static str {
        match self {
            Self::Select => "code_db_read",
            _ => "code_db_write",
        }
    }

    fn edge_type(self) -> &'static str {
        match self {
            Self::Select => "READS",
            _ => "WRITES",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct QueryEvidence {
    operation: QueryOperation,
    columns: BTreeSet<String>,
    line_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct QueryTableAccess {
    token: String,
    alias: Option<String>,
    operation: QueryOperation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedQuery {
    accesses: Vec<QueryTableAccess>,
    identifiers: BTreeSet<String>,
    line_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StaticLiteral {
    value: String,
    start: usize,
    end: usize,
}

pub(crate) fn apply_explicit_query_evidence(
    snapshot: &mut InventorySnapshot,
    table_id: &str,
    repo_path: &str,
    matches: &[AppliedCodeMatch],
    schema_ambiguous: bool,
) -> usize {
    let Some(table) = snapshot
        .items
        .iter()
        .find(|item| item.id == table_id && item.source == "db" && item.kind == "table")
        .cloned()
    else {
        return 0;
    };
    let columns = snapshot
        .items
        .iter()
        .filter(|item| item.kind == "column" && item.parent_id.as_deref() == Some(table_id))
        .cloned()
        .collect::<Vec<_>>();
    let mut confirmed = 0;

    for matched in matches {
        let (snippet, first_line) = match read_match_source(repo_path, matched) {
            Ok(source) => source,
            Err(error) => {
                record_inspection_gap(snapshot, table_id, matched, &error);
                continue;
            }
        };
        let column_names = columns
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>();
        let evidence = analyze_source(
            &snippet,
            table.name.as_str(),
            table.group_id.as_deref(),
            &column_names,
            schema_ambiguous,
        );
        for query in evidence {
            confirmed += apply_query_links(
                snapshot,
                matched,
                &table,
                &columns,
                &query,
                first_line + query.line_offset,
            );
        }
    }

    confirmed
}

pub(crate) fn apply_explicit_query_evidence_for_code(
    snapshot: &mut InventorySnapshot,
    repo_path: &str,
    code_ids: &[String],
) -> usize {
    let mut normalized_code_ids = code_ids.to_vec();
    normalized_code_ids.sort();
    normalized_code_ids.dedup();
    let cache_key = SemanticCacheKey {
        workspace_id: snapshot.workspace_id.clone(),
        saved_at: snapshot.saved_at.clone(),
        repo_path: repo_path.to_string(),
        code_ids: normalized_code_ids.clone(),
        source_signature: semantic_source_signature(snapshot, repo_path, &normalized_code_ids),
    };
    if let Some(links) = cached_semantic_links(&cache_key) {
        return append_cached_links(snapshot, &links);
    }

    let existing_link_ids = snapshot
        .links
        .iter()
        .map(|link| link.id.clone())
        .collect::<HashSet<_>>();
    let confirmed =
        apply_explicit_query_evidence_for_code_uncached(snapshot, repo_path, &normalized_code_ids);
    let links = snapshot
        .links
        .iter()
        .filter(|link| !existing_link_ids.contains(&link.id) && is_semantic_link(link))
        .cloned()
        .collect::<Vec<_>>();
    cache_semantic_links(cache_key, links);
    confirmed
}

fn apply_explicit_query_evidence_for_code_uncached(
    snapshot: &mut InventorySnapshot,
    repo_path: &str,
    code_ids: &[String],
) -> usize {
    let selected_ids = code_ids.iter().map(String::as_str).collect::<HashSet<_>>();
    let matches = snapshot
        .items
        .iter()
        .filter(|item| item.source == "code" && selected_ids.contains(item.id.as_str()))
        .filter_map(|item| {
            let path = item
                .path
                .clone()
                .or_else(|| item.location.as_ref().map(|location| location.path.clone()))?;
            let start_line = item
                .location
                .as_ref()
                .and_then(|location| location.line)
                .unwrap_or(1);
            let end_line = item
                .location
                .as_ref()
                .and_then(|location| location.end_line)
                .unwrap_or(start_line.saturating_add(MAX_FUNCTION_LINES as u64 - 1));
            Some(AppliedCodeMatch {
                item_id: item.id.clone(),
                file: path,
                start_line,
                end_line,
                match_lines: vec![start_line],
            })
        })
        .collect::<Vec<_>>();
    let tables = snapshot
        .items
        .iter()
        .filter(|item| item.source == "db" && item.kind == "table")
        .cloned()
        .collect::<Vec<_>>();
    let columns_by_table = snapshot
        .items
        .iter()
        .filter(|item| item.source == "db" && item.kind == "column")
        .filter_map(|column| {
            column
                .parent_id
                .clone()
                .map(|parent| (parent, column.clone()))
        })
        .fold(
            HashMap::<String, Vec<InventoryItem>>::new(),
            |mut grouped, (parent, column)| {
                grouped.entry(parent).or_default().push(column);
                grouped
            },
        );
    let mut confirmed = 0;

    for matched in &matches {
        let (snippet, line) = match read_match_source(repo_path, matched) {
            Ok(source) => source,
            Err(error) => {
                record_inspection_gap(snapshot, matched.item_id.as_str(), matched, &error);
                continue;
            }
        };
        for query in parse_source(&snippet) {
            let resolved_accesses = query
                .accesses
                .iter()
                .filter_map(|access| {
                    let resolved = resolve_table(&access.token, &tables);
                    (resolved.len() == 1).then_some((access, resolved[0]))
                })
                .collect::<Vec<_>>();
            for (access, table) in &resolved_accesses {
                let columns = columns_by_table
                    .get(table.id.as_str())
                    .cloned()
                    .unwrap_or_default();
                let evidence = QueryEvidence {
                    operation: access.operation,
                    columns: matched_columns(
                        &query,
                        access,
                        table,
                        &columns,
                        &resolved_accesses,
                        &columns_by_table,
                    ),
                    line_offset: query.line_offset,
                };
                confirmed += apply_query_links(
                    snapshot,
                    matched,
                    table,
                    &columns,
                    &evidence,
                    line + query.line_offset,
                );
            }
        }
    }
    confirmed
}

fn cached_semantic_links(key: &SemanticCacheKey) -> Option<Arc<Vec<SnapshotLink>>> {
    SEMANTIC_LINK_CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()?
        .get(key)
        .cloned()
}

fn semantic_source_signature(
    snapshot: &InventorySnapshot,
    repo_path: &str,
    code_ids: &[String],
) -> Vec<String> {
    code_ids
        .iter()
        .map(|code_id| {
            let Some(item) = snapshot.items.iter().find(|item| item.id == *code_id) else {
                return format!("{code_id}|missing-item");
            };
            let path = item
                .path
                .as_deref()
                .or_else(|| {
                    item.location
                        .as_ref()
                        .map(|location| location.path.as_str())
                })
                .unwrap_or_default();
            let line = item
                .location
                .as_ref()
                .and_then(|location| location.line)
                .unwrap_or_default();
            let end_line = item
                .location
                .as_ref()
                .and_then(|location| location.end_line)
                .unwrap_or_default();
            let stamp = crate::source::resolve_repo_source(repo_path, path)
                .and_then(|resolved| fs::metadata(resolved).map_err(|error| error.to_string()))
                .map(|metadata| {
                    let modified = metadata
                        .modified()
                        .ok()
                        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|duration| duration.as_nanos())
                        .unwrap_or_default();
                    format!("{}:{modified}", metadata.len())
                })
                .unwrap_or_else(|_| "missing-source".to_string());
            format!("{code_id}|{path}|{line}|{end_line}|{stamp}")
        })
        .collect()
}

fn cache_semantic_links(key: SemanticCacheKey, links: Vec<SnapshotLink>) {
    let Ok(mut cache) = SEMANTIC_LINK_CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
    else {
        return;
    };
    if cache.len() >= MAX_SEMANTIC_CACHE_ENTRIES {
        cache.clear();
    }
    cache.insert(key, Arc::new(links));
}

fn append_cached_links(snapshot: &mut InventorySnapshot, links: &[SnapshotLink]) -> usize {
    let known_ids = snapshot
        .items
        .iter()
        .map(|item| item.id.clone())
        .collect::<HashSet<_>>();
    let mut confirmed_tables = 0;
    for link in links {
        if !known_ids.contains(link.from.as_str()) || !known_ids.contains(link.to.as_str()) {
            continue;
        }
        if snapshot.links.iter().any(|existing| existing.id == link.id) {
            continue;
        }
        snapshot.links.push(link.clone());
        if matches!(link.kind.as_str(), "code_db_read" | "code_db_write") {
            confirmed_tables += 1;
        }
    }
    confirmed_tables
}

fn is_semantic_link(link: &SnapshotLink) -> bool {
    matches!(
        link.kind.as_str(),
        "code_db_read" | "code_db_write" | "code_db_uses_column"
    )
}

fn read_match_source(
    repo_path: &str,
    matched: &AppliedCodeMatch,
) -> Result<(String, usize), String> {
    let path = crate::source::resolve_repo_source(repo_path, &matched.file)?;
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    if metadata.len() > MAX_SOURCE_BYTES {
        return Err("파일이 2 MiB 검사 한도를 넘었습니다.".to_string());
    }
    let source = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let (snippet, first_line) = bounded_source_range(&source, matched)
        .ok_or_else(|| "코드 인벤토리의 줄 범위가 실제 파일과 맞지 않습니다.".to_string())?;
    Ok((snippet.to_string(), first_line))
}

fn record_inspection_gap(
    snapshot: &mut InventorySnapshot,
    table_id: &str,
    matched: &AppliedCodeMatch,
    reason: &str,
) {
    record_code_search_gap(
        snapshot,
        table_id,
        "explicit-sql-inspection-failure",
        &format!(
            "{}의 정적 SQL 실행 근거를 검사하지 못했습니다: {reason}",
            matched.file
        ),
        vec![matched.item_id.clone()],
    );
}

fn bounded_source_range<'a>(
    source: &'a str,
    matched: &AppliedCodeMatch,
) -> Option<(&'a str, usize)> {
    let lines = source.split_inclusive('\n').collect::<Vec<_>>();
    if lines.is_empty() {
        return None;
    }
    let start = matched.start_line.saturating_sub(1) as usize;
    if start >= lines.len() {
        return None;
    }
    let requested_end = matched.end_line.max(matched.start_line) as usize;
    let end = requested_end
        .min(start + MAX_FUNCTION_LINES)
        .min(lines.len());
    let start_byte = lines[..start].iter().map(|line| line.len()).sum::<usize>();
    let end_byte = start_byte
        + lines[start..end]
            .iter()
            .map(|line| line.len())
            .sum::<usize>();
    source
        .get(start_byte..end_byte)
        .map(|snippet| (snippet, start + 1))
}

fn analyze_source(
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

fn parse_source(source: &str) -> Vec<ParsedQuery> {
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

fn resolve_table<'a>(token: &str, tables: &'a [InventoryItem]) -> Vec<&'a InventoryItem> {
    let parts = token.split('.').collect::<Vec<_>>();
    let name = parts.last().copied().unwrap_or(token);
    let schema = (parts.len() > 1).then(|| parts[parts.len() - 2]);
    tables
        .iter()
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

fn matched_columns(
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

fn apply_query_links(
    snapshot: &mut InventorySnapshot,
    matched: &AppliedCodeMatch,
    table: &InventoryItem,
    columns: &[InventoryItem],
    query: &QueryEvidence,
    line: usize,
) -> usize {
    let location = format!("{}:L{line}", matched.file);
    let inserted = insert_confirmed_link(
        snapshot,
        &matched.item_id,
        &table.id,
        query.operation.edge_kind(),
        &format!("EXECUTES_QUERY · {}", query.operation.as_str()),
        query.operation.edge_type(),
        vec![Evidence {
            kind: "explicit-sql-execution".to_string(),
            text: format!(
                "{location}에서 실행되는 정적 {} 문이 {} 테이블을 직접 참조합니다.",
                query.operation.as_str(),
                qualified_table_name(table)
            ),
        }],
    );
    for column in columns.iter().filter(|column| {
        query
            .columns
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&column.name))
    }) {
        insert_confirmed_link(
            snapshot,
            &matched.item_id,
            &column.id,
            "code_db_uses_column",
            &format!("USES_COLUMN · {}", query.operation.as_str()),
            "USES_COLUMN",
            vec![Evidence {
                kind: "explicit-sql-column".to_string(),
                text: format!(
                    "{location}의 정적 {} 문에서 {}.{} 컬럼 식별자를 직접 읽었습니다.",
                    query.operation.as_str(),
                    table.name,
                    column.name
                ),
            }],
        );
    }
    usize::from(inserted)
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
                    lower[open + 1..literal_start].trim().is_empty()
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
    let Some(receiver) = execution_receiver(source, marker_start) else {
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
    )
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
        let quote = bytes[index];
        if !matches!(quote, b'\'' | b'"' | b'`') {
            index += 1;
            continue;
        }
        let literal_start = index;
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
                if !dynamic {
                    literals.push(StaticLiteral {
                        value,
                        start: literal_start,
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
        if bytes.get(index..index + 2) == Some(b"//") {
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

fn insert_confirmed_link(
    snapshot: &mut InventorySnapshot,
    from: &str,
    to: &str,
    kind: &str,
    label: &str,
    edge_type: &str,
    evidence: Vec<Evidence>,
) -> bool {
    if snapshot
        .links
        .iter()
        .any(|link| link.kind == kind && link.from == from && link.to == to)
    {
        return false;
    }
    snapshot.links.push(SnapshotLink {
        id: format!("{kind}:{from}->{to}"),
        from: from.to_string(),
        to: to.to_string(),
        kind: kind.to_string(),
        label: Some(label.to_string()),
        truth_class: "confirmed".to_string(),
        direction: "outbound".to_string(),
        engine_edge_type: Some(edge_type.to_string()),
        evidence,
    });
    true
}

fn qualified_table_name(table: &InventoryItem) -> String {
    table.group_id.as_deref().map_or_else(
        || table.name.clone(),
        |schema| format!("{schema}.{}", table.name),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_static_sql_from_selected_code_without_name_candidates() {
        let root =
            std::env::temp_dir().join(format!("backend-map-semantic-links-{}", std::process::id()));
        let source_dir = root.join("src");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(
            source_dir.join("repository.ts"),
            "function load() {\n  return db.query(\"SELECT id, status FROM public.orders\");\n}\n",
        )
        .unwrap();
        let mut snapshot = InventorySnapshot {
            schema_version: 2,
            workspace_id: "workspace".to_string(),
            saved_at: "1".to_string(),
            metadata: Default::default(),
            stale_reasons: Vec::new(),
            links: Vec::new(),
            items: vec![
                inventory_item(
                    "code:load",
                    "function",
                    "load",
                    "code",
                    None,
                    Some("src/repository.ts"),
                    None,
                ),
                inventory_item(
                    "db:table:public.orders",
                    "table",
                    "orders",
                    "db",
                    None,
                    None,
                    Some("public"),
                ),
                inventory_item(
                    "db:column:public.orders:id",
                    "column",
                    "id",
                    "db",
                    Some("db:table:public.orders"),
                    None,
                    Some("public"),
                ),
                inventory_item(
                    "db:column:public.orders:status",
                    "column",
                    "status",
                    "db",
                    Some("db:table:public.orders"),
                    None,
                    Some("public"),
                ),
            ],
        };
        snapshot.items[0].location = Some(super::super::model::SourceLocation {
            path: "src/repository.ts".to_string(),
            line: Some(1),
            column: None,
            end_line: Some(3),
            end_column: None,
        });

        let count = apply_explicit_query_evidence_for_code(
            &mut snapshot,
            root.to_str().unwrap(),
            &["code:load".to_string()],
        );

        assert_eq!(count, 1);
        let table_link = snapshot
            .links
            .iter()
            .find(|link| {
                link.from == "code:load"
                    && link.to == "db:table:public.orders"
                    && link.kind == "code_db_read"
                    && link.truth_class == "confirmed"
            })
            .unwrap();
        assert!(table_link.evidence[0].text.contains("repository.ts:L2"));
        assert_eq!(
            snapshot
                .links
                .iter()
                .filter(|link| link.kind == "code_db_uses_column")
                .count(),
            2
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn confirms_static_select_and_exact_columns() {
        let result = analyze_source(
            r#"const sql = "SELECT id, status FROM public.orders WHERE id = ?";
               return connection.query(sql, params);"#,
            "orders",
            Some("public"),
            &["id", "status", "created_at"],
            false,
        );

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].operation, QueryOperation::Select);
        assert_eq!(
            result[0].columns,
            BTreeSet::from(["id".to_string(), "status".to_string()])
        );
    }

    #[test]
    fn confirms_static_update_as_write() {
        let result = analyze_source(
            r#"jdbcTemplate.execute("UPDATE orders SET status = ? WHERE id = ?");"#,
            "orders",
            None,
            &["id", "status"],
            false,
        );

        assert_eq!(result[0].operation, QueryOperation::Update);
        assert_eq!(result[0].operation.edge_type(), "WRITES");
    }

    #[test]
    fn confirms_inline_generic_execution_call() {
        let result = analyze_source(
            r#"return connection.QueryAsync<Order>("SELECT id FROM orders");"#,
            "orders",
            None,
            &["id"],
            false,
        );

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].operation, QueryOperation::Select);
    }

    #[test]
    fn confirms_static_sql_across_common_framework_execution_apis() {
        for (source, operation) in [
            (
                r#"return connection.QuerySingleAsync<Order>("SELECT id, status FROM orders WHERE id = @id");"#,
                QueryOperation::Select,
            ),
            (
                r#"context.Database.ExecuteSqlRaw("UPDATE orders SET status = ? WHERE id = ?");"#,
                QueryOperation::Update,
            ),
            (
                r#"jdbcTemplate.queryForObject("SELECT id FROM orders WHERE id = ?", mapper, id);"#,
                QueryOperation::Select,
            ),
            (
                r#"session.execute(text("SELECT id FROM orders WHERE id = :id"), params)"#,
                QueryOperation::Select,
            ),
        ] {
            let result = analyze_source(
                source,
                "orders",
                None,
                &["id", "status", "created_at"],
                false,
            );
            assert_eq!(
                result.len(),
                1,
                "explicit static SQL should be confirmed: {source}"
            );
            assert_eq!(result[0].operation, operation);
        }

        for source in [
            r#"reporter.QuerySingle("SELECT id FROM orders")"#,
            r#"session.execute(text(prefix + "SELECT id FROM orders"))"#,
            r#"session.execute(text("SELECT id FROM orders" + suffix))"#,
            r#"session.execute(render("SELECT id FROM orders"))"#,
        ] {
            assert!(
                analyze_source(source, "orders", None, &["id"], false).is_empty(),
                "non-evidence execution form must stay unconfirmed: {source}"
            );
        }
    }

    #[test]
    fn rejects_dynamic_or_commented_sql() {
        assert!(analyze_source(
            r#"cursor.execute(f"SELECT id FROM {table_name}")"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
        for source in [
            r#"db.query("SELECT id FROM orders " + whereClause)"#,
            r#"const sql = "SELECT id FROM orders " + whereClause; db.query(sql);"#,
            "const sql = \"SELECT id FROM orders \"\n  + whereClause;\ndb.query(sql);",
            r#"db.query(prefix + "SELECT id FROM orders")"#,
            r#"db.query("SELECT ${column} FROM orders")"#,
            r##"db.query("SELECT #{column} FROM orders")"##,
            r#"db.query("SELECT $column FROM orders")"#,
            r#"const sql = "SELECT id FROM orders"; db.query(sql + whereClause)"#,
            r#"const sql = "SELECT id FROM orders"; db.query(prefix + sql)"#,
        ] {
            assert!(
                analyze_source(source, "orders", None, &["id"], false).is_empty(),
                "dynamic SQL must not become confirmed: {source}"
            );
        }
        assert!(analyze_source(
            r#"// connection.query("SELECT id FROM orders")"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
    }

    #[test]
    fn rejects_unrelated_sql_literal_near_an_execution_call() {
        assert!(analyze_source(
            r#"const help = "SELECT id FROM orders";
               logger.info(help);
               return connection.query(otherSql);"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
        assert!(analyze_source(
            r#"connection.query(otherSql);
               const help = "SELECT id FROM orders";"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
    }

    #[test]
    fn ignores_sql_string_values_that_match_column_names() {
        let result = analyze_source(
            r#"db.query("SELECT id FROM orders WHERE name = 'status'")"#,
            "orders",
            None,
            &["id", "status"],
            false,
        );

        assert_eq!(result[0].columns, BTreeSet::from(["id".to_string()]));
    }

    #[test]
    fn ignores_projection_aliases_that_match_real_columns() {
        let result = analyze_source(
            r#"db.query("SELECT count(*) AS id, status AS state, 'fixed' name FROM orders")"#,
            "orders",
            None,
            &["id", "status", "state", "name"],
            false,
        );

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].columns, BTreeSet::from(["status".to_string()]));
    }

    #[test]
    fn fails_closed_for_dialect_projection_clauses_outside_the_bounded_grammar() {
        for source in [
            r#"db.query("SELECT TOP (10) id FROM orders")"#,
            r#"db.query("SELECT TOP @limit id FROM orders")"#,
            r#"db.query("SELECT DISTINCT ON (tenant_id) status FROM orders")"#,
        ] {
            assert!(
                analyze_source(source, "orders", None, &["id", "status"], false).is_empty(),
                "unsupported projection syntax must stay unconfirmed: {source}"
            );
        }
    }

    #[test]
    fn keeps_top_as_a_column_outside_the_sql_server_projection_clause() {
        let selected = analyze_source(
            r#"db.query("SELECT top FROM orders")"#,
            "orders",
            None,
            &["top"],
            false,
        );
        let updated = analyze_source(
            r#"db.execute("UPDATE orders SET top = ? WHERE id = ?")"#,
            "orders",
            None,
            &["id", "top"],
            false,
        );

        assert_eq!(selected[0].columns, BTreeSet::from(["top".to_string()]));
        assert_eq!(
            updated[0].columns,
            BTreeSet::from(["id".to_string(), "top".to_string()])
        );
    }

    #[test]
    fn ignores_named_parameters_that_match_column_names() {
        let result = analyze_source(
            r#"db.query("SELECT id FROM orders WHERE name = :status AND role = @status")"#,
            "orders",
            None,
            &["id", "status"],
            false,
        );

        assert_eq!(result[0].columns, BTreeSet::from(["id".to_string()]));
    }

    #[test]
    fn rejects_generic_receivers_and_reassigned_query_variables() {
        assert!(analyze_source(
            r#"logger.raw("SELECT id FROM orders")"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
        assert!(analyze_source(
            r#"let sql = "SELECT id FROM orders";
               sql = buildSql();
               db.query(sql);"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
        assert!(analyze_source(
            r#"db.query(format("SELECT id FROM orders"))"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
        assert!(analyze_source(
            r#"const sql = "SELECT id FROM orders";
               db.query(transform(sql));"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
        assert!(analyze_source(
            r#"const sql = "SELECT id FROM orders";
               db.query("sql");"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
        assert!(analyze_source(
            r#"db.query({ text: "SELECT id FROM orders" })"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
    }

    #[test]
    fn assigns_qualified_join_columns_only_to_their_owner() {
        let source = r#"db.query("SELECT users.id, orders.status FROM orders JOIN users ON users.id = orders.user_id")"#;
        let orders = analyze_source(source, "orders", None, &["id", "status", "user_id"], false);
        let users = analyze_source(source, "users", None, &["id"], false);

        assert_eq!(
            orders[0].columns,
            BTreeSet::from(["status".to_string(), "user_id".to_string()])
        );
        assert_eq!(users[0].columns, BTreeSet::from(["id".to_string()]));

        let ambiguous =
            r#"db.query("SELECT id FROM orders JOIN users ON orders.user_id = users.owner_id")"#;
        assert!(
            !analyze_source(ambiguous, "orders", None, &["id"], false)[0]
                .columns
                .contains("id")
        );
    }

    #[test]
    fn accepts_qualified_table_when_duplicate_schemas_exist() {
        assert_eq!(
            analyze_source(
                r#"db.query("SELECT id FROM public.orders")"#,
                "orders",
                Some("public"),
                &["id"],
                true,
            )
            .len(),
            1
        );
        assert!(analyze_source(
            r#"db.query("SELECT id FROM orders")"#,
            "orders",
            Some("public"),
            &["id"],
            true,
        )
        .is_empty());
        assert_eq!(
            analyze_source(
                r#"db.query('SELECT id FROM "public"."orders"')"#,
                "orders",
                Some("public"),
                &["id"],
                true,
            )
            .len(),
            1
        );
    }

    #[test]
    fn separates_read_and_write_targets_in_composite_dml() {
        let source = r#"db.execute("INSERT INTO archived_orders (id) SELECT id FROM orders")"#;
        let target = analyze_source(source, "archived_orders", None, &["id"], false);
        let source_table = analyze_source(source, "orders", None, &["id"], false);

        assert_eq!(target[0].operation, QueryOperation::Insert);
        assert_eq!(source_table[0].operation, QueryOperation::Select);

        let merge = r#"db.execute("MERGE INTO orders AS o USING staged_orders AS s ON o.id = s.id WHEN MATCHED THEN UPDATE SET status = s.status")"#;
        assert_eq!(
            analyze_source(merge, "orders", None, &["id", "status"], false)[0].operation,
            QueryOperation::Merge
        );
        assert_eq!(
            analyze_source(merge, "staged_orders", None, &["id", "status"], false)[0].operation,
            QueryOperation::Select
        );
    }

    #[test]
    fn keeps_insert_column_lists_out_of_alias_detection() {
        let result = analyze_source(
            r#"db.execute("INSERT INTO orders (id, status) VALUES (?, ?)")"#,
            "orders",
            None,
            &["id", "status"],
            false,
        );

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].columns,
            BTreeSet::from(["id".to_string(), "status".to_string()])
        );
    }

    #[test]
    fn fails_closed_for_ctes_and_unresolved_join_column_owners() {
        assert!(analyze_source(
            r#"db.query("WITH recent AS (SELECT id FROM orders) SELECT id FROM recent")"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());

        let result = analyze_source(
            r#"db.query("SELECT status FROM orders JOIN audit_feed ON audit_feed.order_id = audit_feed.id")"#,
            "orders",
            None,
            &["status"],
            false,
        );
        assert_eq!(result.len(), 1);
        assert!(result[0].columns.is_empty());
    }

    #[test]
    fn fails_closed_for_multi_statement_comma_join_and_table_function_sql() {
        for source in [
            r#"db.query("SELECT id FROM orders; DELETE FROM audit")"#,
            r#"db.query("SELECT id FROM orders, users")"#,
            r#"db.query("SELECT id FROM orders(?)")"#,
        ] {
            assert!(analyze_source(source, "orders", None, &["id"], false).is_empty());
        }
    }

    #[test]
    fn ignores_sql_comments_and_does_not_treat_temp_tables_as_real_tables() {
        assert!(analyze_source(
            "db.query(\"SELECT 1 -- FROM orders\\n\")",
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
        assert!(analyze_source(
            r#"db.query("SELECT id FROM #orders")"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
        assert!(analyze_source(
            r#"db.query("SELECT id FROM orders # JOIN audit")"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
    }

    #[test]
    fn semantic_cache_signature_changes_with_the_source_file() {
        let root = std::env::temp_dir().join(format!(
            "backend-map-semantic-signature-{}",
            std::process::id()
        ));
        let source_dir = root.join("src");
        std::fs::create_dir_all(&source_dir).unwrap();
        let path = source_dir.join("repository.ts");
        std::fs::write(&path, "db.query('SELECT id FROM orders')").unwrap();
        let snapshot = InventorySnapshot {
            schema_version: 2,
            workspace_id: "workspace".to_string(),
            saved_at: "1".to_string(),
            metadata: Default::default(),
            stale_reasons: Vec::new(),
            links: Vec::new(),
            items: vec![inventory_item(
                "code:load",
                "function",
                "load",
                "code",
                None,
                Some("src/repository.ts"),
                None,
            )],
        };
        let first = semantic_source_signature(
            &snapshot,
            root.to_str().unwrap(),
            &["code:load".to_string()],
        );
        std::fs::write(&path, "db.query('SELECT id, status FROM orders')").unwrap();
        let second = semantic_source_signature(
            &snapshot,
            root.to_str().unwrap(),
            &["code:load".to_string()],
        );

        assert_ne!(first, second);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn inventory_item(
        id: &str,
        kind: &str,
        name: &str,
        source: &str,
        parent_id: Option<&str>,
        path: Option<&str>,
        group_id: Option<&str>,
    ) -> InventoryItem {
        InventoryItem {
            id: id.to_string(),
            kind: kind.to_string(),
            name: name.to_string(),
            layer: if source == "db" { "db" } else { "code" }.to_string(),
            source: source.to_string(),
            parent_id: parent_id.map(str::to_string),
            path: path.map(str::to_string),
            qualified_name: None,
            engine_label: None,
            project_id: None,
            group_id: group_id.map(str::to_string),
            location: None,
            is_primary_key: false,
            is_foreign_key: false,
            nullable: None,
        }
    }
}
