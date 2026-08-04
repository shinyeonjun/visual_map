#[derive(Debug, Default)]
struct DepthAnalysis {
    item_depths: HashMap<String, usize>,
    root_count: usize,
}

fn confirmed_call_depths(snapshot: &InventorySnapshot) -> DepthAnalysis {
    let item_ids = snapshot
        .items
        .iter()
        .map(|item| item.id.as_str())
        .collect::<HashSet<_>>();
    let mut outgoing = HashMap::<&str, Vec<&str>>::new();
    let mut incoming = HashSet::<&str>::new();
    for link in snapshot.links.iter().filter(|link| {
        link.is_confirmed()
            && (link.kind == "code_call" || link.engine_edge_type.as_deref() == Some("CALLS"))
    }) {
        if !item_ids.contains(link.from.as_str()) || !item_ids.contains(link.to.as_str()) {
            continue;
        }
        outgoing
            .entry(link.from.as_str())
            .or_default()
            .push(link.to.as_str());
        incoming.insert(link.to.as_str());
    }

    let api_roots = snapshot
        .items
        .iter()
        .filter(|item| item.layer == "api")
        .map(|item| item.id.as_str())
        .collect::<Vec<_>>();
    let roots = if api_roots.is_empty() {
        snapshot
            .items
            .iter()
            .filter(|item| item.kind != "column" && !incoming.contains(item.id.as_str()))
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>()
    } else {
        api_roots
    };

    let mut item_depths = HashMap::<String, usize>::new();
    let mut queue = std::collections::VecDeque::new();
    for root in &roots {
        if item_depths.insert((*root).to_string(), 0).is_none() {
            queue.push_back((*root, 0));
        }
    }
    while let Some((current, depth)) = queue.pop_front() {
        for next in outgoing.get(current).into_iter().flatten() {
            if item_depths.contains_key(*next) {
                continue;
            }
            let next_depth = depth + 1;
            item_depths.insert((*next).to_string(), next_depth);
            queue.push_back((*next, next_depth));
        }
    }

    DepthAnalysis {
        item_depths,
        root_count: roots.len(),
    }
}

fn group_depths(
    groups: &[AtlasGroup],
    item_depths: &HashMap<String, usize>,
) -> HashMap<String, usize> {
    groups
        .iter()
        .filter_map(|group| {
            group
                .member_ids
                .iter()
                .filter_map(|id| item_depths.get(id).copied())
                .min()
                .map(|depth| (group.id.clone(), depth))
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod depth_tests {
    use super::{confirmed_call_depths, group_depths, AtlasGroup};
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

    fn call(from: &str, to: &str) -> SnapshotLink {
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

    fn snapshot(items: Vec<InventoryItem>, links: Vec<SnapshotLink>) -> InventorySnapshot {
        InventorySnapshot {
            schema_version: 2,
            workspace_id: "workspace-1".to_string(),
            saved_at: "1".to_string(),
            metadata: SnapshotMetadata::default(),
            stale_reasons: Vec::new(),
            links,
            items,
        }
    }

    #[test]
    fn depth_bfs_is_cycle_safe_and_uses_the_shortest_confirmed_path() {
        let snapshot = snapshot(
            vec![item("route", "api"), item("a", "code"), item("b", "code")],
            vec![call("route", "a"), call("a", "b"), call("b", "a")],
        );

        let result = confirmed_call_depths(&snapshot);

        assert_eq!(result.root_count, 1);
        assert_eq!(result.item_depths.get("route"), Some(&0));
        assert_eq!(result.item_depths.get("a"), Some(&1));
        assert_eq!(result.item_depths.get("b"), Some(&2));
    }

    #[test]
    fn depth_bfs_returns_empty_when_a_cycle_has_no_root() {
        let snapshot = snapshot(
            vec![item("a", "code"), item("b", "code")],
            vec![call("a", "b"), call("b", "a")],
        );

        let result = confirmed_call_depths(&snapshot);

        assert_eq!(result.root_count, 0);
        assert!(result.item_depths.is_empty());
    }

    #[test]
    fn group_depth_uses_the_nearest_reachable_member() {
        let groups = vec![AtlasGroup {
            id: "group:orders".to_string(),
            title: "orders".to_string(),
            title_priority: 0,
            member_ids: vec!["unreachable".to_string(), "reachable".to_string()],
            api_count: 0,
            code_count: 2,
            db_count: 0,
            confirmed_degree: 0,
            in_degree: 0,
            out_degree: 0,
            language_counts: std::collections::HashMap::new(),
            has_partial: false,
            top_api: Vec::new(),
            top_code: Vec::new(),
            top_db: Vec::new(),
            handler_count: 0,
            service_count: 0,
            repository_count: 0,
            parent_id: None,
            depth: 0,
            assigned_by: "path-root",
        }];
        let depths = std::collections::HashMap::from([("reachable".to_string(), 3)]);

        assert_eq!(group_depths(&groups, &depths).get("group:orders"), Some(&3));
    }
}
