use super::gold::{EvalGold, EvalMode};
use super::snapshot::{EvalSnapshot, SnapDomain};
use crate::domain::capability_keys::canonical_capability_key;
use crate::domain::contract_path::{contract_identity, normalize_contract_path};
use crate::domain::DomainKind;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

const FOLDER_DOMAIN_KEYS: &[&str] = &[
    "src", "lib", "app", "apps", "packages", "frontend", "backend", "web", "api", "core", "utils",
    "common", "python", "rust", "go",
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EvalFinding {
    pub layer: String,
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalReport {
    pub id: String,
    pub set: String,
    pub passed: bool,
    pub domain_hits: usize,
    pub domain_expected: usize,
    pub feature_hits: usize,
    pub feature_expected: usize,
    pub flow_hits: usize,
    pub flow_expected: usize,
    pub findings: Vec<EvalFinding>,
}

pub fn score_gold(gold: &EvalGold, snapshot: &EvalSnapshot) -> EvalReport {
    let mut findings = Vec::new();
    let aliases = alias_map(gold);

    let domain_hits = score_domains(gold, snapshot, &aliases, &mut findings);
    let feature_hits = score_features(gold, snapshot, &aliases, &mut findings);
    let flow_hits = score_flows(gold, snapshot, &mut findings);

    EvalReport {
        id: gold.id.clone(),
        set: gold.set.clone(),
        passed: findings.is_empty(),
        domain_hits,
        domain_expected: gold.must_have_domains.len(),
        feature_hits,
        feature_expected: gold.must_have_features.len(),
        flow_hits,
        flow_expected: gold.flow_invariants.len(),
        findings,
    }
}

fn score_domains(
    gold: &EvalGold,
    snapshot: &EvalSnapshot,
    aliases: &HashMap<String, Vec<String>>,
    findings: &mut Vec<EvalFinding>,
) -> usize {
    let mut hits = 0;
    for expected in &gold.must_have_domains {
        let matched = matching_domains(snapshot, expected, aliases);
        if matched.is_empty() {
            findings.push(finding(
                "domain",
                "missing",
                format!("필수 Domain '{expected}' 가 없습니다."),
            ));
        } else {
            hits += 1;
        }
    }

    for pair in &gold.must_not_merge {
        if pair.len() < 2 {
            continue;
        }
        let left = matching_domain_ids(snapshot, &pair[0], aliases);
        let right = matching_domain_ids(snapshot, &pair[1], aliases);
        if !left.is_empty() && !right.is_empty() && left.intersection(&right).next().is_some() {
            findings.push(finding(
                "domain",
                "overMerge",
                format!("'{}' 와 '{}' 가 한 Domain으로 합쳐졌습니다.", pair[0], pair[1]),
            ));
        }
    }

    for expected in &gold.must_not_split {
        if matching_domains(snapshot, expected, aliases).len() > 1 {
            findings.push(finding(
                "domain",
                "overSplit",
                format!("'{expected}' 가 여러 Domain 카드로 쪼개졌습니다."),
            ));
        }
    }

    for domain in &snapshot.domains {
        let key = canonical_capability_key(&domain.key).to_ascii_lowercase();
        if is_forbidden_domain_key(&key, gold) {
            findings.push(finding(
                "domain",
                "folderName",
                format!("폴더성 Domain key '{}' 가 올라왔습니다.", domain.key),
            ));
        }
        if gold.cross_cutting.iter().any(|item| item.eq_ignore_ascii_case(&key))
            && domain.kind == DomainKind::Business
        {
            findings.push(finding(
                "domain",
                "crossCuttingAsBusiness",
                format!("운영 경계 '{}' 가 비즈니스 Domain으로 올라왔습니다.", domain.key),
            ));
        }
        if gold.mode == EvalMode::Library
            && domain.kind == DomainKind::Business
            && !is_expected_domain(&key, gold, aliases)
        {
            findings.push(finding(
                "domain",
                "unexpectedBusiness",
                format!(
                    "라이브러리/CLI 에서 예상하지 않은 비즈니스 Domain '{}' 입니다.",
                    domain.key
                ),
            ));
        }
    }

    hits
}

fn score_features(
    gold: &EvalGold,
    snapshot: &EvalSnapshot,
    aliases: &HashMap<String, Vec<String>>,
    findings: &mut Vec<EvalFinding>,
) -> usize {
    let mut hits = 0;
    for expected in &gold.must_have_features {
        let Some(contract) = normalize_gold_contract(&expected.contract) else {
            findings.push(finding(
                "feature",
                "invalidContract",
                format!("정답 계약 '{}' 을 정규화하지 못했습니다.", expected.contract),
            ));
            continue;
        };
        let matches: Vec<_> = snapshot
            .features
            .iter()
            .filter(|feature| feature.contracts.iter().any(|item| contracts_match(item, &contract)))
            .collect();
        if matches.is_empty() {
            findings.push(finding(
                "feature",
                "missing",
                format!("필수 Feature '{contract}' 가 없습니다."),
            ));
            continue;
        }
        hits += 1;
        if matches.len() > 1 {
            findings.push(finding(
                "feature",
                "duplicate",
                format!("Feature '{contract}' 가 {}개로 중복되었습니다.", matches.len()),
            ));
        }
        if let Some(domain) = &expected.domain {
            let ok = matches.iter().any(|feature| {
                feature
                    .domain_keys
                    .iter()
                    .any(|key| keys_match(key, domain, aliases))
            });
            if !ok {
                findings.push(finding(
                    "feature",
                    "wrongDomain",
                    format!("Feature '{contract}' 가 Domain '{domain}' 에 붙지 않았습니다."),
                ));
            }
        }
    }
    hits
}

fn score_flows(gold: &EvalGold, snapshot: &EvalSnapshot, findings: &mut Vec<EvalFinding>) -> usize {
    let mut hits = 0;
    for expected in &gold.flow_invariants {
        let Some(contract) = normalize_gold_contract(&expected.feature_contract) else {
            findings.push(finding(
                "flow",
                "invalidContract",
                format!(
                    "정답 계약 '{}' 을 정규화하지 못했습니다.",
                    expected.feature_contract
                ),
            ));
            continue;
        };
        let kinds: HashSet<String> = snapshot
            .features
            .iter()
            .filter(|feature| feature.contracts.iter().any(|item| contracts_match(item, &contract)))
            .flat_map(|feature| &feature.flow_ids)
            .filter_map(|flow_id| snapshot.flows.get(flow_id))
            .flatten()
            .cloned()
            .collect();
        if kinds.is_empty() && !expected.must_have_edge_kinds.is_empty() {
            findings.push(finding(
                "flow",
                "missing",
                format!("Feature '{contract}' 에 실행 흐름이 없습니다."),
            ));
            continue;
        }
        let missing: Vec<_> = expected
            .must_have_edge_kinds
            .iter()
            .filter(|kind| !kinds.contains(kind.as_str()))
            .cloned()
            .collect();
        if missing.is_empty() {
            hits += 1;
        } else {
            findings.push(finding(
                "flow",
                "wrongEdge",
                format!("Feature '{contract}' 에 {missing:?} edge가 없습니다. 실제: {kinds:?}"),
            ));
        }
    }
    hits
}

fn matching_domains<'a>(
    snapshot: &'a EvalSnapshot,
    expected: &str,
    aliases: &HashMap<String, Vec<String>>,
) -> Vec<&'a SnapDomain> {
    snapshot
        .domains
        .iter()
        .filter(|domain| keys_match(&domain.key, expected, aliases))
        .collect()
}

