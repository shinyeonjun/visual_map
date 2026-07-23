use super::*;
use crate::{base_paths, engine, EngineRegistry};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn temp_root(name: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("backend-visual-map-{name}-{}", std::process::id()));

    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }

    root
}

fn create_local_workspace(root: &Path, name: &str) -> Workspace {
    let repo = root.join(format!(
        "repo-{}",
        name.to_ascii_lowercase().replace(' ', "-")
    ));
    fs::create_dir_all(&repo).unwrap();
    create_workspace(
        root,
        CreateWorkspaceRequest {
            name: name.to_string(),
            repo_path: repo.display().to_string(),
        },
    )
    .unwrap()
}

fn run_test_git(current_dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(current_dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("git must be available for workspace lifecycle tests");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn create_managed_git_workspace(root: &Path) -> (Workspace, PathBuf, PathBuf) {
    fs::create_dir_all(root).unwrap();
    let remote = root.join("remote.git");
    let seed = root.join("seed");
    let remote_arg = remote.display().to_string();
    let seed_arg = seed.display().to_string();
    run_test_git(root, &["init", "--bare", remote_arg.as_str()]);
    run_test_git(root, &["init", seed_arg.as_str()]);
    run_test_git(&seed, &["config", "user.email", "tests@example.com"]);
    run_test_git(&seed, &["config", "user.name", "Backend Visual Map Tests"]);
    fs::write(seed.join("README.md"), "first\n").unwrap();
    run_test_git(&seed, &["add", "README.md"]);
    run_test_git(&seed, &["commit", "-m", "first"]);
    run_test_git(&seed, &["remote", "add", "origin", remote_arg.as_str()]);
    run_test_git(&seed, &["push", "-u", "origin", "HEAD"]);

    let mut workspace = create_local_workspace(root, "Managed GitHub");
    let paths = base_paths(root);
    let managed_repo = workspace_repo_dir(&paths.workspaces_dir, &workspace.id);
    let managed_arg = managed_repo.display().to_string();
    run_test_git(root, &["clone", remote_arg.as_str(), managed_arg.as_str()]);
    workspace.repo_path = managed_arg;
    workspace.repo_source = RepoSource::Github;
    workspace.repo_origin = Some("https://github.com/acme/managed".to_string());
    super::store::write_workspace(&paths.workspaces_dir, &workspace).unwrap();
    (workspace, seed, managed_repo)
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
        repo_source: RepoSource::Github,
        repo_origin: Some("https://github.com/acme/shop-api".to_string()),
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
    assert!(json.contains("\"repoSource\":\"github\""));
    assert!(json.contains("\"repoOrigin\":\"https://github.com/acme/shop-api\""));
    assert!(json.contains("\"dbProfiles\""));
    assert!(json.contains("\"engineCache\""));
    assert!(json.contains("\"activeDbProfileId\""));
    assert!(json.contains("\"passwordStored\":false"));
    assert!(json.contains("\"source\":\"postgres\""));
}

#[test]
fn create_open_and_list_workspace_round_trip() {
    let root = temp_root("workspace-round-trip");
    let created = create_local_workspace(&root, "Shop API");

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
fn github_refresh_rejects_local_workspace() {
    let root = temp_root("github-refresh-local");
    let workspace = create_local_workspace(&root, "Local");

    let error = refresh_github_workspace(&root, &workspace.id).unwrap_err();

    assert!(error.contains("앱이 복제한 GitHub 프로젝트만"));
}

#[test]
fn github_refresh_fast_forwards_managed_clone() {
    let root = temp_root("github-refresh-fast-forward");
    let (workspace, seed, managed_repo) = create_managed_git_workspace(&root);
    fs::write(seed.join("README.md"), "second\n").unwrap();
    run_test_git(&seed, &["add", "README.md"]);
    run_test_git(&seed, &["commit", "-m", "second"]);
    run_test_git(&seed, &["push"]);

    let refreshed = refresh_github_workspace(&root, &workspace.id).unwrap();

    assert_eq!(
        fs::read_to_string(managed_repo.join("README.md"))
            .unwrap()
            .trim(),
        "second"
    );
    assert_eq!(refreshed.repo_source, RepoSource::Github);
    assert!(refreshed.updated_at >= workspace.updated_at);
}

#[test]
fn github_refresh_preserves_dirty_managed_clone() {
    let root = temp_root("github-refresh-dirty");
    let (workspace, _seed, managed_repo) = create_managed_git_workspace(&root);
    fs::write(managed_repo.join("README.md"), "local change\n").unwrap();

    let error = refresh_github_workspace(&root, &workspace.id).unwrap_err();

    assert!(error.contains("로컬 변경이 있어"));
    assert_eq!(
        fs::read_to_string(managed_repo.join("README.md")).unwrap(),
        "local change\n"
    );
}

#[test]
fn create_workspace_rejects_invalid_local_paths() {
    let root = temp_root("workspace-invalid-local-path");
    let missing = root.join("missing");
    let missing_error = create_workspace(
        &root,
        CreateWorkspaceRequest {
            name: "Missing".to_string(),
            repo_path: missing.display().to_string(),
        },
    )
    .unwrap_err();

    fs::create_dir_all(&root).unwrap();
    let file = root.join("not-a-directory.txt");
    fs::write(&file, "not a repository").unwrap();
    let file_error = create_workspace(
        &root,
        CreateWorkspaceRequest {
            name: "File".to_string(),
            repo_path: file.display().to_string(),
        },
    )
    .unwrap_err();

    assert!(missing_error.contains("찾을 수 없습니다"));
    assert_eq!(file_error, "프로젝트 경로는 폴더여야 합니다");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn delete_workspace_removes_only_app_metadata() {
    let root = temp_root("workspace-delete");
    let created = create_local_workspace(&root, "Shop API");
    let repo = PathBuf::from(&created.repo_path);
    let workspace_dir = base_paths(&root).workspaces_dir.join(&created.id);

    delete_workspace(&root, &created.id).unwrap();

    assert!(!workspace_dir.exists());
    assert!(repo.is_dir());
    assert!(list_workspaces(&root).unwrap().is_empty());
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
    let mut created = create_local_workspace(&root, "Shop API");
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
    let created = create_local_workspace(&root, "Shop API");

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
        .starts_with(r"engines\database-memory\0.2.0\contract-2\profiles\"));
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
    let created = create_local_workspace(&root, "Shop API");
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
fn delete_db_profile_removes_profile_and_cache() {
    let root = temp_root("db-profile-delete");
    let created = create_local_workspace(&root, "Shop API");
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
    let profile_id = updated.active_db_profile_id.unwrap();
    let cache = db_cache_path(&base_paths(&root).workspaces_dir, &created.id, &profile_id);
    fs::write(&cache, "cache").unwrap();

    let without_profile = delete_db_profile(&root, &created.id, &profile_id).unwrap();

    assert!(without_profile.db_profiles.is_empty());
    assert_eq!(without_profile.active_db_profile_id, None);
    assert!(!cache.exists());
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
#[ignore = "requires bundled database-memory sidecar"]
fn database_memory_v2_adapter_round_trip_is_complete_and_metadata_only() {
    let root = temp_root("database-memory-v2-adapter");
    let created = create_local_workspace(&root, "Adapter Smoke");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("scripts")
        .join("fixtures")
        .join("product-smoke-schema.sql");
    let workspace = save_db_profile(
        &root,
        SaveDbProfileRequest {
            workspace_id: created.id.clone(),
            name: "Adapter DDL".to_string(),
            source: DbSource::DdlSqlite,
            path: Some(fixture.display().to_string()),
        },
    )
    .unwrap();
    let profile_id = workspace.active_db_profile_id.clone().unwrap();
    let engine_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("engines");
    let registry = engine::engine_registry(
        engine::EngineRuntimeMode::Dev,
        &root,
        None,
        None,
        Some(&engine_dir),
    );

    let indexed = index_db_profile(
        &root,
        &registry,
        IndexDbProfileRequest {
            workspace_id: created.id.clone(),
            profile_id: profile_id.clone(),
            connection_string: None,
        },
    )
    .unwrap();
    assert!(indexed.run.ok, "{}", indexed.run.stderr);
    assert_eq!(indexed.index_json.as_ref().unwrap()["contract_version"], 2);
    assert_eq!(indexed.index_json.as_ref().unwrap()["status"], "complete");

    let inventory = db_inventory(&root, &registry, &created.id, Some(&profile_id)).unwrap();
    assert_eq!(inventory.contract_version.as_deref(), Some("2"));
    assert_eq!(inventory.result_count, inventory.total_tables);
    assert_eq!(inventory.truncated, Some(false));
    assert!(inventory.gaps.is_empty());
    assert!(inventory.tables.iter().all(|table| {
        table.key.is_some()
            && table.columns.iter().all(|column| {
                column.key.is_some() && column.table_key.as_ref() == table.key.as_ref()
            })
    }));
    assert!(inventory
        .tables
        .iter()
        .any(|table| !table.foreign_keys.is_empty()));

    fs::remove_dir_all(root).unwrap();
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
fn db_index_args_for_postgres_uses_environment_profile() {
    assert_db_index_args(
        DbSource::Postgres,
        "postgres",
        None,
        Some("postgres://app:secret@localhost/shop"),
        "--config-path",
        r"D:\cache\database-memory-profile.toml",
    );
}

#[test]
fn db_index_args_for_yugabytedb_uses_environment_profile() {
    assert_db_index_args(
        DbSource::Yugabytedb,
        "yugabytedb",
        None,
        Some("postgres://app:secret@localhost:5433/shop"),
        "--config-path",
        r"D:\cache\database-memory-profile.toml",
    );
}

#[test]
fn db_index_args_for_mysql_uses_environment_profile() {
    assert_db_index_args(
        DbSource::Mysql,
        "mysql",
        None,
        Some("mysql://app:secret@localhost/shop"),
        "--config-path",
        r"D:\cache\database-memory-profile.toml",
    );
}

#[test]
fn db_index_args_for_mariadb_uses_environment_profile() {
    assert_db_index_args(
        DbSource::Mariadb,
        "mariadb",
        None,
        Some("mysql://app:secret@localhost:3306/shop"),
        "--config-path",
        r"D:\cache\database-memory-profile.toml",
    );
}

#[test]
fn db_index_args_for_sqlserver_uses_environment_profile() {
    assert_db_index_args(
        DbSource::Sqlserver,
        "sqlserver",
        None,
        Some("Server=localhost;Database=shop;User Id=app;Password=secret;"),
        "--config-path",
        r"D:\cache\database-memory-profile.toml",
    );
}

#[test]
fn db_index_args_for_oracle_uses_environment_profile() {
    assert_db_index_args(
        DbSource::Oracle,
        "oracle",
        None,
        Some("app/secret@localhost/XEPDB1"),
        "--config-path",
        r"D:\cache\database-memory-profile.toml",
    );
}

#[test]
fn db_network_secret_never_enters_process_arguments() {
    let secret = "postgres://app:secret@localhost/shop";
    let profile = test_db_profile(DbSource::Postgres, "local-db", None);
    let args = db_index_args(&profile, Path::new(r"D:\cache\graph.sqlite"), Some(secret)).unwrap();

    assert!(!args.iter().any(|argument| argument.contains(secret)));
    assert!(!args
        .iter()
        .any(|argument| argument == "--connection-string"));
    assert_eq!(
        db_connection_env_var("local-db"),
        "DATABASE_MEMORY_LOCAL_DB_CONNECTION_STRING"
    );
    assert_eq!(
        db_connection_config_path(Path::new(r"D:\cache\graph.sqlite")),
        PathBuf::from(r"D:\cache\database-memory-profile.toml")
    );
}

#[test]
fn index_db_profile_requires_network_connection_string_before_engine_lookup() {
    let root = temp_root("network-index-missing-secret");
    let created = create_local_workspace(&root, "Shop API");
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
            expected_version: "0.2.0".to_string(),
            contract_version: "2".to_string(),
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
        dependents: Vec::new(),
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
fn db_description_preserves_supported_dependents_and_merges_column_evidence() {
    let mut inventory = extract_db_inventory(
        "profile-1".to_string(),
        &serde_json::json!({ "tables": ["orders"] }),
        &serde_json::json!({ "columns": [] }),
    );
    let description = serde_json::json!({
        "dependents": [
            {
                "key": "sqlite:shop:main:view:active_orders",
                "kind": "view",
                "name": "active_orders",
                "relation": "view_depends_on",
                "columnKeys": ["sqlite:shop:main:column:orders:status"]
            },
            {
                "key": "sqlite:shop:main:view:active_orders",
                "kind": "view",
                "name": "active_orders",
                "relation": "view_depends_on",
                "column_keys": [
                    "sqlite:shop:main:column:orders:id",
                    "sqlite:shop:main:column:orders:status"
                ]
            },
            {
                "key": "sqlite:shop:main:trigger:orders:trg_orders_status",
                "kind": "trigger",
                "name": "trg_orders_status",
                "relation": "table_has_trigger",
                "column_keys": []
            },
            {
                "key": "sqlite:shop:main:sequence:orders_id_seq",
                "kind": "sequence",
                "name": "orders_id_seq",
                "relation": "TABLE_USES_SEQUENCE"
            }
        ]
    });

    apply_table_description(&mut inventory.tables[0], &description);

    let dependents = &inventory.tables[0].dependents;
    assert_eq!(dependents.len(), 2);
    assert_eq!(dependents[0].kind, "trigger");
    assert_eq!(dependents[1].kind, "view");
    assert_eq!(
        dependents[1].column_keys,
        vec![
            "sqlite:shop:main:column:orders:id".to_string(),
            "sqlite:shop:main:column:orders:status".to_string(),
        ]
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
fn db_cache_path_is_derived_from_validated_ids() {
    let root = Path::new("workspaces");
    let path = db_cache_path(root, "workspace-1", "profile-1");

    assert_eq!(
        path,
        root.join("workspace-1")
            .join("engines")
            .join("database-memory")
            .join("0.2.0")
            .join("contract-2")
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
            .join("0.9.0")
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
            .join("0.2.0")
            .join("contract-2")
            .join("profiles")
    );
}

#[test]
fn code_index_payload_forces_full_read_only_mode_without_transport_fields() {
    let payload = index_payload(r"D:\projects\shop-api", "shop-api");

    assert_eq!(payload["repo_path"], r"D:\projects\shop-api");
    assert_eq!(payload["name"], "shop-api");
    assert_eq!(payload["mode"], "full");
    assert_eq!(payload["persistence"], false);
    assert!(payload.get("cache_path").is_none());
    assert!(payload.get("project_name").is_none());
    assert!(!payload
        .to_string()
        .to_ascii_lowercase()
        .contains("password"));
    assert!(!payload.to_string().to_ascii_lowercase().contains("token"));
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
fn code_project_generations_are_transport_safe_and_never_reused() {
    let first = next_code_project_generation();
    let second = next_code_project_generation();

    assert_ne!(first, second);
    for project in [first, second] {
        assert!(project.starts_with("visual-map-"));
        assert!(project
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'));
    }
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
fn code_inventory_role_buckets_require_class_like_role_names() {
    let routes = serde_json::json!({ "results": [] });
    let code = serde_json::json!({
        "results": [
            { "name": "OrderService", "qualified_name": "types.OrderService", "label": "Class" },
            { "name": "OrderRepositoryImpl", "qualified_name": "types.OrderRepositoryImpl", "label": "Class" },
            { "name": "OrdersController", "qualified_name": "types.OrdersController", "label": "Class" },
            { "name": "ServiceRegistry", "qualified_name": "types.ServiceRegistry", "label": "Class" },
            { "name": "services", "qualified_name": "model.services", "label": "Field" },
            { "name": "sourceRepository", "qualified_name": "config.sourceRepository", "label": "Variable" },
            { "name": "move_confirmed_handlers", "qualified_name": "code.move_confirmed_handlers", "label": "Function" },
            { "name": "index_code_repository", "qualified_name": "code.index_code_repository", "label": "Function" }
        ]
    });
    let files = serde_json::json!({ "results": [] });

    let inventory =
        extract_code_inventory("shop-api".to_string(), None, &routes, &code, &files).unwrap();

    assert_eq!(inventory.services[0].name, "OrderService");
    assert_eq!(inventory.repositories[0].name, "OrderRepositoryImpl");
    assert_eq!(inventory.handlers[0].name, "OrdersController");
    assert!(inventory
        .classes
        .iter()
        .any(|item| item.name == "ServiceRegistry"));
    assert!(inventory
        .functions
        .iter()
        .any(|item| item.name == "move_confirmed_handlers"));
    assert!(inventory
        .functions
        .iter()
        .any(|item| item.name == "index_code_repository"));
    assert!(inventory.unknown.iter().any(|item| item.name == "services"));
    assert!(inventory
        .unknown
        .iter()
        .any(|item| item.name == "sourceRepository"));
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
                "to": "services.OrderService.create",
                "confidence": "0.38",
                "strategy": "unique_name",
                "call_expression": "createOrder"
            },
            {
                "from": "routes.orders.create",
                "to": "services.OrderService.create",
                "confidence": "0.95",
                "strategy": "lsp_direct",
                "call_expression": "OrderService.create"
            },
            {
                "caller.qualified_name": "services.OrderService.create",
                "callee.qualified_name": "services.AuditService.record",
                "confidence": 0.75,
                "strategy": "unique_name",
                "callExpression": "recordAudit"
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
                confidence: Some(95),
                strategy: Some("lsp_direct".to_string()),
                expression: Some("OrderService.create".to_string()),
            },
            CodeCall {
                from: "services.OrderService.create".to_string(),
                to: "services.AuditService.record".to_string(),
                confidence: Some(75),
                strategy: Some("unique_name".to_string()),
                expression: Some("recordAudit".to_string()),
            },
        ]
    );
}

#[test]
fn code_inventory_rejects_only_production_calls_into_test_code() {
    let empty = serde_json::json!({ "results": [] });
    let code = serde_json::json!({
        "results": [
            {
                "name": "ExecuteAsync",
                "qualified_name": "src.Create.ExecuteAsync",
                "label": "Method",
                "file_path": "src/Web/Create.cs",
                "is_test": false
            },
            {
                "name": "Send",
                "qualified_name": "tests.NoOpMediator.Send",
                "label": "Method",
                "file_path": "tests/UnitTests/NoOpMediator.cs",
                "is_test": false
            }
        ]
    });
    let calls = serde_json::json!({
        "rows": [
            {
                "from": "src.Create.ExecuteAsync",
                "to": "tests.NoOpMediator.Send",
                "confidence": "0.85"
            },
            {
                "from": "tests.NoOpMediator.Send",
                "to": "src.Create.ExecuteAsync",
                "confidence": "0.85"
            }
        ]
    });
    let inventory =
        extract_code_inventory("shop-api".to_string(), None, &empty, &code, &empty).unwrap();

    assert_eq!(
        extract_code_calls(&calls, &inventory),
        vec![CodeCall {
            from: "tests.NoOpMediator.Send".to_string(),
            to: "src.Create.ExecuteAsync".to_string(),
            confidence: Some(85),
            strategy: None,
            expression: None,
        }]
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
fn code_inventory_discards_obvious_engine_noise() {
    let routes = serde_json::json!({
        "results": [
            { "name": "/api/v1/sessions", "qualified_name": "routes.sessions", "label": "Route" },
            {
                "name": "postgresql://user:secret@localhost/app",
                "qualified_name": "__route__infra__postgresql://user:secret@localhost/app",
                "label": "Route",
                "file_path": ".github/workflows/release.yml"
            }
        ]
    });
    let code = serde_json::json!({
        "results": [
            { "name": "#[cfg", "qualified_name": "<decorator:#[cfg>", "label": "Decorator" },
            { "name": "@authenticated", "qualified_name": "auth.authenticated", "label": "Decorator" }
        ]
    });
    let files = serde_json::json!({ "results": [] });

    let inventory =
        extract_code_inventory("shop-api".to_string(), None, &routes, &code, &files).unwrap();

    assert_eq!(
        inventory
            .routes
            .iter()
            .map(|item| item.name.as_str())
            .collect::<Vec<_>>(),
        vec!["/api/v1/sessions"]
    );
    assert_eq!(
        inventory
            .unknown
            .iter()
            .map(|item| item.name.as_str())
            .collect::<Vec<_>>(),
        vec!["@authenticated"]
    );
}

#[test]
fn code_inventory_downgrades_routes_without_location_or_handles() {
    let routes = serde_json::json!({
        "results": [
            { "name": "GET /located", "qualified_name": "routes.located", "label": "Route", "file_path": "src/routes.rs" },
            { "name": "GET /handled", "qualified_name": "routes.handled", "label": "Route" },
            { "name": "GET /string-only", "qualified_name": "routes.string_only", "label": "Route" }
        ]
    });
    let code = serde_json::json!({
        "results": [
            { "name": "handled", "qualified_name": "handlers.handled", "label": "Function" }
        ]
    });
    let files = serde_json::json!({ "results": [] });
    let handles = serde_json::json!({
        "rows": [
            { "from": "handlers.handled", "to": "routes.handled" }
        ]
    });
    let mut inventory =
        extract_code_inventory("shop-api".to_string(), None, &routes, &code, &files).unwrap();
    attach_code_handles(&handles, &mut inventory);

    downgrade_unverified_routes(&mut inventory);

    assert_eq!(
        inventory
            .routes
            .iter()
            .map(|item| item.name.as_str())
            .collect::<Vec<_>>(),
        vec!["GET /handled", "GET /located"]
    );
    let unverified = inventory
        .unknown
        .iter()
        .find(|item| item.name == "GET /string-only")
        .unwrap();
    assert_eq!(unverified.kind, "unknown");
    assert_eq!(unverified.engine_label, "Route");
    assert_eq!(inventory.summary.routes, 2);
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
            { "name": "/", "qualified_name": "routes.generic", "label": "Route" },
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
    assert_eq!(inventory.routes[0].id, "routes.orders.create");
    assert_eq!(inventory.routes[1].id, "routes.generic");
    assert_eq!(inventory.handlers[0].id, "handlers.createOrder");
    assert!(inventory.functions.is_empty());
    assert_eq!(inventory.summary.handlers, 1);
    assert_eq!(inventory.summary.functions, 0);
}

#[test]
fn code_inventory_splits_collapsed_routes_into_handler_bindings() {
    let routes = serde_json::json!({
        "results": [
            { "name": "/", "qualified_name": "__route__GET__/", "label": "Route" }
        ]
    });
    let code = serde_json::json!({
        "results": [
            {
                "name": "list_events",
                "qualified_name": "handlers.events.list_events",
                "label": "Function",
                "file_path": "routes/events.py",
                "start_line": 28,
                "route_path": "/"
            },
            {
                "name": "list_sessions",
                "qualified_name": "handlers.sessions.list_sessions",
                "label": "Function",
                "file_path": "routes/sessions.py",
                "start_line": 33,
                "route_path": "/"
            }
        ]
    });
    let handles = serde_json::json!({
        "rows": [
            { "from": "handlers.events.list_events", "to": "__route__GET__/" },
            { "from": "handlers.sessions.list_sessions", "to": "__route__GET__/" }
        ]
    });
    let mut inventory = extract_code_inventory(
        "shop-api".to_string(),
        None,
        &routes,
        &code,
        &serde_json::json!({ "results": [] }),
    )
    .unwrap();

    attach_code_handles(&handles, &mut inventory);

    assert_eq!(inventory.routes.len(), 2);
    assert_eq!(inventory.handles.len(), 2);
    assert!(inventory
        .routes
        .iter()
        .all(|route| route.id.contains("#handler=") && route.file_path.is_some()));
    assert_eq!(
        inventory
            .routes
            .iter()
            .map(|route| route.file_path.as_deref().unwrap())
            .collect::<Vec<_>>(),
        vec!["routes/events.py", "routes/sessions.py"]
    );
    assert!(inventory.handles.iter().all(|handle| inventory
        .routes
        .iter()
        .any(|route| route.id == handle.route)));
}

#[test]
fn code_inventory_reads_locations_from_the_node_contract() {
    let code = serde_json::json!({
        "results": [{
            "name": "createOrder",
            "qualified_name": "handlers.createOrder",
            "label": "Function",
            "file_path": "src/handlers/orders.rs",
            "start_line": "20",
            "start_column": "3",
            "end_line": "46",
            "end_column": "18"
        }]
    });
    let inventory = extract_code_inventory(
        "shop-api".to_string(),
        None,
        &serde_json::json!({ "results": [] }),
        &code,
        &serde_json::json!({ "results": [] }),
    )
    .unwrap();
    let item = &inventory.functions[0];
    assert_eq!(
        (item.line, item.column, item.end_line, item.end_column),
        (Some(20), Some(3), Some(46), Some(18))
    );
    assert_eq!(item.file_path.as_deref(), Some("src/handlers/orders.rs"));

    let query = inventory_nodes_query();
    assert!(query.contains("node.start_line AS start_line"));
    assert!(query.contains("node.start_column AS start_column"));
    assert!(query.contains("node.end_line AS end_line"));
    assert!(query.contains("node.end_column AS end_column"));
}

fn pinned_code_field_inventory(label: &str) -> (PathBuf, CodeInventory) {
    let repo_path = std::env::var("BACKEND_MAP_TEST_CODE_REPO")
        .expect("BACKEND_MAP_TEST_CODE_REPO must point to the pinned field repository");
    let engine_path = std::env::var("BACKEND_MAP_TEST_CODE_ENGINE")
        .expect("BACKEND_MAP_TEST_CODE_ENGINE must point to codebase-memory-mcp");
    let root = temp_root(label);
    fs::create_dir_all(&root).unwrap();
    let workspace = create_workspace(
        &root,
        CreateWorkspaceRequest {
            name: format!("Pinned {label}"),
            repo_path,
        },
    )
    .unwrap();
    let registry = EngineRegistry {
        mode: engine::EngineRuntimeMode::Dev,
        engine_dir: Path::new(&engine_path)
            .parent()
            .unwrap()
            .display()
            .to_string(),
        engines: vec![engine::EngineAvailability {
            id: "codebase-memory".to_string(),
            label: "codebase-memory".to_string(),
            role: "code".to_string(),
            executable: "codebase-memory-mcp.exe".to_string(),
            expected_version: "0.9.0".to_string(),
            contract_version: "1".to_string(),
            path: engine_path,
            available: true,
            releasable: true,
            integrity: "field-test".to_string(),
            sha256: None,
            error: None,
        }],
    };

    let result = index_code_repository(
        &root,
        &registry,
        IndexCodeRequest {
            workspace_id: workspace.id,
        },
    )
    .unwrap();
    assert!(result.run.ok, "{}", result.run.stderr);
    (root, result.inventory.expect("field inventory"))
}

#[test]
#[ignore = "requires the pinned C# field repository and bundled code sidecar"]
fn code_field_fastendpoints_adapter_proves_real_routes_and_handlers() {
    let (root, inventory) = pinned_code_field_inventory("fastendpoints-field");
    let derived = inventory
        .routes
        .iter()
        .filter(|route| {
            route.detail["routePathSource"]
                .as_str()
                .is_some_and(|source| source == "fastendpoints-static-configure")
        })
        .collect::<Vec<_>>();

    assert!(
        derived.len() >= 5,
        "expected the pinned fixture's real FastEndpoints routes, got {}",
        derived.len()
    );
    let create_route = derived
        .iter()
        .find(|route| {
            route.name == "/Contributors"
                && route.detail["routeMethod"].as_str() == Some("POST")
                && route.detail["handlerQualifiedName"]
                    .as_str()
                    .is_some_and(|handler| {
                        handler.contains(
                            "Clean.Architecture.Web.Contributors.Create.Create.ExecuteAsync",
                        )
                    })
        })
        .expect("main project POST /Contributors route");
    let derived_ids = derived
        .iter()
        .map(|route| route.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let derived_handles = inventory
        .handles
        .iter()
        .filter(|handle| derived_ids.contains(handle.route.as_str()))
        .count();
    assert!(inventory
        .handles
        .iter()
        .any(|handle| handle.route == create_route.id));
    assert_eq!(derived_handles, derived.len());
    let create_handler = create_route.detail["handlerQualifiedName"]
        .as_str()
        .expect("derived route handler");
    assert!(!inventory.calls.iter().any(|call| {
        call.from == create_handler && call.to.contains("tests.Clean.Architecture.UnitTests")
    }));
    assert!(derived
        .iter()
        .all(|route| !route.name.contains("nameof") && !route.name.contains("://")));
    println!(
        "product FastEndpoints routes={} handles={} handlers={} calls={}",
        derived.len(),
        derived_handles,
        inventory.handlers.len(),
        inventory.calls.len()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "requires the pinned FastAPI field repository and bundled code sidecar"]
fn code_field_fastapi_adapter_proves_real_import_calls() {
    let (root, inventory) = pinned_code_field_inventory("fastapi-field");
    let route = inventory
        .routes
        .iter()
        .find(|route| route.name.ends_with("/login/access-token"))
        .expect("POST /login/access-token route");
    let handler = inventory
        .handles
        .iter()
        .find(|handle| handle.route == route.id)
        .map(|handle| handle.handler.as_str())
        .expect("login route handler");
    let proven = inventory
        .calls
        .iter()
        .filter(|call| {
            call.from == handler
                && call.confidence == Some(95)
                && call.strategy.as_deref() == Some("python_static_import")
        })
        .map(|call| call.to.as_str())
        .collect::<Vec<_>>();

    assert!(proven
        .iter()
        .any(|target| target.ends_with(".backend.app.crud.authenticate")));
    assert!(proven
        .iter()
        .any(|target| target.ends_with(".backend.app.core.security.create_access_token")));
    println!("product FastAPI static import calls={}", proven.len());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn code_engine_queries_use_one_bounded_node_contract_and_safe_aliases() {
    let query = inventory_nodes_query();

    assert!(query.starts_with("MATCH (node:Route|Function|Method|Class|"));
    assert!(query.contains("|Package|Resource|File)"));
    assert!(!query.contains("|Union|"));
    assert!(query.contains("labels(node) AS labels"));
    assert!(query.ends_with("LIMIT 100000"));
    assert!(!query.contains("SEMANTICALLY_RELATED"));
    assert!(!query.contains("SIMILAR_TO"));
    assert!(CALLS_QUERY.contains(" AS source"));
    assert!(CALLS_QUERY.contains(" AS target"));
    assert!(CALLS_QUERY.contains(" AS confidence"));
    assert!(CALLS_QUERY.contains(" AS strategy"));
    assert!(CALLS_QUERY.contains(" AS call_expression"));
    assert!(!CALLS_QUERY.contains(" AS from"));
    assert!(HANDLES_QUERY.starts_with("MATCH (handler)-[:HANDLES]->(route)"));
    assert!(HANDLES_QUERY.contains(" AS source"));
    assert!(HANDLES_QUERY.contains(" AS target"));
}

#[test]
fn bundled_code_engine_fixture_preserves_route_handler_call_and_location() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/codebase-memory-contract-v0.9.0.json"
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

    assert_eq!(fixture["engineVersion"], "0.9.0");
    assert_eq!(inventory.routes.len(), 2);
    assert_eq!(inventory.handlers.len(), 1);
    assert_eq!(inventory.handlers[0].name, "processCreationForm");
    assert_eq!(inventory.handlers[0].line, Some(77));
    assert_eq!(inventory.handlers[0].end_line, Some(87));
    assert_eq!(inventory.handles.len(), 1);
    assert_eq!(inventory.handles[0].route, "__route__POST__/owners/new");
    assert_eq!(inventory.calls.len(), 1);
    assert_eq!(inventory.calls[0].confidence, Some(75));
    assert_eq!(inventory.calls[0].strategy.as_deref(), Some("unique_name"));
    assert_eq!(
        inventory.calls[0].to,
        "spring-petclinic.src.main.java.org.springframework.samples.petclinic.model.BaseEntity.getId"
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

    let payload = focused_code_search_payload("shop-api", "orders", Some("^src/db/"), 100).unwrap();
    assert_eq!(payload["regex"], true);
    assert_eq!(payload["mode"], "compact");
    assert_eq!(payload["context"], 0);
    assert_eq!(payload["limit"], 32);
    assert_eq!(payload["path_filter"], "^src/db/");
    assert!(focused_code_search_payload("shop-api", "orders", Some("\n"), 8).is_err());
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
    let healthy = create_local_workspace(&root, "Healthy API");
    let corrupt = create_local_workspace(&root, "Corrupt API");
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
    let mut workspace = create_local_workspace(&root, "Shop API");
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
    let mut workspace = create_local_workspace(&root, "Shop API");
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
