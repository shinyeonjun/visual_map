    fn transaction_capability(
        connection: &Connection<'_>,
    ) -> Result<OdbcTransactionCapability, OdbcCallError> {
        match info_u16(connection, InfoType::TransactionCapable)? {
            0 => Ok(OdbcTransactionCapability::None),
            1 => Ok(OdbcTransactionCapability::DmlOnly),
            2 => Ok(OdbcTransactionCapability::DdlAndDml),
            3 => Ok(OdbcTransactionCapability::DdlCommits),
            4 => Ok(OdbcTransactionCapability::DdlIgnored),
            value => Err(OdbcCallError::new(
                None,
                format!("ODBC driver returned invalid transaction capability {value}"),
            )),
        }
    }

    fn info_u16(connection: &Connection<'_>, info_type: InfoType) -> Result<u16, OdbcCallError> {
        let mut value = 0u16;
        let result = unsafe {
            SQLGetInfo(
                connection.as_sys(),
                info_type,
                &mut value as *mut u16 as Pointer,
                size_of::<u16>() as i16,
                null_mut(),
            )
        };
        require_clean_success(connection, result, "SQLGetInfo")?;
        Ok(value)
    }

    #[cfg(target_os = "windows")]
    fn info_string(
        connection: &Connection<'_>,
        info_type: InfoType,
    ) -> Result<String, OdbcCallError> {
        let mut buffer = vec![0u16; INFO_BUFFER_UNITS];
        let mut length_bytes = 0i16;
        let result = unsafe {
            SQLGetInfoW(
                connection.as_sys(),
                info_type,
                buffer.as_mut_ptr() as Pointer,
                (buffer.len() * size_of::<u16>()) as i16,
                &mut length_bytes,
            )
        };
        require_clean_success(connection, result, "SQLGetInfoW")?;
        if length_bytes < 0
            || length_bytes as usize >= buffer.len() * size_of::<u16>()
            || !(length_bytes as usize).is_multiple_of(size_of::<u16>())
        {
            return Err(OdbcCallError::new(
                None,
                "ODBC driver returned an invalid or truncated SQLGetInfoW string",
            ));
        }
        let units = length_bytes as usize / size_of::<u16>();
        let value = String::from_utf16(&buffer[..units]).map_err(|error| {
            OdbcCallError::new(None, format!("ODBC identity is not valid UTF-16: {error}"))
        })?;
        non_empty_identity(value)
    }

    #[cfg(not(target_os = "windows"))]
    fn info_string(
        connection: &Connection<'_>,
        info_type: InfoType,
    ) -> Result<String, OdbcCallError> {
        let mut buffer = vec![0u8; INFO_BUFFER_UNITS];
        let mut length_bytes = 0i16;
        let result = unsafe {
            SQLGetInfo(
                connection.as_sys(),
                info_type,
                buffer.as_mut_ptr() as Pointer,
                buffer.len() as i16,
                &mut length_bytes,
            )
        };
        require_clean_success(connection, result, "SQLGetInfo")?;
        if length_bytes < 0 || length_bytes as usize >= buffer.len() {
            return Err(OdbcCallError::new(
                None,
                "ODBC driver returned an invalid or truncated SQLGetInfo string",
            ));
        }
        let value =
            String::from_utf8(buffer[..length_bytes as usize].to_vec()).map_err(|error| {
                OdbcCallError::new(None, format!("ODBC identity is not valid UTF-8: {error}"))
            })?;
        non_empty_identity(value)
    }

    fn non_empty_identity(value: String) -> Result<String, OdbcCallError> {
        let value = value.trim().to_owned();
        if value.is_empty() {
            Err(OdbcCallError::new(
                None,
                "ODBC driver returned an empty identity field",
            ))
        } else {
            Ok(value)
        }
    }

    fn current_catalog(connection: &Connection<'_>) -> Result<Option<String>, OdbcCallError> {
        let mut buffer = Vec::new();
        match connection
            .fetch_current_catalog(&mut buffer)
            .into_result(connection)
        {
            Ok(()) => {
                let value = slice_to_utf8(&buffer)
                    .map_err(|error| {
                        OdbcCallError::new(
                            None,
                            format!("ODBC current catalog is not valid text: {error}"),
                        )
                    })?
                    .trim()
                    .to_owned();
                if value.len() > MAX_SCOPE_VALUE_BYTES {
                    return Err(OdbcCallError::new(
                        None,
                        "ODBC current catalog exceeds the 1024-byte contract limit",
                    ));
                }
                Ok((!value.is_empty()).then_some(value))
            }
            Err(error) if is_unsupported_error(&error) => Ok(None),
            Err(error) => Err(OdbcCallError::from(error)),
        }
    }

    fn validate_scope(
        request: &IntrospectionRequest,
        current_catalog: Option<&str>,
        connection_string: &str,
    ) -> Result<(), AnalysisFailure> {
        if request.requested_catalogs.is_empty() {
            return Ok(());
        }
        if current_catalog.is_some_and(|catalog| request.requested_catalogs == [catalog]) {
            return Ok(());
        }
        Err(configuration_failure(
            request,
            connection_string,
            format!(
                "generic ODBC analysis is bound to current catalog '{}'; requested catalogs were {}",
                current_catalog.unwrap_or("<not reported>"),
                request.requested_catalogs.join(", ")
            ),
            "connect directly to one catalog and request only that exact catalog",
        ))
    }

    fn require_clean_success(
        handle: &impl Diagnostics,
        result: SqlReturn,
        function: &'static str,
    ) -> Result<(), OdbcCallError> {
        match result {
            SqlReturn::SUCCESS => Ok(()),
            SqlReturn::SUCCESS_WITH_INFO => Err(diagnostic_error(
                handle,
                format!("{function} returned a warning; strict capability proof rejects substituted values"),
            )),
            other => Err(diagnostic_error(
                handle,
                format!("{function} failed with return code {}", other.0),
            )),
        }
    }

    fn diagnostic_error(handle: &impl Diagnostics, fallback: impl Into<String>) -> OdbcCallError {
        let mut record = Record::with_capacity(512);
        if record.fill_from(handle, 1) {
            OdbcCallError::new(Some(record.state.as_str().to_owned()), record.to_string())
        } else {
            OdbcCallError::new(None, fallback)
        }
    }
