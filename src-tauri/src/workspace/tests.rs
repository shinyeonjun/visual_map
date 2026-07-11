use super::*;
use crate::{base_paths, engine, EngineRegistry};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn temp_root(name: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("backend-visual-map-{name}-{}", std::process::id()));

    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }

    root
}

fn test_db_profile(source: DbSource, id: &str, path: Option<&str>) -> DbProfile {
    DbProfile {
        id: id.to_string(),
        name: id.to_string(),
        source,
        path: path.map(str::to_string),
        host: None,
        port: None,
        database: None,
        username: None,
        cache_path: format!(r"db\{id}\graph.sqlite"),
        last_indexed_at: None,
        password_stored: false,
    }
}

fn assert_db_index_args(
    source: DbSource,
    cli_source: &str,
    path: Option<&str>,
    connection_string: Option<&str>,
    expected_flag: &str,
    expected_value: &str,
) {
    let profile = test_db_profile(source, "local-db", path);
    let args = db_index_args(
        &profile,
        Path::new(r"D:\cache\graph.sqlite"),
        connection_string,
    )
    .unwrap();

    assert_eq!(args[0], "index");
    assert!(args.contains(&"--format".to_string()));
    assert!(args.contains(&"json".to_string()));
    assert!(args.contains(&"--source".to_string()));
    assert!(args.contains(&cli_source.to_string()));
    assert!(args.contains(&expected_flag.to_string()));
    assert!(args.contains(&expected_value.to_string()));
    assert!(args.contains(&"--alias".to_string()));
    assert!(args.contains(&"local-db".to_string()));
    assert!(args.contains(&"--cache-path".to_string()));
    assert!(!args.contains(&"--cache".to_string()));
}

#[test]
fn workspace_id_is_unique_for_fast_same_name_calls() {
    let ids = (0..1024)
        .map(|_| workspace_id("Shop API"))
        .collect::<Vec<_>>();
    let unique = ids.iter().collect::<std::collections::HashSet<_>>();

    assert_eq!(unique.len(), ids.len());
    assert!(ids.iter().all(|id| validate_workspace_id(id).is_ok()));
}

#[test]
fn workspace_serializes_with_camel_case_contract() {
    let workspace = Workspace {
        id: "shop-api-1".to_string(),
        name: "shop-api".to_string(),
        repo_path: r"D:\projects\shop-api".to_string(),
        code_project: Some("shop-api".to_string()),
        engine_cache: WorkspaceEngineCache::default(),
        db_profiles: vec![DbProfile {
            id: "local-postgres".to_string(),
            name: "local".to_string(),
            source: DbSource::Postgres,
            path: None,
            host: Some("localhost".to_string()),
            port: Some(5432),
            database: Some("shop".to_string()),
            username: Some("app".to_string()),
            cache_path: r"db\local-postgres\graph.sqlite".to_string(),
            last_indexed_at: Some("123".to_string()),
            password_stored: false,
        }],
        active_db_profile_id: Some("local-postgres".to_string()),
        created_at: "1".to_string(),
        updated_at: "2".to_string(),
    };

    let json = serde_json::to_string(&workspace).unwrap();

    assert!(json.contains("\"repoPath\""));
    assert!(json.contains("\"dbProfiles\""));
    assert!(json.contains("\"engineCache\""));
    assert!(json.contains("\"activeDbProfileId\""));
    assert!(json.contains("\"passwordStored\":false"));
    assert!(json.contains("\"source\":\"postgres\""));
}

#[test]
fn create_open_and_list_workspace_round_trip() {
    let root = temp_root("workspace-round-trip");

    let created = create_workspace(
        &root,
        CreateWorkspaceRequest {
            name: "Shop API".to_string(),
            repo_path: r"D:\projects\shop-api".to_string(),
        },
    )
    .unwrap();

    let opened = open_workspace(&root, &created.id).unwrap();
    let listed = list_workspaces(&root).unwrap();

    assert_eq!(opened, created);
    assert_eq!(listed, vec![created.clone()]);
    assert!(validate_workspace_id(&created.id).is_ok());
    assert!(base_paths(&root)
        .workspaces_dir
        .join(&created.id)
        .join("workspace.json")
        .is_file());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn github_repo_name_accepts_supported_urls() {
    assert_eq!(
        github_repo_name("https://github.com/openai/codex.git"),
        Some("codex".to_string())
    );
    assert_eq!(
        github_repo_name("git@github.com:openai/codex.git"),
        Some("codex".to_string())
    );
}

#[test]
fn github_repo_name_rejects_unsupported_urls() {
    assert_eq!(github_repo_name("http://github.com/openai/codex"), None);
    assert_eq!(github_repo_name("https://example.com/openai/codex"), None);
    assert_eq!(
        github_repo_name("https://github.com/openai/codex/tree/main"),
        None
    );
    assert_eq!(github_repo_name("https://github.com/openai/../codex"), None);
}

#[test]
fn workspace_repo_dir_is_workspace_scoped() {
    let root = Path::new("workspaces");
    let path = workspace_repo_dir(root, "workspace-1");

    assert_eq!(path, root.join("workspace-1").join("repo"));
}

#[test]
fn open_workspace_rejects_path_traversal_id() {
    let root = temp_root("workspace-id-traversal");
    let result = open_workspace(&root, "../x");

    assert_eq!(
        result.unwrap_err(),
        "프로젝트 ID에 허용되지 않는 문자가 있습니다"
    );
}

#[test]
fn open_workspace_rejects_workspace_file_id_mismatch() {
    let root = temp_root("workspace-id-mismatch");
    let mut created = create_workspace(
        &root,
        CreateWorkspaceRequest {
            name: "Shop API".to_string(),
            repo_path: r"D:\projects\shop-api".to_string(),
        },
    )
    .unwrap();
    let original_id = created.id.clone();
    let workspace_file = base_paths(&root)
        .workspaces_dir
        .join(&original_id)
        .join("workspace.json");
    created.id = "other-workspace".to_string();
    fs::write(
        &workspace_file,
        serde_json::to_string_pretty(&created).unwrap(),
    )
    .unwrap();

    let error = open_workspace(&root, &original_id).unwrap_err();
    let listed = list_workspaces(&root).unwrap();
    let warnings = super::store::workspace_recovery_warnings(&root).unwrap();

    assert_eq!(error, "프로젝트 파일 ID가 경로와 일치하지 않습니다");
    assert!(listed.is_empty());
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].workspace_id, original_id);
    assert_eq!(warnings[0].kind, "unrecoverable");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn open_workspace_rejects_empty_id() {
    let root = temp_root("workspace-id-empty");
    let result = open_workspace(&root, "");

    assert_eq!(result.unwrap_err(), "프로젝트 ID가 필요합니다");
}

