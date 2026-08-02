use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::net::IpAddr;
use std::ops::ControlFlow;
use std::time::Duration;

use mysql::prelude::Queryable;
use mysql::{
    AccessMode, Conn, Error as MysqlError, IsolationLevel, LocalInfileHandler, Opts, OptsBuilder,
    Row, SslOpts, TxOpts,
};
use sqlparser::ast::{visit_relations, ObjectName, ObjectNamePart, Query, Visit, Visitor};
use sqlparser::dialect::MySqlDialect;
use sqlparser::parser::Parser;

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

const MYSQL_FAMILY_SOURCE: &str = "mysql-family";
const MAX_INTROSPECTION_TIMEOUT_MS: u64 = 86_400_000;
const MAX_DEFINITION_BYTES: u64 = 1_048_576;
const ADAPTER_VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) struct MysqlFamilyCatalogAdapter {
    connection_string: String,
}

impl MysqlFamilyCatalogAdapter {
    pub(crate) fn new(connection_string: impl Into<String>) -> Self {
        Self {
            connection_string: connection_string.into(),
        }
    }
}

impl CatalogIntrospector for MysqlFamilyCatalogAdapter {
    fn source_kind(&self) -> &'static str {
        MYSQL_FAMILY_SOURCE
    }

    fn discover(
        &mut self,
        request: &IntrospectionRequest,
    ) -> Result<CatalogDiscovery, AnalysisFailure> {
        discover_mysql_family(&self.connection_string, request, &CancellationToken::new())
    }

    fn discover_with_cancellation(
        &mut self,
        request: &IntrospectionRequest,
        cancellation: &CancellationToken,
    ) -> Result<CatalogDiscovery, AnalysisFailure> {
        discover_mysql_family(&self.connection_string, request, cancellation)
    }
}

fn discover_mysql_family(
    connection_string: &str,
    request: &IntrospectionRequest,
    cancellation: &CancellationToken,
) -> Result<CatalogDiscovery, AnalysisFailure> {
    cancellation.checkpoint(
        MYSQL_FAMILY_SOURCE,
        &request.connection_alias,
        AnalysisStage::Configuration,
    )?;
    validate_request(request)?;
    let opts = secure_connection_options(request, connection_string)?;
    let mut connection = Conn::new(opts).map_err(|error| {
        classify_mysql_error(
            request,
            connection_string,
            MYSQL_FAMILY_SOURCE,
            error,
            AnalysisStage::Connection,
        )
    })?;
    cancellation.checkpoint(
        MYSQL_FAMILY_SOURCE,
        &request.connection_alias,
        AnalysisStage::Connection,
    )?;
    let facts = ServerFacts::read(&mut connection)
        .map_err(|error| catalog_failure(request, connection_string, MYSQL_FAMILY_SOURCE, error))?;
    let strategy = MysqlFamilyVersion::detect(&facts.version)
        .map_err(|error| catalog_failure(request, connection_string, facts.source_kind(), error))?;
    validate_scope(request, &facts.database).map_err(|error| {
        catalog_failure(request, connection_string, strategy.source_kind(), error)
    })?;
    configure_session(&mut connection, strategy, request.timeout_ms).map_err(|error| {
        catalog_failure(request, connection_string, strategy.source_kind(), error)
    })?;
    cancellation.checkpoint(
        strategy.source_kind(),
        &request.connection_alias,
        AnalysisStage::CapabilityProbe,
    )?;

    let tx_options = TxOpts::default()
        .set_isolation_level(Some(IsolationLevel::RepeatableRead))
        .set_access_mode(Some(AccessMode::ReadOnly))
        .set_with_consistent_snapshot(true);
    let mut transaction = connection.start_transaction(tx_options).map_err(|error| {
        classify_mysql_error(
            request,
            connection_string,
            strategy.source_kind(),
            error,
            AnalysisStage::CapabilityProbe,
        )
    })?;
    let raw = RawMysqlFamilyCatalog::read(&mut transaction, &facts, strategy).map_err(|error| {
        catalog_failure(request, connection_string, strategy.source_kind(), error)
    })?;
    cancellation.checkpoint(
        strategy.source_kind(),
        &request.connection_alias,
        AnalysisStage::Discovery,
    )?;
    let discovery = MysqlFamilySnapshotMapper::new(&request.connection_alias, strategy)
        .map(raw)
        .map_err(|error| {
            catalog_failure(request, connection_string, strategy.source_kind(), error)
        })?;
    cancellation.checkpoint(
        strategy.source_kind(),
        &request.connection_alias,
        AnalysisStage::Mapping,
    )?;
    transaction.commit().map_err(|error| {
        classify_mysql_error(
            request,
            connection_string,
            strategy.source_kind(),
            error,
            AnalysisStage::Discovery,
        )
    })?;
    Ok(discovery)
}

