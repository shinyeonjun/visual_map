use crate::config::{DomainPolicy, PathPolicy};
use crate::facts::{CodeUnitKind, Evidence, FactStore};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// 도메인 후보를 뒷받침하는 신호의 종류다.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum DomainSignalKind {
    Path,
    Symbol,
    Entrypoint,
    Resource,
    Reference,
    Framework,
}

/// 코드 단위에서 도메인 후보로 향하는 하나의 신호다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainSignal {
    pub domain_key: String,
    pub unit_id: String,
    pub kind: DomainSignalKind,
    pub weight: u32,
    pub evidence: Vec<Evidence>,
}

/// Facts에서 도메인 후보 신호를 만든다.
pub fn collect(
    store: &FactStore,
    domain_policy: &DomainPolicy,
    path_policy: &PathPolicy,
) -> Vec<DomainSignal> {
    let mut signals = Vec::new();
    let mut anchor_keys = BTreeSet::new();
    let mut symbol_signals = Vec::new();
    let mut symbol_units: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for unit in store.units.values() {
        if path_policy.is_test_path(&unit.relative_path) {
            continue;
        }
        // 파일명은 `Builder`, `Utils`, `Actual`처럼 구현 세부 단어가 되기
        // 쉽다. 도메인 경로 신호는 디렉터리 구조에서만 만들고, 파일명·심볼은
        // 별도의 symbol 신호로 보수적으로 처리한다.
        let path_tokens = directory_tokens(&unit.relative_path, domain_policy);
        // 이름이 없는 람다를 위한 합성 이름(`<lambda@...>`)은 코드 구조에는
        // 필요하지만 비즈니스 도메인 후보가 되면 안 된다.
        let symbol_tokens = if unit.kind == CodeUnitKind::Lambda {
            Vec::new()
        } else {
            tokenize(&unit.name, domain_policy)
        };

        for token in path_tokens {
            anchor_keys.insert(token.clone());
            signals.push(DomainSignal {
                domain_key: token,
                unit_id: unit.id.clone(),
                kind: DomainSignalKind::Path,
                weight: domain_policy.path_weight,
                evidence: vec![Evidence::new(
                    "path",
                    unit.relative_path.clone(),
                    unit.span.clone(),
                )],
            });
        }
        for token in symbol_tokens {
            symbol_units
                .entry(token.clone())
                .or_default()
                .insert(unit.id.clone());
            symbol_signals.push(DomainSignal {
                domain_key: token,
                unit_id: unit.id.clone(),
                kind: DomainSignalKind::Symbol,
                weight: domain_policy.symbol_weight,
                evidence: vec![Evidence::new(
                    "symbol",
                    unit.name.clone(),
                    unit.span.clone(),
                )],
            });
        }
    }

    for entrypoint in &store.entrypoints {
        if store
            .unit(&entrypoint.unit_id)
            .map(|unit| path_policy.is_test_path(&unit.relative_path))
            .unwrap_or(false)
        {
            continue;
        }
        let tokens = format!(
            "{} {} {}",
            entrypoint.name,
            entrypoint.path.as_deref().unwrap_or_default(),
            entrypoint.method.as_deref().unwrap_or_default()
        );
        for token in tokenize(&tokens, domain_policy) {
            anchor_keys.insert(token.clone());
            signals.push(DomainSignal {
                domain_key: token,
                unit_id: entrypoint.unit_id.clone(),
                kind: DomainSignalKind::Entrypoint,
                weight: domain_policy.entrypoint_weight,
                evidence: entrypoint.evidence.clone(),
            });
        }
    }

    for resource in &store.resources {
        if store
            .unit(&resource.unit_id)
            .map(|unit| path_policy.is_test_path(&unit.relative_path))
            .unwrap_or(false)
        {
            continue;
        }
        for token in tokenize(&resource.name, domain_policy) {
            anchor_keys.insert(token.clone());
            signals.push(DomainSignal {
                domain_key: token,
                unit_id: resource.unit_id.clone(),
                kind: DomainSignalKind::Resource,
                weight: domain_policy.resource_weight,
                evidence: resource.evidence.clone(),
            });
        }
    }

    // 프레임워크 이름 자체를 비즈니스 도메인으로 만들지는 않는다. 다만 같은
    // 파일 경로에 이미 존재하는 도메인 후보를 프레임워크 사실로 보강한다.
    for framework in &store.frameworks {
        for evidence in &framework.evidence {
            let Some(unit) = store
                .units
                .values()
                .find(|unit| unit.file_id == evidence.span.file_id)
            else {
                continue;
            };
            for token in tokenize(&framework.display_name, domain_policy) {
                if anchor_keys.contains(&token) {
                    signals.push(DomainSignal {
                        domain_key: token,
                        unit_id: unit.id.clone(),
                        kind: DomainSignalKind::Framework,
                        weight: domain_policy.framework_weight,
                        evidence: vec![evidence.clone()],
                    });
                }
            }
        }
    }

    // 함수 이름의 일반 동사나 구현 세부 용어가 도메인으로 승격되는 것을 막는다.
    // 파일 경로·진입점·자원에 이미 나타나거나 여러 코드 단위에서 반복된 심볼만
    // 도메인 후보 신호로 사용한다.
    signals.extend(symbol_signals.into_iter().filter(|signal| {
        anchor_keys.contains(&signal.domain_key)
            || symbol_units
                .get(&signal.domain_key)
                .map(|units| units.len() >= domain_policy.minimum_repeated_symbol_units)
                .unwrap_or(false)
    }));

    let candidate_keys: BTreeMap<String, usize> =
        signals.iter().fold(BTreeMap::new(), |mut map, signal| {
            *map.entry(signal.domain_key.clone()).or_insert(0) += 1;
            map
        });
    let known_keys: Vec<String> = candidate_keys
        .into_iter()
        .filter(|(key, count)| *count >= 1 && !domain_policy.is_generic(key))
        .map(|(key, _)| key)
        .collect();

    super::reference_signals::append(store, domain_policy, &known_keys, &mut signals);

    signals.retain(|signal| !domain_policy.is_generic(&signal.domain_key));
    signals
}

pub fn tokenize(value: &str, domain_policy: &DomainPolicy) -> Vec<String> {
    let mut normalized = String::with_capacity(value.len() + 8);
    let mut previous_is_lower = false;
    for character in value.chars() {
        if character.is_uppercase() && previous_is_lower {
            normalized.push(' ');
        }
        if character.is_alphanumeric() {
            normalized.push(character.to_ascii_lowercase());
        } else {
            normalized.push(' ');
        }
        previous_is_lower = character.is_lowercase() || character.is_ascii_digit();
    }

    normalized
        .split_whitespace()
        .filter(|token| {
            token.len() >= domain_policy.minimum_token_length && !domain_policy.is_generic(token)
        })
        .map(str::to_string)
        .collect()
}

fn directory_tokens(path: &str, domain_policy: &DomainPolicy) -> Vec<String> {
    let normalized = path.replace('\\', "/");
    let directory = normalized.rsplit_once('/').map(|(directory, _)| directory);
    directory
        .map(|directory| tokenize(directory, domain_policy))
        .unwrap_or_default()
}
