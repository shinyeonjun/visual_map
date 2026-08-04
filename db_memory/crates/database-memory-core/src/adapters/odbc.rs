include!("odbc/core.rs");

#[cfg(feature = "odbc")]
mod runtime {
    include!("odbc/runtime/core.rs");
    include!("odbc/runtime/strategies.rs");
    include!("odbc/runtime/introspection.rs");
    include!("odbc/runtime/validation.rs");
    include!("odbc/runtime/errors.rs");
}

include!("odbc/tests.rs");
