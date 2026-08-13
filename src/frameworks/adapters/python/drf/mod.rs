//! Django REST Framework router 등록을 HTTP 진입점으로 변환한다.

use super::common::{add_call_entrypoint, source_file_id, CallEntrypointSpec};
use crate::facts::{EntrypointKind, FactStore};
use std::collections::HashMap;

pub(super) fn enrich(facts: &mut FactStore, file_frameworks: &HashMap<String, Vec<String>>) {
    let call_count = facts.call_sites.len();
    for index in 0..call_count {
        let Some(call) = facts.call_sites.get(index) else {
            continue;
        };
        let Some(file_id) = source_file_id(facts, &call.source_unit_id) else {
            continue;
        };
        if !file_frameworks
            .get(file_id)
            .is_some_and(|ids| ids.iter().any(|id| id == "python.django_rest_framework"))
        {
            continue;
        }
        if call.callee.rsplit('.').next().unwrap_or(&call.callee) != "register" {
            continue;
        }
        add_call_entrypoint(
            facts,
            index,
            CallEntrypointSpec {
                framework_id: "python.django_rest_framework",
                kind: EntrypointKind::Http,
                method: "HTTP",
                path_argument_index: 0,
                target_argument_index: Some(1),
                evidence_kind: "drfRouterRegister",
            },
        );
    }
}
