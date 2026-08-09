use crate::{
    compile_base_prompt, BaseSemanticDraft, CompiledBasePrompt, SemanticCompileError,
    SemanticCompileErrorCode,
};
use codebase_fact_model::{
    fact_graph::FactTruth,
    identity::{Sha256Digest, SnapshotId},
};
use codebase_semantic_model::{
    ApprovedSemanticRevision, BaseSemanticInput, BoundaryRelationSummary, RegionId, ScopeReceipt,
};
use std::collections::{BTreeMap, BTreeSet};

const LOCAL_PARTITION_POLICY: &str = r#"This request is one complete, disjoint local partition of a larger repository.
Describe and group only the regions present in PAYLOAD_JSON. Do not invent missing repository areas or treat this local project summary as the final repository summary.
The local result is an evidence-verified input to a later compact global reconciliation and is never published directly."#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SemanticPartitionPolicy {
    /// A small packet does not benefit from an extra local+global round trip.
    pub direct_max_regions: usize,
    pub direct_max_prompt_bytes: usize,
    /// Hard bounds for each independently executable local request.
    pub max_regions_per_partition: usize,
    pub max_partition_input_bytes: usize,
}

impl Default for SemanticPartitionPolicy {
    fn default() -> Self {
        Self {
            direct_max_regions: 4,
            direct_max_prompt_bytes: 96 * 1024,
            max_regions_per_partition: 12,
            max_partition_input_bytes: 96 * 1024,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CompiledSemanticPartition {
    pub partition_key: String,
    pub region_ids: Vec<RegionId>,
    pub prompt: CompiledBasePrompt,
}

#[derive(Clone, Debug)]
pub struct CompiledSemanticPlan {
    /// The complete authoritative packet used for final verification/storage.
    pub base: CompiledBasePrompt,
    /// Empty means the complete packet is safe to execute directly.
    pub partitions: Vec<CompiledSemanticPartition>,
}

impl CompiledSemanticPlan {
    pub fn is_direct(&self) -> bool {
        self.partitions.is_empty()
    }
}

#[derive(Clone, Debug)]
pub struct VerifiedSemanticPartition {
    pub partition_key: String,
    pub region_ids: Vec<RegionId>,
    pub packet_digest: Sha256Digest,
    pub revision: ApprovedSemanticRevision,
}

pub fn compile_semantic_plan(
    draft: BaseSemanticDraft,
) -> Result<CompiledSemanticPlan, SemanticCompileError> {
    let base = compile_base_prompt(draft)?;
    let policy = adaptive_partition_policy(&base);
    compile_semantic_plan_from_base(base, policy)
}

pub fn compile_semantic_plan_with_policy(
    draft: BaseSemanticDraft,
    policy: SemanticPartitionPolicy,
) -> Result<CompiledSemanticPlan, SemanticCompileError> {
    let base = compile_base_prompt(draft)?;
    compile_semantic_plan_from_base(base, policy)
}

fn compile_semantic_plan_from_base(
    base: CompiledBasePrompt,
    policy: SemanticPartitionPolicy,
) -> Result<CompiledSemanticPlan, SemanticCompileError> {
    validate_policy(policy)?;
    if base.packet.input.regions.len() <= policy.direct_max_regions
        && base.rendered_prompt().len() <= policy.direct_max_prompt_bytes
    {
        return Ok(CompiledSemanticPlan {
            base,
            partitions: Vec::new(),
        });
    }

    let region_groups = plan_region_groups(&base.packet.input, policy)?;
    let mut partitions = Vec::with_capacity(region_groups.len());
    for region_ids in region_groups {
        let input = subset_input(&base.packet.input, &region_ids);
        let region_count = region_ids.len() as u64;
        let draft = BaseSemanticDraft {
            workspace_id: base.packet.workspace_id.clone(),
            snapshot_id: base.packet.snapshot_id.clone(),
            provider: base.packet.provider.clone(),
            output_language: base.packet.output_language,
            scope_receipt: ScopeReceipt {
                included: region_count,
                total: region_count,
                truncated: false,
                reason: None,
            },
            input,
        };
        let mut prompt = compile_base_prompt(draft)?;
        prompt.system_policy = format!("{}\n\n{}", prompt.system_policy, LOCAL_PARTITION_POLICY);
        let partition_key = partition_key(&base.packet.snapshot_id, &region_ids);
        partitions.push(CompiledSemanticPartition {
            partition_key,
            region_ids,
            prompt,
        });
    }

    verify_partition_coverage(&base.packet.input, &partitions)?;
    Ok(CompiledSemanticPlan { base, partitions })
}

fn adaptive_partition_policy(base: &CompiledBasePrompt) -> SemanticPartitionPolicy {
    adaptive_partition_policy_for_workload(
        base.packet.input.regions.len(),
        base.rendered_prompt().len(),
    )
}

fn adaptive_partition_policy_for_workload(
    region_count: usize,
    prompt_bytes: usize,
) -> SemanticPartitionPolicy {
    const DIRECT_PROMPT_BYTES: usize = 96 * 1024;
    const TARGET_PARTITION_BYTES: usize = 88 * 1024;
    const MAX_PARTITION_INPUT_BYTES: usize = 96 * 1024;
    const MAX_REGIONS_PER_PARTITION: usize = 24;

    let region_count = region_count.max(1);
    let target_partitions = prompt_bytes.div_ceil(TARGET_PARTITION_BYTES).max(1);
    let regions_per_partition = region_count
        .div_ceil(target_partitions)
        .clamp(1, MAX_REGIONS_PER_PARTITION);
    SemanticPartitionPolicy {
        // A small prompt is one job regardless of an arbitrary region count.
        // Large projects split because of their actual byte/region workload,
        // not because the product expects a fixed number such as sixteen.
        direct_max_regions: usize::MAX,
        direct_max_prompt_bytes: DIRECT_PROMPT_BYTES,
        max_regions_per_partition: regions_per_partition,
        max_partition_input_bytes: MAX_PARTITION_INPUT_BYTES,
    }
}

fn validate_policy(policy: SemanticPartitionPolicy) -> Result<(), SemanticCompileError> {
    if policy.max_regions_per_partition == 0 || policy.max_partition_input_bytes < 4 * 1024 {
        return Err(SemanticCompileError::new(
            SemanticCompileErrorCode::InvalidPacket,
            "partitionPolicy",
            "partition bounds must allow at least one region and 4096 input bytes",
        ));
    }
    Ok(())
}

fn plan_region_groups(
    input: &BaseSemanticInput,
    policy: SemanticPartitionPolicy,
) -> Result<Vec<Vec<RegionId>>, SemanticCompileError> {
    let weights = relation_weights(&input.boundary_relations);
    let mut unassigned = input
        .regions
        .iter()
        .map(|region| region.region_id.clone())
        .collect::<BTreeSet<_>>();
    let mut groups = Vec::new();

    while !unassigned.is_empty() {
        let seed = choose_seed(&unassigned, &weights).ok_or_else(|| {
            SemanticCompileError::new(
                SemanticCompileErrorCode::InvalidPacket,
                "partitionPlan",
                "no deterministic partition seed was available",
            )
        })?;
        let mut group = vec![seed.clone()];
        unassigned.remove(&seed);

        while group.len() < policy.max_regions_per_partition && !unassigned.is_empty() {
            let mut candidates = unassigned.iter().cloned().collect::<Vec<_>>();
            candidates.sort_by(|left, right| {
                connection_score(right, &group, &weights)
                    .cmp(&connection_score(left, &group, &weights))
                    .then_with(|| total_weight(right, &weights).cmp(&total_weight(left, &weights)))
                    .then_with(|| left.cmp(right))
            });
            let mut accepted = None;
            for candidate in candidates {
                let mut proposed = group.clone();
                proposed.push(candidate.clone());
                proposed.sort();
                let bytes = serialized_subset_bytes(input, &proposed)?;
                if bytes <= policy.max_partition_input_bytes {
                    accepted = Some(candidate);
                    break;
                }
            }
            let Some(candidate) = accepted else {
                break;
            };
            unassigned.remove(&candidate);
            group.push(candidate);
        }
        group.sort();
        let bytes = serialized_subset_bytes(input, &group)?;
        if bytes > policy.max_partition_input_bytes {
            let label = input
                .regions
                .iter()
                .find(|region| region.region_id == group[0])
                .map(|region| region.structural_label.as_str())
                .unwrap_or("<unknown>");
            return Err(SemanticCompileError::new(
                SemanticCompileErrorCode::InvalidPacket,
                "partitionPlan",
                format!(
                    "one region ({label}) needs {bytes} bytes and exceeds the {} byte partition budget",
                    policy.max_partition_input_bytes
                ),
            ));
        }
        groups.push(group);
    }

    groups.sort_by(|left, right| left[0].cmp(&right[0]));
    Ok(groups)
}

fn relation_weights(relations: &[BoundaryRelationSummary]) -> BTreeMap<(RegionId, RegionId), u64> {
    let mut weights = BTreeMap::new();
    for relation in relations {
        let mut endpoints = [
            relation.source_region_id.clone(),
            relation.target_region_id.clone(),
        ];
        endpoints.sort();
        let weight = relation
            .families
            .iter()
            .map(|count| {
                count.relation_count.saturating_mul(match count.truth {
                    FactTruth::Confirmed => 4,
                    FactTruth::Structural => 2,
                    FactTruth::StaticCandidate => 1,
                })
            })
            .fold(0_u64, u64::saturating_add);
        weights
            .entry((endpoints[0].clone(), endpoints[1].clone()))
            .and_modify(|current: &mut u64| *current = current.saturating_add(weight))
            .or_insert(weight);
    }
    weights
}

fn choose_seed(
    unassigned: &BTreeSet<RegionId>,
    weights: &BTreeMap<(RegionId, RegionId), u64>,
) -> Option<RegionId> {
    unassigned.iter().cloned().max_by(|left, right| {
        total_weight(left, weights)
            .cmp(&total_weight(right, weights))
            .then_with(|| right.cmp(left))
    })
}

fn connection_score(
    candidate: &RegionId,
    group: &[RegionId],
    weights: &BTreeMap<(RegionId, RegionId), u64>,
) -> u64 {
    group.iter().fold(0_u64, |total, member| {
        total.saturating_add(pair_weight(candidate, member, weights))
    })
}

fn total_weight(region_id: &RegionId, weights: &BTreeMap<(RegionId, RegionId), u64>) -> u64 {
    weights
        .iter()
        .fold(0_u64, |total, ((left, right), weight)| {
            if left == region_id || right == region_id {
                total.saturating_add(*weight)
            } else {
                total
            }
        })
}

fn pair_weight(
    left: &RegionId,
    right: &RegionId,
    weights: &BTreeMap<(RegionId, RegionId), u64>,
) -> u64 {
    let key = if left <= right {
        (left.clone(), right.clone())
    } else {
        (right.clone(), left.clone())
    };
    weights.get(&key).copied().unwrap_or_default()
}

fn serialized_subset_bytes(
    input: &BaseSemanticInput,
    region_ids: &[RegionId],
) -> Result<usize, SemanticCompileError> {
    serde_json::to_vec(&subset_input(input, region_ids))
        .map(|bytes| bytes.len())
        .map_err(|error| {
            SemanticCompileError::new(
                SemanticCompileErrorCode::InvalidPacket,
                "partitionPlan",
                error.to_string(),
            )
        })
}

fn subset_input(input: &BaseSemanticInput, region_ids: &[RegionId]) -> BaseSemanticInput {
    let selected = region_ids.iter().cloned().collect::<BTreeSet<_>>();
    let internal_relations = input
        .boundary_relations
        .iter()
        .filter(|relation| {
            selected.contains(&relation.source_region_id)
                && selected.contains(&relation.target_region_id)
        })
        .cloned()
        .collect::<Vec<_>>();
    let internal_bundle_ids = internal_relations
        .iter()
        .map(|relation| relation.bundle_id.clone())
        .collect::<BTreeSet<_>>();

    let mut trace_owners = BTreeMap::<_, BTreeSet<RegionId>>::new();
    for region in &input.regions {
        for trace_id in &region.representative_trace_path_ids {
            trace_owners
                .entry(trace_id.clone())
                .or_default()
                .insert(region.region_id.clone());
        }
    }
    let included_trace_ids = trace_owners
        .iter()
        .filter(|(_, owners)| {
            !owners.is_empty() && owners.iter().all(|owner| selected.contains(owner))
        })
        .map(|(trace_id, _)| trace_id.clone())
        .collect::<BTreeSet<_>>();

    let regions = input
        .regions
        .iter()
        .filter(|region| selected.contains(&region.region_id))
        .cloned()
        .map(|mut region| {
            if region
                .parent_region_id
                .as_ref()
                .is_some_and(|parent| !selected.contains(parent))
            {
                region.parent_region_id = None;
            }
            region
                .inbound_bundle_ids
                .retain(|bundle_id| internal_bundle_ids.contains(bundle_id));
            region
                .outbound_bundle_ids
                .retain(|bundle_id| internal_bundle_ids.contains(bundle_id));
            region
                .representative_trace_path_ids
                .retain(|trace_id| included_trace_ids.contains(trace_id));
            region
        })
        .collect::<Vec<_>>();
    let anchors = input
        .anchors
        .iter()
        .filter(|anchor| selected.contains(&anchor.owner_region_id))
        .cloned()
        .collect::<Vec<_>>();
    let anchor_ids = anchors
        .iter()
        .map(|anchor| anchor.fact_id.clone())
        .collect::<BTreeSet<_>>();

    BaseSemanticInput {
        repository: codebase_semantic_model::ProjectSemanticContext {
            fact_id: input.repository.fact_id.clone(),
            name: input.repository.name.clone(),
            languages: input.repository.languages.clone(),
            framework_fact_ids: input
                .repository
                .framework_fact_ids
                .iter()
                .filter(|fact_id| anchor_ids.contains(*fact_id))
                .cloned()
                .collect(),
            root_region_ids: regions
                .iter()
                .filter(|region| region.parent_region_id.is_none())
                .map(|region| region.region_id.clone())
                .collect(),
        },
        regions,
        anchors,
        boundary_relations: internal_relations,
        representative_traces: input
            .representative_traces
            .iter()
            .filter(|trace| included_trace_ids.contains(&trace.trace_path_id))
            .cloned()
            .collect(),
        excerpts: input
            .excerpts
            .iter()
            .filter(|excerpt| selected.contains(&excerpt.owner_region_id))
            .cloned()
            .collect(),
        previous_revision: None,
    }
}

fn partition_key(snapshot_id: &SnapshotId, region_ids: &[RegionId]) -> String {
    let mut material = snapshot_id.to_string();
    for region_id in region_ids {
        material.push('\n');
        material.push_str(region_id.as_str());
    }
    format!("partition-{}", Sha256Digest::of_bytes(material.as_bytes()))
}

fn verify_partition_coverage(
    input: &BaseSemanticInput,
    partitions: &[CompiledSemanticPartition],
) -> Result<(), SemanticCompileError> {
    let expected = input
        .regions
        .iter()
        .map(|region| region.region_id.clone())
        .collect::<BTreeSet<_>>();
    let mut actual = BTreeSet::new();
    for partition in partitions {
        for region_id in &partition.region_ids {
            if !actual.insert(region_id.clone()) {
                return Err(SemanticCompileError::new(
                    SemanticCompileErrorCode::DuplicateIdentifier,
                    "partitionPlan.regionIds",
                    format!("region {region_id} occurs in more than one partition"),
                ));
            }
        }
    }
    if actual != expected {
        return Err(SemanticCompileError::new(
            SemanticCompileErrorCode::IncompleteAssignment,
            "partitionPlan.regionIds",
            "partition plan does not cover the complete base region scope",
        ));
    }
    Ok(())
}

pub(crate) fn validate_verified_partitions(
    base: &CompiledBasePrompt,
    partitions: &[VerifiedSemanticPartition],
) -> Result<(), SemanticCompileError> {
    if partitions.is_empty() {
        return Err(SemanticCompileError::new(
            SemanticCompileErrorCode::InvalidPacket,
            "verifiedPartitions",
            "global reconciliation requires at least one verified partition",
        ));
    }
    let expected = base
        .packet
        .input
        .regions
        .iter()
        .map(|region| region.region_id.clone())
        .collect::<BTreeSet<_>>();
    let mut covered = BTreeSet::new();
    let mut keys = BTreeSet::new();
    for partition in partitions {
        if !keys.insert(partition.partition_key.as_str()) {
            return Err(SemanticCompileError::new(
                SemanticCompileErrorCode::DuplicateIdentifier,
                "verifiedPartitions[].partitionKey",
                "partition key is duplicated",
            ));
        }
        if partition.revision.snapshot_id != base.packet.snapshot_id
            || partition.revision.semantic_input_digest != partition.packet_digest
        {
            return Err(SemanticCompileError::new(
                SemanticCompileErrorCode::DigestMismatch,
                "verifiedPartitions",
                "verified partition identity does not match the base snapshot or local packet",
            ));
        }
        if partition.revision.provider.kind != base.packet.provider.kind
            || partition.revision.provider.model != base.packet.provider.model
            || partition.revision.provider.effort != base.packet.provider.effort
        {
            return Err(SemanticCompileError::new(
                SemanticCompileErrorCode::DigestMismatch,
                "verifiedPartitions[].provider",
                "all partitions must use the selected base provider contract",
            ));
        }
        let expected_local = partition
            .region_ids
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let actual_local = partition
            .revision
            .assignments
            .iter()
            .map(|assignment| assignment.region_id.clone())
            .chain(
                partition
                    .revision
                    .unassigned_regions
                    .iter()
                    .map(|region| region.region_id.clone()),
            )
            .collect::<BTreeSet<_>>();
        if actual_local != expected_local {
            return Err(SemanticCompileError::new(
                SemanticCompileErrorCode::IncompleteAssignment,
                "verifiedPartitions[].regionIds",
                "verified local result does not account for its exact partition scope",
            ));
        }
        for region_id in &partition.region_ids {
            if !covered.insert(region_id.clone()) {
                return Err(SemanticCompileError::new(
                    SemanticCompileErrorCode::DuplicateIdentifier,
                    "verifiedPartitions[].regionIds",
                    format!("region {region_id} occurs in multiple verified partitions"),
                ));
            }
        }
    }
    if covered != expected {
        return Err(SemanticCompileError::new(
            SemanticCompileErrorCode::IncompleteAssignment,
            "verifiedPartitions[].regionIds",
            "verified partitions do not cover the complete base region scope",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod adaptive_policy_tests {
    use super::*;

    #[test]
    fn partition_shape_tracks_real_workload_instead_of_a_fixed_job_count() {
        let small = adaptive_partition_policy_for_workload(3, 40 * 1024);
        let medium = adaptive_partition_policy_for_workload(192, 1_400 * 1024);
        let large = adaptive_partition_policy_for_workload(192, 3_000 * 1024);

        assert_eq!(small.max_regions_per_partition, 3);
        assert_eq!(small.direct_max_regions, usize::MAX);
        assert_eq!(medium.max_regions_per_partition, 12);
        assert_eq!(large.max_regions_per_partition, 6);
        assert!(large.max_regions_per_partition < medium.max_regions_per_partition);
    }
}
