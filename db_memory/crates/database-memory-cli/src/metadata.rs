use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use database_memory_core::graph_store::{GraphNodeRecord, GraphStore};
use database_memory_core::impact_analysis::{
    impact_analysis_bounded, Direction, ImpactAnalysisResult,
};
use database_memory_core::relationship_trace::{trace_relationships_bounded, GraphPath};
use database_memory_core::{
    capability_warnings, ColumnObject, ConstraintKind, ConstraintObject, IndexObject, ObjectKey,
    ObjectKind, SchemaSnapshot, TableObject,
};
use serde_json::json;

use crate::{
    args::{OutputFormat, MAX_INVENTORY_TABLES, MAX_RESULT_LIMIT, MAX_TRAVERSAL_DEPTH},
    PRODUCT_CONTRACT_VERSION,
};

pub(crate) fn open_existing_store(cache_path: &Path) -> Result<GraphStore, String> {
    if !cache_path.exists() {
        return Err(format!(
            "cache path '{}' not found; run index first",
            cache_path.display()
        ));
    }
    GraphStore::open(cache_path).map_err(|err| err.to_string())
}

pub(crate) fn resolve_snapshot_key(store: &GraphStore, selector: &str) -> Result<String, String> {
    if store
        .get_snapshot(selector)
        .map_err(|error| error.to_string())?
        .is_some()
    {
        return Ok(selector.to_owned());
    }

    let matches = store
        .list_snapshots()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|snapshot| {
            snapshot
                .snapshot_key
                .split_once(':')
                .map(|(_, alias)| alias == selector)
                .unwrap_or(false)
        })
        .map(|snapshot| snapshot.snapshot_key)
        .collect::<Vec<_>>();

    match matches.as_slice() {
        [snapshot_key] => Ok(snapshot_key.clone()),
        [] => Err(format!(
            "snapshot selector '{selector}' not found in cache; run index first"
        )),
        _ => Err(format!(
            "snapshot alias '{selector}' is ambiguous; use one snapshot key: {}",
            matches.join(", ")
        )),
    }
}

pub(crate) fn require_snapshot(store: &GraphStore, snapshot_key: &str) -> Result<(), String> {
    store
        .get_snapshot(snapshot_key)
        .map_err(|err| err.to_string())?
        .map(|_| ())
        .ok_or_else(|| format!("snapshot '{snapshot_key}' not found in cache; run index first"))
}

fn snapshot_capability_warnings(
    store: &GraphStore,
    snapshot_key: &str,
) -> Result<Vec<String>, String> {
    store
        .get_snapshot_capabilities(snapshot_key)
        .map_err(|err| err.to_string())?
        .map(|capabilities| capability_warnings(&capabilities))
        .ok_or_else(|| format!("snapshot '{snapshot_key}' not found in cache; run index first"))
}

pub(crate) struct TableDescription {
    snapshot_key: String,
    table_key: String,
    table_name: String,
    columns: Vec<ColumnObject>,
    primary_key: Vec<String>,
    constraints: Vec<ConstraintObject>,
    outbound_foreign_keys: Vec<ForeignKeyDescription>,
    inbound_foreign_keys: Vec<ForeignKeyDescription>,
    indexes: Vec<IndexObject>,
    dependents: Vec<DependentObjectDescription>,
    capability_warnings: Vec<String>,
}

struct ForeignKeyDescription {
    key: String,
    table_key: String,
    name: String,
    table: String,
    columns: Vec<String>,
    column_keys: Vec<String>,
    referenced_table_key: Option<String>,
    referenced_table: String,
    referenced_columns: Vec<String>,
    referenced_column_keys: Vec<String>,
}

#[derive(Clone)]
struct DependentObjectDescription {
    key: String,
    kind: String,
    name: String,
    relation: String,
    column_keys: Vec<String>,
}

include!("metadata/table.rs");
include!("metadata/inventory.rs");
include!("metadata/impact.rs");

include!("metadata_tests.rs");
