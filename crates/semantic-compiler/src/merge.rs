//! Deterministic assembly of independently verified semantic partitions.
//!
//! Local AI jobs are allowed to describe and group only their disjoint region
//! scope.  Publication must not ask another model to reinterpret those results:
//! this module remaps partition-local identities into one global proposal and
//! runs the ordinary full-packet verifier once.

use crate::{
    partition::validate_verified_partitions, verify_base_proposal, CompiledBasePrompt,
    VerifiedSemanticPartition,
};
use codebase_semantic_model::{
    AreaProposal, LabelSource, OutputLanguage, ProjectSemanticProposal, ProposalKey,
    RegionAssignment, RegionId, SemanticRevisionProposal, BASE_SEMANTIC_SCHEMA_VERSION,
};
use std::collections::{BTreeMap, BTreeSet};

/// Joins disjoint verified Map results without another provider call.
///
/// Ordering, proposal identities, assignment coverage, hierarchy references,
/// and the final revision digest are deterministic for an identical base
/// packet and identical verified partitions.
pub fn merge_verified_partitions(
    base: &CompiledBasePrompt,
    partitions: &[VerifiedSemanticPartition],
) -> Result<codebase_semantic_model::ApprovedSemanticRevision, crate::SemanticCompileError> {
    validate_verified_partitions(base, partitions)?;

    let mut ordered = partitions.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| left.partition_key.cmp(&right.partition_key));

    let mut areas = Vec::new();
    let mut assignments = Vec::new();
    let mut unassigned_regions = Vec::new();
    let mut warnings = BTreeSet::new();
    let mut member_hints = BTreeMap::<ProposalKey, Vec<RegionId>>::new();

    for (partition_index, partition) in ordered.into_iter().enumerate() {
        let mut local_areas = partition.revision.areas.iter().collect::<Vec<_>>();
        local_areas.sort_by(|left, right| left.area_id.cmp(&right.area_id));
        let local_keys = local_areas
            .iter()
            .enumerate()
            .map(|(area_index, area)| {
                Ok((
                    area.area_id.clone(),
                    ProposalKey::parse(format!("p{partition_index:04}_a{area_index:04}"))?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>, codebase_semantic_model::SemanticIdError>>()
            .map_err(|error| identity_error("deterministicMerge.areaKey", error))?;

        for area in local_areas {
            let proposal_key = local_keys[&area.area_id].clone();
            member_hints.insert(
                proposal_key.clone(),
                area.effective_member_region_ids.clone(),
            );
            areas.push(AreaProposal {
                proposal_key,
                parent_proposal_key: area
                    .parent_area_id
                    .as_ref()
                    .and_then(|parent| local_keys.get(parent))
                    .cloned(),
                level: area.level,
                label: area.label.clone(),
                summary: area.summary.clone(),
                category: area.category,
                representative_fact_ids: area.representative_fact_ids.clone(),
                representative_trace_path_ids: area.representative_trace_path_ids.clone(),
                evidence_ids: area.evidence_ids.clone(),
                aliases: area.aliases.clone(),
                label_source: area.label_source,
                fallback_reason: area.fallback_reason,
            });
        }

        for assignment in &partition.revision.assignments {
            let Some(area_proposal_key) = local_keys.get(&assignment.area_id) else {
                return Err(crate::SemanticCompileError::new(
                    crate::SemanticCompileErrorCode::MissingReference,
                    "deterministicMerge.assignments[].areaId",
                    format!("approved local area {} does not exist", assignment.area_id),
                ));
            };
            assignments.push(RegionAssignment {
                region_id: assignment.region_id.clone(),
                area_proposal_key: area_proposal_key.clone(),
            });
        }
        unassigned_regions.extend(partition.revision.unassigned_regions.iter().cloned());
        warnings.extend(partition.revision.warnings.iter().cloned());
    }

    make_sibling_labels_unique(base, &mut areas, &member_hints)?;
    let proposal = SemanticRevisionProposal {
        schema_version: BASE_SEMANTIC_SCHEMA_VERSION,
        snapshot_id: base.packet.snapshot_id.clone(),
        semantic_input_digest: base.packet.semantic_input_digest,
        project: ProjectSemanticProposal {
            summary: deterministic_project_summary(
                base.packet.output_language,
                base.packet.input.regions.len(),
            ),
            aliases: Vec::new(),
            representative_fact_ids: Vec::new(),
            evidence_ids: Vec::new(),
        },
        areas,
        assignments,
        unassigned_regions,
        warnings: warnings.into_iter().collect(),
    };
    verify_base_proposal(&base.packet, proposal)
}

fn deterministic_project_summary(language: OutputLanguage, region_count: usize) -> String {
    match language {
        OutputLanguage::Korean => {
            format!("검증된 코드 영역 {region_count}개를 현재 책임 단위로 정리한 코드베이스 지도입니다.")
        }
        OutputLanguage::English => format!(
            "A current codebase map organized from {region_count} verified structural regions."
        ),
    }
}

fn make_sibling_labels_unique(
    base: &CompiledBasePrompt,
    areas: &mut [AreaProposal],
    member_hints: &BTreeMap<ProposalKey, Vec<RegionId>>,
) -> Result<(), crate::SemanticCompileError> {
    areas.sort_by(|left, right| left.proposal_key.cmp(&right.proposal_key));
    let structural_labels = base
        .packet
        .input
        .regions
        .iter()
        .map(|region| (region.region_id.clone(), region.structural_label.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut used = BTreeSet::<(Option<ProposalKey>, String)>::new();

    for (ordinal, area) in areas.iter_mut().enumerate() {
        let parent = area.parent_proposal_key.clone();
        let initial = (parent.clone(), area.label.to_lowercase());
        if used.insert(initial) {
            continue;
        }

        let member_labels = member_hints
            .get(&area.proposal_key)
            .into_iter()
            .flatten()
            .filter_map(|region_id| structural_labels.get(region_id))
            .collect::<BTreeSet<_>>();
        let replacement = if area.label_source == LabelSource::Structural {
            member_labels.into_iter().find_map(|label| {
                let key = (parent.clone(), label.to_lowercase());
                (!used.contains(&key)).then(|| label.clone())
            })
        } else {
            member_labels.into_iter().find_map(|label| {
                let candidate = bounded_qualified_label(&area.label, label, ordinal + 1);
                let key = (parent.clone(), candidate.to_lowercase());
                (!used.contains(&key)).then_some(candidate)
            })
        }
        .or_else(|| {
            (area.label_source == LabelSource::Semantic)
                .then(|| bounded_qualified_label(&area.label, "영역", ordinal + 1))
        })
        .ok_or_else(|| {
            crate::SemanticCompileError::new(
                crate::SemanticCompileErrorCode::NonCanonicalValue,
                format!("deterministicMerge.areas[{}].label", area.proposal_key),
                "duplicate structural sibling labels cannot be disambiguated from their members",
            )
        })?;
        area.label = replacement;
        used.insert((parent, area.label.to_lowercase()));
    }
    Ok(())
}

fn bounded_qualified_label(label: &str, qualifier: &str, ordinal: usize) -> String {
    let ordinal_suffix = format!(" {ordinal}");
    let qualifier_budget = 128usize
        .saturating_sub(" · ".len())
        .saturating_sub(ordinal_suffix.len())
        .saturating_sub(1);
    let qualifier = truncate_utf8(qualifier, qualifier_budget);
    let suffix = format!(" · {qualifier}{ordinal_suffix}");
    let available = 128usize.saturating_sub(suffix.len());
    let prefix = truncate_utf8(label, available);
    format!("{prefix}{suffix}")
}

fn truncate_utf8(value: &str, max_bytes: usize) -> &str {
    let mut end = value.len().min(max_bytes);
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

fn identity_error(
    path: &'static str,
    error: codebase_semantic_model::SemanticIdError,
) -> crate::SemanticCompileError {
    crate::SemanticCompileError::new(
        crate::SemanticCompileErrorCode::InvalidProviderOutput,
        path,
        error.to_string(),
    )
}
