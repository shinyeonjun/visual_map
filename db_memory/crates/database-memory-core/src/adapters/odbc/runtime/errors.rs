    #[derive(Debug)]
    struct OdbcCallError {
        state: Option<String>,
        message: String,
    }

    impl OdbcCallError {
        fn new(state: Option<String>, message: impl Into<String>) -> Self {
            Self {
                state,
                message: message.into(),
            }
        }
    }

    impl std::fmt::Display for OdbcCallError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str(&self.message)
        }
    }

    impl From<Error> for OdbcCallError {
        fn from(error: Error) -> Self {
            Self::new(diagnostic_state(&error), error.to_string())
        }
    }

    fn diagnostic_state(error: &Error) -> Option<String> {
        match error {
            Error::Diagnostics { record, .. } => Some(record.state.as_str().to_owned()),
            _ => None,
        }
    }

    fn is_unsupported_error(error: &Error) -> bool {
        diagnostic_state(error).is_some_and(|state| matches!(state.as_str(), "HYC00" | "IM001"))
    }

    fn classify_error(
        request: &IntrospectionRequest,
        connection_string: &str,
        error: OdbcCallError,
        stage: AnalysisStage,
    ) -> AnalysisFailure {
        let state = error.state.as_deref().unwrap_or("");
        let (code, retryable, remediation) = match state {
            "28000" => (
                AnalysisFailureCode::AuthenticationFailed,
                false,
                "verify the ODBC principal and secret",
            ),
            "IM002" | "IM003" | "IM004" | "IM005" | "IM006" | "IM014" => (
                AnalysisFailureCode::DriverUnavailable,
                false,
                "install and select a matching ODBC driver with the same process architecture",
            ),
            "HYT00" | "HYT01" | "S1T00" => (
                AnalysisFailureCode::Timeout,
                true,
                "increase the bounded timeout or reduce the metadata scope",
            ),
            "HY008" => (
                AnalysisFailureCode::Cancelled,
                true,
                "start a new analysis when the result is still needed",
            ),
            "42501" => (
                AnalysisFailureCode::PermissionDenied,
                false,
                "grant metadata visibility for every requested catalog and schema",
            ),
            "HYC00" | "IM001" => (
                AnalysisFailureCode::UnsupportedMetadata,
                false,
                "use a driver that implements the required ODBC metadata capability",
            ),
            value if value.starts_with("08") => (
                AnalysisFailureCode::ConnectionFailed,
                true,
                "verify the ODBC endpoint, network path, and TLS policy",
            ),
            _ if stage == AnalysisStage::Connection => (
                AnalysisFailureCode::ConnectionFailed,
                true,
                "verify the ODBC driver, endpoint, credentials, and transport policy",
            ),
            _ => (
                AnalysisFailureCode::MetadataQueryFailed,
                true,
                "inspect the ODBC driver diagnostics and retry the metadata-only probe",
            ),
        };
        AnalysisFailure::redacted(
            code,
            stage,
            ODBC_SOURCE,
            &request.connection_alias,
            error.to_string(),
            remediation,
            retryable,
            Some(connection_string),
        )
    }

    fn unsafe_source_failure(
        request: &IntrospectionRequest,
        connection_string: &str,
        message: impl AsRef<str>,
    ) -> AnalysisFailure {
        AnalysisFailure::redacted(
            AnalysisFailureCode::UnsafeSource,
            AnalysisStage::Connection,
            ODBC_SOURCE,
            &request.connection_alias,
            message,
            "use an ODBC driver that accepts and reports SQL_MODE_READ_ONLY for metadata analysis",
            false,
            Some(connection_string),
        )
    }

    fn metadata_failure(
        request: &IntrospectionRequest,
        connection_string: &str,
        message: impl AsRef<str>,
    ) -> AnalysisFailure {
        AnalysisFailure::redacted(
            AnalysisFailureCode::UnsupportedMetadata,
            AnalysisStage::CapabilityProbe,
            ODBC_SOURCE,
            &request.connection_alias,
            message,
            "use a conforming ODBC driver or a certified native adapter",
            false,
            Some(connection_string),
        )
    }
