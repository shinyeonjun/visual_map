//! 호출 기반 외부 이벤트 경계를 공통 사실로 materialize한다.

use crate::facts::{Entrypoint, EntrypointKind, FactStore};
use crate::frameworks::registry::detector::FrameworkDetection;
use crate::languages::common::metadata::stable_id;
use std::collections::HashSet;

use super::FrameworkApplicabilityIndex;

#[derive(Debug, Clone)]
pub struct CallEventRule {
    pub framework_id: &'static str,
    pub call_names: &'static [&'static str],
    pub event_name_argument_index: usize,
    pub method: &'static str,
}

/// `invoke("command")`처럼 호출 자체가 외부 이벤트 경계를 나타내는
/// 프레임워크 API를 정적 Event entrypoint로 보존한다.
pub fn add_call_events(
    facts: &mut FactStore,
    detections: &[FrameworkDetection],
    rule: CallEventRule,
) {
    if !detections
        .iter()
        .any(|detection| detection.id == rule.framework_id)
    {
        return;
    }
    let applicability = FrameworkApplicabilityIndex::new(detections);
    let mut existing_ids = facts
        .entrypoints
        .iter()
        .map(|entrypoint| entrypoint.id.clone())
        .collect::<HashSet<_>>();
    let calls = facts.call_sites.clone();

    for call in calls {
        let call_name = call
            .callee
            .rsplit_once('.')
            .map(|(_, name)| name)
            .or_else(|| call.callee.rsplit_once("::").map(|(_, name)| name))
            .unwrap_or(&call.callee);
        if !rule
            .call_names
            .iter()
            .any(|name| name.eq_ignore_ascii_case(call_name))
        {
            continue;
        }
        if !applicability.applies(facts, &call.source_unit_id, rule.framework_id) {
            continue;
        }

        let event_name = call
            .arguments
            .get(rule.event_name_argument_index)
            .and_then(|argument| string_literal(argument));
        let display_name = event_name
            .clone()
            .unwrap_or_else(|| "<dynamic>".to_string());
        let id = stable_id(
            "entry",
            &format!(
                "{}:{}:{}:{}",
                call.id, rule.framework_id, rule.method, display_name
            ),
        );
        if !existing_ids.insert(id.clone()) {
            continue;
        }
        let evidence = call
            .evidence
            .first()
            .cloned()
            .map(|mut evidence| {
                evidence.kind = "frameworkEventCall".to_string();
                evidence.value = display_name.clone();
                evidence
            })
            .into_iter()
            .collect();
        facts.entrypoints.push(Entrypoint {
            id,
            unit_id: call.source_unit_id,
            kind: EntrypointKind::Event,
            name: display_name,
            method: Some(rule.method.to_string()),
            path: event_name,
            framework_id: Some(rule.framework_id.to_string()),
            evidence,
        });
    }
}

fn string_literal(value: &str) -> Option<String> {
    let value = value.trim();
    if value.len() < 2 {
        return None;
    }
    let (first, last) = (value.chars().next()?, value.chars().last()?);
    if !matches!((first, last), ('"', '"') | ('\'', '\'') | ('`', '`')) {
        return None;
    }
    Some(value[1..value.len() - 1].to_string())
}
