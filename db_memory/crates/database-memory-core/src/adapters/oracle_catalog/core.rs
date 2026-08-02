use std::collections::{BTreeMap, BTreeSet};
use std::net::IpAddr;
use std::time::{Duration, Instant};

use oracle::{Connection, Version};

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
    CancellationToken, CatalogDiscovery, CatalogIntrospector, DatabaseAnalysisService,
    IntrospectionRequest,
};
use crate::{
    AdapterCapabilities, CapabilitySupport, ColumnObject, ConstraintKind, ConstraintObject,
    DatabaseObject, IndexObject, ObjectKey, ObjectKind, RoutineKind, RoutineObject, SchemaObject,
    SchemaSnapshot, TableKind, TableObject, TriggerObject, ViewObject,
};

const ORACLE_SOURCE: &str = "oracle";
const ORACLE_ADAPTER_VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_INTROSPECTION_TIMEOUT_MS: u64 = 86_400_000;
const MAX_DEFINITION_BYTES: usize = 1_048_576;
const MAX_ROUTINE_SIGNATURE_BYTES: usize = 4_096;

pub(crate) struct OracleCatalogAdapter {
    connection_string: String,
}

impl OracleCatalogAdapter {
    pub(crate) fn new(connection_string: impl Into<String>) -> Self {
        Self {
            connection_string: connection_string.into(),
        }
    }
}

impl CatalogIntrospector for OracleCatalogAdapter {
    fn source_kind(&self) -> &'static str {
        ORACLE_SOURCE
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
            ORACLE_SOURCE,
            &request.connection_alias,
            AnalysisStage::Configuration,
        )?;
        validate_request(request)?;
        validate_connection_policy(request, &self.connection_string)?;
        discover_oracle(&self.connection_string, request, cancellation)
    }
}

pub(crate) fn analyze_oracle(
    connection_string: &str,
    connection_alias: &str,
    requested_catalogs: Vec<String>,
    requested_schemas: Vec<String>,
    timeout_ms: u64,
) -> AnalysisOutcome {
    analyze_oracle_with_cancellation(
        connection_string,
        connection_alias,
        requested_catalogs,
        requested_schemas,
        timeout_ms,
        &CancellationToken::new(),
    )
}

pub(crate) fn analyze_oracle_with_cancellation(
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
    DatabaseAnalysisService::new(OracleCatalogAdapter::new(connection_string))
        .analyze_with_cancellation(&request, cancellation)
}

fn discover_oracle(
    connection_string: &str,
    request: &IntrospectionRequest,
    cancellation: &CancellationToken,
) -> Result<CatalogDiscovery, AnalysisFailure> {
    let parsed = parse_oracle_connection_string(connection_string).map_err(|error| {
        catalog_failure(
            request,
            connection_string,
            error,
            AnalysisStage::Configuration,
        )
    })?;
    let connection = Connection::connect(parsed.username, parsed.password, parsed.connect_string)
        .map_err(|error| connection_failure(request, connection_string, error))?;
    cancellation.checkpoint(
        ORACLE_SOURCE,
        &request.connection_alias,
        AnalysisStage::Connection,
    )?;
    connection
        .set_call_timeout(Some(Duration::from_millis(request.timeout_ms)))
        .map_err(|error| {
            catalog_failure(
                request,
                connection_string,
                CatalogError::Query(error),
                AnalysisStage::Connection,
            )
        })?;

    let deadline = Instant::now()
        .checked_add(Duration::from_millis(request.timeout_ms))
        .ok_or_else(|| {
            catalog_failure(
                request,
                connection_string,
                CatalogError::Timeout,
                AnalysisStage::Configuration,
            )
        })?;
    prepare_call(&connection, deadline).map_err(|error| {
        catalog_failure(request, connection_string, error, AnalysisStage::Connection)
    })?;
    connection
        .execute("SET TRANSACTION READ ONLY", &[])
        .map_err(|error| {
            catalog_failure(
                request,
                connection_string,
                CatalogError::Query(error),
                AnalysisStage::CapabilityProbe,
            )
        })?;
    cancellation.checkpoint(
        ORACLE_SOURCE,
        &request.connection_alias,
        AnalysisStage::CapabilityProbe,
    )?;

    let result =
        discover_connected(&connection, request, deadline, cancellation).map_err(|error| {
            let stage = error.stage();
            catalog_failure(request, connection_string, error, stage)
        });
    let rollback = connection.rollback().map_err(|error| {
        catalog_failure(
            request,
            connection_string,
            CatalogError::Query(error),
            AnalysisStage::CapabilityProbe,
        )
    });

    match (result, rollback) {
        (Ok(discovery), Ok(())) => Ok(discovery),
        (Err(failure), _) => Err(failure),
        (Ok(_), Err(failure)) => Err(failure),
    }
}

