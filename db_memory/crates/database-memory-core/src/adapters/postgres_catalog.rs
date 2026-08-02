// Split into focused source fragments; all fragments remain in this Rust module scope.

#[cfg(test)]
pub(crate) const SOURCE: &str = concat!(
    include_str!("postgres_catalog/core.rs"),
    "\n",
    include_str!("postgres_catalog/discovery.rs"),
    "\n",
    include_str!("postgres_catalog/reading.rs"),
    "\n",
    include_str!("postgres_catalog/mapping.rs"),
    "\n",
    include_str!("postgres_catalog/tests.rs"),
);

include!("postgres_catalog/core.rs");
include!("postgres_catalog/discovery.rs");
include!("postgres_catalog/reading.rs");
include!("postgres_catalog/mapping.rs");
include!("postgres_catalog/tests.rs");
