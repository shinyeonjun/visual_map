mod linker;
mod model;
mod snapshot;
mod visual_map;

pub(crate) use linker::{apply_focused_code_evidence, record_code_search_gap};
pub use model::{InventorySnapshot, VisualMap};
pub use snapshot::{
    build_inventory_snapshot, load_inventory_snapshot, load_inventory_snapshot_cached,
    mark_snapshot_staleness, save_inventory_snapshot, snapshot_staleness_reasons,
    snapshot_with_metadata,
};
pub use visual_map::visual_map;

#[cfg(test)]
pub(crate) use linker::candidate_links;
#[cfg(test)]
pub(crate) use model::InventoryItem;
#[cfg(test)]
pub(crate) use snapshot::{item, normalize_inventory, snapshot_backup_path, snapshot_path};
#[cfg(test)]
pub(crate) use visual_map::fixture_inventory;

#[cfg(test)]
mod tests;
