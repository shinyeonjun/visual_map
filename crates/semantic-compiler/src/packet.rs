use crate::{SemanticCompileError, SemanticCompileErrorCode};
use codebase_semantic_model::{
    BaseSemanticInput, BoundaryRelationCount, ScopeReceipt, StaticRegionSummary, TracePathState,
    TracePathSummary,
};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn prepare_input(
    mut input: BaseSemanticInput,
    receipt: &ScopeReceipt,
) -> Result<BaseSemanticInput, SemanticCompileError> {
    validate_scope_receipt(receipt)?;
    validate_input(&input)?;
    canonicalize_input(&mut input);
    Ok(input)
}

fn validate_scope_receipt(receipt: &ScopeReceipt) -> Result<(), SemanticCompileError> {
    if receipt.included > receipt.total {
        return Err(error(
            SemanticCompileErrorCode::InvalidPacket,
            "scopeReceipt.included",
            "included may not exceed total",
        ));
    }
    if receipt.truncated {
        validate_text(
            receipt.reason.as_deref().unwrap_or_default(),
            "scopeReceipt.reason",
            256,
        )?;
        return Err(error(
            SemanticCompileErrorCode::InvalidPacket,
            "scopeReceipt.truncated",
            "base semantic v1 may not publish a partial map; use a future partition and global reconciliation contract",
        ));
    } else {
        if receipt.reason.is_some() {
            return Err(error(
                SemanticCompileErrorCode::NonCanonicalValue,
                "scopeReceipt.reason",
                "a complete packet may not carry a truncation reason",
            ));
        }
        if receipt.included != receipt.total {
            return Err(error(
                SemanticCompileErrorCode::InvalidPacket,
                "scopeReceipt",
                "a non-truncated packet must include its complete declared scope",
            ));
        }
    }
    Ok(())
}

