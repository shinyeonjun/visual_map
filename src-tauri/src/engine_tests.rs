use super::*;
use std::fs;

#[cfg(windows)]
use std::{thread, time::Duration};

#[test]
fn dev_mode_prefers_exe_engines_directory_when_present() {
    let app_data_dir = PathBuf::from(r"C:\Users\dev\AppData\Local\CodebaseWorkspace");
    let root = std::env::temp_dir().join(format!(
        "codebase-workspace-dev-engine-test-{}",
        std::process::id()
    ));
    let exe_dir = root.join("target").join("debug");
    let engine_dir = exe_dir.join("engines");

    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir_all(&engine_dir).unwrap();

    let engine_dir = resolve_engine_dir(
        EngineRuntimeMode::Dev,
        &app_data_dir,
        Some(Path::new(r"C:\Program Files\Codebase Workspace\resources")),
        Some(&exe_dir),
        None,
    );

    assert_eq!(engine_dir, exe_dir.join("engines"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn dev_mode_falls_back_to_app_data_engines_directory() {
    let app_data_dir = PathBuf::from(r"C:\Users\dev\AppData\Local\CodebaseWorkspace");

    let engine_dir = resolve_engine_dir(EngineRuntimeMode::Dev, &app_data_dir, None, None, None);

    assert_eq!(engine_dir, app_data_dir.join("engines"));
}

#[test]
fn production_mode_prefers_resource_engines_directory() {
    let app_data_dir = PathBuf::from(r"C:\Users\dev\AppData\Local\CodebaseWorkspace");
    let resource_dir = PathBuf::from(r"C:\Program Files\Codebase Workspace\resources");

    let engine_dir = resolve_engine_dir(
        EngineRuntimeMode::Production,
        &app_data_dir,
        Some(&resource_dir),
        Some(Path::new(r"C:\Program Files\Codebase Workspace")),
        None,
    );

    assert_eq!(engine_dir, resource_dir.join("engines"));
}

#[test]
fn production_mode_falls_back_to_exe_engines_directory() {
    let app_data_dir = PathBuf::from(r"C:\Users\dev\AppData\Local\CodebaseWorkspace");
    let exe_dir = PathBuf::from(r"C:\Program Files\Codebase Workspace");

    let engine_dir = resolve_engine_dir(
        EngineRuntimeMode::Production,
        app_data_dir,
        None,
        Some(&exe_dir),
        None,
    );

    assert_eq!(engine_dir, exe_dir.join("engines"));
}

#[test]
fn override_directory_wins_in_any_mode() {
    let app_data_dir = PathBuf::from(r"C:\Users\dev\AppData\Local\CodebaseWorkspace");
    let override_dir = PathBuf::from(r"D:\engines");

    let engine_dir = resolve_engine_dir(
        EngineRuntimeMode::Dev,
        app_data_dir,
        None,
        None,
        Some(&override_dir),
    );

    assert_eq!(engine_dir, override_dir);
}

#[test]
fn registry_requires_a_matching_manifest_checksum_without_running_engines() {
    let root = std::env::temp_dir().join(format!(
        "codebase-workspace-engine-test-{}",
        std::process::id()
    ));
    let engine_dir = root.join("engines");

    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir_all(&engine_dir).unwrap();
    fs::write(engine_dir.join("code-memory-language.exe"), b"").unwrap();
    write_test_manifest(
        &engine_dir,
        &sha256_file(&engine_dir.join("code-memory-language.exe")).unwrap(),
        &"0".repeat(64),
        None,
    );

    let registry = engine_registry(EngineRuntimeMode::Dev, &root, None, None, Some(&engine_dir));

    let code_engine = registry
        .engines
        .iter()
        .find(|engine| engine.id == "codebase-memory")
        .unwrap();
    let db_engine = registry
        .engines
        .iter()
        .find(|engine| engine.id == "database-memory")
        .unwrap();

    assert!(code_engine.available);
    assert!(code_engine.releasable);
    assert_eq!(code_engine.integrity, "release");
    assert!(!db_engine.available);
    assert_eq!(db_engine.integrity, "missing");

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn development_artifact_is_allowed_only_in_dev_mode() {
    let root = std::env::temp_dir().join(format!(
        "codebase-workspace-dev-artifact-test-{}",
        std::process::id()
    ));
    let engine_dir = root.join("engines");
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir_all(&engine_dir).unwrap();
    fs::write(engine_dir.join("database-memory.exe"), b"development").unwrap();
    let development_hash = sha256_file(&engine_dir.join("database-memory.exe")).unwrap();
    write_test_manifest(
        &engine_dir,
        &"0".repeat(64),
        &"1".repeat(64),
        Some(&development_hash),
    );

    let dev = engine_registry(EngineRuntimeMode::Dev, &root, None, None, Some(&engine_dir));
    let production = engine_registry(
        EngineRuntimeMode::Production,
        &root,
        None,
        None,
        Some(&engine_dir),
    );
    let internal = engine_registry(
        EngineRuntimeMode::Internal,
        &root,
        None,
        None,
        Some(&engine_dir),
    );
    let dev_db = dev
        .engines
        .iter()
        .find(|engine| engine.role == "db")
        .unwrap();
    let production_db = production
        .engines
        .iter()
        .find(|engine| engine.role == "db")
        .unwrap();
    let internal_db = internal
        .engines
        .iter()
        .find(|engine| engine.role == "db")
        .unwrap();

    assert!(dev_db.available);
    assert!(!dev_db.releasable);
    assert_eq!(dev_db.integrity, "development");
    assert!(!production_db.available);
    assert_eq!(production_db.integrity, "development-rejected");
    assert!(internal_db.available);
    assert!(!internal_db.releasable);
    assert_eq!(internal_db.integrity, "development-internal");

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unpublished_release_hash_is_never_releasable() {
    let root = std::env::temp_dir().join(format!(
        "codebase-workspace-unpublished-engine-test-{}",
        std::process::id()
    ));
    let engine_dir = root.join("engines");
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir_all(&engine_dir).unwrap();
    fs::write(
        engine_dir.join("database-memory.exe"),
        b"unpublished-release",
    )
    .unwrap();
    let hash = sha256_file(&engine_dir.join("database-memory.exe")).unwrap();
    write_test_manifest(&engine_dir, &"0".repeat(64), &hash, None);

    let dev = engine_registry(EngineRuntimeMode::Dev, &root, None, None, Some(&engine_dir));
    let production = engine_registry(
        EngineRuntimeMode::Production,
        &root,
        None,
        None,
        Some(&engine_dir),
    );
    let dev_db = dev
        .engines
        .iter()
        .find(|engine| engine.role == "db")
        .unwrap();
    let production_db = production
        .engines
        .iter()
        .find(|engine| engine.role == "db")
        .unwrap();

    assert!(dev_db.available);
    assert!(!dev_db.releasable);
    assert_eq!(dev_db.integrity, "unpublished");
    assert!(!production_db.available);
    assert_eq!(production_db.integrity, "unpublished-rejected");

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn manifest_version_and_contract_must_match_the_adapter() {
    let root = std::env::temp_dir().join(format!(
        "codebase-workspace-contract-mismatch-test-{}",
        std::process::id()
    ));
    let engine_dir = root.join("engines");
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir_all(&engine_dir).unwrap();
    fs::write(engine_dir.join("code-memory-language.exe"), b"release").unwrap();
    let hash = sha256_file(&engine_dir.join("code-memory-language.exe")).unwrap();
    write_test_manifest(&engine_dir, &hash, &"0".repeat(64), None);
    let manifest_path = engine_dir.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["engines"][0]["contractVersion"] = serde_json::json!("99");
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();

    let registry = engine_registry(EngineRuntimeMode::Dev, &root, None, None, Some(&engine_dir));
    let code = registry
        .engines
        .iter()
        .find(|engine| engine.role == "code")
        .unwrap();

    assert!(!code.available);
    assert_eq!(code.integrity, "contract-mismatch");

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn redaction_masks_common_secret_shapes() {
    let text = concat!(
        "password=hunter2 token:abc123 ",
        "postgres://app:pg_pw@localhost/shop ",
        "mysql://app:mysql_pw@localhost/shop ",
        "Server=localhost;Database=shop;User Id=app;Password=ado_pw;Pwd=short_pw; ",
        "app/oracle_pw@localhost/XEPDB1"
    );
    let redacted = redact_secrets(text);

    assert!(!redacted.contains("hunter2"));
    assert!(!redacted.contains("abc123"));
    assert!(!redacted.contains("pg_pw"));
    assert!(!redacted.contains("mysql_pw"));
    assert!(!redacted.contains("ado_pw"));
    assert!(!redacted.contains("short_pw"));
    assert!(!redacted.contains("oracle_pw"));
    assert!(redacted.contains("password=[REDACTED]"));
    assert!(redacted.contains("token:[REDACTED]"));
    assert!(redacted.contains("postgres://app:[REDACTED]@localhost/shop"));
    assert!(redacted.contains("mysql://app:[REDACTED]@localhost/shop"));
    assert!(redacted.contains("Password=[REDACTED]"));
    assert!(redacted.contains("Pwd=[REDACTED]"));
    assert!(redacted.contains("app/[REDACTED]@localhost/XEPDB1"));
}

#[test]
fn redaction_masks_json_and_spaced_key_values() {
    let text = r#"{"password":"json_pw","access_token": "json_token"} api_key = spaced_key"#;
    let redacted = redact_secrets(text);

    assert!(!redacted.contains("json_pw"));
    assert!(!redacted.contains("json_token"));
    assert!(!redacted.contains("spaced_key"));
    assert!(redacted.contains(r#""password":"[REDACTED]""#));
    assert!(redacted.contains(r#""access_token": "[REDACTED]""#));
    assert!(redacted.contains("api_key = [REDACTED]"));
}

#[test]
fn redaction_masks_standalone_provider_tokens_and_private_keys() {
    // Assemble credential-shaped fixtures at runtime so repository secret
    // scanners never mistake inert test data for a committed live credential.
    let github = ["gh", "p_", "1234567890", "abcdefghijklmnopqrstuvwxyz"].concat();
    let openai = ["sk", "-proj-", "abcdefghijklmnopqrstuvwxyz", "1234567890"].concat();
    let aws = ["AK", "IA", "1234567890ABCDEF"].concat();
    let jwt = [
        "eyJhbGciOiJIUzI1NiJ9",
        ".",
        "eyJzdWIiOiJ1c2VyLTEyMzQ1Njc4OTAifQ",
        ".",
        "abcdefghijklmnopqrstuvwxyz",
    ]
    .concat();
    let bearer = "bearer-secret-1234567890".to_string();
    let private_key =
        "-----BEGIN PRIVATE KEY-----\nraw-private-material\n-----END PRIVATE KEY-----";
    let text = format!(
        "const github = \"{github}\";\nconst model = \"{openai}\";\nconst aws = \"{aws}\";\nconst jwt = \"{jwt}\";\nAuthorization: Bearer {bearer}\n{private_key}\nconst tokenCount = 2;\nimport reset from './forgot-password/token-view';"
    );

    let redacted = redact_secrets(&text);

    for secret in [
        github.as_str(),
        openai.as_str(),
        aws.as_str(),
        jwt.as_str(),
        bearer.as_str(),
        "raw-private-material",
    ] {
        assert!(
            !redacted.contains(secret),
            "secret shape remained: {secret}"
        );
    }
    assert!(redacted.contains("[REDACTED_TOKEN]"));
    assert!(redacted.contains("[REDACTED_PRIVATE_KEY]"));
    assert!(redacted.contains("const tokenCount = 2"));
    assert!(redacted.contains("./forgot-password/token-view"));
}

#[test]
fn redaction_preserves_json_with_secret_like_code_paths() {
    let input = r#"{
        "name": "forgot-password:module",
        "password": null,
        "token": "secret-token",
        "children": [{"path": "accounts/reset-password/<token>/"}]
    }"#;
    let redacted = redact_secrets(input);
    let value = serde_json::from_str::<serde_json::Value>(&redacted).unwrap();

    assert_eq!(value["password"], "[REDACTED]");
    assert_eq!(value["token"], "[REDACTED]");
    assert_eq!(value["name"], "forgot-password:module");
    assert!(value["children"][0]["path"].is_string());
}

#[test]
fn redaction_preserves_code_graph_paths_and_import_aliases() {
    let input = r#"{
        "id": "edge:IMPORTS:apps/web/accounts/forgot-password:@/hooks",
        "target": "module:typescript:apps/admin/app/authentication/@/components",
        "library": "module:typescript:packages/ui/src/form-fields/password:external:npm:react"
    }"#;
    let redacted = redact_secrets(input);
    let value = serde_json::from_str::<serde_json::Value>(&redacted).unwrap();

    assert_eq!(
        value["id"],
        "edge:IMPORTS:apps/web/accounts/forgot-password:@/hooks"
    );
    assert_eq!(
        value["target"],
        "module:typescript:apps/admin/app/authentication/@/components"
    );
    assert_eq!(
        value["library"],
        "module:typescript:packages/ui/src/form-fields/password:external:npm:react"
    );
}

#[test]
fn command_runner_captures_process_output() {
    let result = run_command(
        &std::env::current_exe().unwrap(),
        &["--help"],
        Duration::from_secs(5),
    )
    .unwrap();

    assert!(result.ok);
    assert_eq!(result.exit_code, Some(0));
    assert!(result.stdout.contains("Usage") || result.stdout.contains("USAGE"));
}

#[cfg(windows)]
#[test]
fn command_runner_passes_explicit_environment() {
    let result = run_command_with_env(
        Path::new("cmd"),
        &["/C", "echo %BVM_TEST_ENV%"],
        Duration::from_secs(5),
        &[("BVM_TEST_ENV", "workspace-cache")],
    )
    .unwrap();

    assert!(result.ok);
    assert_eq!(result.stdout.trim(), "workspace-cache");
}

#[cfg(windows)]
#[test]
fn command_runner_delivers_provider_prompt_through_stdin() {
    let current_dir = std::env::temp_dir();
    let result = run_command_with_input(
        Path::new("node.exe"),
        &[
            "-e",
            "let body='';process.stdin.setEncoding('utf8');process.stdin.on('data',chunk=>body+=chunk);process.stdin.on('end',()=>process.stdout.write(body))",
        ],
        EngineRunPolicy::fixed(Duration::from_secs(5)),
        &[],
        &current_dir,
        b"bounded semantic prompt".to_vec(),
    )
    .unwrap();

    assert!(result.ok);
    assert_eq!(result.stdout, "bounded semantic prompt");
}

#[cfg(windows)]
#[test]
fn run_engine_command_preserves_empty_arguments() {
    let engine = EngineAvailability {
        id: "test-node".to_string(),
        label: "test-node".to_string(),
        role: "test".to_string(),
        executable: "node.exe".to_string(),
        expected_version: "test".to_string(),
        contract_version: "test".to_string(),
        path: "node.exe".to_string(),
        available: true,
        releasable: false,
        integrity: "test".to_string(),
        sha256: None,
        error: None,
    };
    let args = vec![
        "-e".to_string(),
        "console.log(process.argv.slice(1).map((value) => value === '' ? 'empty' : value).join('|'))"
            .to_string(),
        String::new(),
        "marker".to_string(),
    ];

    let result = run_engine_command(&engine, &args, Duration::from_secs(5)).unwrap();

    assert!(result.ok);
    assert_eq!(result.stdout.trim(), "empty|marker");
}

#[cfg(windows)]
#[test]
fn command_runner_drains_output_larger_than_pipe_buffers() {
    let result = run_command(
        Path::new("node.exe"),
        &["-e", "process.stdout.write('x'.repeat(1024 * 1024))"],
        Duration::from_secs(10),
    )
    .unwrap();

    assert!(result.ok);
    assert_eq!(result.stdout.len(), 1024 * 1024);
}

#[cfg(windows)]
#[test]
fn command_runner_reports_timeout() {
    let result = run_command(
        Path::new("node.exe"),
        &["-e", "setTimeout(() => {}, 5000)"],
        Duration::from_millis(50),
    )
    .unwrap();

    assert!(!result.ok);
    assert_eq!(result.exit_code, None);
    assert!(result.stderr.contains("시간이 초과되었습니다"));
}

#[cfg(windows)]
#[test]
fn command_runner_forwards_progress_and_stops_after_idle_timeout() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&events);
    let observer: EngineObserver = Arc::new(move |event| {
        observed.lock().unwrap().push(event);
    });
    let result = run_command_with_env_observer(
        Path::new("node.exe"),
        &[
            "-e",
            "console.error('@codebase-workspace-progress {\"stage\":\"test\"}'); setTimeout(() => {}, 5000)",
        ],
        EngineRunPolicy {
            hard_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_millis(500),
        },
        &[],
        Some(observer),
    )
    .unwrap();

    assert!(!result.ok);
    assert!(result.stderr.contains("진행 신호가 없어"));
    assert_eq!(events.lock().unwrap()[0].stream, "stderr");
    assert!(events.lock().unwrap()[0]
        .line
        .contains("\"stage\":\"test\""));
}

#[cfg(windows)]
#[test]
fn command_runner_kills_a_cancelled_operation() {
    let operation_id = format!("process-cancel-test-{}", std::process::id());
    let guard = begin_engine_operation(&operation_id).unwrap();
    let runner = thread::spawn({
        let operation_id = operation_id.clone();
        move || {
            run_command_with_env(
                Path::new("node.exe"),
                &[
                    "-e",
                    "require('child_process').spawn(process.execPath,['-e','setTimeout(()=>{},5000)'],{stdio:'inherit'});setTimeout(()=>{},5000)",
                ],
                Duration::from_secs(10),
                &[("CODEBASE_WORKSPACE_OPERATION_ID", operation_id.as_str())],
            )
            .unwrap()
        }
    });
    thread::sleep(Duration::from_millis(50));
    assert!(cancel_engine_operation(&operation_id));
    let result = runner.join().unwrap();

    assert!(!result.ok);
    assert!(result.stderr.contains("취소되었습니다"));
    drop(guard);
}

#[test]
fn process_stream_reader_drains_but_does_not_store_past_its_limit() {
    let input = std::io::Cursor::new(vec![b'x'; 1024]);
    let captured = read_process_stream_with_limit(input, 128).unwrap();

    assert_eq!(captured.bytes.len(), 128);
    assert!(captured.exceeded_limit);
}

#[test]
fn sidecar_args_reject_installer_and_mcp_registration_paths() {
    assert_eq!(
        sidecar_args(["install"]).unwrap_err(),
        "허용되지 않는 sidecar 실행 인자입니다"
    );
    assert_eq!(
        sidecar_args(["cli", "register-mcp"]).unwrap_err(),
        "허용되지 않는 sidecar 실행 인자입니다"
    );
    assert_eq!(
        sidecar_args(["setup.ps1"]).unwrap_err(),
        "허용되지 않는 sidecar 실행 인자입니다"
    );
    assert_eq!(
        sidecar_args(["--config", "claude_desktop_config.json"]).unwrap_err(),
        "허용되지 않는 sidecar 실행 인자입니다"
    );
}

#[test]
fn engine_operation_cancellation_is_scoped_and_released() {
    let operation_id = format!("cancel-test-{}", std::process::id());
    let guard = begin_engine_operation(&operation_id).unwrap();
    assert!(cancel_engine_operation(&operation_id));
    assert!(!cancel_engine_operation("missing-operation"));
    drop(guard);
    assert!(!cancel_engine_operation(&operation_id));
}

#[test]
fn run_engine_command_rejects_missing_engine_before_spawn() {
    let engine = EngineAvailability {
        id: "codebase-memory".to_string(),
        label: "codebase-memory".to_string(),
        role: "code".to_string(),
        executable: "code-memory-language.exe".to_string(),
        expected_version: "0.1.0".to_string(),
        contract_version: "1".to_string(),
        path: r"D:\missing\code-memory-language.exe".to_string(),
        available: false,
        releasable: false,
        integrity: "missing".to_string(),
        sha256: None,
        error: Some("missing".to_string()),
    };
    let args = sidecar_args(["--version"]).unwrap();

    assert_eq!(
        run_engine_command(&engine, &args, Duration::from_secs(1)).unwrap_err(),
        "읽기 도구가 없습니다: code-memory-language.exe"
    );
}

fn write_test_manifest(
    engine_dir: &Path,
    code_release_hash: &str,
    db_release_hash: &str,
    db_development_hash: Option<&str>,
) {
    let development_artifacts = db_development_hash
        .map(|sha256| vec![serde_json::json!({ "sha256": sha256 })])
        .unwrap_or_default();
    let manifest = serde_json::json!({
        "schemaVersion": 1,
        "engines": [
            {
                "id": "codebase-memory",
                "version": "0.1.0",
                "contractVersion": CODEBASE_MEMORY_CONTRACT_VERSION,
                "releaseReady": true,
                "executable": {
                    "fileName": "code-memory-language.exe",
                    "sha256": code_release_hash
                },
                "developmentArtifacts": []
            },
            {
                "id": "database-memory",
                "version": DATABASE_MEMORY_VERSION,
                "contractVersion": DATABASE_MEMORY_CONTRACT_VERSION,
                "releaseReady": false,
                "executable": {
                    "fileName": "database-memory.exe",
                    "sha256": db_release_hash
                },
                "developmentArtifacts": development_artifacts
            }
        ]
    });
    fs::write(
        engine_dir.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
}
