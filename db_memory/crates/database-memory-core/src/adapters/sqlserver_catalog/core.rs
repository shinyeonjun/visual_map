use std::collections::{BTreeMap, BTreeSet};
use std::net::IpAddr;
use std::time::Duration;

use connection_string::AdoNetString;
use sqlparser::dialect::MsSqlDialect;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, Tokenizer};
use tiberius::{Client, Config, FromSql, Row};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

use crate::analysis_outcome::{
    AnalysisFailure, AnalysisFailureCode, AnalysisOutcome, AnalysisStage,
};
use crate::canonical::{
    CanonicalMetadata, CanonicalSchemaSnapshot, MetadataObject, MetadataRelationship,
    MetadataRelationshipKind, MetadataValue, ObjectAnnotation,
};
use crate::certification::{
    emitted_object_counts, emitted_relationship_counts, AdapterIdentity, CapabilityCheck,
    DiscoveredCount, DiscoveryCounts, IntrospectionScope, ObjectCategory, RelationshipCategory,
    ServerIdentity,
};
use crate::introspection::{
    CancellationToken, CatalogDiscovery, CatalogIntrospector, DatabaseAnalysisService,
    IntrospectionRequest,
};
use crate::{
    AdapterCapabilities, CapabilitySupport, ColumnObject, ConstraintKind, ConstraintObject,
    DatabaseObject, IndexObject, ObjectKey, ObjectKind, RoutineKind, RoutineObject, SchemaObject,
    SchemaSnapshot, TableKind, TableObject, TriggerObject, ViewObject,
};

const SQLSERVER_SOURCE: &str = "sqlserver";
const MAX_INTROSPECTION_TIMEOUT_MS: u64 = 86_400_000;
const MAX_DEFINITION_BYTES: i32 = 1_048_576;
const MAX_PROPERTY_STRING_BYTES: usize = 65_536;
const SQLSERVER_ADAPTER_VERSION: &str = env!("CARGO_PKG_VERSION");

type TdsClient = Client<Compat<TcpStream>>;

pub(crate) struct SqlServerCatalogAdapter {
    connection_string: String,
}

impl SqlServerCatalogAdapter {
    pub(crate) fn new(connection_string: impl Into<String>) -> Self {
        Self {
            connection_string: connection_string.into(),
        }
    }
}

impl CatalogIntrospector for SqlServerCatalogAdapter {
    fn source_kind(&self) -> &'static str {
        SQLSERVER_SOURCE
    }

    fn discover(
        &mut self,
        request: &IntrospectionRequest,
    ) -> Result<CatalogDiscovery, AnalysisFailure> {
        self.discover_with_cancellation(request, &CancellationToken::new())
    }

    fn discover_with_cancellation(
        &mut self,
        request: &IntrospectionRequest,
        cancellation: &CancellationToken,
    ) -> Result<CatalogDiscovery, AnalysisFailure> {
        cancellation.checkpoint(
            SQLSERVER_SOURCE,
            &request.connection_alias,
            AnalysisStage::Configuration,
        )?;
        validate_request(request)?;
        validate_connection_policy(request, &self.connection_string)?;
        run_catalog_discovery(&self.connection_string, request, cancellation)
    }
}

pub(crate) fn analyze_sqlserver(
    connection_string: &str,
    connection_alias: &str,
    requested_catalogs: Vec<String>,
    requested_schemas: Vec<String>,
    timeout_ms: u64,
) -> AnalysisOutcome {
    analyze_sqlserver_with_cancellation(
        connection_string,
        connection_alias,
        requested_catalogs,
        requested_schemas,
        timeout_ms,
        &CancellationToken::new(),
    )
}

pub(crate) fn analyze_sqlserver_with_cancellation(
    connection_string: &str,
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
    DatabaseAnalysisService::new(SqlServerCatalogAdapter::new(connection_string))
        .analyze_with_cancellation(&request, cancellation)
}

