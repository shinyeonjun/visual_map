use super::model::{ClientRequest, CodeInventory, CodeInventoryItem};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

include!("client_requests/scan.rs");
include!("client_requests/parse.rs");
include!("client_requests/resolve.rs");

#[cfg(test)]
include!("client_requests_tests.rs");
