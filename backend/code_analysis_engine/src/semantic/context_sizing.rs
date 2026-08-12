use crate::config::SemanticPolicy;
use crate::domain::{DomainGroup, DomainRelation};
use crate::frameworks::registry::detector::FrameworkDetection;
use std::collections::{BTreeSet, HashMap};

use super::context_compaction::{compact_framework, compact_relation, json_array_size, json_size};

pub(super) struct ContextSizeEstimator {
    domain_sizes: Vec<usize>,
    relation_sizes: Vec<usize>,
    fixed_size: usize,
    relation_limit: usize,
    relations_by_domain: Vec<Vec<usize>>,
}

impl ContextSizeEstimator {
    pub(super) fn new(
        groups: &[DomainGroup],
        relations: &[DomainRelation],
        frameworks: &[FrameworkDetection],
        policy: &SemanticPolicy,
    ) -> Self {
        let domain_sizes = groups.iter().map(json_size).collect();
        let compact_relations: Vec<_> = relations
            .iter()
            .map(|relation| compact_relation(relation, policy))
            .collect();
        let relation_sizes = compact_relations.iter().map(json_size).collect();
        let compact_frameworks: Vec<_> = frameworks
            .iter()
            .map(|framework| compact_framework(framework, policy))
            .collect();

        let framework_array_size = json_array_size(
            compact_frameworks.iter().map(json_size),
            compact_frameworks.len(),
        );
        let fixed_size = b"{\"domains\":".len()
            + b",\"relations\":".len()
            + b",\"frameworks\":".len()
            + framework_array_size
            + 1;

        let domain_indices: HashMap<&str, usize> = groups
            .iter()
            .enumerate()
            .map(|(index, group)| (group.id.as_str(), index))
            .collect();
        let mut relations_by_domain = vec![Vec::new(); groups.len()];
        for (relation_index, relation) in relations.iter().enumerate() {
            if let Some(&domain_index) = domain_indices.get(relation.source_domain_id.as_str()) {
                relations_by_domain[domain_index].push(relation_index);
            }
            if relation.target_domain_id != relation.source_domain_id {
                if let Some(&domain_index) = domain_indices.get(relation.target_domain_id.as_str())
                {
                    relations_by_domain[domain_index].push(relation_index);
                }
            }
        }

        Self {
            domain_sizes,
            relation_sizes,
            fixed_size,
            relation_limit: policy.relation_limit,
            relations_by_domain,
        }
    }

    pub(super) fn state(&self) -> ContextSizeState<'_> {
        ContextSizeState {
            estimator: self,
            domain_count: 0,
            domain_bytes: 0,
            active_relations: vec![false; self.relation_sizes.len()],
            selected_relations: BTreeSet::new(),
            selected_relation_bytes: 0,
        }
    }
}

pub(super) struct ContextSizeState<'a> {
    estimator: &'a ContextSizeEstimator,
    domain_count: usize,
    domain_bytes: usize,
    active_relations: Vec<bool>,
    selected_relations: BTreeSet<usize>,
    selected_relation_bytes: usize,
}

impl ContextSizeState<'_> {
    pub(super) fn reset(&mut self) {
        self.domain_count = 0;
        self.domain_bytes = 0;
        self.active_relations.fill(false);
        self.selected_relations.clear();
        self.selected_relation_bytes = 0;
    }

    pub(super) fn add_domain(&mut self, domain_index: usize) {
        self.domain_count += 1;
        self.domain_bytes = self
            .domain_bytes
            .saturating_add(self.estimator.domain_sizes[domain_index]);
        for &relation_index in &self.estimator.relations_by_domain[domain_index] {
            self.add_relation(relation_index);
        }
    }

    fn add_relation(&mut self, relation_index: usize) {
        if self.active_relations[relation_index] {
            return;
        }
        self.active_relations[relation_index] = true;

        if self.estimator.relation_limit == 0 {
            return;
        }
        if self.selected_relations.len() < self.estimator.relation_limit {
            self.selected_relations.insert(relation_index);
            self.selected_relation_bytes = self
                .selected_relation_bytes
                .saturating_add(self.estimator.relation_sizes[relation_index]);
            return;
        }

        let Some(&largest_selected) = self.selected_relations.iter().next_back() else {
            return;
        };
        if relation_index < largest_selected {
            self.selected_relations.remove(&largest_selected);
            self.selected_relation_bytes = self
                .selected_relation_bytes
                .saturating_sub(self.estimator.relation_sizes[largest_selected]);
            self.selected_relations.insert(relation_index);
            self.selected_relation_bytes = self
                .selected_relation_bytes
                .saturating_add(self.estimator.relation_sizes[relation_index]);
        }
    }

    pub(super) fn total_size(&self) -> usize {
        self.estimator
            .fixed_size
            .saturating_add(json_array_size(
                std::iter::once(self.domain_bytes),
                self.domain_count,
            ))
            .saturating_add(json_array_size(
                std::iter::once(self.selected_relation_bytes),
                self.selected_relations.len(),
            ))
    }
}
