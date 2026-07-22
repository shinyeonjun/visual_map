use super::*;
use crate::{
    engine::{EngineAvailability, EngineRegistry, EngineRuntimeMode},
    workspace::{
        CodeCall, CodeHandle, CodeInventory, CodeInventoryItem, CodeInventorySummary, DbConstraint,
        DbDependentObject, DbForeignKey, DbIndex, DbInventory, DbInventoryColumn, DbInventoryTable,
        DbProfile, DbSource, Workspace, WorkspaceEngineCache,
    },
};
use std::{
    collections::{BTreeSet, HashSet},
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

fn dependent_object_snapshot() -> InventorySnapshot {
    let table_key = "sqlite:shop:main:public:table:orders";
    let status_key = "sqlite:shop:main:public:column:orders:status";
    let db = DbInventory {
        profile_id: "profile-1".to_string(),
        snapshot_key: Some("sqlite:shop".to_string()),
        contract_version: Some("2".to_string()),
        capability_warnings: Vec::new(),
        limit_requested: Some(1_000),
        limit_applied: Some(1_000),
        limit_clamped: Some(false),
        result_count: Some(1),
        total_tables: Some(1),
        truncated: Some(false),
        gaps: Vec::new(),
        tables: vec![DbInventoryTable {
            key: Some(table_key.to_string()),
            database: Some("main".to_string()),
            schema: Some("public".to_string()),
            name: "orders".to_string(),
            columns: vec![DbInventoryColumn {
                key: Some(status_key.to_string()),
                table_key: Some(table_key.to_string()),
                name: "status".to_string(),
                data_type: Some("text".to_string()),
                nullable: Some(false),
                is_primary_key: false,
                is_foreign_key: false,
            }],
            foreign_keys: Vec::new(),
            inbound_foreign_keys: Vec::new(),
            constraints: Vec::new(),
            indexes: Vec::new(),
            dependents: vec![
                DbDependentObject {
                    key: "sqlite:shop:main:public:view:active_orders".to_string(),
                    kind: "view".to_string(),
                    name: "active_orders".to_string(),
                    relation: "view_depends_on".to_string(),
                    column_keys: vec![status_key.to_string()],
                },
                DbDependentObject {
                    key: "sqlite:shop:main:public:trigger:orders:trg_orders_status".to_string(),
                    kind: "trigger".to_string(),
                    name: "trg_orders_status".to_string(),
                    relation: "table_has_trigger".to_string(),
                    column_keys: Vec::new(),
                },
                DbDependentObject {
                    key: "sqlite:shop:main:public:routine:refresh_orders".to_string(),
                    kind: "routine".to_string(),
                    name: "refresh_orders".to_string(),
                    relation: "routine_depends_on".to_string(),
                    column_keys: Vec::new(),
                },
                DbDependentObject {
                    key: "sqlite:shop:main:public:view:missing_column_view".to_string(),
                    kind: "view".to_string(),
                    name: "missing_column_view".to_string(),
                    relation: "view_depends_on".to_string(),
                    column_keys: vec!["sqlite:shop:main:public:column:orders:missing".to_string()],
                },
            ],
        }],
    };
    normalize_inventory("workspace-1".to_string(), None, Some(&db))
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
fn missing_inventory_snapshot_is_an_empty_state() {
    let root = temp_root("missing-snapshot");

    assert_eq!(
        load_inventory_snapshot_optional(&root, "workspace-1").unwrap(),
        None
    );
}

#[test]
fn removing_db_snapshot_preserves_code_and_scrubs_backups() {
    let root = temp_root("remove-db-snapshot");
    let snapshot = fixture_inventory("workspace-1".to_string());
    save_inventory_snapshot(&root, &snapshot).unwrap();

    remove_db_inventory_snapshot(&root, "workspace-1").unwrap();

    let restored = load_inventory_snapshot(&root, "workspace-1").unwrap();
    let backup: InventorySnapshot = serde_json::from_str(
        &fs::read_to_string(snapshot_backup_path(&snapshot_path(&root, "workspace-1"))).unwrap(),
    )
    .unwrap();
    assert!(restored.items.iter().all(|item| item.source != "db"));
    assert!(restored.items.iter().any(|item| item.source == "code"));
    assert!(restored.metadata.db.is_none());
    assert_eq!(backup, restored);
    fs::remove_dir_all(root).unwrap();
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
    let mut unverified_route =
        code_test_item("docs.route_string", "Route", "/api/v1/sessions", "", 1, 1);
    unverified_route.kind = "unknown".to_string();
    unverified_route.file_path = None;
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
        unknown: vec![unverified_route],
        summary: CodeInventorySummary {
            routes: 1,
            handlers: 1,
            services: 0,
            repositories: 0,
            functions: 0,
            classes: 0,
            modules: 0,
            files: 0,
            unknown: 1,
        },
        architecture: Some(serde_json::json!({ "modules": ["orders"] })),
        calls: vec![
            CodeCall {
                from: "shop.routes.create_order".to_string(),
                to: "shop.handlers.create_order".to_string(),
                confidence: Some(95),
                strategy: Some("lsp_direct".to_string()),
                expression: Some("create_order".to_string()),
            },
            CodeCall {
                from: "shop.handlers.create_order".to_string(),
                to: "shop.routes.create_order".to_string(),
                confidence: Some(38),
                strategy: Some("unique_name".to_string()),
                expression: Some("create_order".to_string()),
            },
        ],
        handles: vec![CodeHandle {
            handler: "shop.handlers.create_order".to_string(),
            route: "shop.routes.create_order".to_string(),
        }],
        partial: false,
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
    let trusted_call = snapshot
        .links
        .iter()
        .find(|link| link.kind == "code_call" && link.from == "code:shop.routes.create_order")
        .unwrap();
    let weak_call = snapshot
        .links
        .iter()
        .find(|link| link.kind == "code_call" && link.from == "code:shop.handlers.create_order")
        .unwrap();
    let unverified_route = snapshot
        .items
        .iter()
        .find(|entry| entry.id == "code:docs.route_string")
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
    assert_eq!(unverified_route.kind, "code");
    assert_eq!(unverified_route.engine_label.as_deref(), Some("Route"));
    assert_eq!(handles.from, "code:shop.routes.create_order");
    assert_eq!(handles.to, "code:shop.handlers.create_order");
    assert_eq!(handles.truth_class, "confirmed");
    assert_eq!(handles.direction, "outbound");
    assert_eq!(handles.engine_edge_type.as_deref(), Some("HANDLES"));
    assert!(handles
        .evidence
        .iter()
        .any(|evidence| evidence.text.contains("handler→route")));
    assert_eq!(trusted_call.truth_class, "confirmed");
    assert_eq!(weak_call.truth_class, "unknown");
    assert!(weak_call
        .evidence
        .iter()
        .any(|evidence| evidence.kind == "engine-confidence-score" && evidence.text == "38%"));
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
    assert_eq!(code.engine_version.as_deref(), Some("0.9.0"));
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
        partial: false,
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
fn snapshot_marks_code_stale_when_source_contents_change() {
    let root = temp_root("code-source-revision");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    let workspace = test_workspace(root.to_str().unwrap());
    let snapshot = snapshot_with_metadata(
        InventorySnapshot {
            schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
            workspace_id: workspace.id.clone(),
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
        &workspace,
        &test_registry(),
    );
    assert!(snapshot
        .metadata
        .code
        .as_ref()
        .and_then(|metadata| metadata.source_revision.as_ref())
        .is_some());

    fs::write(
        root.join("src/main.rs"),
        "fn main() { println!(\"changed\"); }\n",
    )
    .unwrap();
    let missing_snapshot = snapshot.clone();
    let stale = mark_snapshot_staleness(snapshot, &workspace, &test_registry());

    assert!(stale
        .stale_reasons
        .iter()
        .any(|reason| reason.contains("코드 파일")));
    fs::remove_dir_all(root).unwrap();
    let missing = mark_snapshot_staleness(missing_snapshot, &workspace, &test_registry());
    assert!(missing
        .stale_reasons
        .iter()
        .any(|reason| reason.contains("확인할 수 없습니다")));
}

#[test]
fn snapshot_freshness_cache_avoids_repeat_scan_until_invalidated() {
    let root = temp_root("freshness-cache");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    let mut workspace = test_workspace(root.to_str().unwrap());
    workspace.id = "freshness-cache-workspace".to_string();
    let registry = test_registry();
    let snapshot = snapshot_with_metadata(
        InventorySnapshot {
            schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
            workspace_id: workspace.id.clone(),
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
        &workspace,
        &registry,
    );

    assert!(snapshot_staleness_reasons_cached(&snapshot, &workspace, &registry).is_empty());
    fs::write(
        root.join("src/main.rs"),
        "fn main() { println!(\"changed after indexing\"); }\n",
    )
    .unwrap();
    assert!(snapshot_staleness_reasons_cached(&snapshot, &workspace, &registry).is_empty());

    invalidate_snapshot_freshness(&workspace.id);
    assert!(
        snapshot_staleness_reasons_cached(&snapshot, &workspace, &registry)
            .iter()
            .any(|reason| reason.contains("코드 파일"))
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn inventory_bootstrap_caps_code_but_keeps_exact_counts_and_full_search() {
    let mut items = (0..150)
        .map(|index| {
            item(
                &format!("code:function:item-{index:03}"),
                "function",
                &format!("function_{index:03}"),
                "code",
                "code",
                None,
                Some(&format!("src/function_{index:03}.rs")),
            )
        })
        .collect::<Vec<_>>();
    items.push(item(
        "code:route:health",
        "api",
        "/health",
        "api",
        "code",
        None,
        Some("src/routes.rs"),
    ));
    items.push(item(
        "db:table:public.users",
        "table",
        "users",
        "database",
        "db",
        None,
        Some("public"),
    ));
    let snapshot = InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: "bounded-bootstrap".to_string(),
        saved_at: "1".to_string(),
        metadata: Default::default(),
        stale_reasons: Vec::new(),
        links: Vec::new(),
        items,
    };

    let bootstrap = inventory_bootstrap(&snapshot);
    assert_eq!(bootstrap.summary.sources["code"].groups["functions"], 150);
    assert_eq!(
        bootstrap
            .snapshot
            .items
            .iter()
            .filter(|item| item.source == "code" && item.kind == "function")
            .count(),
        100
    );
    assert!(bootstrap
        .snapshot
        .items
        .iter()
        .any(|item| item.id == "db:table:public.users"));

    let result = search_inventory(&snapshot, "function_149");
    assert_eq!(result.total, 1);
    assert_eq!(result.hits[0].item.id, "code:function:item-149");
}

#[test]
fn inventory_bootstrap_caps_db_tables_with_their_children_and_keeps_full_search() {
    let mut items = Vec::new();
    for index in 0..150 {
        let table_id = format!("db:table:public.table_{index:03}");
        items.push(item(
            &table_id,
            "table",
            &format!("table_{index:03}"),
            "database",
            "db",
            None,
            Some("public"),
        ));
        items.push(item(
            &format!("db:column:public.table_{index:03}:id"),
            "column",
            "id",
            "database",
            "db",
            Some(&table_id),
            Some("INTEGER"),
        ));
    }
    let snapshot = InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: "bounded-db-bootstrap".to_string(),
        saved_at: "1".to_string(),
        metadata: Default::default(),
        stale_reasons: Vec::new(),
        links: Vec::new(),
        items,
    };

    let bootstrap = inventory_bootstrap(&snapshot);
    assert_eq!(bootstrap.summary.sources["db"].groups["table"], 150);
    assert_eq!(bootstrap.summary.sources["db"].groups["column"], 150);
    assert_eq!(
        bootstrap
            .snapshot
            .items
            .iter()
            .filter(|item| item.source == "db" && item.kind == "table")
            .count(),
        100
    );
    assert_eq!(
        bootstrap
            .snapshot
            .items
            .iter()
            .filter(|item| item.source == "db" && item.kind == "column")
            .count(),
        100
    );
    assert!(bootstrap.snapshot.items.iter().all(|item| {
        item.parent_id.as_ref().is_none_or(|parent| {
            bootstrap
                .snapshot
                .items
                .iter()
                .any(|candidate| candidate.id == *parent)
        })
    }));

    let result = search_inventory(&snapshot, "table_149");
    assert_eq!(result.counts["table"], 1);
    assert!(result
        .hits
        .iter()
        .any(|hit| hit.item.id == "db:table:public.table_149"));
}

#[test]
fn inventory_bootstrap_keeps_db_objects_linked_to_retained_tables() {
    let table_id = "db:table:public.orders";
    let column_id = "db:column:public.orders:status";
    let view_id = "db:view:active-orders";
    let routine_id = "db:routine:refresh-orders";
    let trigger_id = "db:trigger:audit-orders";
    let snapshot = InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: "db-dependents-bootstrap".to_string(),
        saved_at: "1".to_string(),
        metadata: Default::default(),
        stale_reasons: Vec::new(),
        items: vec![
            item(
                table_id,
                "table",
                "orders",
                "database",
                "db",
                None,
                Some("public"),
            ),
            item(
                column_id,
                "column",
                "status",
                "database",
                "db",
                Some(table_id),
                Some("TEXT"),
            ),
            item(view_id, "view", "active_orders", "data", "db", None, None),
            item(
                routine_id,
                "routine",
                "refresh_orders",
                "data",
                "db",
                None,
                None,
            ),
            item(
                trigger_id,
                "trigger",
                "audit_orders",
                "data",
                "db",
                Some(table_id),
                None,
            ),
        ],
        links: vec![
            confirmed_api_link(
                "view-status",
                view_id,
                column_id,
                "db_dependency",
                "VIEW_DEPENDS_ON_COLUMN",
            ),
            confirmed_api_link(
                "routine-orders",
                routine_id,
                table_id,
                "db_dependency",
                "ROUTINE_DEPENDS_ON_TABLE",
            ),
            confirmed_api_link(
                "orders-trigger",
                table_id,
                trigger_id,
                "db_trigger",
                "TABLE_HAS_TRIGGER",
            ),
        ],
    };

    let bootstrap = inventory_bootstrap(&snapshot);
    let retained_ids = bootstrap
        .snapshot
        .items
        .iter()
        .map(|item| item.id.as_str())
        .collect::<HashSet<_>>();

    assert_eq!(retained_ids.len(), 5);
    assert!(retained_ids.contains(view_id));
    assert!(retained_ids.contains(routine_id));
    assert!(retained_ids.contains(trigger_id));
    assert_eq!(bootstrap.snapshot.links.len(), 3);

    let result = search_inventory(&snapshot, "active_orders");
    assert_eq!(result.counts["db-object"], 1);
    assert_eq!(result.hits[0].item.id, view_id);
}

#[test]
fn inventory_bootstrap_caps_db_dependents_and_keeps_full_search() {
    let table_id = "db:table:public.orders";
    let mut items = vec![item(
        table_id,
        "table",
        "orders",
        "database",
        "db",
        None,
        Some("public"),
    )];
    let mut links = Vec::new();
    for index in 0..250 {
        let view_id = format!("db:view:orders-{index:03}");
        items.push(item(
            &view_id,
            "view",
            &format!("orders_view_{index:03}"),
            "data",
            "db",
            None,
            None,
        ));
        links.push(confirmed_api_link(
            &format!("view-orders-{index:03}"),
            &view_id,
            table_id,
            "db_dependency",
            "VIEW_DEPENDS_ON_TABLE",
        ));
    }
    let snapshot = InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: "bounded-db-dependents".to_string(),
        saved_at: "1".to_string(),
        metadata: Default::default(),
        stale_reasons: Vec::new(),
        links,
        items,
    };

    let bootstrap = inventory_bootstrap(&snapshot);
    assert_eq!(
        bootstrap
            .snapshot
            .items
            .iter()
            .filter(|item| item.kind == "view")
            .count(),
        200
    );
    assert_eq!(bootstrap.snapshot.links.len(), 200);
    assert_eq!(bootstrap.summary.sources["db"].groups["view"], 250);

    let result = search_inventory(&snapshot, "orders_view_249");
    assert_eq!(result.total, 1);
    assert_eq!(result.hits[0].item.id, "db:view:orders-249");
}

#[test]
fn replacing_code_inventory_preserves_the_existing_db_source() {
    let existing = fixture_inventory("workspace-1".to_string());
    let db_metadata = existing.metadata.db.clone();
    let db_ids = existing
        .items
        .iter()
        .filter(|item| item.source == "db")
        .map(|item| item.id.clone())
        .collect::<BTreeSet<_>>();
    let mut metadata = super::model::SnapshotMetadata {
        code: existing.metadata.code.clone(),
        architecture: Some(serde_json::json!({ "packages": ["replacement"] })),
        ..Default::default()
    };
    if let Some(code) = metadata.code.as_mut() {
        code.saved_at = "2".to_string();
    }
    let incoming = InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: existing.workspace_id.clone(),
        saved_at: "2".to_string(),
        metadata,
        stale_reasons: Vec::new(),
        links: Vec::new(),
        items: vec![item(
            "code:function:replacement",
            "function",
            "replacement",
            "code",
            "code",
            None,
            Some("src/replacement.rs"),
        )],
    };

    let merged = replace_inventory_source(Some(existing), incoming, "code").unwrap();
    assert_eq!(merged.metadata.db, db_metadata);
    assert!(db_ids
        .iter()
        .all(|id| merged.items.iter().any(|item| &item.id == id)));
    assert!(merged
        .items
        .iter()
        .any(|item| item.id == "code:function:replacement"));
    assert!(merged
        .items
        .iter()
        .filter(|item| item.source == "code")
        .all(|item| item.id == "code:function:replacement"));
    assert!(merged.links.iter().all(|link| {
        merged.items.iter().any(|item| item.id == link.from)
            && merged.items.iter().any(|item| item.id == link.to)
    }));
}

#[test]
fn replacing_code_inventory_clears_resolved_code_reindex_requirement() {
    let mut existing = fixture_inventory("workspace-1".to_string());
    existing.metadata.migration = super::model::SnapshotMigration {
        source_schema_version: Some(1),
        reindex_required: true,
        notes: vec![
            "Snapshot V1의 안전한 필드를 V2로 이전했습니다.".to_string(),
            "기존 CALLS에 엔진 신뢰도 정보가 없어 코드를 다시 읽어야 합니다.".to_string(),
        ],
    };
    existing.metadata.gaps.push(super::model::SnapshotGap {
        id: "gap:code-call-confidence".to_string(),
        kind: "unscored-code-call".to_string(),
        message: "old code calls require a fresh read".to_string(),
        related_ids: Vec::new(),
    });
    existing
        .stale_reasons
        .push("스냅샷 형식이 호환되지 않아 다시 읽어야 합니다".to_string());

    let incoming = InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: existing.workspace_id.clone(),
        saved_at: "2".to_string(),
        metadata: super::model::SnapshotMetadata {
            code: existing.metadata.code.clone(),
            ..Default::default()
        },
        stale_reasons: Vec::new(),
        links: Vec::new(),
        items: vec![item(
            "code:function:fresh",
            "function",
            "fresh",
            "code",
            "code",
            None,
            Some("src/fresh.rs"),
        )],
    };

    let merged = replace_inventory_source(Some(existing), incoming, "code").unwrap();

    assert!(!merged.metadata.migration.reindex_required);
    assert!(!merged
        .metadata
        .migration
        .notes
        .iter()
        .any(|note| note.contains("CALLS")));
    assert!(!merged
        .metadata
        .gaps
        .iter()
        .any(|gap| gap.kind == "unscored-code-call"));
    assert!(merged.stale_reasons.is_empty());
}

#[test]
fn replacing_code_inventory_clears_a_resolved_code_conflict_without_a_note() {
    let mut existing = fixture_inventory("workspace-1".to_string());
    existing.metadata.migration.reindex_required = true;
    existing.metadata.gaps.push(super::model::SnapshotGap {
        id: "gap:node-conflict:code:function:old".to_string(),
        kind: "node-conflict".to_string(),
        message: "old code conflict".to_string(),
        related_ids: vec!["code:function:old".to_string()],
    });

    let incoming = InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: existing.workspace_id.clone(),
        saved_at: "2".to_string(),
        metadata: super::model::SnapshotMetadata {
            code: existing.metadata.code.clone(),
            ..Default::default()
        },
        stale_reasons: Vec::new(),
        links: Vec::new(),
        items: vec![item(
            "code:function:fresh",
            "function",
            "fresh",
            "code",
            "code",
            None,
            Some("src/fresh.rs"),
        )],
    };

    let merged = replace_inventory_source(Some(existing), incoming, "code").unwrap();

    assert!(!merged.metadata.migration.reindex_required);
    assert!(!merged
        .metadata
        .gaps
        .iter()
        .any(|gap| gap.id.contains("code:function:old")));
    assert!(merged.stale_reasons.is_empty());
}

