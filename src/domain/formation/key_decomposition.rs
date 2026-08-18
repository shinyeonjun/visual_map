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
        if segment
            .chars()
            .any(|ch| ch.is_ascii_uppercase())
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

const COMPOUND_SUFFIXES: &[&str] = &[
    "presigned", "administrator", "promotion", "collection", "customer", "payment", "shipping",
    "method", "order", "draft", "account", "product", "session", "report", "contact", "thread",
    "password", "recovery",
];

/// compound/normalized concept를 atomic token family로 분해한다.
pub fn atomize_concept_label(label: &str) -> Vec<String> {
    atomize_concept_label_detailed(label)
        .tokens
        .into_iter()
        .map(|token| token.token)
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConceptNormalizationDiagnostic {
    pub original_token: String,
    pub normalized_token: String,
    pub transformation_rule: String,
    pub noise_candidate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedLexicalToken {
    pub token: String,
    pub diagnostic: Option<ConceptNormalizationDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomizedConceptDetail {
    pub tokens: Vec<NormalizedLexicalToken>,
    pub diagnostics: Vec<ConceptNormalizationDiagnostic>,
}

const NON_SINGULAR_SUFFIX_TOKENS: &[&str] = &["status", "class", "process", "address", "business"];
const TECHNICAL_NOISE_TOKENS: &[&str] = &[
    "html", "css", "json", "xml", "http", "https", "rpc", "graphql", "websocket", "endpoint",
    "endpoints", "middleware", "infrastructure", "framework",
];
const FRAMEWORK_ROLE_TOKENS: &[&str] = &[
    "controller", "service", "resolver", "handler", "repository", "gateway", "interceptor",
    "listener", "adapter", "provider", "manager", "helper", "util", "utils",
];

pub fn atomize_concept_label_detailed(label: &str) -> AtomizedConceptDetail {
    let mut tokens = Vec::new();
    for token in tokenize_capability_key(label) {
        tokens.extend(split_alnum_boundaries(&token));
    }
    if tokens.len() <= 1 {
        let compact = compact_key(label);
        tokens = split_alnum_boundaries(&compact);
        if tokens.len() <= 1 {
            tokens = split_compound_suffixes(&compact);
        }
    }
    let coalesced = coalesce_short_digit_tokens(tokens);
    let mut diagnostics = Vec::new();
    let normalized = coalesced
        .into_iter()
        .filter(|token| token.len() >= 2 || token.chars().any(|ch| ch.is_ascii_digit()))
        .map(|token| normalize_lexical_token(&token))
        .collect::<Vec<_>>();
    for token in &normalized {
        if let Some(diagnostic) = &token.diagnostic {
            diagnostics.push(diagnostic.clone());
        }
    }
    AtomizedConceptDetail {
        tokens: normalized,
        diagnostics,
    }
}

pub fn normalize_lexical_token(token: &str) -> NormalizedLexicalToken {
    let original = token.to_ascii_lowercase();
    if is_technical_noise_candidate(&original) {
        return NormalizedLexicalToken {
            token: original.clone(),
            diagnostic: Some(ConceptNormalizationDiagnostic {
                original_token: original.clone(),
                normalized_token: original,
                transformation_rule: "technicalNoiseCandidate".into(),
                noise_candidate: true,
            }),
        };
    }

    if is_safe_duplicate_plural_candidate(&original) {
        let corrected = original[..original.len() - 1].to_string();
        if corrected.ends_with('s') {
            return NormalizedLexicalToken {
                token: corrected.clone(),
                diagnostic: Some(ConceptNormalizationDiagnostic {
                    original_token: original,
                    normalized_token: corrected,
                    transformation_rule: "duplicatePluralSuffix".into(),
                    noise_candidate: false,
                }),
            };
        }
    }

    NormalizedLexicalToken {
        token: original.clone(),
        diagnostic: Some(ConceptNormalizationDiagnostic {
            original_token: original.clone(),
            normalized_token: original,
            transformation_rule: "identity".into(),
            noise_candidate: false,
        }),
    }
}

fn is_technical_noise_candidate(token: &str) -> bool {
    TECHNICAL_NOISE_TOKENS.contains(&token)
        || FRAMEWORK_ROLE_TOKENS.contains(&token)
        || token.ends_with("endpoint")
}

fn is_safe_duplicate_plural_candidate(token: &str) -> bool {
    if NON_SINGULAR_SUFFIX_TOKENS.contains(&token) {
        return false;
    }
    if !token.ends_with("ss") || token.len() < 5 {
        return false;
    }
    let drop_one = &token[..token.len() - 1];
    let drop_two = &token[..token.len() - 2];
    drop_one.ends_with('s') && !drop_two.ends_with('s') && drop_two.len() >= 3
}

pub fn normalized_root_concept(label: &str) -> (String, Vec<ConceptNormalizationDiagnostic>) {
    let detail = atomize_concept_label_detailed(label);
    let root = detail
        .tokens
        .first()
        .map(|token| token.token.clone())
        .unwrap_or_else(|| compact_key(label));
    (root, detail.diagnostics)
}

fn coalesce_short_digit_tokens(tokens: Vec<String>) -> Vec<String> {
    let mut merged = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        if index + 1 < tokens.len()
            && tokens[index].len() == 1
            && tokens[index + 1].chars().all(|ch| ch.is_ascii_digit())
        {
            merged.push(format!("{}{}", tokens[index], tokens[index + 1]));
            index += 2;
            continue;
        }
        merged.push(tokens[index].clone());
        index += 1;
    }
    merged
}

fn split_alnum_boundaries(compact: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut prev_digit: Option<bool> = None;
    for ch in compact.chars() {
        let is_digit = ch.is_ascii_digit();
        if prev_digit == Some(is_digit) || prev_digit.is_none() {
            current.push(ch);
        } else {
            if !current.is_empty() {
                tokens.push(current.to_ascii_lowercase());
            }
            current.clear();
            current.push(ch);
        }
        prev_digit = Some(is_digit);
    }
    if !current.is_empty() {
        tokens.push(current.to_ascii_lowercase());
    }
    tokens
}

fn split_compound_suffixes(compact: &str) -> Vec<String> {
    let mut remaining = compact.to_string();
    let mut suffixes = Vec::new();
    loop {
        let mut matched = None;
        for suffix in COMPOUND_SUFFIXES {
            if remaining.len() > suffix.len() + 1 {
                if let Some(prefix) = remaining.strip_suffix(suffix) {
                    matched = Some((prefix.to_string(), suffix.to_string()));
                    break;
                }
            }
        }
        let Some((prefix, suffix)) = matched else {
            break;
        };
        suffixes.push(suffix);
        remaining = prefix;
    }
    let mut tokens = Vec::new();
    if !remaining.is_empty() {
        tokens.push(remaining);
    }
    suffixes.reverse();
    tokens.extend(suffixes);
    if tokens.is_empty() {
        vec![compact.to_string()]
    } else {
        tokens
    }
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

    #[test]
    fn duplicate_plural_artifact를_교정한다() {
        let normalized = normalize_lexical_token("filess");
        assert_eq!(normalized.token, "files");
        assert_eq!(
            normalized
                .diagnostic
                .as_ref()
                .map(|value| value.transformation_rule.as_str()),
            Some("duplicatePluralSuffix")
        );
    }

    #[test]
    fn settings는_잘못된_suffix_strip을_적용하지_않는다() {
        let normalized = normalize_lexical_token("settings");
        assert_eq!(normalized.token, "settings");
        assert_eq!(
            normalized
                .diagnostic
                .as_ref()
                .map(|value| value.transformation_rule.as_str()),
            Some("identity")
        );
    }

    #[test]
    fn html은_noise_candidate로_기록한다() {
        let normalized = normalize_lexical_token("html");
        assert!(normalized
            .diagnostic
            .as_ref()
            .is_some_and(|value| value.noise_candidate));
    }

    #[test]
    fn compound_concept를_atomic_family로_분해한다() {
        assert_eq!(
            atomize_concept_label("FilesS3Presigned"),
            vec!["files", "s3", "presigned"]
        );
        assert_eq!(
            atomize_concept_label("ShippingMethod"),
            vec!["shipping", "method"]
        );
        assert_eq!(
            atomize_concept_label("PaymentMethod"),
            vec!["payment", "method"]
        );
        assert_eq!(atomize_concept_label("DraftOrder"), vec!["draft", "order"]);
    }
}
