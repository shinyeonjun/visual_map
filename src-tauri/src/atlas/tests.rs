use super::*;
use crate::{
    engine::{EngineAvailability, EngineRegistry, EngineRuntimeMode},
    workspace::{
        CodeHandle, CodeInventory, CodeInventoryItem, CodeInventorySummary, DbConstraint,
        DbForeignKey, DbIndex, DbInventory, DbInventoryColumn, DbInventoryTable, Workspace,
        WorkspaceEngineCache,
    },
};
use std::{
    fs,
    time::{Duration, Instant},
};

fn temp_root(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "backend-visual-map-atlas-{name}-{}",
        std::process::id()
    ));

    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }

    root
}

fn review_lane<'a>(
    board: &'a super::model::ImpactReviewBoard,
    id: &str,
) -> &'a super::model::ImpactReviewLane {
    board.lanes.iter().find(|lane| lane.id == id).unwrap()
}

fn append_review_db_object(
    snapshot: &mut InventorySnapshot,
    name: &str,
    kind: &str,
    column_id: &str,
    unique_index: bool,
) {
    let object_kind = if kind == "index" {
        "index"
    } else {
        "constraint"
    };
    let object_id = format!("db:{object_kind}:orders:{name}");
    let mut object = item(
        &object_id,
        object_kind,
        name,
        "data",
        "db",
        Some("db:table:orders"),
        None,
    );
    object.engine_label = Some(if kind == "index" {
        "Index".to_string()
    } else {
        format!("Constraint:{kind}")
    });
    object.is_primary_key = kind == "primary_key";
    object.is_foreign_key = kind == "foreign_key";
    snapshot.items.push(object);
    snapshot.links.push(super::model::SnapshotLink {
        id: format!("contains:db:table:orders->{object_id}"),
        from: "db:table:orders".to_string(),
        to: object_id.clone(),
        kind: "contains".to_string(),
        label: Some(name.to_string()),
        truth_class: "structural".to_string(),
        direction: "outbound".to_string(),
        engine_edge_type: Some(kind.to_ascii_uppercase()),
        evidence: vec![super::model::Evidence {
            kind: if kind == "index" {
                "db-index-unique".to_string()
            } else {
                "db-constraint-kind".to_string()
            },
            text: if kind == "index" {
                unique_index.to_string()
            } else {
                kind.to_string()
            },
        }],
    });
    snapshot.links.push(super::model::SnapshotLink {
        id: format!("db-{object_kind}:{object_id}->{column_id}"),
        from: object_id,
        to: column_id.to_string(),
        kind: if kind == "index" {
            "db_index".to_string()
        } else {
            "db_constraint".to_string()
        },
        label: Some(name.to_string()),
        truth_class: "confirmed".to_string(),
        direction: "outbound".to_string(),
        engine_edge_type: Some(kind.to_ascii_uppercase()),
        evidence: vec![
            super::model::Evidence {
                kind: if kind == "index" {
                    "db-index-unique".to_string()
                } else {
                    "db-constraint-kind".to_string()
                },
                text: if kind == "index" {
                    unique_index.to_string()
                } else {
                    kind.to_string()
                },
            },
            super::model::Evidence {
                kind: "test-secret".to_string(),
                text: "password=hunter2".to_string(),
            },
        ],
    });
}

fn append_structural_review_constraint(snapshot: &mut InventorySnapshot, name: &str, kind: &str) {
    let object_id = format!("db:constraint:orders:{name}");
    let mut object = item(
        &object_id,
        "constraint",
        name,
        "data",
        "db",
        Some("db:table:orders"),
        None,
    );
    object.engine_label = Some(format!("Constraint:{kind}"));
    snapshot.items.push(object);
    snapshot.links.push(super::model::SnapshotLink {
        id: format!("contains:db:table:orders->{object_id}"),
        from: "db:table:orders".to_string(),
        to: object_id,
        kind: "contains".to_string(),
        label: Some(name.to_string()),
        truth_class: "structural".to_string(),
        direction: "outbound".to_string(),
        engine_edge_type: Some(kind.to_ascii_uppercase()),
        evidence: vec![super::model::Evidence {
            kind: "db-constraint-kind".to_string(),
            text: kind.to_string(),
        }],
    });
}

#[test]
fn inventory_snapshot_serializes_as_camel_case() {
    let snapshot = fixture_inventory("workspace-1".to_string());
    let json = serde_json::to_string(&snapshot).unwrap();

    assert!(json.contains("workspaceId"));
    assert!(json.contains("\"schemaVersion\":2"));
    assert!(json.contains("parentId"));
    assert!(!json.contains("raw"));
}

#[test]
fn canonical_builder_preserves_handler_location_and_normalizes_handles() {
    let route = code_test_item(
        "shop.routes.create_order",
        "Route",
        "POST /orders",
        "src/routes/orders.rs",
        12,
        12,
    );
    let handler = code_test_item(
        "shop.handlers.create_order",
        "Function",
        "create_order",
        "src/handlers/orders.rs",
        20,
        46,
    );
    let code = CodeInventory {
        project: "shop".to_string(),
        routes: vec![route],
        services: Vec::new(),
        files: Vec::new(),
        handlers: vec![handler],
        repositories: Vec::new(),
        functions: Vec::new(),
        classes: Vec::new(),
        modules: Vec::new(),
        unknown: Vec::new(),
        summary: CodeInventorySummary {
            routes: 1,
            handlers: 1,
            services: 0,
            repositories: 0,
            functions: 0,
            classes: 0,
            modules: 0,
            files: 0,
            unknown: 0,
        },
        architecture: Some(serde_json::json!({ "modules": ["orders"] })),
        calls: Vec::new(),
        handles: vec![CodeHandle {
            handler: "shop.handlers.create_order".to_string(),
            route: "shop.routes.create_order".to_string(),
        }],
    };
    let code_json = serde_json::to_value(code).unwrap();
    assert_eq!(code_json["handlers"][0]["column"], 3);
    assert_eq!(code_json["handlers"][0]["endColumn"], 18);
    let code: CodeInventory = serde_json::from_value(code_json).unwrap();

    let snapshot = build_inventory_snapshot("workspace-1".to_string(), Some(&code), None);
    let handler = snapshot
        .items
        .iter()
        .find(|entry| entry.id == "code:shop.handlers.create_order")
        .unwrap();
    let handles = snapshot
        .links
        .iter()
        .find(|link| link.kind == "code_handle")
        .unwrap();

    assert_eq!(
        snapshot.schema_version,
        super::model::SNAPSHOT_SCHEMA_VERSION
    );
    assert_eq!(handler.kind, "handler");
    assert_eq!(handler.engine_label.as_deref(), Some("Function"));
    assert_eq!(handler.project_id.as_deref(), Some("shop"));
    assert_eq!(handler.group_id.as_deref(), Some("orders"));
    assert_eq!(handler.location.as_ref().unwrap().line, Some(20));
    assert_eq!(handler.location.as_ref().unwrap().column, Some(3));
    assert_eq!(handler.location.as_ref().unwrap().end_line, Some(46));
    assert_eq!(handler.location.as_ref().unwrap().end_column, Some(18));
    assert_eq!(handles.from, "code:shop.routes.create_order");
    assert_eq!(handles.to, "code:shop.handlers.create_order");
    assert_eq!(handles.truth_class, "confirmed");
    assert_eq!(handles.direction, "outbound");
    assert_eq!(handles.engine_edge_type.as_deref(), Some("HANDLES"));
    assert!(handles
        .evidence
        .iter()
        .any(|evidence| evidence.text.contains("handler→route")));
    assert_eq!(
        snapshot.metadata.architecture,
        Some(serde_json::json!({ "modules": ["orders"] }))
    );

    let root = temp_root("snapshot-v2-round-trip");
    save_inventory_snapshot(&root, &snapshot).unwrap();
    assert_eq!(
        load_inventory_snapshot(&root, "workspace-1").unwrap(),
        snapshot
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn snapshot_with_metadata_records_code_source() {
    let snapshot = InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: "workspace-1".to_string(),
        saved_at: "1".to_string(),
        metadata: Default::default(),
        stale_reasons: Vec::new(),
        links: Vec::new(),
        items: vec![item(
            "code:file:main",
            "file",
            "main.rs",
            "code",
            "code",
            None,
            Some("src/main.rs"),
        )],
    };
    let snapshot = snapshot_with_metadata(
        snapshot,
        &test_workspace(r"D:\repo\shop-api"),
        &test_registry(),
    );

    let code = snapshot.metadata.code.unwrap();
    assert_eq!(code.source_path.as_deref(), Some(r"D:\repo\shop-api"));
    assert_eq!(code.source_type, "local-folder");
    assert_eq!(code.engine_version.as_deref(), Some("0.8.1"));
    assert!(snapshot.stale_reasons.is_empty());
}

#[test]
fn empty_inventories_keep_source_provenance() {
    let code = CodeInventory {
        project: "empty".to_string(),
        routes: Vec::new(),
        services: Vec::new(),
        files: Vec::new(),
        handlers: Vec::new(),
        repositories: Vec::new(),
        functions: Vec::new(),
        classes: Vec::new(),
        modules: Vec::new(),
        unknown: Vec::new(),
        summary: CodeInventorySummary {
            routes: 0,
            handlers: 0,
            services: 0,
            repositories: 0,
            functions: 0,
            classes: 0,
            modules: 0,
            files: 0,
            unknown: 0,
        },
        architecture: None,
        calls: Vec::new(),
        handles: Vec::new(),
    };
    let db = DbInventory {
        profile_id: "profile-1".to_string(),
        tables: Vec::new(),
        snapshot_key: Some("ddl-sqlite:profile-1".to_string()),
        contract_version: Some("1".to_string()),
        capability_warnings: Vec::new(),
        limit_requested: None,
        limit_applied: None,
        limit_clamped: None,
        result_count: None,
        total_tables: None,
        truncated: None,
        gaps: Vec::new(),
    };

    let snapshot = build_inventory_snapshot("workspace-1".to_string(), Some(&code), Some(&db));
    let snapshot = snapshot_with_metadata(
        snapshot,
        &test_workspace(r"D:\repo\empty"),
        &test_registry(),
    );

    assert!(snapshot.items.is_empty());
    assert!(snapshot.metadata.code.is_some());
    assert_eq!(
        snapshot
            .metadata
            .db
            .as_ref()
            .and_then(|metadata| metadata.snapshot_key.as_deref()),
        Some("ddl-sqlite:profile-1")
    );
}

#[test]
fn engine_checksum_change_marks_snapshot_stale() {
    let snapshot = snapshot_with_metadata(
        InventorySnapshot {
            schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
            workspace_id: "workspace-1".to_string(),
            saved_at: "1".to_string(),
            metadata: Default::default(),
            stale_reasons: Vec::new(),
            links: Vec::new(),
            items: vec![item(
                "code:file:main",
                "file",
                "main.rs",
                "code",
                "code",
                None,
                Some("src/main.rs"),
            )],
        },
        &test_workspace(r"D:\repo\shop-api"),
        &test_registry(),
    );
    let mut changed_registry = test_registry();
    changed_registry.engines[0].sha256 = Some("changed".to_string());

    let stale = mark_snapshot_staleness(
        snapshot,
        &test_workspace(r"D:\repo\shop-api"),
        &changed_registry,
    );

    assert!(stale
        .stale_reasons
        .iter()
        .any(|reason| reason.contains("checksum")));
}

#[test]
fn snapshot_marks_code_stale_when_repo_path_changes() {
    let snapshot = InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: "workspace-1".to_string(),
        saved_at: "1".to_string(),
        metadata: Default::default(),
        stale_reasons: Vec::new(),
        links: Vec::new(),
        items: vec![item(
            "code:file:main",
            "file",
            "main.rs",
            "code",
            "code",
            None,
            Some("src/main.rs"),
        )],
    };
    let snapshot =
        snapshot_with_metadata(snapshot, &test_workspace(r"D:\repo\old"), &test_registry());
    let stale =
        mark_snapshot_staleness(snapshot, &test_workspace(r"D:\repo\new"), &test_registry());

    assert_eq!(
        stale.stale_reasons,
        vec!["코드 프로젝트 경로가 바뀌었습니다"]
    );
}

