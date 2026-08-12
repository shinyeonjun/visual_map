//! Django ORM 모델·QuerySet 접근을 공통 resource 사실로 변환한다.

use super::common;
use crate::facts::FactStore;
use std::collections::HashMap;

pub(super) fn enrich(facts: &mut FactStore, file_frameworks: &HashMap<String, Vec<String>>) {
    common::add_resources(
        facts,
        file_frameworks,
        &["python.django"],
        &["python.sqlmodel", "python.sqlalchemy"],
    );
}
