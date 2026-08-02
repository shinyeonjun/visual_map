use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use rusqlite::{Connection, ErrorCode, OpenFlags};

use super::sqlite::SqliteAdapterError;
use super::sqlite_sql::{
    parse_index_definition, parse_table_definition, parse_trigger_definition, ParsedConstraint,
    ParsedConstraintKind, ParsedIndexDefinition, ParsedTableDefinition, ParsedTriggerDefinition,
};
use crate::analysis_outcome::{
    AnalysisFailure, AnalysisFailureCode, AnalysisOutcome, AnalysisStage,
};
use crate::canonical::{
    CanonicalMetadata, CanonicalSchemaSnapshot, MetadataObject, MetadataRelationship,
    MetadataRelationshipKind, MetadataValue, ObjectAnnotation,
};
use crate::certification::{
    AdapterIdentity, CapabilityCheck, DiscoveredCount, DiscoveryCounts, IntrospectionScope,
    ObjectCategory, RelationshipCategory, ServerIdentity,
};
use crate::introspection::{
    CancellationToken, CanonicalSnapshotAssembler, CatalogDiscovery, CatalogIntrospector,
    DatabaseAnalysisService, IntrospectionRequest,
};
use crate::{
    AdapterCapabilities, CapabilitySupport, ColumnObject, ConstraintKind, ConstraintObject,
    DatabaseObject, IndexObject, ObjectKey, ObjectKind, SchemaObject, SchemaSnapshot, TableKind,
    TableObject, TriggerObject, ViewObject,
};

const SQLITE_SOURCE: &str = "sqlite";
const MAIN_CATALOG: &str = "main";
const MAIN_SCHEMA: &str = "main";
const MAX_SCHEMA_SQL_BYTES: usize = 1_048_576;
const MAX_INTROSPECTION_TIMEOUT_MS: u64 = 86_400_000;

pub(crate) struct SqlitePathCatalogAdapter {
    path: PathBuf,
}

impl SqlitePathCatalogAdapter {
    pub(crate) fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl CatalogIntrospector for SqlitePathCatalogAdapter {
    fn source_kind(&self) -> &'static str {
        SQLITE_SOURCE
    }

    fn discover(
        &mut self,
        request: &IntrospectionRequest,
    ) -> Result<CatalogDiscovery, AnalysisFailure> {
        discover_sqlite_path(&self.path, request, &CancellationToken::new())
    }

    fn discover_with_cancellation(
        &mut self,
        request: &IntrospectionRequest,
        cancellation: &CancellationToken,
    ) -> Result<CatalogDiscovery, AnalysisFailure> {
        discover_sqlite_path(&self.path, request, cancellation)
    }
}

fn discover_sqlite_path(
    path: &Path,
    request: &IntrospectionRequest,
    cancellation: &CancellationToken,
) -> Result<CatalogDiscovery, AnalysisFailure> {
    cancellation.checkpoint(
        SQLITE_SOURCE,
        &request.connection_alias,
        AnalysisStage::Configuration,
    )?;
    validate_sqlite_scope(request)?;
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| sqlite_open_failure(request, error))?;
    cancellation.checkpoint(
        SQLITE_SOURCE,
        &request.connection_alias,
        AnalysisStage::Connection,
    )?;
    conn.busy_timeout(Duration::from_millis(request.timeout_ms))
        .map_err(|error| sqlite_failure(request, cancellation, error.into()))?;
    conn.pragma_update(None, "query_only", true)
        .map_err(|error| sqlite_capability_failure(request, error))?;

    let deadline = Instant::now() + Duration::from_millis(request.timeout_ms);
    let progress_cancellation = cancellation.clone();
    conn.progress_handler(
        1_000,
        Some(move || progress_cancellation.is_cancelled() || Instant::now() >= deadline),
    );

    let discovery = discover_sqlite_connection(
        &conn,
        SQLITE_SOURCE,
        SQLITE_SOURCE,
        &request.connection_alias,
        vec![
            "SQLite file opened read-only; only sqlite_schema and PRAGMA metadata were read."
                .to_owned(),
        ],
    );
    conn.progress_handler(0, None::<fn() -> bool>);
    discovery.map_err(|error| sqlite_failure(request, cancellation, error))
}

pub(crate) fn analyze_sqlite_path(path: &Path, connection_alias: &str) -> AnalysisOutcome {
    analyze_sqlite_path_with_cancellation(path, connection_alias, &CancellationToken::new())
}

pub(crate) fn analyze_sqlite_path_with_cancellation(
    path: &Path,
    connection_alias: &str,
    cancellation: &CancellationToken,
) -> AnalysisOutcome {
    analyze_sqlite_path_scoped_with_cancellation(
        path,
        connection_alias,
        vec![MAIN_CATALOG.to_owned()],
        vec![MAIN_SCHEMA.to_owned()],
        30_000,
        cancellation,
    )
}

pub(crate) fn analyze_sqlite_path_scoped_with_cancellation(
    path: &Path,
    connection_alias: &str,
    requested_catalogs: Vec<String>,
    requested_schemas: Vec<String>,
    timeout_ms: u64,
    cancellation: &CancellationToken,
) -> AnalysisOutcome {
    let request = IntrospectionRequest {
        connection_alias: connection_alias.to_owned(),
        requested_catalogs,
        requested_schemas,
        timeout_ms,
    };
    DatabaseAnalysisService::new(SqlitePathCatalogAdapter::new(path))
        .analyze_with_cancellation(&request, cancellation)
}