#[test]
fn replacing_code_inventory_keeps_unresolved_global_reindex_requirement() {
    let mut existing = fixture_inventory("workspace-1".to_string());
    existing.metadata.migration.reindex_required = true;
    existing.metadata.migration.notes.push(
        "주 스냅샷 대신 이전 백업을 복구했습니다. 다시 읽어 최신 상태를 확인하세요.".to_string(),
    );

    let incoming = InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: existing.workspace_id.clone(),
        saved_at: "2".to_string(),
        metadata: super::model::SnapshotMetadata {
            code: existing.metadata.code.clone(),
            ..Default::default()
        },
        stale_reasons: Vec::new(),
        links: Vec::new(),
        items: vec![item(
            "code:function:fresh",
            "function",
            "fresh",
            "code",
            "code",
            None,
            Some("src/fresh.rs"),
        )],
    };

    let merged = replace_inventory_source(Some(existing), incoming, "code").unwrap();

    assert!(merged.metadata.migration.reindex_required);
    assert!(merged
        .stale_reasons
        .iter()
        .any(|reason| reason.contains("다시")));
    assert!(merged
        .metadata
        .migration
        .notes
        .iter()
        .any(|note| note.contains("DB 구조")));

    let incoming_db = InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: merged.workspace_id.clone(),
        saved_at: "3".to_string(),
        metadata: super::model::SnapshotMetadata {
            db: merged.metadata.db.clone(),
            ..Default::default()
        },
        stale_reasons: Vec::new(),
        links: Vec::new(),
        items: vec![item(
            "db:table:fresh",
            "table",
            "fresh",
            "database",
            "db",
            None,
            Some("public"),
        )],
    };

    let fully_refreshed = replace_inventory_source(Some(merged), incoming_db, "db").unwrap();
    assert!(!fully_refreshed.metadata.migration.reindex_required);
    assert!(fully_refreshed.stale_reasons.is_empty());
}

