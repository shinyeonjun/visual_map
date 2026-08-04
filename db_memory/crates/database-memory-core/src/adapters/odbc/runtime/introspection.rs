    fn strategy_for(
        driver: &OdbcDriverIdentity,
        server: &OdbcServerIdentity,
    ) -> Option<&'static dyn OdbcProductStrategy> {
        BUILTIN_ODBC_STRATEGIES
            .iter()
            .copied()
            .find(|strategy| strategy.matches(driver, server))
    }

    pub(super) fn sqlserver_native_connection_string(
        connection_string: &str,
        request: &IntrospectionRequest,
    ) -> Result<String, AnalysisFailure> {
        let mut values = connection_string.parse::<AdoNetString>().map_err(|error| {
            configuration_failure(
                request,
                connection_string,
                format!("cannot translate ODBC settings for SQL Server: {error}"),
                "use an explicit SQL Server ODBC driver connection string",
            )
        })?;

        values.remove("driver");
        values.remove("dsn");
        move_connection_value(
            &mut values,
            &["host", "hostname", "address", "addr", "network address"],
            "server",
        );
        move_connection_value(
            &mut values,
            &["trust server certificate"],
            "trustservercertificate",
        );
        move_connection_value(
            &mut values,
            &["trusted_connection", "trusted connection"],
            "integrated security",
        );

        if connection_value(&values, &["server"]).is_none() {
            return Err(configuration_failure(
                request,
                connection_string,
                "SQL Server ODBC bridge requires an explicit server endpoint",
                "set Server explicitly in the ODBC connection string",
            ));
        }
        if connection_value(&values, &["database", "initial catalog", "databasename"]).is_none() {
            return Err(configuration_failure(
                request,
                connection_string,
                "SQL Server ODBC bridge requires one explicit database",
                "set Database or Initial Catalog explicitly",
            ));
        }
        Ok(values.to_string())
    }

    fn move_connection_value(values: &mut AdoNetString, aliases: &[&str], canonical: &str) {
        if values.contains_key(canonical) {
            for alias in aliases {
                values.remove(*alias);
            }
            return;
        }
        if let Some(value) = aliases.iter().find_map(|alias| values.remove(*alias)) {
            values.insert(canonical.to_owned(), value);
        }
        for alias in aliases {
            values.remove(*alias);
        }
    }

    fn allocate_environment() -> Result<Environment, OdbcCallError> {
        match Environment::new() {
            SqlResult::Success(environment) => Ok(environment),
            SqlResult::SuccessWithInfo(_) => Err(OdbcCallError::new(
                None,
                "ODBC environment allocation returned an uninspectable warning",
            )),
            SqlResult::Error { function } => Err(OdbcCallError::new(
                None,
                format!("ODBC call '{function}' failed before diagnostics were available"),
            )),
            unexpected => Err(OdbcCallError::new(
                None,
                format!("ODBC environment allocation returned {unexpected:?}"),
            )),
        }
    }

    struct OdbcSession<'environment> {
        connection: Option<Connection<'environment>>,
    }

    impl<'environment> OdbcSession<'environment> {
        fn new(connection: Connection<'environment>) -> Self {
            Self {
                connection: Some(connection),
            }
        }

        fn connection(&self) -> &Connection<'environment> {
            self.connection.as_ref().expect("ODBC session is connected")
        }

        fn disconnect(mut self) -> Result<(), OdbcCallError> {
            disconnect_connection(self.connection.take().expect("ODBC session is connected"))
        }
    }

    impl Drop for OdbcSession<'_> {
        fn drop(&mut self) {
            if let Some(connection) = self.connection.take() {
                let _ = disconnect_connection(connection);
            }
        }
    }

    fn disconnect_connection(mut connection: Connection<'_>) -> Result<(), OdbcCallError> {
        match connection.disconnect().into_result(&connection) {
            Ok(()) => Ok(()),
            Err(first_error) => {
                let _ = connection.rollback();
                match connection.disconnect().into_result(&connection) {
                    Ok(()) => Err(OdbcCallError::from(first_error)),
                    Err(second_error) => {
                        std::mem::forget(connection);
                        Err(OdbcCallError::new(
                            diagnostic_state(&second_error),
                            format!("ODBC disconnect failed after rollback: {second_error}"),
                        ))
                    }
                }
            }
        }
    }

    #[derive(Clone, Copy)]
    struct Deadline {
        expires_at: Instant,
    }

    impl Deadline {
        fn new(timeout_ms: u64) -> Self {
            Self {
                expires_at: Instant::now() + Duration::from_millis(timeout_ms),
            }
        }

        fn remaining_seconds(
            self,
            request: &IntrospectionRequest,
            stage: AnalysisStage,
        ) -> Result<u32, AnalysisFailure> {
            let remaining = self.expires_at.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(timeout_failure(request, stage));
            }
            let rounded_up = remaining
                .as_secs()
                .saturating_add(u64::from(remaining.subsec_nanos() > 0));
            Ok(rounded_up.clamp(1, u32::MAX as u64) as u32)
        }
    }

    fn checkpoint(
        request: &IntrospectionRequest,
        cancellation: &CancellationToken,
        deadline: &Deadline,
        stage: AnalysisStage,
    ) -> Result<(), AnalysisFailure> {
        cancellation.checkpoint(ODBC_SOURCE, &request.connection_alias, stage)?;
        deadline.remaining_seconds(request, stage).map(|_| ())
    }

    fn timeout_failure(request: &IntrospectionRequest, stage: AnalysisStage) -> AnalysisFailure {
        AnalysisFailure::redacted(
            AnalysisFailureCode::Timeout,
            stage,
            ODBC_SOURCE,
            &request.connection_alias,
            format!(
                "ODBC metadata analysis exceeded the {} ms timeout",
                request.timeout_ms
            ),
            "increase the bounded timeout or reduce the requested metadata scope",
            true,
            None,
        )
    }

    fn set_read_only_access(connection: &Connection<'_>) -> Result<(), OdbcCallError> {
        let result = unsafe {
            SQLSetConnectAttr(
                connection.as_sys(),
                ConnectionAttribute::ACCESS_MODE,
                SQL_MODE_READ_ONLY as Pointer,
                IS_UINTEGER,
            )
        };
        require_clean_success(
            connection,
            result,
            "SQLSetConnectAttr(SQL_ATTR_ACCESS_MODE)",
        )
    }

    fn verify_read_only_access(connection: &Connection<'_>) -> Result<(), OdbcCallError> {
        let mut access_mode = 0u32;
        let result = unsafe {
            SQLGetConnectAttr(
                connection.as_sys(),
                ConnectionAttribute::ACCESS_MODE,
                &mut access_mode as *mut u32 as *mut c_void,
                IS_UINTEGER,
                null_mut(),
            )
        };
        require_clean_success(
            connection,
            result,
            "SQLGetConnectAttr(SQL_ATTR_ACCESS_MODE)",
        )?;
        if access_mode as usize != SQL_MODE_READ_ONLY {
            return Err(OdbcCallError::new(
                None,
                format!("ODBC driver reported access mode {access_mode} instead of read-only"),
            ));
        }
        Ok(())
    }

    fn function_supported(
        connection: &Connection<'_>,
        function: OdbcCatalogFunction,
    ) -> Result<bool, OdbcCallError> {
        let mut supported = 0u16;
        let result = unsafe {
            sql_get_functions(connection.as_sys(), function.function_id(), &mut supported)
        };
        require_clean_success(connection, result, "SQLGetFunctions")?;
        match supported {
            0 => Ok(false),
            1 => Ok(true),
            value => Err(OdbcCallError::new(
                None,
                format!("ODBC driver returned invalid SQLGetFunctions value {value}"),
            )),
        }
    }

    fn runtime_verifiable(function: OdbcCatalogFunction) -> bool {
        matches!(
            function,
            OdbcCatalogFunction::Tables
                | OdbcCatalogFunction::Columns
                | OdbcCatalogFunction::PrimaryKeys
                | OdbcCatalogFunction::ForeignKeys
        )
    }

    fn verify_catalog_function_call(
        connection: &Connection<'_>,
        function: OdbcCatalogFunction,
        catalog: Option<&str>,
        schema: Option<&str>,
        timeout_seconds: u32,
    ) -> Result<(), OdbcCallError> {
        let mut statement = connection
            .allocate_statement()
            .into_result(connection)
            .map_err(OdbcCallError::from)?;
        statement
            .set_query_timeout_sec(timeout_seconds as usize)
            .into_result(&statement)
            .map_err(OdbcCallError::from)?;
        let actual_timeout = statement
            .query_timeout_sec()
            .into_result(&statement)
            .map_err(OdbcCallError::from)?;
        if actual_timeout == 0 || actual_timeout > timeout_seconds as usize {
            return Err(OdbcCallError::new(
                None,
                format!(
                    "ODBC driver reported unsafe query timeout {actual_timeout}s for a {timeout_seconds}s deadline"
                ),
            ));
        }
        let mut statement = unsafe { Preallocated::new(statement) };
        let catalog = catalog.unwrap_or("");
        let schema = schema.unwrap_or("");
        match function {
            OdbcCatalogFunction::Tables => {
                let mut rows = statement
                    .tables(catalog, schema, CATALOG_PROBE_SENTINEL, "")
                    .map_err(OdbcCallError::from)?;
                if let Some(row) = rows.next() {
                    row.map_err(OdbcCallError::from)?;
                }
            }
            OdbcCatalogFunction::Columns => {
                let mut rows = statement
                    .columns(
                        catalog,
                        schema,
                        CATALOG_PROBE_SENTINEL,
                        CATALOG_PROBE_SENTINEL,
                    )
                    .map_err(OdbcCallError::from)?;
                if let Some(row) = rows.next() {
                    row.map_err(OdbcCallError::from)?;
                }
            }
            OdbcCatalogFunction::PrimaryKeys => {
                let mut rows = statement
                    .primary_keys(
                        (!catalog.is_empty()).then_some(catalog),
                        (!schema.is_empty()).then_some(schema),
                        CATALOG_PROBE_SENTINEL,
                    )
                    .map_err(OdbcCallError::from)?;
                if let Some(row) = rows.next() {
                    row.map_err(OdbcCallError::from)?;
                }
            }
            OdbcCatalogFunction::ForeignKeys => {
                let mut rows = statement
                    .foreign_keys("", "", "", catalog, schema, CATALOG_PROBE_SENTINEL)
                    .map_err(OdbcCallError::from)?;
                if let Some(row) = rows.next() {
                    row.map_err(OdbcCallError::from)?;
                }
            }
            _ => {
                return Err(OdbcCallError::new(
                    None,
                    "ODBC catalog function has no runtime verifier",
                ));
            }
        }
        Ok(())
    }