fn discover_connected(
    connection: &Connection,
    request: &IntrospectionRequest,
    deadline: Instant,
    cancellation: &CancellationToken,
) -> Result<CatalogDiscovery, CatalogError> {
    let facts = ServerFacts::read(connection, deadline)?;
    let strategy = OracleCatalogVersion::detect(&facts.version, &facts.release)?;
    validate_catalog_scope(request, &facts)?;
    let scope = DictionaryScope::select(connection, request, &facts, deadline)?;
    cancellation
        .checkpoint(
            ORACLE_SOURCE,
            &request.connection_alias,
            AnalysisStage::CapabilityProbe,
        )
        .map_err(CatalogError::Cancelled)?;

    let first = RawOracleCatalog::read(connection, &scope, deadline)?;
    cancellation
        .checkpoint(
            ORACLE_SOURCE,
            &request.connection_alias,
            AnalysisStage::Discovery,
        )
        .map_err(CatalogError::Cancelled)?;
    let second = RawOracleCatalog::read(connection, &scope, deadline)?;
    let stable = require_stable_catalog(first, &second)?;
    validate_raw_catalog(&stable, &scope)?;
    cancellation
        .checkpoint(
            ORACLE_SOURCE,
            &request.connection_alias,
            AnalysisStage::Mapping,
        )
        .map_err(CatalogError::Cancelled)?;

    OracleSnapshotMapper::new(&request.connection_alias, facts, strategy, scope).map(stable)
}

