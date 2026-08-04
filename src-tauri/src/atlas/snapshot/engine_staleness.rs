fn engine_version(registry: &EngineRegistry, engine_id: &str) -> Option<String> {
    registry
        .engines
        .iter()
        .find(|engine| engine.id == engine_id)
        .map(|engine| engine.expected_version.trim())
        .filter(|version| !version.is_empty() && *version != "unknown")
        .map(str::to_string)
}

fn engine_checksum(registry: &EngineRegistry, engine_id: &str) -> Option<String> {
    registry
        .engines
        .iter()
        .find(|engine| engine.id == engine_id)
        .and_then(|engine| engine.sha256.as_deref())
        .map(str::trim)
        .filter(|checksum| !checksum.is_empty())
        .map(str::to_string)
}

fn engine_contract_version(registry: &EngineRegistry, engine_id: &str) -> Option<String> {
    registry
        .engines
        .iter()
        .find(|engine| engine.id == engine_id)
        .map(|engine| engine.contract_version.trim())
        .filter(|version| !version.is_empty())
        .map(str::to_string)
}

fn mark_engine_staleness(
    metadata: &SnapshotSourceMetadata,
    registry: &EngineRegistry,
    engine_id: &str,
    label: &str,
    reasons: &mut Vec<String>,
) {
    let Some(current) = registry
        .engines
        .iter()
        .find(|engine| engine.id == engine_id)
    else {
        push_unique(reasons, &format!("{label} 엔진 정보를 확인할 수 없습니다"));
        return;
    };
    if metadata.engine_id.as_deref() != Some(engine_id) {
        push_unique(reasons, &format!("{label} 엔진 식별자가 바뀌었습니다"));
    }
    if engine_version(registry, engine_id).as_deref() != metadata.engine_version.as_deref() {
        push_unique(reasons, &format!("{label} 엔진 버전이 바뀌었습니다"));
    }
    let current_checksum = current.sha256.as_deref().map(str::trim);
    let saved_checksum = metadata.engine_checksum.as_deref().map(str::trim);
    if current_checksum.is_none()
        || saved_checksum.is_none()
        || !current_checksum
            .zip(saved_checksum)
            .is_some_and(|(current, saved)| current.eq_ignore_ascii_case(saved))
    {
        push_unique(
            reasons,
            &format!("{label} 엔진 checksum이 바뀌었거나 확인되지 않았습니다"),
        );
    }
    if engine_contract_version(registry, engine_id).as_deref()
        != metadata.contract_version.as_deref()
    {
        push_unique(reasons, &format!("{label} 엔진 contract가 바뀌었습니다"));
    }
}

fn mark_source_revision_staleness(
    saved: Option<&str>,
    current: Option<&(String, String)>,
    source_exists: bool,
    missing_reason: &str,
    changed_reason: &str,
    unavailable_reason: &str,
    reasons: &mut Vec<String>,
) {
    match (saved, current) {
        (Some(saved), Some((current, _))) if saved != current => {
            push_unique(reasons, changed_reason)
        }
        (Some(_), None) => push_unique(reasons, unavailable_reason),
        (None, Some(_)) => push_unique(reasons, missing_reason),
        (None, None) if source_exists => push_unique(reasons, missing_reason),
        (None, None) => push_unique(reasons, unavailable_reason),
        _ => {}
    }
}
