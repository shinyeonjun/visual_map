//! 검증된 의미 제안을 정적 결과에 반영한다.

use super::types::ValidatedProposal;
use crate::diagnostics::Diagnostic;
use crate::domain::{DomainAnalysisOutput, DomainRelation};
use crate::facts::ResolutionStatus;

/// 검증된 의미 제안을 정적 도메인 결과에 보수적으로 반영한다.
pub fn apply(analysis: &mut DomainAnalysisOutput, validated: ValidatedProposal) -> Vec<Diagnostic> {
    let diagnostics = validated.diagnostics;
    for suggestion in validated.suggestions {
        if let Some(group) = analysis
            .groups
            .iter_mut()
            .find(|group| group.id == suggestion.domain_id)
        {
            if let Some(label) = suggestion.label {
                group.label = label;
            }
            if let Some(summary) = suggestion.summary {
                group.summary = Some(summary);
            }
        }
    }

    for merge in validated.merges {
        let Some(merge_group) = analysis
            .groups
            .iter()
            .find(|group| group.id == merge.merge_domain_id)
            .cloned()
        else {
            continue;
        };
        let Some(keep_group) = analysis
            .groups
            .iter_mut()
            .find(|group| group.id == merge.keep_domain_id)
        else {
            continue;
        };
        keep_group
            .primary_unit_ids
            .extend(merge_group.primary_unit_ids);
        keep_group
            .shared_unit_ids
            .extend(merge_group.shared_unit_ids);
        keep_group.entrypoint_ids.extend(merge_group.entrypoint_ids);
        keep_group.resource_ids.extend(merge_group.resource_ids);
        keep_group.evidence.extend(merge_group.evidence);
        keep_group.primary_unit_ids.sort();
        keep_group.primary_unit_ids.dedup();
        keep_group.shared_unit_ids.sort();
        keep_group.shared_unit_ids.dedup();
        keep_group.entrypoint_ids.sort();
        keep_group.entrypoint_ids.dedup();
        keep_group.resource_ids.sort();
        keep_group.resource_ids.dedup();
        keep_group
            .evidence
            .sort_by(|left, right| left.id.cmp(&right.id));
        keep_group
            .evidence
            .dedup_by(|left, right| left.id == right.id);
        keep_group.confidence.score = keep_group
            .confidence
            .score
            .saturating_add(merge_group.confidence.score);
        keep_group.summary = keep_group.summary.take().or(merge_group.summary);
        analysis
            .groups
            .retain(|group| group.id != merge.merge_domain_id);
        for membership in &mut analysis.memberships {
            if membership.domain_id.as_deref() == Some(merge.merge_domain_id.as_str()) {
                membership.domain_id = Some(merge.keep_domain_id.clone());
            }
            for domain_id in &mut membership.domain_ids {
                if domain_id == &merge.merge_domain_id {
                    *domain_id = merge.keep_domain_id.clone();
                }
            }
            membership.domain_ids.sort();
            membership.domain_ids.dedup();
        }
        for relation in &mut analysis.relations {
            if relation.source_domain_id == merge.merge_domain_id {
                relation.source_domain_id = merge.keep_domain_id.clone();
            }
            if relation.target_domain_id == merge.merge_domain_id {
                relation.target_domain_id = merge.keep_domain_id.clone();
            }
        }
        coalesce_relations(&mut analysis.relations);
    }
    diagnostics
}

fn coalesce_relations(relations: &mut Vec<DomainRelation>) {
    let mut merged = std::collections::BTreeMap::new();
    for relation in relations.drain(..) {
        if relation.source_domain_id == relation.target_domain_id {
            continue;
        }
        let key = (
            relation.source_domain_id.clone(),
            relation.target_domain_id.clone(),
            relation.kind.clone(),
        );
        let entry = merged.entry(key).or_insert_with(|| relation.clone());
        entry.weight = entry.weight.saturating_add(relation.weight);
        entry.evidence.extend(relation.evidence);
        entry.evidence.sort_by(|left, right| left.id.cmp(&right.id));
        entry.evidence.dedup_by(|left, right| left.id == right.id);
        entry.status = worse_status(&entry.status, &relation.status);
    }
    *relations = merged.into_values().collect();
}

fn worse_status(left: &ResolutionStatus, right: &ResolutionStatus) -> ResolutionStatus {
    let rank = |status: &ResolutionStatus| match status {
        ResolutionStatus::Dynamic => 4,
        ResolutionStatus::Unknown => 3,
        ResolutionStatus::Candidate => 2,
        ResolutionStatus::Confirmed => 1,
    };
    if rank(left) >= rank(right) {
        left.clone()
    } else {
        right.clone()
    }
}