#[test]
fn load_migrates_v1_and_drops_dangling_relationships() {
    let root = temp_root("snapshot-v1-migration");
    write_snapshot_json(
        &root,
        serde_json::json!({
            "workspaceId": "workspace-1",
            "savedAt": "1",
            "metadata": {},
            "staleReasons": [],
            "items": [
                {
                    "id": "code:route:create-order",
                    "kind": "api",
                    "name": "POST /orders",
                    "layer": "api",
                    "source": "code",
                    "path": "src/routes/orders.rs"
                },
                {
                    "id": "code:handler:create-order",
                    "kind": "handler",
                    "name": "create_order",
                    "layer": "code",
                    "source": "code",
                    "path": "src/handlers/orders.rs"
                },
                {
                    "id": "code:handler:create-order",
                    "kind": "handler",
                    "name": "create_order",
                    "layer": "code",
                    "source": "code",
                    "path": "src/handlers/orders.rs"
                }
            ],
            "links": [
                {
                    "id": "code-call:route->handler",
                    "from": "code:route:create-order",
                    "to": "code:handler:create-order",
                    "kind": "code_call",
                    "label": "CALLS"
                },
                {
                    "id": "code-call:handler->missing",
                    "from": "code:handler:create-order",
                    "to": "code:missing",
                    "kind": "code_call",
                    "label": "CALLS"
                }
            ]
        }),
    );

    let snapshot = load_inventory_snapshot(&root, "workspace-1").unwrap();

    assert_eq!(
        snapshot.schema_version,
        super::model::SNAPSHOT_SCHEMA_VERSION
    );
    assert_eq!(snapshot.metadata.migration.source_schema_version, Some(1));
    assert!(snapshot.metadata.migration.reindex_required);
    assert_eq!(snapshot.items.len(), 2);
    assert_eq!(snapshot.links.len(), 1);
    assert_eq!(snapshot.links[0].truth_class, "confirmed");
    assert_eq!(snapshot.links[0].engine_edge_type.as_deref(), Some("CALLS"));
    assert!(snapshot
        .metadata
        .gaps
        .iter()
        .any(|gap| gap.kind == "dangling-relationship"));
    assert!(snapshot.items.iter().all(|entry| entry.location.is_some()));
    assert!(snapshot
        .stale_reasons
        .iter()
        .any(|reason| reason.contains("다시 읽어야")));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn db_only_v1_keeps_safe_inventory_without_forcing_reindex() {
    let root = temp_root("snapshot-v1-db-only");
    write_snapshot_json(
        &root,
        serde_json::json!({
            "workspaceId": "workspace-1",
            "savedAt": "1",
            "metadata": {},
            "staleReasons": [],
            "items": [{
                "id": "db:table:orders",
                "kind": "table",
                "name": "orders",
                "layer": "data",
                "source": "db"
            }],
            "links": []
        }),
    );

    let snapshot = load_inventory_snapshot(&root, "workspace-1").unwrap();

    assert_eq!(snapshot.metadata.migration.source_schema_version, Some(1));
    assert!(!snapshot.metadata.migration.reindex_required);
    assert_eq!(snapshot.items.len(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn conflicting_duplicate_ids_require_reindex() {
    let root = temp_root("snapshot-duplicate-conflict");
    write_snapshot_json(
        &root,
        serde_json::json!({
            "workspaceId": "workspace-1",
            "savedAt": "1",
            "items": [
                {
                    "id": "code:same",
                    "kind": "file",
                    "name": "same.rs",
                    "layer": "code",
                    "source": "code"
                },
                {
                    "id": "code:same",
                    "kind": "function",
                    "name": "same",
                    "layer": "code",
                    "source": "code"
                }
            ]
        }),
    );

    let snapshot = load_inventory_snapshot(&root, "workspace-1").unwrap();

    assert_eq!(snapshot.items.len(), 1);
    assert!(snapshot.metadata.migration.reindex_required);
    assert!(snapshot
        .stale_reasons
        .iter()
        .any(|reason| reason.contains("다시")));
    assert!(snapshot
        .metadata
        .gaps
        .iter()
        .any(|gap| gap.kind == "node-conflict"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unsupported_snapshot_version_is_not_interpreted_as_v2() {
    let root = temp_root("snapshot-future-version");
    write_snapshot_json(
        &root,
        serde_json::json!({
            "schemaVersion": 99,
            "workspaceId": "workspace-1",
            "savedAt": "1",
            "items": [{
                "id": "code:future",
                "kind": "api",
                "name": "must-not-load",
                "layer": "api",
                "source": "code"
            }]
        }),
    );

    let snapshot = load_inventory_snapshot(&root, "workspace-1").unwrap();

    assert!(snapshot.items.is_empty());
    assert!(snapshot.links.is_empty());
    assert!(snapshot.metadata.migration.reindex_required);
    assert_eq!(snapshot.metadata.migration.source_schema_version, Some(99));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn atomic_save_retains_backup_and_loader_recovers_it() {
    let root = temp_root("snapshot-atomic-backup");
    let first = fixture_inventory("workspace-1".to_string());
    save_inventory_snapshot(&root, &first).unwrap();

    let mut second = first.clone();
    second.items.push(item(
        "code:file:added",
        "file",
        "added.rs",
        "code",
        "code",
        None,
        Some("src/added.rs"),
    ));
    save_inventory_snapshot(&root, &second).unwrap();

    let primary = snapshot_path(&root, "workspace-1");
    let backup = snapshot_backup_path(&primary);
    let primary_snapshot: InventorySnapshot =
        serde_json::from_str(&fs::read_to_string(&primary).unwrap()).unwrap();
    let backup_snapshot: InventorySnapshot =
        serde_json::from_str(&fs::read_to_string(&backup).unwrap()).unwrap();
    assert!(primary_snapshot
        .items
        .iter()
        .any(|entry| entry.id == "code:file:added"));
    assert!(!backup_snapshot
        .items
        .iter()
        .any(|entry| entry.id == "code:file:added"));

    fs::write(&primary, "{broken").unwrap();
    let recovered = load_inventory_snapshot(&root, "workspace-1").unwrap();
    assert!(recovered.metadata.migration.reindex_required);
    assert!(!recovered
        .items
        .iter()
        .any(|entry| entry.id == "code:file:added"));

    let mut partial = second.clone();
    partial.items.retain(|entry| entry.source == "code");
    partial
        .links
        .retain(|link| link.kind == "code_call" || link.kind == "code_handle");
    partial.metadata.db = None;
    save_inventory_snapshot(&root, &partial).unwrap();

    let saved_partial: InventorySnapshot =
        serde_json::from_str(&fs::read_to_string(&primary).unwrap()).unwrap();
    let preserved_backup: InventorySnapshot =
        serde_json::from_str(&fs::read_to_string(&backup).unwrap()).unwrap();
    assert!(saved_partial
        .items
        .iter()
        .all(|entry| entry.source == "code"));
    assert!(preserved_backup
        .items
        .iter()
        .any(|entry| entry.source == "code"));
    assert!(preserved_backup
        .items
        .iter()
        .any(|entry| entry.id == "db:table:orders"));
    assert!(preserved_backup
        .links
        .iter()
        .any(|link| link.kind == "db_fk"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cached_snapshot_reuses_unchanged_file_and_invalidates_after_save() {
    let root = temp_root("snapshot-cache");
    let first = fixture_inventory("workspace-1".to_string());
    save_inventory_snapshot(&root, &first).unwrap();

    let cached_first = load_inventory_snapshot_cached(&root, "workspace-1").unwrap();
    let cached_again = load_inventory_snapshot_cached(&root, "workspace-1").unwrap();
    assert!(std::sync::Arc::ptr_eq(&cached_first, &cached_again));

    let mut second = first.clone();
    second.items.push(item(
        "code:file:cache-refresh",
        "file",
        "cache-refresh.rs",
        "code",
        "code",
        None,
        Some("src/cache-refresh.rs"),
    ));
    save_inventory_snapshot(&root, &second).unwrap();

    let refreshed = load_inventory_snapshot_cached(&root, "workspace-1").unwrap();
    assert!(!std::sync::Arc::ptr_eq(&cached_first, &refreshed));
    assert!(refreshed
        .items
        .iter()
        .any(|item| item.id == "code:file:cache-refresh"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn candidate_linker_matches_code_names_to_tables() {
    let snapshot = fixture_inventory("workspace-1".to_string());
    let links = candidate_links(&snapshot);

    assert!(links.iter().any(|link| link.to == "db:table:orders"));
    assert!(links.iter().all(|link| !link.evidence.is_empty()));
}

#[test]
fn normalize_inventory_distinguishes_same_table_names_by_schema() {
    let db = DbInventory {
        profile_id: "profile-1".to_string(),
        snapshot_key: None,
        contract_version: None,
        capability_warnings: Vec::new(),
        limit_requested: None,
        limit_applied: None,
        limit_clamped: None,
        result_count: None,
        total_tables: None,
        truncated: None,
        gaps: Vec::new(),
        tables: vec![
            crate::workspace::DbInventoryTable {
                key: None,
                database: None,
                schema: Some("public".to_string()),
                name: "users".to_string(),
                columns: vec![crate::workspace::DbInventoryColumn {
                    key: None,
                    table_key: None,
                    name: "id".to_string(),
                    data_type: Some("bigint".to_string()),
                    nullable: Some(false),
                    is_primary_key: true,
                    is_foreign_key: false,
                }],
                foreign_keys: Vec::new(),
                inbound_foreign_keys: Vec::new(),
                constraints: Vec::new(),
                indexes: Vec::new(),
            },
            crate::workspace::DbInventoryTable {
                key: None,
                database: None,
                schema: Some("audit".to_string()),
                name: "users".to_string(),
                columns: Vec::new(),
                foreign_keys: Vec::new(),
                inbound_foreign_keys: Vec::new(),
                constraints: Vec::new(),
                indexes: Vec::new(),
            },
        ],
    };
    let snapshot = normalize_inventory("workspace-1".to_string(), None, Some(&db));
    let table_ids: Vec<&str> = snapshot
        .items
        .iter()
        .filter(|item| item.kind == "table")
        .map(|item| item.id.as_str())
        .collect();

    assert_eq!(table_ids.len(), 2);
    assert!(table_ids.contains(&"db:table:public.users"));
    assert!(table_ids.contains(&"db:table:audit.users"));
    assert!(snapshot.items.iter().any(|item| {
        item.id == "db:column:public.users:id" && item.is_primary_key && !item.is_foreign_key
    }));
}

#[test]
fn normalize_inventory_preserves_confirmed_db_foreign_key_links() {
    let mut db = DbInventory {
        profile_id: "profile-1".to_string(),
        snapshot_key: None,
        contract_version: None,
        capability_warnings: Vec::new(),
        limit_requested: None,
        limit_applied: None,
        limit_clamped: None,
        result_count: None,
        total_tables: None,
        truncated: None,
        gaps: Vec::new(),
        tables: vec![
            crate::workspace::DbInventoryTable {
                key: None,
                database: None,
                schema: Some("public".to_string()),
                name: "orders".to_string(),
                columns: vec![crate::workspace::DbInventoryColumn {
                    key: None,
                    table_key: None,
                    name: "account_id".to_string(),
                    data_type: Some("bigint".to_string()),
                    nullable: Some(false),
                    is_primary_key: false,
                    is_foreign_key: true,
                }],
                foreign_keys: vec![crate::workspace::DbForeignKey {
                    key: None,
                    name: Some("orders_account_id_fkey".to_string()),
                    table_key: None,
                    table_schema: Some("public".to_string()),
                    table: Some("orders".to_string()),
                    columns: vec!["account_id".to_string()],
                    column_keys: Vec::new(),
                    referenced_table_key: None,
                    referenced_schema: Some("public".to_string()),
                    referenced_table: "accounts".to_string(),
                    referenced_columns: vec!["id".to_string()],
                    referenced_column_keys: Vec::new(),
                }],
                inbound_foreign_keys: Vec::new(),
                constraints: Vec::new(),
                indexes: Vec::new(),
            },
            crate::workspace::DbInventoryTable {
                key: None,
                database: None,
                schema: Some("public".to_string()),
                name: "accounts".to_string(),
                columns: vec![crate::workspace::DbInventoryColumn {
                    key: None,
                    table_key: None,
                    name: "id".to_string(),
                    data_type: Some("bigint".to_string()),
                    nullable: Some(false),
                    is_primary_key: true,
                    is_foreign_key: false,
                }],
                foreign_keys: Vec::new(),
                inbound_foreign_keys: Vec::new(),
                constraints: Vec::new(),
                indexes: Vec::new(),
            },
        ],
    };
    let inbound_duplicate = db.tables[0].foreign_keys[0].clone();
    db.tables[1].inbound_foreign_keys.push(inbound_duplicate);
    db.tables.reverse();

    let snapshot = normalize_inventory("workspace-1".to_string(), None, Some(&db));

    assert!(snapshot.items.iter().any(|item| {
        item.id == "db:column:public.orders:account_id" && item.nullable == Some(false)
    }));
    let foreign_keys = snapshot
        .links
        .iter()
        .filter(|link| link.kind == "db_fk")
        .collect::<Vec<_>>();
    assert_eq!(foreign_keys.len(), 1);
    assert_eq!(foreign_keys[0].from, "db:column:public.orders:account_id");
    assert_eq!(foreign_keys[0].to, "db:column:public.accounts:id");
    assert_eq!(
        foreign_keys[0].label.as_deref(),
        Some("orders_account_id_fkey")
    );
    assert!(foreign_keys[0]
        .evidence
        .iter()
        .any(|evidence| evidence.kind == "db-fk-direction" && evidence.text == "outbound"));
}

#[test]
fn db_contract_facts_round_trip_as_nodes_relationships_evidence_and_gaps() {
    let table_key = |table: &str| format!("postgres:shop:shop:public:table:{table}");
    let column_key =
        |table: &str, column: &str| format!("postgres:shop:shop:public:column:{table}:{column}");
    let fk_key = "postgres:shop:shop:public:foreign_key:orders:orders_customer_id_fkey";
    let db = DbInventory {
        profile_id: "profile-1".to_string(),
        snapshot_key: Some("postgres:shop".to_string()),
        contract_version: Some("1".to_string()),
        capability_warnings: vec!["trigger metadata unsupported".to_string()],
        limit_requested: Some(1_000),
        limit_applied: Some(1_000),
        limit_clamped: Some(false),
        result_count: Some(2),
        total_tables: Some(2),
        truncated: Some(false),
        gaps: Vec::new(),
        tables: vec![
            DbInventoryTable {
                key: Some(table_key("orders")),
                database: Some("shop".to_string()),
                schema: Some("public".to_string()),
                name: "orders".to_string(),
                columns: ["id", "customer_id", "external_id", "amount"]
                    .into_iter()
                    .map(|name| DbInventoryColumn {
                        key: Some(column_key("orders", name)),
                        table_key: Some(table_key("orders")),
                        name: name.to_string(),
                        data_type: Some("bigint".to_string()),
                        nullable: Some(false),
                        is_primary_key: name == "id",
                        is_foreign_key: name == "customer_id",
                    })
                    .collect(),
                foreign_keys: vec![DbForeignKey {
                    key: Some(fk_key.to_string()),
                    name: Some("orders_customer_id_fkey".to_string()),
                    table_key: Some(table_key("orders")),
                    table_schema: Some("public".to_string()),
                    table: Some("orders".to_string()),
                    columns: vec!["customer_id".to_string()],
                    column_keys: vec![column_key("orders", "customer_id")],
                    referenced_table_key: Some(table_key("customers")),
                    referenced_schema: Some("public".to_string()),
                    referenced_table: "customers".to_string(),
                    referenced_columns: vec!["id".to_string()],
                    referenced_column_keys: vec![column_key("customers", "id")],
                }],
                inbound_foreign_keys: Vec::new(),
                constraints: vec![
                    DbConstraint {
                        key: Some(
                            "postgres:shop:shop:public:primary_key:orders:orders_pkey"
                                .to_string(),
                        ),
                        name: Some("orders_pkey".to_string()),
                        kind: "primary_key".to_string(),
                        columns: vec!["id".to_string()],
                        column_keys: vec![column_key("orders", "id")],
                        referenced_table_key: None,
                        referenced_schema: None,
                        referenced_table: None,
                        referenced_columns: Vec::new(),
                        referenced_column_keys: Vec::new(),
                        expression: None,
                        source: "constraints".to_string(),
                    },
                    DbConstraint {
                        key: Some(fk_key.to_string()),
                        name: Some("orders_customer_id_fkey".to_string()),
                        kind: "foreign_key".to_string(),
                        columns: vec!["customer_id".to_string()],
                        column_keys: vec![column_key("orders", "customer_id")],
                        referenced_table_key: Some(table_key("customers")),
                        referenced_schema: Some("public".to_string()),
                        referenced_table: Some("customers".to_string()),
                        referenced_columns: vec!["id".to_string()],
                        referenced_column_keys: vec![column_key("customers", "id")],
                        expression: None,
                        source: "constraints".to_string(),
                    },
                    DbConstraint {
                        key: Some(
                            "postgres:shop:shop:public:unique_constraint:orders:orders_external_key"
                                .to_string(),
                        ),
                        name: Some("orders_external_key".to_string()),
                        kind: "unique".to_string(),
                        columns: vec!["external_id".to_string()],
                        column_keys: vec![column_key("orders", "external_id")],
                        referenced_table_key: None,
                        referenced_schema: None,
                        referenced_table: None,
                        referenced_columns: Vec::new(),
                        referenced_column_keys: Vec::new(),
                        expression: None,
                        source: "constraints".to_string(),
                    },
                    DbConstraint {
                        key: Some(
                            "postgres:shop:shop:public:check_constraint:orders:orders_amount_check"
                                .to_string(),
                        ),
                        name: Some("orders_amount_check".to_string()),
                        kind: "check".to_string(),
                        columns: vec!["amount".to_string()],
                        column_keys: vec![column_key("orders", "amount")],
                        referenced_table_key: None,
                        referenced_schema: None,
                        referenced_table: None,
                        referenced_columns: Vec::new(),
                        referenced_column_keys: Vec::new(),
                        expression: Some("amount >= 0".to_string()),
                        source: "constraints".to_string(),
                    },
                ],
                indexes: vec![DbIndex {
                    key: Some(
                        "postgres:shop:shop:public:index:orders:orders_external_idx".to_string(),
                    ),
                    name: "orders_external_idx".to_string(),
                    columns: vec!["external_id".to_string()],
                    column_keys: vec![column_key("orders", "external_id")],
                    unique: true,
                    primary: false,
                    predicate: Some("external_id IS NOT NULL".to_string()),
                    expression: Some("lower(external_id)".to_string()),
                }],
            },
            DbInventoryTable {
                key: Some(table_key("customers")),
                database: Some("shop".to_string()),
                schema: Some("public".to_string()),
                name: "customers".to_string(),
                columns: vec![DbInventoryColumn {
                    key: Some(column_key("customers", "id")),
                    table_key: Some(table_key("customers")),
                    name: "id".to_string(),
                    data_type: Some("bigint".to_string()),
                    nullable: Some(false),
                    is_primary_key: true,
                    is_foreign_key: false,
                }],
                foreign_keys: Vec::new(),
                inbound_foreign_keys: Vec::new(),
                constraints: Vec::new(),
                indexes: Vec::new(),
            },
        ],
    };

    let snapshot = normalize_inventory("workspace-1".to_string(), None, Some(&db));
    let node_ids = snapshot
        .items
        .iter()
        .map(|item| item.id.as_str())
        .collect::<std::collections::HashSet<_>>();

    assert!(!snapshot.metadata.migration.reindex_required);
    assert_eq!(node_ids.len(), snapshot.items.len());
    assert_eq!(
        snapshot
            .metadata
            .db
            .as_ref()
            .and_then(|metadata| metadata.snapshot_key.as_deref()),
        Some("postgres:shop")
    );
    assert_eq!(
        snapshot
            .metadata
            .db
            .as_ref()
            .and_then(|metadata| metadata.total_tables),
        Some(2)
    );
    assert_eq!(
        snapshot
            .metadata
            .db
            .as_ref()
            .and_then(|metadata| metadata.truncated),
        Some(false)
    );
    assert!(snapshot
        .metadata
        .gaps
        .iter()
        .any(|gap| gap.kind == "db-capability"));
    assert_eq!(
        snapshot
            .items
            .iter()
            .filter(|item| item.kind == "constraint")
            .count(),
        4
    );
    assert_eq!(
        snapshot
            .items
            .iter()
            .filter(|item| item.kind == "index")
            .count(),
        1
    );
    assert_eq!(
        snapshot
            .items
            .iter()
            .find(|item| item.id == "db:column:public.orders:customer_id")
            .and_then(|item| item.qualified_name.as_deref()),
        Some("postgres:shop:shop:public:column:orders:customer_id")
    );
    assert!(snapshot.links.iter().any(|link| {
        link.kind == "contains"
            && link.evidence.iter().any(|evidence| {
                evidence.kind == "db-index-predicate" && evidence.text == "external_id IS NOT NULL"
            })
            && link.evidence.iter().any(|evidence| {
                evidence.kind == "db-index-expression" && evidence.text == "lower(external_id)"
            })
    }));
    assert!(snapshot.links.iter().any(|link| {
        link.kind == "db_fk"
            && link.from == "db:column:public.orders:customer_id"
            && link.to == "db:column:public.customers:id"
            && link.truth_class == "confirmed"
            && link
                .evidence
                .iter()
                .any(|evidence| evidence.kind == "db-fk-direction" && evidence.text == "outbound")
            && link.evidence.iter().any(|evidence| {
                evidence.kind == "db-column-key"
                    && evidence.text == "postgres:shop:shop:public:column:orders:customer_id"
            })
            && link.evidence.iter().any(|evidence| {
                evidence.kind == "db-referenced-column-key"
                    && evidence.text == "postgres:shop:shop:public:column:customers:id"
            })
    }));
    assert!(snapshot
        .links
        .iter()
        .filter(|link| matches!(link.kind.as_str(), "db_constraint" | "db_index"))
        .all(|link| link.truth_class == "confirmed"));
    assert!(snapshot
        .links
        .iter()
        .filter(|link| link.kind == "contains")
        .all(|link| link.truth_class == "structural"));
    assert!(snapshot
        .links
        .iter()
        .all(|link| node_ids.contains(link.from.as_str()) && node_ids.contains(link.to.as_str())));

    let json = serde_json::to_string(&snapshot).unwrap();
    let restored: InventorySnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, snapshot);
}

#[test]
fn visual_map_returns_focus_neighborhoods() {
    let snapshot = fixture_inventory("workspace-1".to_string());
    let api_map = visual_map(
        &snapshot,
        Some("code:route:orders:create".to_string()),
        "api-flow".to_string(),
    );
    let table_map = visual_map(
        &snapshot,
        Some("db:table:orders".to_string()),
        "table-usage".to_string(),
    );
    let column_map = visual_map(
        &snapshot,
        Some("db:column:orders:customer_id".to_string()),
        "column-impact".to_string(),
    );

    assert_eq!(api_map.focus, "code:route:orders:create");
    assert_eq!(table_map.focus, "db:table:orders");
    assert_eq!(column_map.focus, "db:column:orders:customer_id");
    assert!(api_map
        .nodes
        .iter()
        .any(|node| node.id == "code:class:OrderService"));
    assert!(api_map
        .edges
        .iter()
        .any(|edge| edge.kind == "code_handle" && edge.from == "code:route:orders:create"));
    assert!(api_map
        .nodes
        .iter()
        .any(|node| node.id == "db:table:orders"));
    assert!(api_map.edges.iter().any(|edge| edge.kind == "code_call"
        && edge
            .evidence
            .iter()
            .any(|evidence| evidence.kind == "code-call")));
    assert!(!api_map.edges.iter().any(|edge| edge.kind == "code_flow"));
    assert!(api_map
        .edges
        .iter()
        .any(|edge| edge.kind == "candidate_uses" && edge.confidence.as_deref() == Some("high")));
    assert!(table_map
        .nodes
        .iter()
        .any(|node| node.id == "db:column:orders:id"));
    assert!(table_map
        .nodes
        .iter()
        .any(|node| node.id == "code:class:OrderService"));
    assert!(table_map.edges.iter().any(|edge| edge.kind == "contains"));
    assert!(table_map
        .edges
        .iter()
        .any(|edge| edge.kind == "candidate_uses"
            && edge.confidence.as_deref() == Some("high")
            && !edge.evidence.is_empty()));
    assert!(table_map.edges.iter().all(|edge| edge
        .confidence
        .as_deref()
        .map(|confidence| matches!(confidence, "high" | "medium" | "low"))
        .unwrap_or(true)));
    assert!(column_map
        .nodes
        .iter()
        .any(|node| node.id == "db:constraint:db:column:orders:customer_id:fk"));
    assert!(column_map
        .edges
        .iter()
        .any(|edge| edge.kind == "db_constraint"));
    assert!(column_map.edges.iter().any(|edge| edge.kind == "db_fk"
        && edge
            .evidence
            .iter()
            .any(|evidence| evidence.text.contains("orders_customer_id_fkey"))));
    assert!(column_map
        .edges
        .iter()
        .any(|edge| edge.kind == "candidate_column_ref"
            && edge.confidence.as_deref() == Some("medium")
            && !edge.evidence.is_empty()));
}

#[test]
fn column_text_evidence_precedes_and_merges_static_candidates() {
    let mut snapshot = fixture_inventory("workspace-1".to_string());
    let repository = snapshot
        .items
        .iter_mut()
        .find(|item| item.id == "code:class:OrderRepository")
        .unwrap();
    repository.name = "CustomerIdRepository".to_string();
    repository.location = Some(super::model::SourceLocation {
        path: "src/orders/order_repository.ts".to_string(),
        line: Some(44),
        column: None,
        end_line: Some(44),
        end_column: None,
    });
    snapshot.links.push(super::model::SnapshotLink {
        id: "code_db_column_text_reference:repository->customer-id".to_string(),
        from: repository.id.clone(),
        to: "db:column:orders:customer_id".to_string(),
        kind: "code_db_column_text_reference".to_string(),
        label: Some("search_code exact token".to_string()),
        truth_class: "candidate".to_string(),
        direction: "outbound".to_string(),
        engine_edge_type: Some("SEARCH_CODE_EXACT_TOKEN".to_string()),
        evidence: vec![super::model::Evidence {
            kind: "code-search-exact-token".to_string(),
            text: "src/orders/order_repository.ts:L44 위치에서 exact token 일치".to_string(),
        }],
    });
    snapshot.links.push(super::model::SnapshotLink {
        id: "code_db_column_text_reference:repository->id".to_string(),
        from: repository.id.clone(),
        to: "db:column:orders:id".to_string(),
        kind: "code_db_column_text_reference".to_string(),
        label: Some("search_code exact token".to_string()),
        truth_class: "candidate".to_string(),
        direction: "outbound".to_string(),
        engine_edge_type: Some("SEARCH_CODE_EXACT_TOKEN".to_string()),
        evidence: vec![super::model::Evidence {
            kind: "code-search-exact-token".to_string(),
            text: "src/orders/order_repository.ts:L45 위치에서 id exact token 일치".to_string(),
        }],
    });
    snapshot.metadata.gaps.push(super::model::SnapshotGap {
        id: "gap:code-search-partial:customer-id".to_string(),
        kind: "code-search-partial".to_string(),
        message: "검색 결과 상한에 도달했습니다.".to_string(),
        related_ids: vec!["db:column:orders:customer_id".to_string()],
    });

    let map = visual_map(
        &snapshot,
        Some("db:column:orders:customer_id".to_string()),
        "column-impact".to_string(),
    );
    let edges = map
        .edges
        .iter()
        .filter(|edge| {
            edge.kind == "candidate_column_ref"
                && edge.from == "code:class:OrderRepository"
                && edge.to == "db:column:orders:customer_id"
        })
        .collect::<Vec<_>>();

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].confidence.as_deref(), Some("high"));
    assert!(edges[0]
        .evidence
        .iter()
        .any(|entry| entry.kind == "code-search-exact-token"));
    assert!(edges[0]
        .evidence
        .iter()
        .any(|entry| entry.kind == "column-name-match"));
    assert!(review_lane(map.review_board.as_ref().unwrap(), "unknowns")
        .items
        .iter()
        .any(|item| item.kind == "code-search-partial"));

    let id_map = visual_map(
        &snapshot,
        Some("db:column:orders:id".to_string()),
        "column-impact".to_string(),
    );
    assert!(id_map.edges.iter().any(|edge| {
        edge.kind == "candidate_column_ref"
            && edge.to == "db:column:orders:id"
            && edge.confidence.as_deref() == Some("high")
    }));
}

#[test]
fn compound_column_candidates_require_the_full_identifier() {
    let mut snapshot = fixture_inventory("workspace-1".to_string());
    let column = snapshot
        .items
        .iter_mut()
        .find(|item| item.id == "db:column:orders:customer_id")
        .unwrap();
    column.name = "order_id".to_string();
    snapshot.items.push(item(
        "code:function:ordered-line",
        "function",
        "_is_ordered_line",
        "code",
        "code",
        None,
        Some("src/report/ordered_line.py"),
    ));
    snapshot.items.push(item(
        "code:function:record-error",
        "function",
        "record_error",
        "code",
        "code",
        None,
        Some("src/runtime/record_error.py"),
    ));
    snapshot.items.push(item(
        "code:function:load-order-id",
        "function",
        "loadOrderId",
        "code",
        "code",
        None,
        Some("src/orders/load_order_id.ts"),
    ));

    let map = visual_map(
        &snapshot,
        Some("db:column:orders:customer_id".to_string()),
        "column-impact".to_string(),
    );
    let candidate_sources = map
        .edges
        .iter()
        .filter(|edge| edge.kind == "candidate_column_ref")
        .map(|edge| edge.from.as_str())
        .collect::<std::collections::HashSet<_>>();

    assert!(candidate_sources.contains("code:function:load-order-id"));
    assert!(!candidate_sources.contains("code:function:ordered-line"));
    assert!(!candidate_sources.contains("code:function:record-error"));
}

#[test]
fn change_impact_review_board_separates_truth_candidates_unknowns_and_checks() {
    let mut snapshot = fixture_inventory("workspace-1".to_string());
    snapshot.metadata.gaps.push(super::model::SnapshotGap {
        id: "gap:db-capability:index-expression".to_string(),
        kind: "db-capability".to_string(),
        message: "expression index metadata unsupported".to_string(),
        related_ids: Vec::new(),
    });
    if let Some(repository) = snapshot
        .items
        .iter_mut()
        .find(|item| item.id == "code:class:OrderRepository")
    {
        repository.location = Some(super::model::SourceLocation {
            path: "src/orders/order_repository.ts".to_string(),
            line: Some(41),
            column: None,
            end_line: Some(83),
            end_column: None,
        });
    }
    append_review_db_object(
        &mut snapshot,
        "orders_pkey",
        "primary_key",
        "db:column:orders:id",
        false,
    );
    append_review_db_object(
        &mut snapshot,
        "orders_customer_fkey",
        "foreign_key",
        "db:column:orders:customer_id",
        false,
    );
    append_review_db_object(
        &mut snapshot,
        "orders_customer_key",
        "unique",
        "db:column:orders:customer_id",
        false,
    );
    append_review_db_object(
        &mut snapshot,
        "orders_customer_check",
        "check",
        "db:column:orders:customer_id",
        false,
    );
    append_review_db_object(
        &mut snapshot,
        "orders_customer_idx",
        "index",
        "db:column:orders:customer_id",
        true,
    );
    append_structural_review_constraint(&mut snapshot, "orders_unbound_check", "check");

    let table_map = visual_map(
        &snapshot,
        Some("db:table:orders".to_string()),
        "table-usage".to_string(),
    );
    let column_map = visual_map(
        &snapshot,
        Some("db:column:orders:customer_id".to_string()),
        "column-impact".to_string(),
    );
    let board = table_map.review_board.as_ref().unwrap();

    assert_eq!(board.scope, "table");
    assert_eq!(
        board
            .lanes
            .iter()
            .map(|lane| lane.id.as_str())
            .collect::<Vec<_>>(),
        ["direct", "candidates", "unknowns", "checks",]
    );
    let direct = review_lane(board, "direct");
    for kind in [
        "primary-key",
        "foreign-key",
        "unique",
        "check",
        "unique-index",
    ] {
        assert!(
            direct.items.iter().any(|item| item.kind == kind),
            "missing {kind}"
        );
    }
    assert!(direct
        .items
        .iter()
        .any(|item| item.truth_class == "confirmed"));
    assert!(direct
        .items
        .iter()
        .any(|item| item.truth_class == "structural"));
    assert!(review_lane(board, "candidates")
        .items
        .iter()
        .all(|item| item.truth_class == "candidate" && item.confidence.is_some()));
    assert!(review_lane(board, "unknowns")
        .items
        .iter()
        .any(|item| item.kind == "db-capability"));
    assert!(review_lane(board, "checks").items.iter().any(|item| item
        .location
        .as_ref()
        .is_some_and(|location| location.line == Some(41))));
    assert!(!board.markdown_summary.contains("hunter2"));
    assert!(board
        .markdown_summary
        .contains("src/orders/order_repository.ts:L41"));
    assert_eq!(column_map.review_board.as_ref().unwrap().scope, "column");
    assert!(column_map
        .nodes
        .iter()
        .any(|node| node.kind == "constraint"));
    assert!(column_map.edges.iter().any(|edge| edge.kind == "db_index"));
}

#[test]
fn change_impact_db_only_and_empty_facts_are_unknown_not_confirmed_absence() {
    let mut snapshot = fixture_inventory("workspace-1".to_string());
    snapshot.items.retain(|item| {
        item.source == "db" && item.id.starts_with("db:table:orders")
            || item.id.starts_with("db:column:orders:")
    });
    for item in &mut snapshot.items {
        item.is_primary_key = false;
        item.is_foreign_key = false;
    }
    snapshot.links.clear();

    let map = visual_map(
        &snapshot,
        Some("db:table:orders".to_string()),
        "table-usage".to_string(),
    );
    let board = map.review_board.as_ref().unwrap();

    assert_eq!(review_lane(board, "direct").total, 0);
    assert_eq!(review_lane(board, "candidates").total, 0);
    assert!(review_lane(board, "unknowns")
        .items
        .iter()
        .any(|item| item.kind == "missing-db-facts"));
    assert!(review_lane(board, "unknowns")
        .items
        .iter()
        .any(|item| item.kind == "missing-code-candidates"));
    assert!(review_lane(board, "direct")
        .empty_message
        .contains("확정하지 않습니다"));
}

#[test]
fn change_impact_review_lanes_are_independently_bounded() {
    let mut snapshot = fixture_inventory("workspace-1".to_string());
    for index in 0..24 {
        append_review_db_object(
            &mut snapshot,
            &format!("orders_check_{index}"),
            "check",
            "db:column:orders:id",
            false,
        );
        let mut code = item(
            &format!("code:function:OrderRepository{index}"),
            "repository",
            &format!("OrderRepository{index}"),
            "code",
            "code",
            None,
            Some(&format!("src/orders/order_repository_{index}.rs")),
        );
        code.location = Some(super::model::SourceLocation {
            path: format!("src/orders/order_repository_{index}.rs"),
            line: Some(index + 1),
            column: None,
            end_line: None,
            end_column: None,
        });
        snapshot.items.push(code);
        snapshot.metadata.gaps.push(super::model::SnapshotGap {
            id: format!("gap:db-capability:{index}"),
            kind: "db-capability".to_string(),
            message: format!("capability {index} unavailable"),
            related_ids: Vec::new(),
        });
    }

    let map = visual_map(
        &snapshot,
        Some("db:table:orders".to_string()),
        "table-usage".to_string(),
    );
    let board = map.review_board.as_ref().unwrap();

    for (id, cap) in [
        ("direct", 12),
        ("candidates", 10),
        ("unknowns", 8),
        ("checks", 10),
    ] {
        let lane = review_lane(board, id);
        assert_eq!(lane.items.len(), cap);
        assert!(lane.hidden > 0);
        assert_eq!(lane.total, lane.items.len() + lane.hidden);
    }
}

#[test]
fn api_flow_without_handles_reports_an_unknown_gap_without_name_fallback() {
    let mut snapshot = fixture_inventory("workspace-1".to_string());
    snapshot.links.retain(|link| link.kind != "code_handle");

    let map = visual_map(
        &snapshot,
        Some("code:route:orders:create".to_string()),
        "api-flow".to_string(),
    );

    assert_eq!(map.nodes.len(), 1);
    assert_eq!(map.nodes[0].id, "code:route:orders:create");
    assert!(map.edges.is_empty());
    assert!(map
        .warnings
        .iter()
        .any(|warning| warning.contains("HANDLES") && warning.contains("알 수 없습니다")));
    let answer = map.api_reading.as_ref().unwrap();
    assert_eq!(answer.steps.len(), 1);
    assert_eq!(answer.steps[0].item.truth_class, "structural");
    assert!(answer.db_candidates.is_empty());
    assert!(answer
        .unknowns
        .iter()
        .any(|item| item.kind == "handler-gap" && item.detail.contains("HANDLES")));
}

#[test]
fn api_flow_rejects_untrusted_and_malformed_engine_edges() {
    for truth_class in ["candidate", "unknown", "structural"] {
        let mut snapshot = fixture_inventory("workspace-1".to_string());
        snapshot
            .links
            .iter_mut()
            .find(|link| link.kind == "code_handle")
            .unwrap()
            .truth_class = truth_class.to_string();
        let map = visual_map(
            &snapshot,
            Some("code:route:orders:create".to_string()),
            "api-flow".to_string(),
        );
        assert_eq!(map.nodes.len(), 1, "truth class {truth_class}");
        assert!(map.edges.is_empty(), "truth class {truth_class}");
        let answer = map.api_reading.unwrap();
        assert_eq!(answer.steps.len(), 1);
        let rejected = answer
            .unknowns
            .iter()
            .find(|item| item.kind == "handler-gap")
            .unwrap();
        assert_eq!(rejected.title, "확정 HANDLES 없음");
        assert_eq!(rejected.truth_class, truth_class);
        assert!(rejected
            .evidence
            .iter()
            .any(|evidence| evidence.kind == "engine-edge"));
    }

    let mut missing_type = fixture_inventory("workspace-1".to_string());
    missing_type
        .links
        .iter_mut()
        .find(|link| link.kind == "code_handle")
        .unwrap()
        .engine_edge_type = None;
    let map = visual_map(
        &missing_type,
        Some("code:route:orders:create".to_string()),
        "api-flow".to_string(),
    );
    assert_eq!(map.nodes.len(), 1);
    assert!(map.edges.is_empty());

    for mutation in ["candidate", "unknown", "missing-type"] {
        let mut snapshot = fixture_inventory("workspace-1".to_string());
        let call = snapshot
            .links
            .iter_mut()
            .find(|link| link.kind == "code_call")
            .unwrap();
        if mutation == "missing-type" {
            call.engine_edge_type = None;
        } else {
            call.truth_class = mutation.to_string();
        }
        let map = visual_map(
            &snapshot,
            Some("code:route:orders:create".to_string()),
            "api-flow".to_string(),
        );
        assert!(!map
            .nodes
            .iter()
            .any(|node| node.id == "code:class:OrderService"));
        assert!(!map.edges.iter().any(|edge| edge.kind == "code_call"));
        let answer = map.api_reading.unwrap();
        assert_eq!(answer.steps.len(), 2);
        let rejected = answer
            .unknowns
            .iter()
            .find(|item| item.kind == "call-gap")
            .unwrap();
        assert_eq!(rejected.title, "비확정 CALLS 제외");
        assert_eq!(
            rejected.truth_class,
            if mutation == "missing-type" {
                "confirmed"
            } else {
                mutation
            }
        );
        assert!(rejected
            .evidence
            .iter()
            .any(|evidence| evidence.kind == "engine-edge"));
    }
}

#[test]
fn api_flow_surfaces_snapshot_coverage_risks_and_reindex_action() {
    let mut snapshot = fixture_inventory("workspace-1".to_string());
    snapshot
        .stale_reasons
        .push("code engine changed".to_string());
    snapshot.metadata.migration.reindex_required = true;
    snapshot
        .metadata
        .migration
        .notes
        .push("schema v1 migration".to_string());
    snapshot.metadata.gaps.extend([
        super::model::SnapshotGap {
            id: "route-gap".to_string(),
            kind: "code-inventory-gap".to_string(),
            message: "route source range unavailable".to_string(),
            related_ids: vec!["code:route:orders:create".to_string()],
        },
        super::model::SnapshotGap {
            id: "unrelated-gap".to_string(),
            kind: "code-inventory-gap".to_string(),
            message: "unrelated item unavailable".to_string(),
            related_ids: vec!["code:function:unrelated".to_string()],
        },
    ]);
    snapshot.metadata.db = Some(super::model::SnapshotSourceMetadata {
        saved_at: "1".to_string(),
        engine_id: Some("database-memory".to_string()),
        engine_version: Some("1".to_string()),
        engine_checksum: None,
        contract_version: Some("1".to_string()),
        snapshot_key: Some("test".to_string()),
        limit_requested: Some(10_000),
        limit_applied: Some(5_000),
        limit_clamped: Some(true),
        result_count: Some(5_000),
        total_tables: Some(6_000),
        truncated: Some(true),
        source_path: None,
        source_type: "postgres".to_string(),
        profile_id: Some("test".to_string()),
    });

    let answer = visual_map(
        &snapshot,
        Some("code:route:orders:create".to_string()),
        "api-flow".to_string(),
    )
    .api_reading
    .unwrap();
    let kinds = answer
        .unknowns
        .iter()
        .map(|item| item.kind.as_str())
        .collect::<std::collections::HashSet<_>>();

    assert!(kinds.contains("stale"));
    assert!(kinds.contains("reindex"));
    assert!(kinds.contains("code-inventory-gap"));
    assert!(kinds.contains("db-inventory-truncated"));
    assert!(kinds.contains("db-limit-clamped"));
    assert!(!answer
        .unknowns
        .iter()
        .any(|item| item.detail.contains("unrelated item")));
    assert!(answer
        .recommended_checks
        .iter()
        .any(|item| item.kind == "reindex"));
}

#[test]
fn api_flow_deduplicates_db_targets_before_the_visible_target_cap() {
    let route_id = "code:route:candidates";
    let handler_id = "code:function:candidate-handler";
    let mut items = vec![
        located_api_item(route_id, "api", "GET /candidates", "api", 1),
        located_api_item(handler_id, "handler", "CandidateHandler", "code", 2),
    ];
    let mut links = vec![confirmed_api_link(
        "handle:candidates",
        route_id,
        handler_id,
        "code_handle",
        "HANDLES",
    )];

    for index in 0..8 {
        let id = format!("code:function:a-order-{index}");
        items.push(located_api_item(
            &id,
            "repository",
            &format!("OrderRepository{index}"),
            "code",
            10 + index,
        ));
        links.push(confirmed_api_link(
            &format!("call:order-{index}"),
            handler_id,
            &id,
            "code_call",
            "CALLS",
        ));
    }
    for (index, (singular, plural)) in [
        ("Invoice", "invoices"),
        ("Payment", "payments"),
        ("Shipment", "shipments"),
        ("Product", "products"),
        ("Supplier", "suppliers"),
        ("Category", "categories"),
        ("Account", "accounts"),
        ("Session", "sessions"),
    ]
    .into_iter()
    .enumerate()
    {
        let id = format!("code:function:z-{index}-{singular}");
        items.push(located_api_item(
            &id,
            "repository",
            &format!("{singular}Repository"),
            "code",
            30 + index as u64,
        ));
        items.push(item(
            &format!("db:table:{plural}"),
            "table",
            plural,
            "data",
            "db",
            None,
            Some("public"),
        ));
        links.push(confirmed_api_link(
            &format!("call:unique-{index}"),
            handler_id,
            &id,
            "code_call",
            "CALLS",
        ));
    }
    items.push(item(
        "db:table:orders",
        "table",
        "orders",
        "data",
        "db",
        None,
        Some("public"),
    ));
    let snapshot = InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: "workspace-1".to_string(),
        saved_at: "1".to_string(),
        metadata: Default::default(),
        stale_reasons: Vec::new(),
        links,
        items,
    };

    let answer = visual_map(
        &snapshot,
        Some(route_id.to_string()),
        "api-flow".to_string(),
    )
    .api_reading
    .unwrap();
    let target_ids = answer
        .db_candidates
        .iter()
        .filter_map(|candidate| candidate.node_id.as_deref())
        .collect::<std::collections::HashSet<_>>();
    let orders = answer
        .db_candidates
        .iter()
        .find(|candidate| candidate.node_id.as_deref() == Some("db:table:orders"))
        .unwrap();

    assert_eq!(answer.db_candidates.len(), 8);
    assert_eq!(target_ids.len(), 8);
    assert!(
        orders
            .evidence
            .iter()
            .filter(|evidence| evidence.kind == "candidate-source")
            .count()
            > 1
    );
    assert_eq!(answer.hidden_branches, 1);
}

#[test]
fn api_flow_reports_the_candidate_linker_pre_cap_as_unknown() {
    let route_id = "code:route:many-candidates";
    let handler_id = "code:function:many-candidates-handler";
    let repository_id = "code:function:many-candidates-repository";
    let mut items = vec![
        located_api_item(route_id, "api", "GET /many", "api", 1),
        located_api_item(handler_id, "handler", "ManyCandidatesHandler", "code", 2),
        located_api_item(
            repository_id,
            "repository",
            "AlphaBetaGammaDeltaEpsilonZetaEtaThetaRepository",
            "code",
            3,
        ),
    ];
    for name in [
        "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta",
    ] {
        items.push(item(
            &format!("db:table:{name}"),
            "table",
            name,
            "data",
            "db",
            None,
            Some("public"),
        ));
    }
    let snapshot = InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: "workspace-1".to_string(),
        saved_at: "1".to_string(),
        metadata: Default::default(),
        stale_reasons: Vec::new(),
        links: vec![
            confirmed_api_link(
                "handle:many-candidates",
                route_id,
                handler_id,
                "code_handle",
                "HANDLES",
            ),
            confirmed_api_link(
                "call:many-candidates",
                handler_id,
                repository_id,
                "code_call",
                "CALLS",
            ),
        ],
        items,
    };

    let answer = visual_map(
        &snapshot,
        Some(route_id.to_string()),
        "api-flow".to_string(),
    )
    .api_reading
    .unwrap();

    assert_eq!(answer.db_candidates.len(), 6);
    assert!(answer.truncated);
    assert!(answer.hidden_branches_is_lower_bound);
    assert!(answer
        .unknowns
        .iter()
        .any(|item| item.kind == "candidate-cap" && item.detail.contains("최대 6개")));
}

#[test]
fn api_flow_is_fair_across_handlers_and_reports_caps_without_dangling_edges() {
    let route_id = "code:route:fair";
    let handler_a = "code:function:handler-a";
    let handler_b = "code:function:handler-b";
    let mut items = vec![
        located_api_item(route_id, "api", "GET /fair", "api", 1),
        located_api_item(handler_a, "handler", "AHandler", "code", 10),
        located_api_item(handler_b, "handler", "BHandler", "code", 20),
    ];
    let mut links = vec![
        confirmed_api_link("handle:a", route_id, handler_a, "code_handle", "HANDLES"),
        confirmed_api_link("handle:b", route_id, handler_b, "code_handle", "HANDLES"),
    ];
    for index in 0..30 {
        let target = format!("code:function:a-{index:02}");
        items.push(located_api_item(
            &target,
            "function",
            &format!("A{index:02}"),
            "code",
            100 + index,
        ));
        links.push(confirmed_api_link(
            &format!("call:a-{index:02}"),
            handler_a,
            &target,
            "code_call",
            "CALLS",
        ));
        links.push(confirmed_api_link(
            &format!("cycle:a-{index:02}"),
            &target,
            handler_a,
            "code_call",
            "CALLS",
        ));
    }
    let b_target = "code:function:b-target";
    items.push(located_api_item(
        b_target, "function", "BTarget", "code", 200,
    ));
    links.push(confirmed_api_link(
        "call:b-target",
        handler_b,
        b_target,
        "code_call",
        "CALLS",
    ));
    links.push(confirmed_api_link(
        "cycle:b-target",
        b_target,
        handler_b,
        "code_call",
        "CALLS",
    ));
    links.push(confirmed_api_link(
        "call:dangling",
        handler_a,
        "code:function:missing",
        "code_call",
        "CALLS",
    ));
    let snapshot = InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: "workspace-1".to_string(),
        saved_at: "1".to_string(),
        metadata: Default::default(),
        stale_reasons: Vec::new(),
        links,
        items,
    };

    let map = visual_map(
        &snapshot,
        Some(route_id.to_string()),
        "api-flow".to_string(),
    );
    let answer = map.api_reading.as_ref().unwrap();
    let first_lines = answer
        .steps
        .iter()
        .take(5)
        .map(|step| {
            step.item
                .location
                .as_ref()
                .and_then(|location| location.line)
        })
        .collect::<Vec<_>>();
    let visible_ids = map
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<std::collections::HashSet<_>>();

    assert_eq!(
        first_lines,
        [Some(1), Some(10), Some(20), Some(100), Some(200)]
    );
    assert!(answer
        .steps
        .iter()
        .any(|step| step.item.node_id.as_deref() == Some(b_target)));
    assert!(answer.truncated && answer.hidden_branches > 0);
    assert!(answer.hidden_branches_is_lower_bound);
    assert!(answer
        .truncation_reason
        .as_deref()
        .is_some_and(|reason| reason.contains("24")));
    assert!(answer
        .truncation_reason
        .as_deref()
        .is_some_and(|reason| reason.contains("32")));
    assert_eq!(map.edges.len(), 32);
    assert!(!map.edges.iter().any(|edge| edge.id == "call:dangling"));
    assert!(map
        .edges
        .iter()
        .all(|edge| visible_ids.contains(edge.from.as_str())
            && visible_ids.contains(edge.to.as_str())));
}

#[test]
fn api_flow_reports_the_call_hop_cap() {
    let route_id = "code:route:deep";
    let handler_id = "code:function:deep-handler";
    let mut items = vec![
        located_api_item(route_id, "api", "GET /deep", "api", 1),
        located_api_item(handler_id, "handler", "DeepHandler", "code", 2),
    ];
    let mut links = vec![confirmed_api_link(
        "handle:deep",
        route_id,
        handler_id,
        "code_handle",
        "HANDLES",
    )];
    let mut from = handler_id.to_string();
    for index in 0..6 {
        let to = format!("code:function:deep-{index}");
        items.push(located_api_item(
            &to,
            "function",
            &format!("Deep{index}"),
            "code",
            10 + index,
        ));
        links.push(confirmed_api_link(
            &format!("call:deep-{index}"),
            &from,
            &to,
            "code_call",
            "CALLS",
        ));
        from = to;
    }
    let snapshot = InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: "workspace-1".to_string(),
        saved_at: "1".to_string(),
        metadata: Default::default(),
        stale_reasons: Vec::new(),
        links,
        items,
    };

    let map = visual_map(
        &snapshot,
        Some(route_id.to_string()),
        "api-flow".to_string(),
    );
    let answer = map.api_reading.unwrap();

    assert_eq!(answer.steps.len(), 6);
    assert!(answer.truncated);
    assert_eq!(answer.hidden_branches, 1);
    assert!(answer.hidden_branches_is_lower_bound);
    assert!(answer.unknowns.iter().any(|item| {
        item.kind == "truncated"
            && item.detail.contains("최소 1개의 경계 관계")
            && item.detail.contains("경계 아래는 탐색하지 않아")
    }));
    assert!(answer
        .truncation_reason
        .as_deref()
        .is_some_and(|reason| reason.contains("4 hop")));
    assert!(!map
        .nodes
        .iter()
        .any(|node| node.id == "code:function:deep-4"));
}

#[test]
fn api_flow_is_cycle_safe_preserves_evidence_and_limits_db_candidates_to_reachable_code() {
    let mut snapshot = fixture_inventory("workspace-1".to_string());
    snapshot
        .items
        .iter_mut()
        .find(|item| item.id == "code:route:orders:create")
        .unwrap()
        .location = Some(super::model::SourceLocation {
        path: "routes/orders.ts".to_string(),
        line: Some(1),
        column: None,
        end_line: None,
        end_column: None,
    });
    snapshot.links.extend([
        confirmed_api_link(
            "code-call:service->handler",
            "code:class:OrderService",
            "code:function:CreateOrderHandler",
            "code_call",
            "CALLS",
        ),
        confirmed_api_link(
            "code-call:service->repository",
            "code:class:OrderService",
            "code:class:OrderRepository",
            "code_call",
            "CALLS",
        ),
    ]);
    snapshot.items.push(item(
        "code:class:CustomerRepository",
        "repository",
        "CustomerRepository",
        "code",
        "code",
        None,
        Some("customer_repository.ts"),
    ));

    let map = visual_map(
        &snapshot,
        Some("code:route:orders:create".to_string()),
        "api-flow".to_string(),
    );
    let answer = map.api_reading.as_ref().unwrap();
    let step_ids = answer
        .steps
        .iter()
        .filter_map(|step| step.item.node_id.as_deref())
        .collect::<Vec<_>>();
    let service_step = answer
        .steps
        .iter()
        .find(|step| step.item.node_id.as_deref() == Some("code:class:OrderService"))
        .unwrap();

    assert_eq!(
        step_ids.len(),
        step_ids
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>()
            .len()
    );
    assert!(map
        .edges
        .iter()
        .any(|edge| edge.id == "code-call:service->handler"));
    assert!(service_step
        .incoming_evidence
        .iter()
        .any(|evidence| evidence.kind == "code-call"));
    assert!(service_step
        .incoming_evidence
        .iter()
        .any(|evidence| evidence.kind == "engine-edge"));
    assert!(answer.steps.iter().any(|step| step.item.node_id.as_deref()
        == Some("code:class:OrderRepository")
        && step.lane == "repository-query"));
    assert!(!answer
        .db_candidates
        .iter()
        .any(|candidate| candidate.id.contains("CustomerRepository")));
    assert!(answer
        .db_candidates
        .iter()
        .all(|candidate| candidate.truth_class == "candidate"));
}

#[test]
fn visual_map_without_focus_returns_overview() {
    let snapshot = fixture_inventory("workspace-1".to_string());
    let map = visual_map(&snapshot, None, "atlas".to_string());
    let node_ids = map
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<std::collections::HashSet<_>>();

    assert_eq!(map.focus, "overview");
    assert!(map.nodes.len() <= 40);
    assert!(map.nodes.iter().any(|node| node.id == "group:domain:order"
        && node.kind == "group-domain"
        && node
            .subtitle
            .as_deref()
            .is_some_and(|subtitle| { subtitle.starts_with("API 1 · 코드 3 · DB 1|") })));
    assert!(map
        .nodes
        .iter()
        .any(|node| node.id == "group:domain:customer"));
    assert!(!map.nodes.iter().any(|node| node.id == "db:table:orders"));
    assert!(map
        .warnings
        .iter()
        .any(|warning| warning.contains("도메인 카드")));
    assert!(map
        .edges
        .iter()
        .all(|edge| node_ids.contains(edge.from.as_str()) && node_ids.contains(edge.to.as_str())));
    assert_eq!(map, visual_map(&snapshot, None, "atlas".to_string()));
}

#[test]
fn atlas_overview_caps_ranked_domain_cards_and_reports_hidden_count() {
    let items = (0..45)
        .map(|index| {
            item(
                &format!("code:route:area{index}"),
                "api",
                &format!("GET /area{index}"),
                "api",
                "code",
                None,
                None,
            )
        })
        .collect();
    let snapshot = InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: "workspace-1".to_string(),
        saved_at: "1".to_string(),
        metadata: Default::default(),
        stale_reasons: Vec::new(),
        links: Vec::new(),
        items,
    };

    let map = visual_map(&snapshot, None, "atlas".to_string());

    assert_eq!(map.nodes.len(), 40);
    assert!(map
        .nodes
        .iter()
        .all(|node| node.id.starts_with("group:domain:")));
    assert!(map
        .warnings
        .iter()
        .any(|warning| warning.contains("+5") && warning.contains("접었습니다")));
}

#[test]
fn atlas_overview_prefers_api_or_db_domains_over_large_code_only_groups() {
    let snapshot = InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: "workspace-1".to_string(),
        saved_at: "1".to_string(),
        metadata: Default::default(),
        stale_reasons: Vec::new(),
        links: vec![super::model::SnapshotLink {
            id: "code-call:audio-one->audio-two".to_string(),
            from: "code:function:audio-one".to_string(),
            to: "code:function:audio-two".to_string(),
            kind: "code_call".to_string(),
            label: Some("CALLS".to_string()),
            truth_class: "confirmed".to_string(),
            direction: "outbound".to_string(),
            engine_edge_type: Some("CALLS".to_string()),
            evidence: Vec::new(),
        }],
        items: vec![
            item(
                "code:function:audio-one",
                "function",
                "decodeAudio",
                "code",
                "code",
                None,
                Some("src/audio/decode.ts"),
            ),
            item(
                "code:function:audio-two",
                "function",
                "encodeAudio",
                "code",
                "code",
                None,
                Some("src/audio/encode.ts"),
            ),
            item(
                "code:route:orders",
                "api",
                "GET /orders",
                "api",
                "code",
                None,
                None,
            ),
        ],
    };

    let map = visual_map(&snapshot, None, "atlas".to_string());

    assert_eq!(map.nodes[0].id, "group:domain:order");
    assert_eq!(map.nodes[1].id, "group:domain:audio");
}

#[test]
fn large_snapshot_projection_stays_bounded_and_has_no_dangling_edges() {
    let domains = [
        "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india",
        "juliet", "kilo", "lima", "mike", "november", "oscar", "papa", "quebec", "romeo", "sierra",
        "tango", "uniform", "victor", "whiskey", "xray", "yankee", "zulu", "amber", "birch",
        "cedar", "dawn", "ember", "fable", "glacier", "harbor", "island", "jasmine", "keystone",
        "lagoon", "meadow", "north", "orchid", "prairie", "quartz", "river", "summit", "thunder",
        "upland", "valley", "willow", "zephyr",
    ];
    let mut items = (0..10_000)
        .map(|index| {
            let domain = domains[index % domains.len()];
            item(
                &format!("code:function:{domain}:{index}"),
                "function",
                &format!("{domain}Function{index}"),
                "code",
                "code",
                None,
                Some(&format!("src/{domain}/flow.rs")),
            )
        })
        .collect::<Vec<_>>();
    items.extend((0..200).map(|index| {
        item(
            &format!("db:table:public.table_{index}"),
            "table",
            &format!("table_{index}"),
            "data",
            "db",
            None,
            Some("public"),
        )
    }));
    let links = (0..40_000)
        .map(|index| {
            let from = index % 10_000;
            let to = (from + 1 + (index / 10_000)) % 10_000;
            let from_domain = domains[from % domains.len()];
            let to_domain = domains[to % domains.len()];
            super::model::SnapshotLink {
                id: format!("code-call:{index}"),
                from: format!("code:function:{from_domain}:{from}"),
                to: format!("code:function:{to_domain}:{to}"),
                kind: "code_call".to_string(),
                label: Some("CALLS".to_string()),
                truth_class: "confirmed".to_string(),
                direction: "outbound".to_string(),
                engine_edge_type: Some("CALLS".to_string()),
                evidence: Vec::new(),
            }
        })
        .collect::<Vec<_>>();
    let snapshot = InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: "large-workspace".to_string(),
        saved_at: "1".to_string(),
        metadata: Default::default(),
        stale_reasons: Vec::new(),
        links,
        items,
    };

    let root = temp_root("large-snapshot");
    save_inventory_snapshot(&root, &snapshot).unwrap();
    let restore_started = Instant::now();
    let restored = load_inventory_snapshot(&root, "large-workspace").unwrap();
    let restore_elapsed = restore_started.elapsed();
    assert_eq!(restored.items.len(), snapshot.items.len());
    assert_eq!(restored.links.len(), snapshot.links.len());

    let overview_started = Instant::now();
    let map = visual_map(&restored, None, "atlas".to_string());
    let overview_elapsed = overview_started.elapsed();
    let node_ids = map
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<std::collections::HashSet<_>>();

    assert_eq!(snapshot.items.len(), 10_200);
    assert_eq!(
        snapshot
            .items
            .iter()
            .filter(|item| item.kind == "table")
            .count(),
        200
    );
    assert_eq!(snapshot.links.len(), 40_000);
    assert_eq!(map.nodes.len(), 40);
    assert!(map.nodes.iter().all(|node| node.kind == "group-domain"));
    assert!(map.edges.len() <= 80);
    assert!(map
        .edges
        .iter()
        .all(|edge| node_ids.contains(edge.from.as_str()) && node_ids.contains(edge.to.as_str())));
    assert!(map
        .warnings
        .iter()
        .any(|warning| warning.contains('+') && warning.contains("접었습니다")));

    let focus_started = Instant::now();
    let focused = visual_map(
        &restored,
        Some("code:function:alpha:0".to_string()),
        "search-focus".to_string(),
    );
    let focus_elapsed = focus_started.elapsed();
    let focused_ids = focused
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    assert!(focused.nodes.len() <= 36);
    assert!(focused.edges.iter().all(|edge| {
        focused_ids.contains(edge.from.as_str()) && focused_ids.contains(edge.to.as_str())
    }));

    let limit = if cfg!(debug_assertions) {
        Duration::from_secs(5)
    } else {
        Duration::from_secs(2)
    };
    eprintln!(
        "large_snapshot metrics: items={}, edges={}, restore_ms={}, overview_ms={}, focus_ms={}",
        snapshot.items.len(),
        snapshot.links.len(),
        restore_elapsed.as_millis(),
        overview_elapsed.as_millis(),
        focus_elapsed.as_millis()
    );
    assert!(
        restore_elapsed + overview_elapsed < limit,
        "restore plus first projection exceeded {limit:?}"
    );
    assert!(
        overview_elapsed < limit,
        "overview projection exceeded {limit:?}"
    );
    assert!(focus_elapsed < limit, "focus projection exceeded {limit:?}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn atlas_domain_canonicalization_keeps_plural_variants_together() {
    let snapshot = InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: "workspace-1".to_string(),
        saved_at: "1".to_string(),
        metadata: Default::default(),
        stale_reasons: Vec::new(),
        links: Vec::new(),
        items: vec![
            item(
                "db:table:status",
                "table",
                "status",
                "data",
                "db",
                None,
                Some("public"),
            ),
            item(
                "db:table:statuses",
                "table",
                "statuses",
                "data",
                "db",
                None,
                Some("public"),
            ),
            item(
                "db:table:category",
                "table",
                "category",
                "data",
                "db",
                None,
                Some("public"),
            ),
            item(
                "db:table:categories",
                "table",
                "categories",
                "data",
                "db",
                None,
                Some("public"),
            ),
        ],
    };

    let map = visual_map(&snapshot, None, "atlas".to_string());

    assert_eq!(map.nodes.len(), 2);
    assert!(map.nodes.iter().any(|node| {
        node.id == "group:domain:status"
            && node
                .subtitle
                .as_deref()
                .is_some_and(|subtitle| subtitle.contains("DB 2"))
    }));
    assert!(map.nodes.iter().any(|node| {
        node.id == "group:domain:category"
            && node
                .subtitle
                .as_deref()
                .is_some_and(|subtitle| subtitle.contains("DB 2"))
    }));
}

#[test]
fn atlas_prefers_group_metadata_and_skips_infrastructure_path_segments() {
    let mut java_item = item(
        "code:service:java-orders",
        "service",
        "OrderService",
        "code",
        "code",
        None,
        Some("src/main/java/com/acme/orders/OrderService.java"),
    );
    java_item.group_id = Some("com.acme.orders".to_string());
    let monorepo_item = item(
        "code:service:mono-orders",
        "service",
        "OrderWorkflow",
        "code",
        "code",
        None,
        Some("packages/web/src/features/orders/service.ts"),
    );
    let snapshot = InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: "workspace-1".to_string(),
        saved_at: "1".to_string(),
        metadata: Default::default(),
        stale_reasons: Vec::new(),
        links: Vec::new(),
        items: vec![java_item, monorepo_item],
    };

    let map = visual_map(&snapshot, None, "atlas".to_string());

    assert_eq!(map.nodes.len(), 1);
    assert_eq!(map.nodes[0].id, "group:domain:order");
    assert!(!map.nodes[0].id.contains("java"));
    assert!(!map.nodes[0].id.contains("feature"));
}

#[test]
fn atlas_cards_report_per_layer_hidden_counts_and_detail_preserves_each_layer() {
    let mut items = (0..3)
        .map(|index| {
            item(
                &format!("code:route:orders:{index}"),
                "api",
                "GET /orders/{id}",
                "api",
                "code",
                None,
                None,
            )
        })
        .collect::<Vec<_>>();
    items.extend((0..50).map(|index| {
        item(
            &format!("code:function:order-{index}"),
            "function",
            &format!("OrderFunction{index}"),
            "code",
            "code",
            None,
            Some(&format!("src/features/orders/order_{index}.rs")),
        )
    }));
    items.extend((0..5).map(|index| {
        item(
            &format!("db:table:order_records_{index}"),
            "table",
            &format!("order_records_{index}"),
            "data",
            "db",
            None,
            Some("public"),
        )
    }));
    let snapshot = InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: "workspace-1".to_string(),
        saved_at: "1".to_string(),
        metadata: Default::default(),
        stale_reasons: Vec::new(),
        links: Vec::new(),
        items,
    };

    let overview = visual_map(&snapshot, None, "atlas".to_string());
    let order = overview
        .nodes
        .iter()
        .find(|node| node.id == "group:domain:order")
        .unwrap();
    let subtitle = order.subtitle.as_deref().unwrap();
    assert!(subtitle.contains("API 3 · 코드 50 · DB 5"));
    assert!(subtitle.contains("+1"));
    assert!(subtitle.contains("+48"));
    assert!(subtitle.contains("+3"));

    let detail = visual_map(
        &snapshot,
        Some("group:domain:order".to_string()),
        "atlas".to_string(),
    );
    assert_eq!(detail.nodes.len(), 36);
    assert!(detail.nodes.iter().any(|node| node.layer == "api"));
    assert!(detail
        .nodes
        .iter()
        .any(|node| node.source == "code" && node.layer != "api"));
    assert!(detail.nodes.iter().any(|node| node.source == "db"));
}

#[test]
fn atlas_group_drilldown_is_bounded_ordered_and_has_no_dangling_edges() {
    let mut snapshot = fixture_inventory("workspace-1".to_string());
    snapshot.items.extend((0..50).map(|index| {
        item(
            &format!("code:function:order-extra-{index}"),
            "function",
            &format!("order_extra_{index}"),
            "code",
            "code",
            None,
            Some(&format!("src/orders/order_extra_{index}.rs")),
        )
    }));

    let map = visual_map(
        &snapshot,
        Some("group:domain:order".to_string()),
        "atlas".to_string(),
    );
    let node_ids = map
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let member_layers = map
        .nodes
        .iter()
        .filter(|node| !node.id.starts_with("group:"))
        .map(|node| match (node.layer.as_str(), node.source.as_str()) {
            ("api", _) => 0,
            (_, "code") => 1,
            (_, "db") => 2,
            _ => 3,
        })
        .collect::<Vec<_>>();

    assert_eq!(map.focus, "group:domain:order");
    assert_eq!(map.nodes.len(), 36);
    assert!(member_layers.windows(2).all(|pair| pair[0] <= pair[1]));
    assert!(map
        .edges
        .iter()
        .all(|edge| node_ids.contains(edge.from.as_str()) && node_ids.contains(edge.to.as_str())));
    assert!(map.edges.iter().any(|edge| {
        edge.kind == "group_contains"
            && edge
                .evidence
                .iter()
                .any(|evidence| evidence.kind == "group-evidence")
    }));
    assert!(map
        .warnings
        .iter()
        .any(|warning| warning.contains("+") && warning.contains("접었습니다")));
}

#[test]
fn atlas_keeps_duplicate_table_names_separate_by_non_default_schema() {
    let snapshot = InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: "workspace-1".to_string(),
        saved_at: "1".to_string(),
        metadata: Default::default(),
        stale_reasons: Vec::new(),
        links: Vec::new(),
        items: vec![
            item(
                "db:table:sales.orders",
                "table",
                "orders",
                "data",
                "db",
                None,
                Some("sales"),
            ),
            item(
                "db:table:billing.orders",
                "table",
                "orders",
                "data",
                "db",
                None,
                Some("billing"),
            ),
        ],
    };

    let map = visual_map(&snapshot, None, "atlas".to_string());

    assert_eq!(map.nodes.len(), 2);
    assert!(map.nodes.iter().any(|node| node.id == "group:domain:sale"));
    assert!(map
        .nodes
        .iter()
        .any(|node| node.id == "group:domain:billing"));
}

#[test]
fn atlas_does_not_promote_candidate_relationships_to_confirmed_group_edges() {
    let mut snapshot = fixture_inventory("workspace-1".to_string());
    snapshot.links.push(super::model::SnapshotLink {
        id: "candidate:orders-route->customers".to_string(),
        from: "code:route:orders:create".to_string(),
        to: "db:table:customers".to_string(),
        kind: "code_db".to_string(),
        label: Some("name match".to_string()),
        truth_class: "candidate".to_string(),
        direction: "outbound".to_string(),
        engine_edge_type: None,
        evidence: vec![super::model::Evidence {
            kind: "name-match".to_string(),
            text: "이름 단서만 있습니다".to_string(),
        }],
    });

    let map = visual_map(&snapshot, None, "atlas".to_string());

    assert!(map
        .edges
        .iter()
        .any(|edge| edge.kind == "candidate_group_code_db"));
    assert!(!map.edges.iter().any(|edge| edge.kind == "group_code_db"));
}

#[test]
fn focused_modes_without_focus_ask_to_narrow_scope() {
    let snapshot = fixture_inventory("workspace-1".to_string());
    let map = visual_map(&snapshot, None, "api-flow".to_string());

    assert_eq!(map.focus, "narrow-focus");
    assert!(map.nodes.is_empty());
    assert!(map.warnings[0].contains("대상"));
}

#[test]
fn capped_focus_map_does_not_return_edges_to_hidden_nodes() {
    let mut items = vec![item(
        "db:table:wide",
        "table",
        "wide",
        "data",
        "db",
        None,
        Some("public"),
    )];
    for index in 0..50 {
        items.push(item(
            &format!("db:column:wide:col_{index}"),
            "column",
            &format!("col_{index}"),
            "data",
            "db",
            Some("db:table:wide"),
            Some("text"),
        ));
    }
    let snapshot = InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: "workspace-1".to_string(),
        saved_at: "1".to_string(),
        metadata: Default::default(),
        stale_reasons: Vec::new(),
        links: Vec::new(),
        items,
    };

    let map = visual_map(
        &snapshot,
        Some("db:table:wide".to_string()),
        "table-usage".to_string(),
    );
    let node_ids = map
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<std::collections::HashSet<_>>();

    assert_eq!(map.nodes.len(), 36);
    assert!(map.warnings.iter().any(|warning| warning.contains("36")));
    assert!(map
        .edges
        .iter()
        .all(|edge| node_ids.contains(edge.from.as_str()) && node_ids.contains(edge.to.as_str())));
}