pub(crate) fn analyze_mysql_family(
    connection_string: &str,
    connection_alias: &str,
    requested_databases: Vec<String>,
    timeout_ms: u64,
) -> AnalysisOutcome {
    analyze_mysql_family_with_cancellation(
        connection_string,
        connection_alias,
        requested_databases,
        timeout_ms,
        &CancellationToken::new(),
    )
}

pub(crate) fn analyze_mysql_family_with_cancellation(
    connection_string: &str,
    connection_alias: &str,
    requested_databases: Vec<String>,
    timeout_ms: u64,
    cancellation: &CancellationToken,
) -> AnalysisOutcome {
    let request = IntrospectionRequest {
        connection_alias: connection_alias.to_owned(),
        requested_catalogs: requested_databases.clone(),
        requested_schemas: requested_databases,
        timeout_ms,
    };
    DatabaseAnalysisService::new(MysqlFamilyCatalogAdapter::new(connection_string))
        .analyze_with_cancellation(&request, cancellation)
}

fn validate_request(request: &IntrospectionRequest) -> Result<(), AnalysisFailure> {
    if request.timeout_ms > MAX_INTROSPECTION_TIMEOUT_MS {
        return Err(AnalysisFailure::redacted(
            AnalysisFailureCode::InvalidConfiguration,
            AnalysisStage::Configuration,
            MYSQL_FAMILY_SOURCE,
            &request.connection_alias,
            format!(
                "MySQL-family introspection timeout exceeds the {MAX_INTROSPECTION_TIMEOUT_MS} ms safety limit"
            ),
            "choose a timeout between 1 ms and 86400000 ms",
            false,
            None,
        ));
    }
    let duplicate_catalogs = request.requested_catalogs.len()
        != request
            .requested_catalogs
            .iter()
            .collect::<BTreeSet<_>>()
            .len();
    let duplicate_schemas = request.requested_schemas.len()
        != request
            .requested_schemas
            .iter()
            .collect::<BTreeSet<_>>()
            .len();
    if duplicate_catalogs || duplicate_schemas {
        return Err(AnalysisFailure::redacted(
            AnalysisFailureCode::InvalidConfiguration,
            AnalysisStage::Configuration,
            MYSQL_FAMILY_SOURCE,
            &request.connection_alias,
            "MySQL-family scope contains duplicate database names",
            "provide each requested database exactly once",
            false,
            None,
        ));
    }
    Ok(())
}

