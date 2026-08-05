use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const RECEIPT_SCHEMA: &str = "code-memory.generation-receipt.v1";
const CHUNK_SIZE: usize = 512;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GenerationCounts {
    pub(crate) nodes: usize,
    pub(crate) calls: usize,
    pub(crate) handles: usize,
    pub(crate) architecture_nodes: usize,
    pub(crate) architecture_edges: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GenerationReceipt {
    pub(crate) schema: String,
    pub(crate) generation_id: String,
    pub(crate) status: String,
    pub(crate) project: String,
    pub(crate) repo_path: String,
    pub(crate) database_path: String,
    pub(crate) created_at_unix_ms: u128,
    pub(crate) counts: GenerationCounts,
}

pub(crate) struct GenerationStore {
    connection: Connection,
    pub(crate) receipt: GenerationReceipt,
}

pub(crate) fn new_generation_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("g-{nanos:032x}-{}", std::process::id())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn publish_generation(
    project_dir: &Path,
    staging_dir: &Path,
    generation_id: &str,
    project: &str,
    repo_path: &str,
    inventory: &Value,
    calls: &Value,
    handles: &Value,
    architecture: &Value,
    evidence: &Value,
) -> Result<GenerationReceipt, String> {
    let generations = project_dir.join("generations");
    fs::create_dir_all(&generations)
        .map_err(|error| format!("cannot create generation directory: {error}"))?;
    let final_dir = generations.join(generation_id);
    let final_database = final_dir.join("code-graph.sqlite");
    let staging_database = staging_dir.join("code-graph.sqlite");
    write_database(
        &staging_database,
        repo_path,
        inventory,
        calls,
        handles,
        architecture,
        evidence,
    )?;

    let receipt = GenerationReceipt {
        schema: RECEIPT_SCHEMA.to_string(),
        generation_id: generation_id.to_string(),
        status: "complete".to_string(),
        project: project.to_string(),
        repo_path: repo_path.to_string(),
        database_path: final_database.display().to_string(),
        created_at_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        counts: GenerationCounts {
            nodes: result_rows(inventory).len(),
            calls: result_rows(calls).len(),
            handles: result_rows(handles).len(),
            architecture_nodes: architecture
                .get("nodes")
                .and_then(Value::as_array)
                .map_or(0, Vec::len),
            architecture_edges: architecture
                .get("edges")
                .and_then(Value::as_array)
                .map_or(0, Vec::len),
        },
    };
    write_json_file(&staging_dir.join("receipt.json"), &receipt)?;
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&staging_database)
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("cannot sync generation database: {error}"))?;
    fs::rename(staging_dir, &final_dir)
        .map_err(|error| format!("cannot publish generation directory: {error}"))?;
    if let Err(error) = publish_current(project_dir, &receipt) {
        let _ = fs::remove_dir_all(&final_dir);
        return Err(error);
    }
    prune_generations(project_dir);
    Ok(receipt)
}

pub(crate) fn open_current(project_dir: &Path) -> Result<Option<GenerationStore>, String> {
    let current = project_dir.join("current.json");
    if !current.is_file() {
        return Ok(None);
    }
    let receipt: GenerationReceipt = serde_json::from_slice(
        &fs::read(&current).map_err(|error| format!("cannot read generation receipt: {error}"))?,
    )
    .map_err(|error| format!("invalid generation receipt: {error}"))?;
    if receipt.schema != RECEIPT_SCHEMA || receipt.status != "complete" {
        return Err("current code generation is not complete".to_string());
    }
    let database = project_dir
        .join("generations")
        .join(&receipt.generation_id)
        .join("code-graph.sqlite");
    let connection = Connection::open_with_flags(
        sqlite_database_path(&database),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| {
        format!(
            "cannot open code generation {}: {error}",
            database.display()
        )
    })?;
    connection
        .pragma_update(None, "query_only", true)
        .map_err(|error| format!("cannot protect code generation: {error}"))?;
    Ok(Some(GenerationStore {
        connection,
        receipt,
    }))
}

