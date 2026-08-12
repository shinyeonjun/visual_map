//! 호출 기반 외부 이벤트 경계를 공통 사실로 materialize한다.

use crate::facts::{Entrypoint, EntrypointKind, FactStore};
use crate::frameworks::registry::detector::FrameworkDetection;
use crate::languages::common::metadata::stable_id;
use crate::model::Language;
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

/// 언어 자체가 제공하는 이벤트 등록 경계를 공통 Event entrypoint로 보존한다.
///
/// DOM의 `addEventListener`와 Tauri Rust의 `on_window_event`는 특정 프레임워크
/// detection이 없어도 외부 입력 경계라는 의미가 분명하다. 콜백 함수의 실제
/// 런타임 대상이 동적이면 추측하지 않고 등록 호출의 소스 유닛을 경계로
/// 남긴다. 이후 정적 callback resolver가 확정하면 같은 entrypoint를 보강할 수 있다.
pub fn add_language_callback_events(facts: &mut FactStore) {
    let calls = facts.call_sites.clone();
    let mut existing_ids = facts
        .entrypoints
        .iter()
        .map(|entrypoint| entrypoint.id.clone())
        .collect::<HashSet<_>>();

    for call in calls {
        let Some(unit) = facts.unit(&call.source_unit_id) else {
            continue;
        };
        let call_name = last_segment(&call.callee).to_ascii_lowercase();
        let (event_kind, method) = match (unit.language, call_name.as_str()) {
            (Language::JavaScript | Language::TypeScript, "addeventlistener") => {
                (EntrypointKind::Event, "DOM_EVENT_LISTENER")
            }
            (Language::Rust, "on_window_event") => (EntrypointKind::Callback, "TAURI_WINDOW_EVENT"),
            _ => continue,
        };
        let event_name = call
            .arguments
            .first()
            .and_then(|argument| string_literal(argument));
        let display_name = event_name
            .clone()
            .unwrap_or_else(|| "<dynamic>".to_string());
        let id = stable_id("entry", &format!("{}:{}:{}", call.id, method, display_name));
        if !existing_ids.insert(id.clone()) {
            continue;
        }
        let evidence = call
            .evidence
            .first()
            .cloned()
            .map(|mut evidence| {
                evidence.kind = "languageEventRegistration".to_string();
                evidence.value = display_name.clone();
                evidence
            })
            .into_iter()
            .collect();
        facts.entrypoints.push(Entrypoint {
            id,
            unit_id: call.source_unit_id,
            kind: event_kind,
            name: display_name,
            method: Some(method.to_string()),
            path: event_name,
            framework_id: None,
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

fn last_segment(value: &str) -> &str {
    value
        .rsplit_once('.')
        .map(|(_, value)| value)
        .or_else(|| value.rsplit_once("::").map(|(_, value)| value))
        .or_else(|| value.rsplit_once("->").map(|(_, value)| value))
        .unwrap_or(value)
}
