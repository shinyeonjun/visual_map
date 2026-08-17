//! 클러스터 병합 제약과 미배정 유닛 수집.

use crate::config::{DomainClusteringMode, PathPolicy};
use crate::domain::capabilities::Capability;
use crate::facts::FactStore;
use std::collections::HashSet;

use crate::domain::clustering;

pub(super) fn collect_unassigned_units(
    store: &FactStore,
    assigned_units: &HashSet<String>,
    path_policy: &PathPolicy,
) -> Vec<String> {
    store
        .units
        .keys()
        .filter(|uid| !assigned_units.contains(*uid))
        .filter(|uid| {
            store
                .unit(uid)
                .map(|u| path_policy.is_production_path(&u.relative_path))
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

pub(super) fn build_constraints(
    capabilities: &[Capability],
    store: &FactStore,
    path_policy: &PathPolicy,
    mode: DomainClusteringMode,
) -> clustering::MergeConstraints {
    let mut forbidden_pairs = Vec::new();
    for (i, capability_a) in capabilities.iter().enumerate() {
        for (j, capability_b) in capabilities.iter().enumerate().skip(i + 1) {
            if is_test_capability(capability_a, store, path_policy)
                != is_test_capability(capability_b, store, path_policy)
            {
                push_forbidden_pair(&mut forbidden_pairs, i, j);
                continue;
            }
            if mode == DomainClusteringMode::LegacyStrictKey
                && capability_a.key != capability_b.key
            {
                push_forbidden_pair(&mut forbidden_pairs, i, j);
            }
        }
    }
    clustering::MergeConstraints { forbidden_pairs }
}

fn is_test_capability(
    capability: &Capability,
    store: &FactStore,
    path_policy: &PathPolicy,
) -> bool {
    capability.unit_ids.iter().any(|unit_id| {
        store
            .unit(unit_id)
            .map(|unit| path_policy.is_test_path(&unit.relative_path))
            .unwrap_or(false)
    })
}

fn push_forbidden_pair(forbidden_pairs: &mut Vec<(usize, usize)>, a: usize, b: usize) {
    let (lo, hi) = if a < b { (a, b) } else { (b, a) };
    if !forbidden_pairs.iter().any(|&(x, y)| x == lo && y == hi) {
        forbidden_pairs.push((lo, hi));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DomainClusteringMode;
    use std::collections::BTreeSet;

    fn capability(key: &str) -> Capability {
        Capability {
            key: key.to_string(),
            entrypoint_ids: Vec::new(),
            resource_ids: Vec::new(),
            unit_ids: Vec::new(),
            contract_paths: BTreeSet::new(),
        }
    }

    #[test]
    fn 서로_다른_계약_접두는_병합_금지() {
        let capabilities = vec![capability("auth"), capability("sessions")];
        let store = FactStore::default();
        let path_policy = PathPolicy::default();
        let constraints = build_constraints(
            &capabilities,
            &store,
            &path_policy,
            DomainClusteringMode::LegacyStrictKey,
        );
        assert!(constraints.is_forbidden(0, 1));
    }

    #[test]
    fn 같은_계약_접두는_병합_금지가_아니다() {
        let capabilities = vec![capability("billing"), capability("billing")];
        let store = FactStore::default();
        let path_policy = PathPolicy::default();
        let constraints = build_constraints(
            &capabilities,
            &store,
            &path_policy,
            DomainClusteringMode::LegacyStrictKey,
        );
        assert!(!constraints.is_forbidden(0, 1));
    }

    #[test]
    fn structural_모드는_서로_다른_key를_금지하지_않는다() {
        let capabilities = vec![capability("auth"), capability("sessions")];
        let store = FactStore::default();
        let path_policy = PathPolicy::default();
        let constraints = build_constraints(
            &capabilities,
            &store,
            &path_policy,
            DomainClusteringMode::StructuralCrossKey,
        );
        assert!(!constraints.is_forbidden(0, 1));
    }
}
