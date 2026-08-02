// Split into focused source fragments; all fragments remain in this Rust module scope.

#[cfg(test)]
pub(crate) const SOURCE: &str = concat!(
    include_str!("mysql_catalog/core.rs"),
    "\n",
    include_str!("mysql_catalog/discovery.rs"),
    "\n",
    include_str!("mysql_catalog/reading.rs"),
    "\n",
    include_str!("mysql_catalog/mapping.rs"),
    "\n",
    include_str!("mysql_catalog/tests.rs"),
);

include!("mysql_catalog/core.rs");
include!("mysql_catalog/discovery.rs");
include!("mysql_catalog/reading.rs");
include!("mysql_catalog/mapping.rs");
include!("mysql_catalog/tests.rs");
