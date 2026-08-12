//! Codex 제안의 참조 무결성을 확인하고 안전하게 반영하는 영역.

mod apply;
mod types;
mod validate;

pub use apply::apply;
pub use types::ValidatedProposal;
pub use validate::validate;

#[cfg(test)]
mod tests {
    use super::{apply, validate};
    use crate::domain::confidence::{DomainConfidence, DomainStatus};
    use crate::domain::membership::{DomainMembership, MembershipKind};
    use crate::domain::{DomainAnalysisOutput, DomainGroup, DomainKind, DomainRelation};
    use crate::facts::{Evidence, ResolutionStatus, SourceSpan};
    use crate::semantic::proposal::{
        CodexProposal, DomainMergeSuggestion, DomainSemanticSuggestion,
    };
    use std::collections::BTreeSet;

    fn group(id: &str, key: &str, evidence: Evidence) -> DomainGroup {
        DomainGroup {
            id: id.into(),
            key: key.into(),
            label: key.into(),
            kind: DomainKind::Business,
            status: DomainStatus::Candidate,
            confidence: DomainConfidence {
                level: "medium".into(),
                score: 4,
                signal_families: BTreeSet::new(),
            },
            primary_unit_ids: vec![format!("unit_{key}")],
            shared_unit_ids: Vec::new(),
            entrypoint_ids: Vec::new(),
            feature_ids: Vec::new(),
            resource_ids: Vec::new(),
            evidence: vec![evidence],
            summary: None,
        }
    }

    #[test]
    fn codex_제안은_실제_근거만_반영하고_병합_후_관계를_정리한다() {
        let evidence = Evidence::new(
            "path",
            "orders",
            SourceSpan::new("file_1", "src/orders.ts", 1, 1, 1, 10),
        );
        let mut analysis = DomainAnalysisOutput {
            groups: vec![
                group("domain_a", "orders", evidence.clone()),
                group(
                    "domain_b",
                    "checkout",
                    Evidence::new(
                        "path",
                        "checkout",
                        SourceSpan::new("file_2", "src/checkout.ts", 1, 1, 1, 12),
                    ),
                ),
            ],
            memberships: vec![
                DomainMembership {
                    unit_id: "unit_orders".into(),
                    domain_id: Some("domain_a".into()),
                    domain_ids: vec!["domain_a".into()],
                    kind: MembershipKind::Primary,
                    score: 5,
                },
                DomainMembership {
                    unit_id: "unit_checkout".into(),
                    domain_id: Some("domain_b".into()),
                    domain_ids: vec!["domain_b".into()],
                    kind: MembershipKind::Primary,
                    score: 5,
                },
            ],
            relations: vec![DomainRelation {
                source_domain_id: "domain_a".into(),
                target_domain_id: "domain_b".into(),
                kind: "call".into(),
                status: ResolutionStatus::Confirmed,
                weight: 1,
                evidence: vec![evidence.clone()],
            }],
            ..DomainAnalysisOutput::default()
        };
        let proposal = CodexProposal {
            domains: vec![DomainSemanticSuggestion {
                domain_id: "domain_a".into(),
                label: Some("주문".into()),
                summary: Some("주문 처리 기능".into()),
                evidence_ids: vec![evidence.id.clone()],
            }],
            merges: vec![DomainMergeSuggestion {
                keep_domain_id: "domain_a".into(),
                merge_domain_id: "domain_b".into(),
                reason: "같은 주문 결제 경계".into(),
            }],
        };

        let validated = validate(
            proposal,
            &analysis,
            &crate::config::SemanticPolicy::default(),
        );
        assert_eq!(validated.suggestions.len(), 1);
        assert_eq!(validated.merges.len(), 1);
        let diagnostics = apply(&mut analysis, validated);

        assert!(diagnostics.is_empty());
        assert_eq!(analysis.groups.len(), 1);
        assert_eq!(analysis.groups[0].label, "주문");
        assert_eq!(
            analysis.memberships[1].domain_id.as_deref(),
            Some("domain_a")
        );
        assert!(analysis.relations.is_empty());
    }
}
