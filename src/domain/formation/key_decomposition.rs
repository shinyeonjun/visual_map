//! capability 키의 action/entity 분해.

use serde::{Deserialize, Serialize};

const ACTION_PREFIXES: &[&str] = &[
    "authenticate",
    "authorize",
    "register",
    "configure",
    "confirm",
    "download",
    "upload",
    "remove",
    "adjust",
    "create",
    "update",
    "delete",
    "reset",
    "password",
    "recover",
    "search",
    "getall",
    "list",
    "index",
    "show",
    "edit",
    "start",
    "stop",
    "add",
    "get",
    "set",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct KeyDecomposition {
    pub raw: String,
    pub action: Option<String>,
    pub entity: Option<String>,
}

pub fn tokenize_capability_key(key: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for segment in key.split(|c| c == '-' || c == '_' || c == '/') {
        if segment.is_empty() {
            continue;
        }
        if segment.chars().any(|ch| ch.is_ascii_uppercase())
            && segment.chars().any(|ch| ch.is_ascii_lowercase())
        {
            tokens.extend(split_camel_case(segment));
        } else {
            tokens.push(segment.to_ascii_lowercase());
        }
    }
    tokens
}

fn split_camel_case(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut prev_kind: Option<char> = None;
    for ch in value.chars() {
        let kind = if ch.is_ascii_digit() {
            'd'
        } else if ch.is_ascii_uppercase() {
            'u'
        } else {
            'l'
        };
        if !current.is_empty()
            && prev_kind.is_some()
            && prev_kind != Some(kind)
            && !(prev_kind == Some('u') && kind == 'l')
        {
            tokens.push(current.to_ascii_lowercase());
            current.clear();
        }
        current.push(ch);
        prev_kind = Some(kind);
    }
    if !current.is_empty() {
        tokens.push(current.to_ascii_lowercase());
    }
    tokens
}

pub fn decompose_capability_key(key: &str) -> KeyDecomposition {
    let raw = key.to_string();
    let tokens = tokenize_capability_key(key);
    if tokens.is_empty() {
        return KeyDecomposition::default();
    }

    for action in ACTION_PREFIXES {
        if let Some(rest) = compact_key(key).strip_prefix(action) {
            if !rest.is_empty() {
                return KeyDecomposition {
                    raw,
                    action: Some(action.to_string()),
                    entity: Some(rest.to_string()),
                };
            }
        }
    }

    for prefix_len in (1..=tokens.len()).rev() {
        let prefix = tokens[..prefix_len].join("");
        if ACTION_PREFIXES.contains(&prefix.as_str()) {
            let entity = tokens[prefix_len..].join("");
            if !entity.is_empty() {
                return KeyDecomposition {
                    raw,
                    action: Some(prefix),
                    entity: Some(entity),
                };
            }
        }
    }

    KeyDecomposition {
        raw,
        action: None,
        entity: Some(tokens.join("")),
    }
}

fn compact_key(key: &str) -> String {
    key.replace(['-', '_', '/'], "").to_ascii_lowercase()
}

pub fn entity_family_match(left_key: &str, right_key: &str) -> bool {
    keys_share_entity(left_key, right_key) || token_entity_overlap(left_key, right_key)
}

pub fn keys_share_entity(left: &str, right: &str) -> bool {
    let left = decompose_capability_key(left);
    let right = decompose_capability_key(right);
    match (left.entity.as_deref(), right.entity.as_deref()) {
        (Some(left_entity), Some(right_entity)) if !left_entity.is_empty() => {
            left_entity == right_entity
                || left_entity.contains(right_entity)
                || right_entity.contains(left_entity)
        }
        _ => false,
    }
}

fn token_entity_overlap(left_key: &str, right_key: &str) -> bool {
    let left_tokens = entity_tokens(left_key);
    let right_tokens = entity_tokens(right_key);
    if left_tokens.is_empty() || right_tokens.is_empty() {
        return false;
    }
    left_tokens
        .iter()
        .any(|token| token.len() >= 3 && right_tokens.contains(token))
}

fn entity_tokens(key: &str) -> Vec<String> {
    let decomposition = decompose_capability_key(key);
    let action = decomposition.action.as_deref().unwrap_or("");
    tokenize_capability_key(key)
        .into_iter()
        .filter(|token| token != action && token.len() >= 3)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendure_administrator_계열을_분해한다() {
        let value = decompose_capability_key("createadministrator");
        assert_eq!(value.action.as_deref(), Some("create"));
        assert_eq!(value.entity.as_deref(), Some("administrator"));
    }

    #[test]
    fn hyphen_키는_세그먼트로_분해한다() {
        let value = decompose_capability_key("password-recovery");
        assert_eq!(value.action.as_deref(), Some("password"));
        assert_eq!(value.entity.as_deref(), Some("recovery"));
    }

    #[test]
    fn snake_case_키는_action_entity로_분해한다() {
        let value = decompose_capability_key("start_live_audio_stream");
        assert_eq!(value.action.as_deref(), Some("start"));
        assert_eq!(value.entity.as_deref(), Some("liveaudiostream"));
    }
}