fn validate_input(input: &BaseSemanticInput) -> Result<(), SemanticCompileError> {
    validate_text(&input.repository.name, "input.repository.name", 256)?;
    if input.repository.languages.is_empty() {
        return Err(error(
            SemanticCompileErrorCode::InvalidPacket,
            "input.repository.languages",
            "at least one source language is required",
        ));
    }
    if input.regions.is_empty() {
        return Err(error(
            SemanticCompileErrorCode::InvalidPacket,
            "input.regions",
            "base semantic compilation requires at least one static region",
        ));
    }

    ensure_unique(
        input.regions.iter().map(|region| &region.region_id),
        "input.regions[].regionId",
    )?;
    ensure_unique(
        input.anchors.iter().map(|anchor| &anchor.fact_id),
        "input.anchors[].factId",
    )?;
    ensure_unique(
        input
            .boundary_relations
            .iter()
            .map(|relation| &relation.bundle_id),
        "input.boundaryRelations[].bundleId",
    )?;
    ensure_unique(
        input
            .representative_traces
            .iter()
            .map(|trace| &trace.trace_path_id),
        "input.representativeTraces[].tracePathId",
    )?;
    ensure_unique(
        input.excerpts.iter().map(|excerpt| &excerpt.evidence_id),
        "input.excerpts[].evidenceId",
    )?;

    let regions: BTreeMap<_, _> = input
        .regions
        .iter()
        .map(|region| (&region.region_id, region))
        .collect();
    let region_ids: BTreeSet<_> = regions.keys().copied().collect();

    ensure_unique(
        input.repository.root_region_ids.iter(),
        "input.repository.rootRegionIds",
    )?;
    if input.repository.root_region_ids.is_empty() {
        return Err(error(
            SemanticCompileErrorCode::InvalidPacket,
            "input.repository.rootRegionIds",
            "at least one root region is required",
        ));
    }
    for root in &input.repository.root_region_ids {
        if !region_ids.contains(root) {
            return Err(missing(
                "input.repository.rootRegionIds",
                format!("root region {root} is not present in input.regions"),
            ));
        }
    }

    for (index, region) in input.regions.iter().enumerate() {
        validate_region(index, region, &region_ids)?;
    }
    validate_region_hierarchy(&regions)?;

    let anchors: BTreeMap<_, _> = input
        .anchors
        .iter()
        .map(|anchor| (&anchor.fact_id, anchor))
        .collect();
    for (index, anchor) in input.anchors.iter().enumerate() {
        if !region_ids.contains(&anchor.owner_region_id) {
            return Err(missing(
                format!("input.anchors[{index}].ownerRegionId"),
                format!("region {} is not present", anchor.owner_region_id),
            ));
        }
        validate_text(&anchor.name, &format!("input.anchors[{index}].name"), 512)?;
        validate_optional_text(
            anchor.qualified_name.as_deref(),
            &format!("input.anchors[{index}].qualifiedName"),
            2048,
        )?;
        validate_optional_text(
            anchor.signature.as_deref(),
            &format!("input.anchors[{index}].signature"),
            4096,
        )?;
        ensure_unique(
            anchor.static_roles.iter(),
            &format!("input.anchors[{index}].staticRoles"),
        )?;
        ensure_unique(
            anchor.evidence_ids.iter(),
            &format!("input.anchors[{index}].evidenceIds"),
        )?;
    }
    for (index, region) in input.regions.iter().enumerate() {
        ensure_unique(
            region.anchor_fact_ids.iter(),
            &format!("input.regions[{index}].anchorFactIds"),
        )?;
        for fact_id in &region.anchor_fact_ids {
            let Some(anchor) = anchors.get(fact_id) else {
                return Err(missing(
                    format!("input.regions[{index}].anchorFactIds"),
                    format!("anchor fact {fact_id} is not present"),
                ));
            };
            if anchor.owner_region_id != region.region_id {
                return Err(error(
                    SemanticCompileErrorCode::InvalidPacket,
                    format!("input.regions[{index}].anchorFactIds"),
                    format!("anchor fact {fact_id} belongs to another region"),
                ));
            }
        }
    }
    for (index, anchor) in input.anchors.iter().enumerate() {
        if !regions[&anchor.owner_region_id]
            .anchor_fact_ids
            .contains(&anchor.fact_id)
        {
            return Err(error(
                SemanticCompileErrorCode::InvalidPacket,
                format!("input.anchors[{index}]"),
                "the owner region does not list this anchor fact",
            ));
        }
    }

    let bundles: BTreeMap<_, _> = input
        .boundary_relations
        .iter()
        .map(|relation| (&relation.bundle_id, relation))
        .collect();
    for (index, relation) in input.boundary_relations.iter().enumerate() {
        if !region_ids.contains(&relation.source_region_id)
            || !region_ids.contains(&relation.target_region_id)
        {
            return Err(missing(
                format!("input.boundaryRelations[{index}]"),
                "boundary endpoint region is not present",
            ));
        }
        if relation.source_region_id == relation.target_region_id {
            return Err(error(
                SemanticCompileErrorCode::InvalidPacket,
                format!("input.boundaryRelations[{index}]"),
                "boundary relation endpoints must be different regions",
            ));
        }
        if relation.families.is_empty() {
            return Err(error(
                SemanticCompileErrorCode::InvalidPacket,
                format!("input.boundaryRelations[{index}].families"),
                "a boundary relation must contain at least one counted family",
            ));
        }
        ensure_unique(
            relation.families.iter().map(boundary_count_key),
            &format!("input.boundaryRelations[{index}].families"),
        )?;
        if relation
            .families
            .iter()
            .any(|family| family.relation_count == 0)
        {
            return Err(error(
                SemanticCompileErrorCode::InvalidPacket,
                format!("input.boundaryRelations[{index}].families"),
                "zero-count relation families must be omitted",
            ));
        }
        ensure_unique(
            relation.representative_edge_ids.iter(),
            &format!("input.boundaryRelations[{index}].representativeEdgeIds"),
        )?;
        ensure_unique(
            relation.evidence_ids.iter(),
            &format!("input.boundaryRelations[{index}].evidenceIds"),
        )?;
    }

    for (index, region) in input.regions.iter().enumerate() {
        for bundle_id in &region.outbound_bundle_ids {
            let Some(bundle) = bundles.get(bundle_id) else {
                return Err(missing(
                    format!("input.regions[{index}].outboundBundleIds"),
                    format!("bundle {bundle_id} is not present"),
                ));
            };
            if bundle.source_region_id != region.region_id {
                return Err(error(
                    SemanticCompileErrorCode::InvalidPacket,
                    format!("input.regions[{index}].outboundBundleIds"),
                    format!("bundle {bundle_id} has a different source region"),
                ));
            }
        }
        for bundle_id in &region.inbound_bundle_ids {
            let Some(bundle) = bundles.get(bundle_id) else {
                return Err(missing(
                    format!("input.regions[{index}].inboundBundleIds"),
                    format!("bundle {bundle_id} is not present"),
                ));
            };
            if bundle.target_region_id != region.region_id {
                return Err(error(
                    SemanticCompileErrorCode::InvalidPacket,
                    format!("input.regions[{index}].inboundBundleIds"),
                    format!("bundle {bundle_id} has a different target region"),
                ));
            }
        }
    }

    let traces: BTreeMap<_, _> = input
        .representative_traces
        .iter()
        .map(|trace| (&trace.trace_path_id, trace))
        .collect();
    for (index, trace) in input.representative_traces.iter().enumerate() {
        if trace.ordered_fact_ids.first() != Some(&trace.entry_fact_id) {
            return Err(error(
                SemanticCompileErrorCode::InvalidPacket,
                format!("input.representativeTraces[{index}].entryFactId"),
                "entryFactId must be the first ordered fact",
            ));
        }
        if trace.ordered_fact_ids.len() != trace.ordered_edge_ids.len().saturating_add(1) {
            return Err(error(
                SemanticCompileErrorCode::InvalidPacket,
                format!("input.representativeTraces[{index}]"),
                "an ordered trace requires exactly one fewer edge than facts",
            ));
        }
        ensure_unique(
            trace.ordered_edge_ids.iter(),
            &format!("input.representativeTraces[{index}].orderedEdgeIds"),
        )?;
        let expected_id =
            TracePathSummary::stable_id(&trace.entry_fact_id, &trace.ordered_edge_ids).map_err(
                |identity_error| {
                    error(
                        SemanticCompileErrorCode::InvalidPacket,
                        format!("input.representativeTraces[{index}].tracePathId"),
                        format!("trace identity could not be derived: {identity_error}"),
                    )
                },
            )?;
        if trace.trace_path_id != expected_id {
            return Err(error(
                SemanticCompileErrorCode::NonCanonicalValue,
                format!("input.representativeTraces[{index}].tracePathId"),
                "tracePathId must be derived from entryFactId and the ordered edge sequence",
            ));
        }
        if trace.evidence_ids.is_empty() {
            return Err(error(
                SemanticCompileErrorCode::InvalidPacket,
                format!("input.representativeTraces[{index}].evidenceIds"),
                "a static trace requires at least one canonical evidence id",
            ));
        }
        let closing_cycle = trace.ordered_fact_ids.last().is_some_and(|last| {
            trace.ordered_fact_ids[..trace.ordered_fact_ids.len().saturating_sub(1)].contains(last)
        });
        let unique_prefix_length = match trace.state {
            TracePathState::Cycle => trace.ordered_fact_ids.len().saturating_sub(1),
            _ => trace.ordered_fact_ids.len(),
        };
        ensure_unique(
            trace.ordered_fact_ids[..unique_prefix_length].iter(),
            &format!("input.representativeTraces[{index}].orderedFactIds"),
        )?;
        if matches!(trace.state, TracePathState::Cycle) != closing_cycle {
            return Err(error(
                SemanticCompileErrorCode::InvalidPacket,
                format!("input.representativeTraces[{index}].state"),
                "cycle state must match one explicit closing repetition of an earlier fact",
            ));
        }
        ensure_unique(
            trace.evidence_ids.iter(),
            &format!("input.representativeTraces[{index}].evidenceIds"),
        )?;
    }
    for (index, region) in input.regions.iter().enumerate() {
        ensure_unique(
            region.representative_trace_path_ids.iter(),
            &format!("input.regions[{index}].representativeTracePathIds"),
        )?;
        for trace_id in &region.representative_trace_path_ids {
            if !traces.contains_key(trace_id) {
                return Err(missing(
                    format!("input.regions[{index}].representativeTracePathIds"),
                    format!("trace {trace_id} is not present"),
                ));
            }
        }
    }

    for (index, excerpt) in input.excerpts.iter().enumerate() {
        if !region_ids.contains(&excerpt.owner_region_id) {
            return Err(missing(
                format!("input.excerpts[{index}].ownerRegionId"),
                format!("region {} is not present", excerpt.owner_region_id),
            ));
        }
        if excerpt.start_line == 0 || excerpt.end_line < excerpt.start_line {
            return Err(error(
                SemanticCompileErrorCode::InvalidPacket,
                format!("input.excerpts[{index}]"),
                "source excerpt lines are one-based and endLine must not precede startLine",
            ));
        }
        validate_source_excerpt(
            &excerpt.text,
            &format!("input.excerpts[{index}].text"),
            32 * 1024,
        )?;
    }

    Ok(())
}

