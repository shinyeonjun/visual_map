use crate::engine;
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use rusqlite::{params, Connection, OpenFlags};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::collections::{hash_map::Entry, BTreeMap, HashMap};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

use super::{InventoryItem, InventorySnapshot, SnapshotLink};
use crate::atlas::inventory_query::{
    search_group_fields, search_score_fields, InventorySearchHit, InventorySearchResult,
    SEARCH_RESULTS_PER_GROUP,
};

const STORE_SCHEMA: &str = "visual-map.snapshot-store.v1";
const CHUNK_SIZE: usize = 512;

pub(super) fn is_snapshot_database(path: &Path) -> bool {
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut header = [0u8; 16];
    file.read_exact(&mut header).is_ok() && &header == b"SQLite format 3\0"
}

pub(super) fn write_snapshot_database(
    path: &Path,
    snapshot: &InventorySnapshot,
) -> Result<(), String> {
    let mut connection = Connection::open(sqlite_database_path(path))
        .map_err(|error| format!("스냅샷 SQLite를 만들지 못했습니다: {error}"))?;
    connection
        .execute_batch(
            "PRAGMA journal_mode = OFF;
             PRAGMA synchronous = OFF;
             PRAGMA temp_store = MEMORY;
             CREATE TABLE store_info (key TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;
             CREATE TABLE chunks (
               kind TEXT NOT NULL,
               chunk_index INTEGER NOT NULL,
               item_count INTEGER NOT NULL,
               payload BLOB NOT NULL,
               PRIMARY KEY(kind, chunk_index)
             ) WITHOUT ROWID;
             CREATE TABLE item_index (
               ordinal INTEGER PRIMARY KEY,
               chunk_index INTEGER NOT NULL,
               chunk_offset INTEGER NOT NULL,
               id TEXT NOT NULL UNIQUE,
               kind TEXT NOT NULL,
               layer TEXT NOT NULL,
               source TEXT NOT NULL,
               name TEXT NOT NULL,
               qualified_name TEXT,
               path TEXT
             );
             CREATE INDEX item_index_name ON item_index(name);
             CREATE INDEX item_index_kind ON item_index(kind, source);
             CREATE INDEX item_index_path ON item_index(path);
             CREATE TABLE link_index (
               ordinal INTEGER PRIMARY KEY,
               chunk_index INTEGER NOT NULL,
               chunk_offset INTEGER NOT NULL,
               id TEXT NOT NULL UNIQUE,
               from_id TEXT NOT NULL,
               to_id TEXT NOT NULL,
               kind TEXT NOT NULL,
               truth_class TEXT NOT NULL
             );
             CREATE INDEX link_index_from ON link_index(from_id, kind);
             CREATE INDEX link_index_to ON link_index(to_id, kind);
             CREATE TABLE architecture_node_index (
               ordinal INTEGER PRIMARY KEY,
               chunk_index INTEGER NOT NULL,
               chunk_offset INTEGER NOT NULL,
               id TEXT NOT NULL UNIQUE,
               kind TEXT NOT NULL,
               name TEXT,
               path TEXT,
               parent_id TEXT
             );
             CREATE INDEX architecture_node_kind ON architecture_node_index(kind);
             CREATE INDEX architecture_node_parent ON architecture_node_index(parent_id);
             CREATE TABLE architecture_edge_index (
               ordinal INTEGER PRIMARY KEY,
               chunk_index INTEGER NOT NULL,
               chunk_offset INTEGER NOT NULL,
               id TEXT NOT NULL UNIQUE,
               kind TEXT NOT NULL,
               from_id TEXT NOT NULL,
               to_id TEXT NOT NULL
             );
             CREATE INDEX architecture_edge_kind ON architecture_edge_index(kind);
             CREATE INDEX architecture_edge_from ON architecture_edge_index(from_id, kind);
             CREATE INDEX architecture_edge_to ON architecture_edge_index(to_id, kind);",
        )
        .map_err(|error| format!("스냅샷 SQLite 스키마를 만들지 못했습니다: {error}"))?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("스냅샷 트랜잭션을 시작하지 못했습니다: {error}"))?;
    transaction
        .execute(
            "INSERT INTO store_info(key, value) VALUES ('schema', ?1), ('workspace_id', ?2)",
            params![STORE_SCHEMA, snapshot.workspace_id],
        )
        .map_err(|error| format!("스냅샷 식별자를 저장하지 못했습니다: {error}"))?;

    let mut header = snapshot.clone();
    header.items.clear();
    header.links.clear();
    let (architecture_nodes, architecture_edges) = take_architecture_arrays(&mut header);
    insert_payload(&transaction, "header", 0, 1, &header)?;
    insert_chunks(&transaction, "items", &snapshot.items)?;
    insert_chunks(&transaction, "links", &snapshot.links)?;
    if let Some(nodes) = architecture_nodes.as_deref() {
        insert_chunks(&transaction, "architecture_nodes", nodes)?;
    }
    if let Some(edges) = architecture_edges.as_deref() {
        insert_chunks(&transaction, "architecture_edges", edges)?;
    }
    insert_item_index(&transaction, &snapshot.items)?;
    insert_link_index(&transaction, &snapshot.links)?;
    insert_architecture_indexes(
        &transaction,
        architecture_nodes.as_deref().unwrap_or_default(),
        architecture_edges.as_deref().unwrap_or_default(),
    )?;
    transaction
        .commit()
        .map_err(|error| format!("스냅샷 트랜잭션을 완료하지 못했습니다: {error}"))?;
    connection
        .execute_batch("PRAGMA optimize;")
        .map_err(|error| format!("스냅샷 SQLite를 최적화하지 못했습니다: {error}"))?;
    drop(connection);
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("스냅샷 SQLite를 디스크에 반영하지 못했습니다: {error}"))
}