fn run_catalog_discovery(
    connection_string: &str,
    request: &IntrospectionRequest,
    cancellation: &CancellationToken,
) -> Result<CatalogDiscovery, AnalysisFailure> {
    let connection_string = connection_string.to_owned();
    let request = request.clone();
    let cancellation = cancellation.clone();
    if tokio::runtime::Handle::try_current().is_ok() {
        let worker_request = request.clone();
        let worker_cancellation = cancellation.clone();
        return std::thread::spawn(move || {
            run_catalog_discovery_on_runtime(
                &connection_string,
                &worker_request,
                &worker_cancellation,
            )
        })
        .join()
        .map_err(|_| internal_failure(&request, "SQL Server adapter worker thread panicked"))?;
    }
    run_catalog_discovery_on_runtime(&connection_string, &request, &cancellation)
}

fn run_catalog_discovery_on_runtime(
    connection_string: &str,
    request: &IntrospectionRequest,
    cancellation: &CancellationToken,
) -> Result<CatalogDiscovery, AnalysisFailure> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| internal_failure(request, error.to_string()))?;
    runtime.block_on(async {
        tokio::select! {
            biased;
            _ = wait_for_cancellation(cancellation.clone()) => {
                Err(cancelled_failure(request))
            }
            result = tokio::time::timeout(
                Duration::from_millis(request.timeout_ms),
                discover_catalog_async(connection_string, request, cancellation),
            ) => match result {
                Ok(result) => result,
                Err(_) => Err(AnalysisFailure::redacted(
                    AnalysisFailureCode::Timeout,
                    AnalysisStage::Discovery,
                    SQLSERVER_SOURCE,
                    &request.connection_alias,
                    format!(
                        "SQL Server metadata analysis exceeded the {} ms timeout",
                        request.timeout_ms
                    ),
                    "increase the bounded timeout or reduce the requested schema scope",
                    true,
                    Some(connection_string),
                )),
            },
        }
    })
}

