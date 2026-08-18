//! 진입점 패킷을 경로/패키지 버킷으로 묶어 Overview 도메인을 만든다.
//!
//! AI는 이 버킷 ID만 고르고 이름·병합을 붙인다. 그래프를 다시 만들지 않는다.

use super::cluster_groups::{library_style_project, FormationResult};
use crate::config::DomainPolicy;
use crate::domain::capabilities::Capability;
use crate::domain::capability_keys::is_operational_capability_key;
use crate::domain::confidence::{DomainConfidence, DomainStatus};
use crate::domain::grouping::{stable_domain_id, DomainGroup, DomainKind};
use crate::domain::membership::{DomainMembership, MembershipKind};
use crate::domain::naming::label;
use crate::facts::{Evidence, FactStore, ReferenceKind, SourceSpan};
use crate::graph::StaticRelationGraph;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

pub(super) const MAX_EVIDENCE_BUCKETS: usize = 32;
const MAX_BUCKET_SEGMENTS: usize = 2;
const MAX_CALL_NAMES: usize = 6;
const MAX_PACKETS_PER_BUCKET: usize = 12;
const CALL_DEPTH: usize = 2;
const PACKAGE_SKIP_SEGMENTS: &[&str] = &["src", "lib", "pkg", "internal"];

#[derive(Debug, Clone)]
pub(super) struct EvidencePacket {
    pub capability_key: String,
    pub bucket_key: String,
    pub summary: String,
    pub entrypoint_ids: Vec<String>,
    pub resource_ids: Vec<String>,
    pub unit_ids: Vec<String>,
    pub span: Option<SourceSpan>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct EvidenceCatalog {
    pub packets: Vec<EvidencePacket>,
    pub bucket_cap_merges: usize,
}

pub(super) fn form_domains_from_evidence(
    capabilities: &[Capability],
    store: &FactStore,
    graph: &StaticRelationGraph,
    domain_policy: &DomainPolicy,
) -> (FormationResult, EvidenceCatalog) {
    let catalog = build_catalog(capabilities, store, graph);
    let mut groups: Vec<DomainGroup> = Vec::new();
    let mut memberships = Vec::new();
    let mut assigned_units = HashSet::new();
    let mut unit_membership_idx: HashMap<String, usize> = HashMap::new();
    let mut group_index_by_id: HashMap<String, usize> = HashMap::new();

    let mut packets_by_bucket: BTreeMap<String, Vec<&EvidencePacket>> = BTreeMap::new();
    for packet in &catalog.packets {
        packets_by_bucket
            .entry(packet.bucket_key.clone())
            .or_default()
            .push(packet);
    }

    for (bucket_key, packets) in &packets_by_bucket {
        let domain_id = stable_domain_id(bucket_key);
        let capability_keys: Vec<&str> = packets
            .iter()
            .map(|packet| packet.capability_key.as_str())
            .collect();
        let kind = bucket_domain_kind(bucket_key, &capability_keys, domain_policy, store);
        let (status, confidence) = bucket_confidence(packets.len());
        let idx = groups.len();
        group_index_by_id.insert(domain_id.clone(), idx);
        groups.push(DomainGroup {
            id: domain_id.clone(),
            key: bucket_key.clone(),
            label: label(bucket_key.rsplit('/').next().unwrap_or(bucket_key)),
            kind,
            status,
            confidence: confidence.clone(),
            primary_unit_ids: Vec::new(),
            shared_unit_ids: Vec::new(),
            entrypoint_ids: Vec::new(),
            feature_ids: Vec::new(),
            resource_ids: Vec::new(),
            evidence: Vec::new(),
            summary: None,
        });

        for packet in packets.iter().take(MAX_PACKETS_PER_BUCKET) {
            if let Some(&group_idx) = group_index_by_id.get(&domain_id) {
                let group = &mut groups[group_idx];
                if let Some(span) = &packet.span {
                    group.evidence.push(Evidence::new(
                        "packet",
                        packet.summary.clone(),
                        span.clone(),
                    ));
                }
                for entrypoint_id in &packet.entrypoint_ids {
                    if !group.entrypoint_ids.contains(entrypoint_id) {
                        group.entrypoint_ids.push(entrypoint_id.clone());
                    }
                }
                for resource_id in &packet.resource_ids {
                    if !group.resource_ids.contains(resource_id) {
                        group.resource_ids.push(resource_id.clone());
                    }
                }
            }
            for unit_id in &packet.unit_ids {
                assign_unit_membership(
                    unit_id,
                    &domain_id,
                    confidence.score,
                    &mut memberships,
                    &mut unit_membership_idx,
                    &mut groups,
                    &group_index_by_id,
                    &mut assigned_units,
                );
            }
        }
    }

    for group in &mut groups {
        group.primary_unit_ids.sort();
        group.primary_unit_ids.dedup();
        group.shared_unit_ids.sort();
        group.shared_unit_ids.dedup();
        group.entrypoint_ids.sort();
        group.entrypoint_ids.dedup();
        group.resource_ids.sort();
        group.resource_ids.dedup();
        for unit_id in group
            .primary_unit_ids
            .iter()
            .chain(group.shared_unit_ids.iter())
        {
            if let Some(unit) = store.unit(unit_id) {
                group.evidence.push(Evidence::new(
                    "unit",
                    unit.qualified_name.clone(),
                    unit.span.clone(),
                ));
            }
        }
        group.evidence.sort_by(|left, right| {
            packet_rank(&left.kind)
                .cmp(&packet_rank(&right.kind))
                .then_with(|| left.id.cmp(&right.id))
        });
        group.evidence.dedup_by(|left, right| left.id == right.id);
        group.evidence.truncate(24);
    }

    (
        FormationResult {
            groups,
            memberships,
            assigned_units,
        },
        catalog,
    )
}

fn packet_rank(kind: &str) -> u8 {
    if kind == "packet" {
        0
    } else {
        1
    }
}

pub(super) fn build_catalog(
    capabilities: &[Capability],
    store: &FactStore,
    graph: &StaticRelationGraph,
) -> EvidenceCatalog {
    let call_index = outbound_calls(graph);
    let mut packets = Vec::with_capacity(capabilities.len());
    for capability in capabilities {
        let path = representative_path(capability, store).unwrap_or_else(|| "_unscoped".into());
        let bucket_key = bucket_key_from_path(&path);
        let summary = packet_summary(capability, store, &call_index);
        let span = representative_span(capability, store);
        packets.push(EvidencePacket {
            capability_key: capability.key.clone(),
            bucket_key,
            summary,
            entrypoint_ids: capability.entrypoint_ids.clone(),
            resource_ids: capability.resource_ids.clone(),
            unit_ids: capability.unit_ids.clone(),
            span,
        });
    }

    let mut buckets: BTreeMap<String, Vec<EvidencePacket>> = BTreeMap::new();
    for packet in packets {
        buckets
            .entry(packet.bucket_key.clone())
            .or_default()
            .push(packet);
    }
    let (buckets, bucket_cap_merges) = collapse_buckets(buckets);
    let packets = buckets
        .into_iter()
        .flat_map(|(bucket_key, mut packets)| {
            for packet in &mut packets {
                packet.bucket_key = bucket_key.clone();
            }
            packets
        })
        .collect();
    EvidenceCatalog {
        packets,
        bucket_cap_merges,
    }
}

pub(crate) fn bucket_key_from_path(relative_path: &str) -> String {
    let normalized = relative_path.replace('\\', "/");
    let segments: Vec<_> = normalized
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.is_empty() {
        return "_unscoped".into();
    }
    let start = segments
        .iter()
        .position(|segment| !PACKAGE_SKIP_SEGMENTS.contains(segment))
        .unwrap_or(0);
    let dirs_end = segments.len().saturating_sub(1);
    if start >= dirs_end {
        return segments
            .get(start)
            .filter(|segment| !segment.contains('.'))
            .map(|segment| (*segment).to_string())
            .unwrap_or_else(|| "_unscoped".into());
    }
    let available = &segments[start..dirs_end];
    let take = available.len().min(MAX_BUCKET_SEGMENTS);
    if take == 0 {
        return "_unscoped".into();
    }
    available[..take].join("/")
}

fn parent_bucket_key(key: &str) -> Option<String> {
    key.rsplit_once('/').map(|(parent, _)| parent.to_string())
}

fn collapse_buckets(
    mut buckets: BTreeMap<String, Vec<EvidencePacket>>,
) -> (BTreeMap<String, Vec<EvidencePacket>>, usize) {
    let mut merges = 0;
    while buckets.len() > MAX_EVIDENCE_BUCKETS {
        let mut nested: Vec<(usize, String)> = buckets
            .iter()
            .filter(|(key, _)| parent_bucket_key(key).is_some())
            .map(|(key, packets)| (packets.len(), key.clone()))
            .collect();
        nested.sort();
        if let Some((_, source_key)) = nested.first().cloned() {
            let packets = buckets.remove(&source_key).unwrap_or_default();
            let parent = parent_bucket_key(&source_key).expect("nested bucket has a parent");
            buckets.entry(parent).or_default().extend(packets);
            merges += 1;
            continue;
        }

        let mut roots: Vec<(usize, String)> = buckets
            .iter()
            .map(|(key, packets)| (packets.len(), key.clone()))
            .collect();
        roots.sort();
        let Some((_, source_key)) = roots.first().cloned() else {
            break;
        };
        if roots.len() < 2 {
            break;
        }
        let packets = buckets.remove(&source_key).unwrap_or_default();
        buckets.entry("_other".into()).or_default().extend(packets);
        merges += 1;
    }
    (buckets, merges)
}

fn representative_path(capability: &Capability, store: &FactStore) -> Option<String> {
    for entrypoint_id in &capability.entrypoint_ids {
        let Some(entrypoint) = store
            .entrypoints
            .iter()
            .find(|entrypoint| entrypoint.id == *entrypoint_id)
        else {
            continue;
        };
        if let Some(unit) = store.unit(&entrypoint.unit_id) {
            return Some(unit.relative_path.replace('\\', "/"));
        }
    }
    capability.unit_ids.iter().find_map(|unit_id| {
        store
            .unit(unit_id)
            .map(|unit| unit.relative_path.replace('\\', "/"))
    })
}

fn representative_span(capability: &Capability, store: &FactStore) -> Option<SourceSpan> {
    for entrypoint_id in &capability.entrypoint_ids {
        let Some(entrypoint) = store
            .entrypoints
            .iter()
            .find(|entrypoint| entrypoint.id == *entrypoint_id)
        else {
            continue;
        };
        if let Some(unit) = store.unit(&entrypoint.unit_id) {
            return Some(unit.span.clone());
        }
    }
    capability
        .unit_ids
        .iter()
        .find_map(|unit_id| store.unit(unit_id).map(|unit| unit.span.clone()))
}

fn packet_summary(
    capability: &Capability,
    store: &FactStore,
    call_index: &HashMap<String, Vec<String>>,
) -> String {
    let route = route_hint(capability, store);
    let handler = handler_name(capability, store);
    let calls = call_names(capability, store, call_index);
    let resources = resource_names(capability, store);
    let mut parts = vec![route];
    if !handler.is_empty() {
        parts.push(handler);
    }
    if !calls.is_empty() {
        parts.push(format!("calls {}", calls.join(", ")));
    }
    if !resources.is_empty() {
        parts.push(resources.join(", "));
    }
    let summary = parts.join(" · ");
    if summary.chars().count() <= 160 {
        summary
    } else {
        summary.chars().take(157).collect::<String>() + "..."
    }
}

fn route_hint(capability: &Capability, store: &FactStore) -> String {
    for entrypoint_id in &capability.entrypoint_ids {
        let Some(entrypoint) = store
            .entrypoints
            .iter()
            .find(|entrypoint| entrypoint.id == *entrypoint_id)
        else {
            continue;
        };
        return match (&entrypoint.method, &entrypoint.path) {
            (Some(method), Some(path)) => format!("{method} {path}"),
            (_, Some(path)) => path.clone(),
            _ => entrypoint.name.clone(),
        };
    }
    capability.key.clone()
}

fn handler_name(capability: &Capability, store: &FactStore) -> String {
    for entrypoint_id in &capability.entrypoint_ids {
        let Some(entrypoint) = store
            .entrypoints
            .iter()
            .find(|entrypoint| entrypoint.id == *entrypoint_id)
        else {
            continue;
        };
        if let Some(unit) = store.unit(&entrypoint.unit_id) {
            return unit.name.clone();
        }
    }
    String::new()
}

fn resource_names(capability: &Capability, store: &FactStore) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = BTreeSet::new();
    for resource_id in &capability.resource_ids {
        let Some(resource) = store
            .resources
            .iter()
            .find(|resource| resource.id == *resource_id)
        else {
            continue;
        };
        if seen.insert(resource.name.clone()) {
            names.push(resource.name.clone());
        }
        if names.len() >= 3 {
            break;
        }
    }
    names
}

