//! Codex가 반환한 도메인 제안의 참조 무결성을 검증한다.

use super::types::ValidatedProposal;
use crate::config::SemanticPolicy;
use crate::diagnostics::Diagnostic;
use crate::domain::DomainAnalysisOutput;
use crate::semantic::proposal::CodexProposal;
use std::collections::BTreeSet;

/// Codex가 참조한 domain ID와 evidence가 실제 분석 결과에 존재하는지 검증한다.
pub fn validate(
    proposal: CodexProposal,
    analysis: &DomainAnalysisOutput,
    policy: &SemanticPolicy,
) -> ValidatedProposal {
    let domain_ids: BTreeSet<_> = analysis
        .groups
        .iter()
        .map(|group| group.id.as_str())
        .collect();
    let evidence_ids: BTreeSet<_> = analysis
        .groups
        .iter()
        .flat_map(|group| group.evidence.iter().map(|evidence| evidence.id.as_str()))
        .collect();
    let mut output = ValidatedProposal::default();

    for suggestion in proposal.domains {
        if !domain_ids.contains(suggestion.domain_id.as_str()) {
            output.diagnostics.push(Diagnostic::info(
                "CODEX_UNKNOWN_DOMAIN",
                format!(
                    "Codex가 알 수 없는 도메인 ID를 제안해 버렸습니다: {}",
                    suggestion.domain_id
                ),
            ));
            continue;
        }
        let evidence_is_known = !suggestion.evidence_ids.is_empty()
            && suggestion
                .evidence_ids
                .iter()
                .all(|evidence| evidence_ids.contains(evidence.as_str()));
        if !evidence_is_known {
            output.diagnostics.push(Diagnostic::info(
                "CODEX_UNKNOWN_EVIDENCE",
                format!(
                    "Codex 제안의 근거를 확인할 수 없어 무시했습니다: {}",
                    suggestion.domain_id
                ),
            ));
            continue;
        }
        let valid_label = suggestion
            .label
            .as_deref()
            .map(|label| !label.trim().is_empty() && label.len() <= policy.maximum_label_length)
            .unwrap_or(false);
        let valid_summary = suggestion
            .summary
            .as_deref()
            .map(|summary| {
                !summary.trim().is_empty() && summary.len() <= policy.maximum_summary_length
            })
            .unwrap_or(false);
        if valid_label || valid_summary {
            output.suggestions.push(suggestion);
        }
    }

    for merge in proposal.merges {
        let valid_ids = domain_ids.contains(merge.keep_domain_id.as_str())
            && domain_ids.contains(merge.merge_domain_id.as_str())
            && merge.keep_domain_id != merge.merge_domain_id
            && !merge.reason.trim().is_empty()
            && merge.reason.len() <= policy.maximum_merge_reason_length;
        let both_uncertain = analysis
            .groups
            .iter()
            .filter(|group| group.id == merge.keep_domain_id || group.id == merge.merge_domain_id)
            .all(|group| {
                !matches!(
                    group.status,
                    crate::domain::confidence::DomainStatus::Confirmed
                )
            });
        if valid_ids && both_uncertain {
            output.merges.push(merge);
        } else {
            output.diagnostics.push(Diagnostic::info(
                "CODEX_MERGE_REJECTED",
                format!(
                    "정적 분석 결과와 충돌하는 Codex 병합 제안을 거부했습니다: {}",
                    merge.reason
                ),
            ));
        }
    }
    output
}