async fn wait_for_cancellation(cancellation: CancellationToken) {
    while !cancellation.is_cancelled() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn cancelled_failure(request: &IntrospectionRequest) -> AnalysisFailure {
    AnalysisFailure::redacted(
        AnalysisFailureCode::Cancelled,
        AnalysisStage::Discovery,
        SQLSERVER_SOURCE,
        &request.connection_alias,
        "SQL Server metadata analysis was cancelled",
        "start a new analysis when the result is still needed",
        true,
        None,
    )
}

async fn discover_catalog_async(
    connection_string: &str,
    request: &IntrospectionRequest,
    cancellation: &CancellationToken,
) -> Result<CatalogDiscovery, AnalysisFailure> {
    let mut client = connect_sqlserver(connection_string, request).await?;
    cancellation.checkpoint(
        SQLSERVER_SOURCE,
        &request.connection_alias,
        AnalysisStage::Connection,
    )?;
    configure_session(&mut client, request)
        .await
        .map_err(|error| catalog_failure(request, connection_string, error))?;
    verify_metadata_privileges(&mut client)
        .await
        .map_err(|error| catalog_failure(request, connection_string, error))?;
    let facts = ServerFacts::read(&mut client)
        .await
        .map_err(|error| catalog_failure(request, connection_string, error))?;
    let strategy = SqlServerCatalogVersion::detect(&facts)
        .map_err(|error| catalog_failure(request, connection_string, error))?;
    validate_scope(request, &facts.database)
        .map_err(|error| catalog_failure(request, connection_string, error))?;
    let available_schemas = read_schemas(&mut client)
        .await
        .map_err(|error| catalog_failure(request, connection_string, error))?;
    let selected_schemas = select_schemas(request, &available_schemas)
        .map_err(|error| catalog_failure(request, connection_string, error))?;
    cancellation.checkpoint(
        SQLSERVER_SOURCE,
        &request.connection_alias,
        AnalysisStage::CapabilityProbe,
    )?;

    let first = RawSqlServerCatalog::read(&mut client, strategy, &selected_schemas)
        .await
        .map_err(|error| catalog_failure(request, connection_string, error))?;
    cancellation.checkpoint(
        SQLSERVER_SOURCE,
        &request.connection_alias,
        AnalysisStage::Discovery,
    )?;
    let second = RawSqlServerCatalog::read(&mut client, strategy, &selected_schemas)
        .await
        .map_err(|error| catalog_failure(request, connection_string, error))?;
    let stable = require_stable_catalog(first, &second)
        .map_err(|error| catalog_failure(request, connection_string, error))?;
    cancellation.checkpoint(
        SQLSERVER_SOURCE,
        &request.connection_alias,
        AnalysisStage::Mapping,
    )?;

    SqlServerSnapshotMapper::new(&request.connection_alias, facts, strategy)
        .map(stable)
        .map_err(|error| catalog_failure(request, connection_string, error))
}

fn require_stable_catalog<T: PartialEq>(first: T, second: &T) -> Result<T, CatalogError> {
    if &first != second {
        return Err(CatalogError::CatalogChanged(
            "SQL Server catalog changed while metadata was being collected".to_owned(),
        ));
    }
    Ok(first)
}

async fn connect_sqlserver(
    connection_string: &str,
    request: &IntrospectionRequest,
) -> Result<TdsClient, AnalysisFailure> {
    let mut config = Config::from_ado_string(connection_string).map_err(|error| {
        connection_failure(request, connection_string, error.to_string(), false)
    })?;
    config.readonly(true);
    config.application_name("database-memory");
    let tcp = TcpStream::connect(config.get_addr())
        .await
        .map_err(|error| connection_failure(request, connection_string, error.to_string(), true))?;
    tcp.set_nodelay(true)
        .map_err(|error| connection_failure(request, connection_string, error.to_string(), true))?;
    Client::connect(config, tcp.compat_write())
        .await
        .map_err(|error| {
            classify_tiberius_error(request, connection_string, error, AnalysisStage::Connection)
        })
}

async fn configure_session(
    client: &mut TdsClient,
    request: &IntrospectionRequest,
) -> Result<(), CatalogError> {
    let lock_timeout = request.timeout_ms.min(i32::MAX as u64);
    let statement = format!(
        "SET NOCOUNT ON; SET XACT_ABORT ON; SET TRANSACTION ISOLATION LEVEL READ COMMITTED; SET LOCK_TIMEOUT {lock_timeout};"
    );
    client.simple_query(statement).await?.into_results().await?;
    Ok(())
}

fn validate_request(request: &IntrospectionRequest) -> Result<(), AnalysisFailure> {
    if request.timeout_ms > MAX_INTROSPECTION_TIMEOUT_MS {
        return Err(AnalysisFailure::redacted(
            AnalysisFailureCode::InvalidConfiguration,
            AnalysisStage::Configuration,
            SQLSERVER_SOURCE,
            &request.connection_alias,
            format!(
                "SQL Server introspection timeout exceeds the {MAX_INTROSPECTION_TIMEOUT_MS} ms safety limit"
            ),
            "choose a timeout between 1 ms and 86400000 ms",
            false,
            None,
        ));
    }
    if has_duplicates(&request.requested_catalogs) || has_duplicates(&request.requested_schemas) {
        return Err(AnalysisFailure::redacted(
            AnalysisFailureCode::InvalidConfiguration,
            AnalysisStage::Configuration,
            SQLSERVER_SOURCE,
            &request.connection_alias,
            "SQL Server scope contains duplicate catalog or schema names",
            "provide each requested catalog and schema exactly once",
            false,
            None,
        ));
    }
    Ok(())
}

fn has_duplicates(values: &[String]) -> bool {
    values.len() != values.iter().collect::<BTreeSet<_>>().len()
}

fn validate_connection_policy(
    request: &IntrospectionRequest,
    connection_string: &str,
) -> Result<(), AnalysisFailure> {
    let values = connection_string.parse::<AdoNetString>().map_err(|error| {
        connection_failure(request, connection_string, error.to_string(), false)
    })?;
    let database = values
        .get("database")
        .or_else(|| values.get("initial catalog"))
        .or_else(|| values.get("databasename"))
        .map(|value| value.trim())
        .filter(|value| !value.is_empty());
    if database.is_none() {
        return Err(AnalysisFailure::redacted(
            AnalysisFailureCode::InvalidConfiguration,
            AnalysisStage::Configuration,
            SQLSERVER_SOURCE,
            &request.connection_alias,
            "SQL Server connection string must select one database",
            "set Database or Initial Catalog explicitly",
            false,
            Some(connection_string),
        ));
    }

    let config = Config::from_ado_string(connection_string).map_err(|error| {
        connection_failure(request, connection_string, error.to_string(), false)
    })?;
    let address = config.get_addr();
    let host = host_from_address(&address);
    let remote = !is_loopback_host(host);
    let encrypt = connection_bool(&values, "encrypt").unwrap_or(false);
    let trust_server_certificate =
        connection_bool(&values, "trustservercertificate").unwrap_or(false);
    if remote && (!encrypt || trust_server_certificate) {
        return Err(AnalysisFailure::redacted(
            AnalysisFailureCode::UnsafeSource,
            AnalysisStage::Configuration,
            SQLSERVER_SOURCE,
            &request.connection_alias,
            "remote SQL Server connections require encrypted transport with certificate validation",
            "set Encrypt=true and TrustServerCertificate=false, then trust the server CA",
            false,
            Some(connection_string),
        ));
    }
    Ok(())
}

fn connection_bool(values: &AdoNetString, key: &str) -> Option<bool> {
    values
        .get(key)
        .and_then(|value| match value.trim().to_ascii_lowercase().as_str() {
            "true" | "yes" => Some(true),
            "false" | "no" => Some(false),
            _ => None,
        })
}

fn host_from_address(address: &str) -> &str {
    address.rsplit_once(':').map_or(address, |(host, _)| host)
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .trim_matches(['[', ']'])
            .parse::<IpAddr>()
            .map(|address| address.is_loopback())
            .unwrap_or(false)
}

fn validate_scope(
    request: &IntrospectionRequest,
    current_database: &str,
) -> Result<(), CatalogError> {
    if request.requested_catalogs.is_empty()
        || request.requested_catalogs == [current_database.to_owned()]
    {
        return Ok(());
    }
    Err(CatalogError::InvalidScope(format!(
        "SQL Server analysis is bound to current database '{current_database}'; requested catalogs were {}",
        request.requested_catalogs.join(", ")
    )))
}

fn select_schemas(
    request: &IntrospectionRequest,
    available: &[RawSchema],
) -> Result<BTreeSet<String>, CatalogError> {
    let names = available
        .iter()
        .map(|schema| schema.name.clone())
        .collect::<BTreeSet<_>>();
    if request.requested_schemas.is_empty() {
        return Ok(names);
    }
    let missing = request
        .requested_schemas
        .iter()
        .filter(|schema| !names.contains(*schema))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(CatalogError::InvalidScope(format!(
            "requested SQL Server schemas are missing or system-owned: {}",
            missing.join(", ")
        )));
    }
    Ok(request.requested_schemas.iter().cloned().collect())
}