fn validate_region(
    index: usize,
    region: &StaticRegionSummary,
    region_ids: &BTreeSet<&codebase_semantic_model::RegionId>,
) -> Result<(), SemanticCompileError> {
    validate_text(
        &region.structural_label,
        &format!("input.regions[{index}].structuralLabel"),
        512,
    )?;
    if region.path_roots.is_empty() {
        return Err(error(
            SemanticCompileErrorCode::InvalidPacket,
            format!("input.regions[{index}].pathRoots"),
            "a static region requires at least one source path",
        ));
    }
    if region.languages.is_empty() {
        return Err(error(
            SemanticCompileErrorCode::InvalidPacket,
            format!("input.regions[{index}].languages"),
            "a static region requires at least one source language",
        ));
    }
    if region.file_count == 0 {
        return Err(error(
            SemanticCompileErrorCode::InvalidPacket,
            format!("input.regions[{index}].fileCount"),
            "empty structural containers are not AI grouping units",
        ));
    }
    if region
        .parent_region_id
        .as_ref()
        .is_some_and(|parent| !region_ids.contains(parent))
    {
        return Err(missing(
            format!("input.regions[{index}].parentRegionId"),
            "parent region is not present",
        ));
    }
    if region.parent_region_id.as_ref() == Some(&region.region_id) {
        return Err(error(
            SemanticCompileErrorCode::InvalidHierarchy,
            format!("input.regions[{index}].parentRegionId"),
            "a region may not parent itself",
        ));
    }
    ensure_unique(
        region.path_roots.iter(),
        &format!("input.regions[{index}].pathRoots"),
    )?;
    ensure_unique(
        region.languages.iter(),
        &format!("input.regions[{index}].languages"),
    )?;
    ensure_unique(
        region.inbound_bundle_ids.iter(),
        &format!("input.regions[{index}].inboundBundleIds"),
    )?;
    ensure_unique(
        region.outbound_bundle_ids.iter(),
        &format!("input.regions[{index}].outboundBundleIds"),
    )?;
    Ok(())
}

