use std::collections::BTreeSet;
use std::net::IpAddr;
#[cfg(feature = "odbc")]
use std::time::{Duration, Instant};

use connection_string::AdoNetString;
use serde::{Deserialize, Serialize};

use crate::analysis_outcome::{
    AnalysisFailure, AnalysisFailureCode, AnalysisOutcome, AnalysisStage,
};
use crate::introspection::{CancellationToken, IntrospectionRequest};
use crate::redact::redact_connection_string;

const ODBC_SOURCE: &str = "odbc";
#[cfg(feature = "odbc")]
const ODBC_PROBE_CONTRACT_VERSION: u32 = 1;
const MAX_INTROSPECTION_TIMEOUT_MS: u64 = 86_400_000;
const MAX_SCOPE_VALUE_BYTES: usize = 1_024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OdbcCapabilityReport {
    pub contract_version: u32,
    pub source_kind: String,
    pub connection_alias: String,
    pub driver: OdbcDriverIdentity,
    pub server: OdbcServerIdentity,
    pub current_catalog: Option<String>,
    pub metadata_functions_only: bool,
    pub read_only_access_mode: bool,
    pub data_source_read_only: bool,
    pub transaction_capability: OdbcTransactionCapability,
    pub catalog_functions: Vec<OdbcCatalogFunctionCapability>,
    pub completeness: OdbcCompletenessAssessment,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OdbcDriverIdentity {
    pub name: String,
    pub version: String,
    pub odbc_version: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OdbcServerIdentity {
    pub product: String,
    pub version: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OdbcTransactionCapability {
    None,
    DmlOnly,
    DdlAndDml,
    DdlCommits,
    DdlIgnored,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OdbcCatalogFunction {
    Tables,
    Columns,
    Statistics,
    SpecialColumns,
    PrimaryKeys,
    ForeignKeys,
    TablePrivileges,
    ColumnPrivileges,
    Procedures,
    ProcedureColumns,
    TypeInfo,
}

impl OdbcCatalogFunction {
    pub const ALL: [Self; 11] = [
        Self::Tables,
        Self::Columns,
        Self::Statistics,
        Self::SpecialColumns,
        Self::PrimaryKeys,
        Self::ForeignKeys,
        Self::TablePrivileges,
        Self::ColumnPrivileges,
        Self::Procedures,
        Self::ProcedureColumns,
        Self::TypeInfo,
    ];

    #[cfg(feature = "odbc")]
    const fn function_id(self) -> u16 {
        match self {
            Self::Columns => 40,
            Self::TypeInfo => 47,
            Self::SpecialColumns => 52,
            Self::Statistics => 53,
            Self::Tables => 54,
            Self::ColumnPrivileges => 56,
            Self::ForeignKeys => 60,
            Self::PrimaryKeys => 65,
            Self::ProcedureColumns => 66,
            Self::Procedures => 67,
            Self::TablePrivileges => 70,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OdbcCatalogFunctionCapability {
    pub function: OdbcCatalogFunction,
    pub support: OdbcCatalogFunctionSupport,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OdbcCatalogFunctionSupport {
    NotSupported,
    DriverDeclared,
    RuntimeCallVerified,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum OdbcCompletenessAssessment {
    Rejected { blockers: Vec<String> },
    Eligible { strategy: String },
}

pub const fn odbc_runtime_available() -> bool {
    cfg!(feature = "odbc")
}

pub fn probe_odbc_capabilities(
    connection_string: &str,
    connection_alias: &str,
    timeout_ms: u64,
) -> Result<OdbcCapabilityReport, AnalysisFailure> {
    probe_odbc_capabilities_with_cancellation(
        connection_string,
        connection_alias,
        timeout_ms,
        &CancellationToken::new(),
    )
}

pub fn probe_odbc_capabilities_with_cancellation(
    connection_string: &str,
    connection_alias: &str,
    timeout_ms: u64,
    cancellation: &CancellationToken,
) -> Result<OdbcCapabilityReport, AnalysisFailure> {
    let request = IntrospectionRequest {
        connection_alias: connection_alias.to_owned(),
        requested_catalogs: Vec::new(),
        requested_schemas: Vec::new(),
        timeout_ms,
    };
    probe_request(connection_string, &request, cancellation)
}

pub fn introspect_odbc_complete(
    connection_string: &str,
    connection_alias: &str,
) -> AnalysisOutcome {
    introspect_odbc_complete_scoped(
        connection_string,
        connection_alias,
        Vec::new(),
        Vec::new(),
        30_000,
    )
}

pub fn introspect_odbc_complete_scoped(
    connection_string: &str,
    connection_alias: &str,
    requested_catalogs: Vec<String>,
    requested_schemas: Vec<String>,
    timeout_ms: u64,
) -> AnalysisOutcome {
    introspect_odbc_complete_scoped_with_cancellation(
        connection_string,
        connection_alias,
        requested_catalogs,
        requested_schemas,
        timeout_ms,
        &CancellationToken::new(),
    )
}

pub fn introspect_odbc_complete_scoped_with_cancellation(
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
    #[cfg(feature = "odbc")]
    let started = Instant::now();
    let report = match probe_request(connection_string, &request, cancellation) {
        Ok(report) => report,
        Err(failure) => return AnalysisOutcome::failed(failure),
    };
    #[cfg(feature = "odbc")]
    {
        let Some(remaining) =
            Duration::from_millis(request.timeout_ms).checked_sub(started.elapsed())
        else {
            return AnalysisOutcome::failed(AnalysisFailure::redacted(
                AnalysisFailureCode::Timeout,
                AnalysisStage::CapabilityProbe,
                ODBC_SOURCE,
                &request.connection_alias,
                "ODBC capability negotiation exhausted the introspection deadline",
                "increase the bounded timeout or inspect driver and network latency",
                true,
                Some(connection_string),
            ));
        };
        let remaining_ms = u64::try_from(remaining.as_millis()).unwrap_or(u64::MAX);
        if remaining_ms == 0 {
            return AnalysisOutcome::failed(AnalysisFailure::redacted(
                AnalysisFailureCode::Timeout,
                AnalysisStage::CapabilityProbe,
                ODBC_SOURCE,
                &request.connection_alias,
                "ODBC capability negotiation left no time for authoritative discovery",
                "increase the bounded timeout or inspect driver and network latency",
                true,
                Some(connection_string),
            ));
        }
        let mut strategy_request = request.clone();
        strategy_request.timeout_ms = remaining_ms;
        if let Some(outcome) = runtime::analyze_with_registered_strategy(
            connection_string,
            &strategy_request,
            cancellation,
            &report,
        ) {
            return outcome;
        }
    }
    AnalysisOutcome::failed(AnalysisFailure::redacted(
        AnalysisFailureCode::UnsupportedProduct,
        AnalysisStage::CapabilityProbe,
        ODBC_SOURCE,
        &request.connection_alias,
        format!(
            "ODBC connected to {} {}, but no live-certified product strategy matches this source",
            report.server.product, report.server.version
        ),
        "use a certified native adapter or add a product-specific ODBC strategy with live completeness evidence",
        false,
        Some(connection_string),
    ))
}

fn probe_request(
    connection_string: &str,
    request: &IntrospectionRequest,
    cancellation: &CancellationToken,
) -> Result<OdbcCapabilityReport, AnalysisFailure> {
    validate_request(request, connection_string)?;
    cancellation.checkpoint(
        ODBC_SOURCE,
        &request.connection_alias,
        AnalysisStage::Configuration,
    )?;
    validate_connection_policy(request, connection_string)?;

    #[cfg(feature = "odbc")]
    {
        runtime::probe(connection_string, request, cancellation)
    }
    #[cfg(not(feature = "odbc"))]
    {
        let _ = cancellation;
        Err(AnalysisFailure::redacted(
            AnalysisFailureCode::DriverUnavailable,
            AnalysisStage::Configuration,
            ODBC_SOURCE,
            &request.connection_alias,
            "this database-memory build does not include the optional ODBC runtime",
            "build database-memory-core with the 'odbc' feature and install a matching 64-bit ODBC driver",
            false,
            Some(connection_string),
        ))
    }
}

fn validate_request(
    request: &IntrospectionRequest,
    connection_string: &str,
) -> Result<(), AnalysisFailure> {
    request.validate(ODBC_SOURCE)?;
    if request.connection_alias.len() > MAX_SCOPE_VALUE_BYTES
        || redact_connection_string(&request.connection_alias) != request.connection_alias
    {
        return Err(configuration_failure(
            request,
            connection_string,
            "ODBC connection alias must be a bounded non-secret label",
            "use a short logical alias that contains no credentials or connection string",
        ));
    }
    if request.timeout_ms > MAX_INTROSPECTION_TIMEOUT_MS {
        return Err(configuration_failure(
            request,
            connection_string,
            format!(
                "ODBC introspection timeout exceeds the {MAX_INTROSPECTION_TIMEOUT_MS} ms safety limit"
            ),
            "choose a timeout between 1 ms and 86400000 ms",
        ));
    }
    if connection_string.trim().is_empty() || connection_string.contains('\0') {
        return Err(configuration_failure(
            request,
            connection_string,
            "ODBC connection string must be non-empty and contain no NUL bytes",
            "provide a non-secret alias and a valid ODBC connection string",
        ));
    }
    if has_duplicates(&request.requested_catalogs) || has_duplicates(&request.requested_schemas) {
        return Err(configuration_failure(
            request,
            connection_string,
            "ODBC scope contains duplicate catalog or schema names",
            "provide each requested catalog and schema exactly once",
        ));
    }
    if request
        .requested_catalogs
        .iter()
        .chain(&request.requested_schemas)
        .any(|value| value.trim().is_empty() || value.len() > MAX_SCOPE_VALUE_BYTES)
    {
        return Err(configuration_failure(
            request,
            connection_string,
            "ODBC scope values must be non-empty and at most 1024 bytes",
            "provide bounded exact catalog and schema names",
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
        configuration_failure(
            request,
            connection_string,
            format!("invalid ODBC connection string: {error}"),
            "use a driver connection string with explicit Driver, Server, and database settings",
        )
    })?;
    let driver = connection_value(&values, &["driver"]);
    let dsn = connection_value(&values, &["dsn"]);
    if driver.is_none() && dsn.is_none() {
        return Err(configuration_failure(
            request,
            connection_string,
            "ODBC connection string must identify an installed Driver",
            "set Driver explicitly; opaque DSN-only sources are not accepted by the generic path",
        ));
    }
    if dsn.is_some() {
        return Err(AnalysisFailure::redacted(
            AnalysisFailureCode::UnsafeSource,
            AnalysisStage::Configuration,
            ODBC_SOURCE,
            &request.connection_alias,
            "generic ODBC analysis cannot verify the endpoint and transport policy hidden inside a DSN",
            "use an explicit driver connection string or a product adapter that validates the DSN policy",
            false,
            Some(connection_string),
        ));
    }

    let endpoint = connection_value(
        &values,
        &[
            "server",
            "host",
            "hostname",
            "address",
            "addr",
            "network address",
        ],
    )
    .ok_or_else(|| {
        configuration_failure(
            request,
            connection_string,
            "generic ODBC analysis requires an explicit server endpoint",
            "set Server or Host explicitly so local-versus-remote transport can be verified",
        )
    })?;
    let host = endpoint_host(endpoint);
    if !is_loopback_host(host) && !has_verified_remote_transport(&values) {
        return Err(AnalysisFailure::redacted(
            AnalysisFailureCode::UnsafeSource,
            AnalysisStage::Configuration,
            ODBC_SOURCE,
            &request.connection_alias,
            "remote ODBC sources require an explicit encrypted transport with certificate verification",
            "enable verified TLS in the driver connection string and disable trust-server-certificate bypasses",
            false,
            Some(connection_string),
        ));
    }
    Ok(())
}

fn connection_value<'a>(values: &'a AdoNetString, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| values.get(*key))
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
}

fn endpoint_host(endpoint: &str) -> &str {
    let endpoint = endpoint
        .trim()
        .strip_prefix("tcp:")
        .unwrap_or(endpoint.trim());
    if let Some(rest) = endpoint.strip_prefix('[') {
        return rest.split_once(']').map_or(rest, |(host, _)| host);
    }
    let host = endpoint
        .split_once(['\\', ','])
        .map_or(endpoint, |(host, _)| host);
    host.split_once(':').map_or(host, |(host, _)| host).trim()
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host == "."
        || host.eq_ignore_ascii_case("(local)")
        || host
            .parse::<IpAddr>()
            .map(|address| address.is_loopback())
            .unwrap_or(false)
}

fn has_verified_remote_transport(values: &AdoNetString) -> bool {
    if connection_flag(
        values,
        &["trustservercertificate", "trust server certificate"],
    ) == Some(true)
    {
        return false;
    }
    let encrypt = connection_value(values, &["encrypt"])
        .map(|value| value.to_ascii_lowercase())
        .is_some_and(|value| matches!(value.as_str(), "yes" | "true" | "mandatory" | "strict"));
    let ssl_mode = connection_value(values, &["sslmode", "ssl mode", "ssl-mode"])
        .map(|value| value.replace('-', "_").to_ascii_lowercase())
        .is_some_and(|value| {
            matches!(
                value.as_str(),
                "verify_ca" | "verify_full" | "verify_identity"
            )
        });
    let explicit_verify = connection_flag(values, &["ssl", "use ssl"]) == Some(true)
        && connection_flag(values, &["sslverify", "verify server certificate"]) == Some(true);
    encrypt || ssl_mode || explicit_verify
}

fn connection_flag(values: &AdoNetString, keys: &[&str]) -> Option<bool> {
    connection_value(values, keys).and_then(|value| {
        match value.trim().to_ascii_lowercase().as_str() {
            "true" | "yes" | "1" | "on" => Some(true),
            "false" | "no" | "0" | "off" => Some(false),
            _ => None,
        }
    })
}

fn configuration_failure(
    request: &IntrospectionRequest,
    connection_string: &str,
    message: impl AsRef<str>,
    remediation: impl AsRef<str>,
) -> AnalysisFailure {
    AnalysisFailure::redacted(
        AnalysisFailureCode::InvalidConfiguration,
        AnalysisStage::Configuration,
        ODBC_SOURCE,
        &request.connection_alias,
        message,
        remediation,
        false,
        Some(connection_string),
    )
}

#[cfg(any(feature = "odbc", test))]
fn rejected_assessment(
    functions: &[OdbcCatalogFunctionCapability],
    strategy: Option<&str>,
) -> OdbcCompletenessAssessment {
    let mut blockers = functions
        .iter()
        .filter(|capability| capability.support == OdbcCatalogFunctionSupport::NotSupported)
        .map(|capability| {
            format!(
                "driver does not declare support for {:?}",
                capability.function
            )
        })
        .collect::<Vec<_>>();
    blockers.push(match strategy {
        Some(strategy) => format!(
            "ODBC strategy '{strategy}' cannot run because its required catalog functions are unavailable"
        ),
        None => "no live-certified product strategy is registered for this ODBC identity".to_owned(),
    });
    blockers.extend([
        "ODBC catalog functions do not prove unique and check constraint semantics".to_owned(),
        "ODBC catalog functions do not expose a complete trigger inventory".to_owned(),
        "ODBC catalog functions do not expose complete cross-object dependencies".to_owned(),
        "driver-declared function support is not completeness evidence without live reconciliation"
            .to_owned(),
    ]);
    blockers.sort();
    blockers.dedup();
    OdbcCompletenessAssessment::Rejected { blockers }
}
