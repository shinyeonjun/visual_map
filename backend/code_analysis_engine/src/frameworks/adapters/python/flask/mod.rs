//! Flask adapter.

mod routes;

use crate::facts::FactStore;
use std::collections::HashMap;

pub(super) fn enrich(facts: &mut FactStore, file_frameworks: &HashMap<String, Vec<String>>) {
    routes::enrich(facts, file_frameworks);
}