fn matching_domain_ids(
    snapshot: &EvalSnapshot,
    expected: &str,
    aliases: &HashMap<String, Vec<String>>,
) -> HashSet<String> {
    matching_domains(snapshot, expected, aliases)
        .into_iter()
        .map(|domain| domain.id.clone())
        .collect()
}

fn keys_match(actual: &str, expected: &str, aliases: &HashMap<String, Vec<String>>) -> bool {
    let actual = canonical_capability_key(actual).to_ascii_lowercase();
    let expected = expected.to_ascii_lowercase();
    if actual == expected {
        return true;
    }
    aliases
        .get(&expected)
        .is_some_and(|names| names.iter().any(|alias| alias.eq_ignore_ascii_case(&actual)))
}

fn is_expected_domain(
    actual: &str,
    gold: &EvalGold,
    aliases: &HashMap<String, Vec<String>>,
) -> bool {
    gold.must_have_domains
        .iter()
        .any(|expected| keys_match(actual, expected, aliases))
}

fn alias_map(gold: &EvalGold) -> HashMap<String, Vec<String>> {
    gold.domain_aliases
        .iter()
        .map(|item| {
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

fn is_forbidden_domain_key(key: &str, gold: &EvalGold) -> bool {
    FOLDER_DOMAIN_KEYS
        .iter()
        .any(|forbidden| key.eq_ignore_ascii_case(forbidden))
        || gold
            .forbidden_domain_keys
            .iter()
            .any(|forbidden| forbidden.eq_ignore_ascii_case(key))
}

fn contracts_match(actual: &str, expected: &str) -> bool {
    if actual == expected {
        return true;
    }
    let Some((actual_method, actual_path)) = split_contract(actual) else {
        return false;
    };
    let Some((expected_method, expected_path)) = split_contract(expected) else {
        return false;
    };
    if !methods_compatible(actual_method, expected_method) {
        return false;
    }
    match (
        normalize_contract_path(actual_path),
        normalize_contract_path(expected_path),
    ) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

fn split_contract(value: &str) -> Option<(&str, &str)> {
    value
        .split_once(':')
        .filter(|(_, path)| path.starts_with('/'))
}

fn methods_compatible(left: &str, right: &str) -> bool {
    let left = left.to_ascii_uppercase();
    let right = right.to_ascii_uppercase();
    left == right
        || left == "*"
        || right == "*"
        || websocket_method(&left) && websocket_method(&right)
}

fn websocket_method(method: &str) -> bool {
    matches!(method, "WS" | "WSS" | "WEBSOCKET")
}

fn finding(layer: &str, kind: &str, message: String) -> EvalFinding {
    EvalFinding {
        layer: layer.into(),
        kind: kind.into(),
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::gold::{DomainAlias, FeatureGold, FlowInvariantGold};
    use super::super::snapshot::SnapFeature;
    use super::super::{EvalGold, EvalMode, EvalSnapshot};

    fn domain(id: &str, key: &str, kind: DomainKind) -> SnapDomain {
        SnapDomain {
            id: id.into(),
            key: key.into(),
            kind,
        }
    }

    fn feature(id: &str, contract: &str, domain_key: &str, flow_ids: &[&str]) -> SnapFeature {
        SnapFeature {
            id: id.into(),
            key: format!("contract:{contract}"),
            domain_keys: vec![domain_key.into()],
            contracts: vec![contract.into()],
            flow_ids: flow_ids.iter().map(|value| (*value).to_string()).collect(),
        }
    }

    fn gold_service() -> EvalGold {
        EvalGold {
            id: "fixture".into(),
            set: "test".into(),
            must_have_domains: vec!["auth".into(), "sessions".into()],
            domain_aliases: vec![DomainAlias {
                key: "sessions".into(),
                aliases: vec!["participants".into()],
            }],
            must_not_merge: vec![vec!["auth".into(), "sessions".into()]],
            must_not_split: vec!["sessions".into()],
            cross_cutting: vec!["health".into()],
            must_have_features: vec![FeatureGold {
                contract: "POST /api/v1/auth/login".into(),
                domain: Some("auth".into()),
            }],
            flow_invariants: vec![FlowInvariantGold {
                feature_contract: "POST /api/v1/auth/login".into(),
                must_have_edge_kinds: vec!["trueBranch".into(), "falseBranch".into()],
            }],
            ..EvalGold::default()
        }
    }

    #[test]
    fn 정답_스냅샷은_통과한다() {
        let snapshot = EvalSnapshot {
            domains: vec![
                domain("d-auth", "auth", DomainKind::Business),
                domain("d-sessions", "sessions", DomainKind::Business),
                domain("d-health", "health", DomainKind::CrossCutting),
            ],
            features: vec![feature(
                "f-login",
                "POST:/auth/login",
                "auth",
                &["flow-login"],
            )],
            flows: HashMap::from([(
                "flow-login".into(),
                vec!["trueBranch".into(), "falseBranch".into()],
            )]),
        };

        let report = score_gold(&gold_service(), &snapshot);
        assert!(report.passed, "{:?}", report.findings);
        assert_eq!(report.domain_hits, 2);
        assert_eq!(report.feature_hits, 1);
        assert_eq!(report.flow_hits, 1);
    }

    #[test]
    fn 필수_domain_누락과_over_merge_over_split을_잡는다() {
        let snapshot = EvalSnapshot {
            domains: vec![
                domain("d-sessions", "sessions", DomainKind::Business),
                domain("d-part", "participants", DomainKind::Business),
            ],
            features: Vec::new(),
            flows: HashMap::new(),
        };
        let mut gold = gold_service();
        gold.must_have_features.clear();
        gold.flow_invariants.clear();
        let report = score_gold(&gold, &snapshot);
        let kinds: Vec<_> = report.findings.iter().map(|item| item.kind.as_str()).collect();
        assert!(kinds.contains(&"missing"));
        assert!(kinds.contains(&"overSplit"));
    }

    #[test]
    fn 같은_id로_합쳐진_능력은_over_merge다() {
        let snapshot = EvalSnapshot {
            domains: vec![domain("d-one", "auth", DomainKind::Business)],
            features: Vec::new(),
            flows: HashMap::new(),
        };
        let gold = EvalGold {
            id: "merge".into(),
            must_have_domains: vec!["auth".into(), "sessions".into()],
            domain_aliases: vec![DomainAlias {
                key: "sessions".into(),
                aliases: vec!["auth".into()],
            }],
            must_not_merge: vec![vec!["auth".into(), "sessions".into()]],
            ..EvalGold::default()
        };
        let report = score_gold(&gold, &snapshot);
        assert!(report.findings.iter().any(|item| item.kind == "overMerge"));
    }

    #[test]
    fn 운영_경계와_폴더명_domain을_감점한다() {
        let snapshot = EvalSnapshot {
            domains: vec![
                domain("d-health", "health", DomainKind::Business),
                domain("d-src", "src", DomainKind::Unknown),
            ],
            features: Vec::new(),
            flows: HashMap::new(),
        };
        let gold = EvalGold {
            id: "ops".into(),
            cross_cutting: vec!["health".into()],
            ..EvalGold::default()
        };
        let report = score_gold(&gold, &snapshot);
        let kinds: Vec<_> = report.findings.iter().map(|item| item.kind.as_str()).collect();
        assert!(kinds.contains(&"crossCuttingAsBusiness"));
        assert!(kinds.contains(&"folderName"));
    }

    #[test]
    fn 라이브러리_모드에서_예상_밖_비즈니스_domain을_감점한다() {
        let snapshot = EvalSnapshot {
            domains: vec![domain("d-easy", "easy", DomainKind::Business)],
            features: Vec::new(),
            flows: HashMap::new(),
        };
        let gold = EvalGold {
            id: "c-curl".into(),
            mode: EvalMode::Library,
            ..EvalGold::default()
        };
        let report = score_gold(&gold, &snapshot);
        assert!(report
            .findings
            .iter()
            .any(|item| item.kind == "unexpectedBusiness"));
    }

    #[test]
    fn gold_경로_파라미터_계약을_정규화한다() {
        let gold = EvalGold {
            id: "fastapi".into(),
            must_have_features: vec![FeatureGold {
                contract: "GET /api/v1/users/{user_id}".into(),
                domain: Some("users".into()),
            }],
            ..EvalGold::default()
        };
        let snapshot = EvalSnapshot {
            domains: vec![domain("d-users", "users", DomainKind::Business)],
            features: vec![feature(
                "f-user",
                "GET:/users/:param",
                "users",
                &[],
            )],
            flows: HashMap::new(),
        };
        let report = score_gold(&gold, &snapshot);
        assert_eq!(report.feature_hits, 1, "{:?}", report.findings);
        assert!(report.findings.is_empty(), "{:?}", report.findings);
    }

    #[test]
    fn api_접두가_다른_같은_계약은_정규화_후_일치한다() {
        let gold = EvalGold {
            id: "health".into(),
            must_have_features: vec![FeatureGold {
                contract: "GET /api/v1/runtime/readiness".into(),
                domain: None,
            }],
            ..EvalGold::default()
        };
        let snapshot = EvalSnapshot {
            domains: Vec::new(),
            features: vec![feature("f-ready", "GET:/runtime/readiness", "", &[])],
            flows: HashMap::new(),
        };
        let report = score_gold(&gold, &snapshot);
        assert_eq!(report.feature_hits, 1, "{:?}", report.findings);
    }

    #[test]
    fn feature_wrong_domain과_flow_edge_누락을_잡는다() {
        let snapshot = EvalSnapshot {
            domains: vec![
                domain("d-auth", "auth", DomainKind::Business),
                domain("d-sessions", "sessions", DomainKind::Business),
            ],
            features: vec![feature(
                "f-login",
                "POST:/auth/login",
                "sessions",
                &["flow-login"],
            )],
            flows: HashMap::from([("flow-login".into(), vec!["sequential".into()])]),
        };
        let report = score_gold(&gold_service(), &snapshot);
        let kinds: Vec<_> = report.findings.iter().map(|item| item.kind.as_str()).collect();
        assert!(kinds.contains(&"wrongDomain"));
        assert!(kinds.contains(&"wrongEdge"));
    }
}
