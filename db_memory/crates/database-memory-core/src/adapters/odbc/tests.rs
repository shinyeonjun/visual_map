#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis_outcome::AnalysisStatus;

    const LOCAL_SQLSERVER: &str = "Driver={ODBC Driver 17 for SQL Server};Server=127.0.0.1,11433;Database=master;UID=sa;PWD={Password123!};Encrypt=no";

    #[test]
    fn runtime_availability_matches_the_build_feature() {
        assert_eq!(odbc_runtime_available(), cfg!(feature = "odbc"));
    }

    #[test]
    fn connection_policy_rejects_opaque_or_unverified_remote_sources() {
        let request = request();
        let dsn =
            validate_connection_policy(&request, "DSN=production;UID=app;PWD=secret").unwrap_err();
        assert_eq!(dsn.code, AnalysisFailureCode::UnsafeSource);
        assert!(!dsn.message.contains("secret"));

        let remote = validate_connection_policy(
            &request,
            "Driver={ODBC Driver 17 for SQL Server};Server=db.example;UID=app;PWD=secret;Encrypt=no",
        )
        .unwrap_err();
        assert_eq!(remote.code, AnalysisFailureCode::UnsafeSource);

        validate_connection_policy(
            &request,
            "Driver={ODBC Driver 17 for SQL Server};Server=db.example;UID=app;PWD=secret;Encrypt=yes;TrustServerCertificate=no",
        )
        .unwrap();
        validate_connection_policy(&request, LOCAL_SQLSERVER).unwrap();
    }

    #[test]
    fn rejected_assessment_names_every_unproven_contract() {
        let capabilities = OdbcCatalogFunction::ALL
            .into_iter()
            .map(|function| OdbcCatalogFunctionCapability {
                function,
                support: OdbcCatalogFunctionSupport::DriverDeclared,
            })
            .collect::<Vec<_>>();
        let OdbcCompletenessAssessment::Rejected { blockers } =
            rejected_assessment(&capabilities, None)
        else {
            panic!("generic ODBC must remain rejected without a certified strategy");
        };
        assert!(blockers.iter().any(|blocker| blocker.contains("trigger")));
        assert!(blockers
            .iter()
            .any(|blocker| blocker.contains("dependencies")));
        assert!(blockers
            .iter()
            .any(|blocker| blocker.contains("live-certified")));
    }

    #[cfg(feature = "odbc")]
    #[test]
    fn sqlserver_strategy_requires_runtime_verified_catalog_calls() {
        let driver = OdbcDriverIdentity {
            name: "ODBC Driver 17 for SQL Server".to_owned(),
            version: "17.10".to_owned(),
            odbc_version: "03.80".to_owned(),
        };
        let server = OdbcServerIdentity {
            product: "Microsoft SQL Server".to_owned(),
            version: "16.00".to_owned(),
        };
        let declared = OdbcCatalogFunction::ALL
            .into_iter()
            .map(|function| OdbcCatalogFunctionCapability {
                function,
                support: OdbcCatalogFunctionSupport::DriverDeclared,
            })
            .collect::<Vec<_>>();
        assert!(matches!(
            runtime::completeness_assessment(&driver, &server, &declared),
            OdbcCompletenessAssessment::Rejected { .. }
        ));

        let verified = declared
            .into_iter()
            .map(|capability| OdbcCatalogFunctionCapability {
                support: if matches!(
                    capability.function,
                    OdbcCatalogFunction::Tables
                        | OdbcCatalogFunction::Columns
                        | OdbcCatalogFunction::PrimaryKeys
                        | OdbcCatalogFunction::ForeignKeys
                ) {
                    OdbcCatalogFunctionSupport::RuntimeCallVerified
                } else {
                    capability.support
                },
                ..capability
            })
            .collect::<Vec<_>>();
        assert_eq!(
            runtime::completeness_assessment(&driver, &server, &verified),
            OdbcCompletenessAssessment::Eligible {
                strategy: "sqlserver-native-bridge-v1".to_owned()
            }
        );

        let impostor = OdbcServerIdentity {
            product: "SQL Server compatible proxy".to_owned(),
            version: "16.00".to_owned(),
        };
        assert!(matches!(
            runtime::completeness_assessment(&driver, &impostor, &verified),
            OdbcCompletenessAssessment::Rejected { .. }
        ));
    }

    #[cfg(feature = "odbc")]
    #[test]
    fn sqlserver_bridge_normalizes_odbc_aliases_without_losing_secrets() {
        let input = "Driver={ODBC Driver 17 for SQL Server};Address=127.0.0.1,1433;Initial Catalog=app;UID=reader;PWD={p;a};Encrypt=no;Trust Server Certificate=yes";
        let translated = runtime::sqlserver_native_connection_string(input, &request()).unwrap();
        let values = translated.parse::<AdoNetString>().unwrap();

        assert!(!values.contains_key("driver"));
        assert_eq!(
            values.get("server").map(String::as_str),
            Some("127.0.0.1,1433")
        );
        assert_eq!(
            values.get("initial catalog").map(String::as_str),
            Some("app")
        );
        assert_eq!(values.get("uid").map(String::as_str), Some("reader"));
        assert_eq!(values.get("pwd").map(String::as_str), Some("p;a"));
        assert_eq!(
            values.get("trustservercertificate").map(String::as_str),
            Some("yes")
        );
    }

    #[test]
    fn cancellation_prevents_driver_work() {
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let failure = probe_odbc_capabilities_with_cancellation(
            "not-even-an-odbc-connection-string",
            "cancelled",
            1_000,
            &cancellation,
        )
        .unwrap_err();
        assert_eq!(failure.code, AnalysisFailureCode::Cancelled);
    }

    #[cfg(not(feature = "odbc"))]
    #[test]
    fn disabled_runtime_fails_without_a_snapshot() {
        let outcome = introspect_odbc_complete(LOCAL_SQLSERVER, "disabled");
        assert_eq!(outcome.status(), AnalysisStatus::Failed);
        assert_eq!(
            outcome.failure().map(|failure| failure.code),
            Some(AnalysisFailureCode::DriverUnavailable)
        );
        assert!(outcome.certified_snapshot().is_none());
    }

    #[cfg(feature = "odbc")]
    #[test]
    #[ignore = "requires DATABASE_MEMORY_TEST_ODBC_SQLSERVER_URL"]
    fn sqlserver_capability_probe_is_live_and_env_gated() {
        let connection_string = std::env::var("DATABASE_MEMORY_TEST_ODBC_SQLSERVER_URL")
            .expect("live ODBC test requires DATABASE_MEMORY_TEST_ODBC_SQLSERVER_URL");
        let report = probe_odbc_capabilities(&connection_string, "odbc-sqlserver", 30_000)
            .expect("live ODBC capability probe must succeed");
        assert!(report
            .server
            .product
            .to_ascii_lowercase()
            .contains("sql server"));
        assert!(report.read_only_access_mode);
        assert!(report.metadata_functions_only);
        for function in [
            OdbcCatalogFunction::Tables,
            OdbcCatalogFunction::Columns,
            OdbcCatalogFunction::PrimaryKeys,
            OdbcCatalogFunction::ForeignKeys,
        ] {
            assert!(report
                .catalog_functions
                .iter()
                .find(|capability| capability.function == function)
                .is_some_and(|capability| {
                    capability.support == OdbcCatalogFunctionSupport::RuntimeCallVerified
                }));
        }
        assert!(matches!(
            report.completeness,
            OdbcCompletenessAssessment::Eligible { ref strategy }
                if strategy == "sqlserver-native-bridge-v1"
        ));
        let serialized = serde_json::to_string(&report).unwrap();
        assert!(!serialized.contains(&connection_string));
        assert!(!serialized.contains("PWD"));

        let outcome = introspect_odbc_complete(&connection_string, "odbc-sqlserver");
        assert_eq!(outcome.status(), AnalysisStatus::Complete);
        let snapshot = outcome
            .certified_snapshot()
            .expect("SQL Server ODBC bridge must return the native certified snapshot");
        assert_eq!(snapshot.snapshot.schema.source_kind, "sqlserver");
        assert_eq!(
            snapshot.completeness.status,
            crate::certification::CompletionStatus::Complete
        );
        assert!(outcome.failure().is_none());
    }

    fn request() -> IntrospectionRequest {
        IntrospectionRequest {
            connection_alias: "odbc-test".to_owned(),
            requested_catalogs: Vec::new(),
            requested_schemas: Vec::new(),
            timeout_ms: 30_000,
        }
    }
}
