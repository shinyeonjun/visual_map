use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::time::Duration;

use postgres::config::{Host, SslMode};
use postgres::error::SqlState;
use postgres::{Config, GenericClient, IsolationLevel};
use postgres_native_tls::MakeTlsConnector;

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

const POSTGRES_SOURCE: &str = "postgres";
const MIN_SUPPORTED_MAJOR: i32 = 14;
const MAX_SUPPORTED_MAJOR: i32 = 18;
const MAX_INTROSPECTION_TIMEOUT_MS: u64 = 86_400_000;
const MAX_DEFINITION_BYTES: i32 = 1_048_576;
const MAX_PROPERTY_STRING_BYTES: i32 = 65_536;
const YUGABYTEDB_SOURCE: &str = "yugabytedb";
const CERTIFIED_YUGABYTEDB_VERSION: &str = "15.12-YB-2025.2.3.2-b0";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PgWireProduct {
    PostgreSql,
    YugabyteDb,
}

impl PgWireProduct {
    const fn source_kind(self) -> &'static str {
        match self {
            Self::PostgreSql => POSTGRES_SOURCE,
            Self::YugabyteDb => YUGABYTEDB_SOURCE,
        }
    }

    const fn display_name(self) -> &'static str {
        match self {
            Self::PostgreSql => "PostgreSQL",
            Self::YugabyteDb => "YugabyteDB YSQL",
        }
    }
}

pub(crate) struct PostgresCatalogAdapter {
    connection_string: String,
    expected_product: PgWireProduct,
}

impl PostgresCatalogAdapter {
    pub(crate) fn new(connection_string: impl Into<String>) -> Self {
        Self {
            connection_string: connection_string.into(),
            expected_product: PgWireProduct::PostgreSql,
        }
    }

    pub(crate) fn new_yugabytedb(connection_string: impl Into<String>) -> Self {
        Self {
            connection_string: connection_string.into(),
            expected_product: PgWireProduct::YugabyteDb,
        }
    }
}

impl CatalogIntrospector for PostgresCatalogAdapter {
    fn source_kind(&self) -> &'static str {
        self.expected_product.source_kind()
    }

    fn discover(
        &mut self,
        request: &IntrospectionRequest,
    ) -> Result<CatalogDiscovery, AnalysisFailure> {
        discover_postgres(
            &self.connection_string,
            request,
            &CancellationToken::new(),
            self.expected_product,
        )
    }

    fn discover_with_cancellation(
        &mut self,
        request: &IntrospectionRequest,
        cancellation: &CancellationToken,
    ) -> Result<CatalogDiscovery, AnalysisFailure> {
        discover_postgres(
            &self.connection_string,
            request,
            cancellation,
            self.expected_product,
        )
    }
}