#[test]
fn replacing_code_inventory_preserves_incoming_reindex_requirement() {
    let existing = fixture_inventory("workspace-1".to_string());
    let incoming = InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: existing.workspace_id.clone(),
        saved_at: "2".to_string(),
        metadata: super::model::SnapshotMetadata {
            code: existing.metadata.code.clone(),
            migration: super::model::SnapshotMigration {
                source_schema_version: None,
                reindex_required: true,
                notes: vec![
                    "기존 CALLS에 엔진 신뢰도 정보가 없어 코드를 다시 읽어야 합니다.".to_string(),
                ],
            },
            gaps: vec![super::model::SnapshotGap {
                id: "gap:code-call-confidence".to_string(),
                kind: "unscored-code-call".to_string(),
                message: "incoming code calls require a fresh read".to_string(),
                related_ids: Vec::new(),
            }],
            ..Default::default()
        },
        stale_reasons: Vec::new(),
        links: Vec::new(),
        items: vec![item(
            "code:function:unscored",
            "function",
            "unscored",
            "code",
            "code",
            None,
            Some("src/unscored.rs"),
        )],
    };

    let merged = replace_inventory_source(Some(existing), incoming, "code").unwrap();

    assert!(merged.metadata.migration.reindex_required);
    assert!(merged
        .metadata
        .migration
        .notes
        .iter()
        .any(|note| note.contains("CALLS")));
    assert!(merged
        .stale_reasons
        .iter()
        .any(|reason| reason.contains("다시")));
}