pub(super) fn read_snapshot_database(
    path: &Path,
    workspace_id: &str,
) -> Result<InventorySnapshot, String> {
    let connection = open_snapshot_database(path, workspace_id)?;

    let mut snapshot: InventorySnapshot = read_single_payload(&connection, "header")?;
    snapshot.items = read_chunks(&connection, "items")?;
    snapshot.links = read_chunks(&connection, "links")?;
    let nodes: Option<Vec<Value>> = read_optional_chunks(&connection, "architecture_nodes")?;
    let edges: Option<Vec<Value>> = read_optional_chunks(&connection, "architecture_edges")?;
    if let Some(architecture) = snapshot.metadata.architecture.as_mut() {
        let object = architecture
            .as_object_mut()
            .ok_or("스냅샷 architecture가 객체가 아닙니다")?;
        if let Some(nodes) = nodes {
            object.insert("nodes".to_string(), Value::Array(nodes));
        }
        if let Some(edges) = edges {
            object.insert("edges".to_string(), Value::Array(edges));
        }
    }
    Ok(snapshot)
}

pub(super) fn search_snapshot_database(
    path: &Path,
    workspace_id: &str,
    query: &str,
) -> Result<InventorySearchResult, String> {
    let query = query.trim().to_lowercase();
    if query.chars().count() < 2 {
        return Ok(InventorySearchResult {
            hits: Vec::new(),
            total: 0,
            counts: BTreeMap::new(),
            truncated: false,
        });
    }
    let connection = open_snapshot_database(path, workspace_id)?;
    let mut statement = connection
        .prepare(
            "SELECT chunk_index, chunk_offset, id, kind, layer, source, name, qualified_name, path
             FROM item_index ORDER BY ordinal",
        )
        .map_err(|error| format!("스냅샷 검색 인덱스를 준비하지 못했습니다: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok(IndexedSearchCandidate {
                chunk_index: row.get(0)?,
                chunk_offset: row.get(1)?,
                id: row.get(2)?,
                kind: row.get(3)?,
                layer: row.get(4)?,
                source: row.get(5)?,
                name: row.get(6)?,
                qualified_name: row.get(7)?,
                path: row.get(8)?,
                score: 0,
            })
        })
        .map_err(|error| format!("스냅샷 검색 인덱스를 읽지 못했습니다: {error}"))?;
    let mut counts = BTreeMap::<String, usize>::new();
    let mut ranked = BTreeMap::<String, Vec<IndexedSearchCandidate>>::new();
    for row in rows {
        let mut candidate =
            row.map_err(|error| format!("스냅샷 검색 행을 읽지 못했습니다: {error}"))?;
        if candidate.source == "code"
            && candidate
                .path
                .as_deref()
                .is_some_and(|path| path.trim().starts_with('<'))
        {
            continue;
        }
        let Some(group) = search_group_fields(&candidate.source, &candidate.kind, &candidate.layer)
        else {
            continue;
        };
        candidate.score = search_score_fields(
            &candidate.name,
            candidate.qualified_name.as_deref(),
            candidate.path.as_deref(),
            &candidate.id,
            &query,
        );
        if candidate.score == 0 {
            continue;
        }
        *counts.entry(group.to_string()).or_default() += 1;
        let hits = ranked.entry(group.to_string()).or_default();
        hits.push(candidate);
        hits.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.id.cmp(&right.id))
        });
        hits.truncate(SEARCH_RESULTS_PER_GROUP);
    }
    drop(statement);

    let total = counts.values().sum();
    let mut chunk_cache = HashMap::<usize, Vec<InventoryItem>>::new();
    let mut hits = Vec::new();
    for group in ["api", "code", "file", "table", "db-object", "column"] {
        for candidate in ranked.remove(group).unwrap_or_default() {
            if let Entry::Vacant(entry) = chunk_cache.entry(candidate.chunk_index) {
                entry.insert(read_item_chunk(&connection, candidate.chunk_index)?);
            }
            let item = chunk_cache
                .get(&candidate.chunk_index)
                .and_then(|items| items.get(candidate.chunk_offset))
                .cloned()
                .ok_or("스냅샷 검색 인덱스가 항목 청크 범위를 벗어났습니다")?;
            hits.push(InventorySearchHit {
                group: group.to_string(),
                item,
            });
        }
    }
    Ok(InventorySearchResult {
        truncated: total > hits.len(),
        hits,
        total,
        counts,
    })
}