fn require_stable_catalog<T: PartialEq>(first: T, second: &T) -> Result<T, CatalogError> {
    if &first != second {
        return Err(CatalogError::CatalogChanged(
            "Oracle data dictionary changed while metadata was being collected".to_owned(),
        ));
    }
    Ok(first)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OracleCatalogVersion {
    Oracle26Ai,
}

impl OracleCatalogVersion {
    fn detect(version: &Version, release: &str) -> Result<Self, CatalogError> {
        if version.major() == 23 && release.to_ascii_uppercase().contains("26AI") {
            Ok(Self::Oracle26Ai)
        } else {
            Err(CatalogError::UnsupportedVersion(format!(
                "{} ({release})",
                version
            )))
        }
    }

    fn strategy_name(self) -> &'static str {
        match self {
            Self::Oracle26Ai => "oracle-26ai-dictionary-v1",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ServerFacts {
    database: String,
    container: String,
    container_id: i64,
    session_user: String,
    current_schema: String,
    version: Version,
    release: String,
}

impl ServerFacts {
    fn read(connection: &Connection, deadline: Instant) -> Result<Self, CatalogError> {
        prepare_call(connection, deadline)?;
        let (version, release) = connection.server_version()?;
        let release = normalize_server_release(&release)?;
        prepare_call(connection, deadline)?;
        let (database, container, container_id, session_user, current_schema) = connection
            .query_row_as::<(String, String, String, String, String)>(
                "
                SELECT SYS_CONTEXT('USERENV', 'DB_NAME'),
                       SYS_CONTEXT('USERENV', 'CON_NAME'),
                       SYS_CONTEXT('USERENV', 'CON_ID'),
                       SYS_CONTEXT('USERENV', 'SESSION_USER'),
                       SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA')
                FROM DUAL
                ",
                &[],
            )?;
        let container_id = container_id.parse::<i64>().map_err(|error| {
            CatalogError::Mapping(format!("invalid Oracle container id: {error}"))
        })?;
        if container.eq_ignore_ascii_case("CDB$ROOT") || container_id == 1 {
            return Err(CatalogError::InvalidScope(
                "root-container discovery is not part of the certified single-PDB contract"
                    .to_owned(),
            ));
        }
        Ok(Self {
            database,
            container,
            container_id,
            session_user,
            current_schema,
            version,
            release,
        })
    }
}

fn normalize_server_release(value: &str) -> Result<String, CatalogError> {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() || normalized.len() > MAX_DEFINITION_BYTES {
        return Err(CatalogError::Mapping(
            "Oracle server release text is empty or exceeds the safety limit".to_owned(),
        ));
    }
    Ok(normalized)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DictionaryScopeMode {
    User,
    Dba,
}

impl DictionaryScopeMode {
    fn label(self) -> &'static str {
        match self {
            Self::User => "USER_* owned-schema scope",
            Self::Dba => "DBA_* selected-schema scope",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DictionaryScope {
    mode: DictionaryScopeMode,
    owners: Vec<String>,
    principals: Vec<RawPrincipal>,
}

impl DictionaryScope {
    fn select(
        connection: &Connection,
        request: &IntrospectionRequest,
        facts: &ServerFacts,
        deadline: Instant,
    ) -> Result<Self, CatalogError> {
        let owners = normalize_requested_schemas(request, &facts.session_user)?;
        let mode = if owners.len() == 1 && owners[0] == facts.session_user {
            DictionaryScopeMode::User
        } else {
            DictionaryScopeMode::Dba
        };
        let principals = read_principals(connection, mode, &owners, deadline)?;
        if principals.len() != owners.len() {
            let found = principals
                .iter()
                .map(|principal| principal.name.as_str())
                .collect::<BTreeSet<_>>();
            let missing = owners
                .iter()
                .filter(|owner| !found.contains(owner.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            return Err(CatalogError::InvalidScope(format!(
                "requested Oracle schema owner(s) do not exist or are not visible: {}",
                missing.join(", ")
            )));
        }
        for principal in &principals {
            if principal.oracle_maintained {
                return Err(CatalogError::InvalidScope(format!(
                    "Oracle-maintained schema '{}' is outside the application-schema contract",
                    principal.name
                )));
            }
        }
        Ok(Self {
            mode,
            owners,
            principals,
        })
    }

    fn contains_owner(&self, owner: &str) -> bool {
        self.owners
            .binary_search_by(|value| value.as_str().cmp(owner))
            .is_ok()
    }
}

fn validate_request(request: &IntrospectionRequest) -> Result<(), AnalysisFailure> {
    if request.timeout_ms > MAX_INTROSPECTION_TIMEOUT_MS {
        return Err(AnalysisFailure::redacted(
            AnalysisFailureCode::InvalidConfiguration,
            AnalysisStage::Configuration,
            ORACLE_SOURCE,
            &request.connection_alias,
            format!(
                "Oracle introspection timeout {} exceeds the {} ms maximum",
                request.timeout_ms, MAX_INTROSPECTION_TIMEOUT_MS
            ),
            "use a bounded timeout of at most 86400000 milliseconds",
            false,
            None,
        ));
    }
    for value in request
        .requested_catalogs
        .iter()
        .chain(&request.requested_schemas)
    {
        if value.trim().is_empty() {
            return Err(AnalysisFailure::redacted(
                AnalysisFailureCode::InvalidConfiguration,
                AnalysisStage::Configuration,
                ORACLE_SOURCE,
                &request.connection_alias,
                "Oracle catalog and schema selectors must not be blank",
                "remove blank selectors and retry",
                false,
                None,
            ));
        }
    }
    Ok(())
}

fn validate_catalog_scope(
    request: &IntrospectionRequest,
    facts: &ServerFacts,
) -> Result<(), CatalogError> {
    if request.requested_catalogs.len() > 1 {
        return Err(CatalogError::InvalidScope(
            "an Oracle connection certifies exactly one connected PDB or non-CDB".to_owned(),
        ));
    }
    if let Some(requested) = request.requested_catalogs.first() {
        if requested != &facts.container && requested != &facts.database {
            return Err(CatalogError::InvalidScope(format!(
                "connected Oracle catalog is '{}' (database '{}'), requested '{}'",
                facts.container, facts.database, requested
            )));
        }
    }
    Ok(())
}

fn normalize_requested_schemas(
    request: &IntrospectionRequest,
    session_user: &str,
) -> Result<Vec<String>, CatalogError> {
    let mut owners = if request.requested_schemas.is_empty() {
        vec![session_user.to_owned()]
    } else {
        request.requested_schemas.clone()
    };
    owners.sort();
    if owners.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(CatalogError::InvalidScope(
            "Oracle schema selection contains duplicate owners".to_owned(),
        ));
    }
    Ok(owners)
}

struct ParsedOracleConnection<'a> {
    username: &'a str,
    password: &'a str,
    connect_string: &'a str,
}

fn parse_oracle_connection_string(value: &str) -> Result<ParsedOracleConnection<'_>, CatalogError> {
    let (username, rest) = value
        .split_once('/')
        .ok_or(CatalogError::InvalidConnectionString)?;
    let (password, connect_string) = rest
        .rsplit_once('@')
        .ok_or(CatalogError::InvalidConnectionString)?;
    if username.is_empty() || password.is_empty() || connect_string.is_empty() {
        return Err(CatalogError::InvalidConnectionString);
    }
    Ok(ParsedOracleConnection {
        username,
        password,
        connect_string,
    })
}

fn validate_connection_policy(
    request: &IntrospectionRequest,
    connection_string: &str,
) -> Result<(), AnalysisFailure> {
    let parsed = parse_oracle_connection_string(connection_string).map_err(|error| {
        catalog_failure(
            request,
            connection_string,
            error,
            AnalysisStage::Configuration,
        )
    })?;
    let connect = parsed.connect_string.trim();
    let normalized = connect.to_ascii_lowercase();
    if normalized.starts_with("tcps://") || normalized.contains("(protocol=tcps)") {
        return Ok(());
    }
    if extract_oracle_host(connect).is_some_and(is_loopback_host) {
        return Ok(());
    }
    Err(AnalysisFailure::redacted(
        AnalysisFailureCode::UnsafeSource,
        AnalysisStage::Configuration,
        ORACLE_SOURCE,
        &request.connection_alias,
        "remote Oracle metadata connections must use a verifiable TCPS endpoint",
        "use a tcps:// Easy Connect string or a descriptor with PROTOCOL=TCPS; plain TCP is allowed only for loopback test databases",
        false,
        Some(connection_string),
    ))
}

fn extract_oracle_host(connect: &str) -> Option<&str> {
    let trimmed = connect.trim();
    let normalized = trimmed.to_ascii_lowercase();
    if let Some(position) = normalized.find("(host=") {
        let start = position + "(host=".len();
        let end = trimmed[start..].find(')')? + start;
        return Some(trimmed[start..end].trim());
    }
    let easy = trimmed
        .strip_prefix("//")
        .or_else(|| trimmed.strip_prefix("tcp://"))
        .or_else(|| trimmed.strip_prefix("TCP://"))
        .unwrap_or(trimmed);
    let authority = easy.split(['/', '?']).next()?;
    if let Some(rest) = authority.strip_prefix('[') {
        return rest.split(']').next();
    }
    let (host, port) = authority.rsplit_once(':')?;
    if port.parse::<u16>().is_err() {
        return None;
    }
    Some(host)
}

fn is_loopback_host(host: &str) -> bool {
    let host = host.trim().trim_matches(['[', ']']);
    host.eq_ignore_ascii_case("localhost")
        || host == "."
        || host
            .parse::<IpAddr>()
            .map(|address| address.is_loopback())
            .unwrap_or(false)
}

fn prepare_call(connection: &Connection, deadline: Instant) -> Result<(), CatalogError> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or(CatalogError::Timeout)?;
    if remaining < Duration::from_millis(1) {
        return Err(CatalogError::Timeout);
    }
    connection.set_call_timeout(Some(remaining))?;
    Ok(())
}

#[derive(Debug)]
enum CatalogError {
    InvalidConnectionString,
    InvalidScope(String),
    UnsupportedVersion(String),
    UnsupportedMetadata(String),
    CatalogChanged(String),
    Mapping(String),
    Cancelled(AnalysisFailure),
    Timeout,
    Query(oracle::Error),
    QueryContext {
        catalog: &'static str,
        source: oracle::Error,
    },
}

impl CatalogError {
    fn stage(&self) -> AnalysisStage {
        match self {
            Self::InvalidConnectionString | Self::InvalidScope(_) => AnalysisStage::Configuration,
            Self::UnsupportedVersion(_) => AnalysisStage::CapabilityProbe,
            Self::UnsupportedMetadata(_) | Self::CatalogChanged(_) | Self::Timeout => {
                AnalysisStage::Discovery
            }
            Self::Mapping(_) => AnalysisStage::Mapping,
            Self::Cancelled(failure) => failure.stage,
            Self::Query(_) | Self::QueryContext { .. } => AnalysisStage::Discovery,
        }
    }

    fn catalog_context(self, catalog: &'static str) -> Self {
        match self {
            Self::Query(source) => Self::QueryContext { catalog, source },
            other => other,
        }
    }
}

impl From<oracle::Error> for CatalogError {
    fn from(error: oracle::Error) -> Self {
        if is_timeout_error(&error) {
            Self::Timeout
        } else {
            Self::Query(error)
        }
    }
}

fn connection_failure(
    request: &IntrospectionRequest,
    connection_string: &str,
    error: oracle::Error,
) -> AnalysisFailure {
    let authentication = error.oci_code() == Some(1017);
    AnalysisFailure::redacted(
        if authentication {
            AnalysisFailureCode::AuthenticationFailed
        } else if is_timeout_error(&error) {
            AnalysisFailureCode::Timeout
        } else {
            AnalysisFailureCode::ConnectionFailed
        },
        AnalysisStage::Connection,
        ORACLE_SOURCE,
        &request.connection_alias,
        error.to_string(),
        if authentication {
            "verify the Oracle username, password, and authentication policy"
        } else {
            "verify the Oracle listener, service name, network policy, and native client availability"
        },
        !authentication,
        Some(connection_string),
    )
}

fn catalog_failure(
    request: &IntrospectionRequest,
    connection_string: &str,
    error: CatalogError,
    stage: AnalysisStage,
) -> AnalysisFailure {
    let (code, message, remediation, retryable) = match error {
        CatalogError::InvalidConnectionString => (
            AnalysisFailureCode::InvalidConfiguration,
            "Oracle connection string must be user/password@connect_string".to_owned(),
            "provide a non-empty username, password, and Oracle connect string".to_owned(),
            false,
        ),
        CatalogError::InvalidScope(message) => (
            AnalysisFailureCode::InvalidConfiguration,
            message,
            "select the connected PDB and non-Oracle-maintained schema owners, then retry"
                .to_owned(),
            false,
        ),
        CatalogError::UnsupportedVersion(version) => (
            AnalysisFailureCode::UnsupportedVersion,
            format!("Oracle server version '{version}' has no live-certified catalog strategy"),
            "use a certified Oracle version or add and live-verify a version strategy".to_owned(),
            false,
        ),
        CatalogError::UnsupportedMetadata(message) => (
            AnalysisFailureCode::UnsupportedMetadata,
            message,
            "extend the Oracle catalog mapper for every reported object before retrying".to_owned(),
            false,
        ),
        CatalogError::CatalogChanged(message) => (
            AnalysisFailureCode::CompletenessMismatch,
            message,
            "retry after schema migrations and DDL activity have completed".to_owned(),
            true,
        ),
        CatalogError::Mapping(message) => (
            AnalysisFailureCode::MetadataMappingFailed,
            message,
            "inspect the Oracle catalog identities and fix every unresolved mapping".to_owned(),
            false,
        ),
        CatalogError::Cancelled(failure) => return failure,
        CatalogError::Timeout => (
            AnalysisFailureCode::Timeout,
            format!(
                "Oracle metadata analysis exceeded the {} ms timeout",
                request.timeout_ms
            ),
            "increase the bounded timeout or reduce the selected schema scope".to_owned(),
            true,
        ),
        CatalogError::Query(error) => {
            let timeout = is_timeout_error(&error);
            let permission = matches!(error.oci_code(), Some(942 | 1031));
            (
                if timeout {
                    AnalysisFailureCode::Timeout
                } else if permission {
                    AnalysisFailureCode::PermissionDenied
                } else {
                    AnalysisFailureCode::MetadataQueryFailed
                },
                if timeout {
                    format!(
                        "Oracle metadata analysis exceeded the {} ms timeout",
                        request.timeout_ms
                    )
                } else {
                    error.to_string()
                },
                if timeout {
                    "increase the bounded timeout or reduce the selected schema scope".to_owned()
                } else if permission {
                    "use USER_* for the session owner or grant direct/role access to every required DBA_* dictionary view"
                        .to_owned()
                } else {
                    "verify Oracle dictionary availability and retry after transient catalog errors"
                        .to_owned()
                },
                !permission,
            )
        }
        CatalogError::QueryContext { catalog, source } => {
            let timeout = is_timeout_error(&source);
            let permission = matches!(source.oci_code(), Some(942 | 1031));
            (
                if timeout {
                    AnalysisFailureCode::Timeout
                } else if permission {
                    AnalysisFailureCode::PermissionDenied
                } else {
                    AnalysisFailureCode::MetadataQueryFailed
                },
                if timeout {
                    format!(
                        "Oracle metadata analysis exceeded the {} ms timeout while reading {catalog}",
                        request.timeout_ms
                    )
                } else {
                    format!("Oracle {catalog} query failed: {source}")
                },
                if timeout {
                    "increase the bounded timeout or reduce the selected schema scope".to_owned()
                } else if permission {
                    "use USER_* for the session owner or grant direct/role access to every required DBA_* dictionary view"
                        .to_owned()
                } else {
                    "verify Oracle dictionary availability and the catalog column contract"
                        .to_owned()
                },
                !permission,
            )
        }
    };
    AnalysisFailure::redacted(
        code,
        stage,
        ORACLE_SOURCE,
        &request.connection_alias,
        message,
        remediation,
        retryable,
        Some(connection_string),
    )
}

