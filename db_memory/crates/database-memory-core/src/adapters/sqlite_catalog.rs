// Split into focused source fragments; all fragments remain in this Rust module scope.

#[cfg(test)]
pub(crate) const SOURCE: &str = concat!(
    include_str!("sqlite_catalog/core.rs"),
    "\n",
    include_str!("sqlite_catalog/discovery.rs"),
    "\n",
    include_str!("sqlite_catalog/mapping.rs"),
    "\n",
    include_str!("sqlite_catalog/access.rs"),
    "\n",
    include_str!("sqlite_catalog/validation.rs"),
);

include!("sqlite_catalog/core.rs");
include!("sqlite_catalog/discovery.rs");
include!("sqlite_catalog/mapping.rs");
include!("sqlite_catalog/access.rs");
include!("sqlite_catalog/validation.rs");