fn secure_connection_options(
    request: &IntrospectionRequest,
    connection_string: &str,
) -> Result<Opts, AnalysisFailure> {
    let parsed = Opts::from_url(connection_string).map_err(|error| {
        AnalysisFailure::redacted(
            AnalysisFailureCode::InvalidConfiguration,
            AnalysisStage::Configuration,
            MYSQL_FAMILY_SOURCE,
            &request.connection_alias,
            error.to_string(),
            "provide a valid mysql:// connection URL selecting one database",
            false,
            Some(connection_string),
        )
    })?;
    if parsed.get_db_name().is_none_or(str::is_empty) {
        return Err(AnalysisFailure::redacted(
            AnalysisFailureCode::InvalidConfiguration,
            AnalysisStage::Configuration,
            MYSQL_FAMILY_SOURCE,
            &request.connection_alias,
            "MySQL-family connection URL must select one database",
            "append the database name to the connection URL path",
            false,
            Some(connection_string),
        ));
    }
    if parsed.get_enable_cleartext_plugin() {
        return Err(AnalysisFailure::redacted(
            AnalysisFailureCode::UnsafeSource,
            AnalysisStage::Configuration,
            MYSQL_FAMILY_SOURCE,
            &request.connection_alias,
            "mysql_clear_password authentication is disabled by the metadata reader policy",
            "use a challenge-response authentication plugin over a verified TLS connection",
            false,
            Some(connection_string),
        ));
    }
    if let Some(ssl) = parsed.get_ssl_opts() {
        if ssl.accept_invalid_certs() || ssl.skip_domain_validation() {
            return Err(AnalysisFailure::redacted(
                AnalysisFailureCode::UnsafeSource,
                AnalysisStage::Configuration,
                MYSQL_FAMILY_SOURCE,
                &request.connection_alias,
                "TLS certificate or hostname verification cannot be disabled",
                "use a certificate trusted by the operating system or configure a trusted CA",
                false,
                Some(connection_string),
            ));
        }
    }

    let host = parsed.get_ip_or_hostname().into_owned();
    let remote_tcp = parsed.get_socket().is_none() && !is_loopback_host(&host);
    let timeout = Duration::from_millis(request.timeout_ms);
    let mut builder = OptsBuilder::from_opts(parsed)
        .tcp_connect_timeout(Some(timeout))
        .read_timeout(Some(timeout))
        .write_timeout(Some(timeout))
        .local_infile_handler(Some(LocalInfileHandler::new(|_, _| {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "LOCAL INFILE is disabled for metadata introspection",
            ))
        })));
    if remote_tcp {
        builder = builder.ssl_opts(Some(SslOpts::default()));
    }
    Ok(Opts::from(builder))
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .map(|address| address.is_loopback())
            .unwrap_or(false)
}

fn validate_scope(request: &IntrospectionRequest, database: &str) -> Result<(), CatalogError> {
    let requested = request
        .requested_catalogs
        .iter()
        .chain(&request.requested_schemas)
        .collect::<BTreeSet<_>>();
    if requested.is_empty() || (requested.len() == 1 && requested.contains(&database.to_owned())) {
        return Ok(());
    }
    Err(CatalogError::InvalidScope(format!(
        "the connection selects database '{database}', but the requested scope is {}",
        requested
            .into_iter()
            .map(|name| format!("'{name}'"))
            .collect::<Vec<_>>()
            .join(", ")
    )))
}

