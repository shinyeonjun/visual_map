use super::framework::ingest_framework_routes;
use super::store::{BundleFinalizationInput, BundleStore};
use super::verification::ingest_test_relations;
use super::{
    CanonicalLanguageEmission, CanonicalLanguageInput, CanonicalLinkerReceipt,
    LINKER_RECEIPT_SCHEMA,
};
use crate::static_pipeline::language_ir::artifact::visit_language_ir_records;
use codebase_fact_model::analysis::AnalysisUnit;
use codebase_fact_model::coverage::{
    AnalysisCapability, AnalysisGap, AnalysisScope, AnalysisUnitReceipt, GapCode,
};
use codebase_fact_model::evidence::{
    ArtifactLocation, EvidenceKind, EvidenceLocation, EvidenceProducer, EvidenceProducerKind,
    FactEvidence,
};
use codebase_fact_model::fact_graph::{
    DispatchKind, FactEdge, FactEdgeKind, FactNode, FactNodeKind, FactTruth, ResolutionMethod,
    Visibility,
};
use codebase_fact_model::identity::{
    AnalysisUnitId, EvidenceId, FactNodeId, Sha256Digest, SnapshotId,
};
use codebase_fact_model::language_ir::{
    IrEndpoint, IrRelation, LanguageIrHeader, LanguageIrRecord, LanguageIrStreamValidator,
    LanguageRelationKind,
};
use codebase_fact_model::source::{RepositoryPath, SourceFileKind, SourceFlags};
use codebase_fact_model::validation::Validate;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::time::Instant;

const FILE_STRUCTURE_STRATEGY: &str = "source-manifest-file";