#[test]
fn save_inventory_snapshot_redacts_secret_shapes_before_persisting() {
    let root = temp_root("snapshot-redaction");
    let snapshot = InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: "workspace-1".to_string(),
        saved_at: "1".to_string(),
        metadata: Default::default(),
        stale_reasons: Vec::new(),
        links: Vec::new(),
        items: vec![
            InventoryItem {
                id: "db:profile:postgres".to_string(),
                kind: "profile".to_string(),
                name: "postgres://app:atlas_pg_pw@localhost/shop".to_string(),
                layer: "meta".to_string(),
                source: "db".to_string(),
                parent_id: None,
                path: Some(
                    "Server=localhost;Password=atlas_ado_pw;Pwd=atlas_short_pw;".to_string(),
                ),
                qualified_name: None,
                engine_label: None,
                project_id: None,
                group_id: None,
                location: None,
                is_primary_key: false,
                is_foreign_key: false,
                nullable: None,
            },
            InventoryItem {
                id: "db:profile:oracle".to_string(),
                kind: "profile".to_string(),
                name: "app/atlas_oracle_pw@localhost/XEPDB1".to_string(),
                layer: "meta".to_string(),
                source: "db".to_string(),
                parent_id: None,
                path: None,
                qualified_name: None,
                engine_label: None,
                project_id: None,
                group_id: None,
                location: None,
                is_primary_key: false,
                is_foreign_key: false,
                nullable: None,
            },
        ],
    };

    save_inventory_snapshot(&root, &snapshot).unwrap();

    let json = fs::read_to_string(snapshot_path(&root, "workspace-1")).unwrap();
    assert!(!json.contains("atlas_pg_pw"));
    assert!(!json.contains("atlas_ado_pw"));
    assert!(!json.contains("atlas_short_pw"));
    assert!(!json.contains("atlas_oracle_pw"));
    assert!(json.contains("[REDACTED]"));

    fs::remove_dir_all(root).unwrap();
}

