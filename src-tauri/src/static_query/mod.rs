//! Bounded, deterministic queries derived from the published Fact Graph.
//!
//! These queries never add facts. They only order and aggregate already
//! verified nodes, edges, evidence, coverage, and gaps for product views.

mod trace_path;

pub(crate) use trace_path::{representative_trace_paths, trace_paths_from_fact, TraceLimits};