pub(crate) fn normalize_language_ir(
    input: CanonicalLanguageInput<'_>,
) -> Result<CanonicalLanguageEmission, String> {
    let timing_enabled = std::env::var_os("CODE_MEMORY_CANONICAL_TIMING").is_some();
    let total_started = Instant::now();
    let mut phase_started = Instant::now();
    input
        .manifest
        .validate()
        .map_err(|error| format!("invalid Source Manifest before canonical linking: {error}"))?;
    input
        .plan
        .validate_against(input.manifest)
        .map_err(|error| format!("invalid Analysis Plan before canonical linking: {error}"))?;
    let expected_snapshot = SnapshotId::from_execution_inputs(
        &input.manifest.workspace_id,
        input.manifest.manifest_digest,
        input.plan.plan_digest,
        input.provider_set_digest,
        input.execution_context_set_digest,
    )
    .map_err(|error| format!("cannot compute canonical snapshot identity: {error}"))?;
    if &expected_snapshot != input.ir_snapshot_id {
        return Err(format!(
            "Language IR snapshot does not match executed canonical inputs: expected={} actual={}",
            expected_snapshot, input.ir_snapshot_id
        ));
    }
    verify_ir_artifact(
        input.ir_path,
        input.ir_content_digest,
        input.ir_record_count,
    )?;
    emit_canonical_timing(timing_enabled, "verify_ir", phase_started, total_started);
    phase_started = Instant::now();

    let units = input
        .plan
        .units
        .iter()
        .map(|unit| (unit.id.clone(), unit))
        .collect::<BTreeMap<_, _>>();
    let repository_id = contract(
        FactNode::stable_id(
            FactNodeKind::Repository,
            None,
            None,
            input.manifest.workspace_id.as_str(),
            None,
        ),
        "cannot build canonical repository identity",
    )?;
    let mut store = BundleStore::create(
        input.project_root,
        input.output_root,
        expected_snapshot.clone(),
    )?;
    let empty_framework_ir;
    let framework_ir = match input.framework_ir {
        Some(framework_ir) => framework_ir,
        None => {
            empty_framework_ir = crate::static_pipeline::framework_ir::FrameworkIr::empty(
                &expected_snapshot,
                input.plan,
            );
            &empty_framework_ir
        }
    };
    let empty_test_ir;
    let test_ir = match input.test_ir {
        Some(test_ir) => test_ir,
        None => {
            empty_test_ir =
                crate::static_pipeline::test_ir::TestIr::empty(&expected_snapshot, input.plan);
            &empty_test_ir
        }
    };

    for scope in &input.manifest.scopes {
        store.insert_source_scope_coverage(scope)?;
    }
    let provider_definition_identity_count = ingest_receipts_structure_and_definitions(
        &mut store,
        &input,
        &units,
        &repository_id,
        &expected_snapshot,
    )?;
    emit_canonical_timing(
        timing_enabled,
        "structure_receipts_and_definitions",
        phase_started,
        total_started,
    );
    phase_started = Instant::now();
    materialize_definition_nodes(&mut store, &units, &expected_snapshot)?;
    emit_canonical_timing(
        timing_enabled,
        "materialize_definitions",
        phase_started,
        total_started,
    );
    phase_started = Instant::now();
    let canonical_definition_node_count = store.canonical_definition_node_count()?;
    let relation_counts = link_relations(
        &mut store,
        input.ir_path,
        &units,
        &repository_id,
        &expected_snapshot,
    )?;
    emit_canonical_timing(
        timing_enabled,
        "link_relations",
        phase_started,
        total_started,
    );
    phase_started = Instant::now();
    let framework_counts = ingest_framework_routes(
        &mut store,
        framework_ir,
        &units,
        &repository_id,
        &expected_snapshot,
    )?;
    let test_counts = ingest_test_relations(&mut store, test_ir, &units, &expected_snapshot)?;
    emit_canonical_timing(
        timing_enabled,
        "framework_and_tests",
        phase_started,
        total_started,
    );
    phase_started = Instant::now();
    store.retain_relevant_nodes_and_evidence()?;
    create_declaration_edges(&mut store, &units)?;
    store.prune_unreferenced_evidence()?;
    let retained_definition_node_count = store.retained_definition_count()?;
    let invariants = store.validate_invariants()?;
    let semantic_digest = store.semantic_digest()?;
    emit_canonical_timing(
        timing_enabled,
        "retain_validate_digest",
        phase_started,
        total_started,
    );
    phase_started = Instant::now();
    let merged_node_count = store.merged_node_count();
    let merged_edge_count = store.merged_edge_count();
    let finalization = store.finish(
        BundleFinalizationInput {
            workspace_id: input.manifest.workspace_id.clone(),
            source_manifest_digest: input.manifest.manifest_digest,
            config_digest: input.plan.config_digest,
            analysis_plan_digest: input.plan.plan_digest,
            provider_set_digest: input.provider_set_digest,
            execution_context_set_digest: input.execution_context_set_digest,
        },
        semantic_digest,
    )?;
    emit_canonical_timing(
        timing_enabled,
        "finalize_sqlite",
        phase_started,
        total_started,
    );
    let receipt = CanonicalLinkerReceipt {
        schema: LINKER_RECEIPT_SCHEMA,
        snapshot_id: expected_snapshot,
        language_ir_content_digest: input.ir_content_digest,
        language_ir_record_count: input.ir_record_count,
        provider_definition_identity_count,
        canonical_definition_node_count,
        retained_definition_node_count,
        pruned_definition_node_count: canonical_definition_node_count
            .saturating_sub(retained_definition_node_count),
        resolved_relation_count: relation_counts.resolved,
        unresolved_relation_count: relation_counts.unresolved,
        framework_route_node_count: framework_counts.route_node_count,
        framework_exposes_edge_count: framework_counts.exposes_edge_count,
        framework_handles_edge_count: framework_counts.handles_edge_count,
        framework_unresolved_handler_count: framework_counts.unresolved_handler_count,
        framework_ir_content_digest: framework_ir.receipt.content_digest,
        test_case_node_count: test_counts.test_case_node_count,
        tests_edge_count: test_counts.tests_edge_count,
        unlinked_test_case_count: test_counts.unlinked_test_case_count,
        test_ir_content_digest: test_ir.receipt.content_digest,
        merged_node_count,
        merged_edge_count,
        dangling_endpoint_count: invariants.dangling_endpoint_count,
        confirmed_without_evidence_count: invariants.confirmed_without_evidence_count,
        duplicate_logical_edge_count: invariants.duplicate_logical_edge_count,
        semantic_digest,
    };
    Ok(CanonicalLanguageEmission {
        receipt,
        manifest: finalization.manifest,
        artifact: finalization.artifact,
    })
}

fn emit_canonical_timing(enabled: bool, phase: &str, started: Instant, total: Instant) {
    if enabled {
        eprintln!(
            "timing stage=canonical_linker phase={phase} elapsed_ms={} total_ms={}",
            started.elapsed().as_millis(),
            total.elapsed().as_millis()
        );
    }
}

