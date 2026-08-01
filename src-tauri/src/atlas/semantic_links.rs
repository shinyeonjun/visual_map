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
mod tests {
    use super::sql_parser::QueryOperation;
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
                    && link.is_confirmed()
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
    fn ignores_static_sql_when_snapshot_has_no_matching_table() {
        let root = std::env::temp_dir().join(format!(
            "backend-map-semantic-empty-table-{}",
            std::process::id()
        ));
        let source_dir = root.join("src");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(
            source_dir.join("repository.ts"),
            "function load() { return db.query(\"SELECT id FROM missing_orders\"); }\n",
        )
        .unwrap();

        let mut snapshot = InventorySnapshot {
            schema_version: 2,
            workspace_id: "workspace-empty-table".to_string(),
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
        snapshot.items[0].location = Some(super::super::model::SourceLocation {
            path: "src/repository.ts".to_string(),
            line: Some(1),
            column: None,
            end_line: Some(1),
            end_column: None,
        });

        let count = apply_explicit_query_evidence_for_code(
            &mut snapshot,
            root.to_str().unwrap(),
            &["code:load".to_string()],
        );

        assert_eq!(count, 0);
        assert!(snapshot.links.is_empty());
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
                r#"return connection.QuerySingleAsync<Order>(@"SELECT id, status FROM orders WHERE id = @id");"#,
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
            (
                r##"sqlx::query!(r#"SELECT "id", status FROM orders WHERE status = 'ready'"#);"##,
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
            r#"const query = sql`SELECT id FROM orders`; db.query(query);"#,
        ] {
            assert!(
                analyze_source(source, "orders", None, &["id"], false).is_empty(),
                "non-evidence execution form must stay unconfirmed: {source}"
            );
        }
    }

    #[test]
    fn confirms_static_sql_for_every_active_language_shape() {
        let cases = [
            ("typescript", r#"db.query("SELECT id FROM orders")"#),
            ("javascript", r#"db.query("SELECT id FROM orders")"#),
            (
                "python",
                r#"session.execute(text("SELECT id FROM orders"))"#,
            ),
            (
                "java",
                r#"jdbcTemplate.queryForObject("SELECT id FROM orders", mapper);"#,
            ),
            (
                "csharp",
                r#"connection.QuerySingleAsync<Order>("SELECT id FROM orders");"#,
            ),
            (
                "c",
                r#"sqlite3_exec(db, "SELECT id FROM orders", callback, 0, error);"#,
            ),
            (
                "cpp",
                r#"sqlite3_exec(db, "SELECT id FROM orders", callback, 0, error);"#,
            ),
            ("go", r#"db.Query("SELECT id FROM orders")"#),
            ("rust", r##"sqlx::query!(r#"SELECT id FROM orders"#);"##),
            ("php", r#"$pdo->query("SELECT id FROM orders");"#),
            ("ruby", r#"connection.query("SELECT id FROM orders")"#),
            ("dart", r#"db.query("SELECT id FROM orders");"#),
        ];

        let root = std::env::temp_dir().join(format!(
            "backend-map-language-db-shapes-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src")).unwrap();

        for (language, source) in cases {
            let result = analyze_source(source, "orders", None, &["id"], false);
            assert_eq!(
                result.len(),
                1,
                "static SQL should be confirmed for {language}: {source}"
            );
            assert_eq!(result[0].operation, QueryOperation::Select);
            assert_eq!(result[0].columns, BTreeSet::from(["id".to_string()]));

            let path = format!("src/{language}.source");
            std::fs::write(root.join(&path), source).unwrap();
            let code_id = format!("code:{language}:load");
            let table_id = format!("db:table:{language}:orders");
            let column_id = format!("db:column:{language}:orders:id");
            let mut snapshot = InventorySnapshot {
                schema_version: 2,
                workspace_id: format!("workspace-{language}"),
                saved_at: "1".to_string(),
                metadata: Default::default(),
                stale_reasons: Vec::new(),
                links: Vec::new(),
                items: vec![
                    inventory_item(
                        &code_id,
                        "function",
                        "load",
                        "code",
                        None,
                        Some(&path),
                        None,
                    ),
                    inventory_item(&table_id, "table", "orders", "db", None, None, None),
                    inventory_item(
                        &column_id,
                        "column",
                        "id",
                        "db",
                        Some(&table_id),
                        None,
                        None,
                    ),
                ],
            };
            snapshot.items[0].location = Some(super::super::model::SourceLocation {
                path: path.clone(),
                line: Some(1),
                column: None,
                end_line: Some(1),
                end_column: None,
            });
            let count = apply_explicit_query_evidence_for_code(
                &mut snapshot,
                root.to_str().unwrap(),
                std::slice::from_ref(&code_id),
            );
            assert_eq!(
                count, 1,
                "exact table join should be confirmed for {language}"
            );
            assert!(snapshot.links.iter().any(|link| {
                link.from == code_id
                    && link.to == table_id
                    && link.kind == "code_db_read"
                    && link.is_confirmed()
            }));
            assert!(snapshot.links.iter().any(|link| {
                link.from == code_id
                    && link.to == column_id
                    && link.kind == "code_db_uses_column"
                    && link.is_confirmed()
            }));
        }
        std::fs::remove_dir_all(root).unwrap();
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
            r#"logger->query("SELECT id FROM orders")"#,
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
