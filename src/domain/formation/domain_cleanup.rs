//! 진입점 없는 유령 도메인과 리소스 전용 도메인을 정리한다.

use crate::domain::capability_keys::canonical_capability_key;
use crate::domain::grouping::DomainKind;
use std::collections::HashSet;

use super::cluster_groups::FormationResult;
use super::singleton_absorption::merge_domains;

pub(super) fn prune_resource_only_domains(formation: &mut FormationResult) {
    let mut absorbed_domain_ids = HashSet::new();
    let mut merges: Vec<(String, String)> = Vec::new();

    for group in &formation.groups {
        if group.kind != DomainKind::Business {
            continue;
        }
        if !group.entrypoint_ids.is_empty() {
            continue;
        }
        if absorbed_domain_ids.contains(&group.id) {
            continue;
        }

        let canonical = canonical_capability_key(&group.key);
        let Some(parent_id) = formation
            .groups
            .iter()
            .find(|candidate| {
                candidate.kind == DomainKind::Business
                    && candidate.id != group.id
                    && !candidate.entrypoint_ids.is_empty()
                    && canonical_capability_key(&candidate.key) == canonical
            })
            .map(|candidate| candidate.id.clone())
        else {
            continue;
        };

        merges.push((group.id.clone(), parent_id.clone()));
        absorbed_domain_ids.insert(group.id.clone());
    }

    for (child_id, parent_id) in merges {
        merge_domains(
            &mut formation.groups,
            &mut formation.memberships,
            &child_id,
            &parent_id,
        );
    }

    formation
        .groups
        .retain(|group| !absorbed_domain_ids.contains(&group.id));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::confidence::{DomainConfidence, DomainStatus};
    use crate::domain::grouping::{stable_domain_id, DomainGroup};
    use std::collections::HashSet;

    fn group(id: &str, key: &str, entrypoint_ids: Vec<&str>) -> DomainGroup {
        DomainGroup {
            id: id.into(),
            key: key.into(),
            label: key.into(),
            kind: DomainKind::Business,
            status: DomainStatus::Candidate,
            confidence: DomainConfidence {
                level: "candidate".into(),
                score: 0,
                signal_families: Default::default(),
                cohesion: 0.0,
                separation: 0.0,
                evidence_diversity: 0.0,
                overall: 0.0,
            },
            primary_unit_ids: Vec::new(),
            shared_unit_ids: Vec::new(),
            entrypoint_ids: entrypoint_ids.into_iter().map(str::to_string).collect(),
            feature_ids: Vec::new(),
            resource_ids: Vec::new(),
            evidence: Vec::new(),
            summary: None,
        }
    }

    #[test]
    fn 리소스_전용_도메인은_같은_명사_도메인으로_흡수된다() {
        let sessions_id = stable_domain_id("sessions");
        let ghost_id = stable_domain_id("sessions:param");
        let mut formation = FormationResult {
            groups: vec![
                group(&sessions_id, "sessions", vec!["entry-sessions"]),
                group(&ghost_id, "sessions:param", vec![]),
            ],
            memberships: Vec::new(),
            assigned_units: HashSet::new(),
        };

        prune_resource_only_domains(&mut formation);

        assert_eq!(formation.groups.len(), 1);
        assert_eq!(formation.groups[0].key, "sessions");
    }
}
