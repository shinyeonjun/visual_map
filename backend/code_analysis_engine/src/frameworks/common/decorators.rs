//! 언어 공통 decorator 기반 route materializer.

use crate::facts::{Entrypoint, EntrypointKind, FactStore};
use crate::frameworks::registry::detector::FrameworkDetection;
use crate::languages::common::metadata::stable_id;
use std::collections::HashMap;

use super::FrameworkApplicabilityIndex;

#[derive(Debug, Clone)]
pub struct DecoratorRouteRule {
    pub framework_id: &'static str,
    pub controller_names: &'static [&'static str],
    pub route_names: &'static [&'static str],
    pub websocket_names: &'static [&'static str],
    /// `#[route("/users", method = "GET")]`처럼 인자에서 method를
    /// 읽어야 하는 경우의 인덱스다.
    pub method_argument_index: Option<usize>,
}

/// route 이외의 decorator 기반 외부 경계를 공통 entrypoint로 materialize하는 규칙이다.
#[derive(Debug, Clone)]
pub struct DecoratorEntrypointRule {
    pub framework_id: &'static str,
    pub receiver: Option<&'static str>,
    pub names: &'static [&'static str],
    pub kind: EntrypointKind,
    pub method: &'static str,
}

/// `#[tauri::command]`, `#[tokio::main]`처럼 함수에 붙은 attribute를
/// 이벤트/메인 진입점으로 보존한다.
pub fn add_decorator_entrypoints(
    facts: &mut FactStore,
    detections: &[FrameworkDetection],
    rule: DecoratorEntrypointRule,
) {
    if !detections
        .iter()
        .any(|detection| detection.id == rule.framework_id)
    {
        return;
    }
    let applicability = FrameworkApplicabilityIndex::new(detections);
    for decorator in facts.decorators.clone() {
        if !rule
            .names
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&decorator.name))
            || rule.receiver.is_some_and(|receiver| {
                !decorator
                    .receiver
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case(receiver))
            })
        {
            continue;
        }
        let Some(unit) = facts.unit(&decorator.unit_id) else {
            continue;
        };
        if !applicability.applies(facts, &decorator.unit_id, rule.framework_id) {
            continue;
        }
        let id = stable_id(
            "entry",
            &format!("{}:{}:{}", decorator.id, rule.framework_id, rule.method),
        );
        if facts.entrypoints.iter().any(|entrypoint| {
            entrypoint.id == id
                || (entrypoint.framework_id.as_deref() == Some(rule.framework_id)
                    && entrypoint.unit_id == decorator.unit_id
                    && entrypoint.kind == rule.kind)
        }) {
            continue;
        }
        facts.entrypoints.push(Entrypoint {
            id,
            unit_id: decorator.unit_id,
            kind: rule.kind.clone(),
            name: unit.name.clone(),
            method: Some(rule.method.to_string()),
            path: None,
            framework_id: Some(rule.framework_id.to_string()),
            evidence: decorator
                .evidence
                .into_iter()
                .map(|mut evidence| {
                    evidence.kind = "frameworkDecoratorEntrypoint".to_string();
                    evidence.value = unit.name.clone();
                    evidence
                })
                .collect(),
        });
    }
}