fn discover_postgres(
    connection_string: &str,
    request: &IntrospectionRequest,
    cancellation: &CancellationToken,
    expected_product: PgWireProduct,
) -> Result<CatalogDiscovery, AnalysisFailure> {
    let source_kind = expected_product.source_kind();
    cancellation.checkpoint(
        source_kind,
        &request.connection_alias,
        AnalysisStage::Configuration,
    )?;
    validate_request(request, expected_product)?;
    let mut config = Config::from_str(connection_string).map_err(|error| {
        connection_failure(
            request,
            connection_string,
            error.to_string(),
            expected_product,
        )
    })?;
    validate_transport_policy(request, connection_string, &config, expected_product)?;
    config.connect_timeout(Duration::from_millis(request.timeout_ms));
    let tls = native_tls::TlsConnector::builder()
        .build()
        .map_err(|error| {
            connection_failure(
                request,
                connection_string,
                error.to_string(),
                expected_product,
            )
        })?;
    let mut client = config
        .connect(MakeTlsConnector::new(tls))
        .map_err(|error| {
            classify_postgres_error(
                request,
                connection_string,
                error,
                AnalysisStage::Connection,
                expected_product,
            )
        })?;
    cancellation.checkpoint(
        source_kind,
        &request.connection_alias,
        AnalysisStage::Connection,
    )?;
    let mut transaction = client
        .build_transaction()
        .isolation_level(IsolationLevel::RepeatableRead)
        .read_only(true)
        .start()
        .map_err(|error| {
            classify_postgres_error(
                request,
                connection_string,
                error,
                AnalysisStage::CapabilityProbe,
                expected_product,
            )
        })?;
    let timeout = format!("{}ms", request.timeout_ms);
    transaction
        .query_one(
            "SELECT set_config('statement_timeout', $1, true)",
            &[&timeout],
        )
        .map_err(|error| {
            classify_postgres_error(
                request,
                connection_string,
                error,
                AnalysisStage::CapabilityProbe,
                expected_product,
            )
        })?;
    transaction
        .query_one("SELECT set_config('lock_timeout', $1, true)", &[&timeout])
        .map_err(|error| {
            classify_postgres_error(
                request,
                connection_string,
                error,
                AnalysisStage::CapabilityProbe,
                expected_product,
            )
        })?;
    cancellation.checkpoint(
        source_kind,
        &request.connection_alias,
        AnalysisStage::CapabilityProbe,
    )?;

    let raw = RawPostgresCatalog::read(&mut transaction, request, expected_product)
        .map_err(|error| catalog_failure(request, connection_string, error, expected_product))?;
    cancellation.checkpoint(
        source_kind,
        &request.connection_alias,
        AnalysisStage::Discovery,
    )?;
    let discovery = PostgresSnapshotMapper::new(&request.connection_alias, source_kind)
        .map(raw)
        .map_err(|error| catalog_failure(request, connection_string, error, expected_product))?;
    cancellation.checkpoint(
        source_kind,
        &request.connection_alias,
        AnalysisStage::Mapping,
    )?;
    transaction.commit().map_err(|error| {
        classify_postgres_error(
            request,
            connection_string,
            error,
            AnalysisStage::Discovery,
            expected_product,
        )
    })?;
    Ok(discovery)
}

pub(crate) fn analyze_postgres(
    connection_string: &str,
    connection_alias: &str,
    requested_schemas: Vec<String>,
    timeout_ms: u64,
) -> AnalysisOutcome {
    analyze_postgres_with_cancellation(
        connection_string,
        connection_alias,
        requested_schemas,
        timeout_ms,
        &CancellationToken::new(),
    )
}

pub(crate) fn analyze_postgres_with_cancellation(
    connection_string: &str,
    connection_alias: &str,
    requested_schemas: Vec<String>,
    timeout_ms: u64,
    cancellation: &CancellationToken,
) -> AnalysisOutcome {
    let request = IntrospectionRequest {
        connection_alias: connection_alias.to_owned(),
        requested_catalogs: Vec::new(),
        requested_schemas,
        timeout_ms,
    };
    DatabaseAnalysisService::new(PostgresCatalogAdapter::new(connection_string))
        .analyze_with_cancellation(&request, cancellation)
}

pub(crate) fn analyze_yugabytedb(
    connection_string: &str,
    connection_alias: &str,
    requested_schemas: Vec<String>,
    timeout_ms: u64,
) -> AnalysisOutcome {
    analyze_yugabytedb_with_cancellation(
        connection_string,
        connection_alias,
        requested_schemas,
        timeout_ms,
        &CancellationToken::new(),
    )
}

pub(crate) fn analyze_yugabytedb_with_cancellation(
    connection_string: &str,
    connection_alias: &str,
    requested_schemas: Vec<String>,
    timeout_ms: u64,
    cancellation: &CancellationToken,
) -> AnalysisOutcome {
    let request = IntrospectionRequest {
        connection_alias: connection_alias.to_owned(),
        requested_catalogs: Vec::new(),
        requested_schemas,
        timeout_ms,
    };
    DatabaseAnalysisService::new(PostgresCatalogAdapter::new_yugabytedb(connection_string))
        .analyze_with_cancellation(&request, cancellation)
}

#[derive(Debug)]
enum CatalogError {
    Query(postgres::Error),
    InvalidScope(String),
    PermissionDenied(String),
    UnsupportedProduct(String),
    UnsupportedVersion(i32),
    UnsupportedRelease(String),
    UnsupportedMetadata(String),
    Mapping(String),
}

impl From<postgres::Error> for CatalogError {
    fn from(error: postgres::Error) -> Self {
        Self::Query(error)
    }
}