fn outbound_calls(graph: &StaticRelationGraph) -> HashMap<String, Vec<String>> {
    let mut index: HashMap<String, Vec<String>> = HashMap::new();
    for edge in graph.edges.iter() {
        if edge.kind != ReferenceKind::Call {
            continue;
        }
        let Some(target) = &edge.target_unit_id else {
            continue;
        };
        index
            .entry(edge.source_unit_id.clone())
            .or_default()
            .push(target.clone());
    }
    index
}

fn call_names(
    capability: &Capability,
    store: &FactStore,
    call_index: &HashMap<String, Vec<String>>,
) -> Vec<String> {
    let owned: HashSet<&str> = capability.unit_ids.iter().map(String::as_str).collect();
    let mut names = Vec::new();
    let mut seen_names = BTreeSet::new();
    let mut seen_units = HashSet::new();
    let mut queue: VecDeque<(String, usize)> = capability
        .unit_ids
        .iter()
        .cloned()
        .map(|unit_id| (unit_id, 0))
        .collect();
    while let Some((unit_id, depth)) = queue.pop_front() {
        if depth >= CALL_DEPTH || !seen_units.insert(unit_id.clone()) {
            continue;
        }
        let Some(targets) = call_index.get(&unit_id) else {
            continue;
        };
        for target in targets {
            if depth + 1 < CALL_DEPTH {
                queue.push_back((target.clone(), depth + 1));
            }
            if owned.contains(target.as_str()) {
                continue;
            }
            let Some(unit) = store.unit(target) else {
                continue;
            };
            if seen_names.insert(unit.name.clone()) {
                names.push(unit.name.clone());
            }
            if names.len() >= MAX_CALL_NAMES {
                return names;
            }
        }
    }
    names
}

