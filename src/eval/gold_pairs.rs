//! gold 기준 positive/negative capability pair 추출.

use super::gold::{DomainAlias, EvalGold};
use crate::domain::capability_keys::canonical_capability_key;
use crate::domain::contract_path::{capability_key_from_path, contract_identity};
use crate::domain::keys_share_entity;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GoldPairKind {
    Positive,
    Negative,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoldPairLabel {
    pub left_key: String,
    pub right_key: String,
    pub kind: GoldPairKind,
    pub source: String,
}

pub fn extract_gold_pair_labels(gold: &EvalGold) -> (Vec<GoldPairLabel>, Vec<GoldPairLabel>) {
    let aliases = alias_map(gold);
    let feature_domains = feature_domain_keys(gold);
    let logical_domains = build_logical_domain_groups(gold, &aliases, &feature_domains);

    let mut positives = Vec::new();
    let mut negatives = Vec::new();
    let mut seen_positive: BTreeSet<(String, String, String)> = BTreeSet::new();
    let mut seen_negative = BTreeSet::new();

    for (domain, keys) in &feature_domains {
        push_positive_pairs(
            keys,
            &format!("sameDomain:{domain}"),
            &mut positives,
            &mut seen_positive,
        );
    }

    for domain in &gold.must_not_split {
        let domain = domain.to_ascii_lowercase();
        if let Some(keys) = logical_domains.get(&domain) {
            push_positive_pairs(
                keys,
                &format!("mustNotSplit:{domain}"),
                &mut positives,
                &mut seen_positive,
            );
        }
    }

    for alias_entry in &gold.domain_aliases {
        let domain = alias_entry.key.to_ascii_lowercase();
        if let Some(keys) = logical_domains.get(&domain) {
            push_positive_pairs(
                keys,
                &format!("domainAlias:{domain}"),
                &mut positives,
                &mut seen_positive,
            );
        }
    }

    for pair in &gold.must_not_merge {
        if pair.len() < 2 {
            continue;
        }
        let left_keys = capability_keys_for_domain(gold, &pair[0], &aliases, &feature_domains);
        let right_keys = capability_keys_for_domain(gold, &pair[1], &aliases, &feature_domains);
        push_cross_domain_pairs(
            &left_keys,
            &right_keys,
            &format!("mustNotMerge:{}-{}", pair[0], pair[1]),
            &mut negatives,
            &mut seen_negative,
        );
    }

    let domain_groups: Vec<BTreeSet<String>> = gold
        .must_have_domains
        .iter()
        .map(|domain| capability_keys_for_domain(gold, domain, &aliases, &feature_domains))
        .collect();
    for left_index in 0..domain_groups.len() {
        for right_index in (left_index + 1)..domain_groups.len() {
            push_cross_domain_pairs(
                &domain_groups[left_index],
                &domain_groups[right_index],
                &format!(
                    "differentDomain:{}-{}",
                    gold.must_have_domains[left_index], gold.must_have_domains[right_index]
                ),
                &mut negatives,
                &mut seen_negative,
            );
        }
    }

    (positives, negatives)
}

pub fn extract_actual_positive_pairs(
    gold: &EvalGold,
    actual_keys: &[String],
) -> Vec<GoldPairLabel> {
    let aliases = alias_map(gold);
    let feature_domains = feature_domain_keys(gold);
    let logical_domains = build_logical_domain_groups(gold, &aliases, &feature_domains);
    let mut positives = Vec::new();
    let mut seen = BTreeSet::new();

    for (domain, expected_keys) in &logical_domains {
        let actual_in_domain =
            actual_keys_for_logical_domain(domain, expected_keys, actual_keys, &aliases);
        if actual_in_domain.len() < 2 {
            continue;
        }
        let source = format!("actualDomain:{domain}");
        push_positive_pairs(&actual_in_domain, &source, &mut positives, &mut seen);
    }

    positives
}

fn feature_domain_keys(gold: &EvalGold) -> BTreeMap<String, BTreeSet<String>> {
    let mut by_domain: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for feature in &gold.must_have_features {
        let Some(domain) = feature.domain.as_deref() else {
            continue;
        };
        let Some(key) = contract_to_capability_key(&feature.contract) else {
            continue;
        };
        by_domain
            .entry(domain.to_ascii_lowercase())
            .or_default()
            .insert(key);
    }
    by_domain
}

fn build_logical_domain_groups(
    gold: &EvalGold,
    aliases: &BTreeMap<String, Vec<String>>,
    feature_domains: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut groups: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for (domain, keys) in feature_domains {
        groups
            .entry(domain.clone())
            .or_default()
            .extend(keys.iter().cloned());
    }

    for domain in gold
        .must_have_domains
        .iter()
        .chain(gold.must_not_split.iter())
    {
        insert_domain_names(&mut groups, domain, aliases, feature_domains);
    }

    for alias_entry in &gold.domain_aliases {
        insert_domain_names(&mut groups, &alias_entry.key, aliases, feature_domains);
        let domain = alias_entry.key.to_ascii_lowercase();
        let group = groups.entry(domain).or_default();
        group.insert(canonical_capability_key(&alias_entry.key));
        for alias in &alias_entry.aliases {
            group.insert(canonical_capability_key(alias));
        }
    }

    groups
}

fn insert_domain_names(
    groups: &mut BTreeMap<String, BTreeSet<String>>,
    domain_name: &str,
    aliases: &BTreeMap<String, Vec<String>>,
    feature_domains: &BTreeMap<String, BTreeSet<String>>,
) {
    let domain = domain_name.to_ascii_lowercase();
    let group = groups.entry(domain.clone()).or_default();
    group.insert(canonical_capability_key(domain_name));
    if let Some(names) = aliases.get(&domain) {
        for alias in names {
            group.insert(canonical_capability_key(alias));
        }
    }
    if let Some(keys) = feature_domains.get(&domain) {
        group.extend(keys.iter().cloned());
    }
}

fn actual_keys_for_logical_domain(
    domain: &str,
    expected_keys: &BTreeSet<String>,
    actual_keys: &[String],
    aliases: &BTreeMap<String, Vec<String>>,
) -> BTreeSet<String> {
    let mut matched = BTreeSet::new();
    let domain_names: BTreeSet<String> = std::iter::once(canonical_capability_key(domain))
        .chain(
            aliases
                .get(domain)
                .into_iter()
                .flatten()
                .map(|alias| canonical_capability_key(alias)),
        )
        .collect();

    for actual in actual_keys {
        let normalized = canonical_capability_key(actual);
        if expected_keys.contains(&normalized) || domain_names.contains(&normalized) {
            matched.insert(actual.clone());
            continue;
        }
        if expected_keys
            .iter()
            .any(|expected| keys_share_entity(expected, &normalized))
        {
            matched.insert(actual.clone());
            continue;
        }
        if domain_names
            .iter()
            .any(|name| keys_share_entity(name, &normalized))
        {
            matched.insert(actual.clone());
        }
    }
    matched
}

fn push_positive_pairs(
    keys: &BTreeSet<String>,
    source: &str,
    positives: &mut Vec<GoldPairLabel>,
    seen: &mut BTreeSet<(String, String, String)>,
) {
    for left in keys {
        for right in keys {
            if left >= right {
                continue;
            }
            if seen.insert((left.clone(), right.clone(), source.to_string())) {
                positives.push(GoldPairLabel {
                    left_key: left.clone(),
                    right_key: right.clone(),
                    kind: GoldPairKind::Positive,
                    source: source.to_string(),
                });
            }
        }
    }
}

fn push_cross_domain_pairs(
    left_keys: &BTreeSet<String>,
    right_keys: &BTreeSet<String>,
    source: &str,
    negatives: &mut Vec<GoldPairLabel>,
    seen: &mut BTreeSet<(String, String)>,
) {
    for left in left_keys {
        for right in right_keys {
            if left == right {
                continue;
            }
            let (left_key, right_key) = if left < right {
                (left.clone(), right.clone())
            } else {
                (right.clone(), left.clone())
            };
            if seen.insert((left_key.clone(), right_key.clone())) {
                negatives.push(GoldPairLabel {
                    left_key,
                    right_key,
                    kind: GoldPairKind::Negative,
                    source: source.to_string(),
                });
            }
        }
    }
}

fn capability_keys_for_domain(
    gold: &EvalGold,
    domain_name: &str,
    aliases: &BTreeMap<String, Vec<String>>,
    feature_domains: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeSet<String> {
    let domain = domain_name.to_ascii_lowercase();
    let mut keys = feature_domains.get(&domain).cloned().unwrap_or_default();
    keys.insert(canonical_capability_key(domain_name));
    if let Some(names) = aliases.get(&domain) {
        for alias in names {
            keys.insert(canonical_capability_key(alias));
        }
    }
    if keys.len() <= 1 {
        for feature in &gold.must_have_features {
            if feature
                .domain
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(domain_name))
            {
                if let Some(key) = contract_to_capability_key(&feature.contract) {
                    keys.insert(key);
                }
            }
        }
    }
    keys
}

fn alias_map(gold: &EvalGold) -> BTreeMap<String, Vec<String>> {
    gold.domain_aliases
        .iter()
        .map(|item: &DomainAlias| {
            (
                item.key.to_ascii_lowercase(),
                item.aliases
                    .iter()
                    .map(|alias| alias.to_ascii_lowercase())
                    .collect(),
            )
        })
        .collect()
}

pub fn contract_to_capability_key(contract: &str) -> Option<String> {
    let identity = normalize_gold_contract(contract)?;
    let path = identity
        .split_once(':')
        .map(|(_, path)| path)
        .unwrap_or(identity.as_str());
    capability_key_from_path(path).map(|key| canonical_capability_key(&key))
}

fn normalize_gold_contract(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if let Some((method, path)) = trimmed.split_once(' ') {
        return contract_identity(Some(method), path);
    }
    if let Some((method, path)) = trimmed.split_once(':') {
        if path.starts_with('/') {
            return contract_identity(Some(method), path);
        }
    }
    contract_identity(None, trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::gold::FeatureGold;

    fn fastapi_gold() -> EvalGold {
        EvalGold {
            must_have_domains: vec!["login".into(), "users".into(), "items".into()],
            domain_aliases: vec![DomainAlias {
                key: "login".into(),
                aliases: vec!["auth".into(), "password-recovery".into()],
            }],
            must_not_merge: vec![vec!["login".into(), "users".into()]],
            must_not_split: vec!["login".into(), "users".into(), "items".into()],
            must_have_features: vec![
                FeatureGold {
                    contract: "POST /api/v1/password-recovery/{email}".into(),
                    domain: Some("login".into()),
                },
                FeatureGold {
                    contract: "POST /api/v1/reset-password/".into(),
                    domain: Some("login".into()),
                },
                FeatureGold {
                    contract: "POST /api/v1/login/access-token".into(),
                    domain: Some("login".into()),
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn fastapi_gold에서_login_domain_positive_pair를_추출한다() {
        let (positives, negatives) = extract_gold_pair_labels(&fastapi_gold());
        assert!(positives.iter().any(|pair| {
            pair.left_key == "password-recovery" && pair.right_key == "reset-password"
        }));
        assert!(positives
            .iter()
            .any(|pair| pair.source.starts_with("mustNotSplit:login")));
        assert!(positives
            .iter()
            .any(|pair| pair.source.starts_with("domainAlias:login")));
        assert!(negatives
            .iter()
            .any(|pair| pair.source.starts_with("mustNotMerge:")));
    }

    #[test]
    fn actual_domain_positive_pair를_생성한다() {
        let positives = extract_actual_positive_pairs(
            &fastapi_gold(),
            &[
                "login".into(),
                "password-recovery".into(),
                "reset-password".into(),
                "users".into(),
            ],
        );
        assert!(positives
            .iter()
            .any(|pair| pair.source.starts_with("actualDomain:login")));
        assert!(positives.iter().any(|pair| {
            pair.left_key == "password-recovery" && pair.right_key == "reset-password"
        }));
    }

    #[test]
    fn contract_to_capability_key는_경로에서_키를_추출한다() {
        assert_eq!(
            contract_to_capability_key("POST /api/v1/users/me"),
            Some("users".into())
        );
    }
}
