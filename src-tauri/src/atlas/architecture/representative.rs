use std::collections::VecDeque;

pub(super) fn representative_paths(
    snapshot: &InventorySnapshot,
    limit: usize,
) -> Vec<RepresentativePath> {
    let item_by_id = snapshot
        .items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let mut outgoing = HashMap::<&str, Vec<&str>>::new();
    let mut confirmed_ids = HashSet::new();
    for link in snapshot.links.iter().filter(|link| link.is_confirmed()) {
        if item_by_id.contains_key(link.from.as_str()) && item_by_id.contains_key(link.to.as_str()) {
            outgoing
                .entry(link.from.as_str())
                .or_default()
                .push(link.to.as_str());
            confirmed_ids.insert(link.from.as_str());
            confirmed_ids.insert(link.to.as_str());
        }
    }
    for neighbors in outgoing.values_mut() {
        neighbors.sort_unstable();
        neighbors.dedup();
    }

    let mut entries = snapshot
        .items
        .iter()
        .filter(|item| {
            item.layer == "api"
                || matches!(item.kind.as_str(), "endpoint" | "route" | "job")
        })
        .filter(|item| confirmed_ids.contains(item.id.as_str()) || item.layer == "api")
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.name.cmp(&right.name).then_with(|| left.id.cmp(&right.id)));
    if entries.is_empty() || limit == 0 {
        return Vec::new();
    }

    let reachable = entries
        .iter()
        .map(|entry| (entry.id.as_str(), bfs(entry.id.as_str(), &outgoing)))
        .collect::<HashMap<_, _>>();
    let universe = reachable
        .values()
        .flat_map(|items| items.iter().copied())
        .collect::<HashSet<_>>();
    if universe.is_empty() {
        return Vec::new();
    }

    let mut covered = HashSet::new();
    let mut selected = HashSet::new();
    let mut result = Vec::new();
    while result.len() < limit && covered.len() < universe.len() {
        let Some((entry, items)) = entries
            .iter()
            .filter(|entry| !selected.contains(entry.id.as_str()))
            .filter_map(|entry| reachable.get(entry.id.as_str()).map(|items| (*entry, items)))
            .map(|(entry, items)| {
                let gain = items
                    .iter()
                    .filter(|item| !covered.contains(*item))
                    .count();
                (entry, items, gain)
            })
            .max_by(|(left, left_items, left_gain), (right, right_items, right_gain)| {
                left_gain
                    .cmp(right_gain)
                    .then_with(|| left_items.len().cmp(&right_items.len()))
                    .then_with(|| right.name.cmp(&left.name))
                    .then_with(|| right.id.cmp(&left.id))
            })
            .map(|(entry, items, _)| (entry, items))
        else {
            break;
        };
        let new_coverage = items
            .iter()
            .filter(|item| covered.insert(**item))
            .count();
        selected.insert(entry.id.as_str());
        result.push(RepresentativePath {
            entry_id: entry.id.clone(),
            title: entry.name.clone(),
            method: None,
            step_count: items.len().saturating_sub(1),
            new_coverage,
            cumulative_share: covered.len() as f32 / universe.len() as f32,
        });
    }
    result
}

fn bfs<'a>(start: &'a str, outgoing: &HashMap<&'a str, Vec<&'a str>>) -> HashSet<&'a str> {
    let mut result = HashSet::from([start]);
    let mut queue = VecDeque::from([start]);
    while let Some(current) = queue.pop_front() {
        for next in outgoing.get(current).into_iter().flatten() {
            if result.insert(*next) {
                queue.push_back(next);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::representative_paths;
    use crate::atlas::model::{InventoryItem, InventorySnapshot, SnapshotLink, SnapshotMetadata};

    fn item(id: &str, layer: &str) -> InventoryItem {
        InventoryItem {
            id: id.to_string(),
            kind: if layer == "api" { "api" } else { "function" }.to_string(),
            name: id.to_string(),
            layer: layer.to_string(),
            source: "code".to_string(),
            parent_id: None,
            path: None,
            qualified_name: None,
            engine_label: None,
            language: None,
            role_basis: None,
            project_id: None,
            group_id: None,
            location: None,
            is_primary_key: false,
            is_foreign_key: false,
            nullable: None,
        }
    }

    fn link(from: &str, to: &str) -> SnapshotLink {
        SnapshotLink {
            id: format!("{from}->{to}"),
            from: from.to_string(),
            to: to.to_string(),
            kind: "code_call".to_string(),
            label: None,
            truth_class: "confirmed".to_string(),
            direction: "forward".to_string(),
            engine_edge_type: Some("CALLS".to_string()),
            evidence: Vec::new(),
        }
    }

    #[test]
    fn greedy_paths_are_stable_and_cycle_safe() {
        let snapshot = InventorySnapshot {
            schema_version: 2,
            workspace_id: "w".to_string(),
            saved_at: "1".to_string(),
            metadata: SnapshotMetadata::default(),
            stale_reasons: Vec::new(),
            links: vec![link("a", "x"), link("x", "y"), link("b", "y"), link("y", "x")],
            items: vec![item("a", "api"), item("b", "api"), item("x", "code"), item("y", "code")],
        };
        let first = representative_paths(&snapshot, 5);
        let second = representative_paths(&snapshot, 5);
        assert_eq!(first, second);
        assert_eq!(first.len(), 2);
        assert_eq!(first.last().unwrap().cumulative_share, 1.0);
    }

    #[test]
    fn no_entry_returns_empty_without_panicking() {
        let snapshot = InventorySnapshot {
            schema_version: 2,
            workspace_id: "w".to_string(),
            saved_at: "1".to_string(),
            metadata: SnapshotMetadata::default(),
            stale_reasons: Vec::new(),
            links: vec![link("x", "y")],
            items: vec![item("x", "code"), item("y", "code")],
        };
        assert!(representative_paths(&snapshot, 5).is_empty());
    }
}
