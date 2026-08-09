use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum AiProviderKind {
    Codex,
    Claude,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiProviderAvailability {
    pub kind: AiProviderKind,
    pub label: &'static str,
    pub installed: bool,
    pub executable: Option<String>,
    pub version: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedProvider {
    pub kind: AiProviderKind,
    pub executable: PathBuf,
    pub version: String,
    version_number: CliVersion,
}

#[derive(Debug, Clone)]
struct ProviderRecord {
    availability: AiProviderAvailability,
    runtime: Option<ResolvedProvider>,
}

/// App-lifetime registry for installed provider CLIs.
///
/// Discovery can execute several `--version` probes, so it happens once when
/// the app starts. An analysis only re-discovers when the pinned executable or
/// its compatibility contract changed. Every partition in one analysis gets
/// the same `ResolvedProvider` snapshot.
pub(crate) struct ProviderRegistry {
    records: Mutex<Vec<ProviderRecord>>,
}

impl ProviderRegistry {
    pub(crate) fn discover() -> Self {
        Self {
            records: Mutex::new(discover_provider_records()),
        }
    }

    pub(crate) fn list(&self) -> Vec<AiProviderAvailability> {
        self.records
            .lock()
            .map(|records| {
                records
                    .iter()
                    .map(|record| record.availability.clone())
                    .collect()
            })
            .unwrap_or_else(|_| provider_registry_failure())
    }

    pub(crate) fn resolve(&self, kind: AiProviderKind) -> Result<ResolvedProvider, String> {
        let mut records = self
            .records
            .lock()
            .map_err(|_| "AI 공급자 registry lock이 손상됐습니다".to_string())?;
        if let Some(runtime) = record_for(&records, kind)
            .and_then(|record| record.runtime.as_ref())
            .filter(|runtime| runtime_is_current(runtime))
        {
            return Ok(runtime.clone());
        }

        // Codex Desktop updates can rotate its hashed runtime directory while
        // this app is open. Refresh once at the analysis boundary, never once
        // per semantic partition.
        *records = discover_provider_records();
        let record = record_for(&records, kind)
            .ok_or_else(|| "AI 공급자 registry에 요청한 공급자가 없습니다".to_string())?;
        record.runtime.clone().ok_or_else(|| {
            record.availability.error.clone().unwrap_or_else(|| {
                format!("{} CLI를 사용할 수 없습니다", record.availability.label)
            })
        })
    }
}

fn record_for(records: &[ProviderRecord], kind: AiProviderKind) -> Option<&ProviderRecord> {
    records
        .iter()
        .find(|record| record.availability.kind == kind)
}

fn discover_provider_records() -> Vec<ProviderRecord> {
    vec![
        discover_codex(),
        discover_path_provider(AiProviderKind::Claude, "Claude", "claude"),
    ]
}

fn discover_codex() -> ProviderRecord {
    let paths = codex_candidate_paths();
    let candidates = paths
        .iter()
        .filter_map(|path| probe_provider(AiProviderKind::Codex, path))
        .collect::<Vec<_>>();
    let cache_version = codex_models_cache_version();

    match choose_codex_candidate(&candidates, cache_version.as_ref()) {
        Ok(runtime) => available_record("Codex", runtime),
        Err(detail) => {
            unavailable_record(AiProviderKind::Codex, "Codex", !paths.is_empty(), detail)
        }
    }
}

fn discover_path_provider(
    kind: AiProviderKind,
    label: &'static str,
    command: &str,
) -> ProviderRecord {
    let paths = command_candidates(command);
    let runtime = paths.iter().find_map(|path| probe_provider(kind, path));
    match runtime {
        Some(runtime) => available_record(label, runtime),
        None => unavailable_record(
            kind,
            label,
            !paths.is_empty(),
            format!("{label} CLI를 PATH에서 실행할 수 없습니다"),
        ),
    }
}

fn available_record(label: &'static str, runtime: ResolvedProvider) -> ProviderRecord {
    ProviderRecord {
        availability: AiProviderAvailability {
            kind: runtime.kind,
            label,
            installed: true,
            executable: Some(runtime.executable.display().to_string()),
            version: Some(runtime.version.clone()),
            error: None,
        },
        runtime: Some(runtime),
    }
}

fn unavailable_record(
    kind: AiProviderKind,
    label: &'static str,
    candidate_found: bool,
    detail: String,
) -> ProviderRecord {
    ProviderRecord {
        availability: AiProviderAvailability {
            kind,
            label,
            installed: false,
            executable: None,
            version: None,
            error: Some(if candidate_found {
                detail
            } else {
                format!("{label} CLI를 현재 기기에서 찾지 못했습니다")
            }),
        },
        runtime: None,
    }
}

fn provider_registry_failure() -> Vec<AiProviderAvailability> {
    [
        (AiProviderKind::Codex, "Codex"),
        (AiProviderKind::Claude, "Claude"),
    ]
    .into_iter()
    .map(|(kind, label)| AiProviderAvailability {
        kind,
        label,
        installed: false,
        executable: None,
        version: None,
        error: Some("AI 공급자 registry를 읽지 못했습니다".to_string()),
    })
    .collect()
}

fn runtime_is_current(runtime: &ResolvedProvider) -> bool {
    let Some(current) = probe_provider(runtime.kind, &runtime.executable) else {
        return false;
    };
    if current.version_number != runtime.version_number {
        return false;
    }
    runtime.kind != AiProviderKind::Codex
        || codex_models_cache_version()
            .is_none_or(|cache| current.version_number.core() >= cache.core())
}

fn probe_provider(kind: AiProviderKind, executable: &Path) -> Option<ResolvedProvider> {
    if !executable.is_file() {
        return None;
    }
    let output = Command::new(executable).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let version = stdout.trim().to_string();
    if version.is_empty() {
        return None;
    }
    let version_number = CliVersion::parse(&version)?;
    Some(ResolvedProvider {
        kind,
        executable: executable.to_path_buf(),
        version,
        version_number,
    })
}

fn choose_codex_candidate(
    candidates: &[ResolvedProvider],
    cache_version: Option<&CliVersion>,
) -> Result<ResolvedProvider, String> {
    let newest = candidates
        .iter()
        .max_by(|left, right| left.version_number.cmp(&right.version_number));
    let Some(newest) = newest else {
        return Err("설치된 Codex CLI 후보를 실행하지 못했습니다".to_string());
    };
    if let Some(cache_version) = cache_version {
        if newest.version_number.core() < cache_version.core() {
            return Err(format!(
                "Codex CLI 버전 충돌: 모델 캐시는 {} 형식인데 실행 가능한 최신 CLI는 {}입니다. Codex 설치를 갱신해야 합니다",
                cache_version.core_text(),
                newest.version
            ));
        }
    }
    Ok(newest.clone())
}

fn codex_candidate_paths() -> Vec<PathBuf> {
    let mut candidates = command_candidates("codex");

    #[cfg(target_os = "windows")]
    {
        if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
            let local_app_data = PathBuf::from(local_app_data);
            push_candidate(
                &mut candidates,
                local_app_data
                    .join("Programs")
                    .join("OpenAI")
                    .join("Codex")
                    .join("bin")
                    .join("codex.exe"),
            );
            let managed_root = local_app_data.join("OpenAI").join("Codex").join("bin");
            if let Ok(entries) = fs::read_dir(managed_root) {
                let mut managed = entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.path().join("codex.exe"))
                    .filter(|path| path.is_file())
                    .collect::<Vec<_>>();
                managed.sort();
                for path in managed {
                    push_candidate(&mut candidates, path);
                }
            }
        }
    }

    if let Some(home) = codex_home() {
        let executable = if cfg!(target_os = "windows") {
            "codex.exe"
        } else {
            "codex"
        };
        push_candidate(
            &mut candidates,
            home.join("packages")
                .join("standalone")
                .join("current")
                .join("bin")
                .join(executable),
        );
    }
    deduplicate_paths(candidates)
}