#[test]
fn save_db_profile_updates_workspace_without_password_storage() {
    let root = temp_root("db-profile");
    let created = create_workspace(
        &root,
        CreateWorkspaceRequest {
            name: "Shop API".to_string(),
            repo_path: r"D:\projects\shop-api".to_string(),
        },
    )
    .unwrap();

    let updated = save_db_profile(
        &root,
        SaveDbProfileRequest {
            workspace_id: created.id.clone(),
            name: "Local DDL".to_string(),
            source: DbSource::DdlSqlite,
            path: Some(r"D:\schemas\shop.sql".to_string()),
        },
    )
    .unwrap();

    assert_eq!(updated.db_profiles.len(), 1);
    assert_eq!(
        updated.active_db_profile_id,
        Some(updated.db_profiles[0].id.clone())
    );
    assert!(!updated.db_profiles[0].password_stored);
    assert!(updated.db_profiles[0]
        .cache_path
        .starts_with(r"engines\database-memory\0.1.1\contract-1\profiles\"));
    assert!(updated.db_profiles[0]
        .cache_path
        .ends_with(r"\graph.sqlite"));

    let json = fs::read_to_string(
        base_paths(&root)
            .workspaces_dir
            .join(&created.id)
            .join("workspace.json"),
    )
    .unwrap();
    assert!(!json.to_ascii_lowercase().contains("password\":"));
    assert!(json.contains("\"passwordStored\": false"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn save_network_db_profile_does_not_persist_connection_string_shape() {
    let root = temp_root("network-db-profile");
    let created = create_workspace(
        &root,
        CreateWorkspaceRequest {
            name: "Shop API".to_string(),
            repo_path: r"D:\projects\shop-api".to_string(),
        },
    )
    .unwrap();
    let fixture_secret = "postgres://app:fixture_password@localhost/shop";

    let updated = save_db_profile(
        &root,
        SaveDbProfileRequest {
            workspace_id: created.id.clone(),
            name: "Local Postgres".to_string(),
            source: DbSource::Postgres,
            path: Some(fixture_secret.to_string()),
        },
    )
    .unwrap();

    assert_eq!(updated.db_profiles.len(), 1);
    assert_eq!(updated.db_profiles[0].source, DbSource::Postgres);
    assert_eq!(updated.db_profiles[0].path, None);
    assert!(!updated.db_profiles[0].password_stored);

    let json = fs::read_to_string(
        base_paths(&root)
            .workspaces_dir
            .join(&created.id)
            .join("workspace.json"),
    )
    .unwrap();
    assert!(!json.contains("fixture_password"));
    assert!(!json.contains(fixture_secret));
    assert!(json.contains("\"source\": \"postgres\""));
    assert!(json.contains("\"passwordStored\": false"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn db_inventory_extracts_tables_and_columns_from_engine_json() {
    let tables = serde_json::json!({
        "tables": [
            { "schema": "public", "name": "orders" }
        ]
    });
    let columns = serde_json::json!({
        "columns": [
            {
                "tableName": "orders",
                "columnName": "id",
                "dataType": "bigint",
                "isPrimaryKey": true
            },
            {
                "tableName": "orders",
                "columnName": "customer_id",
                "dataType": "bigint",
                "isForeignKey": true
            }
        ]
    });

    let inventory = extract_db_inventory("profile-1".to_string(), &tables, &columns);

    assert_eq!(inventory.tables.len(), 1);
    assert_eq!(inventory.tables[0].name, "orders");
    assert_eq!(inventory.tables[0].columns.len(), 2);
    assert!(inventory.tables[0].columns[0].is_primary_key);
    assert!(inventory.tables[0].columns[1].is_foreign_key);
}

#[test]
fn db_inventory_attaches_columns_to_matching_schema_table() {
    let tables = serde_json::json!({
        "tables": [
            { "schema": "public", "name": "users" },
            { "schema": "audit", "name": "users" }
        ]
    });
    let columns = serde_json::json!({
        "columns": [
            { "schema": "public", "tableName": "users", "columnName": "id" },
            { "schema": "audit", "tableName": "users", "columnName": "event_id" }
        ]
    });

    let inventory = extract_db_inventory("profile-1".to_string(), &tables, &columns);
    let public = inventory
        .tables
        .iter()
        .find(|table| table.schema.as_deref() == Some("public"))
        .unwrap();
    let audit = inventory
        .tables
        .iter()
        .find(|table| table.schema.as_deref() == Some("audit"))
        .unwrap();

    assert_eq!(public.columns[0].name, "id");
    assert_eq!(audit.columns[0].name, "event_id");
}

#[test]
fn db_index_args_match_database_memory_cli_contract() {
    let profile = test_db_profile(
        DbSource::DdlSqlite,
        "local-ddl",
        Some(r"D:\schemas\shop.sql"),
    );
    let args = db_index_args(&profile, Path::new(r"D:\cache\graph.sqlite"), None).unwrap();

    assert!(args.contains(&"--alias".to_string()));
    assert!(args.contains(&"local-ddl".to_string()));
    assert!(args.contains(&"--cache-path".to_string()));
    assert!(!args.contains(&"--cache".to_string()));
}

#[test]
fn db_index_args_for_sqlite_uses_path() {
    assert_db_index_args(
        DbSource::Sqlite,
        "sqlite",
        Some(r"D:\data\shop.sqlite"),
        None,
        "--path",
        r"D:\data\shop.sqlite",
    );
}

#[test]
fn db_index_args_for_ddl_sqlite_uses_path() {
    assert_db_index_args(
        DbSource::DdlSqlite,
        "ddl-sqlite",
        Some(r"D:\schemas\shop.sql"),
        None,
        "--path",
        r"D:\schemas\shop.sql",
    );
}

#[test]
fn db_index_args_for_postgres_uses_connection_string() {
    assert_db_index_args(
        DbSource::Postgres,
        "postgres",
        None,
        Some("postgres://app:secret@localhost/shop"),
        "--connection-string",
        "postgres://app:secret@localhost/shop",
    );
}

#[test]
fn db_index_args_for_mysql_uses_connection_string() {
    assert_db_index_args(
        DbSource::Mysql,
        "mysql",
        None,
        Some("mysql://app:secret@localhost/shop"),
        "--connection-string",
        "mysql://app:secret@localhost/shop",
    );
}

#[test]
fn db_index_args_for_sqlserver_uses_connection_string() {
    assert_db_index_args(
        DbSource::Sqlserver,
        "sqlserver",
        None,
        Some("Server=localhost;Database=shop;User Id=app;Password=secret;"),
        "--connection-string",
        "Server=localhost;Database=shop;User Id=app;Password=secret;",
    );
}

#[test]
fn db_index_args_for_oracle_uses_connection_string() {
    assert_db_index_args(
        DbSource::Oracle,
        "oracle",
        None,
        Some("app/secret@localhost/XEPDB1"),
        "--connection-string",
        "app/secret@localhost/XEPDB1",
    );
}

#[test]
fn index_db_profile_requires_network_connection_string_before_engine_lookup() {
    let root = temp_root("network-index-missing-secret");
    let created = create_workspace(
        &root,
        CreateWorkspaceRequest {
            name: "Shop API".to_string(),
            repo_path: r"D:\projects\shop-api".to_string(),
        },
    )
    .unwrap();
    let updated = save_db_profile(
        &root,
        SaveDbProfileRequest {
            workspace_id: created.id.clone(),
            name: "Local Postgres".to_string(),
            source: DbSource::Postgres,
            path: None,
        },
    )
    .unwrap();
    let registry = EngineRegistry {
        mode: engine::EngineRuntimeMode::Dev,
        engine_dir: r"D:\missing\engines".to_string(),
        engines: vec![engine::EngineAvailability {
            id: "database-memory".to_string(),
            label: "rdb-memory".to_string(),
            role: "db".to_string(),
            executable: "database-memory.exe".to_string(),
            expected_version: "unknown".to_string(),
            contract_version: "1".to_string(),
            path: r"D:\missing\engines\database-memory.exe".to_string(),
            available: false,
            releasable: false,
            integrity: "missing".to_string(),
            sha256: None,
            error: None,
        }],
    };

    let error = index_db_profile(
        &root,
        &registry,
        IndexDbProfileRequest {
            workspace_id: updated.id.clone(),
            profile_id: updated.active_db_profile_id.unwrap(),
            connection_string: None,
        },
    )
    .unwrap_err();

    assert_eq!(error, "postgres 연결에는 DB 연결 문자열이 필요합니다");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn db_find_args_use_source_qualified_snapshot_alias() {
    let profile = test_db_profile(
        DbSource::DdlSqlite,
        "local-ddl",
        Some(r"D:\schemas\shop.sql"),
    );

    let args = db_find_args(&profile, "find-table", Path::new(r"D:\cache\graph.sqlite")).unwrap();

    assert_eq!(args[0], "find-table");
    assert_eq!(args[1], "ddl-sqlite:local-ddl");
    assert_eq!(args[2], "");
    assert!(args.contains(&"--format".to_string()));
    assert!(args.contains(&"json".to_string()));
    assert!(args.contains(&"--cache-path".to_string()));
    assert!(!args.contains(&"--cache".to_string()));
}

#[test]
fn db_inventory_args_use_one_bounded_json_call() {
    let profile = test_db_profile(DbSource::Sqlite, "shop", Some(r"D:\shop.sqlite"));
    let args = db_inventory_args(&profile, Path::new(r"D:\cache\graph.sqlite")).unwrap();

    assert_eq!(args[0], "inventory");
    assert_eq!(args[1], "sqlite:shop");
    assert_eq!(args[2..6], ["--limit", "1000", "--format", "json"]);
    assert!(args.contains(&"--cache-path".to_string()));
}

#[test]
fn db_describe_table_args_use_source_qualified_snapshot_alias() {
    let profile = test_db_profile(DbSource::Postgres, "caps-postgres", None);

    let args = db_describe_table_args(
        &profile,
        "public.accounts",
        Path::new(r"D:\cache\graph.sqlite"),
    )
    .unwrap();

    assert_eq!(args[0], "describe-table");
    assert_eq!(args[1], "postgres:caps-postgres");
    assert_eq!(args[2], "public.accounts");
    assert!(args.contains(&"--format".to_string()));
    assert!(args.contains(&"json".to_string()));
    assert!(args.contains(&"--cache-path".to_string()));
    assert!(!args.contains(&"--cache".to_string()));
}

#[test]
fn db_describe_table_args_prefer_stable_object_key() {
    let profile = test_db_profile(DbSource::Postgres, "shop", None);
    let key = "postgres:shop:shop:public:table:orders";
    let args = db_describe_table_args(&profile, key, Path::new(r"D:\cache\graph.sqlite")).unwrap();

    assert_eq!(args[2..4], ["--object-key", key]);
}

#[test]
fn db_description_enriches_column_types_pk_and_named_foreign_keys() {
    let mut table = DbInventoryTable {
        key: None,
        database: None,
        schema: Some("public".to_string()),
        name: "orders".to_string(),
        columns: vec![
            DbInventoryColumn {
                key: None,
                table_key: None,
                name: "id".to_string(),
                data_type: None,
                nullable: None,
                is_primary_key: false,
                is_foreign_key: false,
            },
            DbInventoryColumn {
                key: None,
                table_key: None,
                name: "account_id".to_string(),
                data_type: None,
                nullable: None,
                is_primary_key: false,
                is_foreign_key: false,
            },
        ],
        foreign_keys: Vec::new(),
        inbound_foreign_keys: Vec::new(),
        constraints: Vec::new(),
        indexes: Vec::new(),
    };
    let description = serde_json::json!({
        "columns": [
            { "name": "id", "type": "bigint", "nullable": false },
            { "name": "account_id", "type": "bigint", "nullable": false },
            { "name": "note", "type": "text", "nullable": true }
        ],
        "primary_key": ["id"],
        "foreign_keys": {
            "outbound": [
                {
                    "constraint_name": "orders_account_id_fkey",
                    "columns": ["account_id"],
                    "referenced_schema": "public",
                    "referenced_table": "accounts",
                    "referenced_columns": ["id"]
                }
            ]
        }
    });

    apply_table_description(&mut table, &description);

    assert_eq!(table.columns.len(), 3);
    let id = table
        .columns
        .iter()
        .find(|column| column.name == "id")
        .unwrap();
    let account_id = table
        .columns
        .iter()
        .find(|column| column.name == "account_id")
        .unwrap();
    let note = table
        .columns
        .iter()
        .find(|column| column.name == "note")
        .unwrap();
    assert_eq!(id.data_type.as_deref(), Some("bigint"));
    assert_eq!(id.nullable, Some(false));
    assert!(id.is_primary_key);
    assert_eq!(account_id.data_type.as_deref(), Some("bigint"));
    assert_eq!(account_id.nullable, Some(false));
    assert!(account_id.is_foreign_key);
    assert_eq!(note.data_type.as_deref(), Some("text"));
    assert_eq!(note.nullable, Some(true));
    assert_eq!(
        table.foreign_keys,
        vec![DbForeignKey {
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
        }]
    );
}

#[test]
fn bundled_describe_fixture_preserves_known_facts_and_marks_unknown_coverage() {
    let description: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/database-memory-describe-v0.1.0.json"
    ))
    .unwrap();
    let mut inventory = extract_db_inventory(
        "profile-1".to_string(),
        &serde_json::json!({ "tables": ["orders"] }),
        &serde_json::json!({ "columns": [] }),
    );

    apply_inventory_description_metadata(&mut inventory, "orders", &description);
    apply_table_description(&mut inventory.tables[0], &description);

    let table = &inventory.tables[0];
    assert_eq!(inventory.contract_version, None);
    assert_eq!(inventory.snapshot_key, None);
    assert_eq!(inventory.capability_warnings.len(), 1);
    assert!(inventory
        .gaps
        .iter()
        .any(|gap| gap.kind == "db-contract-coverage"));
    assert!(table
        .constraints
        .iter()
        .any(|constraint| constraint.kind == "primary_key"));
    assert!(table
        .constraints
        .iter()
        .any(|constraint| constraint.kind == "foreign_key"));
    assert!(!table
        .constraints
        .iter()
        .any(|constraint| matches!(constraint.kind.as_str(), "unique" | "check")));
    assert_eq!(table.foreign_keys.len(), 1);
    assert_eq!(table.inbound_foreign_keys.len(), 1);
    assert_eq!(table.indexes.len(), 1);
    assert_eq!(table.indexes[0].predicate, None);
    assert_eq!(table.indexes[0].expression, None);
}

#[test]
fn contract_one_describe_fixture_preserves_constraints_indexes_identity_and_direction() {
    let description: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/database-memory-describe-contract-1.json"
    ))
    .unwrap();
    let mut inventory = extract_db_inventory(
        "profile-1".to_string(),
        &serde_json::json!({ "tables": ["orders"] }),
        &serde_json::json!({ "columns": [] }),
    );

    apply_inventory_description_metadata(&mut inventory, "orders", &description);
    apply_table_description(&mut inventory.tables[0], &description);

    let table = &inventory.tables[0];
    assert_eq!(inventory.contract_version.as_deref(), Some("1"));
    assert_eq!(inventory.snapshot_key.as_deref(), Some("postgres:shop"));
    assert!(inventory.gaps.is_empty());
    assert_eq!(
        table.key.as_deref(),
        Some("postgres:shop:shop:public:table:orders")
    );
    assert_eq!(table.constraints.len(), 4);
    assert!(table.constraints.iter().any(|constraint| {
        constraint.kind == "check" && constraint.expression.as_deref() == Some("amount >= 0")
    }));
    assert!(table
        .constraints
        .iter()
        .any(|constraint| constraint.kind == "unique"));
    assert_eq!(table.foreign_keys[0].key, table.constraints[1].key);
    assert_eq!(
        table.inbound_foreign_keys[0].table_key.as_deref(),
        Some("postgres:shop:shop:public:table:order_items")
    );
    assert_eq!(table.indexes.len(), 1);
    assert!(table.indexes[0].unique);
    assert!(!table.indexes[0].primary);
    assert_eq!(
        table.indexes[0].predicate.as_deref(),
        Some("external_id IS NOT NULL")
    );
    assert_eq!(
        table.indexes[0].expression.as_deref(),
        Some("lower(external_id)")
    );
}

#[test]
fn bulk_inventory_fixture_preserves_bounds_and_stable_keys() {
    let value: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/database-memory-inventory-contract-1.json"
    ))
    .unwrap();
    let inventory = extract_bulk_db_inventory("shop".to_string(), &value).unwrap();

    assert_eq!(inventory.snapshot_key.as_deref(), Some("sqlite:shop"));
    assert_eq!(inventory.contract_version.as_deref(), Some("1"));
    assert_eq!(inventory.limit_requested, Some(1_000));
    assert_eq!(inventory.limit_applied, Some(1_000));
    assert_eq!(inventory.limit_clamped, Some(false));
    assert_eq!(inventory.result_count, Some(2));
    assert_eq!(inventory.total_tables, Some(2));
    assert_eq!(inventory.truncated, Some(false));
    let orders = inventory
        .tables
        .iter()
        .find(|table| table.name == "orders")
        .unwrap();
    assert_eq!(orders.database.as_deref(), Some("main"));
    assert_eq!(orders.schema.as_deref(), Some("main"));
    assert_eq!(
        orders.columns[1].key.as_deref(),
        Some("sqlite:shop:main:main:column:orders:account_id")
    );
    assert_eq!(
        orders.foreign_keys[0].column_keys,
        vec!["sqlite:shop:main:main:column:orders:account_id"]
    );
    assert_eq!(
        orders.constraints[0].referenced_column_keys,
        vec!["sqlite:shop:main:main:column:accounts:id"]
    );
    assert_eq!(
        orders.indexes[0].column_keys,
        vec!["sqlite:shop:main:main:column:orders:account_id"]
    );
    assert!(!serde_json::to_string(&inventory)
        .unwrap()
        .contains("must-not-persist"));
}

#[test]
fn fallback_table_matches_keep_duplicate_schema_identity() {
    let tables = serde_json::json!({
        "tables": ["users", "users"],
        "table_matches": [
            {
                "table_key": "postgres:shop:shop:audit:table:users",
                "name": "users",
                "schema": "audit",
                "database": "shop"
            },
            {
                "table_key": "postgres:shop:shop:public:table:users",
                "name": "users",
                "schema": "public",
                "database": "shop"
            }
        ]
    });
    let columns = serde_json::json!({
        "columns": [
            {
                "key": "postgres:shop:shop:audit:column:users:event_id",
                "table_key": "postgres:shop:shop:audit:table:users",
                "schema": "audit",
                "database": "shop",
                "table": "users",
                "column": "event_id"
            },
            {
                "key": "postgres:shop:shop:public:column:users:id",
                "table_key": "postgres:shop:shop:public:table:users",
                "schema": "public",
                "database": "shop",
                "table": "users",
                "column": "id"
            }
        ]
    });

    let inventory = extract_db_inventory("shop".to_string(), &tables, &columns);

    assert_eq!(inventory.tables.len(), 2);
    let audit = inventory
        .tables
        .iter()
        .find(|table| table.schema.as_deref() == Some("audit"))
        .unwrap();
    let public = inventory
        .tables
        .iter()
        .find(|table| table.schema.as_deref() == Some("public"))
        .unwrap();
    assert_eq!(audit.columns[0].name, "event_id");
    assert_eq!(public.columns[0].name, "id");
    assert_ne!(audit.key, public.key);
}

#[test]
fn truncated_and_failed_bulk_inventory_are_explicit_unknown_gaps() {
    let mut value: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/database-memory-inventory-contract-1.json"
    ))
    .unwrap();
    value["total_tables"] = serde_json::json!(3);
    value["truncated"] = serde_json::json!(true);
    let truncated = extract_bulk_db_inventory("shop".to_string(), &value).unwrap();
    assert!(truncated
        .gaps
        .iter()
        .any(|gap| gap.kind == "db-inventory-truncated"));

    let mut fallback = extract_db_inventory(
        "shop".to_string(),
        &serde_json::json!({ "tables": ["orders"] }),
        &serde_json::json!({ "columns": [] }),
    );
    record_bulk_fallback_gap(&mut fallback);
    assert!(fallback
        .gaps
        .iter()
        .any(|gap| gap.kind == "db-inventory-bulk-unavailable"));
}

#[test]
fn db_inventory_ignores_row_payload_and_secret_shaped_unknown_fields() {
    let mut description: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/database-memory-describe-contract-1.json"
    ))
    .unwrap();
    description["rows"] = serde_json::json!([{ "password": "must-not-persist" }]);
    description["connection_string"] =
        serde_json::json!("postgres://app:must-not-persist@localhost/shop");
    let mut inventory = extract_db_inventory(
        "profile-1".to_string(),
        &serde_json::json!({ "tables": ["orders"] }),
        &serde_json::json!({ "columns": [] }),
    );

    apply_inventory_description_metadata(&mut inventory, "orders", &description);
    apply_table_description(&mut inventory.tables[0], &description);
    let json = serde_json::to_string(&inventory).unwrap();

    assert!(!json.contains("must-not-persist"));
    assert!(!json.contains("connection_string"));
    assert!(!json.contains("\"rows\""));
}

