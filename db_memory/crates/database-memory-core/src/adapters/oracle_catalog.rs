// Split into focused source fragments; all fragments remain in this Rust module scope.

#[cfg(test)]
pub(crate) const SOURCE: &str = concat!(
    include_str!("oracle_catalog/core.rs"),
    "\n",
    include_str!("oracle_catalog/discovery.rs"),
    "\n",
    include_str!("oracle_catalog/reading.rs"),
    "\n",
    include_str!("oracle_catalog/validation.rs"),
    "\n",
    include_str!("oracle_catalog/catalog_validation.rs"),
    "\n",
    include_str!("oracle_catalog/mapping.rs"),
    "\n",
    include_str!("oracle_catalog/tests.rs"),
);

include!("oracle_catalog/core.rs");
include!("oracle_catalog/discovery.rs");
include!("oracle_catalog/reading.rs");
include!("oracle_catalog/validation.rs");
include!("oracle_catalog/catalog_validation.rs");
include!("oracle_catalog/mapping.rs");
include!("oracle_catalog/tests.rs");