impl GenerationStore {
    pub(crate) fn inventory(&self) -> Result<Value, String> {
        let rows = if self.is_chunked()? {
            self.chunk_rows("inventory")?
        } else {
            self.json_rows("SELECT row_json FROM inventory_nodes ORDER BY ordinal", [])?
        };
        self.rows_result("inventory_columns", rows)
    }

    pub(crate) fn relationships(&self, kind: &str) -> Result<Value, String> {
        let metadata = match kind {
            "CALLS" => "calls_columns",
            "HANDLES" => "handles_columns",
            _ => return self.architecture_relationships(kind),
        };
        let rows = if self.is_chunked()? {
            self.chunk_rows(&kind.to_ascii_lowercase())?
        } else {
            self.json_rows(
                "SELECT row_json FROM relationships WHERE kind = ?1 ORDER BY ordinal",
                [kind],
            )?
        };
        self.rows_result(metadata, rows)
    }

    pub(crate) fn architecture(&self) -> Result<Value, String> {
        let mut architecture = self.metadata_value("architecture_header")?;
        let object = architecture
            .as_object_mut()
            .ok_or("architecture header is not an object")?;
        let (nodes, edges) = if self.is_chunked()? {
            (
                self.chunk_rows("architecture_nodes")?,
                self.chunk_rows("architecture_edges")?,
            )
        } else {
            (
                self.json_rows(
                    "SELECT node_json FROM architecture_nodes ORDER BY ordinal",
                    [],
                )?,
                self.json_rows(
                    "SELECT edge_json FROM architecture_edges ORDER BY ordinal",
                    [],
                )?,
            )
        };
        object.insert("nodes".to_string(), Value::Array(nodes));
        object.insert("edges".to_string(), Value::Array(edges));
        Ok(architecture)
    }

    pub(crate) fn evidence(&self) -> Result<Value, String> {
        self.metadata_value("evidence")
    }

    pub(crate) fn project_root(&self) -> Result<String, String> {
        self.metadata_value("project_root")?
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| "generation has no project root".to_string())
    }

    pub(crate) fn document_symbol_names(&self) -> Result<HashMap<String, Vec<String>>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT file_path, qualified_name FROM inventory_nodes \
                 WHERE file_path IS NOT NULL AND qualified_name IS NOT NULL ORDER BY ordinal",
            )
            .map_err(|error| format!("cannot prepare symbol query: {error}"))?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| format!("cannot query symbols: {error}"))?;
        let mut result = HashMap::<String, Vec<String>>::new();
        for row in rows {
            let (path, name) = row.map_err(|error| format!("cannot read symbol row: {error}"))?;
            result.entry(path).or_default().push(name);
        }
        Ok(result)
    }

    fn architecture_relationships(&self, kind: &str) -> Result<Value, String> {
        let rows = if self.is_chunked()? {
            self.chunk_rows("architecture_edges")?
                .into_iter()
                .filter(|edge| edge.get("kind").and_then(Value::as_str) == Some(kind))
                .collect()
        } else {
            self.json_rows(
                "SELECT edge_json FROM architecture_edges WHERE kind = ?1 ORDER BY ordinal",
                [kind],
            )?
        };
        let rows = rows
            .into_iter()
            .map(|edge| {
                json!([
                    edge.get("from").cloned().unwrap_or(Value::Null),
                    edge.get("to").cloned().unwrap_or(Value::Null),
                    edge.get("kind").cloned().unwrap_or(Value::Null),
                    edge.get("level").cloned().unwrap_or(Value::Null),
                    edge.get("properties").cloned().unwrap_or(Value::Null),
                    edge.get("evidence").cloned().unwrap_or(Value::Null),
                ])
            })
            .collect::<Vec<_>>();
        let total = rows.len();
        Ok(json!({
            "columns": ["source", "target", "kind", "level", "properties", "evidence"],
            "rows": rows,
            "total": total
        }))
    }

    fn rows_result(&self, columns_key: &str, rows: Vec<Value>) -> Result<Value, String> {
        let columns = self.metadata_value(columns_key)?;
        let total = rows.len();
        Ok(json!({"columns": columns, "rows": rows, "total": total}))
    }

    fn is_chunked(&self) -> Result<bool, String> {
        Ok(self.metadata_value("schema")?.as_str() == Some("code-memory.graph-store.v3"))
    }

    fn chunk_rows(&self, kind: &str) -> Result<Vec<Value>, String> {
        let mut statement = self
            .connection
            .prepare("SELECT payload FROM chunks WHERE kind = ?1 ORDER BY chunk_index")
            .map_err(|error| format!("cannot prepare generation chunks: {error}"))?;
        let rows = statement
            .query_map([kind], |row| Ok(row.get_ref(0)?.as_bytes()?.to_vec()))
            .map_err(|error| format!("cannot query generation chunks: {error}"))?;
        let mut values = Vec::new();
        for row in rows {
            let bytes = row.map_err(|error| format!("cannot read generation chunk: {error}"))?;
            let value = decode_json_blob(&bytes)?;
            let chunk = value.as_array().ok_or("generation chunk is not an array")?;
            values.extend(chunk.iter().cloned());
        }
        Ok(values)
    }

    fn json_rows<const N: usize>(
        &self,
        sql: &str,
        params: [&str; N],
    ) -> Result<Vec<Value>, String> {
        let mut statement = self
            .connection
            .prepare(sql)
            .map_err(|error| format!("cannot prepare generation query: {error}"))?;
        let rows = statement
            .query_map(rusqlite::params_from_iter(params), |row| {
                Ok(row.get_ref(0)?.as_bytes()?.to_vec())
            })
            .map_err(|error| format!("cannot query generation: {error}"))?;
        rows.map(|row| {
            let bytes = row.map_err(|error| format!("cannot read generation row: {error}"))?;
            decode_json_blob(&bytes)
        })
        .collect()
    }

    fn metadata_value(&self, key: &str) -> Result<Value, String> {
        let value = self
            .connection
            .query_row(
                "SELECT value_json FROM metadata WHERE key = ?1",
                [key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("cannot read generation metadata: {error}"))?
            .ok_or_else(|| format!("generation metadata '{key}' is missing"))?;
        serde_json::from_str(&value)
            .map_err(|error| format!("invalid generation metadata '{key}': {error}"))
    }
}

