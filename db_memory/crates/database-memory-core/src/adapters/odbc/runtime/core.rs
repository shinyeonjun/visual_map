    use std::ffi::c_void;
    use std::mem::size_of;
    use std::ptr::null_mut;
    use std::time::{Duration, Instant};

    use odbc_api::handles::{
        slice_to_utf8, Connection, Diagnostics, Environment, Record, SqlResult, SqlText, Statement,
    };
    #[cfg(target_os = "windows")]
    use odbc_api::sys::SQLGetInfoW;
    use odbc_api::sys::{
        AttrOdbcVersion, ConnectionAttribute, HDbc, InfoType, Pointer, SQLGetConnectAttr,
        SQLGetInfo, SQLSetConnectAttr, SqlReturn, IS_UINTEGER,
    };
    use odbc_api::{Error, Preallocated};

    use super::*;

    const INFO_BUFFER_UNITS: usize = 1_024;
    const SQL_MODE_READ_ONLY: usize = 1;
    const CATALOG_PROBE_SENTINEL: &str = "__database_memory_odbc_capability_probe__";
    const SQLSERVER_BRIDGE_STRATEGY: &str = "sqlserver-native-bridge-v1";
    const SQLSERVER_REQUIRED_FUNCTIONS: [OdbcCatalogFunction; 4] = [
        OdbcCatalogFunction::Tables,
        OdbcCatalogFunction::Columns,
        OdbcCatalogFunction::PrimaryKeys,
        OdbcCatalogFunction::ForeignKeys,
    ];