fn command_candidates(command: &str) -> Vec<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let Ok(output) = Command::new("where.exe").arg(command).output() else {
            return Vec::new();
        };
        if !output.status.success() {
            return Vec::new();
        }
        String::from_utf8(output.stdout)
            .ok()
            .map(|stdout| {
                stdout
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(PathBuf::from)
                    .collect()
            })
            .unwrap_or_default()
    }

    #[cfg(not(target_os = "windows"))]
    {
        let Ok(output) = Command::new("which").arg("-a").arg(command).output() else {
            return Vec::new();
        };
        if !output.status.success() {
            return Vec::new();
        }
        String::from_utf8(output.stdout)
            .ok()
            .map(|stdout| {
                stdout
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(PathBuf::from)
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn push_candidate(candidates: &mut Vec<PathBuf>, path: PathBuf) {
    if path.is_file() {
        candidates.push(path);
    }
}

fn deduplicate_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter(|path| {
            let resolved = fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            let identity = if cfg!(target_os = "windows") {
                resolved.to_string_lossy().to_lowercase()
            } else {
                resolved.to_string_lossy().into_owned()
            };
            seen.insert(identity)
        })
        .collect()
}

fn codex_home() -> Option<PathBuf> {
    env::var_os("CODEX_HOME").map(PathBuf::from).or_else(|| {
        let variable = if cfg!(target_os = "windows") {
            "USERPROFILE"
        } else {
            "HOME"
        };
        env::var_os(variable).map(|home| PathBuf::from(home).join(".codex"))
    })
}

#[derive(Deserialize)]
struct ModelsCacheHeader {
    client_version: String,
}

fn codex_models_cache_version() -> Option<CliVersion> {
    let path = codex_home()?.join("models_cache.json");
    let bytes = fs::read(path).ok()?;
    let header: ModelsCacheHeader = serde_json::from_slice(&bytes).ok()?;
    CliVersion::parse(&header.client_version)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliVersion {
    major: u64,
    minor: u64,
    patch: u64,
    prerelease: Option<String>,
}

impl CliVersion {
    fn parse(value: &str) -> Option<Self> {
        value.split_whitespace().find_map(|token| {
            let token = token.trim_start_matches('v');
            let (core, prerelease) = token
                .split_once('-')
                .map_or((token, None), |(core, suffix)| (core, Some(suffix)));
            let mut parts = core.split('.');
            let major = parts.next()?.parse().ok()?;
            let minor = parts.next()?.parse().ok()?;
            let patch = parts.next()?.parse().ok()?;
            if parts.next().is_some() {
                return None;
            }
            Some(Self {
                major,
                minor,
                patch,
                prerelease: prerelease.map(str::to_string),
            })
        })
    }

    fn core(&self) -> (u64, u64, u64) {
        (self.major, self.minor, self.patch)
    }

    fn core_text(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl Ord for CliVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        self.core()
            .cmp(&other.core())
            .then_with(|| match (&self.prerelease, &other.prerelease) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(left), Some(right)) => left.cmp(right),
            })
    }
}

impl PartialOrd for CliVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime(path: &str, version: &str) -> ResolvedProvider {
        ResolvedProvider {
            kind: AiProviderKind::Codex,
            executable: PathBuf::from(path),
            version: format!("codex-cli {version}"),
            version_number: CliVersion::parse(version).unwrap(),
        }
    }

    #[test]
    fn cli_version_parser_accepts_stable_and_desktop_prerelease_versions() {
        assert_eq!(
            CliVersion::parse("codex-cli 0.147.0-alpha.6.5")
                .unwrap()
                .core(),
            (0, 147, 0)
        );
        assert_eq!(CliVersion::parse("0.142.5").unwrap().core(), (0, 142, 5));
    }

    #[test]
    fn codex_selection_uses_the_newest_installed_compatible_cli() {
        let candidates = vec![
            runtime("standalone/codex.exe", "0.142.5"),
            runtime("desktop/codex.exe", "0.147.0-alpha.6.5"),
        ];
        let cache = CliVersion::parse("0.147.0").unwrap();

        let selected = choose_codex_candidate(&candidates, Some(&cache)).unwrap();

        assert_eq!(selected.executable, PathBuf::from("desktop/codex.exe"));
        assert_eq!(selected.version_number.core(), cache.core());
    }

    #[test]
    fn codex_selection_rejects_a_cli_older_than_the_shared_model_cache() {
        let candidates = vec![runtime("standalone/codex.exe", "0.142.5")];
        let cache = CliVersion::parse("0.147.0").unwrap();

        let error = choose_codex_candidate(&candidates, Some(&cache)).unwrap_err();

        assert!(error.contains("모델 캐시는 0.147.0"));
        assert!(error.contains("0.142.5"));
    }

    #[test]
    fn provider_registry_always_exposes_the_two_supported_adapters() {
        let registry = ProviderRegistry::discover();
        let providers = registry.list();
        assert_eq!(providers.len(), 2);
        assert!(matches!(providers[0].kind, AiProviderKind::Codex));
        assert!(matches!(providers[1].kind, AiProviderKind::Claude));
    }

    #[test]
    #[ignore = "requires an installed and authenticated Codex Desktop or standalone CLI"]
    fn installed_codex_runtime_matches_or_exceeds_the_shared_cache_version() {
        let registry = ProviderRegistry::discover();
        let runtime = registry.resolve(AiProviderKind::Codex).unwrap();
        let cache = codex_models_cache_version().unwrap();

        eprintln!(
            "selected Codex runtime: {} ({})",
            runtime.executable.display(),
            runtime.version
        );
        assert!(runtime.version_number.core() >= cache.core());
    }
}