fn write_database(
    path: &Path,
    repo_path: &str,
    inventory: &Value,
    calls: &Value,
    handles: &Value,
    architecture: &Value,
    evidence: &Value,
) -> Result<(), String> {
    let mut connection = Connection::open(sqlite_database_path(path))
        .map_err(|error| format!("cannot create code generation database: {error}"))?;
    connection
        .execute_batch(
            "PRAGMA journal_mode = OFF;
             PRAGMA synchronous = OFF;
             PRAGMA temp_store = MEMORY;
             CREATE TABLE metadata (key TEXT PRIMARY KEY, value_json TEXT NOT NULL) WITHOUT ROWID;
             CREATE TABLE chunks (
               kind TEXT NOT NULL,
               chunk_index INTEGER NOT NULL,
               item_count INTEGER NOT NULL,
               payload BLOB NOT NULL,
               PRIMARY KEY(kind, chunk_index)
             ) WITHOUT ROWID;
             CREATE TABLE inventory_nodes (
               ordinal INTEGER PRIMARY KEY,
               qualified_name TEXT,
               file_path TEXT
             );
             CREATE INDEX inventory_nodes_file_path ON inventory_nodes(file_path);
             CREATE TABLE relationships (
               kind TEXT NOT NULL,
               ordinal INTEGER NOT NULL,
               source TEXT,
               target TEXT,
               PRIMARY KEY(kind, ordinal)
             ) WITHOUT ROWID;
             CREATE INDEX relationships_source ON relationships(source, kind);
             CREATE INDEX relationships_target ON relationships(target, kind);
             CREATE TABLE architecture_nodes (
               ordinal INTEGER PRIMARY KEY,
               id TEXT NOT NULL UNIQUE,
               kind TEXT NOT NULL,
               name TEXT,
               path TEXT,
               parent_id TEXT
             );
             CREATE INDEX architecture_nodes_kind ON architecture_nodes(kind);
             CREATE INDEX architecture_nodes_parent ON architecture_nodes(parent_id);
             CREATE TABLE architecture_edges (
               ordinal INTEGER PRIMARY KEY,
               id TEXT NOT NULL UNIQUE,
               kind TEXT NOT NULL,
               source TEXT NOT NULL,
               target TEXT NOT NULL
             );
             CREATE INDEX architecture_edges_kind ON architecture_edges(kind);
             CREATE INDEX architecture_edges_source ON architecture_edges(source, kind);
             CREATE INDEX architecture_edges_target ON architecture_edges(target, kind);",
        )
        .map_err(|error| format!("cannot initialize code generation database: {error}"))?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("cannot begin code generation transaction: {error}"))?;
    insert_metadata(&transaction, "schema", &json!("code-memory.graph-store.v3"))?;
    insert_metadata(&transaction, "project_root", &json!(repo_path))?;
    insert_metadata(
        &transaction,
        "inventory_columns",
        inventory.get("columns").unwrap_or(&Value::Null),
    )?;
    insert_metadata(
        &transaction,
        "calls_columns",
        calls.get("columns").unwrap_or(&Value::Null),
    )?;
    insert_metadata(
        &transaction,
        "handles_columns",
        handles.get("columns").unwrap_or(&Value::Null),
    )?;
    insert_metadata(&transaction, "evidence", evidence)?;
    let mut architecture_header = architecture.clone();
    if let Some(object) = architecture_header.as_object_mut() {
        object.remove("nodes");
        object.remove("edges");
    }
    insert_metadata(&transaction, "architecture_header", &architecture_header)?;
    let inventory_rows = result_rows(inventory);
    let call_rows = result_rows(calls);
    let handle_rows = result_rows(handles);
    insert_inventory_nodes(&transaction, inventory_rows)?;
    insert_relationships(&transaction, "CALLS", call_rows)?;
    insert_relationships(&transaction, "HANDLES", handle_rows)?;
    insert_chunks(&transaction, "inventory", inventory_rows)?;
    insert_chunks(&transaction, "calls", call_rows)?;
    insert_chunks(&transaction, "handles", handle_rows)?;
    insert_architecture(&transaction, architecture)?;
    transaction
        .commit()
        .map_err(|error| format!("cannot commit code generation: {error}"))?;
    connection
        .execute_batch("PRAGMA optimize;")
        .map_err(|error| format!("cannot optimize code generation: {error}"))?;
    Ok(())
}

