use super::store::BundleStore;
use crate::static_pipeline::test_ir::TestIr;
use codebase_fact_model::analysis::AnalysisUnit;
use codebase_fact_model::coverage::{
    AnalysisCapability, AnalysisGap, AnalysisScope, CapabilityExecutionState, CapabilityReceipt,
    CoverageDenominator, DeclaredSupport, EvidencePrecision, GapCode,
};
use codebase_fact_model::fact_graph::{
    DispatchKind, FactEdge, FactEdgeKind, FactNode, FactNodeKind, FactTruth, ResolutionMethod,
    Visibility,
};
use codebase_fact_model::identity::{AnalysisUnitId, FactEdgeId, FactNodeId, SnapshotId};
use codebase_fact_model::validation::Validate;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct CanonicalTestCounts {
    pub(super) test_case_node_count: u64,
    pub(super) tests_edge_count: u64,
    pub(super) unlinked_test_case_count: u64,
}

#[derive(Default)]
struct UnitCanonicalAudit {
    test_nodes: BTreeSet<FactNodeId>,
    edges: BTreeSet<FactEdgeId>,
    linked_cases: BTreeSet<FactNodeId>,
    gap_codes: BTreeSet<GapCode>,
}

pub(super) fn ingest_test_relations(
    store: &mut BundleStore,
    test_ir: &TestIr,
    units: &BTreeMap<AnalysisUnitId, &AnalysisUnit>,
    snapshot_id: &SnapshotId,
) -> Result<CanonicalTestCounts, String> {
    if test_ir.snapshot_id != *snapshot_id || test_ir.receipt.snapshot_id != *snapshot_id {
        return Err("Test IR belongs to another snapshot".to_string());
    }
    for evidence in &test_ir.evidence {
        store.insert_evidence(evidence)?;
    }
    for gap in &test_ir.gaps {
        store.insert_gap(gap)?;
    }

    let mut counts = CanonicalTestCounts::default();
    let mut unit_results = units
        .keys()
        .cloned()
        .map(|unit_id| (unit_id, UnitCanonicalAudit::default()))
        .collect::<BTreeMap<_, _>>();
    for gap in &test_ir.gaps {
        if let Some(unit_id) = gap.scope.unit_id() {
            unit_results
                .get_mut(unit_id)
                .ok_or_else(|| format!("Test IR gap references unknown unit {unit_id}"))?
                .gap_codes
                .insert(gap.code);
        }
    }

    let mut case_ids = BTreeMap::<(AnalysisUnitId, String), FactNodeId>::new();
    for test_case in &test_ir.cases {
        let unit = units
            .get(&test_case.unit_id)
            .ok_or_else(|| format!("Test IR case references unknown unit {}", test_case.unit_id))?;
        if unit.language != test_case.language {
            return Err(format!(
                "Test IR case language differs from unit {}",
                test_case.unit_id
            ));
        }
        if !store.has_evidence(test_case.marker_evidence_id.as_str())? {
            return Err(format!(
                "Test IR case references missing evidence {}",
                test_case.marker_evidence_id
            ));
        }
        let file_id = store
            .resolve_file_exact(
                &test_case.unit_id,
                test_case.language,
                &test_case.source_path,
            )?
            .ok_or_else(|| {
                format!(
                    "Test IR case has no exact canonical file: {}/{}",
                    test_case.unit_id, test_case.source_path
                )
            })?;
        let test_id = FactNode::stable_id(
            FactNodeKind::TestCase,
            Some(test_case.language),
            Some(&test_case.unit_id),
            &test_case.qualified_name,
            None,
        )
        .map_err(|error| format!("cannot build canonical TestCase identity: {error}"))?;
        store.insert_node(
            &FactNode {
                id: test_id.clone(),
                snapshot_id: snapshot_id.clone(),
                family: FactNodeKind::TestCase.family(),
                kind: FactNodeKind::TestCase,
                native_kind: Some(test_case.native_kind.clone()),
                qualified_name: test_case.qualified_name.clone(),
                display_name: test_case.display_name.clone(),
                signature: None,
                details: None,
                visibility: Visibility::Private,
                language: Some(test_case.language),
                analysis_unit_id: Some(test_case.unit_id.clone()),
                parent_id: Some(file_id),
                definition_evidence_id: Some(test_case.marker_evidence_id.clone()),
                evidence_ids: vec![test_case.marker_evidence_id.clone()],
                roles: Vec::new(),
                flags: test_case.flags,
            },
            true,
        )?;
        let key = (test_case.unit_id.clone(), test_case.qualified_name.clone());
        if case_ids.insert(key, test_id.clone()).is_some() {
            return Err(format!(
                "Test IR repeats TestCase identity {}/{}",
                test_case.unit_id, test_case.qualified_name
            ));
        }
        if unit_results
            .get_mut(&test_case.unit_id)
            .expect("planned Test IR unit")
            .test_nodes
            .insert(test_id)
        {
            counts.test_case_node_count += 1;
        }
    }

    for relation in &test_ir.relations {
        let unit = units.get(&relation.unit_id).ok_or_else(|| {
            format!(
                "Test IR relation references unknown unit {}",
                relation.unit_id
            )
        })?;
        let test_id = case_ids
            .get(&(
                relation.unit_id.clone(),
                relation.test_qualified_name.clone(),
            ))
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Test IR relation references unknown TestCase {}/{}",
                    relation.unit_id, relation.test_qualified_name
                )
            })?;
        for evidence_id in &relation.evidence_ids {
            if !store.has_evidence(evidence_id.as_str())? {
                return Err(format!(
                    "Test IR relation references missing evidence {evidence_id}"
                ));
            }
        }
        let target_id =
            store.resolve_symbol_exact(&relation.unit_id, &relation.target_symbol_id)?;
        let target = match target_id {
            Some(target_id) => store.node(&target_id)?.map(|node| (target_id, node)),
            None => None,
        };
        let Some((target_id, target)) = target else {
            record_unresolved_target(
                store,
                &mut unit_results,
                relation,
                "The provider-resolved production target was absent or ambiguous in the canonical identity table",
            )?;
            continue;
        };
        if target.flags.test || target.kind == FactNodeKind::TestCase {
            record_unresolved_target(
                store,
                &mut unit_results,
                relation,
                "A test helper is not promoted as the production target of a tests relation",
            )?;
            continue;
        }
        let edge = canonical_test_edge(
            snapshot_id,
            &test_id,
            &target_id,
            Some(&unit.context.id),
            relation.evidence_ids.clone(),
        )?;
        store.mark_node_relevant(&test_id)?;
        store.mark_node_relevant(&target_id)?;
        store.insert_edge(&edge)?;
        let result = unit_results
            .get_mut(&relation.unit_id)
            .expect("planned Test IR unit");
        result.linked_cases.insert(test_id);
        if result.edges.insert(edge.id) {
            counts.tests_edge_count += 1;
        }
    }

    for (unit_id, unit) in units {
        let test_audit = test_ir
            .unit_audit
            .get(unit_id)
            .ok_or_else(|| format!("Test IR omitted unit audit {unit_id}"))?;
        let result = unit_results
            .get(unit_id)
            .expect("canonical Test IR unit audit");
        let detected = test_audit.detected_test_case_count;
        let linked = result.linked_cases.len() as u64;
        let inventory_failed = test_audit.inventory_failed_file_count > 0;
        let (execution_state, precision, denominator) = if detected == 0 && !inventory_failed {
            (
                CapabilityExecutionState::NotApplicable,
                EvidencePrecision::None,
                CoverageDenominator::Known { eligible_count: 0 },
            )
        } else if !inventory_failed && linked == detected {
            (
                CapabilityExecutionState::Complete,
                EvidencePrecision::ExactRange,
                CoverageDenominator::Known {
                    eligible_count: detected,
                },
            )
        } else {
            (
                CapabilityExecutionState::Partial,
                EvidencePrecision::ExactRange,
                if inventory_failed {
                    CoverageDenominator::Unknown
                } else {
                    CoverageDenominator::Known {
                        eligible_count: detected,
                    }
                },
            )
        };
        let mut gap_codes = result.gap_codes.iter().copied().collect::<Vec<_>>();
        if execution_state == CapabilityExecutionState::Partial && gap_codes.is_empty() {
            gap_codes.push(if inventory_failed {
                GapCode::ProviderExecutionIncomplete
            } else {
                GapCode::UnresolvedTarget
            });
        }
        if execution_state == CapabilityExecutionState::NotApplicable {
            gap_codes.clear();
        }
        gap_codes.sort();
        gap_codes.dedup();
        let receipt = CapabilityReceipt {
            unit_id: unit_id.clone(),
            capability: AnalysisCapability::TestRelations,
            declared_support: DeclaredSupport::Conditional,
            execution_state,
            precision,
            denominator,
            covered_count: linked.min(detected),
            emitted_fact_count: result.test_nodes.len() as u64,
            emitted_relation_count: result.edges.len() as u64,
            truncated_count: if inventory_failed {
                0
            } else {
                detected.saturating_sub(linked)
            },
            gap_codes,
        };
        receipt.validate().map_err(|error| {
            format!(
                "invalid test capability receipt for {}/{}: {error}",
                unit.language.as_str(),
                unit_id
            )
        })?;
        store.insert_capability_receipt(&receipt)?;
        counts.unlinked_test_case_count += detected.saturating_sub(linked);
    }
    Ok(counts)
}