fn ingest_receipts_structure_and_definitions(
    store: &mut BundleStore,
    input: &CanonicalLanguageInput<'_>,
    units: &BTreeMap<AnalysisUnitId, &AnalysisUnit>,
    repository_id: &FactNodeId,
    snapshot_id: &SnapshotId,
) -> Result<u64, String> {
    let mut validator = None::<LanguageIrStreamValidator>;
    let mut current_header = None::<LanguageIrHeader>;
    let mut seen_units = BTreeSet::new();
    let mut first_structure_evidence = None::<EvidenceId>;
    let mut record_count = 0_u64;
    let mut registered_definition_count = 0_u64;
    let mut pending_definitions = Vec::new();
    visit_language_ir_records(input.ir_path, |record| {
        record_count += 1;
        if matches!(record, LanguageIrRecord::Header(_)) {
            if let Some(previous) = validator.take() {
                previous
                    .finish()
                    .map_err(|error| format!("invalid concatenated Language IR stream: {error}"))?;
            }
            validator = Some(LanguageIrStreamValidator::default());
        }
        validator
            .as_mut()
            .ok_or_else(|| "Language IR artifact does not start with a header".to_string())?
            .push(&record)
            .map_err(|error| format!("invalid Language IR before canonical linking: {error}"))?;

        match record {
            LanguageIrRecord::Header(header) => {
                let header = *header;
                if header.snapshot_id != *snapshot_id
                    || header.source_manifest_digest != input.manifest.manifest_digest
                {
                    return Err(format!(
                        "Language IR header {} belongs to another snapshot",
                        header.unit.id
                    ));
                }
                let planned = units.get(&header.unit.id).ok_or_else(|| {
                    format!(
                        "Language IR header references unplanned unit {}",
                        header.unit.id
                    )
                })?;
                if **planned != header.unit {
                    return Err(format!(
                        "Language IR unit {} differs from the sealed Analysis Plan",
                        header.unit.id
                    ));
                }
                if !seen_units.insert(header.unit.id.clone()) {
                    return Err(format!(
                        "Language IR artifact repeats unit {}",
                        header.unit.id
                    ));
                }
                store.insert_header(&header.unit.id, &header)?;
                current_header = Some(header);
            }
            LanguageIrRecord::File(file) => {
                store.insert_file_coverage(&file)?;
                let unit_id = file
                    .unit_id
                    .as_ref()
                    .ok_or_else(|| "unit-scoped file coverage omitted unit ID".to_string())?;
                let unit = units
                    .get(unit_id)
                    .ok_or_else(|| format!("file coverage references unknown unit {unit_id}"))?;
                let language = file
                    .language
                    .ok_or_else(|| format!("unit file coverage omitted language: {}", file.path))?;
                if language != unit.language {
                    return Err(format!(
                        "file coverage language differs from unit {}",
                        unit.id
                    ));
                }
                let digest = file.content_digest.ok_or_else(|| {
                    format!(
                        "canonical file identity requires the census digest for {}",
                        file.path
                    )
                })?;
                let evidence = structural_file_evidence(&file.path, digest)?;
                first_structure_evidence.get_or_insert_with(|| evidence.id.clone());
                store.insert_evidence(&evidence)?;
                let node_id = contract(
                    FactNode::stable_id(
                        FactNodeKind::File,
                        Some(language),
                        Some(unit_id),
                        file.path.as_str(),
                        None,
                    ),
                    "cannot build canonical file identity",
                )?;
                let node = FactNode {
                    id: node_id.clone(),
                    snapshot_id: snapshot_id.clone(),
                    family: FactNodeKind::File.family(),
                    kind: FactNodeKind::File,
                    native_kind: Some(file.file_kind.as_str().to_string()),
                    qualified_name: file.path.as_str().to_string(),
                    display_name: file_display_name(&file.path),
                    signature: None,
                    details: None,
                    visibility: Visibility::Unknown,
                    language: Some(language),
                    analysis_unit_id: Some(unit_id.clone()),
                    parent_id: Some(repository_id.clone()),
                    definition_evidence_id: Some(evidence.id.clone()),
                    evidence_ids: vec![evidence.id.clone()],
                    roles: Vec::new(),
                    flags: flags_for_file_kind(file.file_kind),
                };
                store.insert_node(&node, true)?;
                store.register_file_identity(unit_id, language, &file.path, &node_id)?;
                store.insert_edge(&structural_edge(
                    snapshot_id,
                    repository_id,
                    &node_id,
                    FactEdgeKind::Contains,
                    Some(&unit.context.id),
                    ResolutionMethod::Manifest,
                    vec![evidence.id],
                )?)?;
            }
            LanguageIrRecord::Evidence(evidence) => store.insert_evidence(&evidence)?,
            LanguageIrRecord::Definition(definition) => {
                if store
                    .source_evidence_path(definition.definition_evidence_id.as_str())?
                    .is_some()
                {
                    registered_definition_count +=
                        u64::from(register_definition(store, &definition, units)?);
                } else {
                    // The current producer emits evidence before definitions.
                    // Keep the shared IR contract order-independent by
                    // deferring the exceptional forward reference instead of
                    // silently tightening the accepted stream grammar.
                    pending_definitions.push(definition);
                }
            }
            LanguageIrRecord::CapabilityReceipt(receipt) => {
                store.insert_capability_receipt(&receipt)?
            }
            LanguageIrRecord::Gap(gap) => store.insert_gap(&gap)?,
            LanguageIrRecord::Issue(issue) => store.insert_issue(&issue)?,
            LanguageIrRecord::Complete(completion) => {
                let header = current_header
                    .take()
                    .ok_or_else(|| "Language IR completion has no active header".to_string())?;
                if header.unit.id != completion.unit_id {
                    return Err("Language IR completion closes another unit".to_string());
                }
                store.insert_completion(&completion.unit_id, &completion)?;
                store.insert_analysis_unit_receipt(&AnalysisUnitReceipt {
                    unit: header.unit,
                    provider: header.provider,
                    completion,
                })?;
            }
            LanguageIrRecord::Relation(_) => {}
        }
        Ok(())
    })?;
    validator
        .ok_or_else(|| "Language IR artifact is empty".to_string())?
        .finish()
        .map_err(|error| format!("incomplete Language IR artifact: {error}"))?;
    if current_header.is_some() {
        return Err("Language IR artifact ended before closing its current unit".to_string());
    }
    if record_count != input.ir_record_count {
        return Err(format!(
            "Language IR record count changed before canonical linking: expected={} actual={record_count}",
            input.ir_record_count
        ));
    }
    let planned_units = units.keys().cloned().collect::<BTreeSet<_>>();
    if seen_units != planned_units {
        let missing = planned_units
            .difference(&seen_units)
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        return Err(format!(
            "canonical publication requires one closed Language IR stream per Analysis Plan unit; missing={}",
            missing.join(",")
        ));
    }
    for definition in pending_definitions {
        registered_definition_count += u64::from(register_definition(store, &definition, units)?);
    }
    let evidence_id = first_structure_evidence.ok_or_else(|| {
        "canonical repository node requires at least one manifest-backed file".to_string()
    })?;
    let display_name = if input.repository_display_name.trim().is_empty() {
        "Repository".to_string()
    } else {
        input.repository_display_name.trim().to_string()
    };
    store.insert_node(
        &FactNode {
            id: repository_id.clone(),
            snapshot_id: snapshot_id.clone(),
            family: FactNodeKind::Repository.family(),
            kind: FactNodeKind::Repository,
            native_kind: None,
            qualified_name: input.manifest.workspace_id.as_str().to_string(),
            display_name,
            signature: None,
            details: None,
            visibility: Visibility::Unknown,
            language: None,
            analysis_unit_id: None,
            parent_id: None,
            definition_evidence_id: Some(evidence_id.clone()),
            evidence_ids: vec![evidence_id],
            roles: Vec::new(),
            flags: SourceFlags::default(),
        },
        true,
    )?;
    Ok(registered_definition_count)
}