fn validate_request(
    request: &IntrospectionRequest,
    product: PgWireProduct,
) -> Result<(), AnalysisFailure> {
    let source_kind = product.source_kind();
    let product_name = product.display_name();
    if request.timeout_ms > MAX_INTROSPECTION_TIMEOUT_MS {
        return Err(AnalysisFailure::redacted(
            AnalysisFailureCode::InvalidConfiguration,
            AnalysisStage::Configuration,
            source_kind,
            &request.connection_alias,
            format!(
                "{product_name} introspection timeout exceeds the {MAX_INTROSPECTION_TIMEOUT_MS} ms safety limit"
            ),
            "choose a timeout between 1 ms and 86400000 ms",
            false,
            None,
        ));
    }
    let has_duplicate_catalogs = request.requested_catalogs.len()
        != request
            .requested_catalogs
            .iter()
            .collect::<BTreeSet<_>>()
            .len();
    let has_duplicate_schemas = request.requested_schemas.len()
        != request
            .requested_schemas
            .iter()
            .collect::<BTreeSet<_>>()
            .len();
    if has_duplicate_catalogs || has_duplicate_schemas {
        return Err(AnalysisFailure::redacted(
            AnalysisFailureCode::InvalidConfiguration,
            AnalysisStage::Configuration,
            source_kind,
            &request.connection_alias,
            format!("{product_name} scope contains duplicate catalog or schema names"),
            "provide each requested catalog and schema exactly once",
            false,
            None,
        ));
    }
    Ok(())
}

fn validate_transport_policy(
    request: &IntrospectionRequest,
    connection_string: &str,
    config: &Config,
    product: PgWireProduct,
) -> Result<(), AnalysisFailure> {
    let has_remote_tcp_host = config.get_hosts().iter().any(|host| match host {
        Host::Tcp(host) => !is_loopback_host(host),
        #[cfg(unix)]
        Host::Unix(_) => false,
    });
    if has_remote_tcp_host && config.get_ssl_mode() != SslMode::Require {
        return Err(AnalysisFailure::redacted(
            AnalysisFailureCode::UnsafeSource,
            AnalysisStage::Configuration,
            product.source_kind(),
            &request.connection_alias,
            format!(
                "remote {} connections require sslmode=require to prevent plaintext fallback",
                product.display_name()
            ),
            "set sslmode=require and use a certificate trusted by the operating system",
            false,
            Some(connection_string),
        ));
    }
    Ok(())
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .map(|address| address.is_loopback())
            .unwrap_or(false)
}

fn connection_failure(
    request: &IntrospectionRequest,
    connection_string: &str,
    message: String,
    product: PgWireProduct,
) -> AnalysisFailure {
    AnalysisFailure::redacted(
        AnalysisFailureCode::ConnectionFailed,
        AnalysisStage::Connection,
        product.source_kind(),
        &request.connection_alias,
        message,
        format!(
            "verify the {} connection settings, network path, and TLS policy",
            product.display_name()
        ),
        true,
        Some(connection_string),
    )
}

fn classify_postgres_error(
    request: &IntrospectionRequest,
    connection_string: &str,
    error: postgres::Error,
    stage: AnalysisStage,
    product: PgWireProduct,
) -> AnalysisFailure {
    let message = postgres_error_message(&error);
    let (code, retryable, remediation) = match error.code() {
        Some(code) if code == &SqlState::INVALID_PASSWORD => (
            AnalysisFailureCode::AuthenticationFailed,
            false,
            "verify the database principal and secret",
        ),
        Some(code) if code == &SqlState::INSUFFICIENT_PRIVILEGE => (
            AnalysisFailureCode::PermissionDenied,
            false,
            "grant metadata visibility for every requested schema and retry",
        ),
        Some(code) if code == &SqlState::QUERY_CANCELED => (
            AnalysisFailureCode::Timeout,
            true,
            "increase the bounded timeout or reduce the requested schema scope",
        ),
        _ if stage == AnalysisStage::Connection => (
            AnalysisFailureCode::ConnectionFailed,
            true,
            "verify the database endpoint and retry",
        ),
        _ => (
            AnalysisFailureCode::MetadataQueryFailed,
            true,
            "inspect the database server state and retry the metadata-only analysis",
        ),
    };
    AnalysisFailure::redacted(
        code,
        stage,
        product.source_kind(),
        &request.connection_alias,
        message,
        remediation,
        retryable,
        Some(connection_string),
    )
}