fn record_unresolved_target(
    store: &mut BundleStore,
    unit_results: &mut BTreeMap<AnalysisUnitId, UnitCanonicalAudit>,
    relation: &crate::static_pipeline::test_ir::TestRelationRecord,
    message: &str,
) -> Result<(), String> {
    unit_results
        .get_mut(&relation.unit_id)
        .ok_or_else(|| {
            format!(
                "Test IR relation references unknown unit {}",
                relation.unit_id
            )
        })?
        .gap_codes
        .insert(GapCode::UnresolvedTarget);
    store.insert_gap(&AnalysisGap {
        code: GapCode::UnresolvedTarget,
        scope: AnalysisScope::NativeSymbol {
            unit_id: relation.unit_id.clone(),
            symbol_id: relation.target_symbol_id.clone(),
        },
        capability: Some(AnalysisCapability::TestRelations),
        evidence_ids: relation.evidence_ids.clone(),
        message: message.to_string(),
    })
}

fn canonical_test_edge(
    snapshot_id: &SnapshotId,
    source_id: &FactNodeId,
    target_id: &FactNodeId,
    semantic_context_id: Option<&codebase_fact_model::identity::SemanticContextId>,
    mut evidence_ids: Vec<codebase_fact_model::identity::EvidenceId>,
) -> Result<FactEdge, String> {
    evidence_ids.sort();
    evidence_ids.dedup();
    let id = FactEdge::stable_id(
        source_id,
        target_id,
        FactEdgeKind::Tests,
        semantic_context_id,
        None,
        None,
    )
    .map_err(|error| format!("cannot build canonical tests edge identity: {error}"))?;
    let edge = FactEdge {
        id,
        snapshot_id: snapshot_id.clone(),
        source_id: source_id.clone(),
        target_id: target_id.clone(),
        family: FactEdgeKind::Tests.family(),
        kind: FactEdgeKind::Tests,
        truth: FactTruth::Confirmed,
        resolution: ResolutionMethod::Provider,
        dispatch: DispatchKind::NotApplicable,
        semantic_context_id: semantic_context_id.cloned(),
        qualifier: None,
        execution: None,
        evidence_ids,
    };
    edge.validate()
        .map_err(|error| format!("invalid canonical tests edge: {error}"))?;
    Ok(edge)
}