fn validate_region_hierarchy(
    regions: &BTreeMap<
        &codebase_semantic_model::RegionId,
        &codebase_semantic_model::StaticRegionSummary,
    >,
) -> Result<(), SemanticCompileError> {
    for start in regions.keys() {
        let mut seen = BTreeSet::new();
        let mut current = Some(*start);
        while let Some(region_id) = current {
            if !seen.insert(region_id) {
                return Err(error(
                    SemanticCompileErrorCode::InvalidHierarchy,
                    "input.regions[].parentRegionId",
                    format!("structural region hierarchy contains a cycle at {region_id}"),
                ));
            }
            current = regions
                .get(region_id)
                .and_then(|region| region.parent_region_id.as_ref());
        }
    }
    Ok(())
}

fn canonicalize_input(input: &mut BaseSemanticInput) {
    input.repository.languages.sort();
    input.repository.framework_fact_ids.sort();
    input.repository.root_region_ids.sort();

    for region in &mut input.regions {
        region.path_roots.sort();
        region.languages.sort();
        region.anchor_fact_ids.sort();
        region.representative_trace_path_ids.sort();
        region.inbound_bundle_ids.sort();
        region.outbound_bundle_ids.sort();
    }
    input
        .regions
        .sort_by(|left, right| left.region_id.cmp(&right.region_id));

    for anchor in &mut input.anchors {
        anchor.static_roles.sort();
        anchor.evidence_ids.sort();
    }
    input
        .anchors
        .sort_by(|left, right| left.fact_id.cmp(&right.fact_id));

    for relation in &mut input.boundary_relations {
        relation.families.sort_by_key(boundary_count_key);
        relation.representative_edge_ids.sort();
        relation.evidence_ids.sort();
    }
    input
        .boundary_relations
        .sort_by(|left, right| left.bundle_id.cmp(&right.bundle_id));

    for trace in &mut input.representative_traces {
        trace.evidence_ids.sort();
    }
    input
        .representative_traces
        .sort_by(|left, right| left.trace_path_id.cmp(&right.trace_path_id));
    input
        .excerpts
        .sort_by(|left, right| left.evidence_id.cmp(&right.evidence_id));

    if let Some(previous) = &mut input.previous_revision {
        for area in &mut previous.areas {
            area.member_region_ids.sort();
        }
        previous
            .areas
            .sort_by(|left, right| left.area_id.cmp(&right.area_id));
        previous
            .assignments
            .sort_by(|left, right| left.region_id.cmp(&right.region_id));
    }
}