#[test]
fn fallback_describe_plan_is_capped_and_reports_every_unknown_table() {
    let tables = (0..1_001)
        .map(|index| serde_json::Value::String(format!("table_{index}")))
        .collect::<Vec<_>>();
    let inventory = extract_db_inventory(
        "profile-1".to_string(),
        &serde_json::json!({ "tables": tables }),
        &serde_json::json!({ "columns": [] }),
    );

    let (targets, gaps) = db_describe_plan(&inventory);

    assert_eq!(targets.len(), 200);
    assert!(targets.iter().any(|(_, table)| table == "table_40"));
    assert_eq!(gaps.len(), 801);
    assert_eq!(gaps[0].table_key.as_deref(), Some("table_200"));
}

#[test]
fn duplicate_table_identity_is_an_explicit_gap_not_a_name_based_merge() {
    let mut inventory = extract_db_inventory(
        "profile-1".to_string(),
        &serde_json::json!({ "tables": ["users", "users"] }),
        &serde_json::json!({ "columns": [] }),
    );
    inventory.contract_version = Some("1".to_string());
    inventory.tables[0].key = Some("postgres:shop:shop:public:table:users".to_string());
    inventory.tables[1].key = Some("postgres:shop:shop:public:table:users".to_string());

    record_db_identity_gaps(&mut inventory);

    assert_eq!(inventory.tables.len(), 2);
    assert!(inventory
        .gaps
        .iter()
        .any(|gap| gap.kind == "db-table-identity-ambiguous"));
    assert!(inventory
        .gaps
        .iter()
        .any(|gap| gap.kind == "db-table-key-collision"));
}