#[derive(Debug)]
struct IndexedSearchCandidate {
    chunk_index: usize,
    chunk_offset: usize,
    id: String,
    kind: String,
    layer: String,
    source: String,
    name: String,
    qualified_name: Option<String>,
    path: Option<String>,
    score: u16,
}

fn open_snapshot_database(path: &Path, workspace_id: &str) -> Result<Connection, String> {
    let connection = Connection::open_with_flags(
        sqlite_database_path(path),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| format!("스냅샷 SQLite를 열지 못했습니다: {error}"))?;
    connection
        .pragma_update(None, "query_only", true)
        .map_err(|error| format!("스냅샷을 읽기 전용으로 열지 못했습니다: {error}"))?;
    let (schema, stored_workspace): (String, String) = connection
        .query_row(
            "SELECT
               (SELECT value FROM store_info WHERE key = 'schema'),
               (SELECT value FROM store_info WHERE key = 'workspace_id')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| format!("스냅샷 식별자를 읽지 못했습니다: {error}"))?;
    if schema != STORE_SCHEMA {
        return Err(format!("지원하지 않는 스냅샷 저장 형식입니다: {schema}"));
    }
    if stored_workspace != workspace_id {
        return Err("스냅샷 프로젝트 ID가 경로와 일치하지 않습니다".to_string());
    }
    Ok(connection)
}

