//! ORM별 adapter가 공유하는 모델·접근 resource materializer다.

use crate::facts::{AccessMode, CodeUnitKind, Evidence, FactStore, ResourceAccess, ResourceKind};
use crate::languages::common::metadata::stable_id;
use std::collections::{HashMap, HashSet};

pub(super) fn add_resources(
    facts: &mut FactStore,
    file_frameworks: &HashMap<String, Vec<String>>,
    framework_ids: &[&str],
    excluded_framework_ids: &[&str],
) {
    let model_units = collect_model_units(
        facts,
        file_frameworks,
        framework_ids,
        excluded_framework_ids,
    );
    if model_units.is_empty() {
        return;
    }
    let variable_models = collect_variable_models(facts, &model_units);
    let mut resources = model_resources(facts, &model_units);
    resources.extend(call_resources(facts, &model_units, &variable_models));

    let existing = facts
        .resources
        .iter()
        .map(|resource| resource.id.clone())
        .collect::<HashSet<_>>();
    facts.resources.extend(
        resources
            .into_iter()
            .filter(|resource| !existing.contains(&resource.id)),
    );
}

fn collect_model_units(
    facts: &FactStore,
    file_frameworks: &HashMap<String, Vec<String>>,
    framework_ids: &[&str],
    excluded_framework_ids: &[&str],
) -> HashMap<String, String> {
    facts
        .units
        .values()
        .filter(|unit| unit.language.key() == "python" && unit.kind == CodeUnitKind::Class)
        .filter(|unit| {
            unit.signature
                .as_deref()
                .is_some_and(is_orm_model_signature)
        })
        .filter(|unit| {
            file_frameworks
                .get(unit.file_id.as_str())
                .is_some_and(|ids| {
                    ids.iter().any(|id| framework_ids.contains(&id.as_str()))
                        && !ids
                            .iter()
                            .any(|id| excluded_framework_ids.contains(&id.as_str()))
                })
        })
        .map(|unit| (unit.name.clone(), unit.id.clone()))
        .collect()
}

fn collect_variable_models(
    facts: &FactStore,
    model_units: &HashMap<String, String>,
) -> HashMap<String, String> {
    facts
        .bindings
        .iter()
        .filter_map(|binding| {
            model_units
                .get(
                    binding
                        .target_name
                        .rsplit('.')
                        .next()
                        .unwrap_or(&binding.target_name),
                )
                .map(|unit_id| (binding.local_name.clone(), unit_id.clone()))
        })
        .collect()
}

fn model_resources(
    facts: &FactStore,
    model_units: &HashMap<String, String>,
) -> Vec<ResourceAccess> {
    model_units
        .iter()
        .filter_map(|(model_name, unit_id)| {
            let unit = facts.unit(unit_id)?;
            Some(ResourceAccess {
                id: stable_id("resource", &format!("orm-model:{}", unit.id)),
                unit_id: unit.id.clone(),
                kind: ResourceKind::Table,
                name: default_table_name(model_name),
                mode: AccessMode::Unknown,
                evidence: vec![Evidence::new("ormModel", model_name, unit.span.clone())],
            })
        })
        .collect()
}

fn call_resources(
    facts: &FactStore,
    model_units: &HashMap<String, String>,
    variable_models: &HashMap<String, String>,
) -> Vec<ResourceAccess> {
    let mut resources = Vec::new();
    for call in &facts.call_sites {
        let method = call.callee.rsplit('.').next().unwrap_or(&call.callee);
        let mode =
            match method.to_ascii_lowercase().as_str() {
                "get" | "query" | "select" | "filter" | "first" | "last" | "one" | "all"
                | "exists" | "count" | "values" | "values_list" | "order_by" | "annotate"
                | "exclude" => AccessMode::Read,
                "add" | "add_all" | "merge" | "delete" | "remove" | "update" | "commit"
                | "create" | "get_or_create" | "update_or_create" | "bulk_create"
                | "bulk_update" => AccessMode::Write,
                "execute" | "exec" => {
                    if call.arguments.iter().any(|argument| {
                        argument.to_ascii_lowercase().contains("select")
                            || argument.to_ascii_lowercase().contains("query")
                    }) {
                        AccessMode::Read
                    } else {
                        AccessMode::ReadWrite
                    }
                }
                _ => continue,
            };
        let Some(model_name) = call.arguments.iter().find_map(|argument| {
            model_candidate_names(argument)
                .into_iter()
                .find_map(|candidate| {
                    if model_units.contains_key(candidate.as_str()) {
                        Some(candidate)
                    } else {
                        variable_models
                            .get(candidate.as_str())
                            .and_then(|unit_id| facts.unit(unit_id))
                            .map(|unit| unit.name.clone())
                    }
                })
        }) else {
            let Some(model_name) = model_candidate_names(&call.callee)
                .into_iter()
                .find(|candidate| model_units.contains_key(candidate))
            else {
                continue;
            };
            let Some(evidence) = call.evidence.first() else {
                continue;
            };
            resources.push(ResourceAccess {
                id: stable_id(
                    "resource",
                    &format!("orm-call:{}:{}:{:?}", call.id, model_name, mode),
                ),
                unit_id: call.source_unit_id.clone(),
                kind: ResourceKind::Table,
                name: default_table_name(&model_name),
                mode,
                evidence: vec![Evidence::new(
                    "ormAccess",
                    model_name,
                    evidence.span.clone(),
                )],
            });
            continue;
        };
        let Some(evidence) = call.evidence.first() else {
            continue;
        };
        resources.push(ResourceAccess {
            id: stable_id(
                "resource",
                &format!("orm-call:{}:{}:{:?}", call.id, model_name, mode),
            ),
            unit_id: call.source_unit_id.clone(),
            kind: ResourceKind::Table,
            name: default_table_name(&model_name),
            mode,
            evidence: vec![Evidence::new(
                "ormAccess",
                model_name,
                evidence.span.clone(),
            )],
        });
    }
    resources
}

fn is_orm_model_signature(signature: &str) -> bool {
    let compact = signature
        .to_ascii_lowercase()
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    compact.contains("table=true")
        || compact.contains("declarativebase")
        || compact.contains("declarative_base")
        || compact.contains("db.model")
        || compact.contains("models.model")
        || compact.contains("django.db.models.model")
        || compact.contains("(base)")
        || compact.contains("(dbbase)")
}

fn model_candidate_names(value: &str) -> Vec<String> {
    let value = value.trim();
    let mut candidates = Vec::new();
    for token in
        value.split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
    {
        if token.is_empty() || candidates.iter().any(|candidate| candidate == token) {
            continue;
        }
        candidates.push(token.to_string());
    }
    candidates
}

fn default_table_name(model_name: &str) -> String {
    let mut result = String::new();
    for (index, character) in model_name.chars().enumerate() {
        if character.is_uppercase() && index > 0 {
            result.push('_');
        }
        result.push(character.to_ascii_lowercase());
    }
    result
}
