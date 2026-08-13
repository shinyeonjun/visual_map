use crate::domain::signals::{DomainSignal, DomainSignalKind};
use crate::facts::Evidence;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// 하나의 도메인 후보를 구성하는 근거 묶음이다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainCandidate {
    pub key: String,
    pub score: u32,
    pub signal_families: BTreeSet<DomainSignalKind>,
    pub unit_ids: BTreeSet<String>,
    pub evidence: Vec<Evidence>,
    #[serde(skip)]
    signal_unit_ids: BTreeMap<DomainSignalKind, BTreeSet<String>>,
}

pub fn build(
    signals: &[DomainSignal],
    maximum_evidence: usize,
    minimum_repeated_symbol_units: usize,
) -> Vec<DomainCandidate> {
    let mut candidates: BTreeMap<String, DomainCandidate> = BTreeMap::new();
    for signal in signals {
        let candidate = candidates
            .entry(signal.domain_key.clone())
            .or_insert_with(|| DomainCandidate {
                key: signal.domain_key.clone(),
                score: 0,
                signal_families: BTreeSet::new(),
                unit_ids: BTreeSet::new(),
                evidence: Vec::new(),
                signal_unit_ids: BTreeMap::new(),
            });
        candidate.score += signal.weight;
        candidate.signal_families.insert(signal.kind.clone());
        candidate.unit_ids.insert(signal.unit_id.clone());
        candidate
            .signal_unit_ids
            .entry(signal.kind.clone())
            .or_default()
            .insert(signal.unit_id.clone());
        for evidence in &signal.evidence {
            if candidate.evidence.len() < maximum_evidence && !candidate.evidence.contains(evidence)
            {
                candidate.evidence.push(evidence.clone());
            }
        }
    }

    let mut values: Vec<_> = candidates
        .into_values()
        .filter(|candidate| {
            candidate
                .signal_unit_ids
                .get(&DomainSignalKind::Entrypoint)
                .is_some_and(|units| !units.is_empty())
                || candidate
                    .signal_unit_ids
                    .get(&DomainSignalKind::Resource)
                    .is_some_and(|units| !units.is_empty())
                || candidate
                    .signal_unit_ids
                    .get(&DomainSignalKind::Path)
                    .is_some_and(|units| units.len() >= 2)
                || candidate
                    .signal_unit_ids
                    .get(&DomainSignalKind::Symbol)
                    .is_some_and(|units| units.len() >= minimum_repeated_symbol_units)
        })
        .collect();
    values.sort_by(|left, right| right.score.cmp(&left.score).then(left.key.cmp(&right.key)));
    values
}