#[test]
fn old_db_inventory_json_defaults_new_evidence_fields() {
    let inventory: DbInventory = serde_json::from_value(serde_json::json!({
        "profileId": "profile-1",
        "tables": [{
            "schema": null,
            "name": "orders",
            "columns": [],
            "foreignKeys": []
        }]
    }))
    .unwrap();

    assert_eq!(inventory.snapshot_key, None);
    assert_eq!(inventory.contract_version, None);
    assert!(inventory.capability_warnings.is_empty());
    assert!(inventory.gaps.is_empty());
    assert_eq!(inventory.tables[0].key, None);
    assert!(inventory.tables[0].constraints.is_empty());
    assert!(inventory.tables[0].indexes.is_empty());
    assert!(inventory.tables[0].inbound_foreign_keys.is_empty());
}

#[test]
fn db_inventory_accepts_database_memory_line_output() {
    let mut inventory = DbInventory {
        profile_id: "profile-1".to_string(),
        tables: Vec::new(),
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
    };

    merge_db_inventory_lines(
        &mut inventory,
        "sessions\nusers\n",
        "sessions.id\nsessions.title\nusers.id\n",
    );

    let sessions = inventory
        .tables
        .iter()
        .find(|table| table.name == "sessions")
        .unwrap();
    let users = inventory
        .tables
        .iter()
        .find(|table| table.name == "users")
        .unwrap();

    assert_eq!(inventory.tables.len(), 2);
    assert_eq!(sessions.columns.len(), 2);
    assert!(sessions.columns.iter().any(|column| column.name == "id"));
    assert!(sessions.columns.iter().any(|column| column.name == "title"));
    assert_eq!(users.columns.len(), 1);
    assert_eq!(users.columns[0].name, "id");
}