fn register_definition(
    store: &BundleStore,
    definition: &codebase_fact_model::language_ir::IrDefinition,
    units: &BTreeMap<AnalysisUnitId, &AnalysisUnit>,
) -> Result<bool, String> {
    let unit = units.get(&definition.unit_id).ok_or_else(|| {
        format!(
            "definition references unknown Analysis Plan unit {}",
            definition.unit_id
        )
    })?;
    if !store.has_evidence(definition.definition_evidence_id.as_str())? {
        return Err(format!(
            "definition {} references missing evidence {}",
            definition.symbol_id, definition.definition_evidence_id
        ));
    }
    if store
        .source_evidence_path(definition.definition_evidence_id.as_str())?
        .is_none()
    {
        return Err(format!(
            "definition {} requires exact source evidence",
            definition.symbol_id
        ));
    }
    let node_id = contract(
        FactNode::stable_id(
            definition.canonical_kind_hint,
            Some(unit.language),
            Some(&definition.unit_id),
            &definition.qualified_name,
            definition.signature.as_deref(),
        ),
        "cannot build canonical definition identity",
    )?;
    store.register_definition(definition, &node_id)
}

fn materialize_definition_nodes(
    store: &mut BundleStore,
    units: &BTreeMap<AnalysisUnitId, &AnalysisUnit>,
    snapshot_id: &SnapshotId,
) -> Result<(), String> {
    const PAGE_SIZE: usize = 512;
    let mut after = None::<FactNodeId>;
    loop {
        let node_ids = store.definition_node_ids_page(after.as_ref(), PAGE_SIZE)?;
        if node_ids.is_empty() {
            break;
        }
        for node_id in &node_ids {
            let mut definitions = store.definitions_for_node(node_id)?;
            definitions.sort_by(|left, right| {
                (left.unit_id.as_str(), left.symbol_id.as_str())
                    .cmp(&(right.unit_id.as_str(), right.symbol_id.as_str()))
            });
            let first = definitions
                .first()
                .ok_or_else(|| "empty canonical definition group".to_string())?;
            let unit = units
                .get(&first.unit_id)
                .ok_or_else(|| format!("definition references unknown unit {}", first.unit_id))?;
            let mut evidence_ids = definitions
                .iter()
                .map(|definition| definition.definition_evidence_id.clone())
                .collect::<Vec<_>>();
            evidence_ids.sort();
            evidence_ids.dedup();
            let parent_ids = definitions
                .iter()
                .filter_map(|definition| definition.parent_symbol_id.as_ref())
                .map(|parent| store.resolve_local_symbol(&first.unit_id, parent))
                .collect::<Result<Vec<_>, _>>()?;
            let resolved_parents = parent_ids.into_iter().flatten().collect::<BTreeSet<_>>();
            if resolved_parents.len() > 1 {
                return Err(format!(
                    "canonical definition {} has conflicting exact parents",
                    node_id
                ));
            }
            let declared_parent = definitions
                .iter()
                .any(|definition| definition.parent_symbol_id.is_some());
            let source_path = store
                .source_evidence_path(first.definition_evidence_id.as_str())?
                .ok_or_else(|| "definition evidence disappeared during linking".to_string())?;
            let file_parent = store
                .resolve_file_exact(&first.unit_id, unit.language, &source_path)?
                .ok_or_else(|| {
                    format!(
                        "definition {} has no canonical file identity for {}",
                        first.symbol_id, source_path
                    )
                })?;
            let parent_id = resolved_parents
                .iter()
                .next()
                .cloned()
                .unwrap_or_else(|| file_parent.clone());
            if declared_parent && resolved_parents.is_empty() {
                store.insert_gap(&AnalysisGap {
                code: GapCode::UnresolvedTarget,
                scope: AnalysisScope::NativeSymbol {
                    unit_id: first.unit_id.clone(),
                    symbol_id: first.symbol_id.clone(),
                },
                capability: Some(AnalysisCapability::Definitions),
                evidence_ids: evidence_ids.clone(),
                message: "A declared parent symbol was not registered; the definition remains attached to its exact source file".to_string(),
            })?;
            }
            let native_kind = definitions
                .iter()
                .map(|definition| definition.native_kind.as_str())
                .collect::<BTreeSet<_>>();
            let visibility = definitions
                .iter()
                .map(|definition| definition.visibility)
                .collect::<BTreeSet<_>>();
            let mut flags = SourceFlags::default();
            for definition in &definitions {
                flags.test |= definition.flags.test;
                flags.generated |= definition.flags.generated;
                flags.vendor |= definition.flags.vendor;
                flags.external |= definition.flags.external;
                if definition.canonical_kind_hint != first.canonical_kind_hint
                    || definition.qualified_name != first.qualified_name
                    || definition.display_name != first.display_name
                    || definition.signature != first.signature
                    || definition.unit_id != first.unit_id
                {
                    return Err(format!(
                        "canonical node {} merges incompatible definitions",
                        node_id
                    ));
                }
            }
            let baseline_relevant =
                !declared_parent && baseline_definition_kind(first.canonical_kind_hint);
            store.insert_node(
                &FactNode {
                    id: node_id.clone(),
                    snapshot_id: snapshot_id.clone(),
                    family: first.canonical_kind_hint.family(),
                    kind: first.canonical_kind_hint,
                    native_kind: (native_kind.len() == 1).then(|| {
                        native_kind
                            .iter()
                            .next()
                            .expect("one native kind")
                            .to_string()
                    }),
                    qualified_name: first.qualified_name.clone(),
                    display_name: first.display_name.clone(),
                    signature: first.signature.clone(),
                    details: None,
                    visibility: if visibility.len() == 1 {
                        *visibility.iter().next().expect("one visibility")
                    } else {
                        Visibility::Unknown
                    },
                    language: Some(unit.language),
                    analysis_unit_id: Some(first.unit_id.clone()),
                    parent_id: Some(parent_id),
                    definition_evidence_id: evidence_ids.first().cloned(),
                    evidence_ids,
                    roles: Vec::new(),
                    flags,
                },
                baseline_relevant,
            )?;
        }
        after = node_ids.last().cloned();
    }
    Ok(())
}