fn read_item_chunk(
    connection: &Connection,
    chunk_index: usize,
) -> Result<Vec<InventoryItem>, String> {
    let payload = connection
        .query_row(
            "SELECT payload FROM chunks WHERE kind = 'items' AND chunk_index = ?1",
            [chunk_index as i64],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .map_err(|error| format!("스냅샷 검색 항목 청크를 읽지 못했습니다: {error}"))?;
    decode_json(&payload)
}

fn take_architecture_arrays(
    header: &mut InventorySnapshot,
) -> (Option<Vec<Value>>, Option<Vec<Value>>) {
    let Some(object) = header
        .metadata
        .architecture
        .as_mut()
        .and_then(Value::as_object_mut)
    else {
        return (None, None);
    };
    let nodes = object.get("nodes").and_then(Value::as_array).cloned();
    let edges = object.get("edges").and_then(Value::as_array).cloned();
    if nodes.is_some() {
        object.remove("nodes");
    }
    if edges.is_some() {
        object.remove("edges");
    }
    (nodes, edges)
}

fn insert_chunks<T: Serialize>(
    transaction: &rusqlite::Transaction<'_>,
    kind: &str,
    values: &[T],
) -> Result<(), String> {
    if values.is_empty() {
        return insert_payload(transaction, kind, 0, 0, values);
    }
    for (chunk_index, chunk) in values.chunks(CHUNK_SIZE).enumerate() {
        insert_payload(transaction, kind, chunk_index, chunk.len(), chunk)?;
    }
    Ok(())
}

fn insert_payload<T: Serialize + ?Sized>(
    transaction: &rusqlite::Transaction<'_>,
    kind: &str,
    chunk_index: usize,
    item_count: usize,
    value: &T,
) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO chunks(kind, chunk_index, item_count, payload) VALUES (?1, ?2, ?3, ?4)",
            params![
                kind,
                chunk_index as i64,
                item_count as i64,
                compressed_json(value)?
            ],
        )
        .map_err(|error| format!("스냅샷 청크를 저장하지 못했습니다: {error}"))?;
    Ok(())
}

fn insert_item_index(
    transaction: &rusqlite::Transaction<'_>,
    items: &[InventoryItem],
) -> Result<(), String> {
    let mut statement = transaction
        .prepare(
            "INSERT INTO item_index(
               ordinal, chunk_index, chunk_offset, id, kind, layer, source, name, qualified_name, path
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )
        .map_err(|error| format!("스냅샷 항목 인덱스를 준비하지 못했습니다: {error}"))?;
    for (ordinal, item) in items.iter().enumerate() {
        let id = redacted_text(&item.id);
        let kind = redacted_text(&item.kind);
        let layer = redacted_text(&item.layer);
        let source = redacted_text(&item.source);
        let name = redacted_text(&item.name);
        let qualified_name = item.qualified_name.as_deref().map(redacted_text);
        let path = item.path.as_deref().map(redacted_text);
        statement
            .execute(params![
                ordinal as i64,
                (ordinal / CHUNK_SIZE) as i64,
                (ordinal % CHUNK_SIZE) as i64,
                id,
                kind,
                layer,
                source,
                name,
                qualified_name,
                path,
            ])
            .map_err(|error| format!("스냅샷 항목 인덱스를 저장하지 못했습니다: {error}"))?;
    }
    Ok(())
}

