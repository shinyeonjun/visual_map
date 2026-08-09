mod support;

use codebase_semantic_compiler::{
    compile_reconciliation_prompt, compile_semantic_plan_with_policy, verify_base_proposal,
    SemanticCompileErrorCode, SemanticPartitionPolicy, VerifiedSemanticPartition,
};
use codebase_semantic_model::{
    AreaCategory, AreaProposal, LabelSource, ProjectSemanticProposal, ProposalKey,
    RegionAssignment, SemanticFallbackReason, SemanticRevisionProposal,
    BASE_SEMANTIC_SCHEMA_VERSION,
};
use std::collections::BTreeSet;
use support::{fixture_draft, valid_proposal};

fn forced_partition_policy(max_regions: usize) -> SemanticPartitionPolicy {
    SemanticPartitionPolicy {
        direct_max_regions: 0,
        direct_max_prompt_bytes: 0,
        max_regions_per_partition: max_regions,
        max_partition_input_bytes: 96 * 1024,
    }
}

#[test]
fn partition_plan_is_deterministic_disjoint_and_complete() {
    let (draft, _) = fixture_draft();
    let mut reordered = draft.clone();
    reordered.input.regions.reverse();
    reordered.input.anchors.reverse();
    reordered.input.boundary_relations.reverse();
    reordered.input.excerpts.reverse();

    let first = compile_semantic_plan_with_policy(draft, forced_partition_policy(1)).unwrap();
    let second = compile_semantic_plan_with_policy(reordered, forced_partition_policy(1)).unwrap();

    assert_eq!(first.partitions.len(), 2);
    assert_eq!(
        first
            .partitions
            .iter()
            .map(|partition| (&partition.partition_key, &partition.region_ids))
            .collect::<Vec<_>>(),
        second
            .partitions
            .iter()
            .map(|partition| (&partition.partition_key, &partition.region_ids))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        first
            .partitions
            .iter()
            .map(|partition| partition.prompt.rendered_prompt())
            .collect::<Vec<_>>(),
        second
            .partitions
            .iter()
            .map(|partition| partition.prompt.rendered_prompt())
            .collect::<Vec<_>>()
    );

    let expected = first
        .base
        .packet
        .input
        .regions
        .iter()
        .map(|region| region.region_id.clone())
        .collect::<BTreeSet<_>>();
    let actual = first
        .partitions
        .iter()
        .flat_map(|partition| partition.region_ids.iter().cloned())
        .collect::<BTreeSet<_>>();
    assert_eq!(actual, expected);
    assert!(first.partitions.iter().all(|partition| {
        partition.prompt.packet.input.regions.len() == 1
            && partition.prompt.packet.scope_receipt.included == 1
            && partition.prompt.packet.scope_receipt.total == 1
            && !partition.prompt.packet.scope_receipt.truncated
    }));
}

#[test]
fn source_evidence_is_sent_only_to_its_own_local_partition() {
    let (draft, _) = fixture_draft();
    let plan = compile_semantic_plan_with_policy(draft, forced_partition_policy(1)).unwrap();

    for partition in &plan.partitions {
        assert!(partition
            .prompt
            .packet
            .input
            .excerpts
            .iter()
            .all(|excerpt| { partition.region_ids.contains(&excerpt.owner_region_id) }));
        assert!(partition
            .prompt
            .packet
            .input
            .anchors
            .iter()
            .all(|anchor| { partition.region_ids.contains(&anchor.owner_region_id) }));
        assert!(partition
            .prompt
            .packet
            .input
            .boundary_relations
            .iter()
            .all(|relation| {
                partition.region_ids.contains(&relation.source_region_id)
                    && partition.region_ids.contains(&relation.target_region_id)
            }));
    }
}

#[test]
fn verified_local_results_reconcile_without_resending_source_excerpts() {
    let (draft, ids) = fixture_draft();
    let plan = compile_semantic_plan_with_policy(draft, forced_partition_policy(1)).unwrap();
    let verified = plan
        .partitions
        .iter()
        .map(|partition| {
            let proposal = structural_proposal(&partition.prompt);
            let revision = verify_base_proposal(&partition.prompt.packet, proposal).unwrap();
            VerifiedSemanticPartition {
                partition_key: partition.partition_key.clone(),
                region_ids: partition.region_ids.clone(),
                packet_digest: partition.prompt.packet.semantic_input_digest,
                revision,
            }
        })
        .collect::<Vec<_>>();

    let reconciliation = compile_reconciliation_prompt(&plan.base, &verified).unwrap();
    let rendered = reconciliation.rendered_prompt();

    assert!(rendered.contains("RECONCILIATION_PAYLOAD_JSON"));
    assert!(rendered.contains("verifiedPartitions"));
    assert!(!rendered.contains(&plan.base.packet.input.excerpts[0].text));
    assert!(!rendered.contains(&plan.base.packet.input.excerpts[1].text));
    assert_eq!(reconciliation.packet, plan.base.packet);

    // Final publication still goes through the original full-packet verifier.
    let proposal = valid_proposal(&reconciliation, &ids);
    let approved = verify_base_proposal(&reconciliation.packet, proposal).unwrap();
    assert_eq!(approved.assignments.len(), 2);
}

#[test]
fn one_oversized_region_fails_explicitly_instead_of_silent_truncation() {
    let (mut draft, _) = fixture_draft();
    draft.input.excerpts[0].text = "x".repeat(16 * 1024);
    let policy = SemanticPartitionPolicy {
        direct_max_regions: 0,
        direct_max_prompt_bytes: 0,
        max_regions_per_partition: 1,
        max_partition_input_bytes: 8 * 1024,
    };

    let error = compile_semantic_plan_with_policy(draft, policy).unwrap_err();

    assert_eq!(error.code, SemanticCompileErrorCode::InvalidPacket);
    assert_eq!(error.path, "partitionPlan");
    assert!(error.message.contains("exceeds"));
}

fn structural_proposal(
    compiled: &codebase_semantic_compiler::CompiledBasePrompt,
) -> SemanticRevisionProposal {
    let areas = compiled
        .packet
        .input
        .regions
        .iter()
        .enumerate()
        .map(|(index, region)| AreaProposal {
            proposal_key: ProposalKey::parse(format!("local-{index}")).unwrap(),
            parent_proposal_key: None,
            level: 0,
            label: region.structural_label.clone(),
            summary: format!("{} 구조 단위를 그대로 표시합니다.", region.structural_label),
            category: AreaCategory::Structural,
            representative_fact_ids: vec![],
            representative_trace_path_ids: vec![],
            evidence_ids: vec![],
            aliases: vec![],
            label_source: LabelSource::Structural,
            fallback_reason: Some(SemanticFallbackReason::InsufficientSemanticSignal),
        })
        .collect::<Vec<_>>();
    let assignments = compiled
        .packet
        .input
        .regions
        .iter()
        .enumerate()
        .map(|(index, region)| RegionAssignment {
            region_id: region.region_id.clone(),
            area_proposal_key: ProposalKey::parse(format!("local-{index}")).unwrap(),
        })
        .collect();
    SemanticRevisionProposal {
        schema_version: BASE_SEMANTIC_SCHEMA_VERSION,
        snapshot_id: compiled.packet.snapshot_id.clone(),
        semantic_input_digest: compiled.packet.semantic_input_digest,
        project: ProjectSemanticProposal {
            summary: "이 로컬 구조 단위의 현재 책임을 설명합니다.".to_string(),
            aliases: vec![],
            representative_fact_ids: vec![],
            evidence_ids: vec![],
        },
        areas,
        assignments,
        unassigned_regions: vec![],
        warnings: vec![],
    }
}
