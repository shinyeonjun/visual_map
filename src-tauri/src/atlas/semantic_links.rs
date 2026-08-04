use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    sync::{Arc, Mutex, OnceLock},
};

use super::{
    linker::{record_code_search_gap, AppliedCodeMatch},
    model::{Evidence, InventoryItem, InventorySnapshot, SnapshotLink},
};

mod sql_parser;

use sql_parser::{analyze_source, matched_columns, parse_source, resolve_table, QueryEvidence};

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
        .find(|item| item.id == table_id && item.is_db() && item.kind == "table")
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
        .filter(|item| item.is_code() && selected_ids.contains(item.id.as_str()))
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
        .filter(|item| item.is_db() && item.kind == "table")
        .cloned()
        .collect::<Vec<_>>();
    let tables_by_name = tables.iter().fold(
        HashMap::<String, Vec<&InventoryItem>>::new(),
        |mut grouped, table| {
            grouped
                .entry(table.name.to_ascii_lowercase())
                .or_default()
                .push(table);
            grouped
        },
    );
    let columns_by_table = snapshot
        .items
        .iter()
        .filter(|item| item.is_db() && item.kind == "column")
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
                    let resolved = resolve_table(&access.token, &tables_by_name);
                    match resolved.as_slice() {
                        [table] => Some((access, *table)),
                        _ => None,
                    }
                })
                .collect::<Vec<_>>();
            for (access, table) in &resolved_accesses {
                let columns = columns_by_table
                    .get(table.id.as_str())
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                let evidence = QueryEvidence {
                    operation: access.operation,
                    columns: matched_columns(
                        &query,
                        access,
                        table,
                        columns,
                        &resolved_accesses,
                        &columns_by_table,
                    ),
                    line_offset: query.line_offset,
                };
                confirmed += apply_query_links(
                    snapshot,
                    matched,
                    table,
                    columns,
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
    let items = snapshot
        .items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<HashMap<_, _>>();
    code_ids
        .iter()
        .map(|code_id| {
            let Some(item) = items.get(code_id.as_str()) else {
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
include!("semantic_links_tests.rs");