fn located_api_item(
    id: &str,
    kind: &str,
    name: &str,
    layer: &str,
    line: u64,
) -> super::model::InventoryItem {
    let mut value = item(id, kind, name, layer, "code", None, Some("src/api.rs"));
    value.location = Some(super::model::SourceLocation {
        path: "src/api.rs".to_string(),
        line: Some(line),
        column: None,
        end_line: None,
        end_column: None,
    });
    value
}

fn confirmed_api_link(
    id: &str,
    from: &str,
    to: &str,
    kind: &str,
    engine_edge_type: &str,
) -> super::model::SnapshotLink {
    super::model::SnapshotLink {
        id: id.to_string(),
        from: from.to_string(),
        to: to.to_string(),
        kind: kind.to_string(),
        label: Some(engine_edge_type.to_string()),
        truth_class: "confirmed".to_string(),
        direction: "outbound".to_string(),
        engine_edge_type: Some(engine_edge_type.to_string()),
        evidence: vec![super::model::Evidence {
            kind: "engine-edge".to_string(),
            text: format!("original {engine_edge_type} evidence"),
        }],
    }
}

fn code_test_item(
    id: &str,
    engine_label: &str,
    name: &str,
    file_path: &str,
    line: u64,
    end_line: u64,
) -> CodeInventoryItem {
    CodeInventoryItem {
        id: id.to_string(),
        kind: engine_label.to_string(),
        name: name.to_string(),
        project: "shop".to_string(),
        qualified_name: id.to_string(),
        engine_label: engine_label.to_string(),
        file_path: Some(file_path.to_string()),
        line: Some(line),
        column: Some(3),
        end_line: Some(end_line),
        end_column: Some(18),
        detail: serde_json::json!({
            "module": "orders"
        }),
    }
}

