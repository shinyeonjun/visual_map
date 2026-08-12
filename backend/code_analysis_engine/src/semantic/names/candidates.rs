//! Overview에서 Codex 이름 분석에 필요한 최소 후보를 만든다.

use crate::facts::CodeUnitKind;
use crate::views::overview::model::OverviewResponse;
use std::collections::{BTreeSet, HashMap, VecDeque};

use super::context::{NameDomainContext, NameModuleContext};

const MAX_DOMAIN_SYMBOLS: usize = 24;
const MAX_DOMAIN_PATHS: usize = 12;
const MAX_DOMAIN_ENTRYPOINTS: usize = 8;
const MAX_DOMAIN_RESOURCES: usize = 12;
const MAX_MODULE_SYMBOLS: usize = 24;
const MAX_MODULE_PATHS: usize = 8;
const MAX_MODULE_CALLS: usize = 24;

pub(super) fn domains(overview: &OverviewResponse) -> Vec<NameDomainContext> {
    let units_by_id: HashMap<_, _> = overview
        .units
        .iter()
        .map(|unit| (unit.id.as_str(), unit))
        .collect();
    let entrypoints_by_id: HashMap<_, _> = overview
        .entrypoints
        .iter()
        .map(|entrypoint| (entrypoint.id.as_str(), entrypoint))
        .collect();
    let resources_by_id: HashMap<_, _> = overview
        .resources
        .iter()
        .map(|resource| (resource.id.as_str(), resource))
        .collect();

    overview
        .domains
        .iter()
        .map(|domain| {
            let unit_ids = domain
                .primary_unit_ids
                .iter()
                .chain(&domain.shared_unit_ids);
            let mut symbols = BTreeSet::new();
            let mut paths = BTreeSet::new();
            for unit_id in unit_ids {
                if let Some(unit) = units_by_id.get(unit_id.as_str()) {
                    if !unit.name.is_empty() && !is_file_unit(unit.kind.clone()) {
                        symbols.insert(unit.name.clone());
                    }
                    paths.insert(unit.relative_path.clone());
                }
            }
            let mut entrypoints = domain
                .entrypoint_ids
                .iter()
                .filter_map(|id| entrypoints_by_id.get(id.as_str()))
                .map(|entrypoint| entrypoint_display_name(entrypoint))
                .collect::<Vec<_>>();
            entrypoints.sort();
            entrypoints.dedup();

            let mut resources = domain
                .resource_ids
                .iter()
                .filter_map(|id| resources_by_id.get(id.as_str()))
                .map(|resource| format!("{:?}:{}", resource.kind, resource.name))
                .collect::<Vec<_>>();
            resources.sort();
            resources.dedup();

            NameDomainContext {
                id: domain.id.clone(),
                candidate_key: domain.key.clone(),
                current_name: domain.label.clone(),
                symbols: take_sorted(symbols, MAX_DOMAIN_SYMBOLS),
                paths: take_sorted(paths, MAX_DOMAIN_PATHS),
                entrypoints: take_sorted(entrypoints.into_iter().collect(), MAX_DOMAIN_ENTRYPOINTS),
                resources: take_sorted(resources.into_iter().collect(), MAX_DOMAIN_RESOURCES),
            }
        })
        .collect()
}