#[derive(Default)]
struct RelationCounts {
    resolved: u64,
    unresolved: u64,
}

fn link_relations(
    store: &mut BundleStore,
    ir_path: &Path,
    units: &BTreeMap<AnalysisUnitId, &AnalysisUnit>,
    repository_id: &FactNodeId,
    snapshot_id: &SnapshotId,
) -> Result<RelationCounts, String> {
    let mut counts = RelationCounts::default();
    visit_language_ir_records(ir_path, |record| {
        let LanguageIrRecord::Relation(relation) = record else {
            return Ok(());
        };
        if !units.contains_key(&relation.unit_id) {
            return Err(format!(
                "relation references unknown unit {}",
                relation.unit_id
            ));
        }
        for evidence_id in &relation.evidence_ids {
            if !store.has_evidence(evidence_id.as_str())? {
                return Err(format!(
                    "relation references missing evidence {}",
                    evidence_id
                ));
            }
        }
        let source = resolve_endpoint(
            store,
            &relation,
            &relation.source,
            units,
            repository_id,
            snapshot_id,
        )?;
        let target = resolve_endpoint(
            store,
            &relation,
            &relation.target,
            units,
            repository_id,
            snapshot_id,
        )?;
        let (Some(source_id), Some(target_id)) = (source, target) else {
            counts.unresolved += 1;
            store.insert_gap(&AnalysisGap {
                code: GapCode::UnresolvedTarget,
                scope: endpoint_scope(&relation),
                capability: Some(capability_for_relation(relation.kind)),
                evidence_ids: relation.evidence_ids.clone(),
                message: "An exact Language IR endpoint was absent or ambiguous in the canonical identity table; no edge was created".to_string(),
            })?;
            return Ok(());
        };
        store.mark_node_relevant(&source_id)?;
        store.mark_node_relevant(&target_id)?;
        let kind = canonical_relation_kind(relation.kind);
        let id = contract(
            FactEdge::stable_id(
                &source_id,
                &target_id,
                kind,
                Some(&relation.semantic_context_id),
                None,
            ),
            "cannot build canonical relation identity",
        )?;
        store.insert_edge(&FactEdge {
            id,
            snapshot_id: snapshot_id.clone(),
            source_id,
            target_id,
            family: kind.family(),
            kind,
            truth: relation.truth,
            resolution: relation.resolution,
            dispatch: relation.dispatch,
            semantic_context_id: Some(relation.semantic_context_id),
            qualifier: None,
            evidence_ids: relation.evidence_ids,
        })?;
        counts.resolved += 1;
        Ok(())
    })?;
    Ok(counts)
}

