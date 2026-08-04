use crate::{
    engine,
    paths::{base_paths, ensure_base_dirs},
};
use serde::Serialize;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use super::model::{CreateWorkspaceRequest, RepoSource, Workspace, WorkspaceEngineCache};

static NEXT_WORKSPACE_ID: AtomicU64 = AtomicU64::new(0);
static NEXT_WORKSPACE_WRITE_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspaceRecoveryWarning {
    pub workspace_id: String,
    pub kind: String,
    pub message: String,
    pub action: String,
}

include!("store/lifecycle.rs");
include!("store/persistence.rs");
include!("store/paths.rs");
include!("store/git.rs");

#[cfg(test)]
include!("store_tests.rs");