fn configure_session<Q: Queryable>(
    connection: &mut Q,
    strategy: MysqlFamilyVersion,
    timeout_ms: u64,
) -> Result<(), CatalogError> {
    match strategy.product() {
        MysqlProduct::Mysql => {
            connection.query_drop(format!("SET SESSION MAX_EXECUTION_TIME = {timeout_ms}"))?
        }
        MysqlProduct::MariaDb => {
            let seconds = timeout_ms as f64 / 1_000.0;
            connection.query_drop(format!("SET SESSION max_statement_time = {seconds}"))?;
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MysqlProduct {
    Mysql,
    MariaDb,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MysqlFamilyVersion {
    Mysql80,
    Mysql84,
    Mysql97,
    MariaDb1011,
    MariaDb114,
    MariaDb118,
    MariaDb123,
}

impl MysqlFamilyVersion {
    fn detect(version: &str) -> Result<Self, CatalogError> {
        let maria_db = version.to_ascii_lowercase().contains("mariadb");
        let numeric = version
            .split(['-', '+'])
            .next()
            .unwrap_or(version)
            .split('.')
            .take(2)
            .map(|part| part.parse::<u32>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| CatalogError::UnsupportedVersion(version.to_owned()))?;
        if numeric.len() != 2 {
            return Err(CatalogError::UnsupportedVersion(version.to_owned()));
        }
        match (maria_db, numeric[0], numeric[1]) {
            (false, 8, 0) => Ok(Self::Mysql80),
            (false, 8, 4) => Ok(Self::Mysql84),
            (false, 9, 7) => Ok(Self::Mysql97),
            (true, 10, 11) => Ok(Self::MariaDb1011),
            (true, 11, 4) => Ok(Self::MariaDb114),
            (true, 11, 8) => Ok(Self::MariaDb118),
            (true, 12, 3) => Ok(Self::MariaDb123),
            _ => Err(CatalogError::UnsupportedVersion(version.to_owned())),
        }
    }

    fn product(self) -> MysqlProduct {
        match self {
            Self::Mysql80 | Self::Mysql84 | Self::Mysql97 => MysqlProduct::Mysql,
            Self::MariaDb1011 | Self::MariaDb114 | Self::MariaDb118 | Self::MariaDb123 => {
                MysqlProduct::MariaDb
            }
        }
    }

    fn source_kind(self) -> &'static str {
        match self.product() {
            MysqlProduct::Mysql => "mysql",
            MysqlProduct::MariaDb => "mariadb",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Mysql80 => "mysql-8.0",
            Self::Mysql84 => "mysql-8.4",
            Self::Mysql97 => "mysql-9.7",
            Self::MariaDb1011 => "mariadb-10.11",
            Self::MariaDb114 => "mariadb-11.4",
            Self::MariaDb118 => "mariadb-11.8",
            Self::MariaDb123 => "mariadb-12.3",
        }
    }

    fn signature_queries(self) -> &'static [&'static str] {
        match self.product() {
            MysqlProduct::Mysql => MYSQL_SIGNATURE_QUERIES,
            MysqlProduct::MariaDb => MARIADB_SIGNATURE_QUERIES,
        }
    }
}

const COMMON_SIGNATURE_QUERIES: &[&str] = &[
    "SELECT CONCAT('table:', SHA2(CONCAT_WS(CHAR(31), TABLE_NAME, TABLE_TYPE, \
        COALESCE(ENGINE, '<null>'), COALESCE(ROW_FORMAT, '<null>'), \
        COALESCE(TABLE_COLLATION, '<null>'), COALESCE(CREATE_OPTIONS, '<null>'), \
        COALESCE(TABLE_COMMENT, '<null>')), 256)) AS signature \
     FROM INFORMATION_SCHEMA.TABLES WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME",
    "SELECT CONCAT('key:', SHA2(CONCAT_WS(CHAR(31), TABLE_NAME, CONSTRAINT_NAME, \
        COLUMN_NAME, ORDINAL_POSITION, COALESCE(POSITION_IN_UNIQUE_CONSTRAINT, '<null>'), \
        COALESCE(REFERENCED_TABLE_SCHEMA, '<null>'), COALESCE(REFERENCED_TABLE_NAME, '<null>'), \
        COALESCE(REFERENCED_COLUMN_NAME, '<null>')), 256)) AS signature \
     FROM INFORMATION_SCHEMA.KEY_COLUMN_USAGE WHERE TABLE_SCHEMA = ? \
     ORDER BY TABLE_NAME, CONSTRAINT_NAME, ORDINAL_POSITION",
    "SELECT CONCAT('reference:', SHA2(CONCAT_WS(CHAR(31), TABLE_NAME, CONSTRAINT_NAME, \
        MATCH_OPTION, UPDATE_RULE, DELETE_RULE, COALESCE(REFERENCED_TABLE_NAME, '<null>')), 256)) \
        AS signature FROM INFORMATION_SCHEMA.REFERENTIAL_CONSTRAINTS \
     WHERE CONSTRAINT_SCHEMA = ? ORDER BY TABLE_NAME, CONSTRAINT_NAME",
    "SELECT CONCAT('routine:', SHA2(CONCAT_WS(CHAR(31), SPECIFIC_NAME, ROUTINE_NAME, \
        ROUTINE_TYPE, COALESCE(DTD_IDENTIFIER, '<null>'), COALESCE(ROUTINE_DEFINITION, '<null>'), \
        IS_DETERMINISTIC, SQL_DATA_ACCESS, SECURITY_TYPE, SQL_MODE, COALESCE(ROUTINE_COMMENT, '<null>'), \
        DEFINER), 256)) AS signature FROM INFORMATION_SCHEMA.ROUTINES \
     WHERE ROUTINE_SCHEMA = ? ORDER BY SPECIFIC_NAME",
    "SELECT CONCAT('parameter:', SHA2(CONCAT_WS(CHAR(31), SPECIFIC_NAME, ORDINAL_POSITION, \
        COALESCE(PARAMETER_MODE, '<null>'), COALESCE(PARAMETER_NAME, '<null>'), DATA_TYPE, \
        COALESCE(DTD_IDENTIFIER, '<null>'), ROUTINE_TYPE), 256)) AS signature \
     FROM INFORMATION_SCHEMA.PARAMETERS WHERE SPECIFIC_SCHEMA = ? \
     ORDER BY SPECIFIC_NAME, ORDINAL_POSITION",
    "SELECT CONCAT('trigger:', SHA2(CONCAT_WS(CHAR(31), TRIGGER_NAME, EVENT_MANIPULATION, \
        EVENT_OBJECT_TABLE, ACTION_ORDER, COALESCE(ACTION_CONDITION, '<null>'), \
        COALESCE(ACTION_STATEMENT, '<null>'), ACTION_ORIENTATION, ACTION_TIMING, SQL_MODE, DEFINER), 256)) \
        AS signature FROM INFORMATION_SCHEMA.TRIGGERS WHERE TRIGGER_SCHEMA = ? \
     ORDER BY TRIGGER_NAME",
    "SELECT CONCAT('event:', SHA2(CONCAT_WS(CHAR(31), EVENT_NAME, DEFINER, TIME_ZONE, EVENT_BODY, \
        COALESCE(EVENT_DEFINITION, '<null>'), EVENT_TYPE, COALESCE(CAST(EXECUTE_AT AS CHAR), '<null>'), \
        COALESCE(INTERVAL_VALUE, '<null>'), COALESCE(INTERVAL_FIELD, '<null>'), SQL_MODE, \
        COALESCE(CAST(STARTS AS CHAR), '<null>'), COALESCE(CAST(ENDS AS CHAR), '<null>'), STATUS, \
        ON_COMPLETION, COALESCE(EVENT_COMMENT, '<null>')), 256)) AS signature \
     FROM INFORMATION_SCHEMA.EVENTS WHERE EVENT_SCHEMA = ? ORDER BY EVENT_NAME",
    "SELECT CONCAT('partition:', SHA2(CONCAT_WS(CHAR(31), TABLE_NAME, PARTITION_NAME, \
        COALESCE(SUBPARTITION_NAME, '<null>'), PARTITION_ORDINAL_POSITION, \
        COALESCE(SUBPARTITION_ORDINAL_POSITION, '<null>'), COALESCE(PARTITION_METHOD, '<null>'), \
        COALESCE(SUBPARTITION_METHOD, '<null>'), COALESCE(PARTITION_EXPRESSION, '<null>'), \
        COALESCE(SUBPARTITION_EXPRESSION, '<null>'), COALESCE(PARTITION_DESCRIPTION, '<null>')), 256)) \
        AS signature FROM INFORMATION_SCHEMA.PARTITIONS \
     WHERE TABLE_SCHEMA = ? AND PARTITION_NAME IS NOT NULL \
     ORDER BY TABLE_NAME, PARTITION_ORDINAL_POSITION, SUBPARTITION_ORDINAL_POSITION",
];

const MYSQL_SIGNATURE_QUERIES: &[&str] = &[
    COMMON_SIGNATURE_QUERIES[0],
    "SELECT CONCAT('column:', SHA2(CONCAT_WS(CHAR(31), TABLE_NAME, COLUMN_NAME, ORDINAL_POSITION, \
        COALESCE(COLUMN_DEFAULT, '<null>'), IS_NULLABLE, DATA_TYPE, COLUMN_TYPE, \
        COALESCE(CHARACTER_SET_NAME, '<null>'), COALESCE(COLLATION_NAME, '<null>'), COLUMN_KEY, \
        EXTRA, PRIVILEGES, COALESCE(COLUMN_COMMENT, '<null>'), \
        COALESCE(GENERATION_EXPRESSION, '<null>'), COALESCE(SRS_ID, '<null>')), 256)) AS signature \
     FROM INFORMATION_SCHEMA.COLUMNS WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME, ORDINAL_POSITION",
    "SELECT CONCAT('constraint:', SHA2(CONCAT_WS(CHAR(31), TABLE_NAME, CONSTRAINT_NAME, \
        CONSTRAINT_TYPE, ENFORCED), 256)) AS signature FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS \
     WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME, CONSTRAINT_NAME",
    COMMON_SIGNATURE_QUERIES[1],
    COMMON_SIGNATURE_QUERIES[2],
    "SELECT CONCAT('check:', SHA2(CONCAT_WS(CHAR(31), tc.TABLE_NAME, cc.CONSTRAINT_NAME, \
        cc.CHECK_CLAUSE), 256)) AS signature FROM INFORMATION_SCHEMA.CHECK_CONSTRAINTS cc \
     JOIN INFORMATION_SCHEMA.TABLE_CONSTRAINTS tc ON tc.CONSTRAINT_SCHEMA = cc.CONSTRAINT_SCHEMA \
       AND tc.CONSTRAINT_NAME = cc.CONSTRAINT_NAME AND tc.CONSTRAINT_TYPE = 'CHECK' \
     WHERE cc.CONSTRAINT_SCHEMA = ? ORDER BY tc.TABLE_NAME, cc.CONSTRAINT_NAME",
    "SELECT CONCAT('index:', SHA2(CONCAT_WS(CHAR(31), TABLE_NAME, INDEX_NAME, NON_UNIQUE, \
        SEQ_IN_INDEX, COALESCE(COLUMN_NAME, '<null>'), COALESCE(COLLATION, '<null>'), \
        COALESCE(SUB_PART, '<null>'), INDEX_TYPE, COMMENT, INDEX_COMMENT, IS_VISIBLE, \
        COALESCE(EXPRESSION, '<null>')), 256)) AS signature FROM INFORMATION_SCHEMA.STATISTICS \
     WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME, INDEX_NAME, SEQ_IN_INDEX",
    "SELECT CONCAT('view:', SHA2(CONCAT_WS(CHAR(31), TABLE_NAME, COALESCE(VIEW_DEFINITION, '<null>'), \
        CHECK_OPTION, IS_UPDATABLE, DEFINER, SECURITY_TYPE, CHARACTER_SET_CLIENT, \
        COLLATION_CONNECTION), 256)) AS signature FROM INFORMATION_SCHEMA.VIEWS \
     WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME",
    "SELECT CONCAT('view-table:', SHA2(CONCAT_WS(CHAR(31), VIEW_NAME, TABLE_SCHEMA, TABLE_NAME), 256)) \
        AS signature FROM INFORMATION_SCHEMA.VIEW_TABLE_USAGE WHERE VIEW_SCHEMA = ? \
     ORDER BY VIEW_NAME, TABLE_SCHEMA, TABLE_NAME",
    "SELECT CONCAT('view-routine:', SHA2(CONCAT_WS(CHAR(31), TABLE_NAME, SPECIFIC_SCHEMA, \
        SPECIFIC_NAME), 256)) AS signature FROM INFORMATION_SCHEMA.VIEW_ROUTINE_USAGE \
     WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME, SPECIFIC_SCHEMA, SPECIFIC_NAME",
    COMMON_SIGNATURE_QUERIES[3],
    COMMON_SIGNATURE_QUERIES[4],
    COMMON_SIGNATURE_QUERIES[5],
    COMMON_SIGNATURE_QUERIES[6],
    COMMON_SIGNATURE_QUERIES[7],
];

const MARIADB_SIGNATURE_QUERIES: &[&str] = &[
    COMMON_SIGNATURE_QUERIES[0],
    "SELECT CONCAT('column:', SHA2(CONCAT_WS(CHAR(31), TABLE_NAME, COLUMN_NAME, ORDINAL_POSITION, \
        COALESCE(COLUMN_DEFAULT, '<null>'), IS_NULLABLE, DATA_TYPE, COLUMN_TYPE, \
        COALESCE(CHARACTER_SET_NAME, '<null>'), COALESCE(COLLATION_NAME, '<null>'), COLUMN_KEY, \
        EXTRA, PRIVILEGES, COALESCE(COLUMN_COMMENT, '<null>'), IS_GENERATED, \
        COALESCE(GENERATION_EXPRESSION, '<null>')), 256)) AS signature \
     FROM INFORMATION_SCHEMA.COLUMNS WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME, ORDINAL_POSITION",
    "SELECT CONCAT('constraint:', SHA2(CONCAT_WS(CHAR(31), TABLE_NAME, CONSTRAINT_NAME, \
        CONSTRAINT_TYPE), 256)) AS signature FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS \
     WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME, CONSTRAINT_NAME",
    COMMON_SIGNATURE_QUERIES[1],
    COMMON_SIGNATURE_QUERIES[2],
    "SELECT CONCAT('check:', SHA2(CONCAT_WS(CHAR(31), TABLE_NAME, CONSTRAINT_NAME, CHECK_CLAUSE), 256)) \
        AS signature FROM INFORMATION_SCHEMA.CHECK_CONSTRAINTS WHERE CONSTRAINT_SCHEMA = ? \
     ORDER BY TABLE_NAME, CONSTRAINT_NAME",
    "SELECT CONCAT('index:', SHA2(CONCAT_WS(CHAR(31), TABLE_NAME, INDEX_NAME, NON_UNIQUE, \
        SEQ_IN_INDEX, COALESCE(COLUMN_NAME, '<null>'), COALESCE(COLLATION, '<null>'), \
        COALESCE(SUB_PART, '<null>'), INDEX_TYPE, COMMENT, INDEX_COMMENT, IGNORED), 256)) AS signature \
     FROM INFORMATION_SCHEMA.STATISTICS WHERE TABLE_SCHEMA = ? \
     ORDER BY TABLE_NAME, INDEX_NAME, SEQ_IN_INDEX",
    "SELECT CONCAT('view:', SHA2(CONCAT_WS(CHAR(31), TABLE_NAME, COALESCE(VIEW_DEFINITION, '<null>'), \
        CHECK_OPTION, IS_UPDATABLE, DEFINER, SECURITY_TYPE, CHARACTER_SET_CLIENT, \
        COLLATION_CONNECTION, ALGORITHM), 256)) AS signature FROM INFORMATION_SCHEMA.VIEWS \
     WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME",
    COMMON_SIGNATURE_QUERIES[3],
    COMMON_SIGNATURE_QUERIES[4],
    COMMON_SIGNATURE_QUERIES[5],
    COMMON_SIGNATURE_QUERIES[6],
    COMMON_SIGNATURE_QUERIES[7],
];

#[derive(Clone, Debug)]
struct ServerFacts {
    database: String,
    version: String,
    version_comment: String,
    current_user: String,
    session_user: String,
    lower_case_table_names: u64,
    tls_cipher: Option<String>,
}

impl ServerFacts {
    fn read<Q: Queryable>(connection: &mut Q) -> Result<Self, CatalogError> {
        let row = connection
            .query_first::<Row, _>(
                "SELECT DATABASE() AS database_name, VERSION() AS version_value, \
                        @@version_comment AS version_comment, CURRENT_USER() AS current_user_value, \
                        USER() AS session_user_value, @@lower_case_table_names AS lower_case_table_names",
            )?
            .ok_or_else(|| CatalogError::Mapping("server identity query returned no row".into()))?;
        let tls_cipher = connection
            .query_first::<Row, _>("SHOW SESSION STATUS LIKE 'Ssl_cipher'")?
            .map(|row| optional_at::<String>(&row, 1))
            .transpose()?
            .flatten()
            .filter(|value| !value.is_empty());
        Ok(Self {
            database: required(&row, "database_name")?,
            version: required(&row, "version_value")?,
            version_comment: required(&row, "version_comment")?,
            current_user: required(&row, "current_user_value")?,
            session_user: required(&row, "session_user_value")?,
            lower_case_table_names: required(&row, "lower_case_table_names")?,
            tls_cipher,
        })
    }

    fn source_kind(&self) -> &'static str {
        if self.version.to_ascii_lowercase().contains("mariadb") {
            "mariadb"
        } else {
            "mysql"
        }
    }
}

#[derive(Debug)]
enum CatalogError {
    Query(MysqlError),
    InvalidScope(String),
    PermissionDenied(String),
    UnsupportedVersion(String),
    UnsupportedMetadata(String),
    ConcurrentDdl(String),
    Mapping(String),
}

impl From<MysqlError> for CatalogError {
    fn from(error: MysqlError) -> Self {
        Self::Query(error)
    }
}

fn classify_mysql_error(
    request: &IntrospectionRequest,
    connection_string: &str,
    source_kind: &str,
    error: MysqlError,
    stage: AnalysisStage,
) -> AnalysisFailure {
    let (code, retryable, remediation) = match &error {
        MysqlError::MySqlError(server) if server.code == 1045 => (
            AnalysisFailureCode::AuthenticationFailed,
            false,
            "verify the MySQL-family principal and secret",
        ),
        MysqlError::MySqlError(server) if matches!(server.code, 1044 | 1142 | 1227) => (
            AnalysisFailureCode::PermissionDenied,
            false,
            "grant schema-wide metadata visibility and retry",
        ),
        MysqlError::MySqlError(server) if matches!(server.code, 1317 | 3024) => (
            AnalysisFailureCode::Timeout,
            true,
            "increase the bounded timeout or reduce the selected database scope",
        ),
        _ if stage == AnalysisStage::Connection => (
            AnalysisFailureCode::ConnectionFailed,
            true,
            "verify the MySQL-family endpoint, TLS trust, and network path",
        ),
        _ => (
            AnalysisFailureCode::MetadataQueryFailed,
            true,
            "inspect the server state and retry the metadata-only analysis",
        ),
    };
    AnalysisFailure::redacted(
        code,
        stage,
        source_kind,
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
    source_kind: &str,
    error: CatalogError,
) -> AnalysisFailure {
    match error {
        CatalogError::Query(error) => classify_mysql_error(
            request,
            connection_string,
            source_kind,
            error,
            AnalysisStage::Discovery,
        ),
        CatalogError::InvalidScope(message) => AnalysisFailure::redacted(
            AnalysisFailureCode::InvalidConfiguration,
            AnalysisStage::CapabilityProbe,
            source_kind,
            &request.connection_alias,
            message,
            "select and request exactly one current MySQL-family database",
            false,
            Some(connection_string),
        ),
        CatalogError::PermissionDenied(message) => AnalysisFailure::redacted(
            AnalysisFailureCode::PermissionDenied,
            AnalysisStage::CapabilityProbe,
            source_kind,
            &request.connection_alias,
            message,
            "grant SELECT, SHOW VIEW, EXECUTE, EVENT, and TRIGGER for the selected database",
            false,
            Some(connection_string),
        ),
        CatalogError::UnsupportedVersion(version) => AnalysisFailure::redacted(
            AnalysisFailureCode::UnsupportedVersion,
            AnalysisStage::CapabilityProbe,
            source_kind,
            &request.connection_alias,
            format!("server version '{version}' has no certified MySQL-family strategy"),
            "use MySQL 8.0, 8.4, or 9.7, or MariaDB 10.11, 11.4, 11.8, or 12.3",
            false,
            Some(connection_string),
        ),
        CatalogError::UnsupportedMetadata(message) => AnalysisFailure::redacted(
            AnalysisFailureCode::UnsupportedMetadata,
            AnalysisStage::CapabilityProbe,
            source_kind,
            &request.connection_alias,
            message,
            "remove the unprovable construct or extend and verify the product strategy",
            false,
            Some(connection_string),
        ),
        CatalogError::ConcurrentDdl(message) => AnalysisFailure::redacted(
            AnalysisFailureCode::CompletenessMismatch,
            AnalysisStage::Validation,
            source_kind,
            &request.connection_alias,
            message,
            "retry after concurrent DDL activity has finished",
            true,
            Some(connection_string),
        ),
        CatalogError::Mapping(message) => AnalysisFailure::redacted(
            AnalysisFailureCode::MetadataMappingFailed,
            AnalysisStage::Mapping,
            source_kind,
            &request.connection_alias,
            message,
            "fix every unresolved catalog mapping before retrying",
            false,
            Some(connection_string),
        ),
    }
}