pub fn add_decorator_routes(
    facts: &mut FactStore,
    detections: &[FrameworkDetection],
    rule: DecoratorRouteRule,
) {
    if !detections
        .iter()
        .any(|detection| detection.id == rule.framework_id)
    {
        return;
    }
    let applicability = FrameworkApplicabilityIndex::new(detections);
    let controller_prefixes = facts
        .decorators
        .iter()
        .filter(|decorator| {
            rule.controller_names
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&decorator.name))
        })
        .filter_map(|decorator| {
            facts.unit(&decorator.unit_id)?;
            applicability
                .applies(facts, &decorator.unit_id, rule.framework_id)
                .then(|| {
                    (
                        decorator.unit_id.clone(),
                        literal(decorator.arguments.first().map(String::as_str)),
                    )
                })
        })
        .collect::<HashMap<_, _>>();

    let mut additions = Vec::new();
    for decorator in &facts.decorators {
        let Some(unit) = facts.unit(&decorator.unit_id) else {
            continue;
        };
        if !applicability.applies(facts, &decorator.unit_id, rule.framework_id) {
            continue;
        }
        let is_websocket = rule
            .websocket_names
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&decorator.name));
        let is_route = is_websocket
            || rule
                .route_names
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&decorator.name));
        if !is_route {
            continue;
        }
        let route = literal(decorator.arguments.first().map(String::as_str));
        let prefix = parent_prefix(facts, &controller_prefixes, &unit.id);
        let path = match (prefix.as_deref(), route.as_deref()) {
            (Some(prefix), Some(route)) => Some(join_paths(prefix, route)),
            (None, Some(route)) => Some(join_paths("", route)),
            // `[HttpGet]`처럼 메서드만 지정한 attribute는 controller
            // prefix 자체가 endpoint 경로가 된다. prefix도 없으면 공통
            // line parser가 만드는 기본 `/` route에 맡긴다.
            (Some(prefix), None) => Some(join_paths(prefix, "")),
            (None, None) => None,
        };
        let display_path = path.clone().unwrap_or_else(|| "<dynamic>".to_string());
        let method = if is_websocket {
            "WEBSOCKET".to_string()
        } else if let Some(index) = rule.method_argument_index {
            decorator
                .arguments
                .get(index)
                .and_then(|argument| method_argument(argument))
                .unwrap_or_else(|| decorator.name.to_ascii_uppercase())
        } else {
            decorator.name.to_ascii_uppercase()
        };
        let id = stable_id(
            "entry",
            &format!(
                "{}:{}:{}:{}",
                decorator.id, method, display_path, rule.framework_id
            ),
        );
        if facts.entrypoints.iter().any(|entrypoint| {
            entrypoint.id == id
                || (entrypoint.framework_id.as_deref() == Some(rule.framework_id)
                    && entrypoint.unit_id == decorator.unit_id
                    && entrypoint.kind
                        == if is_websocket {
                            EntrypointKind::WebSocket
                        } else {
                            EntrypointKind::Http
                        }
                    && entrypoint.path == path
                    && !is_generic_route(entrypoint))
        }) || additions
            .iter()
            .any(|entrypoint: &Entrypoint| entrypoint.id == id)
        {
            continue;
        }
        additions.push(Entrypoint {
            id,
            unit_id: decorator.unit_id.clone(),
            kind: if is_websocket {
                EntrypointKind::WebSocket
            } else {
                EntrypointKind::Http
            },
            name: display_path.clone(),
            method: Some(method),
            path,
            framework_id: Some(rule.framework_id.to_string()),
            evidence: decorator
                .evidence
                .iter()
                .cloned()
                .map(|mut evidence| {
                    evidence.kind = "frameworkDecoratorRoute".to_string();
                    evidence.value = display_path.clone();
                    evidence
                })
                .collect(),
        });
    }
    let generic_keys = additions
        .iter()
        .map(|entrypoint| {
            (
                entrypoint.unit_id.clone(),
                entrypoint.kind.clone(),
                entrypoint.path.clone(),
                entrypoint.method.clone(),
            )
        })
        .collect::<Vec<_>>();
    facts.entrypoints.retain(|entrypoint| {
        if entrypoint.framework_id.is_some() && !is_generic_route(entrypoint) {
            return true;
        }
        !generic_keys.iter().any(|(unit_id, kind, path, method)| {
            &entrypoint.unit_id == unit_id
                && &entrypoint.kind == kind
                && &entrypoint.path == path
                && &entrypoint.method == method
        })
    });
    facts.entrypoints.extend(additions);
}

fn is_generic_route(entrypoint: &Entrypoint) -> bool {
    entrypoint
        .evidence
        .first()
        .is_some_and(|evidence| evidence.kind == "route")
}

fn parent_prefix(
    facts: &FactStore,
    prefixes: &HashMap<String, Option<String>>,
    unit_id: &str,
) -> Option<String> {
    let mut current = facts.unit(unit_id)?.parent_id.clone();
    while let Some(parent_id) = current {
        if let Some(prefix) = prefixes.get(&parent_id).and_then(Clone::clone) {
            return Some(prefix);
        }
        current = facts
            .unit(&parent_id)
            .and_then(|unit| unit.parent_id.clone());
    }
    None
}

fn literal(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.len() < 2 {
        return None;
    }
    let first = value.chars().next()?;
    let last = value.chars().last()?;
    matches!((first, last), ('"', '"') | ('\'', '\''))
        .then(|| value[1..value.len() - 1].to_string())
}

fn join_paths(prefix: &str, path: &str) -> String {
    let prefix = prefix.trim_matches('/');
    let path = path.trim_matches('/');
    if prefix.is_empty() && path.is_empty() {
        "/".to_string()
    } else if prefix.is_empty() {
        format!("/{path}")
    } else if path.is_empty() {
        format!("/{prefix}")
    } else {
        format!("/{prefix}/{path}")
    }
}

fn method_argument(argument: &str) -> Option<String> {
    let (_, value) = argument.split_once('=')?;
    let value = value.trim();
    let value = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(value);
    let method = value
        .rsplit_once("::")
        .map(|(_, value)| value)
        .unwrap_or(value)
        .trim();
    (!method.is_empty()
        && method
            .chars()
            .all(|character| character.is_ascii_alphabetic()))
    .then(|| method.to_ascii_uppercase())
}
