// Split into focused validation fragments; all fragments remain in this adapter module scope.

include!("catalog_validation/inventory.rs");
include!("catalog_validation/partition.rs");
include!("catalog_validation/lob.rs");
include!("catalog_validation/mapper.rs");

