mod support;

use codebase_semantic_compiler::{
    compile_global_reconciliation_prompt, compile_semantic_plan_with_policy,
    parse_and_verify_base_response, parse_and_verify_global_reconciliation, verify_base_proposal,
    SemanticCompileErrorCode, SemanticPartitionPolicy, SemanticVerificationPhase,
    VerifiedSemanticPartition,
};
use codebase_semantic_model::AreaCategory;
use serde_json::json;
use std::collections::BTreeSet;
use support::{fixture_draft, structural_proposal, valid_proposal};

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
fn local_partition_defers_sibling_name_uniqueness_to_global_reconciliation() {
    let (draft, ids) = fixture_draft();
    let plan = compile_semantic_plan_with_policy(draft, forced_partition_policy(2)).unwrap();
    assert_eq!(plan.partitions.len(), 1);
    let partition = &plan.partitions[0];
    assert_eq!(
        partition.prompt.verification_phase,
        SemanticVerificationPhase::LocalPartition
    );
    let mut proposal = valid_proposal(&partition.prompt, &ids);
    proposal.areas[2].label = proposal.areas[0].label.clone();
    let raw = serde_json::to_string(&proposal).unwrap();

    parse_and_verify_base_response(&partition.prompt, &raw).unwrap();
    let final_error = verify_base_proposal(&partition.prompt.packet, proposal).unwrap_err();
    assert_eq!(
        final_error.code,
        SemanticCompileErrorCode::NonCanonicalValue
    );
}

#[test]
fn compact_global_reconciliation_can_merge_meaning_across_partition_boundaries() {
    let (draft, _) = fixture_draft();
    let plan = compile_semantic_plan_with_policy(draft, forced_partition_policy(1)).unwrap();
    let verified = verified_structural_partitions(&plan);
    let compiled = compile_global_reconciliation_prompt(&plan.base, &verified).unwrap();
    let rendered = compiled.prompt.rendered_prompt();

    assert!(rendered.len() < 64 * 1024);
    assert!(rendered.contains("boundaryRelations"));
    assert!(rendered.contains("r0000"));
    assert!(rendered.contains("r0001"));
    assert!(!rendered.contains(plan.base.packet.input.regions[0].region_id.as_str()));
    assert!(compiled.prompt.system_policy.contains("NAMING CONTRACT"));
    assert!(compiled
        .prompt
        .system_policy
        .contains("There is no target area count"));
    assert!(compiled
        .prompt
        .system_policy
        .contains("readability is only a tie-breaker"));
    assert!(compiled
        .prompt
        .system_policy
        .contains("Never merge an explicit material legacy/deprecated implementation"));
    assert_eq!(
        compiled.prompt.output_schema["properties"]["snapshotId"]["enum"][0],
        serde_json::json!(plan.base.packet.snapshot_id.as_str())
    );
    assert_eq!(
        compiled.prompt.output_schema["properties"]["semanticInputDigest"]["enum"][0],
        serde_json::json!(plan.base.packet.semantic_input_digest.to_hex())
    );

    let raw = serde_json::to_string(&json!({
        "schemaVersion": 1,
        "snapshotId": plan.base.packet.snapshot_id,
        "semanticInputDigest": plan.base.packet.semantic_input_digest,
        "projectSummary": "주문 처리와 요청 인증을 제공하는 커머스 백엔드입니다.",
        "areas": [{
            "proposalKey": "commerce",
            "parentProposalKey": null,
            "level": 0,
            "label": "주문·인증",
            "summary": "주문 요청 처리와 요청 인증을 함께 담당합니다.",
            "category": "domain",
            "directMemberRegionKeys": ["r0000", "r0001"],
            "aliases": []
        }],
        "unassignedRegions": [],
        "warnings": []
    }))
    .unwrap();
    let revision = parse_and_verify_global_reconciliation(&compiled, &raw).unwrap();

    assert_eq!(revision.areas.len(), 1);
    assert_eq!(revision.assignments.len(), 2);
    assert_eq!(revision.areas[0].category, AreaCategory::Structural);
    assert_eq!(revision.areas[0].effective_member_region_ids.len(), 2);
}

#[test]
fn compact_global_reconciliation_rejects_duplicate_region_assignment() {
    let (draft, _) = fixture_draft();
    let plan = compile_semantic_plan_with_policy(draft, forced_partition_policy(1)).unwrap();
    let verified = verified_structural_partitions(&plan);
    let compiled = compile_global_reconciliation_prompt(&plan.base, &verified).unwrap();
    let raw = serde_json::to_string(&json!({
        "schemaVersion": 1,
        "snapshotId": plan.base.packet.snapshot_id,
        "semanticInputDigest": plan.base.packet.semantic_input_digest,
        "projectSummary": "현재 커머스 백엔드 구조입니다.",
        "areas": [
            {
                "proposalKey": "one",
                "parentProposalKey": null,
                "level": 0,
                "label": "첫 영역",
                "summary": "첫 번째 현재 책임입니다.",
                "category": "domain",
                "directMemberRegionKeys": ["r0000"],
                "aliases": []
            },
            {
                "proposalKey": "two",
                "parentProposalKey": null,
                "level": 0,
                "label": "둘째 영역",
                "summary": "두 번째 현재 책임입니다.",
                "category": "domain",
                "directMemberRegionKeys": ["r0000", "r0001"],
                "aliases": []
            }
        ],
        "unassignedRegions": [],
        "warnings": []
    }))
    .unwrap();

    let error = parse_and_verify_global_reconciliation(&compiled, &raw).unwrap_err();
    assert_eq!(error.code, SemanticCompileErrorCode::DuplicateIdentifier);
}

fn verified_structural_partitions(
    plan: &codebase_semantic_compiler::CompiledSemanticPlan,
) -> Vec<VerifiedSemanticPartition> {
    plan.partitions
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
        .collect()
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
