use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use database_memory_core::graph_query::{
    query_graph as run_query_graph, GraphQuery, GraphQueryResult, GraphQueryTraversal,
    PayloadArrayMinLen,
};
use database_memory_core::graph_store::{GraphNodeRecord, GraphStore};
use database_memory_core::impact_analysis::{
    impact_analysis_bounded as run_impact_analysis, Direction, ImpactAnalysisResult,
};
use database_memory_core::interface_contract::{
    describe_object as describe_generic_object, describe_snapshot, index_complete_source,
    list_objects as list_generic_objects, list_snapshot_summaries, product_contract,
    CompleteIndexRequest, InterfaceError, InterfaceStage, DEFAULT_TIMEOUT_MS,
    INTERFACE_CONTRACT_VERSION,
};
use database_memory_core::relationship_trace::{
    trace_relationships_bounded as run_trace_relationships, GraphPath,
};
use database_memory_core::schema_diff::{
    schema_diff_bounded as run_schema_diff, BoundedSchemaDiff,
};
use database_memory_core::{
    capability_warnings, ColumnObject, ConstraintKind, ConstraintObject, IndexObject, ObjectKey,
    ObjectKind,
};
use serde::Serialize;
use serde_json::{json, Value};

use crate::path_policy::DEFAULT_CACHE_PATH;
use crate::types::*;

const DEFAULT_PAGE_LIMIT: usize = 100;
const MAX_PAGE_LIMIT: usize = 500;
const DEFAULT_TRAVERSAL_DEPTH: u32 = 3;
const MAX_TRAVERSAL_DEPTH: u32 = 8;
const DEFAULT_RESULT_LIMIT: usize = 100;
const MAX_RESULT_LIMIT: usize = 200;

struct Page<T> {
    items: Vec<T>,
    metadata: PageMetadata,
}

include!("tools/request_handlers.rs");
include!("tools/metadata.rs");
include!("tools/store.rs");
include!("tools/output.rs");
