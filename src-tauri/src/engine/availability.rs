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

pub(crate) fn run_engine_command_with_env_observer(
    engine: &EngineAvailability,
    args: &[String],
    policy: EngineRunPolicy,
    envs: &[(&str, &str)],
    observer: Option<EngineObserver>,
) -> Result<EngineRunResult, String> {
    if !engine.available {
        return Err(format!("읽기 도구가 없습니다: {}", engine.executable));
    }

    validate_sidecar_args(args)?;
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    run_command_with_env_observer(Path::new(&engine.path), &arg_refs, policy, envs, observer)
}
