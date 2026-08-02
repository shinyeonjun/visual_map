// Split into focused source fragments; all fragments remain in this Rust module scope.

#[cfg(test)]
pub(crate) const SOURCE: &str = concat!(
    include_str!("sqlserver_catalog/core.rs"),
    "\n",
    include_str!("sqlserver_catalog/discovery.rs"),
    "\n",
    include_str!("sqlserver_catalog/reading.rs"),
    "\n",
    include_str!("sqlserver_catalog/mapping.rs"),
    "\n",
    include_str!("sqlserver_catalog/tests.rs"),
);

include!("sqlserver_catalog/core.rs");
include!("sqlserver_catalog/discovery.rs");
include!("sqlserver_catalog/reading.rs");
include!("sqlserver_catalog/mapping.rs");
include!("sqlserver_catalog/tests.rs");
