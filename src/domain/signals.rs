use crate::config::DomainPolicy;
use serde::{Deserialize, Serialize};

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

pub fn tokenize(value: &str, domain_policy: &DomainPolicy) -> Vec<String> {
    tokenize_with_generic(value, domain_policy)
        .into_iter()
        .filter(|token| !domain_policy.is_generic(token))
        .collect()
}

pub(super) fn tokenize_with_generic(value: &str, domain_policy: &DomainPolicy) -> Vec<String> {
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
        .filter(|token| token.len() >= domain_policy.minimum_token_length)
        .map(str::to_string)
        .collect()
}