fn bucket_domain_kind(
    bucket_key: &str,
    capability_keys: &[&str],
    policy: &DomainPolicy,
    store: &FactStore,
) -> DomainKind {
    let leaf = bucket_key.rsplit('/').next().unwrap_or(bucket_key);
    if policy.cross_cutting_keys.contains(bucket_key) || policy.cross_cutting_keys.contains(leaf) {
        return DomainKind::CrossCutting;
    }
    if !capability_keys.is_empty()
        && capability_keys
            .iter()
            .all(|key| is_operational_capability_key(key))
    {
        return DomainKind::CrossCutting;
    }
    if library_style_project(store) {
        return DomainKind::CrossCutting;
    }
    DomainKind::Business
}

fn bucket_confidence(packet_count: usize) -> (DomainStatus, DomainConfidence) {
    let overall = if packet_count >= 3 {
        0.8
    } else if packet_count == 2 {
        0.55
    } else {
        0.35
    };
    let (status, level) = if packet_count >= 2 && overall >= 0.6 {
        (DomainStatus::Confirmed, "high")
    } else if overall >= 0.3 {
        (DomainStatus::Candidate, "medium")
    } else {
        (DomainStatus::Ambiguous, "low")
    };
    (
        status,
        DomainConfidence {
            level: level.into(),
            score: (overall * 100.0_f64).round() as u32,
            signal_families: BTreeSet::new(),
            cohesion: overall,
            separation: 0.5,
            evidence_diversity: 0.5,
            overall,
        },
    )
}

