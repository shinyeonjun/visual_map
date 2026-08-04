use std::collections::{BTreeMap, HashMap, HashSet};

use super::{implementation_file_score, signal_needle, symbol_short_name, FrameworkPack};
use crate::DocumentOutput;

#[cfg(test)]
use super::symbol_matches_name;

// Split into focused fragments; all fragments remain in this module scope.
include!("facts/symbol_index.rs");
include!("facts/fact_evidence.rs");
include!("facts/fact_properties.rs");
include!("facts/route_parsing.rs");
include!("facts/handler_resolution.rs");
