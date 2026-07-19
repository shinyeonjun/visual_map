mod api_flow;
mod architecture;
mod impact;
mod impact_review;
mod linker;
mod model;
mod projection_support;
mod snapshot;
mod visual_map;

pub(crate) use linker::{apply_focused_code_evidence, record_code_search_gap};
pub(crate) use model::{ChangeIntent, InventorySnapshot, VisualMap};
pub(crate) use snapshot::{
    build_inventory_snapshot, load_inventory_snapshot_cached, load_inventory_snapshot_optional,
    mark_snapshot_staleness, remove_db_inventory_snapshot, save_inventory_snapshot,
    snapshot_staleness_reasons, snapshot_with_metadata,
};
pub(crate) use visual_map::visual_map_with_change;

#[cfg(test)]
pub(crate) use visual_map::visual_map;

#[cfg(test)]
use linker::candidate_links;
#[cfg(test)]
pub(crate) use model::InventoryItem;
#[cfg(test)]
pub(crate) use snapshot::{
    item, load_inventory_snapshot, normalize_inventory, snapshot_backup_path, snapshot_path,
};
#[cfg(test)]
pub(crate) use visual_map::fixture_inventory;

#[cfg(test)]
mod tests;