#[test]
fn db_cache_path_is_derived_from_validated_ids() {
    let root = Path::new("workspaces");
    let path = db_cache_path(root, "workspace-1", "profile-1");

    assert_eq!(
        path,
        root.join("workspace-1")
            .join("engines")
            .join("database-memory")
            .join("0.1.1")
            .join("contract-1")
            .join("profiles")
            .join("profile-1")
            .join("graph.sqlite")
    );
}

#[test]
fn code_cache_path_is_workspace_scoped() {
    let root = Path::new("workspaces");
    let path = workspace_code_cache_path(root, "workspace-1");

    assert_eq!(
        path,
        root.join("workspace-1")
            .join("engines")
            .join("codebase-memory")
            .join("0.8.1")
            .join("contract-1")
            .join("cache")
    );
    assert_ne!(path, root.join("code").join("cache"));
}

#[test]
fn workspace_db_cache_dir_is_workspace_scoped() {
    let root = Path::new("workspaces");
    let path = workspace_db_cache_dir(root, "workspace-1");

    assert_eq!(
        path,
        root.join("workspace-1")
            .join("engines")
            .join("database-memory")
            .join("0.1.1")
            .join("contract-1")
            .join("profiles")
    );
}

#[test]
fn code_index_payload_uses_workspace_repo_without_secrets() {
    let workspace = Workspace {
        id: "shop-api-1".to_string(),
        name: "shop-api".to_string(),
        repo_path: r"D:\projects\shop-api".to_string(),
        code_project: None,
        engine_cache: WorkspaceEngineCache::default(),
        db_profiles: Vec::new(),
        active_db_profile_id: None,
        created_at: "1".to_string(),
        updated_at: "1".to_string(),
    };

    let payload = code_index_payload(
        &workspace,
        Path::new(r"D:\app\workspaces\shop-api-1\code\cache"),
    );

    assert!(payload.contains(r"D:\\projects\\shop-api"));
    assert!(payload.contains(r"D:\\app\\workspaces\\shop-api-1\\code\\cache"));
    assert!(payload.contains("shop-api"));
    assert!(!payload.to_ascii_lowercase().contains("password"));
    assert!(!payload.to_ascii_lowercase().contains("token"));
}

#[test]
fn code_project_uses_index_stdout_project_name() {
    let stdout = r#"{"project":"D-project-backend_map-fixture-shop-api","status":"indexed"}"#;

    assert_eq!(
        code_project_from_index_stdout(stdout, "shop-api"),
        "D-project-backend_map-fixture-shop-api"
    );
    assert_eq!(
        code_project_from_index_stdout(
            "level=info msg=mem.init\n{\"project\":\"mixed-stdout\",\"status\":\"indexed\"}",
            "shop-api"
        ),
        "mixed-stdout"
    );
    assert_eq!(
        code_project_from_index_stdout("not-json", "shop-api"),
        "shop-api"
    );
}

#[test]
fn engine_json_value_accepts_log_prefixed_json_line() {
    let value =
        engine_json_value("level=info msg=mem.init\n{\"results\":[{\"name\":\"GET /health\"}]}")
            .unwrap();

    assert_eq!(value["results"][0]["name"], "GET /health");
}

