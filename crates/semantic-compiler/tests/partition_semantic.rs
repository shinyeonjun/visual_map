mod support;

use codebase_semantic_compiler::{
    compile_semantic_plan_with_policy, merge_verified_partitions, verify_base_proposal,
    SemanticCompileErrorCode, SemanticPartitionPolicy, VerifiedSemanticPartition,
};
use std::collections::BTreeSet;
use support::{fixture_draft, structural_proposal};

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
fn verified_local_results_merge_deterministically_without_a_global_ai_prompt() {
    let (draft, _) = fixture_draft();
    let plan = compile_semantic_plan_with_policy(draft, forced_partition_policy(1)).unwrap();
    let mut verified = plan
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
    let first = merge_verified_partitions(&plan.base, &verified).unwrap();
    verified.reverse();
    let second = merge_verified_partitions(&plan.base, &verified).unwrap();

    assert_eq!(first, second);
    assert_eq!(
        first.semantic_input_digest,
        plan.base.packet.semantic_input_digest
    );
    assert_eq!(first.assignments.len(), 2);
    assert_eq!(first.areas.len(), 2);
    assert!(first.project.summary.contains("검증된 코드 영역 2개"));
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
