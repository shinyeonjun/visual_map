use crate::paths::base_paths;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, OnceLock},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

type EngineHashEntry = (u64, SystemTime, String);
type EngineHashCache = Mutex<HashMap<PathBuf, EngineHashEntry>>;

static ENGINE_HASH_CACHE: OnceLock<EngineHashCache> = OnceLock::new();

// A broken sidecar must not be able to exhaust the desktop process while its
// pipe is drained. Normal bounded inventory responses stay well below this.
const MAX_ENGINE_STREAM_BYTES: usize = 128 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum EngineRuntimeMode {
    Dev,
    Internal,
    Production,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EngineSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub role: &'static str,
    pub executable: &'static str,
    pub expected_version: &'static str,
    pub expected_contract_version: &'static str,
}

pub(crate) const CODEBASE_MEMORY_VERSION: &str = "0.9.0";
pub(crate) const CODEBASE_MEMORY_CONTRACT_VERSION: &str = "1";
pub(crate) const DATABASE_MEMORY_VERSION: &str = "0.2.0";
pub(crate) const DATABASE_MEMORY_CONTRACT_VERSION: &str = "2";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EngineRegistry {
    pub mode: EngineRuntimeMode,
    pub engine_dir: String,
    pub engines: Vec<EngineAvailability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EngineAvailability {
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
pub(crate) struct EngineRunResult {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub started_at: String,
    pub finished_at: String,
}

pub(crate) const ENGINE_SPECS: &[EngineSpec] = &[
    EngineSpec {
        id: "codebase-memory",
        label: "codebase-memory",
        role: "code",
        executable: "codebase-memory-mcp.exe",
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

pub(crate) fn resolve_engine_dir(
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

pub(crate) fn engine_registry(
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

fn engine_availability(
    mode: EngineRuntimeMode,
    spec: &EngineSpec,
    path: PathBuf,
    manifest: Result<&EngineManifest, &String>,
) -> EngineAvailability {
    let base = |expected_version: String,
                contract_version: String,
                available: bool,
                releasable: bool,
                integrity: &str,
                sha256: Option<String>,
                error: Option<String>| EngineAvailability {
        id: spec.id.to_string(),
        label: spec.label.to_string(),
        role: spec.role.to_string(),
        executable: spec.executable.to_string(),
        expected_version,
        contract_version,
        path: path.display().to_string(),
        available,
        releasable,
        integrity: integrity.to_string(),
        sha256,
        error,
    };

    let manifest = match manifest {
        Ok(manifest) => manifest,
        Err(error) => {
            return base(
                spec.expected_version.to_string(),
                "unknown".to_string(),
                false,
                false,
                "manifest-error",
                None,
                Some(error.clone()),
            )
        }
    };
    let Some(entry) = manifest.engines.iter().find(|entry| entry.id == spec.id) else {
        return base(
            spec.expected_version.to_string(),
            "unknown".to_string(),
            false,
            false,
            "manifest-error",
            None,
            Some(format!("엔진 manifest에 '{}' 항목이 없습니다", spec.id)),
        );
    };
    if entry.executable.file_name != spec.executable {
        return base(
            entry.version.clone(),
            entry.contract_version.clone(),
            false,
            false,
            "manifest-error",
            None,
            Some(format!(
                "엔진 manifest 실행 파일명이 일치하지 않습니다: expected {}, got {}",
                spec.executable, entry.executable.file_name
            )),
        );
    }
    if entry.version != spec.expected_version
        || entry.contract_version != spec.expected_contract_version
    {
        return base(
            entry.version.clone(),
            entry.contract_version.clone(),
            false,
            false,
            "contract-mismatch",
            None,
            Some(format!(
                "엔진 계약이 어댑터와 맞지 않습니다: expected version {} contract {}, got version {} contract {}",
                spec.expected_version,
                spec.expected_contract_version,
                entry.version,
                entry.contract_version
            )),
        );
    }
    if !path.is_file() {
        return base(
            entry.version.clone(),
            entry.contract_version.clone(),
            false,
            false,
            "missing",
            None,
            Some(format!("읽기 도구가 없습니다: {}", spec.executable)),
        );
    }

    let actual_hash = match sha256_file(&path) {
        Ok(hash) => hash,
        Err(error) => {
            return base(
                entry.version.clone(),
                entry.contract_version.clone(),
                false,
                false,
                "unreadable",
                None,
                Some(format!("읽기 도구 체크섬을 계산하지 못했습니다: {error}")),
            )
        }
    };
    if actual_hash.eq_ignore_ascii_case(&entry.executable.sha256) && entry.release_ready {
        return base(
            entry.version.clone(),
            entry.contract_version.clone(),
            true,
            true,
            "release",
            Some(actual_hash),
            None,
        );
    }
    if actual_hash.eq_ignore_ascii_case(&entry.executable.sha256) {
        return match mode {
            EngineRuntimeMode::Dev => base(
                entry.version.clone(),
                entry.contract_version.clone(),
                true,
                false,
                "unpublished",
                Some(actual_hash),
                Some("공식 배포 준비가 끝나지 않은 엔진입니다".to_string()),
            ),
            EngineRuntimeMode::Internal => base(
                entry.version.clone(),
                entry.contract_version.clone(),
                true,
                false,
                "unpublished-internal",
                Some(actual_hash),
                Some("내부 전용 빌드에만 포함된 미공개 엔진입니다".to_string()),
            ),
            EngineRuntimeMode::Production => base(
                entry.version.clone(),
                entry.contract_version.clone(),
                false,
                false,
                "unpublished-rejected",
                Some(actual_hash),
                Some(
                    "공식 배포 준비가 끝나지 않은 엔진은 배포 앱에서 사용할 수 없습니다"
                        .to_string(),
                ),
            ),
        };
    }
    let declared_development = entry
        .development_artifacts
        .iter()
        .any(|artifact| actual_hash.eq_ignore_ascii_case(&artifact.sha256));
    if matches!(mode, EngineRuntimeMode::Dev | EngineRuntimeMode::Internal) && declared_development
    {
        return base(
            entry.version.clone(),
            entry.contract_version.clone(),
            true,
            false,
            if mode == EngineRuntimeMode::Internal {
                "development-internal"
            } else {
                "development"
            },
            Some(actual_hash),
            Some(if mode == EngineRuntimeMode::Internal {
                "내부 전용 빌드에만 포함된 개발 엔진입니다. 재배포할 수 없습니다".to_string()
            } else {
                "개발용 엔진입니다. 배포 빌드에서는 사용할 수 없습니다".to_string()
            }),
        );
    }

    base(
        entry.version.clone(),
        entry.contract_version.clone(),
        false,
        false,
        if declared_development {
            "development-rejected"
        } else {
            "mismatch"
        },
        Some(actual_hash),
        Some("읽기 도구 체크섬이 manifest와 일치하지 않습니다".to_string()),
    )
}

fn load_engine_manifest(path: &Path) -> Result<EngineManifest, String> {
    let json = std::fs::read_to_string(path)
        .map_err(|error| format!("엔진 manifest를 열지 못했습니다: {error}"))?;
    let manifest: EngineManifest = serde_json::from_str(&json)
        .map_err(|error| format!("엔진 manifest 형식이 올바르지 않습니다: {error}"))?;
    if manifest.schema_version != 1 {
        return Err(format!(
            "지원하지 않는 엔진 manifest 버전입니다: {}",
            manifest.schema_version
        ));
    }
    let mut ids = HashSet::new();
    for entry in &manifest.engines {
        if !ids.insert(entry.id.as_str()) {
            return Err(format!("엔진 manifest ID가 중복됩니다: {}", entry.id));
        }
        if Path::new(&entry.executable.file_name)
            .file_name()
            .and_then(|name| name.to_str())
            != Some(entry.executable.file_name.as_str())
        {
            return Err(format!(
                "엔진 manifest 실행 파일명에 경로를 사용할 수 없습니다: {}",
                entry.executable.file_name
            ));
        }
        if !is_sha256(&entry.executable.sha256)
            || entry
                .development_artifacts
                .iter()
                .any(|artifact| !is_sha256(&artifact.sha256))
        {
            return Err(format!(
                "엔진 manifest 체크섬이 올바르지 않습니다: {}",
                entry.id
            ));
        }
    }
    Ok(manifest)
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let metadata = path.metadata().map_err(|error| error.to_string())?;
    let modified = metadata.modified().map_err(|error| error.to_string())?;
    let cache = ENGINE_HASH_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some((_, _, hash)) = cache
        .lock()
        .map_err(|_| "엔진 체크섬 캐시 잠금이 손상됐습니다".to_string())?
        .get(path)
        .filter(|(length, cached_modified, _)| {
            *length == metadata.len() && *cached_modified == modified
        })
        .cloned()
    {
        return Ok(hash);
    }

    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let hash = format!("{:X}", hasher.finalize());
    cache
        .lock()
        .map_err(|_| "엔진 체크섬 캐시 잠금이 손상됐습니다".to_string())?
        .insert(path.to_path_buf(), (metadata.len(), modified, hash.clone()));
    Ok(hash)
}

pub(crate) fn sidecar_args<const N: usize>(args: [&str; N]) -> Result<Vec<String>, String> {
    let args = args
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    validate_sidecar_args(&args)?;
    Ok(args)
}

pub(crate) fn run_engine_command(
    engine: &EngineAvailability,
    args: &[String],
    timeout: Duration,
) -> Result<EngineRunResult, String> {
    if !engine.available {
        return Err(format!("읽기 도구가 없습니다: {}", engine.executable));
    }

    validate_sidecar_args(args)?;
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();

    run_command(Path::new(&engine.path), &arg_refs, timeout)
}

pub(crate) fn run_engine_command_with_env(
    engine: &EngineAvailability,
    args: &[String],
    timeout: Duration,
    envs: &[(&str, &str)],
) -> Result<EngineRunResult, String> {
    if !engine.available {
        return Err(format!("읽기 도구가 없습니다: {}", engine.executable));
    }

    validate_sidecar_args(args)?;
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();

    run_command_with_env(Path::new(&engine.path), &arg_refs, timeout, envs)
}

fn validate_sidecar_args(args: &[String]) -> Result<(), String> {
    for arg in args {
        let lower = arg.to_ascii_lowercase();
        let token = lower.trim_start_matches('-');
        if matches!(
            token,
            "install" | "installer" | "setup" | "register" | "register-mcp" | "mcp-register"
        ) || lower.ends_with(".ps1")
            || lower.ends_with(".bat")
            || lower.ends_with(".cmd")
            || lower.ends_with(".msi")
            || lower.contains("claude_desktop_config")
            || lower.contains("mcp_config")
        {
            return Err("허용되지 않는 sidecar 실행 인자입니다".to_string());
        }
    }

    Ok(())
}

pub(crate) fn run_command(
    executable: &Path,
    args: &[&str],
    timeout: Duration,
) -> Result<EngineRunResult, String> {
    run_command_with_env(executable, args, timeout, &[])
}

pub(crate) fn run_command_with_env(
    executable: &Path,
    args: &[&str],
    timeout: Duration,
    envs: &[(&str, &str)],
) -> Result<EngineRunResult, String> {
    let started_at = timestamp();
    let deadline = Instant::now() + timeout;
    let mut child = Command::new(executable)
        .args(args)
        .envs(envs.iter().copied())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("읽기 도구 실행 실패: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "읽기 도구 stdout을 열지 못했습니다".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "읽기 도구 stderr를 열지 못했습니다".to_string())?;
    let stdout_reader = thread::spawn(move || read_process_stream(stdout));
    let stderr_reader = thread::spawn(move || read_process_stream(stderr));

    loop {
        let status = match child.try_wait() {
            Ok(status) => status,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = collect_process_streams(stdout_reader, stderr_reader);
                return Err(format!("읽기 도구 상태 확인 실패: {error}"));
            }
        };
        if let Some(status) = status {
            let (stdout, stderr) = collect_process_streams(stdout_reader, stderr_reader)?;

            return Ok(EngineRunResult {
                ok: status.success(),
                stdout: redact_secrets(&String::from_utf8_lossy(&stdout)),
                stderr: redact_secrets(&String::from_utf8_lossy(&stderr)),
                exit_code: status.code(),
                started_at,
                finished_at: timestamp(),
            });
        }

        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            let (stdout, stderr) = collect_process_streams(stdout_reader, stderr_reader)?;
            let stderr = String::from_utf8_lossy(&stderr);
            let stderr = if stderr.trim().is_empty() {
                "읽기 도구 실행 시간이 초과되었습니다".to_string()
            } else {
                format!(
                    "{}\n읽기 도구 실행 시간이 초과되었습니다",
                    stderr.trim_end()
                )
            };

            return Ok(EngineRunResult {
                ok: false,
                stdout: redact_secrets(&String::from_utf8_lossy(&stdout)),
                stderr: redact_secrets(&stderr),
                exit_code: None,
                started_at,
                finished_at: timestamp(),
            });
        }

        thread::sleep(Duration::from_millis(10));
    }
}

struct CapturedProcessStream {
    bytes: Vec<u8>,
    exceeded_limit: bool,
}

fn read_process_stream(stream: impl Read) -> Result<CapturedProcessStream, String> {
    read_process_stream_with_limit(stream, MAX_ENGINE_STREAM_BYTES)
}

fn read_process_stream_with_limit(
    mut stream: impl Read,
    limit: usize,
) -> Result<CapturedProcessStream, String> {
    let mut output = Vec::with_capacity(limit.min(64 * 1024));
    let mut buffer = [0u8; 64 * 1024];
    let mut exceeded_limit = false;

    loop {
        let read = stream
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(output.len());
        let stored = remaining.min(read);
        output.extend_from_slice(&buffer[..stored]);
        exceeded_limit |= stored < read;
    }

    Ok(CapturedProcessStream {
        bytes: output,
        exceeded_limit,
    })
}

fn collect_process_streams(
    stdout: thread::JoinHandle<Result<CapturedProcessStream, String>>,
    stderr: thread::JoinHandle<Result<CapturedProcessStream, String>>,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    let stdout = stdout
        .join()
        .map_err(|_| "읽기 도구 stdout 수집 작업이 중단됐습니다".to_string())??;
    let stderr = stderr
        .join()
        .map_err(|_| "읽기 도구 stderr 수집 작업이 중단됐습니다".to_string())??;
    if stdout.exceeded_limit || stderr.exceeded_limit {
        return Err(format!(
            "읽기 도구 출력이 안전 한도({MAX_ENGINE_STREAM_BYTES} bytes)를 초과했습니다"
        ));
    }
    Ok((stdout.bytes, stderr.bytes))
}

pub(crate) fn redact_secrets(input: &str) -> String {
    let mut redacted = redact_key_values(input);
    redacted = redact_url_passwords(&redacted);
    redacted = redact_oracle_connect_strings(&redacted);
    redacted
}

fn redact_key_values(input: &str) -> String {
    let secret_keys = [
        "password",
        "passwd",
        "pwd",
        "token",
        "secret",
        "api_key",
        "apikey",
        "access_token",
    ];
    let mut output = input.to_string();

    for key in secret_keys {
        let mut search_start = 0;
        loop {
            let lower = output.to_ascii_lowercase();
            let Some(offset) = lower[search_start..].find(key) else {
                break;
            };
            let key_start = search_start + offset;
            let key_end = key_start + key.len();
            let Some((value_start, value_end)) = secret_value_range(&output, key_start, key_end)
            else {
                search_start = key_end;
                continue;
            };

            output.replace_range(value_start..value_end, "[REDACTED]");
            search_start = value_start + "[REDACTED]".len();
        }
    }

    output
}

fn secret_value_range(input: &str, key_start: usize, key_end: usize) -> Option<(usize, usize)> {
    let bytes = input.as_bytes();
    if key_start > 0 {
        let previous = bytes[key_start - 1];
        if previous.is_ascii_alphanumeric() || previous == b'_' {
            return None;
        }
    }

    let mut cursor = key_end;
    if matches!(bytes.get(cursor), Some(b'"' | b'\'')) {
        cursor += 1;
    }
    while matches!(bytes.get(cursor), Some(byte) if byte.is_ascii_whitespace()) {
        cursor += 1;
    }
    if !matches!(bytes.get(cursor), Some(b'=' | b':')) {
        return None;
    }
    cursor += 1;
    while matches!(bytes.get(cursor), Some(byte) if byte.is_ascii_whitespace()) {
        cursor += 1;
    }

    let quote = match bytes.get(cursor) {
        Some(b'"') => Some(b'"'),
        Some(b'\'') => Some(b'\''),
        _ => None,
    };
    let value_start = cursor + usize::from(quote.is_some());
    let value_end = if let Some(quote) = quote {
        input[value_start..]
            .find(quote as char)
            .map(|offset| value_start + offset)
            .unwrap_or(input.len())
    } else {
        input[value_start..]
            .find(|character: char| {
                character.is_whitespace() || matches!(character, '&' | ';' | ',' | '"' | '\'')
            })
            .map(|offset| value_start + offset)
            .unwrap_or(input.len())
    };

    Some((value_start, value_end))
}

fn redact_url_passwords(input: &str) -> String {
    let mut output = input.to_string();
    let mut search_from = 0;

    while let Some(scheme_offset) = output[search_from..].find("://") {
        let auth_start = search_from + scheme_offset + 3;
        let rest = &output[auth_start..];
        let Some(at_offset) = rest.find('@') else {
            break;
        };
        let at = auth_start + at_offset;
        let userinfo = &output[auth_start..at];
        if let Some(colon_offset) = userinfo.rfind(':') {
            let password_start = auth_start + colon_offset + 1;
            output.replace_range(password_start..at, "[REDACTED]");
            search_from = password_start + "[REDACTED]".len();
        } else {
            search_from = at + 1;
        }
    }

    output
}

fn redact_oracle_connect_strings(input: &str) -> String {
    let mut output = input.to_string();
    let mut search_from = 0;

    while let Some(at_offset) = output[search_from..].find('@') {
        let at = search_from + at_offset;
        let token_start = output[..at]
            .rfind(|character: char| {
                character.is_whitespace() || matches!(character, '"' | '\'' | ';' | ',' | '(' | ')')
            })
            .map(|offset| offset + 1)
            .unwrap_or(0);
        let token = &output[token_start..at];

        if token.contains("://") {
            search_from = at + 1;
            continue;
        }

        if let Some(slash_offset) = token.rfind('/') {
            let password_start = token_start + slash_offset + 1;
            if password_start < at && slash_offset > 0 {
                output.replace_range(password_start..at, "[REDACTED]");
                search_from = password_start + "[REDACTED]".len() + 1;
                continue;
            }
        }

        search_from = at + 1;
    }

    output
}

fn timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