#[test]
fn code_inventory_extracts_items_from_search_results() {
    let routes = serde_json::json!({
        "results": [
            {
                "name": "POST /orders",
                "qualified_name": "routes.orders.create",
                "label": "Route",
                "file_path": "src/routes/orders.ts",
                "start_line": 12
            }
        ]
    });
    let services = serde_json::json!({
        "results": [
            { "name": "OrderService", "qualified_name": "services.OrderService", "label": "Class", "file_path": "src/order_service.ts" },
            { "name": "OrderRepository", "qualified_name": "repositories.OrderRepository", "label": "Class", "file_path": "src/order_repository.ts" },
            { "name": "calculateTotals", "qualified_name": "math.calculateTotals", "label": "Function", "file_path": "src/order_math.ts", "line": 7, "end_line": 11 },
            { "name": "ServerState", "qualified_name": "server.ServerState", "label": "Struct", "file_path": "src/server.rs" },
            { "name": "orders.module", "qualified_name": "orders.module", "label": "Module", "file_path": "src/orders.ts" },
            { "name": "OrderThing", "qualified_name": "resources.OrderThing", "label": "Resource", "file_path": "src/order_thing.ts" }
        ]
    });
    let files = serde_json::json!({
        "results": [
            {
                "name": "orders.ts",
                "qualified_name": "files.orders",
                "label": "File",
                "file_path": "src/routes/orders.ts"
            }
        ]
    });

    let inventory =
        extract_code_inventory("shop-api".to_string(), None, &routes, &services, &files).unwrap();

    assert_eq!(inventory.routes[0].name, "POST /orders");
    assert_eq!(inventory.routes[0].line, Some(12));
    assert_eq!(inventory.routes[0].project, "shop-api");
    assert_eq!(inventory.routes[0].engine_label, "Route");
    assert_eq!(inventory.routes[0].qualified_name, "routes.orders.create");
    assert_eq!(inventory.services[0].kind, "Class");
    assert_eq!(inventory.repositories[0].name, "OrderRepository");
    assert_eq!(inventory.functions[0].line, Some(7));
    assert_eq!(inventory.functions[0].end_line, Some(11));
    assert_eq!(inventory.classes[0].engine_label, "Struct");
    assert_eq!(inventory.modules[0].name, "orders.module");
    assert_eq!(inventory.unknown[0].name, "OrderThing");
    assert_eq!(inventory.summary.routes, 1);
    assert_eq!(inventory.summary.services, 1);
    assert_eq!(inventory.summary.repositories, 1);
    assert_eq!(inventory.summary.functions, 1);
    assert_eq!(inventory.summary.classes, 1);
    assert_eq!(inventory.summary.modules, 1);
    assert_eq!(inventory.summary.unknown, 1);
    assert_eq!(inventory.files[0].name, "orders.ts");
    assert_eq!(
        inventory.files[0].file_path.as_deref(),
        Some("src/routes/orders.ts")
    );

    let mut ids = std::collections::HashSet::new();
    for item in inventory
        .routes
        .iter()
        .chain(inventory.handlers.iter())
        .chain(inventory.services.iter())
        .chain(inventory.repositories.iter())
        .chain(inventory.functions.iter())
        .chain(inventory.classes.iter())
        .chain(inventory.modules.iter())
        .chain(inventory.unknown.iter())
        .chain(inventory.files.iter())
    {
        assert!(
            ids.insert(item.id.as_str()),
            "duplicate bucket item: {}",
            item.id
        );
    }
}

#[test]
fn code_inventory_extracts_calls_between_known_inventory_items_only() {
    let routes = serde_json::json!({
        "results": [
            { "name": "POST /orders", "qualified_name": "routes.orders.create", "label": "Route" }
        ]
    });
    let services = serde_json::json!({
        "results": [
            { "name": "createOrder", "qualified_name": "services.OrderService.create", "label": "Function" },
            { "name": "recordAudit", "qualified_name": "services.AuditService.record", "label": "Function" }
        ]
    });
    let files = serde_json::json!({ "results": [] });
    let calls = serde_json::json!({
        "rows": [
            {
                "from": "routes.orders.create",
                "to": "services.OrderService.create"
            },
            {
                "caller.qualified_name": "services.OrderService.create",
                "callee.qualified_name": "services.AuditService.record"
            },
            {
                "from": "routes.orders.create",
                "to": "outside.graph.raw"
            }
        ]
    });
    let mut inventory =
        extract_code_inventory("shop-api".to_string(), None, &routes, &services, &files).unwrap();
    assert_eq!(inventory.functions.len(), 2);

    inventory.calls = extract_code_calls(&calls, &inventory);

    assert_eq!(
        inventory.calls,
        vec![
            CodeCall {
                from: "routes.orders.create".to_string(),
                to: "services.OrderService.create".to_string(),
            },
            CodeCall {
                from: "services.OrderService.create".to_string(),
                to: "services.AuditService.record".to_string(),
            },
        ]
    );
}

#[test]
fn code_inventory_rejects_bucket_misclassification_and_conflicting_labels() {
    let false_route = serde_json::json!({
        "results": [
            { "name": "route_tokens", "qualified_name": "code.route_tokens", "label": "Function" }
        ]
    });
    let empty = serde_json::json!({ "results": [] });

    let error = extract_code_inventory("shop-api".to_string(), None, &false_route, &empty, &empty)
        .unwrap_err();
    assert!(error.contains("expected Route, got Function"));

    let routes = serde_json::json!({
        "results": [
            { "name": "GET /orders", "qualified_name": "shared.node", "label": "Route" }
        ]
    });
    let code = serde_json::json!({
        "results": [
            { "name": "shared", "qualified_name": "shared.node", "label": "Function" }
        ]
    });
    let error =
        extract_code_inventory("shop-api".to_string(), None, &routes, &code, &empty).unwrap_err();
    assert!(error.contains("서로 다른 label"));
}

#[test]
fn code_inventory_keeps_no_route_empty_instead_of_fabricating_one() {
    let routes = serde_json::json!({ "results": [] });
    let code = serde_json::json!({
        "results": [
            { "name": "route_tokens", "qualified_name": "code.route_tokens", "label": "Function" }
        ]
    });
    let files = serde_json::json!({ "results": [] });

    let inventory =
        extract_code_inventory("shop-api".to_string(), None, &routes, &code, &files).unwrap();

    assert!(inventory.routes.is_empty());
    assert_eq!(inventory.functions.len(), 1);
}