#[derive(Debug)]
enum CatalogError {
    Query(tiberius::error::Error),
    InvalidScope(String),
    PermissionDenied(String),
    UnsupportedProduct(String),
    UnsupportedVersion(i32),
    UnsupportedMetadata(String),
    CatalogChanged(String),
    Mapping(String),
}

impl From<tiberius::error::Error> for CatalogError {
    fn from(error: tiberius::error::Error) -> Self {
        Self::Query(error)
    }
}

fn connection_failure(
    request: &IntrospectionRequest,
    connection_string: &str,
    message: String,
    retryable: bool,
) -> AnalysisFailure {
    AnalysisFailure::redacted(
        AnalysisFailureCode::ConnectionFailed,
        AnalysisStage::Connection,
        SQLSERVER_SOURCE,
        &request.connection_alias,
        message,
        "verify the SQL Server connection settings, endpoint, credentials, and TLS policy",
        retryable,
        Some(connection_string),
    )
}

fn classify_tiberius_error(
    request: &IntrospectionRequest,
    connection_string: &str,
    error: tiberius::error::Error,
    stage: AnalysisStage,
) -> AnalysisFailure {
    let code = error.code();
    let (failure_code, retryable, remediation) = match code {
        Some(18456) => (
            AnalysisFailureCode::AuthenticationFailed,
            false,
            "verify the SQL Server principal and secret",
        ),
        Some(229 | 15151 | 916) => (
            AnalysisFailureCode::PermissionDenied,
            false,
            "grant database metadata visibility and dependency catalog access",
        ),
        Some(1222) => (
            AnalysisFailureCode::Timeout,
            true,
            "retry after concurrent schema work finishes or increase the bounded timeout",
        ),
        Some(1205) => (
            AnalysisFailureCode::MetadataQueryFailed,
            true,
            "retry the metadata-only analysis after the deadlock victim transaction ends",
        ),
        _ if stage == AnalysisStage::Connection => (
            AnalysisFailureCode::ConnectionFailed,
            true,
            "verify the SQL Server endpoint and TLS policy, then retry",
        ),
        _ => (
            AnalysisFailureCode::MetadataQueryFailed,
            true,
            "inspect the SQL Server state and retry the metadata-only analysis",
        ),
    };
    AnalysisFailure::redacted(
        failure_code,
        stage,
        SQLSERVER_SOURCE,
        &request.connection_alias,
        error.to_string(),
        remediation,
        retryable,
        Some(connection_string),
    )
}