pub(super) fn modules(overview: &OverviewResponse) -> Vec<NameModuleContext> {
    let children = children_by_parent(overview);
    let domains_by_unit = domains_by_unit(overview);
    let units_by_id: HashMap<_, _> = overview
        .units
        .iter()
        .map(|unit| (unit.id.as_str(), unit))
        .collect();
    let references_by_source = references_by_source(overview);

    let mut candidates = overview
        .units
        .iter()
        .filter(|unit| is_module_unit(unit.kind.clone()))
        .map(|unit| {
            let member_ids = descendant_ids(&unit.id, &children);
            let member_units = member_ids
                .iter()
                .filter_map(|id| units_by_id.get(id.as_str()));
            let mut symbols = BTreeSet::new();
            let mut paths = BTreeSet::new();
            for member in member_units {
                if !member.name.is_empty() {
                    symbols.insert(member.name.clone());
                }
                paths.insert(member.relative_path.clone());
            }
            let mut calls = BTreeSet::new();
            for member_id in &member_ids {
                if let Some(references) = references_by_source.get(member_id.as_str()) {
                    for reference in references {
                        if matches!(reference.kind, crate::facts::ReferenceKind::Call)
                            && !reference.target_name.is_empty()
                        {
                            calls.insert(reference.target_name.clone());
                        }
                    }
                }
            }

            let domain_ids = member_ids
                .iter()
                .flat_map(|id| domains_by_unit.get(id.as_str()).into_iter().flatten())
                .cloned()
                .collect::<BTreeSet<_>>();

            NameModuleContext {
                id: unit.id.clone(),
                current_name: unit.name.clone(),
                domain_ids: domain_ids.into_iter().collect(),
                symbols: take_sorted(symbols, MAX_MODULE_SYMBOLS),
                paths: take_sorted(paths, MAX_MODULE_PATHS),
                call_targets: take_sorted(calls, MAX_MODULE_CALLS),
            }
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.id.cmp(&right.id));
    candidates
}

fn children_by_parent(overview: &OverviewResponse) -> HashMap<String, Vec<String>> {
    let mut children = HashMap::new();
    for unit in &overview.units {
        if let Some(parent_id) = &unit.parent_id {
            children
                .entry(parent_id.clone())
                .or_insert_with(Vec::new)
                .push(unit.id.clone());
        }
    }
    children
}

fn descendant_ids(root_id: &str, children: &HashMap<String, Vec<String>>) -> Vec<String> {
    let mut result = vec![root_id.to_string()];
    let mut queue = VecDeque::from([root_id.to_string()]);
    while let Some(parent_id) = queue.pop_front() {
        for child_id in children.get(&parent_id).into_iter().flatten() {
            result.push(child_id.clone());
            queue.push_back(child_id.clone());
        }
    }
    result
}

fn domains_by_unit(overview: &OverviewResponse) -> HashMap<String, Vec<String>> {
    let mut result: HashMap<String, BTreeSet<String>> = HashMap::new();
    for domain in &overview.domains {
        for unit_id in domain
            .primary_unit_ids
            .iter()
            .chain(&domain.shared_unit_ids)
        {
            result
                .entry(unit_id.clone())
                .or_default()
                .insert(domain.id.clone());
        }
    }
    result
        .into_iter()
        .map(|(unit_id, domains)| (unit_id, domains.into_iter().collect()))
        .collect()
}

fn references_by_source(
    overview: &OverviewResponse,
) -> HashMap<String, Vec<&crate::facts::Reference>> {
    let mut result: HashMap<String, Vec<&crate::facts::Reference>> = HashMap::new();
    for reference in &overview.static_graph.edges {
        result
            .entry(reference.source_unit_id.clone())
            .or_default()
            .push(reference);
    }
    result
}

fn entrypoint_display_name(entrypoint: &crate::facts::Entrypoint) -> String {
    match (&entrypoint.method, &entrypoint.path) {
        (Some(method), Some(path)) => format!("{method} {path}"),
        (_, Some(path)) => path.clone(),
        _ => entrypoint.name.clone(),
    }
}

fn is_file_unit(kind: CodeUnitKind) -> bool {
    matches!(kind, CodeUnitKind::File)
}

fn is_module_unit(kind: CodeUnitKind) -> bool {
    matches!(
        kind,
        CodeUnitKind::Module
            | CodeUnitKind::Namespace
            | CodeUnitKind::Package
            | CodeUnitKind::Class
            | CodeUnitKind::Interface
            | CodeUnitKind::Struct
            | CodeUnitKind::Enum
            | CodeUnitKind::Trait
            | CodeUnitKind::Impl
            | CodeUnitKind::Record
            | CodeUnitKind::Mixin
            | CodeUnitKind::Extension
    )
}

fn take_sorted<T: Ord>(values: BTreeSet<T>, limit: usize) -> Vec<T> {
    values.into_iter().take(limit).collect()
}