pub(crate) fn discover_sqlite_connection(
    conn: &Connection,
    snapshot_source_kind: &str,
    object_source_kind: &str,
    connection_alias: &str,
    notes: Vec<String>,
) -> Result<CatalogDiscovery, SqliteAdapterError> {
    let transaction = conn.unchecked_transaction()?;
    let raw = RawSqliteCatalog::read(&transaction)?;
    let discovery = SqliteSnapshotMapper::new(
        &transaction,
        snapshot_source_kind,
        object_source_kind,
        connection_alias,
        notes,
    )
    .map(raw)?;
    transaction.commit()?;
    Ok(discovery)
}

pub(crate) fn certify_discovery(
    discovery: CatalogDiscovery,
) -> Result<crate::certification::CertifiedSchemaSnapshot, crate::certification::CertificationError>
{
    CanonicalSnapshotAssembler::certify(discovery)
}

fn validate_sqlite_scope(request: &IntrospectionRequest) -> Result<(), AnalysisFailure> {
    let catalogs_are_valid = request.requested_catalogs.is_empty()
        || request.requested_catalogs == [MAIN_CATALOG.to_owned()];
    let schemas_are_valid = request.requested_schemas.is_empty()
        || request.requested_schemas == [MAIN_SCHEMA.to_owned()];
    if catalogs_are_valid && schemas_are_valid {
        if request.timeout_ms <= MAX_INTROSPECTION_TIMEOUT_MS {
            return Ok(());
        }
        return Err(AnalysisFailure::redacted(
            AnalysisFailureCode::InvalidConfiguration,
            AnalysisStage::Configuration,
            SQLITE_SOURCE,
            &request.connection_alias,
            format!(
                "SQLite introspection timeout exceeds the {MAX_INTROSPECTION_TIMEOUT_MS} ms safety limit"
            ),
            "choose a timeout between 1 ms and 86400000 ms",
            false,
            None,
        ));
    }
    Err(AnalysisFailure::redacted(
        AnalysisFailureCode::InvalidConfiguration,
        AnalysisStage::Configuration,
        SQLITE_SOURCE,
        &request.connection_alias,
        "SQLite certified introspection supports the main catalog/schema only",
        "request catalog 'main' and schema 'main'",
        false,
        None,
    ))
}

fn sqlite_failure(
    request: &IntrospectionRequest,
    cancellation: &CancellationToken,
    error: SqliteAdapterError,
) -> AnalysisFailure {
    if matches!(
        &error,
        SqliteAdapterError::Storage(storage)
            if storage.sqlite_error_code() == Some(ErrorCode::OperationInterrupted)
    ) {
        let cancelled = cancellation.is_cancelled();
        return AnalysisFailure::redacted(
            if cancelled {
                AnalysisFailureCode::Cancelled
            } else {
                AnalysisFailureCode::Timeout
            },
            AnalysisStage::Discovery,
            SQLITE_SOURCE,
            &request.connection_alias,
            if cancelled {
                "SQLite metadata introspection was cancelled"
            } else {
                "SQLite metadata introspection exceeded its configured timeout"
            },
            if cancelled {
                "start a new analysis when the result is still needed"
            } else {
                "increase the bounded timeout or reduce schema complexity, then retry"
            },
            true,
            None,
        );
    }
    let (code, stage, remediation) = match &error {
        SqliteAdapterError::Storage(_) => (
            AnalysisFailureCode::MetadataQueryFailed,
            AnalysisStage::Discovery,
            "verify that the SQLite file is readable and retry",
        ),
        SqliteAdapterError::Parse { .. } => (
            AnalysisFailureCode::UnsupportedMetadata,
            AnalysisStage::Mapping,
            "use a supported SQLite schema construct or upgrade the adapter parser",
        ),
        SqliteAdapterError::Mapping { .. } => (
            AnalysisFailureCode::MetadataMappingFailed,
            AnalysisStage::Mapping,
            "repair the inconsistent schema metadata and retry",
        ),
        SqliteAdapterError::Certification(_) => (
            AnalysisFailureCode::ValidationFailed,
            AnalysisStage::Validation,
            "inspect the completeness evidence and repair the adapter mapping",
        ),
    };
    AnalysisFailure::redacted(
        code,
        stage,
        SQLITE_SOURCE,
        &request.connection_alias,
        error.to_string(),
        remediation,
        false,
        None,
    )
}

fn sqlite_open_failure(request: &IntrospectionRequest, error: rusqlite::Error) -> AnalysisFailure {
    AnalysisFailure::redacted(
        AnalysisFailureCode::ConnectionFailed,
        AnalysisStage::Connection,
        SQLITE_SOURCE,
        &request.connection_alias,
        error.to_string(),
        "verify that the SQLite file exists and is readable, then retry",
        true,
        None,
    )
}

fn sqlite_capability_failure(
    request: &IntrospectionRequest,
    error: rusqlite::Error,
) -> AnalysisFailure {
    AnalysisFailure::redacted(
        AnalysisFailureCode::PermissionDenied,
        AnalysisStage::CapabilityProbe,
        SQLITE_SOURCE,
        &request.connection_alias,
        error.to_string(),
        "allow read-only query_only metadata access and retry",
        false,
        None,
    )
}

#[derive(Clone, Debug)]
struct RawSqliteCatalog {
    sqlite_version: String,
    schema_version: i64,
    database_names: Vec<String>,
    relations: Vec<RawRelation>,
    indexes: Vec<RawIndex>,
    foreign_keys: BTreeMap<String, Vec<RawForeignKey>>,
    triggers: Vec<RawTrigger>,
}

