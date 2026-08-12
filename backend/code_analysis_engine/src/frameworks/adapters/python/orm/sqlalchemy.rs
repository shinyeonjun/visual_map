//! SQLAlchemy Declarative 모델·Session 접근 adapter.

use super::common;
use crate::facts::FactStore;
use std::collections::HashMap;

pub(super) fn enrich(facts: &mut FactStore, file_frameworks: &HashMap<String, Vec<String>>) {
    // SQLModel은 SQLAlchemy parent detection도 함께 가질 수 있으므로 중복 생성하지 않는다.
    common::add_resources(
        facts,
        file_frameworks,
        &["python.sqlalchemy"],
        &["python.sqlmodel"],
    );
}
