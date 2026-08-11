use crate::paths::base_paths;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

static ENGINE_OPERATION_CANCELLATIONS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> =
    OnceLock::new();

pub struct EngineOperationGuard {
    operation_id: String,
}

impl Drop for EngineOperationGuard {
    fn drop(&mut self) {
        if let Some(registry) = ENGINE_OPERATION_CANCELLATIONS.get() {
            if let Ok(mut registry) = registry.lock() {
                registry.remove(&self.operation_id);
            }
        }
    }
}

pub fn begin_engine_operation(operation_id: &str) -> Result<EngineOperationGuard, String> {
    validate_operation_id(operation_id)?;
    let registry = ENGINE_OPERATION_CANCELLATIONS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut registry = registry
        .lock()
        .map_err(|_| "읽기 작업 취소 상태가 손상됐습니다".to_string())?;
    if registry.contains_key(operation_id) {
        return Err("이미 실행 중인 읽기 작업 ID입니다".to_string());
    }
    registry.insert(operation_id.to_string(), Arc::new(AtomicBool::new(false)));
    Ok(EngineOperationGuard {
        operation_id: operation_id.to_string(),
    })
}

pub fn cancel_engine_operation(operation_id: &str) -> bool {
    let Some(registry) = ENGINE_OPERATION_CANCELLATIONS.get() else {
        return false;
    };
    let Ok(registry) = registry.lock() else {
        return false;
    };
    let Some(cancellation) = registry.get(operation_id) else {
        return false;
    };
    cancellation.store(true, Ordering::Release);
    true
}

fn cancellation_for_operation(operation_id: &str) -> Option<Arc<AtomicBool>> {
    ENGINE_OPERATION_CANCELLATIONS
        .get()
        .and_then(|registry| registry.lock().ok())
        .and_then(|registry| registry.get(operation_id).cloned())
}

fn validate_operation_id(operation_id: &str) -> Result<(), String> {
    if operation_id.is_empty()
        || operation_id.len() > 128
        || operation_id.chars().any(char::is_control)
    {
        return Err("읽기 작업 ID가 올바르지 않습니다".to_string());
    }
    Ok(())
}

// A broken sidecar must not be able to exhaust the desktop process while its
// pipe is drained. Normal bounded inventory responses stay well below this.
const MAX_ENGINE_STREAM_BYTES: usize = 128 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EngineRuntimeMode {
    Dev,
    Internal,
    Production,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub role: &'static str,
    pub executable: &'static str,
    pub expected_version: &'static str,
    pub expected_contract_version: &'static str,
}

pub const CODEBASE_MEMORY_VERSION: &str = "0.1.0";
pub const CODEBASE_MEMORY_CONTRACT_VERSION: &str = "4";
pub const DATABASE_MEMORY_VERSION: &str = "0.2.0";
pub const DATABASE_MEMORY_CONTRACT_VERSION: &str = "3";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineRegistry {
    pub mode: EngineRuntimeMode,
    pub engine_dir: String,
    pub engines: Vec<EngineAvailability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineAvailability {
    pub id: String,
    pub label: String,
    pub role: String,
    pub executable: String,
    pub expected_version: String,
    pub contract_version: String,
    pub path: String,
    pub available: bool,
    pub releasable: bool,
    pub integrity: String,
    pub sha256: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EngineManifest {
    schema_version: u32,
    engines: Vec<EngineManifestEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EngineManifestEntry {
    id: String,
    version: String,
    executable: EngineManifestExecutable,
    contract_version: String,
    #[serde(default)]
    release_ready: bool,
    #[serde(default)]
    development_artifacts: Vec<DevelopmentArtifact>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EngineManifestExecutable {
    file_name: String,
    sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DevelopmentArtifact {
    sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineRunResult {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub started_at: String,
    pub finished_at: String,
}

#[derive(Debug, Clone, Copy)]
pub struct EngineRunPolicy {
    pub hard_timeout: Duration,
    pub idle_timeout: Duration,
}

impl EngineRunPolicy {
    pub fn fixed(timeout: Duration) -> Self {
        Self {
            hard_timeout: timeout,
            idle_timeout: timeout,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineProcessEvent {
    pub stream: &'static str,
    pub line: String,
}

pub type EngineObserver = Arc<dyn Fn(EngineProcessEvent) + Send + Sync>;

pub const ENGINE_SPECS: &[EngineSpec] = &[
    EngineSpec {
        id: "codebase-memory",
        label: "codebase-memory",
        role: "code",
        executable: "code-memory-language.exe",
        expected_version: CODEBASE_MEMORY_VERSION,
        expected_contract_version: CODEBASE_MEMORY_CONTRACT_VERSION,
    },
    EngineSpec {
        id: "database-memory",
        label: "rdb-memory",
        role: "db",
        executable: "database-memory.exe",
        expected_version: DATABASE_MEMORY_VERSION,
        expected_contract_version: DATABASE_MEMORY_CONTRACT_VERSION,
    },
];

pub fn resolve_engine_dir(
    mode: EngineRuntimeMode,
    app_data_dir: impl AsRef<Path>,
    resource_dir: Option<&Path>,
    exe_dir: Option<&Path>,
    override_dir: Option<&Path>,
) -> PathBuf {
    if let Some(override_dir) = override_dir {
        return override_dir.to_path_buf();
    }

    match mode {
        EngineRuntimeMode::Dev => exe_dir
            .map(|path| path.join("engines"))
            .filter(|path| path.is_dir())
            .or_else(|| {
                resource_dir
                    .map(|path| path.join("engines"))
                    .filter(|path| path.is_dir())
            })
            .unwrap_or_else(|| base_paths(app_data_dir).engines_dir),
        EngineRuntimeMode::Internal | EngineRuntimeMode::Production => resource_dir
            .map(|path| path.join("engines"))
            .or_else(|| exe_dir.map(|path| path.join("engines")))
            .unwrap_or_else(|| base_paths(app_data_dir).engines_dir),
    }
}

pub fn engine_registry(
    mode: EngineRuntimeMode,
    app_data_dir: impl AsRef<Path>,
    resource_dir: Option<&Path>,
    exe_dir: Option<&Path>,
    override_dir: Option<&Path>,
) -> EngineRegistry {
    let engine_dir = resolve_engine_dir(mode, app_data_dir, resource_dir, exe_dir, override_dir);
    let manifest = load_engine_manifest(&engine_dir.join("manifest.json"));
    let engines = ENGINE_SPECS
        .iter()
        .map(|spec| {
            let path = engine_dir.join(spec.executable);
            engine_availability(mode, spec, path, manifest.as_ref())
        })
        .collect();

    EngineRegistry {
        mode,
        engine_dir: engine_dir.display().to_string(),
        engines,
    }
}
