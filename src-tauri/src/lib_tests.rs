#[cfg(test)]
mod analysis_source_mode_tests {
    use super::AnalysisSourceMode;

    #[test]
    fn each_mode_selects_only_its_declared_sources() {
        assert!(AnalysisSourceMode::CodeOnly.includes_code());
        assert!(!AnalysisSourceMode::CodeOnly.includes_db());
        assert!(!AnalysisSourceMode::DbOnly.includes_code());
        assert!(AnalysisSourceMode::DbOnly.includes_db());
        assert!(AnalysisSourceMode::CodeAndDb.includes_code());
        assert!(AnalysisSourceMode::CodeAndDb.includes_db());
    }

    #[test]
    fn missing_mode_keeps_code_only_compatibility_default() {
        let request: super::InitializeWorkspaceAnalysisRequest =
            serde_json::from_value(serde_json::json!({
                "workspaceId": "workspace"
            }))
            .unwrap();

        assert_eq!(request.analysis_mode, AnalysisSourceMode::CodeOnly);
    }

    #[test]
    fn combined_mode_promotes_only_when_both_required_sources_are_ready() {
        assert!(AnalysisSourceMode::CodeOnly.required_sources_ready(true, false));
        assert!(AnalysisSourceMode::DbOnly.required_sources_ready(false, true));
        assert!(AnalysisSourceMode::CodeAndDb.required_sources_ready(true, true));
        assert!(!AnalysisSourceMode::CodeAndDb.required_sources_ready(true, false));
        assert!(!AnalysisSourceMode::CodeAndDb.required_sources_ready(false, true));
    }
}

#[cfg(test)]
mod code_evidence_tests {
    use super::{enrich_integrated_snapshot_code_evidence, Workspace};
    use crate::atlas::{
        api_code_evidence_target_ids, focused_code_path_filter, normalized_change_intent,
        ChangeIntent, InventorySnapshot,
    };
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn api_evidence_search_targets_only_reachable_db_candidates() {
        let snapshot: InventorySnapshot = serde_json::from_value(serde_json::json!({
            "schemaVersion": 2,
            "workspaceId": "shop",
            "savedAt": "1",
            "items": [
                { "id": "code:route", "kind": "api", "name": "/sessions", "layer": "api", "source": "code", "parentId": null, "path": "routes.py" },
                { "id": "code:handler", "kind": "function", "name": "listSessions", "layer": "code", "source": "code", "parentId": null, "path": "routes.py" },
                { "id": "code:repository", "kind": "function", "name": "sessionsRepository", "layer": "code", "source": "code", "parentId": null, "path": "repository.py" },
                { "id": "code:unreachable", "kind": "function", "name": "auditRepository", "layer": "code", "source": "code", "parentId": null, "path": "audit.py" },
                { "id": "db:table:public.sessions", "kind": "table", "name": "sessions", "layer": "db", "source": "db", "parentId": null, "path": null },
                { "id": "db:table:public.audit", "kind": "table", "name": "audit", "layer": "db", "source": "db", "parentId": null, "path": null }
            ],
            "links": [
                { "id": "handles", "from": "code:route", "to": "code:handler", "kind": "code_handle", "truthClass": "confirmed", "direction": "outbound", "engineEdgeType": "HANDLES", "label": null, "evidence": [] },
                { "id": "calls", "from": "code:handler", "to": "code:repository", "kind": "code_call", "truthClass": "confirmed", "direction": "outbound", "engineEdgeType": "CALLS", "label": null, "evidence": [] }
            ]
        }))
        .unwrap();

        assert_eq!(
            api_code_evidence_target_ids(&snapshot, "code:route"),
            vec!["db:table:public.sessions"]
        );
    }

    #[test]
    fn column_search_path_filter_is_exact_deduplicated_and_bounded() {
        let mut paths = (0..20)
            .map(|index| format!("src/r{index}/orders.query.ts"))
            .collect::<Vec<_>>();
        paths.push("src/r0/orders.query.ts".to_string());

        let (filter, omitted) = focused_code_path_filter(&paths);
        let filter = filter.unwrap();

        assert!(filter.starts_with("^("));
        assert!(filter.ends_with(")$"));
        assert!(filter.contains(r"orders\.query\.ts"));
        assert!(filter.len() <= 512);
        assert_eq!(omitted, 4);
    }

    #[test]
    fn change_intent_is_bounded_and_normalized() {
        let intent = normalized_change_intent(Some(ChangeIntent {
            kind: "rename".to_string(),
            value: Some("  display_name  ".to_string()),
        }))
        .unwrap()
        .unwrap();
        assert_eq!(intent.value.as_deref(), Some("display_name"));

        assert!(normalized_change_intent(Some(ChangeIntent {
            kind: "nullability".to_string(),
            value: Some("sometimes".to_string()),
        }))
        .is_err());
        assert!(normalized_change_intent(Some(ChangeIntent {
            kind: "rename".to_string(),
            value: Some("x".repeat(129)),
        }))
        .is_err());
    }

