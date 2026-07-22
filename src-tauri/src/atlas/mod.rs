mod api_flow;
mod architecture;
mod composition;
mod impact;
mod impact_review;
mod inventory_query;
mod linker;
mod model;
mod projection_support;
mod semantic_links;
mod snapshot;
mod visual_map;

pub(crate) use composition::{composition_map, validate_composition_request};
pub(crate) use inventory_query::{
    inventory_bootstrap, search_inventory, InventoryBootstrap, InventorySearchResult,
};
pub(crate) use linker::{apply_focused_code_evidence, record_code_search_gap};
pub(crate) use model::{ChangeIntent, InventorySnapshot, VisualMap};
pub(crate) use semantic_links::{
    apply_explicit_query_evidence, apply_explicit_query_evidence_for_code,
};
pub(crate) use snapshot::{
    build_inventory_snapshot, invalidate_snapshot_freshness, load_inventory_snapshot_cached,
    load_inventory_snapshot_optional, load_inventory_snapshot_optional_cached,
    remove_db_inventory_snapshot, replace_inventory_source, save_inventory_snapshot,
    snapshot_staleness_reasons_cached, snapshot_with_metadata,
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
    item, load_inventory_snapshot, mark_snapshot_staleness, normalize_inventory,
    snapshot_backup_path, snapshot_path,
};
#[cfg(test)]
pub(crate) use visual_map::fixture_inventory;

#[cfg(test)]
mod tests;
