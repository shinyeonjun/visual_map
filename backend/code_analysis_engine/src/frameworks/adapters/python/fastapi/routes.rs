//! FastAPI decorator와 APIRouter prefix를 처리한다.

use super::{add_routes, policy};
use crate::facts::FactStore;
use std::collections::HashMap;

pub(super) fn enrich(facts: &mut FactStore, file_frameworks: &HashMap<String, Vec<String>>) {
    add_routes(facts, file_frameworks, policy());
}
