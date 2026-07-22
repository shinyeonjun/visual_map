use crate::engine;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Workspace {
    pub id: String,
    pub name: String,
    pub repo_path: String,
    #[serde(default)]
    pub repo_source: RepoSource,
    #[serde(default)]
    pub repo_origin: Option<String>,
    pub code_project: Option<String>,
    #[serde(default)]
    pub engine_cache: WorkspaceEngineCache,
    pub db_profiles: Vec<DbProfile>,
    pub active_db_profile_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RepoSource {
    #[default]
    Local,
    Github,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspaceEngineCache {
    pub code_cache_path: Option<String>,
    pub db_cache_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DbProfile {
    pub id: String,
    pub name: String,
    pub source: DbSource,
    pub path: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub username: Option<String>,
    pub cache_path: String,
    pub last_indexed_at: Option<String>,
    pub password_stored: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum DbSource {
    Sqlite,
    DdlSqlite,
    Postgres,
    Yugabytedb,
    Mysql,
    Mariadb,
    Sqlserver,
    Oracle,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreateWorkspaceRequest {
    pub name: String,
    pub repo_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SaveDbProfileRequest {
    pub workspace_id: String,
    pub name: String,
    pub source: DbSource,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IndexDbProfileRequest {
    pub workspace_id: String,
    pub profile_id: String,
    pub connection_string: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IndexCodeRequest {
    pub workspace_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DbIndexResult {
    pub workspace: Workspace,
    pub run: engine::EngineRunResult,
    pub index_json: Option<serde_json::Value>,
    pub inventory: Option<DbInventory>,
    pub inventory_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodeIndexResult {
    pub workspace: Workspace,
    pub run: engine::EngineRunResult,
    pub inventory: Option<CodeInventory>,
    pub inventory_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodeInventory {
    pub project: String,
    pub routes: Vec<CodeInventoryItem>,
    pub services: Vec<CodeInventoryItem>,
    pub files: Vec<CodeInventoryItem>,
    pub handlers: Vec<CodeInventoryItem>,
    pub repositories: Vec<CodeInventoryItem>,
    pub functions: Vec<CodeInventoryItem>,
    pub classes: Vec<CodeInventoryItem>,
    pub modules: Vec<CodeInventoryItem>,
    pub unknown: Vec<CodeInventoryItem>,
    pub summary: CodeInventorySummary,
    pub architecture: Option<serde_json::Value>,
    pub calls: Vec<CodeCall>,
    #[serde(default)]
    pub handles: Vec<CodeHandle>,
    #[serde(default)]
    pub partial: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodeInventorySummary {
    pub routes: usize,
    pub handlers: usize,
    pub services: usize,
    pub repositories: usize,
    pub functions: usize,
    pub classes: usize,
    pub modules: usize,
    pub files: usize,
    pub unknown: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodeInventoryItem {
    pub id: String,
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub qualified_name: String,
    #[serde(default)]
    pub engine_label: String,
    pub file_path: Option<String>,
    pub line: Option<u64>,
    #[serde(default)]
    pub column: Option<u64>,
    #[serde(default)]
    pub end_line: Option<u64>,
    #[serde(default)]
    pub end_column: Option<u64>,
    pub detail: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodeCall {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub confidence: Option<u8>,
    #[serde(default)]
    pub strategy: Option<String>,
    #[serde(default)]
    pub expression: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodeHandle {
    pub handler: String,
    pub route: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FocusedCodeSearch {
    pub matches: Vec<FocusedCodeSearchMatch>,
    pub totals: FocusedCodeSearchTotals,
    pub partial: bool,
    pub partial_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FocusedCodeSearchMatch {
    pub qualified_name: String,
    pub label: String,
    pub file: String,
    pub start_line: u64,
    pub end_line: u64,
    pub match_lines: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FocusedCodeSearchTotals {
    pub returned: usize,
    pub total_results: usize,
    pub total_grep_matches: usize,
    pub raw_match_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DbInventory {
    pub profile_id: String,
    pub tables: Vec<DbInventoryTable>,
    #[serde(default)]
    pub snapshot_key: Option<String>,
    #[serde(default)]
    pub contract_version: Option<String>,
    #[serde(default)]
    pub capability_warnings: Vec<String>,
    #[serde(default)]
    pub limit_requested: Option<usize>,
    #[serde(default)]
    pub limit_applied: Option<usize>,
    #[serde(default)]
    pub limit_clamped: Option<bool>,
    #[serde(default)]
    pub result_count: Option<usize>,
    #[serde(default)]
    pub total_tables: Option<usize>,
    #[serde(default)]
    pub truncated: Option<bool>,
    #[serde(default)]
    pub gaps: Vec<DbInventoryGap>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DbInventoryTable {
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub database: Option<String>,
    pub schema: Option<String>,
    pub name: String,
    pub columns: Vec<DbInventoryColumn>,
    #[serde(default)]
    pub foreign_keys: Vec<DbForeignKey>,
    #[serde(default)]
    pub inbound_foreign_keys: Vec<DbForeignKey>,
    #[serde(default)]
    pub constraints: Vec<DbConstraint>,
    #[serde(default)]
    pub indexes: Vec<DbIndex>,
    #[serde(default)]
    pub dependents: Vec<DbDependentObject>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DbDependentObject {
    pub key: String,
    pub kind: String,
    pub name: String,
    pub relation: String,
    #[serde(default)]
    pub column_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DbInventoryColumn {
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub table_key: Option<String>,
    pub name: String,
    pub data_type: Option<String>,
    pub nullable: Option<bool>,
    pub is_primary_key: bool,
    pub is_foreign_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DbForeignKey {
    #[serde(default)]
    pub key: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub table_key: Option<String>,
    #[serde(default)]
    pub table_schema: Option<String>,
    #[serde(default)]
    pub table: Option<String>,
    pub columns: Vec<String>,
    #[serde(default)]
    pub column_keys: Vec<String>,
    #[serde(default)]
    pub referenced_table_key: Option<String>,
    pub referenced_schema: Option<String>,
    pub referenced_table: String,
    pub referenced_columns: Vec<String>,
    #[serde(default)]
    pub referenced_column_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DbConstraint {
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub column_keys: Vec<String>,
    #[serde(default)]
    pub referenced_table_key: Option<String>,
    #[serde(default)]
    pub referenced_schema: Option<String>,
    #[serde(default)]
    pub referenced_table: Option<String>,
    #[serde(default)]
    pub referenced_columns: Vec<String>,
    #[serde(default)]
    pub referenced_column_keys: Vec<String>,
    #[serde(default)]
    pub expression: Option<String>,
    #[serde(default)]
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DbIndex {
    #[serde(default)]
    pub key: Option<String>,
    pub name: String,
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub column_keys: Vec<String>,
    #[serde(default)]
    pub unique: bool,
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub predicate: Option<String>,
    #[serde(default)]
    pub expression: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DbInventoryGap {
    pub id: String,
    pub kind: String,
    pub message: String,
    #[serde(default)]
    pub table_key: Option<String>,
}
