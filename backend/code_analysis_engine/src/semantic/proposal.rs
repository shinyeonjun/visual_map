use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Codex가 특정 도메인에 대해 제안하는 의미 정보다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainSemanticSuggestion {
    pub domain_id: String,
    pub label: Option<String>,
    pub summary: Option<String>,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
}

/// Codex가 후보 도메인을 합치자고 제안하는 정보다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainMergeSuggestion {
    pub keep_domain_id: String,
    pub merge_domain_id: String,
    pub reason: String,
}

/// Codex 의미 분석의 구조화된 결과다.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CodexProposal {
    pub domains: Vec<DomainSemanticSuggestion>,
    pub merges: Vec<DomainMergeSuggestion>,
}

/// 순서대로 도착한 여러 Codex 청크 결과를 하나의 제안으로 합친다.
///
/// 청크 경계에 걸쳐 같은 도메인이나 관계가 반복되어도 안정적인 ID를 기준으로
/// 한 번만 남긴다. 라벨·요약은 앞 청크의 값을 우선하고, 앞 청크에 값이 없을 때
/// 뒤 청크의 값을 사용한다.
pub fn merge_proposals(proposals: impl IntoIterator<Item = CodexProposal>) -> CodexProposal {
    let mut domains: BTreeMap<String, DomainSemanticSuggestion> = BTreeMap::new();
    let mut merges: BTreeMap<(String, String), DomainMergeSuggestion> = BTreeMap::new();

    for proposal in proposals {
        for suggestion in proposal.domains {
            let entry = domains
                .entry(suggestion.domain_id.clone())
                .or_insert_with(|| DomainSemanticSuggestion {
                    domain_id: suggestion.domain_id.clone(),
                    label: None,
                    summary: None,
                    evidence_ids: Vec::new(),
                });
            if entry.label.is_none() {
                entry.label = suggestion.label;
            }
            if entry.summary.is_none() {
                entry.summary = suggestion.summary;
            }
            entry.evidence_ids.extend(suggestion.evidence_ids);
        }

        for merge in proposal.merges {
            merges
                .entry((merge.keep_domain_id.clone(), merge.merge_domain_id.clone()))
                .or_insert(merge);
        }
    }

    for suggestion in domains.values_mut() {
        suggestion.evidence_ids.sort();
        suggestion.evidence_ids.dedup();
    }

    CodexProposal {
        domains: domains.into_values().collect(),
        merges: merges.into_values().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::{merge_proposals, CodexProposal, DomainMergeSuggestion, DomainSemanticSuggestion};

    #[test]
    fn merge_chunk_results_by_id_and_order() {
        let merged = merge_proposals([
            CodexProposal {
                domains: vec![DomainSemanticSuggestion {
                    domain_id: "domain_a".into(),
                    label: Some("주문".into()),
                    summary: None,
                    evidence_ids: vec!["e2".into(), "e1".into()],
                }],
                merges: vec![DomainMergeSuggestion {
                    keep_domain_id: "domain_a".into(),
                    merge_domain_id: "domain_b".into(),
                    reason: "첫 청크의 판단".into(),
                }],
            },
            CodexProposal {
                domains: vec![DomainSemanticSuggestion {
                    domain_id: "domain_a".into(),
                    label: Some("다른 이름".into()),
                    summary: Some("주문 도메인".into()),
                    evidence_ids: vec!["e1".into(), "e3".into()],
                }],
                merges: vec![DomainMergeSuggestion {
                    keep_domain_id: "domain_a".into(),
                    merge_domain_id: "domain_b".into(),
                    reason: "중복 제안".into(),
                }],
            },
        ]);

        assert_eq!(merged.domains.len(), 1);
        assert_eq!(merged.domains[0].label.as_deref(), Some("주문"));
        assert_eq!(merged.domains[0].summary.as_deref(), Some("주문 도메인"));
        assert_eq!(merged.domains[0].evidence_ids, ["e1", "e2", "e3"]);
        assert_eq!(merged.merges.len(), 1);
        assert_eq!(merged.merges[0].reason, "첫 청크의 판단");
    }
}
