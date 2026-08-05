use crate::{
    engine::EngineRegistry,
    paths::base_paths,
    workspace::{
        route_binding_id, validate_workspace_id, CodeCall, CodeInventory, CodeInventoryItem,
        DbConstraint, DbDependentObject, DbForeignKey, DbIndex, DbInventory, DbProfile, DbSource,
        Workspace,
    },
};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{btree_map::Entry, BTreeMap, BTreeSet, HashMap, HashSet},
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zip::ZipArchive;

use super::model::{
    Evidence, InventoryItem, InventorySnapshot, SnapshotGap, SnapshotLink, SnapshotMetadata,
    SnapshotMigration, SnapshotSourceMetadata, SourceLocation, SNAPSHOT_SCHEMA_VERSION,
};
mod client_request_links;
mod sqlite_store;

// Split into focused snapshot fragments; all fragments remain in this module scope.
include!("snapshot/core.rs");
include!("snapshot/persistence.rs");
include!("snapshot/engine_staleness.rs");
include!("snapshot/source_revision.rs");
include!("snapshot/item_mapping.rs");
