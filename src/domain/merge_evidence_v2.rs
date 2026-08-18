//! V2 cross-key merge gate: 증거 계층형 규칙.

use crate::domain::capabilities::Capability;
use crate::domain::feature_graph::FeatureSimilarity;
use crate::domain::formation::capability_evidence::collect_semantic_ownership;
use crate::domain::formation::key_decomposition::entity_family_match;
use crate::facts::FactStore;
use std::collections::BTreeSet;

const LEXICAL_STRONG: f64 = 0.5;
const PATH_HIGH: f64 = 0.6;
const CALL_MIN: f64 = 0.12;
const FLOW_MIN: f64 = 0.12;

pub(super) fn merge_allowed_v2(
    left: &Capability,
    right: &Capability,
    sim: &FeatureSimilarity,
    store: &FactStore,
) -> Option<&'static str> {
    let left_ownership = collect_semantic_ownership(left, store);
    let right_ownership = collect_semantic_ownership(right, store);

    let mut strong = 0usize;
    let mut medium = 0usize;

    if shared_value(&left_ownership.modules, &right_ownership.modules)
        && sim.lexical >= LEXICAL_STRONG
    {
        strong += 1;
    }

    if shared_value(
        &left_ownership.owner_classes,
        &right_ownership.owner_classes,
    ) && entity_family_match(&left.key, &right.key)
    {
        strong += 1;
    }

    if entity_family_match(&left.key, &right.key)
        && (shared_value(&left_ownership.packages, &right_ownership.packages)
            || shared_value(&left_ownership.modules, &right_ownership.modules))
    {
        strong += 1;
    }

    if nested_contract_ownership(left, right) {
        strong += 1;
    }

    if shared_value(&left_ownership.packages, &right_ownership.packages)
        && sim.lexical >= LEXICAL_STRONG
        && sim.path >= PATH_HIGH
    {
        medium += 1;
    }

    if shared_unit_path(left, right, store) && sim.lexical >= LEXICAL_STRONG {
        medium += 1;
    }

    if sim.call >= CALL_MIN
        && sim.flow >= FLOW_MIN
        && ownership_assist(&left_ownership, &right_ownership)
    {
        medium += 1;
    }

    if strong >= 1 {
        return Some("v2-strong");
    }
    if medium >= 2 {
        return Some("v2-medium");
    }
    None
}

fn shared_value(left: &[String], right: &[String]) -> bool {
    if left.is_empty() || right.is_empty() {
        return false;
    }
    let right_set: BTreeSet<_> = right.iter().collect();
    left.iter().any(|value| right_set.contains(value))
}

fn ownership_assist(
    left: &crate::domain::formation::capability_evidence::CapabilitySemanticOwnership,
    right: &crate::domain::formation::capability_evidence::CapabilitySemanticOwnership,
) -> bool {
    shared_value(&left.modules, &right.modules)
        || shared_value(&left.packages, &right.packages)
        || shared_value(&left.owner_classes, &right.owner_classes)
}

fn shared_unit_path(left: &Capability, right: &Capability, store: &FactStore) -> bool {
    let left_paths = unit_paths(left, store);
    let right_paths = unit_paths(right, store);
    if left_paths.is_empty() || right_paths.is_empty() {
        return false;
    }
    left_paths.iter().any(|path| right_paths.contains(path))
}

fn unit_paths(capability: &Capability, store: &FactStore) -> BTreeSet<String> {
    capability
        .unit_ids
        .iter()
        .filter_map(|unit_id| {
            store
                .unit(unit_id)
                .map(|unit| unit.relative_path.replace('\\', "/"))
        })
        .collect()
}

fn nested_contract_ownership(left: &Capability, right: &Capability) -> bool {
    for left_path in &left.contract_paths {
        for right_path in &right.contract_paths {
            if left_path == right_path {
                return true;
            }
            if is_nested_contract_path(left_path, right_path)
                || is_nested_contract_path(right_path, left_path)
            {
                return true;
            }
        }
    }
    false
}

