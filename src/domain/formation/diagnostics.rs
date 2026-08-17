//! 도메인 형성 단계의 추적 가능한 진단 지표.

use crate::config::DomainClusteringMode;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DomainFormationDiagnostics {
    pub clustering_mode: String,
    pub capabilities: usize,
    pub distinct_keys: usize,
    pub total_pairs: usize,
    pub forbidden_pairs: usize,
    pub forbidden_ratio: f64,
    pub clusters_before_absorption: usize,
    pub domains_before_absorption: usize,
    pub domains_after_absorption: usize,
    pub absorbed_domains: usize,
    pub clustering_merges: usize,
    pub merge_reasons: BTreeMap<String, usize>,
    pub absorption_reasons: BTreeMap<String, usize>,
}

impl DomainFormationDiagnostics {
    pub fn new(mode: DomainClusteringMode) -> Self {
        Self {
            clustering_mode: clustering_mode_label(mode).into(),
            ..Default::default()
        }
    }

    pub fn record_constraint_stats(
        &mut self,
        capability_count: usize,
        distinct_keys: usize,
        forbidden_pairs: usize,
    ) {
        self.capabilities = capability_count;
        self.distinct_keys = distinct_keys;
        self.total_pairs = pair_count(capability_count);
        self.forbidden_pairs = forbidden_pairs;
        self.forbidden_ratio = if self.total_pairs == 0 {
            0.0
        } else {
            forbidden_pairs as f64 / self.total_pairs as f64
        };
    }

    pub fn record_clustering(&mut self, cluster_count: usize, domain_count: usize) {
        self.clusters_before_absorption = cluster_count;
        self.domains_before_absorption = domain_count;
    }

    pub fn record_absorption(&mut self, domain_count: usize) {
        self.domains_after_absorption = domain_count;
        self.absorbed_domains = self
            .domains_before_absorption
            .saturating_sub(domain_count);
    }

    pub fn record_merge(&mut self, reason: &str) {
        self.clustering_merges += 1;
        *self.merge_reasons.entry(reason.to_string()).or_default() += 1;
    }

    pub fn record_absorption_reason(&mut self, reason: &str) {
        *self
            .absorption_reasons
            .entry(reason.to_string())
            .or_default() += 1;
    }

    pub fn is_empty(&self) -> bool {
        self.capabilities == 0 && self.clusters_before_absorption == 0
    }
}

pub(super) fn clustering_mode_label(mode: DomainClusteringMode) -> &'static str {
    match mode {
        DomainClusteringMode::LegacyStrictKey => "legacyStrictKey",
        DomainClusteringMode::StructuralCrossKey => "structuralCrossKey",
    }
}

fn pair_count(n: usize) -> usize {
    n.saturating_mul(n.saturating_sub(1)) / 2
}