#[test]
fn code_inventory_normalizes_handles_from_handler_to_route_rows() {
    let routes = serde_json::json!({
        "results": [
            { "name": "POST /orders", "qualified_name": "routes.orders.create", "label": "Route" }
        ]
    });
    let code = serde_json::json!({
        "results": [
            { "name": "createOrder", "qualified_name": "handlers.createOrder", "label": "Function" }
        ]
    });
    let files = serde_json::json!({ "results": [] });
    let handles = serde_json::json!({
        "columns": ["source", "target"],
        "rows": [
            ["handlers.createOrder", "routes.orders.create"],
            ["outside.graph", "routes.orders.create"]
        ]
    });
    let mut inventory =
        extract_code_inventory("shop-api".to_string(), None, &routes, &code, &files).unwrap();

    assert_eq!(
        extract_code_handles(&handles, &inventory),
        vec![CodeHandle {
            handler: "handlers.createOrder".to_string(),
            route: "routes.orders.create".to_string(),
        }]
    );
    attach_code_handles(&handles, &mut inventory);
    assert_eq!(inventory.handlers[0].id, "handlers.createOrder");
    assert!(inventory.functions.is_empty());
    assert_eq!(inventory.summary.handlers, 1);
    assert_eq!(inventory.summary.functions, 0);
}

#[test]
fn code_inventory_enriches_real_query_graph_source_location_rows() {
    let routes = serde_json::json!({ "results": [] });
    let code = serde_json::json!({
        "results": [
            {
                "name": "createOrder",
                "qualified_name": "handlers.createOrder",
                "label": "Function",
                "file_path": "src/handlers/orders.rs"
            }
        ]
    });
    let files = serde_json::json!({ "results": [] });
    let locations = serde_json::json!({
        "columns": ["source", "path", "start_line", "end_line"],
        "rows": [
            ["handlers.createOrder", "src/handlers/orders.rs", "20", "46"]
        ],
        "total": 1
    });
    let mut inventory =
        extract_code_inventory("shop-api".to_string(), None, &routes, &code, &files).unwrap();

    assert_eq!(inventory.functions[0].line, None);
    enrich_code_locations(&locations, &mut inventory).unwrap();
    assert_eq!(inventory.functions[0].line, Some(20));
    assert_eq!(inventory.functions[0].column, None);
    assert_eq!(inventory.functions[0].end_line, Some(46));
    assert_eq!(inventory.functions[0].end_column, None);
    assert_eq!(
        inventory.functions[0].file_path.as_deref(),
        Some("src/handlers/orders.rs")
    );
    assert!(SOURCE_LOCATIONS_QUERY.contains("node.start_line AS start_line"));
    assert!(SOURCE_LOCATIONS_QUERY.contains("node.start_column AS start_column"));
    assert!(SOURCE_LOCATIONS_QUERY.contains("node.end_line AS end_line"));
    assert!(SOURCE_LOCATIONS_QUERY.contains("node.end_column AS end_column"));
}

#[test]
fn code_inventory_enriches_six_column_source_location_rows() {
    let code = serde_json::json!({
        "results": [{
            "name": "createOrder",
            "qualified_name": "handlers.createOrder",
            "label": "Function"
        }]
    });
    let mut inventory = extract_code_inventory(
        "shop-api".to_string(),
        None,
        &serde_json::json!({ "results": [] }),
        &code,
        &serde_json::json!({ "results": [] }),
    )
    .unwrap();
    let locations = serde_json::json!({
        "columns": ["source", "path", "start_line", "start_column", "end_line", "end_column"],
        "rows": [["handlers.createOrder", "src/handlers/orders.rs", "20", "3", "46", "18"]]
    });

    enrich_code_locations(&locations, &mut inventory).unwrap();
    let item = &inventory.functions[0];
    assert_eq!(
        (item.line, item.column, item.end_line, item.end_column),
        (Some(20), Some(3), Some(46), Some(18))
    );
}

#[test]
fn code_engine_queries_use_exact_labels_pagination_and_safe_aliases() {
    let payload = code_label_payload("shop-api", "Route", r"D:\cache", 500);

    assert_eq!(payload["label"], "Route");
    assert_eq!(payload["limit"], 500);
    assert_eq!(payload["offset"], 500);
    assert!(payload.get("query").is_none());
    assert!(CALLS_QUERY.contains(" AS source"));
    assert!(CALLS_QUERY.contains(" AS target"));
    assert!(!CALLS_QUERY.contains(" AS from"));
    assert!(HANDLES_QUERY.starts_with("MATCH (handler)-[:HANDLES]->(route)"));
    assert!(HANDLES_QUERY.contains(" AS source"));
    assert!(HANDLES_QUERY.contains(" AS target"));
}

#[test]
fn bundled_code_engine_fixture_preserves_route_handler_call_and_location() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/codebase-memory-contract-v0.8.1.json"
    ))
    .unwrap();
    let project = fixture["project"].as_str().unwrap().to_string();
    let mut inventory = extract_code_inventory(
        project,
        None,
        &fixture["routes"],
        &fixture["code"],
        &fixture["files"],
    )
    .unwrap();

    inventory.calls = extract_code_calls(&fixture["calls"], &inventory);
    attach_code_handles(&fixture["handles"], &mut inventory);
    enrich_code_locations(&fixture["sourceLocations"], &mut inventory).unwrap();

    assert_eq!(fixture["engineVersion"], "0.8.1");
    assert_eq!(inventory.routes.len(), 2);
    assert_eq!(inventory.handlers.len(), 1);
    assert_eq!(inventory.handlers[0].name, "bootstrap_admin");
    assert_eq!(inventory.handlers[0].line, Some(69));
    assert_eq!(inventory.handlers[0].end_line, Some(86));
    assert_eq!(inventory.handles.len(), 1);
    assert_eq!(
        inventory.handles[0].route,
        "__route__POST__/bootstrap-admin"
    );
    assert_eq!(inventory.calls.len(), 1);
    assert_eq!(
        inventory.calls[0].to,
        "D-meeting-overlay-assistant.server.app.api.http.routes.auth._to_session_response"
    );
    assert_eq!(inventory.files.len(), 1);
    assert_eq!(inventory.files[0].engine_label, "File");
}

#[test]
fn code_inventory_deserializes_pre_handles_items_for_snapshot_migration() {
    let inventory: CodeInventory = serde_json::from_value(serde_json::json!({
        "project": "shop-api",
        "routes": [],
        "services": [
            {
                "id": "services.OrderService",
                "kind": "Class",
                "name": "OrderService",
                "filePath": "src/order_service.ts",
                "line": 3,
                "detail": {}
            }
        ],
        "files": [],
        "handlers": [],
        "repositories": [],
        "functions": [],
        "classes": [],
        "modules": [],
        "unknown": [],
        "summary": {
            "routes": 0,
            "handlers": 0,
            "services": 1,
            "repositories": 0,
            "functions": 0,
            "classes": 0,
            "modules": 0,
            "files": 0,
            "unknown": 0
        },
        "architecture": null,
        "calls": []
    }))
    .unwrap();

    assert!(inventory.handles.is_empty());
    assert!(inventory.services[0].project.is_empty());
    assert!(inventory.services[0].qualified_name.is_empty());
    assert!(inventory.services[0].engine_label.is_empty());
    assert_eq!(inventory.services[0].column, None);
    assert_eq!(inventory.services[0].end_line, None);
    assert_eq!(inventory.services[0].end_column, None);
}