    #[test]
    fn integrated_snapshot_runs_sql_evidence_when_both_sources_exist() {
        let root = std::env::temp_dir().join(format!(
            "backend-map-integrated-evidence-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("repo.py"),
            "cursor.execute(\"SELECT id FROM users\")\n",
        )
        .unwrap();

        let mut snapshot: InventorySnapshot = serde_json::from_value(serde_json::json!({
            "schemaVersion": 2,
            "workspaceId": "shop",
            "savedAt": "1",
            "metadata": {
                "code": { "savedAt": "1", "sourceType": "local" },
                "db": { "savedAt": "1", "sourceType": "ddl-sqlite" }
            },
            "items": [
                {
                    "id": "code:query",
                    "kind": "function",
                    "name": "list_users",
                    "layer": "code",
                    "source": "code",
                    "parentId": null,
                    "path": "repo.py",
                    "location": { "path": "repo.py", "line": 1, "endLine": 1 }
                },
                {
                    "id": "db:table:users",
                    "kind": "table",
                    "name": "users",
                    "layer": "db",
                    "source": "db",
                    "parentId": null,
                    "path": null
                },
                {
                    "id": "db:column:users:id",
                    "kind": "column",
                    "name": "id",
                    "layer": "db",
                    "source": "db",
                    "parentId": "db:table:users",
                    "path": null
                }
            ],
            "links": []
        }))
        .unwrap();
        let workspace = Workspace {
            id: "shop".to_string(),
            name: "shop".to_string(),
            repo_path: root.display().to_string(),
            repo_source: Default::default(),
            repo_origin: None,
            code_project: Some("shop".to_string()),
            engine_cache: Default::default(),
            db_profiles: Vec::new(),
            active_db_profile_id: None,
            created_at: "1".to_string(),
            updated_at: "1".to_string(),
        };

        enrich_integrated_snapshot_code_evidence(&workspace, &mut snapshot);

        assert!(snapshot.links.iter().any(|link| {
            link.kind == "code_db_read" && link.from == "code:query" && link.to == "db:table:users"
        }));
        assert!(snapshot
            .links
            .iter()
            .any(|link| { link.kind == "code_db_uses_column" && link.to == "db:column:users:id" }));
        let _ = fs::remove_dir_all(root);
    }
}

#[cfg(test)]
mod command_tests {
    use super::*;

    #[test]
    fn workspace_mutations_are_serialized_per_workspace_and_release_on_drop() {
        let root = std::env::temp_dir().join(format!(
            "backend-visual-map-workspace-lock-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let first = begin_workspace_mutation(&root, "guard-workspace-a").unwrap();
        assert!(begin_workspace_mutation(&root, "guard-workspace-a").is_err());
        let other = begin_workspace_mutation(&root, "guard-workspace-b").unwrap();
        drop(other);
        drop(first);
        assert!(begin_workspace_mutation(&root, "guard-workspace-a").is_ok());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn deleting_inactive_db_profile_keeps_active_snapshot() {
        let workspace = Workspace {
            id: "workspace".to_string(),
            name: "workspace".to_string(),
            repo_path: "D:/repo".to_string(),
            repo_source: Default::default(),
            repo_origin: None,
            code_project: None,
            engine_cache: Default::default(),
            db_profiles: Vec::new(),
            active_db_profile_id: Some("active".to_string()),
            created_at: "1".to_string(),
            updated_at: "1".to_string(),
        };

        assert!(!should_remove_db_snapshot(&workspace, "inactive"));
        assert!(should_remove_db_snapshot(&workspace, "active"));
    }

    #[test]
    fn bounded_code_inventory_keeps_totals_and_drops_dangling_relationships() {
        let functions = (0..101)
            .map(|index| {
                serde_json::json!({
                    "id": format!("function-{index}"),
                    "kind": "function",
                    "name": format!("function_{index}"),
                    "detail": {}
                })
            })
            .collect::<Vec<_>>();
        let inventory: CodeInventory = serde_json::from_value(serde_json::json!({
            "project": "test",
            "routes": [],
            "services": [],
            "files": [],
            "handlers": [],
            "repositories": [],
            "functions": functions,
            "classes": [],
            "modules": [],
            "unknown": [],
            "summary": {
                "routes": 0,
                "handlers": 0,
                "services": 0,
                "repositories": 0,
                "functions": 101,
                "classes": 0,
                "modules": 0,
                "files": 0,
                "unknown": 0
            },
            "architecture": null,
            "calls": [{ "from": "function-0", "to": "function-100" }],
            "handles": []
        }))
        .unwrap();

        let bounded = bounded_code_inventory(inventory);
        assert_eq!(bounded.functions.len(), 100);
        assert_eq!(bounded.summary.functions, 101);
        assert!(bounded.partial);
        assert!(bounded.calls.is_empty());
    }

    #[test]
    fn bounded_db_inventory_keeps_exact_engine_totals() {
        let tables = (0..101)
            .map(|index| {
                serde_json::json!({
                    "key": format!("sqlite:test:main:main:table:table_{index}"),
                    "schema": "main",
                    "name": format!("table_{index}"),
                    "columns": []
                })
            })
            .collect::<Vec<_>>();
        let inventory: DbInventory = serde_json::from_value(serde_json::json!({
            "profileId": "test",
            "tables": tables,
            "resultCount": 101,
            "totalTables": 101,
            "truncated": false
        }))
        .unwrap();

        let bounded = bounded_db_inventory(inventory);
        assert_eq!(bounded.tables.len(), 100);
        assert_eq!(bounded.result_count, Some(101));
        assert_eq!(bounded.total_tables, Some(101));
        assert_eq!(bounded.truncated, Some(true));
    }
}
