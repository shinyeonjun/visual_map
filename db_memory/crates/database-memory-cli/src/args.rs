use std::path::{Path, PathBuf};

use database_memory_core::config::{
    default_config_path as default_config_file_path, load_optional_config, DatabaseMemoryConfig,
    ResolvedConnectionProfile,
};
use database_memory_core::impact_analysis::Direction;
use database_memory_core::interface_contract::{
    DEFAULT_OBJECT_PAGE_LIMIT, DEFAULT_RELATIONSHIP_LIMIT, DEFAULT_TIMEOUT_MS,
};
use database_memory_core::ObjectKind;

pub(crate) const DEFAULT_TRAVERSAL_DEPTH: u32 = 3;
pub(crate) const DEFAULT_RESULT_LIMIT: usize = 100;
pub(crate) const MAX_TRAVERSAL_DEPTH: u32 = 8;
pub(crate) const MAX_RESULT_LIMIT: usize = 200;
pub(crate) const DEFAULT_INVENTORY_LIMIT: usize = 1_000;
pub(crate) const MAX_INVENTORY_TABLES: usize = 5_000;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Contract {
        format: OutputFormat,
    },
    Index {
        source: String,
        path: Option<PathBuf>,
        connection_string: Option<String>,
        alias: String,
        requested_catalogs: Vec<String>,
        requested_schemas: Vec<String>,
        timeout_ms: u64,
        format: OutputFormat,
        cache_path: PathBuf,
    },
    ListSnapshots {
        format: OutputFormat,
        cache_path: PathBuf,
    },
    DescribeSnapshot {
        selector: String,
        format: OutputFormat,
        cache_path: PathBuf,
    },
    ListObjects {
        selector: String,
        kind: Option<ObjectKind>,
        offset: usize,
        limit: usize,
        format: OutputFormat,
        cache_path: PathBuf,
    },
    FindObjects {
        selector: String,
        query: String,
        kind: Option<ObjectKind>,
        offset: usize,
        limit: usize,
        format: OutputFormat,
        cache_path: PathBuf,
    },
    DescribeObject {
        selector: String,
        object_key: String,
        relationship_limit: usize,
        format: OutputFormat,
        cache_path: PathBuf,
    },
    DescribeTable {
        alias: String,
        object_key: Option<String>,
        table_name: Option<String>,
        format: OutputFormat,
        cache_path: PathBuf,
    },
    Inventory {
        alias: String,
        offset: usize,
        limit: usize,
        cache_path: PathBuf,
    },
    FindTable {
        alias: String,
        query: String,
        format: OutputFormat,
        cache_path: PathBuf,
    },
    FindColumn {
        alias: String,
        query: String,
        format: OutputFormat,
        cache_path: PathBuf,
    },
    ImpactAnalysis {
        alias: String,
        object_key: Option<String>,
        table_name: Option<String>,
        column_name: Option<String>,
        direction: Direction,
        max_depth: u32,
        limit: usize,
        cache_path: PathBuf,
    },
    TraceRelationships {
        alias: String,
        object_key: String,
        direction: Direction,
        max_depth: u32,
        limit: usize,
        cache_path: PathBuf,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutputFormat {
    Text,
    Json,
}

include!("args/parse_core.rs");
include!("args/parse_details.rs");
include!("args/parse_helpers.rs");

include!("args_tests.rs");