fn insert_link_index(
    transaction: &rusqlite::Transaction<'_>,
    links: &[SnapshotLink],
) -> Result<(), String> {
    let mut statement = transaction
        .prepare(
            "INSERT INTO link_index(
               ordinal, chunk_index, chunk_offset, id, from_id, to_id, kind, truth_class
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .map_err(|error| format!("스냅샷 연결 인덱스를 준비하지 못했습니다: {error}"))?;
    for (ordinal, link) in links.iter().enumerate() {
        let id = redacted_text(&link.id);
        let from = redacted_text(&link.from);
        let to = redacted_text(&link.to);
        let kind = redacted_text(&link.kind);
        let truth_class = redacted_text(&link.truth_class);
        statement
            .execute(params![
                ordinal as i64,
                (ordinal / CHUNK_SIZE) as i64,
                (ordinal % CHUNK_SIZE) as i64,
                id,
                from,
                to,
                kind,
                truth_class,
            ])
            .map_err(|error| format!("스냅샷 연결 인덱스를 저장하지 못했습니다: {error}"))?;
    }
    Ok(())
}

fn insert_architecture_indexes(
    transaction: &rusqlite::Transaction<'_>,
    nodes: &[Value],
    edges: &[Value],
) -> Result<(), String> {
    let mut node_statement = transaction
        .prepare(
            "INSERT INTO architecture_node_index(
               ordinal, chunk_index, chunk_offset, id, kind, name, path, parent_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .map_err(|error| format!("architecture 노드 인덱스를 준비하지 못했습니다: {error}"))?;
    for (ordinal, node) in nodes.iter().enumerate() {
        let id = redacted_optional(value_string(node, "id")).unwrap_or_default();
        let kind =
            redacted_optional(value_string(node, "kind")).unwrap_or_else(|| "RESOURCE".to_string());
        let name = redacted_optional(value_string(node, "name"));
        let path = redacted_optional(value_string(node, "path"));
        let parent_id = redacted_optional(value_string(node, "parent_id"));
        node_statement
            .execute(params![
                ordinal as i64,
                (ordinal / CHUNK_SIZE) as i64,
                (ordinal % CHUNK_SIZE) as i64,
                id,
                kind,
                name,
                path,
                parent_id,
            ])
            .map_err(|error| format!("architecture 노드 인덱스를 저장하지 못했습니다: {error}"))?;
    }
    drop(node_statement);

    let mut edge_statement = transaction
        .prepare(
            "INSERT INTO architecture_edge_index(
               ordinal, chunk_index, chunk_offset, id, kind, from_id, to_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .map_err(|error| format!("architecture 연결 인덱스를 준비하지 못했습니다: {error}"))?;
    for (ordinal, edge) in edges.iter().enumerate() {
        let id = redacted_optional(value_string(edge, "id")).unwrap_or_default();
        let kind =
            redacted_optional(value_string(edge, "kind")).unwrap_or_else(|| "UNKNOWN".to_string());
        let from = redacted_optional(value_string(edge, "from")).unwrap_or_default();
        let to = redacted_optional(value_string(edge, "to")).unwrap_or_default();
        edge_statement
            .execute(params![
                ordinal as i64,
                (ordinal / CHUNK_SIZE) as i64,
                (ordinal % CHUNK_SIZE) as i64,
                id,
                kind,
                from,
                to,
            ])
            .map_err(|error| format!("architecture 연결 인덱스를 저장하지 못했습니다: {error}"))?;
    }
    Ok(())
}

fn read_single_payload<T: DeserializeOwned>(
    connection: &Connection,
    kind: &str,
) -> Result<T, String> {
    let payload = connection
        .query_row(
            "SELECT payload FROM chunks WHERE kind = ?1 AND chunk_index = 0",
            [kind],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .map_err(|error| format!("스냅샷 '{kind}' 청크를 읽지 못했습니다: {error}"))?;
    decode_json(&payload)
}

fn read_chunks<T: DeserializeOwned>(connection: &Connection, kind: &str) -> Result<Vec<T>, String> {
    let mut statement = connection
        .prepare("SELECT payload FROM chunks WHERE kind = ?1 ORDER BY chunk_index")
        .map_err(|error| format!("스냅샷 '{kind}' 질의를 준비하지 못했습니다: {error}"))?;
    let rows = statement
        .query_map([kind], |row| row.get::<_, Vec<u8>>(0))
        .map_err(|error| format!("스냅샷 '{kind}' 청크를 질의하지 못했습니다: {error}"))?;
    let mut values = Vec::new();
    for row in rows {
        let payload = row.map_err(|error| format!("스냅샷 청크를 읽지 못했습니다: {error}"))?;
        values.extend(decode_json::<Vec<T>>(&payload)?);
    }
    Ok(values)
}

fn read_optional_chunks<T: DeserializeOwned>(
    connection: &Connection,
    kind: &str,
) -> Result<Option<Vec<T>>, String> {
    let exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM chunks WHERE kind = ?1)",
            [kind],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| format!("스냅샷 '{kind}' 존재 여부를 읽지 못했습니다: {error}"))?;
    exists.then(|| read_chunks(connection, kind)).transpose()
}

fn compressed_json<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, String> {
    let json = serde_json::to_string(value)
        .map_err(|error| format!("스냅샷 청크를 직렬화하지 못했습니다: {error}"))?;
    let redacted = engine::redact_secrets(&json);
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder
        .write_all(redacted.as_bytes())
        .map_err(|error| format!("스냅샷 청크를 압축하지 못했습니다: {error}"))?;
    encoder
        .finish()
        .map_err(|error| format!("스냅샷 청크 압축을 끝내지 못했습니다: {error}"))
}

fn decode_json<T: DeserializeOwned>(payload: &[u8]) -> Result<T, String> {
    let mut decoder = GzDecoder::new(payload);
    let mut json = Vec::new();
    decoder
        .read_to_end(&mut json)
        .map_err(|error| format!("스냅샷 청크 압축을 풀지 못했습니다: {error}"))?;
    serde_json::from_slice(&json)
        .map_err(|error| format!("스냅샷 청크 JSON이 올바르지 않습니다: {error}"))
}

fn value_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn redacted_text(value: &str) -> String {
    engine::redact_secrets(value)
}

fn redacted_optional(value: Option<&str>) -> Option<String> {
    value.map(redacted_text)
}

fn sqlite_database_path(path: &Path) -> std::path::PathBuf {
    if path.is_file() {
        return fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    }
    path.parent()
        .and_then(|parent| fs::canonicalize(parent).ok())
        .and_then(|parent| path.file_name().map(|name| parent.join(name)))
        .unwrap_or_else(|| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_database_round_trip_uses_multiple_chunks() {
        let root =
            std::env::temp_dir().join(format!("visual-map-snapshot-sqlite-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = root.join("snapshot.sqlite");
        let mut snapshot = crate::atlas::fixture_inventory("workspace-1".to_string());
        while snapshot.items.len() <= CHUNK_SIZE {
            let mut item = snapshot.items[0].clone();
            item.id = format!("{}:{}", item.id, snapshot.items.len());
            snapshot.items.push(item);
        }

        write_snapshot_database(&path, &snapshot).unwrap();
        let restored = read_snapshot_database(&path, "workspace-1").unwrap();
        let indexed_search = search_snapshot_database(&path, "workspace-1", "order").unwrap();
        let memory_search = crate::atlas::search_inventory(&snapshot, "order");

        assert!(is_snapshot_database(&path));
        assert_eq!(restored, snapshot);
        assert_eq!(indexed_search, memory_search);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn snapshot_database_supports_windows_long_paths() {
        let mut root = std::env::temp_dir().join(format!(
            "visual-map-snapshot-long-path-{}",
            std::process::id()
        ));
        while root.as_os_str().len() < 280 {
            root.push("workspace-segment-0123456789abcdef");
        }
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = root.join("snapshot.sqlite");
        let snapshot = crate::atlas::fixture_inventory("workspace-1".to_string());

        write_snapshot_database(&path, &snapshot).unwrap();
        assert_eq!(
            read_snapshot_database(&path, "workspace-1").unwrap(),
            snapshot
        );
        fs::remove_dir_all(root).unwrap();
    }
}