fn catalog_failure(
    request: &IntrospectionRequest,
    connection_string: &str,
    error: CatalogError,
) -> AnalysisFailure {
    match error {
        CatalogError::Query(error) => classify_tiberius_error(
            request,
            connection_string,
            error,
            AnalysisStage::Discovery,
        ),
        CatalogError::InvalidScope(message) => AnalysisFailure::redacted(
            AnalysisFailureCode::InvalidConfiguration,
            AnalysisStage::CapabilityProbe,
            SQLSERVER_SOURCE,
            &request.connection_alias,
            message,
            "select the current database and existing non-system schemas",
            false,
            Some(connection_string),
        ),
        CatalogError::PermissionDenied(message) => AnalysisFailure::redacted(
            AnalysisFailureCode::PermissionDenied,
            AnalysisStage::CapabilityProbe,
            SQLSERVER_SOURCE,
            &request.connection_alias,
            message,
            "grant VIEW DEFINITION on the database and SELECT on sys.sql_expression_dependencies",
            false,
            Some(connection_string),
        ),
        CatalogError::UnsupportedProduct(message) => AnalysisFailure::redacted(
            AnalysisFailureCode::UnsupportedProduct,
            AnalysisStage::CapabilityProbe,
            SQLSERVER_SOURCE,
            &request.connection_alias,
            message,
            "use a certified SQL Server Database Engine product or add a live-tested product strategy",
            false,
            Some(connection_string),
        ),
        CatalogError::UnsupportedVersion(major) => AnalysisFailure::redacted(
            AnalysisFailureCode::UnsupportedVersion,
            AnalysisStage::CapabilityProbe,
            SQLSERVER_SOURCE,
            &request.connection_alias,
            format!("SQL Server major version {major} is not yet certified"),
            "use SQL Server 2022 while additional major-version strategies are being certified",
            false,
            Some(connection_string),
        ),
        CatalogError::UnsupportedMetadata(message) => AnalysisFailure::redacted(
            AnalysisFailureCode::UnsupportedMetadata,
            AnalysisStage::CapabilityProbe,
            SQLSERVER_SOURCE,
            &request.connection_alias,
            message,
            "remove or replace the unprovable construct, or extend and live-test the SQL Server strategy",
            false,
            Some(connection_string),
        ),
        CatalogError::CatalogChanged(message) => AnalysisFailure::redacted(
            AnalysisFailureCode::CompletenessMismatch,
            AnalysisStage::Discovery,
            SQLSERVER_SOURCE,
            &request.connection_alias,
            message,
            "retry after concurrent DDL has completed",
            true,
            Some(connection_string),
        ),
        CatalogError::Mapping(message) => AnalysisFailure::redacted(
            AnalysisFailureCode::MetadataMappingFailed,
            AnalysisStage::Mapping,
            SQLSERVER_SOURCE,
            &request.connection_alias,
            message,
            "fix every SQL Server catalog mapping before retrying",
            false,
            Some(connection_string),
        ),
    }
}

fn internal_failure(request: &IntrospectionRequest, message: impl AsRef<str>) -> AnalysisFailure {
    AnalysisFailure::redacted(
        AnalysisFailureCode::Internal,
        AnalysisStage::Discovery,
        SQLSERVER_SOURCE,
        &request.connection_alias,
        message,
        "restart the analysis and inspect the adapter runtime if the failure repeats",
        true,
        None,
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ServerFacts {
    database: String,
    version: String,
    major: i32,
    engine_edition: i32,
    edition: String,
    current_user: String,
    login: String,
    original_login: String,
    collation: String,
    compatibility_level: u8,
    database_read_only: bool,
    containment: String,
    encrypted_transport: bool,
}