fn assign_unit_membership(
    unit_id: &str,
    domain_id: &str,
    score: u32,
    memberships: &mut Vec<DomainMembership>,
    unit_membership_idx: &mut HashMap<String, usize>,
    groups: &mut [DomainGroup],
    group_index_by_id: &HashMap<String, usize>,
    assigned_units: &mut HashSet<String>,
) {
    assigned_units.insert(unit_id.to_string());
    let Some(&existing_idx) = unit_membership_idx.get(unit_id) else {
        memberships.push(DomainMembership {
            unit_id: unit_id.to_string(),
            domain_id: Some(domain_id.to_string()),
            domain_ids: vec![domain_id.to_string()],
            kind: MembershipKind::Primary,
            score,
        });
        unit_membership_idx.insert(unit_id.to_string(), memberships.len() - 1);
        if let Some(&group_idx) = group_index_by_id.get(domain_id) {
            let group = &mut groups[group_idx];
            if !group.primary_unit_ids.contains(&unit_id.to_string()) {
                group.primary_unit_ids.push(unit_id.to_string());
            }
        }
        return;
    };

    let existing = &memberships[existing_idx];
    let primary_domain_id = existing
        .domain_id
        .clone()
        .unwrap_or_else(|| domain_id.to_string());
    if primary_domain_id == domain_id {
        return;
    }
    let mut domain_ids = existing.domain_ids.clone();
    if !domain_ids.iter().any(|id| id == domain_id) {
        domain_ids.push(domain_id.to_string());
    }
    domain_ids.sort();
    domain_ids.dedup();
    memberships[existing_idx] = DomainMembership {
        unit_id: unit_id.to_string(),
        domain_id: Some(primary_domain_id.clone()),
        domain_ids: domain_ids.clone(),
        kind: MembershipKind::Shared,
        score: existing.score.max(score),
    };
    if let Some(&primary_idx) = group_index_by_id.get(&primary_domain_id) {
        let primary_group = &mut groups[primary_idx];
        primary_group
            .primary_unit_ids
            .retain(|candidate| candidate != unit_id);
        if !primary_group.shared_unit_ids.contains(&unit_id.to_string()) {
            primary_group.shared_unit_ids.push(unit_id.to_string());
        }
    }
    if let Some(&shared_idx) = group_index_by_id.get(domain_id) {
        let shared_group = &mut groups[shared_idx];
        if !shared_group.shared_unit_ids.contains(&unit_id.to_string())
            && !shared_group.primary_unit_ids.contains(&unit_id.to_string())
        {
            shared_group.shared_unit_ids.push(unit_id.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{bucket_key_from_path, collapse_buckets, EvidencePacket, MAX_EVIDENCE_BUCKETS};

    fn packet(bucket: &str) -> EvidencePacket {
        EvidencePacket {
            capability_key: bucket.into(),
            bucket_key: bucket.into(),
            summary: bucket.into(),
            entrypoint_ids: Vec::new(),
            resource_ids: Vec::new(),
            unit_ids: Vec::new(),
            span: None,
        }
    }

    #[test]
    fn 경로_버킷은_src를_건너뛰고_디렉터리만_남긴다() {
        assert_eq!(
            bucket_key_from_path("src/order/OrderController.ts"),
            "order"
        );
        assert_eq!(bucket_key_from_path("server/auth/routes.ts"), "server/auth");
        assert_eq!(bucket_key_from_path("web/client.ts"), "web");
        assert_eq!(bucket_key_from_path("desktop.rs"), "_unscoped");
    }

    #[test]
    fn 버킷_상한은_부모_경로로_접는다() {
        let mut buckets = std::collections::BTreeMap::new();
        for index in 0..(MAX_EVIDENCE_BUCKETS + 3) {
            let key = format!("server/mod{index}");
            buckets.insert(key.clone(), vec![packet(&key)]);
        }
        let (collapsed, merges) = collapse_buckets(buckets);
        assert!(collapsed.len() <= MAX_EVIDENCE_BUCKETS);
        assert!(merges > 0);
        assert!(collapsed.contains_key("server"));
    }
}