fn write_snapshot_json(root: &std::path::Path, value: serde_json::Value) {
    let path = snapshot_path(root, "workspace-1");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_string_pretty(&value).unwrap()).unwrap();
}

fn test_workspace(repo_path: &str) -> Workspace {
    Workspace {
        id: "workspace-1".to_string(),
        name: "shop-api".to_string(),
        repo_path: repo_path.to_string(),
        code_project: None,
        engine_cache: WorkspaceEngineCache::default(),
        db_profiles: Vec::new(),
        active_db_profile_id: None,
        created_at: "1".to_string(),
        updated_at: "1".to_string(),
    }
}

fn test_registry() -> EngineRegistry {
    EngineRegistry {
        mode: EngineRuntimeMode::Dev,
        engine_dir: r"D:\engines".to_string(),
        engines: vec![
            EngineAvailability {
                id: "codebase-memory".to_string(),
                label: "codebase-memory".to_string(),
                role: "code".to_string(),
                executable: "codebase-memory-mcp.exe".to_string(),
                expected_version: "0.8.1".to_string(),
                contract_version: "1".to_string(),
                path: r"D:\engines\codebase-memory-mcp.exe".to_string(),
                available: false,
                releasable: false,
                integrity: "missing".to_string(),
                sha256: Some("code-sha".to_string()),
                error: None,
            },
            EngineAvailability {
                id: "database-memory".to_string(),
                label: "database-memory".to_string(),
                role: "db".to_string(),
                executable: "database-memory.exe".to_string(),
                expected_version: "0.1.0".to_string(),
                contract_version: "1".to_string(),
                path: r"D:\engines\database-memory.exe".to_string(),
                available: false,
                releasable: false,
                integrity: "missing".to_string(),
                sha256: Some("db-sha".to_string()),
                error: None,
            },
        ],
    }
}