#[test]
fn snapshot_marks_ddl_stale_when_file_contents_change() {
    let root = temp_root("ddl-source-revision");
    fs::create_dir_all(&root).unwrap();
    let ddl = root.join("schema.sql");
    fs::write(&ddl, "create table orders(id integer primary key);\n").unwrap();
    let mut workspace = test_workspace(root.to_str().unwrap());
    workspace.db_profiles.push(DbProfile {
        id: "ddl".to_string(),
        name: "schema".to_string(),
        source: DbSource::DdlSqlite,
        path: Some(ddl.display().to_string()),
        host: None,
        port: None,
        database: None,
        username: None,
        cache_path: "db/schema.sqlite".to_string(),
        last_indexed_at: Some("1".to_string()),
        password_stored: false,
    });
    workspace.active_db_profile_id = Some("ddl".to_string());
    let snapshot = snapshot_with_metadata(
        InventorySnapshot {
            schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
            workspace_id: workspace.id.clone(),
            saved_at: "1".to_string(),
            metadata: Default::default(),
            stale_reasons: Vec::new(),
            links: Vec::new(),
            items: vec![item(
                "db:table:orders",
                "table",
                "orders",
                "data",
                "db",
                None,
                Some("main"),
            )],
        },
        &workspace,
        &test_registry(),
    );

    fs::write(
        &ddl,
        "create table orders(id integer primary key, status text);\n",
    )
    .unwrap();
    let stale = mark_snapshot_staleness(snapshot, &workspace, &test_registry());

    assert!(stale
        .stale_reasons
        .iter()
        .any(|reason| reason.contains("DB 파일")));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn snapshot_marks_ddl_directory_stale_when_a_schema_file_changes() {
    let root = temp_root("ddl-directory-source-revision");
    let ddl = root.join("schema");
    fs::create_dir_all(&ddl).unwrap();
    fs::write(
        ddl.join("orders.sql"),
        "create table orders(id integer primary key);\n",
    )
    .unwrap();
    let mut workspace = test_workspace(root.to_str().unwrap());
    workspace.db_profiles.push(DbProfile {
        id: "ddl-directory".to_string(),
        name: "schema directory".to_string(),
        source: DbSource::DdlSqlite,
        path: Some(ddl.display().to_string()),
        host: None,
        port: None,
        database: None,
        username: None,
        cache_path: "db/schema-directory.sqlite".to_string(),
        last_indexed_at: Some("1".to_string()),
        password_stored: false,
    });
    workspace.active_db_profile_id = Some("ddl-directory".to_string());
    let snapshot = snapshot_with_metadata(
        InventorySnapshot {
            schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
            workspace_id: workspace.id.clone(),
            saved_at: "1".to_string(),
            metadata: Default::default(),
            stale_reasons: Vec::new(),
            links: Vec::new(),
            items: vec![item(
                "db:table:orders",
                "table",
                "orders",
                "data",
                "db",
                None,
                Some("main"),
            )],
        },
        &workspace,
        &test_registry(),
    );

    fs::write(
        ddl.join("orders.sql"),
        "create table orders(id integer primary key, status text);\n",
    )
    .unwrap();
    let stale = mark_snapshot_staleness(snapshot, &workspace, &test_registry());

    assert!(stale
        .stale_reasons
        .iter()
        .any(|reason| reason.contains("DB 파일")));
    fs::remove_dir_all(root).unwrap();
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
    assert_eq!(snapshot.links[0].truth_class, "unknown");
    assert_eq!(snapshot.links[0].engine_edge_type.as_deref(), Some("CALLS"));
    assert!(snapshot
        .metadata
        .gaps
        .iter()
        .any(|gap| gap.kind == "dangling-relationship"));
    assert!(snapshot
        .metadata
        .gaps
        .iter()
        .any(|gap| gap.kind == "unscored-code-call"));
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
                dependents: Vec::new(),
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
                dependents: Vec::new(),
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
fn normalize_inventory_escapes_delimiters_without_changing_ordinary_ids() {
    let table = |schema: Option<&str>, name: &str, column: &str| DbInventoryTable {
        key: None,
        database: None,
        schema: schema.map(str::to_string),
        name: name.to_string(),
        columns: vec![DbInventoryColumn {
            key: None,
            table_key: None,
            name: column.to_string(),
            data_type: None,
            nullable: None,
            is_primary_key: false,
            is_foreign_key: false,
        }],
        foreign_keys: Vec::new(),
        inbound_foreign_keys: Vec::new(),
        constraints: Vec::new(),
        indexes: Vec::new(),
        dependents: Vec::new(),
    };
    let mut tables = vec![
        table(Some("audit.2026"), "order:events", "value:raw%text"),
        table(Some("audit"), "2026.order:events", "value"),
        table(Some("public"), "orders", "id"),
    ];
    tables[0].foreign_keys.push(DbForeignKey {
        key: None,
        name: Some("event:value-fkey".to_string()),
        table_key: None,
        table_schema: Some("audit.2026".to_string()),
        table: Some("order:events".to_string()),
        columns: vec!["value:raw%text".to_string()],
        column_keys: Vec::new(),
        referenced_table_key: None,
        referenced_schema: Some("audit".to_string()),
        referenced_table: "2026.order:events".to_string(),
        referenced_columns: vec!["value".to_string()],
        referenced_column_keys: Vec::new(),
    });
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
        tables,
    };

    let snapshot = normalize_inventory("workspace-1".to_string(), None, Some(&db));
    let ids = snapshot
        .items
        .iter()
        .map(|item| item.id.as_str())
        .collect::<BTreeSet<_>>();

    assert!(ids.contains("db:table:audit%2E2026.order%3Aevents"));
    assert!(ids.contains("db:table:audit.2026%2Eorder%3Aevents"));
    assert!(ids.contains("db:column:audit%2E2026.order%3Aevents:value%3Araw%25text"));
    assert!(ids.contains("db:table:public.orders"));
    assert!(ids.contains("db:column:public.orders:id"));
    assert_eq!(
        snapshot
            .items
            .iter()
            .find(|item| item.id == "db:table:audit%2E2026.order%3Aevents")
            .and_then(|item| item.qualified_name.as_deref()),
        Some("audit.2026.order:events")
    );
    assert!(snapshot.links.iter().any(|link| {
        link.kind == "db_fk"
            && link.from == "db:column:audit%2E2026.order%3Aevents:value%3Araw%25text"
            && link.to == "db:column:audit.2026%2Eorder%3Aevents:value"
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
                dependents: Vec::new(),
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
                dependents: Vec::new(),
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
fn db_dependents_round_trip_with_confirmed_endpoints_and_explicit_gaps() {
    let snapshot = dependent_object_snapshot();
    let by_name = |name: &str| {
        snapshot
            .items
            .iter()
            .find(|item| item.name == name)
            .unwrap()
    };
    let view = by_name("active_orders");
    let trigger = by_name("trg_orders_status");
    let routine = by_name("refresh_orders");
    let missing_view = by_name("missing_column_view");

    assert_eq!(view.kind, "view");
    assert_eq!(view.parent_id, None);
    assert_eq!(trigger.parent_id.as_deref(), Some("db:table:public.orders"));
    assert_eq!(routine.kind, "routine");
    assert!(snapshot.links.iter().any(|link| {
        link.kind == "db_dependency"
            && link.from == view.id
            && link.to == "db:column:public.orders:status"
            && link.truth_class == "confirmed"
            && link.engine_edge_type.as_deref() == Some("VIEW_DEPENDS_ON_COLUMN")
            && link.evidence.iter().any(|evidence| {
                evidence.kind == "db-object-key"
                    && evidence.text == "sqlite:shop:main:public:view:active_orders"
            })
    }));
    assert!(snapshot.links.iter().any(|link| {
        link.kind == "db_trigger"
            && link.from == "db:table:public.orders"
            && link.to == trigger.id
            && link.truth_class == "confirmed"
    }));
    assert!(snapshot.links.iter().any(|link| {
        link.kind == "db_dependency"
            && link.from == routine.id
            && link.to == "db:table:public.orders"
            && link.engine_edge_type.as_deref() == Some("ROUTINE_DEPENDS_ON_TABLE")
    }));
    assert!(snapshot.metadata.gaps.iter().any(|gap| {
        gap.kind == "db-dependent-missing-column"
            && gap.related_ids.iter().any(|id| id == &missing_view.id)
    }));
    assert!(snapshot.links.iter().any(|link| {
        link.kind == "db_dependency"
            && link.from == missing_view.id
            && link.to == "db:table:public.orders"
            && link.truth_class == "structural"
            && link.engine_edge_type.as_deref() == Some("DEPENDENCY_SCOPE")
    }));
}

#[test]
fn db_dependents_are_scoped_into_table_and_column_impact_views() {
    let snapshot = dependent_object_snapshot();
    let table_map = visual_map(
        &snapshot,
        Some("db:table:public.orders".to_string()),
        "table-usage".to_string(),
    );
    let column_map = visual_map(
        &snapshot,
        Some("db:column:public.orders:status".to_string()),
        "column-impact".to_string(),
    );

    for kind in ["view", "trigger", "routine"] {
        assert!(
            table_map.nodes.iter().any(|node| node.kind == kind),
            "table map omitted {kind}"
        );
        assert!(
            review_lane(table_map.review_board.as_ref().unwrap(), "direct")
                .items
                .iter()
                .any(|item| item.kind == kind),
            "table review omitted {kind}"
        );
    }
    assert!(column_map
        .nodes
        .iter()
        .any(|node| node.kind == "view" && node.title == "active_orders"));
    assert!(!column_map
        .nodes
        .iter()
        .any(|node| matches!(node.kind.as_str(), "trigger" | "routine")));
    assert!(column_map
        .edges
        .iter()
        .any(|edge| edge.kind == "db_dependency"));
    let column_direct = review_lane(column_map.review_board.as_ref().unwrap(), "direct");
    assert!(column_direct.items.iter().any(|item| item.kind == "view"));
    assert!(!column_direct
        .items
        .iter()
        .any(|item| matches!(item.kind.as_str(), "trigger" | "routine")));
}

#[test]
fn db_capability_warnings_are_localized_preserved_and_visible_after_restore() {
    let db = DbInventory {
        profile_id: "profile-1".to_string(),
        snapshot_key: Some("postgres:shop".to_string()),
        contract_version: Some("1".to_string()),
        capability_warnings: vec![
            "cross-object dependency metadata is partially tracked by the postgres adapter."
                .to_string(),
            "view dependency metadata is not tracked by the mysql adapter.".to_string(),
            "routine dependency metadata support is unknown for the oracle adapter.".to_string(),
            "SQLite generated columns are identified, but generation expressions are not extracted."
                .to_string(),
            "adapter-specific warning".to_string(),
        ],
        limit_requested: None,
        limit_applied: None,
        limit_clamped: None,
        result_count: Some(0),
        total_tables: Some(0),
        truncated: Some(false),
        gaps: Vec::new(),
        tables: Vec::new(),
    };
    let snapshot = normalize_inventory("workspace-capabilities".to_string(), None, Some(&db));
    let messages = snapshot
        .metadata
        .gaps
        .iter()
        .map(|gap| gap.message.as_str())
        .collect::<BTreeSet<_>>();

    assert!(messages.contains("PostgreSQL 어댑터는 객체 간 의존성 메타데이터를 일부만 추적합니다."));
    assert!(messages.contains("MySQL/MariaDB 어댑터는 뷰 의존성 메타데이터를 추적하지 않습니다."));
    assert!(messages.contains(
        "Oracle 어댑터의 프로시저·함수 의존성 메타데이터 지원 여부를 확인할 수 없습니다."
    ));
    assert!(messages.contains("SQLite 생성 열 여부는 식별하지만 생성식은 수집하지 않습니다."));
    assert!(messages.contains("adapter-specific warning"));

    let overview = visual_map(&snapshot, None, "atlas".to_string());
    assert!(!overview
        .warnings
        .iter()
        .any(|warning| warning.contains("DB 지원 범위")));

    let root = temp_root("capability-warning-restore");
    save_inventory_snapshot(&root, &snapshot).unwrap();
    let restored = load_inventory_snapshot(&root, "workspace-capabilities").unwrap();
    assert_eq!(restored.metadata.gaps, snapshot.metadata.gaps);
    fs::remove_dir_all(root).unwrap();
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
        contract_version: Some("2".to_string()),
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
                dependents: Vec::new(),
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
                dependents: Vec::new(),
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
fn single_column_candidates_require_the_parent_table_context() {
    let mut snapshot = fixture_inventory("workspace-1".to_string());
    let column = snapshot
        .items
        .iter_mut()
        .find(|item| item.id == "db:column:orders:customer_id")
        .unwrap();
    column.name = "status".to_string();
    snapshot.items.push(item(
        "code:function:status-only",
        "function",
        "resolveStatusText",
        "code",
        "code",
        None,
        Some("src/runtime/status.ts"),
    ));
    snapshot.items.push(item(
        "code:function:order-status",
        "function",
        "loadOrderStatus",
        "code",
        "code",
        None,
        Some("src/orders/status.ts"),
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

    assert!(candidate_sources.contains("code:function:order-status"));
    assert!(!candidate_sources.contains("code:function:status-only"));
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
fn change_impact_checks_follow_the_selected_change_scenario() {
    let snapshot = fixture_inventory("workspace-1".to_string());
    let rename_map = visual_map_with_change(
        &snapshot,
        Some("db:column:orders:customer_id".to_string()),
        "column-impact".to_string(),
        Some(ChangeIntent {
            kind: "rename".to_string(),
            value: Some("buyer_id".to_string()),
        }),
    );
    let rename_board = rename_map.review_board.as_ref().unwrap();
    assert_eq!(rename_board.change_intent.as_ref().unwrap().kind, "rename");
    assert!(rename_board.markdown_summary.contains("buyer_id"));
    assert!(review_lane(rename_board, "checks")
        .items
        .iter()
        .any(|item| item.id == "check:change:rename-target"));

    let drop_map = visual_map_with_change(
        &snapshot,
        Some("db:column:orders:customer_id".to_string()),
        "column-impact".to_string(),
        Some(ChangeIntent {
            kind: "drop".to_string(),
            value: None,
        }),
    );
    let drop_board = drop_map.review_board.as_ref().unwrap();
    assert!(drop_board.markdown_summary.contains("컬럼 삭제"));
    assert!(review_lane(drop_board, "checks")
        .items
        .iter()
        .any(|item| item.id == "check:change:drop-data"));
}

#[test]
fn table_usage_keeps_confirmed_code_access_ahead_of_structural_overflow() {
    let mut snapshot = fixture_inventory("workspace-1".to_string());
    for index in 0..20 {
        append_structural_review_constraint(
            &mut snapshot,
            &format!("orders_extra_check_{index}"),
            "check",
        );
    }
    snapshot.links.push(confirmed_api_link(
        "reads:order-service->orders",
        "code:class:OrderService",
        "db:table:orders",
        "code_db_read",
        "READS",
    ));

    let map = visual_map(
        &snapshot,
        Some("db:table:orders".to_string()),
        "table-usage".to_string(),
    );
    let direct = review_lane(map.review_board.as_ref().unwrap(), "direct");

    assert_eq!(direct.items[0].kind, "code_db_read");
    assert_eq!(
        direct.items[0].node_id.as_deref(),
        Some("code:class:OrderService")
    );
    assert!(direct.hidden > 0);
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
            id: format!("gap:db-inventory:{index}"),
            kind: "db-inventory-gap".to_string(),
            message: format!("inventory item {index} unavailable"),
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
fn api_flow_keeps_http_method_separate_from_the_route_path() {
    let mut snapshot = fixture_inventory("workspace-1".to_string());
    let route = snapshot
        .items
        .iter_mut()
        .find(|item| item.id == "code:route:orders:create")
        .unwrap();
    route.name = "/{session_id}".to_string();
    route.qualified_name = Some("__route__DELETE__/{session_id}".to_string());

    let answer = visual_map(
        &snapshot,
        Some("code:route:orders:create".to_string()),
        "api-flow".to_string(),
    )
    .api_reading
    .unwrap();

    assert_eq!(answer.method.as_deref(), Some("DELETE"));
    assert_eq!(answer.subject, "/{session_id}");
}

#[test]
fn api_flow_prefers_confirmed_static_sql_over_a_db_candidate() {
    let mut snapshot = fixture_inventory("workspace-1".to_string());
    snapshot.links.push(confirmed_api_link(
        "reads:order-service->orders",
        "code:class:OrderService",
        "db:table:orders",
        "code_db_read",
        "READS",
    ));

    let map = visual_map(
        &snapshot,
        Some("code:route:orders:create".to_string()),
        "api-flow".to_string(),
    );
    let answer = map.api_reading.as_ref().unwrap();

    assert_eq!(answer.db_relations.len(), 1);
    assert_eq!(
        answer.db_relations[0].node_id.as_deref(),
        Some("db:table:orders")
    );
    assert!(answer
        .db_candidates
        .iter()
        .all(|candidate| candidate.node_id.as_deref() != Some("db:table:orders")));
    assert!(map
        .edges
        .iter()
        .any(|edge| edge.kind == "code_db_read" && edge.to == "db:table:orders"));
    assert!(!answer.unknowns.iter().any(|item| item.kind == "db-gap"));
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
        source_revision: None,
        source_revision_label: None,
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
fn api_flow_treats_db_capability_as_fixed_scope_not_a_reindex_failure() {
    let mut snapshot = fixture_inventory("workspace-1".to_string());
    for item in snapshot
        .items
        .iter_mut()
        .filter(|item| item.kind == "table")
    {
        item.name = format!("unrelated_{}", item.name);
    }
    snapshot.metadata.db = Some(super::model::SnapshotSourceMetadata {
        saved_at: "1".to_string(),
        engine_id: Some("database-memory".to_string()),
        engine_version: Some("1".to_string()),
        engine_checksum: None,
        contract_version: Some("1".to_string()),
        snapshot_key: Some("ddl:test".to_string()),
        limit_requested: Some(1000),
        limit_applied: Some(1000),
        limit_clamped: Some(false),
        result_count: Some(2),
        total_tables: Some(2),
        truncated: Some(false),
        source_revision: None,
        source_revision_label: None,
        source_path: None,
        source_type: "ddl-sqlite".to_string(),
        profile_id: Some("test".to_string()),
    });
    snapshot.metadata.gaps.extend([
        super::model::SnapshotGap {
            id: "gap:db-capability:views".to_string(),
            kind: "db-capability".to_string(),
            message: "뷰 의존성을 추적하지 않습니다.".to_string(),
            related_ids: Vec::new(),
        },
        super::model::SnapshotGap {
            id: "gap:db-capability:triggers".to_string(),
            kind: "db-capability".to_string(),
            message: "트리거 의존성을 추적하지 않습니다.".to_string(),
            related_ids: Vec::new(),
        },
    ]);

    let answer = visual_map(
        &snapshot,
        Some("code:route:orders:create".to_string()),
        "api-flow".to_string(),
    )
    .api_reading
    .unwrap();
    let capabilities = answer
        .unknowns
        .iter()
        .filter(|item| item.kind == "db-capability")
        .collect::<Vec<_>>();

    assert_eq!(capabilities.len(), 1);
    assert_eq!(capabilities[0].evidence.len(), 2);
    assert!(capabilities[0].detail.contains("2종"));
    assert!(!capabilities[0].detail.contains("지원하지 않습니다"));
    assert!(capabilities[0].detail.contains("다시 읽어도"));
    assert!(!answer
        .recommended_checks
        .iter()
        .any(|item| item.kind == "reindex"));
    assert_eq!(answer.recommended_checks[0].kind, "db-source-scope");
}

#[test]
fn impact_groups_db_capability_limits_without_polluting_the_overview() {
    let mut snapshot = dependent_object_snapshot();
    snapshot.metadata.gaps.extend([
        super::model::SnapshotGap {
            id: "gap:db-capability:views".to_string(),
            kind: "db-capability".to_string(),
            message: "뷰 의존성을 추적하지 않습니다.".to_string(),
            related_ids: Vec::new(),
        },
        super::model::SnapshotGap {
            id: "gap:db-capability:triggers".to_string(),
            kind: "db-capability".to_string(),
            message: "트리거 의존성을 추적하지 않습니다.".to_string(),
            related_ids: Vec::new(),
        },
    ]);

    let map = visual_map(
        &snapshot,
        Some("db:column:public.orders:status".to_string()),
        "column-impact".to_string(),
    );
    let capabilities = review_lane(map.review_board.as_ref().unwrap(), "unknowns")
        .items
        .iter()
        .filter(|item| item.kind == "db-capability")
        .collect::<Vec<_>>();

    assert_eq!(capabilities.len(), 1);
    assert_eq!(capabilities[0].evidence.len(), 2);
    assert!(capabilities[0].detail.contains("2종"));

    let overview = visual_map(&snapshot, None, "atlas".to_string());
    assert!(!overview
        .warnings
        .iter()
        .any(|warning| warning.contains("DB 지원 범위")));
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
        && step.lane == "repository-query"
        && step.lane_basis == "name-inferred"));
    assert!(answer
        .steps
        .iter()
        .any(|step| step.lane == "handler" && step.lane_basis == "confirmed-handles"));
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
fn focus_map_does_not_show_untrusted_call_neighbors_without_an_edge() {
    let mut snapshot = fixture_inventory("workspace-1".to_string());
    snapshot
        .links
        .iter_mut()
        .find(|link| link.kind == "code_call")
        .unwrap()
        .truth_class = "unknown".to_string();

    let map = visual_map(
        &snapshot,
        Some("code:function:CreateOrderHandler".to_string()),
        "search-focus".to_string(),
    );

    assert!(!map
        .nodes
        .iter()
        .any(|node| node.id == "code:class:OrderService"));
    assert!(!map.edges.iter().any(|edge| edge.kind == "code_call"));
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
        .any(|warning| warning.contains("보조 그룹")));
    assert!(map
        .edges
        .iter()
        .all(|edge| node_ids.contains(edge.from.as_str()) && node_ids.contains(edge.to.as_str())));
    assert_eq!(map, visual_map(&snapshot, None, "atlas".to_string()));
}

#[test]
fn atlas_prefers_engine_packages_and_db_schemas_over_name_groups() {
    let mut snapshot = fixture_inventory("workspace-1".to_string());
    snapshot.metadata.architecture = Some(serde_json::json!({
        "packages": [
            { "name": "server", "node_count": 12 },
            { "name": "app", "node_count": 40 }
        ]
    }));
    for code in snapshot
        .items
        .iter_mut()
        .filter(|item| item.source == "code")
    {
        code.group_id = Some("server.app.orders".to_string());
    }

    let map = visual_map(&snapshot, None, "atlas".to_string());

    assert!(map.nodes.iter().any(|node| node.id == "group:package:app"));
    assert!(map
        .nodes
        .iter()
        .any(|node| node.id == "group:db-schema:public" && node.title == "DB · public"));
    assert!(!map
        .nodes
        .iter()
        .any(|node| node.id.starts_with("group:domain:")));
    assert!(map
        .warnings
        .iter()
        .any(|warning| warning.contains("코드 엔진 패키지와 DB 스키마")));
}

#[test]
fn atlas_overview_uses_primary_code_symbols_only() {
    let mut function = item(
        "code:function:process-order",
        "function",
        "processOrder",
        "code",
        "code",
        None,
        Some("src/app/orders.rs"),
    );
    let mut field = item(
        "code:field:order-id",
        "field",
        "order_id",
        "code",
        "code",
        None,
        Some("src/app/orders.rs"),
    );
    let mut decorator = item(
        "code:decorator:command",
        "decorator",
        "#[tauri::command]",
        "code",
        "code",
        None,
        Some("src/app/orders.rs"),
    );
    for value in [&mut function, &mut field, &mut decorator] {
        value.group_id = Some("app".to_string());
    }
    let snapshot = InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: "workspace-1".to_string(),
        saved_at: "1".to_string(),
        metadata: super::model::SnapshotMetadata {
            architecture: Some(serde_json::json!({ "packages": ["app"] })),
            ..Default::default()
        },
        stale_reasons: Vec::new(),
        links: Vec::new(),
        items: vec![function, field, decorator],
    };

    let map = visual_map(&snapshot, None, "atlas".to_string());
    let app = map
        .nodes
        .iter()
        .find(|node| node.id == "group:package:app")
        .unwrap();
    let subtitle = app.subtitle.as_deref().unwrap();

    assert!(subtitle.starts_with("API 0 · 코드 1 · DB 0|"));
    assert!(subtitle.contains("processOrder"));
    assert!(!subtitle.contains("order_id"));
    assert!(!subtitle.contains("tauri::command"));
    assert!(map.warnings.iter().any(|warning| {
        warning.contains("하위 코드 심벌 2개") && warning.contains("코드 검색에 보존")
    }));
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
    let full_payload_bytes = serde_json::to_vec(&restored).unwrap().len();
    let bootstrap_started = Instant::now();
    let bootstrap = inventory_bootstrap(&restored);
    let bootstrap_elapsed = bootstrap_started.elapsed();
    let bootstrap_payload_bytes = serde_json::to_vec(&bootstrap).unwrap().len();
    assert_eq!(bootstrap.summary.total_items, 10_200);
    assert_eq!(bootstrap.summary.total_links, 40_000);
    assert_eq!(
        bootstrap
            .snapshot
            .items
            .iter()
            .filter(|item| item.source == "code")
            .count(),
        100
    );
    assert!(
        bootstrap_payload_bytes * 5 < full_payload_bytes,
        "bounded bootstrap payload must stay below 20% of the full snapshot"
    );

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
        "large_snapshot metrics: items={}, edges={}, restore_ms={}, bootstrap_ms={}, payload_bytes={}->{}, overview_ms={}, focus_ms={}",
        snapshot.items.len(),
        snapshot.links.len(),
        restore_elapsed.as_millis(),
        bootstrap_elapsed.as_millis(),
        full_payload_bytes,
        bootstrap_payload_bytes,
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
#[ignore = "manual 10k/50k/100k projection performance matrix"]
fn projection_scale_matrix_covers_10k_50k_and_100k_items() {
    for item_count in [10_000, 50_000, 100_000] {
        let snapshot = projection_scale_snapshot(item_count);
        let overview_started = Instant::now();
        let overview = visual_map(&snapshot, None, "atlas".to_string());
        let overview_elapsed = overview_started.elapsed();
        let overview_ids = overview
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<std::collections::HashSet<_>>();

        let focus_started = Instant::now();
        let focused = visual_map(
            &snapshot,
            Some("code:function:domain_0:0".to_string()),
            "search-focus".to_string(),
        );
        let focus_elapsed = focus_started.elapsed();
        let focused_ids = focused
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<std::collections::HashSet<_>>();

        let composition_started = Instant::now();
        let composition = composition_map(
            &snapshot,
            vec![
                "code:function:domain_0:0".to_string(),
                "code:function:domain_8:8".to_string(),
            ],
            "calls",
        )
        .unwrap();
        let composition_elapsed = composition_started.elapsed();
        let composition_ids = composition
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<std::collections::HashSet<_>>();

        eprintln!(
            "projection_scale items={item_count} links={} overview_ms={} focus_ms={} composition_ms={}",
            snapshot.links.len(),
            overview_elapsed.as_millis(),
            focus_elapsed.as_millis(),
            composition_elapsed.as_millis()
        );
        assert!(overview.nodes.len() <= 40);
        assert!(overview.edges.len() <= 80);
        assert!(overview.edges.iter().all(|edge| {
            overview_ids.contains(edge.from.as_str()) && overview_ids.contains(edge.to.as_str())
        }));
        assert!(focused.nodes.len() <= 36);
        assert!(focused.edges.iter().all(|edge| {
            focused_ids.contains(edge.from.as_str()) && focused_ids.contains(edge.to.as_str())
        }));
        assert!(composition.nodes.len() <= 40);
        assert!(composition.edges.len() <= 80);
        assert!(composition.edges.iter().all(|edge| {
            composition_ids.contains(edge.from.as_str())
                && composition_ids.contains(edge.to.as_str())
        }));
        assert!(overview_elapsed < Duration::from_secs(30));
        assert!(focus_elapsed < Duration::from_secs(30));
        assert!(composition_elapsed < Duration::from_secs(30));
    }
}

fn projection_scale_snapshot(item_count: usize) -> InventorySnapshot {
    let items = (0..item_count)
        .map(|index| {
            let domain = index % 64;
            item(
                &format!("code:function:domain_{domain}:{index}"),
                "function",
                &format!("domain_{domain}_function_{index}"),
                "code",
                "code",
                None,
                Some(&format!("src/domain_{domain}/service.rs")),
            )
        })
        .collect::<Vec<_>>();
    let links = (0..item_count.saturating_sub(1))
        .map(|index| {
            let from_domain = index % 64;
            let to = index + 1;
            let to_domain = to % 64;
            super::model::SnapshotLink {
                id: format!("scale-call:{index}"),
                from: format!("code:function:domain_{from_domain}:{index}"),
                to: format!("code:function:domain_{to_domain}:{to}"),
                kind: "code_call".to_string(),
                label: Some("CALLS".to_string()),
                truth_class: "confirmed".to_string(),
                direction: "outbound".to_string(),
                engine_edge_type: Some("CALLS".to_string()),
                evidence: Vec::new(),
            }
        })
        .collect();

    InventorySnapshot {
        schema_version: super::model::SNAPSHOT_SCHEMA_VERSION,
        workspace_id: format!("scale-{item_count}"),
        saved_at: "1".to_string(),
        metadata: Default::default(),
        stale_reasons: Vec::new(),
        links,
        items,
    }
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
fn capped_search_focus_map_always_keeps_the_requested_target() {
    let focus_id = "code:function:zz_target";
    let mut focus_item = item(
        focus_id,
        "function",
        "zz_target",
        "code",
        "code",
        None,
        Some("src/target.ts"),
    );
    focus_item.location = Some(super::model::SourceLocation {
        path: "src/target.ts".to_string(),
        line: Some(42),
        column: Some(3),
        end_line: Some(48),
        end_column: None,
    });
    let mut items = vec![focus_item];
    for index in 0..50 {
        items.push(item(
            &format!("code:function:aa_neighbor_{index:02}"),
            "function",
            &format!("aa_neighbor_{index:02}"),
            "code",
            "code",
            Some(focus_id),
            Some("src/neighbors.ts"),
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
        Some(focus_id.to_string()),
        "search-focus".to_string(),
    );

    assert_eq!(map.nodes.len(), 32);
    assert!(map.nodes.iter().any(|node| node.id == focus_id));
    assert!(map
        .nodes
        .iter()
        .find(|node| node.id == focus_id)
        .and_then(|node| node.location.as_ref())
        .is_some_and(|location| location.line == Some(42) && location.column == Some(3)));
    assert!(map.edges.iter().all(|edge| {
        map.nodes.iter().any(|node| node.id == edge.from)
            && map.nodes.iter().any(|node| node.id == edge.to)
    }));
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
        repo_source: crate::workspace::RepoSource::Local,
        repo_origin: None,
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
                expected_version: "0.9.0".to_string(),
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
