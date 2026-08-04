    trait OdbcProductStrategy: Sync {
        fn id(&self) -> &'static str;
        fn matches(&self, driver: &OdbcDriverIdentity, server: &OdbcServerIdentity) -> bool;
        fn required_functions(&self) -> &'static [OdbcCatalogFunction];
        fn analyze(
            &self,
            connection_string: &str,
            request: &IntrospectionRequest,
            cancellation: &CancellationToken,
        ) -> AnalysisOutcome;
    }

    struct SqlServerOdbcStrategy;

    impl OdbcProductStrategy for SqlServerOdbcStrategy {
        fn id(&self) -> &'static str {
            SQLSERVER_BRIDGE_STRATEGY
        }

        fn matches(&self, _driver: &OdbcDriverIdentity, server: &OdbcServerIdentity) -> bool {
            server
                .product
                .trim()
                .eq_ignore_ascii_case("Microsoft SQL Server")
        }

        fn required_functions(&self) -> &'static [OdbcCatalogFunction] {
            &SQLSERVER_REQUIRED_FUNCTIONS
        }

        fn analyze(
            &self,
            connection_string: &str,
            request: &IntrospectionRequest,
            cancellation: &CancellationToken,
        ) -> AnalysisOutcome {
            let connection_string =
                match sqlserver_native_connection_string(connection_string, request) {
                    Ok(connection_string) => connection_string,
                    Err(failure) => return AnalysisOutcome::failed(failure),
                };
            crate::adapters::sqlserver_catalog::analyze_sqlserver_with_cancellation(
                &connection_string,
                &request.connection_alias,
                request.requested_catalogs.clone(),
                request.requested_schemas.clone(),
                request.timeout_ms,
                cancellation,
            )
        }
    }

    static SQLSERVER_ODBC_STRATEGY: SqlServerOdbcStrategy = SqlServerOdbcStrategy;
    static BUILTIN_ODBC_STRATEGIES: [&dyn OdbcProductStrategy; 1] = [&SQLSERVER_ODBC_STRATEGY];

    extern "system" {
        #[link_name = "SQLGetFunctions"]
        fn sql_get_functions(
            connection_handle: HDbc,
            function_id: u16,
            supported: *mut u16,
        ) -> SqlReturn;
    }

    pub(super) fn probe(
        connection_string: &str,
        request: &IntrospectionRequest,
        cancellation: &CancellationToken,
    ) -> Result<OdbcCapabilityReport, AnalysisFailure> {
        let deadline = Deadline::new(request.timeout_ms);
        checkpoint(request, cancellation, &deadline, AnalysisStage::Connection)?;
        let environment = allocate_environment().map_err(|error| {
            classify_error(request, connection_string, error, AnalysisStage::Connection)
        })?;
        environment
            .declare_version(AttrOdbcVersion::Odbc3_80)
            .into_result(&environment)
            .map_err(OdbcCallError::from)
            .map_err(|error| {
                classify_error(request, connection_string, error, AnalysisStage::Connection)
            })?;
        let mut connection = environment
            .allocate_connection()
            .into_result(&environment)
            .map_err(OdbcCallError::from)
            .map_err(|error| {
                classify_error(request, connection_string, error, AnalysisStage::Connection)
            })?;
        set_read_only_access(&connection).map_err(|error| {
            unsafe_source_failure(request, connection_string, error.to_string())
        })?;
        connection
            .set_login_timeout_sec(deadline.remaining_seconds(request, AnalysisStage::Connection)?)
            .into_result(&connection)
            .map_err(OdbcCallError::from)
            .map_err(|error| {
                classify_error(request, connection_string, error, AnalysisStage::Connection)
            })?;
        connection
            .connect_with_connection_string(&SqlText::new(connection_string))
            .into_result(&connection)
            .map_err(OdbcCallError::from)
            .map_err(|error| {
                classify_error(request, connection_string, error, AnalysisStage::Connection)
            })?;
        let session = OdbcSession::new(connection);
        checkpoint(
            request,
            cancellation,
            &deadline,
            AnalysisStage::CapabilityProbe,
        )?;
        verify_read_only_access(session.connection()).map_err(|error| {
            unsafe_source_failure(request, connection_string, error.to_string())
        })?;

        let driver = OdbcDriverIdentity {
            name: info_string(session.connection(), InfoType::DriverName).map_err(|error| {
                classify_error(
                    request,
                    connection_string,
                    error,
                    AnalysisStage::CapabilityProbe,
                )
            })?,
            version: info_string(session.connection(), InfoType::DriverVer).map_err(|error| {
                classify_error(
                    request,
                    connection_string,
                    error,
                    AnalysisStage::CapabilityProbe,
                )
            })?,
            odbc_version: info_string(session.connection(), InfoType::DriverOdbcVer).map_err(
                |error| {
                    classify_error(
                        request,
                        connection_string,
                        error,
                        AnalysisStage::CapabilityProbe,
                    )
                },
            )?,
        };
        let server = OdbcServerIdentity {
            product: info_string(session.connection(), InfoType::DbmsName).map_err(|error| {
                classify_error(
                    request,
                    connection_string,
                    error,
                    AnalysisStage::CapabilityProbe,
                )
            })?,
            version: info_string(session.connection(), InfoType::DbmsVer).map_err(|error| {
                classify_error(
                    request,
                    connection_string,
                    error,
                    AnalysisStage::CapabilityProbe,
                )
            })?,
        };
        let data_source_read_only =
            match info_string(session.connection(), InfoType::DataSourceReadOnly)
                .map_err(|error| {
                    classify_error(
                        request,
                        connection_string,
                        error,
                        AnalysisStage::CapabilityProbe,
                    )
                })?
                .trim()
                .to_ascii_uppercase()
                .as_str()
            {
                "Y" => true,
                "N" => false,
                value => {
                    return Err(metadata_failure(
                        request,
                        connection_string,
                        format!(
                        "ODBC driver returned invalid SQL_DATA_SOURCE_READ_ONLY value '{value}'"
                    ),
                    ));
                }
            };
        let transaction_capability =
            transaction_capability(session.connection()).map_err(|error| {
                classify_error(
                    request,
                    connection_string,
                    error,
                    AnalysisStage::CapabilityProbe,
                )
            })?;
        let current_catalog = current_catalog(session.connection()).map_err(|error| {
            classify_error(
                request,
                connection_string,
                error,
                AnalysisStage::CapabilityProbe,
            )
        })?;
        validate_scope(request, current_catalog.as_deref(), connection_string)?;

        let mut catalog_functions = Vec::with_capacity(OdbcCatalogFunction::ALL.len());
        for function in OdbcCatalogFunction::ALL {
            checkpoint(
                request,
                cancellation,
                &deadline,
                AnalysisStage::CapabilityProbe,
            )?;
            let declared_supported =
                function_supported(session.connection(), function).map_err(|error| {
                    classify_error(
                        request,
                        connection_string,
                        error,
                        AnalysisStage::CapabilityProbe,
                    )
                })?;
            let support = if !declared_supported {
                OdbcCatalogFunctionSupport::NotSupported
            } else if runtime_verifiable(function) {
                verify_catalog_function_call(
                    session.connection(),
                    function,
                    current_catalog.as_deref(),
                    request.requested_schemas.first().map(String::as_str),
                    deadline.remaining_seconds(request, AnalysisStage::CapabilityProbe)?,
                )
                .map_err(|error| {
                    classify_error(
                        request,
                        connection_string,
                        error,
                        AnalysisStage::CapabilityProbe,
                    )
                })?;
                OdbcCatalogFunctionSupport::RuntimeCallVerified
            } else {
                OdbcCatalogFunctionSupport::DriverDeclared
            };
            catalog_functions.push(OdbcCatalogFunctionCapability { function, support });
        }
        checkpoint(
            request,
            cancellation,
            &deadline,
            AnalysisStage::CapabilityProbe,
        )?;
        session.disconnect().map_err(|error| {
            classify_error(request, connection_string, error, AnalysisStage::Connection)
        })?;

        let completeness = completeness_assessment(&driver, &server, &catalog_functions);
        Ok(OdbcCapabilityReport {
            contract_version: ODBC_PROBE_CONTRACT_VERSION,
            source_kind: ODBC_SOURCE.to_owned(),
            connection_alias: request.connection_alias.clone(),
            driver,
            server,
            current_catalog,
            metadata_functions_only: true,
            read_only_access_mode: true,
            data_source_read_only,
            transaction_capability,
            catalog_functions,
            completeness,
        })
    }

    pub(super) fn analyze_with_registered_strategy(
        connection_string: &str,
        request: &IntrospectionRequest,
        cancellation: &CancellationToken,
        report: &OdbcCapabilityReport,
    ) -> Option<AnalysisOutcome> {
        let strategy = strategy_for(&report.driver, &report.server)?;
        let OdbcCompletenessAssessment::Eligible { strategy: eligible } = &report.completeness
        else {
            return None;
        };
        (eligible == strategy.id())
            .then(|| strategy.analyze(connection_string, request, cancellation))
    }

    pub(super) fn completeness_assessment(
        driver: &OdbcDriverIdentity,
        server: &OdbcServerIdentity,
        functions: &[OdbcCatalogFunctionCapability],
    ) -> OdbcCompletenessAssessment {
        let Some(strategy) = strategy_for(driver, server) else {
            return rejected_assessment(functions, None);
        };
        let requirements_met = strategy.required_functions().iter().all(|required| {
            functions.iter().any(|capability| {
                capability.function == *required
                    && if runtime_verifiable(*required) {
                        capability.support == OdbcCatalogFunctionSupport::RuntimeCallVerified
                    } else {
                        capability.support != OdbcCatalogFunctionSupport::NotSupported
                    }
            })
        });
        if requirements_met {
            OdbcCompletenessAssessment::Eligible {
                strategy: strategy.id().to_owned(),
            }
        } else {
            rejected_assessment(functions, Some(strategy.id()))
        }
    }