fn boundary_count_key(count: &BoundaryRelationCount) -> (String, String, String) {
    (
        serde_json::to_string(&count.family).unwrap_or_default(),
        serde_json::to_string(&count.truth).unwrap_or_default(),
        serde_json::to_string(&count.dispatch).unwrap_or_default(),
    )
}

pub(crate) fn validate_text(
    value: &str,
    path: &str,
    max_bytes: usize,
) -> Result<(), SemanticCompileError> {
    if value.is_empty() || value.trim() != value {
        return Err(error(
            SemanticCompileErrorCode::InvalidText,
            path,
            "value must be non-empty without surrounding whitespace",
        ));
    }
    if value.len() > max_bytes {
        return Err(error(
            SemanticCompileErrorCode::InvalidText,
            path,
            format!("value exceeds {max_bytes} UTF-8 bytes"),
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(error(
            SemanticCompileErrorCode::InvalidText,
            path,
            "value contains a forbidden control character",
        ));
    }
    Ok(())
}

pub(crate) fn validate_message(
    value: &str,
    path: &str,
    max_bytes: usize,
) -> Result<(), SemanticCompileError> {
    if value.is_empty() || value.trim() != value || value.len() > max_bytes {
        return Err(error(
            SemanticCompileErrorCode::InvalidText,
            path,
            format!("message must be trimmed, non-empty, and at most {max_bytes} UTF-8 bytes"),
        ));
    }
    if value
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(error(
            SemanticCompileErrorCode::InvalidText,
            path,
            "message contains a forbidden control character",
        ));
    }
    Ok(())
}

/// Validates source code evidence without pretending that code is polished
/// prose. Indentation and a final line ending are meaningful source bytes and
/// therefore remain intact. Only an all-whitespace excerpt, an oversized
/// payload, or unsafe control bytes are rejected.
fn validate_source_excerpt(
    value: &str,
    path: &str,
    max_bytes: usize,
) -> Result<(), SemanticCompileError> {
    if value.trim().is_empty() {
        return Err(error(
            SemanticCompileErrorCode::InvalidText,
            path,
            "source excerpt must contain at least one non-whitespace character",
        ));
    }
    if value.len() > max_bytes {
        return Err(error(
            SemanticCompileErrorCode::InvalidText,
            path,
            format!("source excerpt exceeds {max_bytes} UTF-8 bytes"),
        ));
    }
    if value
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(error(
            SemanticCompileErrorCode::InvalidText,
            path,
            "source excerpt contains a forbidden control character",
        ));
    }
    Ok(())
}

pub(crate) fn validate_optional_text(
    value: Option<&str>,
    path: &str,
    max_bytes: usize,
) -> Result<(), SemanticCompileError> {
    if let Some(value) = value {
        validate_text(value, path, max_bytes)?;
    }
    Ok(())
}

pub(crate) fn ensure_unique<T: Ord>(
    values: impl IntoIterator<Item = T>,
    path: &str,
) -> Result<(), SemanticCompileError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(error(
                SemanticCompileErrorCode::DuplicateIdentifier,
                path,
                "collection contains a duplicate value",
            ));
        }
    }
    Ok(())
}

pub(crate) fn error(
    code: SemanticCompileErrorCode,
    path: impl Into<String>,
    message: impl Into<String>,
) -> SemanticCompileError {
    SemanticCompileError::new(code, path, message)
}

fn missing(path: impl Into<String>, message: impl Into<String>) -> SemanticCompileError {
    error(SemanticCompileErrorCode::MissingReference, path, message)
}