fn postgres_error_message(error: &postgres::Error) -> String {
    match error.as_db_error() {
        Some(database_error) => {
            let mut message = format!(
                "{} (SQLSTATE {})",
                database_error.message(),
                database_error.code().code()
            );
            if let Some(detail) = database_error.detail() {
                message.push_str(": ");
                message.push_str(detail);
            }
            if let Some(hint) = database_error.hint() {
                message.push_str("; hint: ");
                message.push_str(hint);
            }
            message
        }
        None => error.to_string(),
    }
}

fn catalog_failure(
    request: &IntrospectionRequest,
    connection_string: &str,
    error: CatalogError,
    product: PgWireProduct,
) -> AnalysisFailure {
    match error {
        CatalogError::Query(error) => classify_postgres_error(
            request,
            connection_string,
            error,
            AnalysisStage::Discovery,
            product,
        ),
        CatalogError::InvalidScope(message) => AnalysisFailure::redacted(
            AnalysisFailureCode::InvalidConfiguration,
            AnalysisStage::CapabilityProbe,
            product.source_kind(),
            &request.connection_alias,
            message,
            "request the current database and existing non-system schemas",
            false,
            Some(connection_string),
        ),
        CatalogError::PermissionDenied(message) => AnalysisFailure::redacted(
            AnalysisFailureCode::PermissionDenied,
            AnalysisStage::CapabilityProbe,
            product.source_kind(),
            &request.connection_alias,
            message,
            "grant metadata visibility for every requested schema and retry",
            false,
            Some(connection_string),
        ),
        CatalogError::UnsupportedProduct(message) => AnalysisFailure::redacted(
            AnalysisFailureCode::UnsupportedProduct,
            AnalysisStage::CapabilityProbe,
            product.source_kind(),
            &request.connection_alias,
            message,
            "select the matching product adapter; PostgreSQL compatibility is not PostgreSQL certification",
            false,
            Some(connection_string),
        ),
        CatalogError::UnsupportedVersion(major) => AnalysisFailure::redacted(
            AnalysisFailureCode::UnsupportedVersion,
            AnalysisStage::CapabilityProbe,
            product.source_kind(),
            &request.connection_alias,
            format!(
                "PostgreSQL major version {major} is outside the certified {MIN_SUPPORTED_MAJOR}-{MAX_SUPPORTED_MAJOR} range"
            ),
            "use a certified product version or add and verify a product-specific version strategy",
            false,
            Some(connection_string),
        ),
        CatalogError::UnsupportedRelease(message) => AnalysisFailure::redacted(
            AnalysisFailureCode::UnsupportedVersion,
            AnalysisStage::CapabilityProbe,
            product.source_kind(),
            &request.connection_alias,
            message,
            "use an exact live-certified product release or add and verify a new release strategy",
            false,
            Some(connection_string),
        ),
        CatalogError::UnsupportedMetadata(message) => AnalysisFailure::redacted(
            AnalysisFailureCode::UnsupportedMetadata,
            AnalysisStage::CapabilityProbe,
            product.source_kind(),
            &request.connection_alias,
            message,
            "remove the unprovable construct or use a catalog-tracked definition, then re-index",
            false,
            Some(connection_string),
        ),
        CatalogError::Mapping(message) => AnalysisFailure::redacted(
            AnalysisFailureCode::MetadataMappingFailed,
            AnalysisStage::Mapping,
            product.source_kind(),
            &request.connection_alias,
            message,
            format!(
                "fix the adapter mapping for every discovered {} object before retrying",
                product.display_name()
            ),
            false,
            Some(connection_string),
        ),
    }
}

#[derive(Clone, Debug)]
struct ServerFacts {
    database: String,
    version: String,
    version_banner: String,
    version_num: i32,
    current_user: String,
    session_user: String,
    transaction_read_only: bool,
    transaction_isolation: String,
    tls: bool,
    tls_version: Option<String>,
    tls_cipher: Option<String>,
}