fn resolve_endpoint(
    store: &mut BundleStore,
    relation: &IrRelation,
    endpoint: &IrEndpoint,
    units: &BTreeMap<AnalysisUnitId, &AnalysisUnit>,
    repository_id: &FactNodeId,
    snapshot_id: &SnapshotId,
) -> Result<Option<FactNodeId>, String> {
    let source_unit = units
        .get(&relation.unit_id)
        .ok_or_else(|| format!("unknown relation unit {}", relation.unit_id))?;
    match endpoint {
        IrEndpoint::NativeSymbol { symbol_id } => {
            store.resolve_symbol_exact(&relation.unit_id, symbol_id)
        }
        IrEndpoint::File { path } => {
            store.resolve_file_exact(&relation.unit_id, source_unit.language, path)
        }
        IrEndpoint::Structure {
            unit_id,
            kind,
            qualified_name,
        } => {
            if let Some(existing) = store.resolve_structure_exact(unit_id, *kind, qualified_name)? {
                return Ok(Some(existing));
            }
            let target_unit = units
                .get(unit_id)
                .ok_or_else(|| format!("structure endpoint references unknown unit {unit_id}"))?;
            let node_id = contract(
                FactNode::stable_id(
                    *kind,
                    Some(target_unit.language),
                    Some(unit_id),
                    qualified_name,
                    None,
                ),
                "cannot build canonical structure identity",
            )?;
            let node = FactNode {
                id: node_id.clone(),
                snapshot_id: snapshot_id.clone(),
                family: kind.family(),
                kind: *kind,
                native_kind: None,
                qualified_name: qualified_name.clone(),
                display_name: structure_display_name(qualified_name),
                signature: None,
                details: None,
                visibility: Visibility::Unknown,
                language: Some(target_unit.language),
                analysis_unit_id: Some(unit_id.clone()),
                parent_id: Some(repository_id.clone()),
                definition_evidence_id: None,
                evidence_ids: relation.evidence_ids.clone(),
                roles: Vec::new(),
                flags: SourceFlags::default(),
            };
            store.insert_node(&node, true)?;
            store.register_structure_identity(unit_id, *kind, qualified_name, &node_id)?;
            store.insert_edge(&structural_edge(
                snapshot_id,
                repository_id,
                &node_id,
                FactEdgeKind::Contains,
                Some(&target_unit.context.id),
                relation.resolution,
                relation.evidence_ids.clone(),
            )?)?;
            Ok(Some(node_id))
        }
    }
}

