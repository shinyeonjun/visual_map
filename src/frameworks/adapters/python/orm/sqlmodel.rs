//! SQLModel 모델·Session 접근 adapter.

use super::common;
use crate::facts::FactStore;
use std::collections::HashMap;

pub(super) fn enrich(facts: &mut FactStore, file_frameworks: &HashMap<String, Vec<String>>) {
    common::add_resources(facts, file_frameworks, &["python.sqlmodel"], &[]);
}
