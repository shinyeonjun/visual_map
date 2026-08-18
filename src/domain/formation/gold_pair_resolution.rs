//! gold expected capability key를 actual capability에 매핑한다.

use super::key_decomposition::{decompose_capability_key, keys_share_entity};
use crate::domain::capability_keys::canonical_capability_key;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CapabilityKeyUnmatchedReason {
    CapabilityNotFound,
}

impl CapabilityKeyUnmatchedReason {
    pub fn label(&self) -> &'static str {
        match self {
            Self::CapabilityNotFound => "capabilityNotFound",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityKeyResolution {
    pub expected_key: String,
    pub resolved_key: Option<String>,
    pub matched: bool,
    pub unmatched_reason: Option<String>,
    pub candidate_actual_keys: Vec<String>,
}

pub fn resolve_capability_key(expected: &str, actual_keys: &[String]) -> CapabilityKeyResolution {
    let expected = canonical_capability_key(expected);
    if actual_keys.iter().any(|key| key == &expected) {
        return CapabilityKeyResolution {
            expected_key: expected.clone(),
            resolved_key: Some(expected),
            matched: true,
            unmatched_reason: None,
            candidate_actual_keys: Vec::new(),
        };
    }

    let candidates = candidate_actual_keys(&expected, actual_keys);
    CapabilityKeyResolution {
        expected_key: expected,
        resolved_key: None,
        matched: false,
        unmatched_reason: Some(
            CapabilityKeyUnmatchedReason::CapabilityNotFound
                .label()
                .into(),
        ),
        candidate_actual_keys: candidates,
    }
}

pub fn candidate_actual_keys(expected: &str, actual_keys: &[String]) -> Vec<String> {
    let mut ranked: Vec<(String, u32)> = actual_keys
        .iter()
        .map(|actual| (actual.clone(), match_score(&expected, actual)))
        .filter(|(_, score)| *score > 0)
        .collect();
    ranked.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    ranked.into_iter().take(8).map(|(key, _)| key).collect()
}

fn match_score(expected: &str, actual: &str) -> u32 {
    let expected = canonical_capability_key(expected);
    let actual = canonical_capability_key(actual);
    if expected == actual {
        return 1_000;
    }
    if actual.contains(&expected) || expected.contains(&actual) {
        return 500;
    }
    if keys_share_entity(&expected, &actual) {
        return 300;
    }
    let expected_parts: Vec<_> = expected.split('-').collect();
    let actual_parts: Vec<_> = actual.split('-').collect();
    if expected_parts
        .iter()
        .any(|part| actual_parts.iter().any(|actual_part| part == actual_part))
    {
        return 200;
    }
    let expected_entity = decompose_capability_key(&expected)
        .entity
        .unwrap_or_default();
    let actual_entity = decompose_capability_key(&actual).entity.unwrap_or_default();
    if !expected_entity.is_empty()
        && (actual_entity.contains(&expected_entity) || expected_entity.contains(&actual_entity))
    {
        return 250;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match는_resolved된다() {
        let resolution = resolve_capability_key("login", &["login".into(), "users".into()]);
        assert!(resolution.matched);
        assert_eq!(resolution.resolved_key.as_deref(), Some("login"));
    }

    #[test]
    fn 누락된_키는_후보를_제안한다() {
        let resolution = resolve_capability_key(
            "administrator",
            &[
                "createadministrator".into(),
                "deleteadministrator".into(),
                "orders".into(),
            ],
        );
        assert!(!resolution.matched);
        assert!(resolution
            .candidate_actual_keys
            .iter()
            .any(|key| key.contains("administrator")));
    }
}