fn create_declaration_edges(
    store: &mut BundleStore,
    units: &BTreeMap<AnalysisUnitId, &AnalysisUnit>,
) -> Result<(), String> {
    const PAGE_SIZE: usize = 512;
    let mut after = None::<FactNodeId>;
    loop {
        let nodes = store.retained_nodes_page(after.as_ref(), PAGE_SIZE)?;
        if nodes.is_empty() {
            break;
        }
        for node in &nodes {
            let (Some(parent_id), Some(definition_evidence_id), Some(unit_id)) = (
                node.parent_id.as_ref(),
                node.definition_evidence_id.as_ref(),
                node.analysis_unit_id.as_ref(),
            ) else {
                continue;
            };
            if node.kind == FactNodeKind::File {
                continue;
            }
            let unit = units.get(unit_id).ok_or_else(|| {
                format!(
                    "retained node {} references unknown unit {unit_id}",
                    node.id
                )
            })?;
            store.insert_edge(&structural_edge(
                &node.snapshot_id,
                parent_id,
                &node.id,
                FactEdgeKind::Declares,
                Some(&unit.context.id),
                ResolutionMethod::SyntaxExact,
                vec![definition_evidence_id.clone()],
            )?)?;
        }
        after = nodes.last().map(|node| node.id.clone());
    }
    Ok(())
}

fn structural_file_evidence(
    path: &RepositoryPath,
    content_digest: Sha256Digest,
) -> Result<FactEvidence, String> {
    FactEvidence::new(
        EvidenceKind::DerivedStructural,
        EvidenceProducer {
            kind: EvidenceProducerKind::StaticNormalizer,
            name: "code-memory-canonical-linker".to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
            strategy: Some(FILE_STRUCTURE_STRATEGY.to_string()),
        },
        EvidenceLocation::RepositoryArtifact {
            artifact: ArtifactLocation {
                path: path.clone(),
                content_digest,
                pointer: Some("source-manifest:file".to_string()),
            },
        },
        None,
    )
    .map_err(|error| format!("cannot build manifest-backed file evidence: {error}"))
}

fn structural_edge(
    snapshot_id: &SnapshotId,
    source_id: &FactNodeId,
    target_id: &FactNodeId,
    kind: FactEdgeKind,
    semantic_context_id: Option<&codebase_fact_model::identity::SemanticContextId>,
    resolution: ResolutionMethod,
    mut evidence_ids: Vec<EvidenceId>,
) -> Result<FactEdge, String> {
    evidence_ids.sort();
    evidence_ids.dedup();
    let id = contract(
        FactEdge::stable_id(source_id, target_id, kind, semantic_context_id, None),
        "cannot build canonical structural edge identity",
    )?;
    let edge = FactEdge {
        id,
        snapshot_id: snapshot_id.clone(),
        source_id: source_id.clone(),
        target_id: target_id.clone(),
        family: kind.family(),
        kind,
        truth: FactTruth::Structural,
        resolution,
        dispatch: DispatchKind::NotApplicable,
        semantic_context_id: semantic_context_id.cloned(),
        qualifier: None,
        evidence_ids,
    };
    edge.validate()
        .map_err(|error| format!("invalid derived structural edge: {error}"))?;
    Ok(edge)
}

fn baseline_definition_kind(kind: FactNodeKind) -> bool {
    matches!(
        kind,
        FactNodeKind::Namespace
            | FactNodeKind::Type
            | FactNodeKind::Class
            | FactNodeKind::Interface
            | FactNodeKind::Trait
            | FactNodeKind::Struct
            | FactNodeKind::Enum
            | FactNodeKind::TypeAlias
            | FactNodeKind::Callable
            | FactNodeKind::Function
            | FactNodeKind::Constructor
    )
}