fn insert_metadata(transaction: &Transaction<'_>, key: &str, value: &Value) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO metadata(key, value_json) VALUES (?1, ?2)",
            params![key, json_string(value)?],
        )
        .map_err(|error| format!("cannot store generation metadata: {error}"))?;
    Ok(())
}

fn insert_inventory_nodes(transaction: &Transaction<'_>, rows: &[Value]) -> Result<(), String> {
    let mut statement = transaction
        .prepare(
            "INSERT INTO inventory_nodes(ordinal, qualified_name, file_path)
             VALUES (?1, ?2, ?3)",
        )
        .map_err(|error| format!("cannot prepare inventory insert: {error}"))?;
    for (ordinal, row) in rows.iter().enumerate() {
        let fields = row.as_array().ok_or("inventory row is not an array")?;
        statement
            .execute(params![
                ordinal as i64,
                string_field(fields, 2),
                string_field(fields, 3),
            ])
            .map_err(|error| format!("cannot store inventory node: {error}"))?;
    }
    Ok(())
}

fn insert_relationships(
    transaction: &Transaction<'_>,
    kind: &str,
    rows: &[Value],
) -> Result<(), String> {
    let mut statement = transaction
        .prepare(
            "INSERT INTO relationships(kind, ordinal, source, target)
             VALUES (?1, ?2, ?3, ?4)",
        )
        .map_err(|error| format!("cannot prepare relationship insert: {error}"))?;
    for (ordinal, row) in rows.iter().enumerate() {
        let fields = row.as_array().ok_or("relationship row is not an array")?;
        statement
            .execute(params![
                kind,
                ordinal as i64,
                string_field(fields, 0),
                string_field(fields, 1),
            ])
            .map_err(|error| format!("cannot store relationship: {error}"))?;
    }
    Ok(())
}

