use crate::{
    packet::{ensure_unique, error, validate_message, validate_text},
    CompiledBasePrompt, SemanticCompileError, SemanticCompileErrorCode,
};
use codebase_fact_model::identity::{EvidenceId, FactNodeId, Sha256Digest};
use codebase_semantic_model::{
    ApprovedRegionAssignment, ApprovedSemanticArea, ApprovedSemanticRevision, AreaCategory,
    AreaProposal, BaseSemanticPacket, LabelSource, ProposalKey, RegionAssignment, RegionId,
    SemanticAreaId, SemanticRevisionProposal, SemanticRevisionProvider, TracePathId,
    BASE_SEMANTIC_SCHEMA_VERSION,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

pub fn parse_and_verify_base_response(
    compiled: &CompiledBasePrompt,
    raw_response: &str,
) -> Result<ApprovedSemanticRevision, SemanticCompileError> {
    let proposal =
        serde_json::from_str::<SemanticRevisionProposal>(raw_response).map_err(|err| {
            SemanticCompileError::new(
                SemanticCompileErrorCode::InvalidProviderOutput,
                "providerOutput",
                format!("provider response must be one strict JSON object: {err}"),
            )
        })?;
    verify_base_proposal(&compiled.packet, proposal)
}

pub fn verify_base_proposal(
    packet: &BaseSemanticPacket,
    mut proposal: SemanticRevisionProposal,
) -> Result<ApprovedSemanticRevision, SemanticCompileError> {
    if proposal.schema_version != BASE_SEMANTIC_SCHEMA_VERSION {
        return Err(error(
            SemanticCompileErrorCode::InvalidSchema,
            "schemaVersion",
            "provider output schema version does not match the request",
        ));
    }
    if proposal.snapshot_id != packet.snapshot_id {
        return Err(error(
            SemanticCompileErrorCode::DigestMismatch,
            "snapshotId",
            "provider output references a different static snapshot",
        ));
    }
    if proposal.semantic_input_digest != packet.semantic_input_digest {
        return Err(error(
            SemanticCompileErrorCode::DigestMismatch,
            "semanticInputDigest",
            "provider output does not echo the exact semantic input digest",
        ));
    }

    validate_project(&packet.input, &proposal)?;
    ensure_unique(
        proposal.areas.iter().map(|area| &area.proposal_key),
        "areas[].proposalKey",
    )?;
    if proposal.areas.is_empty() {
        return Err(error(
            SemanticCompileErrorCode::InvalidProviderOutput,
            "areas",
            "at least one semantic or structural area is required",
        ));
    }

    let area_by_key: BTreeMap<_, _> = proposal
        .areas
        .iter()
        .map(|area| (&area.proposal_key, area))
        .collect();
    validate_area_hierarchy(&area_by_key)?;
    validate_assignments(packet, &proposal, &area_by_key)?;

    let direct_members = direct_members(&proposal.assignments);
    let effective_members = effective_members(&proposal.areas, &direct_members);
    validate_areas(packet, &proposal.areas, &area_by_key, &effective_members)?;

    canonicalize_proposal(&mut proposal);
    approve(packet, proposal, direct_members, effective_members)
}

fn validate_project(
    input: &codebase_semantic_model::BaseSemanticInput,
    proposal: &SemanticRevisionProposal,
) -> Result<(), SemanticCompileError> {
    validate_message(&proposal.project.summary, "project.summary", 1200)?;
    reject_prescriptive_text(&proposal.project.summary, "project.summary")?;
    validate_strings(&proposal.project.aliases, "project.aliases", 16, 160)?;
    ensure_unique(
        proposal.project.representative_fact_ids.iter(),
        "project.representativeFactIds",
    )?;
    ensure_unique(proposal.project.evidence_ids.iter(), "project.evidenceIds")?;

    let allowed_facts = allowed_fact_ids(input);
    let allowed_evidence = allowed_evidence_ids(input);
    for fact_id in &proposal.project.representative_fact_ids {
        if !allowed_facts.contains(fact_id) {
            return Err(unexpected(
                "project.representativeFactIds",
                format!("fact {fact_id} was not supplied to the model"),
            ));
        }
    }
    for evidence_id in &proposal.project.evidence_ids {
        if !allowed_evidence.contains(evidence_id) {
            return Err(unexpected(
                "project.evidenceIds",
                format!("evidence {evidence_id} was not supplied to the model"),
            ));
        }
    }
    Ok(())
}

fn validate_area_hierarchy(
    areas: &BTreeMap<&ProposalKey, &AreaProposal>,
) -> Result<(), SemanticCompileError> {
    let mut sibling_labels = BTreeSet::new();
    for area in areas.values() {
        match (area.level, area.parent_proposal_key.as_ref()) {
            (0, None) => {}
            (1, Some(parent_key)) => {
                let Some(parent) = areas.get(parent_key) else {
                    return Err(error(
                        SemanticCompileErrorCode::MissingReference,
                        format!("areas[{}].parentProposalKey", area.proposal_key),
                        format!("parent proposal {parent_key} does not exist"),
                    ));
                };
                if parent.level != 0 || parent.parent_proposal_key.is_some() {
                    return Err(error(
                        SemanticCompileErrorCode::InvalidHierarchy,
                        format!("areas[{}].parentProposalKey", area.proposal_key),
                        "an L1 area must have one L0 parent",
                    ));
                }
            }
            _ => {
                return Err(error(
                    SemanticCompileErrorCode::InvalidHierarchy,
                    format!("areas[{}]", area.proposal_key),
                    "only parentless L0 and direct-child L1 areas are allowed",
                ));
            }
        }

        let parent = area
            .parent_proposal_key
            .as_ref()
            .map(ProposalKey::as_str)
            .unwrap_or("<root>");
        let label = area.label.to_lowercase();
        if !sibling_labels.insert((parent.to_string(), label)) {
            return Err(error(
                SemanticCompileErrorCode::NonCanonicalValue,
                format!("areas[{}].label", area.proposal_key),
                "sibling semantic labels must be distinct",
            ));
        }
    }
    Ok(())
}

fn validate_assignments(
    packet: &BaseSemanticPacket,
    proposal: &SemanticRevisionProposal,
    areas: &BTreeMap<&ProposalKey, &AreaProposal>,
) -> Result<(), SemanticCompileError> {
    ensure_unique(
        proposal.assignments.iter().map(|value| &value.region_id),
        "assignments[].regionId",
    )?;
    ensure_unique(
        proposal
            .unassigned_regions
            .iter()
            .map(|value| &value.region_id),
        "unassignedRegions[].regionId",
    )?;

    let expected: BTreeSet<_> = packet
        .input
        .regions
        .iter()
        .map(|region| &region.region_id)
        .collect();
    let mut accounted = BTreeSet::new();
    for assignment in &proposal.assignments {
        if !expected.contains(&assignment.region_id) {
            return Err(unexpected(
                "assignments[].regionId",
                format!(
                    "region {} was not supplied to the model",
                    assignment.region_id
                ),
            ));
        }
        if !areas.contains_key(&assignment.area_proposal_key) {
            return Err(error(
                SemanticCompileErrorCode::MissingReference,
                "assignments[].areaProposalKey",
                format!("area {} does not exist", assignment.area_proposal_key),
            ));
        }
        accounted.insert(&assignment.region_id);
    }
    for unassigned in &proposal.unassigned_regions {
        if !expected.contains(&unassigned.region_id) {
            return Err(unexpected(
                "unassignedRegions[].regionId",
                format!(
                    "region {} was not supplied to the model",
                    unassigned.region_id
                ),
            ));
        }
        if !accounted.insert(&unassigned.region_id) {
            return Err(error(
                SemanticCompileErrorCode::IncompleteAssignment,
                "unassignedRegions[].regionId",
                format!(
                    "region {} is both assigned and unassigned",
                    unassigned.region_id
                ),
            ));
        }
    }
    if accounted != expected {
        let missing = expected
            .difference(&accounted)
            .map(|region| region.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(error(
            SemanticCompileErrorCode::IncompleteAssignment,
            "assignments",
            format!("every static region must be accounted for exactly once; missing: {missing}"),
        ));
    }
    Ok(())
}

fn validate_areas(
    packet: &BaseSemanticPacket,
    areas: &[AreaProposal],
    area_by_key: &BTreeMap<&ProposalKey, &AreaProposal>,
    effective_members: &BTreeMap<ProposalKey, Vec<RegionId>>,
) -> Result<(), SemanticCompileError> {
    let allowed_facts = allowed_fact_ids(&packet.input);
    let allowed_traces: BTreeSet<_> = packet
        .input
        .representative_traces
        .iter()
        .map(|trace| &trace.trace_path_id)
        .collect();
    let allowed_evidence = allowed_evidence_ids(&packet.input);
    let fact_regions = fact_regions(&packet.input);
    let trace_regions = trace_regions(&packet.input);
    let evidence_regions = evidence_regions(&packet.input, &trace_regions);
    let region_labels: BTreeMap<_, _> = packet
        .input
        .regions
        .iter()
        .map(|region| (&region.region_id, region.structural_label.as_str()))
        .collect();

    for area in areas {
        let path = format!("areas[{}]", area.proposal_key);
        validate_text(&area.label, &format!("{path}.label"), 128)?;
        validate_message(&area.summary, &format!("{path}.summary"), 1200)?;
        reject_prescriptive_text(&area.summary, &format!("{path}.summary"))?;
        validate_strings(&area.aliases, &format!("{path}.aliases"), 16, 160)?;
        ensure_unique(
            area.representative_fact_ids.iter(),
            &format!("{path}.representativeFactIds"),
        )?;
        ensure_unique(
            area.representative_trace_path_ids.iter(),
            &format!("{path}.representativeTracePathIds"),
        )?;
        ensure_unique(area.evidence_ids.iter(), &format!("{path}.evidenceIds"))?;

        let members = effective_members
            .get(&area.proposal_key)
            .cloned()
            .unwrap_or_default();
        if members.is_empty() {
            return Err(error(
                SemanticCompileErrorCode::IncompleteAssignment,
                &path,
                "an area must own at least one direct or descendant region",
            ));
        }
        let member_set: BTreeSet<_> = members.iter().collect();

        validate_fallback(area, &member_set, &region_labels, &path)?;

        for fact_id in &area.representative_fact_ids {
            if !allowed_facts.contains(fact_id) {
                return Err(unexpected(
                    format!("{path}.representativeFactIds"),
                    format!("fact {fact_id} was not supplied to the model"),
                ));
            }
            if !fact_regions
                .get(fact_id)
                .is_some_and(|regions| regions.iter().any(|region| member_set.contains(region)))
            {
                return Err(evidence_mismatch(
                    format!("{path}.representativeFactIds"),
                    format!("fact {fact_id} is unrelated to the area's assigned regions"),
                ));
            }
        }
        for trace_id in &area.representative_trace_path_ids {
            if !allowed_traces.contains(trace_id) {
                return Err(unexpected(
                    format!("{path}.representativeTracePathIds"),
                    format!("trace {trace_id} was not supplied to the model"),
                ));
            }
            if !trace_regions.get(trace_id).is_some_and(|regions| {
                !regions.is_empty() && regions.iter().all(|region| member_set.contains(region))
            }) {
                return Err(evidence_mismatch(
                    format!("{path}.representativeTracePathIds"),
                    format!(
                        "trace {trace_id} crosses a region outside the area's assigned regions"
                    ),
                ));
            }
        }
        for evidence_id in &area.evidence_ids {
            if !allowed_evidence.contains(evidence_id) {
                return Err(unexpected(
                    format!("{path}.evidenceIds"),
                    format!("evidence {evidence_id} was not supplied to the model"),
                ));
            }
            if !evidence_regions
                .get(evidence_id)
                .is_some_and(|regions| regions.iter().any(|region| member_set.contains(region)))
            {
                return Err(evidence_mismatch(
                    format!("{path}.evidenceIds"),
                    format!("evidence {evidence_id} is unrelated to the area's assigned regions"),
                ));
            }
        }

        if area.label_source == LabelSource::Semantic && area.evidence_ids.is_empty() {
            return Err(evidence_mismatch(
                format!("{path}.evidenceIds"),
                "a semantic label requires evidence",
            ));
        }

        if area.level == 1 {
            let parent = area
                .parent_proposal_key
                .as_ref()
                .and_then(|key| area_by_key.get(key));
            if parent.is_none() {
                return Err(error(
                    SemanticCompileErrorCode::InvalidHierarchy,
                    &path,
                    "L1 area has no valid L0 parent",
                ));
            }
        }
    }
    Ok(())
}

fn validate_fallback(
    area: &AreaProposal,
    member_set: &BTreeSet<&RegionId>,
    region_labels: &BTreeMap<&RegionId, &str>,
    path: &str,
) -> Result<(), SemanticCompileError> {
    match (area.label_source, area.fallback_reason) {
        (LabelSource::Semantic, None) if area.category != AreaCategory::Structural => {
            if is_placeholder_label(&area.label) {
                return Err(error(
                    SemanticCompileErrorCode::NonCanonicalValue,
                    format!("{path}.label"),
                    "placeholder labels are not valid semantic names",
                ));
            }
        }
        (LabelSource::Structural, Some(_)) if area.category == AreaCategory::Structural => {
            let exact_structural_label = member_set.iter().any(|region| {
                region_labels
                    .get(*region)
                    .is_some_and(|label| *label == area.label)
            });
            if !exact_structural_label {
                return Err(error(
                    SemanticCompileErrorCode::ContradictoryFallback,
                    format!("{path}.label"),
                    "a structural fallback label must exactly copy one assigned region label",
                ));
            }
        }
        _ => {
            return Err(error(
                SemanticCompileErrorCode::ContradictoryFallback,
                path,
                "semantic labels require no fallback; structural labels require structural category and a fallback reason",
            ));
        }
    }
    Ok(())
}

fn direct_members(assignments: &[RegionAssignment]) -> BTreeMap<ProposalKey, Vec<RegionId>> {
    let mut members: BTreeMap<ProposalKey, Vec<RegionId>> = BTreeMap::new();
    for assignment in assignments {
        members
            .entry(assignment.area_proposal_key.clone())
            .or_default()
            .push(assignment.region_id.clone());
    }
    for regions in members.values_mut() {
        regions.sort();
    }
    members
}

fn effective_members(
    areas: &[AreaProposal],
    direct: &BTreeMap<ProposalKey, Vec<RegionId>>,
) -> BTreeMap<ProposalKey, Vec<RegionId>> {
    let mut effective = direct.clone();
    for child in areas.iter().filter(|area| area.level == 1) {
        let Some(parent) = &child.parent_proposal_key else {
            continue;
        };
        let child_members = direct.get(&child.proposal_key).cloned().unwrap_or_default();
        effective
            .entry(parent.clone())
            .or_default()
            .extend(child_members);
    }
    for members in effective.values_mut() {
        members.sort();
        members.dedup();
    }
    effective
}

fn allowed_fact_ids(input: &codebase_semantic_model::BaseSemanticInput) -> BTreeSet<&FactNodeId> {
    let mut facts = BTreeSet::from([&input.repository.fact_id]);
    facts.extend(input.repository.framework_fact_ids.iter());
    facts.extend(input.anchors.iter().map(|anchor| &anchor.fact_id));
    for trace in &input.representative_traces {
        facts.extend(trace.ordered_fact_ids.iter());
    }
    facts
}

fn allowed_evidence_ids(
    input: &codebase_semantic_model::BaseSemanticInput,
) -> BTreeSet<&EvidenceId> {
    let mut evidence = BTreeSet::new();
    for anchor in &input.anchors {
        evidence.extend(anchor.evidence_ids.iter());
    }
    for relation in &input.boundary_relations {
        evidence.extend(relation.evidence_ids.iter());
    }
    for trace in &input.representative_traces {
        evidence.extend(trace.evidence_ids.iter());
    }
    evidence.extend(input.excerpts.iter().map(|excerpt| &excerpt.evidence_id));
    evidence
}

fn trace_regions(
    input: &codebase_semantic_model::BaseSemanticInput,
) -> BTreeMap<TracePathId, BTreeSet<RegionId>> {
    let mut result: BTreeMap<TracePathId, BTreeSet<RegionId>> = BTreeMap::new();
    for region in &input.regions {
        for trace_id in &region.representative_trace_path_ids {
            result
                .entry(trace_id.clone())
                .or_default()
                .insert(region.region_id.clone());
        }
    }
    result
}

fn fact_regions(
    input: &codebase_semantic_model::BaseSemanticInput,
) -> BTreeMap<FactNodeId, BTreeSet<RegionId>> {
    let mut result: BTreeMap<FactNodeId, BTreeSet<RegionId>> = BTreeMap::new();
    for anchor in &input.anchors {
        result
            .entry(anchor.fact_id.clone())
            .or_default()
            .insert(anchor.owner_region_id.clone());
    }
    let trace_owners = trace_regions(input);
    for trace in &input.representative_traces {
        if let Some(regions) = trace_owners.get(&trace.trace_path_id) {
            // A cross-region path proves ordered connectivity, not that every
            // fact belongs to every region it traverses. Exact anchor ownership
            // above remains authoritative; only a single-region trace may add
            // locality for otherwise non-anchor facts.
            if regions.len() != 1 {
                continue;
            }
            for fact_id in &trace.ordered_fact_ids {
                result
                    .entry(fact_id.clone())
                    .or_default()
                    .extend(regions.iter().cloned());
            }
        }
    }
    result
}

fn evidence_regions(
    input: &codebase_semantic_model::BaseSemanticInput,
    trace_owners: &BTreeMap<TracePathId, BTreeSet<RegionId>>,
) -> BTreeMap<EvidenceId, BTreeSet<RegionId>> {
    let mut result: BTreeMap<EvidenceId, BTreeSet<RegionId>> = BTreeMap::new();
    for anchor in &input.anchors {
        for evidence_id in &anchor.evidence_ids {
            result
                .entry(evidence_id.clone())
                .or_default()
                .insert(anchor.owner_region_id.clone());
        }
    }
    // Boundary evidence proves that two regions are related. It does not, by
    // itself, prove the business meaning of either endpoint and therefore may
    // not be reused as semantic label evidence here.
    for trace in &input.representative_traces {
        if let Some(regions) = trace_owners.get(&trace.trace_path_id) {
            // Cross-region trace evidence proves the path as a whole. It may
            // not independently decorate the semantic label of either side.
            if regions.len() != 1 {
                continue;
            }
            for evidence_id in &trace.evidence_ids {
                result
                    .entry(evidence_id.clone())
                    .or_default()
                    .extend(regions.iter().cloned());
            }
        }
    }
    for excerpt in &input.excerpts {
        result
            .entry(excerpt.evidence_id.clone())
            .or_default()
            .insert(excerpt.owner_region_id.clone());
    }
    result
}

fn canonicalize_proposal(proposal: &mut SemanticRevisionProposal) {
    proposal.project.aliases.sort();
    proposal.project.representative_fact_ids.sort();
    proposal.project.evidence_ids.sort();
    for area in &mut proposal.areas {
        area.aliases.sort();
        area.representative_fact_ids.sort();
        area.representative_trace_path_ids.sort();
        area.evidence_ids.sort();
    }
    proposal
        .areas
        .sort_by(|left, right| left.proposal_key.cmp(&right.proposal_key));
    proposal
        .assignments
        .sort_by(|left, right| left.region_id.cmp(&right.region_id));
    proposal
        .unassigned_regions
        .sort_by(|left, right| left.region_id.cmp(&right.region_id));
    proposal.warnings.sort();
}

fn approve(
    packet: &BaseSemanticPacket,
    proposal: SemanticRevisionProposal,
    direct_members: BTreeMap<ProposalKey, Vec<RegionId>>,
    effective_members: BTreeMap<ProposalKey, Vec<RegionId>>,
) -> Result<ApprovedSemanticRevision, SemanticCompileError> {
    let mut area_ids = BTreeMap::new();
    for area in proposal.areas.iter().filter(|area| area.level == 0) {
        let members = effective_members
            .get(&area.proposal_key)
            .cloned()
            .unwrap_or_default();
        area_ids.insert(
            area.proposal_key.clone(),
            create_area_id(packet, area, None, &members)?,
        );
    }
    for area in proposal.areas.iter().filter(|area| area.level == 1) {
        let parent_key = area.parent_proposal_key.as_ref().ok_or_else(|| {
            error(
                SemanticCompileErrorCode::InvalidHierarchy,
                format!("areas[{}].parentProposalKey", area.proposal_key),
                "L1 area is missing its parent",
            )
        })?;
        let parent_id = area_ids.get(parent_key).ok_or_else(|| {
            error(
                SemanticCompileErrorCode::InvalidHierarchy,
                format!("areas[{}].parentProposalKey", area.proposal_key),
                "L1 parent was not approved",
            )
        })?;
        let members = effective_members
            .get(&area.proposal_key)
            .cloned()
            .unwrap_or_default();
        area_ids.insert(
            area.proposal_key.clone(),
            create_area_id(packet, area, Some(parent_id), &members)?,
        );
    }

    let mut approved_areas = Vec::with_capacity(proposal.areas.len());
    for area in &proposal.areas {
        approved_areas.push(ApprovedSemanticArea {
            area_id: area_ids[&area.proposal_key].clone(),
            parent_area_id: area
                .parent_proposal_key
                .as_ref()
                .and_then(|parent| area_ids.get(parent))
                .cloned(),
            level: area.level,
            label: area.label.clone(),
            summary: area.summary.clone(),
            category: area.category,
            direct_member_region_ids: direct_members
                .get(&area.proposal_key)
                .cloned()
                .unwrap_or_default(),
            effective_member_region_ids: effective_members
                .get(&area.proposal_key)
                .cloned()
                .unwrap_or_default(),
            representative_fact_ids: area.representative_fact_ids.clone(),
            representative_trace_path_ids: area.representative_trace_path_ids.clone(),
            evidence_ids: area.evidence_ids.clone(),
            aliases: area.aliases.clone(),
            label_source: area.label_source,
            fallback_reason: area.fallback_reason,
        });
    }
    approved_areas.sort_by(|left, right| {
        left.level
            .cmp(&right.level)
            .then_with(|| left.area_id.cmp(&right.area_id))
    });

    let mut approved_assignments = proposal
        .assignments
        .iter()
        .map(|assignment| ApprovedRegionAssignment {
            region_id: assignment.region_id.clone(),
            area_id: area_ids[&assignment.area_proposal_key].clone(),
        })
        .collect::<Vec<_>>();
    approved_assignments.sort_by(|left, right| left.region_id.cmp(&right.region_id));

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct RevisionDigestMaterial<'a> {
        snapshot_id: &'a codebase_fact_model::identity::SnapshotId,
        semantic_input_digest: Sha256Digest,
        project: &'a codebase_semantic_model::ProjectSemanticProposal,
        areas: &'a [ApprovedSemanticArea],
        assignments: &'a [ApprovedRegionAssignment],
        unassigned_regions: &'a [codebase_semantic_model::UnassignedRegion],
    }

    let digest_material = RevisionDigestMaterial {
        snapshot_id: &packet.snapshot_id,
        semantic_input_digest: packet.semantic_input_digest,
        project: &proposal.project,
        areas: &approved_areas,
        assignments: &approved_assignments,
        unassigned_regions: &proposal.unassigned_regions,
    };
    let output_bytes = serde_json::to_vec(&digest_material).map_err(|err| {
        error(
            SemanticCompileErrorCode::InvalidProviderOutput,
            "revisionDigest",
            err.to_string(),
        )
    })?;
    let output_digest = Sha256Digest::of_bytes(&output_bytes).to_hex();
    let input_digest = packet.semantic_input_digest.to_hex();
    let revision_id = codebase_semantic_model::SemanticRevisionId::from_components(&[
        packet.snapshot_id.as_str(),
        &input_digest,
        &output_digest,
    ])
    .map_err(|err| {
        error(
            SemanticCompileErrorCode::InvalidProviderOutput,
            "revisionId",
            err.to_string(),
        )
    })?;

    Ok(ApprovedSemanticRevision {
        schema_version: BASE_SEMANTIC_SCHEMA_VERSION,
        revision_id,
        snapshot_id: packet.snapshot_id.clone(),
        semantic_input_digest: packet.semantic_input_digest,
        provider: SemanticRevisionProvider {
            kind: packet.provider.kind,
            model: packet.provider.model.clone(),
            effort: packet.provider.effort,
        },
        prompt_policy_version: packet.prompt_policy_version.clone(),
        project: proposal.project,
        areas: approved_areas,
        assignments: approved_assignments,
        unassigned_regions: proposal.unassigned_regions,
        warnings: proposal.warnings,
    })
}

fn create_area_id(
    packet: &BaseSemanticPacket,
    area: &AreaProposal,
    parent: Option<&SemanticAreaId>,
    members: &[RegionId],
) -> Result<SemanticAreaId, SemanticCompileError> {
    let level = area.level.to_string();
    // Semantic IDs already length-prefix every component before hashing. Keep
    // each member as its own component instead of inventing a newline-delimited
    // aggregate: newlines are intentionally forbidden by the identity contract,
    // and a delimiter would also make a set identity depend on ad-hoc escaping.
    let mut member_ids = members.iter().map(RegionId::as_str).collect::<Vec<_>>();
    member_ids.sort_unstable();
    let mut components = Vec::with_capacity(3 + member_ids.len());
    components.push(packet.snapshot_id.as_str());
    components.push(level.as_str());
    components.push(parent.map(SemanticAreaId::as_str).unwrap_or("<root>"));
    components.extend(member_ids);
    SemanticAreaId::from_components(&components).map_err(|err| {
        error(
            SemanticCompileErrorCode::InvalidPacket,
            format!("areas[{}].derivedAreaId", area.proposal_key),
            err.to_string(),
        )
    })
}

fn validate_strings(
    values: &[String],
    path: &str,
    max_items: usize,
    max_bytes: usize,
) -> Result<(), SemanticCompileError> {
    if values.len() > max_items {
        return Err(error(
            SemanticCompileErrorCode::InvalidProviderOutput,
            path,
            format!("collection exceeds {max_items} items"),
        ));
    }
    ensure_unique(values.iter().map(|value| value.to_lowercase()), path)?;
    for (index, value) in values.iter().enumerate() {
        validate_text(value, &format!("{path}[{index}]"), max_bytes)?;
    }
    Ok(())
}

fn is_placeholder_label(label: &str) -> bool {
    let normalized = label.trim().to_lowercase();
    normalized.starts_with("주요 영역")
        || normalized.starts_with("domain ")
        || matches!(
            normalized.as_str(),
            "misc" | "other" | "unknown" | "unassigned" | "기타" | "미분류"
        )
}

fn reject_prescriptive_text(value: &str, path: &str) -> Result<(), SemanticCompileError> {
    let normalized = value.to_lowercase();
    const FORBIDDEN: &[&str] = &[
        "리팩터링해야",
        "분리해야",
        "삭제해야",
        "이동해야",
        "새 api는",
        "should be refactored",
        "should be moved",
        "should be deleted",
    ];
    if FORBIDDEN.iter().any(|phrase| normalized.contains(phrase)) {
        return Err(error(
            SemanticCompileErrorCode::InvalidProviderOutput,
            path,
            "semantic summaries must describe current code rather than prescribe changes",
        ));
    }
    Ok(())
}

fn unexpected(path: impl Into<String>, message: impl Into<String>) -> SemanticCompileError {
    error(SemanticCompileErrorCode::UnexpectedReference, path, message)
}

fn evidence_mismatch(path: impl Into<String>, message: impl Into<String>) -> SemanticCompileError {
    error(SemanticCompileErrorCode::EvidenceMismatch, path, message)
}