fn canonical_relation_kind(kind: LanguageRelationKind) -> FactEdgeKind {
    match kind {
        LanguageRelationKind::Contains => FactEdgeKind::Contains,
        LanguageRelationKind::Declares => FactEdgeKind::Declares,
        LanguageRelationKind::BelongsTo => FactEdgeKind::BelongsTo,
        LanguageRelationKind::Imports => FactEdgeKind::Imports,
        LanguageRelationKind::Exports => FactEdgeKind::Exports,
        LanguageRelationKind::Calls => FactEdgeKind::Calls,
        LanguageRelationKind::Constructs => FactEdgeKind::Constructs,
        LanguageRelationKind::Extends => FactEdgeKind::Extends,
        LanguageRelationKind::Implements => FactEdgeKind::Implements,
        LanguageRelationKind::MixesIn => FactEdgeKind::MixesIn,
        LanguageRelationKind::Overrides => FactEdgeKind::Overrides,
        LanguageRelationKind::UsesType => FactEdgeKind::UsesType,
        LanguageRelationKind::Tests => FactEdgeKind::Tests,
    }
}

fn capability_for_relation(kind: LanguageRelationKind) -> AnalysisCapability {
    match kind {
        LanguageRelationKind::Contains
        | LanguageRelationKind::Declares
        | LanguageRelationKind::BelongsTo => AnalysisCapability::ProjectStructure,
        LanguageRelationKind::Imports => AnalysisCapability::Imports,
        LanguageRelationKind::Exports => AnalysisCapability::Exports,
        LanguageRelationKind::Calls | LanguageRelationKind::Constructs => {
            AnalysisCapability::DirectCalls
        }
        LanguageRelationKind::Extends
        | LanguageRelationKind::Implements
        | LanguageRelationKind::MixesIn
        | LanguageRelationKind::UsesType => AnalysisCapability::TypeRelations,
        LanguageRelationKind::Overrides => AnalysisCapability::Overrides,
        LanguageRelationKind::Tests => AnalysisCapability::TestRelations,
    }
}

fn endpoint_scope(relation: &IrRelation) -> AnalysisScope {
    match &relation.source {
        IrEndpoint::NativeSymbol { symbol_id } => AnalysisScope::NativeSymbol {
            unit_id: relation.unit_id.clone(),
            symbol_id: symbol_id.clone(),
        },
        IrEndpoint::File { path } => AnalysisScope::File {
            unit_id: Some(relation.unit_id.clone()),
            path: path.clone(),
        },
        IrEndpoint::Structure { unit_id, .. } => AnalysisScope::AnalysisUnit {
            unit_id: unit_id.clone(),
        },
    }
}

fn flags_for_file_kind(kind: SourceFileKind) -> SourceFlags {
    SourceFlags {
        test: kind == SourceFileKind::Test,
        generated: kind == SourceFileKind::Generated,
        vendor: kind == SourceFileKind::Vendor,
        external: false,
    }
}

fn file_display_name(path: &RepositoryPath) -> String {
    path.as_str()
        .rsplit('/')
        .next()
        .unwrap_or(path.as_str())
        .to_string()
}

fn structure_display_name(qualified_name: &str) -> String {
    qualified_name
        .rsplit(['.', '/', ':'])
        .find(|part| !part.is_empty())
        .unwrap_or(qualified_name)
        .to_string()
}

fn verify_ir_artifact(
    path: &Path,
    expected_digest: Sha256Digest,
    expected_record_count: u64,
) -> Result<(), String> {
    let file = File::open(path).map_err(|error| {
        format!(
            "cannot open Language IR artifact {}: {error}",
            path.display()
        )
    })?;
    let mut reader = BufReader::with_capacity(128 * 1024, file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("cannot hash Language IR artifact: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual = Sha256Digest::parse(&format!("{:x}", hasher.finalize()))
        .map_err(|error| format!("cannot encode Language IR content digest: {error}"))?;
    if actual != expected_digest {
        return Err(format!(
            "Language IR content digest changed before canonical linking: expected={expected_digest} actual={actual}"
        ));
    }
    let mut counted = 0_u64;
    visit_language_ir_records(path, |_| {
        counted += 1;
        Ok(())
    })?;
    if counted != expected_record_count {
        return Err(format!(
            "Language IR record count changed before canonical linking: expected={expected_record_count} actual={counted}"
        ));
    }
    Ok(())
}

fn contract<T>(
    result: Result<T, codebase_fact_model::validation::ContractError>,
    context: &str,
) -> Result<T, String> {
    result.map_err(|error| format!("{context}: {error}"))
}
