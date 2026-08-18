//! capability별 merge 증거를 덤프한다.

use crate::domain::capabilities::Capability;
use crate::domain::formation::key_decomposition::{decompose_capability_key, KeyDecomposition};
use crate::domain::tfidf::FeatureTerms;
use crate::facts::{CodeUnitKind, Entrypoint, FactStore};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use super::capability_data::CapabilityData;

const TOP_LEXICAL_TERMS: usize = 12;
const PACKAGE_SKIP_SEGMENTS: &[&str] = &["src", "lib", "pkg", "internal", "test", "tests"];

#[derive(Debug, Clone, Default)]
pub(crate) struct CapabilitySemanticOwnership {
    pub modules: Vec<String>,
    pub packages: Vec<String>,
    pub owner_classes: Vec<String>,
}

pub(crate) fn collect_semantic_ownership(
    capability: &Capability,
    store: &FactStore,
) -> CapabilitySemanticOwnership {
    let mut owner_classes = BTreeSet::new();
    let mut modules = BTreeSet::new();
    let mut packages = BTreeSet::new();

    for entrypoint_id in &capability.entrypoint_ids {
        let Some(entrypoint) = store
            .entrypoints
            .iter()
            .find(|entrypoint| entrypoint.id == *entrypoint_id)
        else {
            continue;
        };
        for class in owner_classes_for_entrypoint(entrypoint, store) {
            owner_classes.insert(class);
        }
    }

    for unit_id in &capability.unit_ids {
        let Some(unit) = store.unit(unit_id) else {
            continue;
        };
        if let Some(module) = module_from_qualified_name(&unit.qualified_name) {
            modules.insert(module);
        }
        if let Some(package) = package_from_relative_path(&unit.relative_path) {
            packages.insert(package);
        }
    }

    CapabilitySemanticOwnership {
        modules: modules.into_iter().collect(),
        packages: packages.into_iter().collect(),
        owner_classes: owner_classes.into_iter().collect(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntrypointEvidence {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub method: Option<String>,
    pub path: Option<String>,
    pub unit_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SemanticOwnershipEvidence {
    pub modules: Vec<String>,
    pub packages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityEvidence {
    pub key: String,
    pub entrypoints: Vec<EntrypointEvidence>,
    pub owner_classes: Vec<String>,
    pub semantic_ownership: SemanticOwnershipEvidence,
    pub key_decomposition: KeyDecomposition,
    pub contract_paths: Vec<String>,
    pub resource_ids: Vec<String>,
    pub unit_paths: Vec<String>,
    pub flow_ids: Vec<String>,
    pub top_lexical_terms: Vec<String>,
}

pub(super) fn build_capability_evidence(
    capability: &Capability,
    capability_data: &CapabilityData,
    index: usize,
    terms: &FeatureTerms,
    store: &FactStore,
) -> CapabilityEvidence {
    let mut owner_classes = BTreeSet::new();
    let mut modules = BTreeSet::new();
    let mut packages = BTreeSet::new();
    let entrypoints = capability
        .entrypoint_ids
        .iter()
        .filter_map(|entrypoint_id| {
            store
                .entrypoints
                .iter()
                .find(|entrypoint| entrypoint.id == *entrypoint_id)
        })
        .map(|entrypoint| {
            for class in owner_classes_for_entrypoint(entrypoint, store) {
                owner_classes.insert(class);
            }
            entrypoint_evidence(entrypoint, store)
        })
        .collect();

    let unit_paths = capability
        .unit_ids
        .iter()
        .filter_map(|unit_id| {
            store
                .unit(unit_id)
                .map(|unit| unit.relative_path.replace('\\', "/"))
        })
        .collect();

    for unit_id in &capability.unit_ids {
        let Some(unit) = store.unit(unit_id) else {
            continue;
        };
        if let Some(module) = module_from_qualified_name(&unit.qualified_name) {
            modules.insert(module);
        }
        if let Some(package) = package_from_relative_path(&unit.relative_path) {
            packages.insert(package);
        }
    }

    CapabilityEvidence {
        key: capability.key.clone(),
        entrypoints,
        owner_classes: owner_classes.into_iter().collect(),
        semantic_ownership: SemanticOwnershipEvidence {
            modules: modules.into_iter().collect(),
            packages: packages.into_iter().collect(),
        },
        key_decomposition: decompose_capability_key(&capability.key),
        contract_paths: capability.contract_paths.iter().cloned().collect(),
        resource_ids: capability.resource_ids.clone(),
        unit_paths,
        flow_ids: capability_data.flow_ids[index].clone(),
        top_lexical_terms: top_lexical_terms(terms),
    }
}

fn module_from_qualified_name(qualified_name: &str) -> Option<String> {
    let separator = if qualified_name.contains("::") {
        "::"
    } else if qualified_name.contains('.') {
        "."
    } else {
        return None;
    };
    let (module, _) = qualified_name.rsplit_once(separator)?;
    if module.is_empty() {
        return None;
    }
    Some(module.replace('\\', "/"))
}

fn package_from_relative_path(relative_path: &str) -> Option<String> {
    let normalized = relative_path.replace('\\', "/");
    let segments: Vec<_> = normalized
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.is_empty() {
        return None;
    }
    let start = segments
        .iter()
        .position(|segment| !PACKAGE_SKIP_SEGMENTS.contains(segment))
        .unwrap_or(0);
    let end = segments.len().saturating_sub(1).max(start);
    if end <= start {
        return segments.get(start).map(|segment| (*segment).to_string());
    }
    Some(segments[start..end].join("/"))
}

fn entrypoint_evidence(entrypoint: &Entrypoint, store: &FactStore) -> EntrypointEvidence {
    let unit_path = store
        .unit(&entrypoint.unit_id)
        .map(|unit| unit.relative_path.replace('\\', "/"))
        .unwrap_or_default();
    EntrypointEvidence {
        id: entrypoint.id.clone(),
        kind: format!("{:?}", entrypoint.kind),
        name: entrypoint.name.clone(),
        method: entrypoint.method.clone(),
        path: entrypoint.path.clone(),
        unit_path,
    }
}

fn owner_classes_for_entrypoint(entrypoint: &Entrypoint, store: &FactStore) -> Vec<String> {
    let Some(unit) = store.unit(&entrypoint.unit_id) else {
        return Vec::new();
    };
    let mut classes = Vec::new();
    let mut current = Some(unit);
    while let Some(unit) = current {
        if unit.kind == CodeUnitKind::Class {
            classes.push(unit.name.clone());
        }
        current = unit
            .parent_id
            .as_deref()
            .and_then(|parent_id| store.unit(parent_id));
    }
    classes
}

fn top_lexical_terms(terms: &FeatureTerms) -> Vec<String> {
    let mut ranked: Vec<_> = terms
        .term_frequencies
        .iter()
        .map(|(term, weight)| (term.clone(), *weight))
        .collect();
    ranked.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });
    ranked
        .into_iter()
        .take(TOP_LEXICAL_TERMS)
        .map(|(term, _)| term)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package는_src_접두를_건너뛴다() {
        assert_eq!(
            package_from_relative_path("src/app/api/routes/login.py").as_deref(),
            Some("app/api/routes")
        );
    }
}