fn insert_architecture(transaction: &Transaction<'_>, architecture: &Value) -> Result<(), String> {
    let mut node_statement = transaction
        .prepare(
            "INSERT INTO architecture_nodes(ordinal, id, kind, name, path, parent_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .map_err(|error| format!("cannot prepare architecture node insert: {error}"))?;
    for (ordinal, node) in architecture
        .get("nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        node_statement
            .execute(params![
                ordinal as i64,
                value_string(node, "id").unwrap_or_default(),
                value_string(node, "kind").unwrap_or("RESOURCE"),
                value_string(node, "name"),
                value_string(node, "path"),
                value_string(node, "parent_id"),
            ])
            .map_err(|error| format!("cannot store architecture node: {error}"))?;
    }
    drop(node_statement);

    let mut edge_statement = transaction
        .prepare(
            "INSERT INTO architecture_edges(ordinal, id, kind, source, target)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .map_err(|error| format!("cannot prepare architecture edge insert: {error}"))?;
    for (ordinal, edge) in architecture
        .get("edges")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        edge_statement
            .execute(params![
                ordinal as i64,
                value_string(edge, "id").unwrap_or_default(),
                value_string(edge, "kind").unwrap_or("UNKNOWN"),
                value_string(edge, "from").unwrap_or_default(),
                value_string(edge, "to").unwrap_or_default(),
            ])
            .map_err(|error| format!("cannot store architecture edge: {error}"))?;
    }
    insert_chunks(
        transaction,
        "architecture_nodes",
        architecture
            .get("nodes")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default(),
    )?;
    insert_chunks(
        transaction,
        "architecture_edges",
        architecture
            .get("edges")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default(),
    )?;
    Ok(())
}

fn insert_chunks(transaction: &Transaction<'_>, kind: &str, rows: &[Value]) -> Result<(), String> {
    let mut statement = transaction
        .prepare(
            "INSERT INTO chunks(kind, chunk_index, item_count, payload)
             VALUES (?1, ?2, ?3, ?4)",
        )
        .map_err(|error| format!("cannot prepare generation chunks: {error}"))?;
    for (chunk_index, chunk) in rows.chunks(CHUNK_SIZE).enumerate() {
        statement
            .execute(params![
                kind,
                chunk_index as i64,
                chunk.len() as i64,
                json_blob(chunk)?,
            ])
            .map_err(|error| format!("cannot store generation chunk: {error}"))?;
    }
    Ok(())
}

fn publish_current(project_dir: &Path, receipt: &GenerationReceipt) -> Result<(), String> {
    fs::create_dir_all(project_dir)
        .map_err(|error| format!("cannot create project store: {error}"))?;
    let current = project_dir.join("current.json");
    let previous = project_dir.join("previous.json");
    let temporary = project_dir.join(format!("current.{}.tmp", std::process::id()));
    write_json_file(&temporary, receipt)?;
    let had_current = current.is_file();
    if had_current {
        let _ = fs::remove_file(&previous);
        fs::rename(&current, &previous)
            .map_err(|error| format!("cannot rotate code generation: {error}"))?;
    }
    if let Err(error) = fs::rename(&temporary, &current) {
        if had_current {
            let _ = fs::rename(&previous, &current);
        }
        let _ = fs::remove_file(&temporary);
        return Err(format!("cannot publish current code generation: {error}"));
    }
    Ok(())
}

fn prune_generations(project_dir: &Path) {
    let retained = ["current.json", "previous.json"]
        .into_iter()
        .filter_map(|name| fs::read(project_dir.join(name)).ok())
        .filter_map(|bytes| serde_json::from_slice::<GenerationReceipt>(&bytes).ok())
        .map(|receipt| receipt.generation_id)
        .collect::<HashSet<_>>();
    let root = project_dir.join("generations");
    let Ok(entries) = fs::read_dir(&root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if entry.path().is_dir() && name.starts_with("g-") && !retained.contains(&name) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

fn write_json_file<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| format!("cannot serialize generation receipt: {error}"))?;
    let mut file = fs::File::create(path)
        .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    file.write_all(&bytes)
        .and_then(|_| file.flush())
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("cannot write {}: {error}", path.display()))
}

