//! 호출 인자로 전달되는 콜백을 정적 Callback 진입점으로 materialize한다.
//!
//! C 계열의 함수 포인터와 C++ 비동기 API는 decorator가 없기 때문에
//! `call_site.arguments`에 남은 함수 이름을 같은 파일/프로젝트의 유일한
//! 함수 유닛과 연결한다. 유일하게 확정할 수 없는 경우에는 추측해서 다른
//! 함수에 연결하지 않고 기존 동적·미해결 호출 사실만 보존한다.

use crate::facts::{Entrypoint, EntrypointKind, FactStore};
use crate::frameworks::registry::detector::FrameworkDetection;
use crate::languages::common::metadata::stable_id;
use std::collections::{HashMap, HashSet};

use super::FrameworkApplicabilityIndex;

#[derive(Debug, Clone)]
pub struct CallbackRegistrationRule {
    pub framework_id: &'static str,
    pub call_name: &'static str,
    pub callback_argument_indices: &'static [usize],
    pub method: &'static str,
}

/// 프레임워크 호출의 콜백 인자를 함수 유닛에 연결한다.
pub fn add_callback_registrations(
    facts: &mut FactStore,
    detections: &[FrameworkDetection],
    rule: CallbackRegistrationRule,
) {
    add_callback_registrations_many(facts, detections, std::slice::from_ref(&rule));
}

/// 같은 프레임워크의 여러 callback API를 한 번에 처리한다.
/// 호출 목록과 대상 유닛 인덱스를 규칙마다 다시 만들지 않는다.
pub fn add_callback_registrations_many(
    facts: &mut FactStore,
    detections: &[FrameworkDetection],
    rules: &[CallbackRegistrationRule],
) {
    let Some(framework_id) = rules.first().map(|rule| rule.framework_id) else {
        return;
    };
    if !rules.iter().all(|rule| rule.framework_id == framework_id)
        || !detections
            .iter()
            .any(|detection| detection.id == framework_id)
    {
        return;
    }
    let applicability = FrameworkApplicabilityIndex::new(detections);

    // 실제 callback 이름과 프레임워크 적용 범위가 맞는 호출만 복사한다.
    // 매칭이 하나도 없으면 대형 프로젝트의 유닛 인덱스도 만들지 않는다.
    let matching_calls = facts
        .call_sites
        .iter()
        .filter(|call| {
            applicability.applies(facts, &call.source_unit_id, framework_id)
                && rules.iter().any(|rule| {
                    rule.call_name
                        .eq_ignore_ascii_case(last_segment(&call.callee))
                })
        })
        .cloned()
        .collect::<Vec<_>>();
    if matching_calls.is_empty() {
        return;
    }

    let unit_index = CallbackUnitIndex::new(facts);
    let mut existing_ids = facts
        .entrypoints
        .iter()
        .map(|entrypoint| entrypoint.id.clone())
        .collect::<HashSet<_>>();

    for call in matching_calls {
        let call_name = last_segment(&call.callee);
        let Some(rule) = rules
            .iter()
            .find(|rule| rule.call_name.eq_ignore_ascii_case(call_name))
        else {
            continue;
        };

        for &argument_index in rule.callback_argument_indices {
            let Some(argument) = call.arguments.get(argument_index) else {
                continue;
            };
            let Some(target) = unit_index.resolve(&call.source_unit_id, argument) else {
                // 함수 포인터, lambda, std::bind 등은 정적 대상이 확정되지
                // 않으므로 오연결하지 않는다. 원래 Call/ Dynamic 사실은
                // pipeline에 이미 남아 있다.
                continue;
            };
            let id = stable_id(
                "entry",
                &format!(
                    "{}:{}:{}:{}",
                    call.id, rule.framework_id, argument_index, target.id
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
                    evidence.kind = "frameworkCallbackRegistration".to_string();
                    evidence.value = target.name.clone();
                    evidence
                })
                .into_iter()
                .collect();
            facts.entrypoints.push(Entrypoint {
                id,
                unit_id: target.id,
                kind: EntrypointKind::Callback,
                name: target.name,
                method: Some(rule.method.to_string()),
                path: None,
                framework_id: Some(rule.framework_id.to_string()),
                evidence,
            });
        }
    }
}

struct CallbackUnitIndex {
    file_by_source_unit: HashMap<String, String>,
    by_file_name: HashMap<(String, String), Vec<String>>,
    by_name: HashMap<String, Vec<String>>,
}

impl CallbackUnitIndex {
    fn new(facts: &FactStore) -> Self {
        let mut index = Self {
            file_by_source_unit: HashMap::new(),
            by_file_name: HashMap::new(),
            by_name: HashMap::new(),
        };
        for unit in facts.units.values() {
            if !matches!(
                unit.kind,
                crate::facts::CodeUnitKind::Function
                    | crate::facts::CodeUnitKind::Method
                    | crate::facts::CodeUnitKind::Constructor
                    | crate::facts::CodeUnitKind::Lambda
            ) {
                continue;
            }
            index
                .file_by_source_unit
                .insert(unit.id.clone(), unit.file_id.clone());
            let name = unit.name.to_ascii_lowercase();
            index
                .by_file_name
                .entry((unit.file_id.clone(), name.clone()))
                .or_default()
                .push(unit.id.clone());
            index.by_name.entry(name).or_default().push(unit.id.clone());
        }
        for candidates in index.by_file_name.values_mut() {
            candidates.sort();
            candidates.dedup();
        }
        for candidates in index.by_name.values_mut() {
            candidates.sort();
            candidates.dedup();
        }
        index
    }

    fn resolve(&self, source_unit_id: &str, argument: &str) -> Option<CallbackTarget> {
        let name = callback_name(argument)?;
        let file_id = self.file_by_source_unit.get(source_unit_id)?;
        if let Some(candidates) = self
            .by_file_name
            .get(&(file_id.clone(), name.to_ascii_lowercase()))
            .filter(|candidates| candidates.len() == 1)
        {
            return candidates.first().cloned().map(|id| CallbackTarget {
                id,
                name: name.clone(),
            });
        }
        let candidates = self.by_name.get(&name.to_ascii_lowercase())?;
        (candidates.len() == 1).then(|| CallbackTarget {
            id: candidates[0].clone(),
            name,
        })
    }
}

struct CallbackTarget {
    id: String,
    name: String,
}

fn callback_name(argument: &str) -> Option<String> {
    let mut value = argument.trim();
    while let Some(stripped) = value.strip_prefix('&').or_else(|| value.strip_prefix('*')) {
        value = stripped.trim_start();
    }
    if value.is_empty()
        || value.contains('(')
        || value.contains(')')
        || value.contains("=>")
        || value.contains("lambda")
        || value.contains("bind")
    {
        return None;
    }
    let value = value.trim_matches(|character: char| {
        matches!(character, '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';')
    });
    let value = value
        .rsplit_once("::")
        .map(|(_, name)| name)
        .or_else(|| value.rsplit_once("->").map(|(_, name)| name))
        .or_else(|| value.rsplit_once('.').map(|(_, name)| name))
        .unwrap_or(value)
        .trim();
    (!value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_'))
    .then(|| value.to_string())
}

fn last_segment(value: &str) -> &str {
    value
        .rsplit_once('.')
        .map(|(_, value)| value)
        .or_else(|| value.rsplit_once("::").map(|(_, value)| value))
        .or_else(|| value.rsplit_once("->").map(|(_, value)| value))
        .unwrap_or(value)
}
