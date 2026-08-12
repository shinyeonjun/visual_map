//! Python decorator route를 프레임워크 공통 HTTP/WebSocket 사실로 변환한다.

use super::arguments::string_literal;
use super::{join_paths, methods_argument, route_path, SymbolIndex};
use crate::facts::{Entrypoint, EntrypointKind, FactStore};
use crate::languages::common::metadata::stable_id;
use std::collections::HashMap;

/// 한 프레임워크가 어떤 decorator와 router 등록 함수를 사용하는지 정의한다.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RoutePolicy {
    pub(crate) framework_id: &'static str,
    pub(crate) constructors: &'static [&'static str],
    pub(crate) registrations: &'static [&'static str],
    pub(crate) route_names: &'static [&'static str],
    pub(crate) websocket_names: &'static [&'static str],
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct PrefixValue {
    path: String,
    dynamic: bool,
}

/// 특정 프레임워크 파일의 decorator를 공통 HTTP/WebSocket 진입점으로 변환한다.
pub(crate) fn add_routes(
    facts: &mut FactStore,
    file_frameworks: &HashMap<String, Vec<String>>,
    policy: RoutePolicy,
) {
    if !file_frameworks
        .values()
        .any(|frameworks| frameworks.iter().any(|id| id == policy.framework_id))
    {
        return;
    }
    let symbol_index = SymbolIndex::new(facts);
    let route_symbols = facts
        .call_sites
        .iter()
        .filter_map(|call| {
            let constructor = call.callee.rsplit('.').next().unwrap_or(&call.callee);
            let file_id = facts.unit(&call.source_unit_id)?.file_id.clone();
            if !policy.constructors.contains(&constructor)
                && !symbol_index.imported_alias_matches(&file_id, constructor, policy.constructors)
            {
                return None;
            }
            let local_name = call.assigned_name.as_deref()?;
            Some(symbol_index.route_symbol(&file_id, local_name))
        })
        .collect::<Vec<_>>();
    let prefixes = router_prefixes(facts, &symbol_index, &route_symbols, policy);
    let mut additions = Vec::new();

    for decorator in &facts.decorators {
        let Some(unit) = facts.unit(&decorator.unit_id) else {
            continue;
        };
        let Some(frameworks) = file_frameworks.get(unit.file_id.as_str()) else {
            continue;
        };
        if !frameworks.iter().any(|id| id == policy.framework_id) {
            continue;
        }

        let name = decorator.name.to_ascii_lowercase();
        let is_websocket = policy
            .websocket_names
            .iter()
            .any(|candidate| *candidate == name);
        let is_route = is_websocket
            || policy
                .route_names
                .iter()
                .any(|candidate| *candidate == name);
        if !is_route {
            continue;
        }

        let path = route_path(&decorator.arguments);
        let prefix = decorator
            .receiver
            .as_deref()
            .and_then(|receiver| {
                symbol_index
                    .resolve(&unit.file_id, receiver, &route_symbols)
                    .and_then(|key| prefixes.get(&key))
            })
            .cloned()
            .unwrap_or_default();
        let full_path = (!prefix.dynamic)
            .then(|| path.as_deref().map(|path| join_paths(&prefix.path, path)))
            .flatten();
        let method = if is_websocket {
            "WEBSOCKET".to_string()
        } else if matches!(name.as_str(), "route" | "api_route") {
            methods_argument(&decorator.arguments).unwrap_or_else(|| "HTTP".to_string())
        } else {
            name.to_ascii_uppercase()
        };
        let display_path = full_path.clone().unwrap_or_else(|| {
            if prefix.dynamic {
                let partial_path = path
                    .as_deref()
                    .map(|path| join_paths(&prefix.path, path))
                    .unwrap_or_else(|| join_paths(&prefix.path, "<dynamic>"));
                join_paths("<dynamic>", &partial_path)
            } else {
                join_paths(&prefix.path, "<dynamic>")
            }
        });
        let id = stable_id(
            "entry",
            &format!(
                "{}:{}:{}:{}",
                decorator.id, method, display_path, policy.framework_id
            ),
        );
        if facts.entrypoints.iter().any(|entry| entry.id == id)
            || additions.iter().any(|entry: &Entrypoint| entry.id == id)
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
            path: full_path,
            framework_id: Some(policy.framework_id.to_string()),
            evidence: decorator
                .evidence
                .iter()
                .cloned()
                .map(|mut evidence| {
                    evidence.kind = "frameworkRoute".into();
                    evidence.value = display_path.clone();
                    evidence
                })
                .collect(),
        });
    }

    facts.entrypoints.extend(additions);
}

fn router_prefixes(
    facts: &FactStore,
    symbol_index: &SymbolIndex,
    route_symbols: &[super::symbols::RouteSymbol],
    policy: RoutePolicy,
) -> HashMap<String, PrefixValue> {
    let mut prefixes = HashMap::new();
    for call in &facts.call_sites {
        let constructor = call.callee.rsplit('.').next().unwrap_or(&call.callee);
        let Some(file_id) = facts
            .unit(&call.source_unit_id)
            .map(|unit| unit.file_id.as_str())
        else {
            continue;
        };
        if !policy.constructors.contains(&constructor)
            && !symbol_index.imported_alias_matches(file_id, constructor, policy.constructors)
        {
            continue;
        }
        let Some(local_name) = call.assigned_name.as_deref() else {
            continue;
        };
        let key = symbol_index.route_symbol(file_id, local_name).key;
        prefixes.insert(key, prefix_value(&call.arguments));
    }

    // registration이 등록된 router의 prefix를 다른 파일에서 합성할 수 있도록
    // constructor 수만큼 반복한다. 대부분의 프로젝트는 한 번이면 끝나지만,
    // router를 다시 mount하는 계층도 순서에 의존하지 않게 한다.
    for _ in 0..route_symbols.len().max(1) {
        let mut changed = false;
        for call in &facts.call_sites {
            let constructor = call.callee.rsplit('.').next().unwrap_or(&call.callee);
            if !policy.registrations.contains(&constructor) {
                continue;
            }
            let Some(router_name) = call.arguments.first().and_then(|argument| {
                let argument = argument.trim();
                (!argument.contains('=')).then_some(argument)
            }) else {
                continue;
            };
            let Some(file_id) = facts
                .unit(&call.source_unit_id)
                .map(|unit| unit.file_id.as_str())
            else {
                continue;
            };
            let Some(key) = symbol_index.resolve(file_id, router_name, route_symbols) else {
                continue;
            };
            let prefix = prefix_value(&call.arguments);
            if prefix == PrefixValue::default() {
                continue;
            }
            let current = prefixes.get(&key).cloned().unwrap_or_default();
            let combined = combine_prefixes(&prefix, &current);
            if prefixes.get(&key) != Some(&combined) {
                prefixes.insert(key, combined);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    prefixes
}

fn prefix_value(arguments: &[String]) -> PrefixValue {
    for argument in arguments {
        let Some((name, value)) = argument.split_once('=') else {
            continue;
        };
        if !matches!(name.trim(), "prefix" | "url_prefix") {
            continue;
        }
        return PrefixValue {
            path: string_literal(value.trim()).unwrap_or_default(),
            dynamic: string_literal(value.trim()).is_none(),
        };
    }
    PrefixValue::default()
}

fn combine_prefixes(outer: &PrefixValue, inner: &PrefixValue) -> PrefixValue {
    PrefixValue {
        path: join_paths(&outer.path, &inner.path),
        dynamic: outer.dynamic || inner.dynamic,
    }
}