fn result_rows(value: &Value) -> &[Value] {
    value
        .get("rows")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn string_field(values: &[Value], index: usize) -> Option<&str> {
    values.get(index).and_then(Value::as_str)
}

fn value_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn json_string(value: &Value) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| format!("cannot serialize graph value: {error}"))
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

fn json_blob<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, String> {
    let json = serde_json::to_vec(value)
        .map_err(|error| format!("cannot serialize graph value: {error}"))?;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder
        .write_all(&json)
        .map_err(|error| format!("cannot compress graph value: {error}"))?;
    encoder
        .finish()
        .map_err(|error| format!("cannot finish graph compression: {error}"))
}

fn decode_json_blob(bytes: &[u8]) -> Result<Value, String> {
    let json = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut decoder = GzDecoder::new(bytes);
        let mut json = Vec::new();
        decoder
            .read_to_end(&mut json)
            .map_err(|error| format!("cannot decompress graph value: {error}"))?;
        json
    } else {
        bytes.to_vec()
    };
    serde_json::from_slice(&json)
        .map_err(|error| format!("invalid JSON in generation row: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_generation_is_queryable_and_keeps_previous() {
        let root = std::env::temp_dir().join(format!(
            "code-memory-generation-store-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let inventory = json!({
            "columns": ["labels", "name", "qualified_name", "file_path"],
            "rows": [[ ["Function"], "run", "app.run", "app.rs" ]],
            "total": 1
        });
        let calls =
            json!({"columns": ["source", "target"], "rows": [["app.run", "db.save"]], "total": 1});
        let handles = json!({"columns": ["source", "target"], "rows": [], "total": 0});
        let architecture = json!({
            "schema": "architecture",
            "nodes": [{"id": "module:app", "kind": "MODULE", "name": "app"}],
            "edges": [{"id": "edge:1", "kind": "CONTAINS", "from": "project", "to": "module:app"}],
            "flows": []
        });
        let evidence = json!({"schema": "evidence"});

        for generation in ["g-first", "g-second", "g-third"] {
            let staging = root.join(format!(".staging-{generation}"));
            fs::create_dir_all(&staging).unwrap();
            publish_generation(
                &root,
                &staging,
                generation,
                "app",
                "repo",
                &inventory,
                &calls,
                &handles,
                &architecture,
                &evidence,
            )
            .unwrap();
        }

        let store = open_current(&root).unwrap().unwrap();
        assert_eq!(store.receipt.generation_id, "g-third");
        assert_eq!(store.inventory().unwrap()["total"], 1);
        assert_eq!(store.relationships("CALLS").unwrap()["total"], 1);
        assert_eq!(
            store
                .connection
                .query_row("SELECT typeof(payload) FROM chunks LIMIT 1", [], |row| {
                    row.get::<_, String>(0)
                })
                .unwrap(),
            "blob"
        );
        assert_eq!(
            store.architecture().unwrap()["nodes"][0]["id"],
            "module:app"
        );
        assert!(!root.join("generations/g-first").exists());
        assert!(root.join("generations/g-second").is_dir());
        assert!(root.join("generations/g-third").is_dir());
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn sqlite_generation_supports_windows_long_paths() {
        let mut root =
            std::env::temp_dir().join(format!("code-memory-long-path-{}", std::process::id()));
        while root.as_os_str().len() < 280 {
            root.push("workspace-segment-0123456789abcdef");
        }
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let database = root.join("graph.sqlite");
        write_database(
            &database,
            "repo",
            &json!({"columns": [], "rows": []}),
            &json!({"columns": [], "rows": []}),
            &json!({"columns": [], "rows": []}),
            &json!({"nodes": [], "edges": []}),
            &json!({}),
        )
        .unwrap();

        assert!(database.is_file());
        fs::remove_dir_all(root).unwrap();
    }
}
