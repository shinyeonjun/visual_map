//! Python ORM adapter 조정 계층이다.

mod common;
mod django;
mod sqlalchemy;
mod sqlmodel;

use crate::facts::FactStore;
use std::collections::HashMap;

pub(super) fn enrich(facts: &mut FactStore, file_frameworks: &HashMap<String, Vec<String>>) {
    sqlmodel::enrich(facts, file_frameworks);
    sqlalchemy::enrich(facts, file_frameworks);
    django::enrich(facts, file_frameworks);
}
