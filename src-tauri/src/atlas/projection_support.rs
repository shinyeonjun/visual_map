use std::collections::HashSet;

use super::model::{Evidence, ImpactReviewItem, InventoryItem};

pub(super) fn assign_review_ranks(items: &mut [ImpactReviewItem]) {
    for (index, item) in items.iter_mut().enumerate() {
        item.rank = index + 1;
    }
}

pub(super) fn safe_evidence(evidence: &[Evidence]) -> Vec<Evidence> {
    let mut seen = HashSet::new();
    evidence
        .iter()
        .filter_map(|entry| {
            let text = safe_text(&entry.text);
            seen.insert((entry.kind.clone(), text.clone()))
                .then(|| Evidence {
                    kind: entry.kind.clone(),
                    text,
                })
        })
        .take(6)
        .collect()
}

pub(super) fn safe_text(value: &str) -> String {
    crate::engine::redact_secrets(value)
}

pub(super) fn confidence_rank(confidence: &str) -> u8 {
    match confidence {
        "high" => 0,
        "medium" => 1,
        "low" => 2,
        _ => 3,
    }
}

pub(super) fn compact_token(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>()
}

pub(super) fn node_sort_key(item: Option<&InventoryItem>) -> (u8, String) {
    match item {
        Some(item) => (layer_rank(&item.layer), item.name.clone()),
        None => (9, String::new()),
    }
}

fn layer_rank(layer: &str) -> u8 {
    match layer {
        "api" => 0,
        "code" => 1,
        "data" => 2,
        _ => 3,
    }
}