#[test]
fn focused_code_search_uses_one_bounded_compact_regex_query() {
    assert_eq!(
        focused_code_search_pattern("sales.orders").unwrap(),
        r"(^|[^A-Za-z0-9_])sales\.orders([^A-Za-z0-9_]|$)"
    );
    assert_eq!(
        focused_code_search_pattern("order[id]").unwrap(),
        r"(^|[^A-Za-z0-9_])order\[id\]([^A-Za-z0-9_]|$)"
    );

    let args = focused_code_search_args("shop-api", "orders", Some("^src/db/"), 100).unwrap();
    assert_eq!(args.iter().filter(|arg| *arg == "search_code").count(), 1);
    assert_eq!(args[0..2], ["cli", "search_code"]);
    assert_eq!(args.len(), 3);
    let payload: serde_json::Value = serde_json::from_str(&args[2]).unwrap();
    assert_eq!(payload["regex"], true);
    assert_eq!(payload["mode"], "compact");
    assert_eq!(payload["context"], 0);
    assert_eq!(payload["limit"], 32);
    assert_eq!(payload["path_filter"], "^src/db/");
    assert!(focused_code_search_args("shop-api", "orders", Some("\n"), 8).is_err());
}

#[test]
fn focused_code_search_discards_bodies_and_reports_every_partial_reason() {
    let stdout = r#"{
        "results": [{
            "node": "loadOrders",
            "qualified_name": "repo.loadOrders",
            "label": "Function",
            "file": "src/repo.ts",
            "start_line": 10,
            "end_line": 20,
            "match_lines": [14],
            "source": "must-not-survive",
            "context": "must-not-survive"
        }],
        "raw_matches": [{"file":"schema.sql","line":2,"content":"must-not-survive"}],
        "total_grep_matches": 500,
        "total_results": 3,
        "raw_match_count": 1,
        "elapsed_ms": 4
    }"#;
    let result = parse_focused_code_search_output(
        stdout,
        "level=info msg=cache.open\nunexpected diagnostic",
        1,
    )
    .unwrap();

    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.matches[0].match_lines, vec![14]);
    assert_eq!(result.totals.returned, 1);
    assert!(result.partial);
    assert_eq!(
        result.partial_reasons,
        [
            "engine-stderr",
            "result-limit",
            "grep-limit",
            "unmapped-raw-matches"
        ]
    );
    let serialized = serde_json::to_string(&result).unwrap();
    assert!(!serialized.contains("must-not-survive"));
    assert!(!serialized.contains("rawMatches"));
    assert!(!serialized.contains("source"));
    assert!(!serialized.contains("context"));
}

#[test]
fn corrupt_workspace_is_isolated_from_the_healthy_workspace_list() {
    let root = temp_root("workspace-corruption-isolation");
    let healthy = create_workspace(
        &root,
        CreateWorkspaceRequest {
            name: "Healthy API".to_string(),
            repo_path: r"D:\projects\healthy-api".to_string(),
        },
    )
    .unwrap();
    let corrupt = create_workspace(
        &root,
        CreateWorkspaceRequest {
            name: "Corrupt API".to_string(),
            repo_path: r"D:\projects\corrupt-api".to_string(),
        },
    )
    .unwrap();
    let workspaces_dir = base_paths(&root).workspaces_dir;
    fs::write(
        workspaces_dir.join(&corrupt.id).join("workspace.json"),
        "{broken",
    )
    .unwrap();

    let listed = list_workspaces(&root).unwrap();
    let warnings = super::store::workspace_recovery_warnings(&root).unwrap();

    assert_eq!(listed, vec![healthy]);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].workspace_id, corrupt.id);
    assert_eq!(warnings[0].kind, "unrecoverable");
    assert_eq!(warnings[0].action, "recreate-workspace");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn valid_workspace_backup_recovers_and_repairs_active_db_profile_without_overwrite() {
    let root = temp_root("workspace-backup-repair");
    let mut workspace = create_workspace(
        &root,
        CreateWorkspaceRequest {
            name: "Shop API".to_string(),
            repo_path: r"D:\projects\shop-api".to_string(),
        },
    )
    .unwrap();
    let profile = test_db_profile(DbSource::Postgres, "production-db", None);
    workspace.active_db_profile_id = Some(profile.id.clone());
    workspace.db_profiles.push(profile.clone());
    workspace.updated_at = "2".to_string();
    let workspaces_dir = base_paths(&root).workspaces_dir;
    super::store::write_workspace(&workspaces_dir, &workspace).unwrap();
    // Rotate again so the backup contains the active profile, not the initial empty workspace.
    super::store::write_workspace(&workspaces_dir, &workspace).unwrap();

    let primary = workspaces_dir.join(&workspace.id).join("workspace.json");
    let backup = super::store::workspace_backup_file(&workspaces_dir, &workspace.id);
    let backup_before = fs::read(&backup).unwrap();
    fs::write(&primary, "{broken").unwrap();

    let recovered = open_workspace(&root, &workspace.id).unwrap();
    let warnings = super::store::workspace_recovery_warnings(&root).unwrap();
    assert_eq!(recovered.active_db_profile_id, Some(profile.id.clone()));
    assert_eq!(recovered.db_profiles, vec![profile]);
    assert_eq!(warnings[0].kind, "backup-recovered");
    assert_eq!(warnings[0].action, "repair-from-backup");

    let repaired = super::store::repair_workspace_from_backup(&root, &workspace.id).unwrap();
    assert_eq!(repaired, recovered);
    assert_eq!(open_workspace(&root, &workspace.id).unwrap(), recovered);
    assert_eq!(fs::read(&backup).unwrap(), backup_before);
    assert!(super::store::workspace_recovery_warnings(&root)
        .unwrap()
        .is_empty());
    assert!(fs::read_dir(primary.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| entry
            .file_name()
            .to_string_lossy()
            .starts_with("workspace.corrupt.")));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn incomplete_workspace_temp_write_leaves_the_last_complete_generation_readable() {
    let root = temp_root("workspace-mid-write");
    let mut workspace = create_workspace(
        &root,
        CreateWorkspaceRequest {
            name: "Shop API".to_string(),
            repo_path: r"D:\projects\shop-api".to_string(),
        },
    )
    .unwrap();
    let original = workspace.clone();
    workspace.name = "Shop API Updated".to_string();
    workspace.updated_at = "2".to_string();
    let workspaces_dir = base_paths(&root).workspaces_dir;
    super::store::write_workspace(&workspaces_dir, &workspace).unwrap();

    let dir = workspaces_dir.join(&workspace.id);
    fs::remove_file(dir.join("workspace.json")).unwrap();
    fs::write(dir.join("workspace.999.999.999.tmp"), "{partial").unwrap();

    assert_eq!(open_workspace(&root, &workspace.id).unwrap(), original);
    assert_eq!(list_workspaces(&root).unwrap(), vec![original]);
    assert_eq!(
        super::store::workspace_recovery_warnings(&root).unwrap()[0].kind,
        "backup-recovered"
    );
    fs::remove_dir_all(root).unwrap();
}