fn is_nested_contract_path(outer: &str, inner: &str) -> bool {
    if !outer.starts_with(inner) {
        return false;
    }
    outer.strip_prefix(inner).is_some_and(|suffix| {
        suffix.is_empty() || suffix.starts_with('/') || suffix.starts_with('.')
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::feature_graph::FeatureSimilarity;
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

    fn sim(lexical: f64, path: f64, call: f64, flow: f64) -> FeatureSimilarity {
        FeatureSimilarity {
            http_match: 0.0,
            call,
            flow,
            resource: 0.0,
            path,
            lexical,
            combined: lexical + path + call + flow,
        }
    }

    #[test]
    fn v2는_http_단독으로_병합하지_않는다() {
        let left = capability("apple");
        let right = capability("firebase");
        let sim = FeatureSimilarity {
            http_match: 1.0,
            call: 0.0,
            flow: 0.0,
            resource: 0.0,
            path: 0.0,
            lexical: 0.0,
            combined: 1.0,
        };
        assert!(merge_allowed_v2(&left, &right, &sim, &FactStore::default()).is_none());
    }

    #[test]
    fn v2는_lexical_단독으로_병합하지_않는다() {
        let left = capability("accounts");
        let right = capability("contacts");
        assert!(merge_allowed_v2(
            &left,
            &right,
            &sim(0.9, 0.0, 0.0, 0.0),
            &FactStore::default()
        )
        .is_none());
    }

    #[test]
    fn v2는_medium_2개로_병합한다() {
        let mut left = capability("accounts");
        let mut right = capability("contacts");
        left.unit_ids.push("unit-a".into());
        right.unit_ids.push("unit-b".into());
        let mut store = FactStore::default();
        let path = "src/app/api/routes.py";
        store.units.insert(
            "unit-a".into(),
            crate::facts::CodeUnit {
                id: "unit-a".into(),
                kind: crate::facts::CodeUnitKind::Module,
                name: "routes".into(),
                qualified_name: "other.module".into(),
                file_id: "file-1".into(),
                relative_path: path.into(),
                language: crate::model::Language::Python,
                parent_id: None,
                span: crate::facts::SourceSpan::new("file-1", path, 1, 1, 2, 1),
                body_span: None,
                signature: None,
                parameters: Vec::new(),
                return_type: None,
                visibility: crate::facts::CodeUnitVisibility::default(),
                modifiers: Vec::new(),
                exported: true,
            },
        );
        store.units.insert(
            "unit-b".into(),
            crate::facts::CodeUnit {
                id: "unit-b".into(),
                kind: crate::facts::CodeUnitKind::Module,
                name: "routes".into(),
                qualified_name: "another.module".into(),
                file_id: "file-1".into(),
                relative_path: path.into(),
                language: crate::model::Language::Python,
                parent_id: None,
                span: crate::facts::SourceSpan::new("file-1", path, 1, 1, 2, 1),
                body_span: None,
                signature: None,
                parameters: Vec::new(),
                return_type: None,
                visibility: crate::facts::CodeUnitVisibility::default(),
                modifiers: Vec::new(),
                exported: true,
            },
        );
        assert_eq!(
            merge_allowed_v2(&left, &right, &sim(0.6, 0.7, 0.0, 0.0), &store),
            Some("v2-medium")
        );
    }

    #[test]
    fn v2는_nested_contract로_strong_병합한다() {
        let mut left = capability("billing");
        let mut right = capability("billing-item");
        left.contract_paths.insert("api.billing".into());
        right.contract_paths.insert("api.billing.item".into());
        assert_eq!(
            merge_allowed_v2(
                &left,
                &right,
                &sim(0.0, 0.0, 0.0, 0.0),
                &FactStore::default()
            ),
            Some("v2-strong")
        );
    }
}
