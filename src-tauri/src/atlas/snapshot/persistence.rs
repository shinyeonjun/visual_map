#[cfg(test)]
pub(crate) fn normalize_inventory(
    workspace_id: String,
    code: Option<&CodeInventory>,
    db: Option<&DbInventory>,
) -> InventorySnapshot {
    build_inventory_snapshot(workspace_id, code, db)
}

pub(crate) fn snapshot_with_metadata(
    mut snapshot: InventorySnapshot,
    workspace: &Workspace,
    registry: &EngineRegistry,
) -> InventorySnapshot {
    let saved_at = timestamp();
    let has_code =
        snapshot.metadata.code.is_some() || snapshot.items.iter().any(|entry| entry.is_code());
    let has_db =
        snapshot.metadata.db.is_some() || snapshot.items.iter().any(|entry| entry.source == "db");
    let code_revision = has_code.then(|| code_source_revision(workspace)).flatten();
    let profile = workspace.active_db_profile_id.as_deref().and_then(|id| {
        workspace
            .db_profiles
            .iter()
            .find(|profile| profile.id == id)
    });
    let db_revision = profile.and_then(db_source_revision);
    snapshot.saved_at = saved_at.clone();
    snapshot.metadata.code = has_code.then(|| SnapshotSourceMetadata {
        saved_at: saved_at.clone(),
        engine_id: Some("codebase-memory".to_string()),
        engine_version: engine_version(registry, "codebase-memory"),
        engine_checksum: engine_checksum(registry, "codebase-memory"),
        adapter_version: Some(CODE_ADAPTER_VERSION.to_string()),
        contract_version: engine_contract_version(registry, "codebase-memory"),
        snapshot_key: None,
        limit_requested: None,
        limit_applied: None,
        limit_clamped: None,
        result_count: None,
        total_tables: None,
        truncated: None,
        source_revision: code_revision.as_ref().map(|(revision, _)| revision.clone()),
        source_revision_label: code_revision.map(|(_, label)| label),
        source_path: Some(workspace.repo_path.clone()),
        source_type: code_source_type(workspace),
        profile_id: None,
    });
    let previous_db_metadata = snapshot.metadata.db.clone();
    let db_contract_version = previous_db_metadata
        .as_ref()
        .and_then(|metadata| metadata.contract_version.clone());
    let db_snapshot_key = previous_db_metadata
        .as_ref()
        .and_then(|metadata| metadata.snapshot_key.clone());
    snapshot.metadata.db = has_db.then(|| SnapshotSourceMetadata {
        saved_at: saved_at.clone(),
        engine_id: Some("database-memory".to_string()),
        engine_version: engine_version(registry, "database-memory"),
        engine_checksum: engine_checksum(registry, "database-memory"),
        adapter_version: previous_db_metadata
            .as_ref()
            .and_then(|metadata| metadata.adapter_version.clone()),
        contract_version: db_contract_version
            .or_else(|| engine_contract_version(registry, "database-memory")),
        snapshot_key: db_snapshot_key,
        limit_requested: previous_db_metadata
            .as_ref()
            .and_then(|metadata| metadata.limit_requested),
        limit_applied: previous_db_metadata
            .as_ref()
            .and_then(|metadata| metadata.limit_applied),
        limit_clamped: previous_db_metadata
            .as_ref()
            .and_then(|metadata| metadata.limit_clamped),
        result_count: previous_db_metadata
            .as_ref()
            .and_then(|metadata| metadata.result_count),
        total_tables: previous_db_metadata
            .as_ref()
            .and_then(|metadata| metadata.total_tables),
        truncated: previous_db_metadata
            .as_ref()
            .and_then(|metadata| metadata.truncated),
        source_revision: db_revision.as_ref().map(|(revision, _)| revision.clone()),
        source_revision_label: db_revision
            .map(|(_, label)| label)
            .or_else(|| profile.map(|_| "외부 DB · 마지막 읽기 기준".to_string())),
        source_path: profile.and_then(|profile| profile.path.clone()),
        source_type: profile
            .map(|profile| db_source_key(&profile.source))
            .unwrap_or_else(|| "unknown".to_string()),
        profile_id: profile.map(|profile| profile.id.clone()),
    });
    snapshot.stale_reasons.clear();
    canonicalize_snapshot(snapshot)
}

#[cfg(test)]
pub(crate) fn mark_snapshot_staleness(
    mut snapshot: InventorySnapshot,
    workspace: &Workspace,
    registry: &EngineRegistry,
) -> InventorySnapshot {
    snapshot.stale_reasons = snapshot_staleness_reasons(&snapshot, workspace, registry);
    snapshot
}

pub(crate) fn snapshot_staleness_reasons_cached(
    snapshot: &InventorySnapshot,
    workspace: &Workspace,
    registry: &EngineRegistry,
) -> Vec<String> {
    let cache = FRESHNESS_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(cache) = cache.lock() {
        if let Some(entry) = cache.get(&workspace.id) {
            if entry.checked_at.elapsed() < FRESHNESS_CACHE_TTL
                && entry.snapshot_saved_at == snapshot.saved_at
                && entry.snapshot_schema_version == snapshot.schema_version
                && entry.snapshot_stale_reasons == snapshot.stale_reasons
                && entry.workspace == *workspace
                && entry.registry == *registry
            {
                return entry.reasons.clone();
            }
        }
    }

    let reasons = snapshot_staleness_reasons(snapshot, workspace, registry);
    if let Ok(mut cache) = cache.lock() {
        cache.insert(
            workspace.id.clone(),
            CachedFreshness {
                snapshot_saved_at: snapshot.saved_at.clone(),
                snapshot_schema_version: snapshot.schema_version,
                snapshot_stale_reasons: snapshot.stale_reasons.clone(),
                workspace: workspace.clone(),
                registry: registry.clone(),
                checked_at: Instant::now(),
                reasons: reasons.clone(),
            },
        );
    }
    reasons
}

pub(crate) fn invalidate_snapshot_freshness(workspace_id: &str) {
    if let Some(cache) = FRESHNESS_CACHE.get() {
        if let Ok(mut cache) = cache.lock() {
            cache.remove(workspace_id);
        }
    }
}

pub(crate) fn snapshot_staleness_reasons(
    snapshot: &InventorySnapshot,
    workspace: &Workspace,
    registry: &EngineRegistry,
) -> Vec<String> {
    let mut reasons = snapshot.stale_reasons.clone();
    let has_code =
        snapshot.metadata.code.is_some() || snapshot.items.iter().any(|entry| entry.is_code());
    let has_db =
        snapshot.metadata.db.is_some() || snapshot.items.iter().any(|entry| entry.source == "db");

    if has_code {
        match &snapshot.metadata.code {
            Some(code) => {
                let same_path = code.source_path.as_deref() == Some(workspace.repo_path.as_str());
                if !same_path {
                    push_unique(&mut reasons, "코드 프로젝트 경로가 바뀌었습니다");
                } else {
                    let current_revision = code_source_revision(workspace);
                    mark_source_revision_staleness(
                        code.source_revision.as_deref(),
                        current_revision.as_ref(),
                        Path::new(&workspace.repo_path).is_dir(),
                        "코드 소스 지문이 없어 다시 읽어야 합니다",
                        "코드 파일이 마지막 읽기 이후 바뀌었습니다",
                        "코드 변경 상태를 확인할 수 없습니다",
                        &mut reasons,
                    );
                }
                if code.adapter_version.as_deref() != Some(CODE_ADAPTER_VERSION) {
                    push_unique(&mut reasons, "코드 분석 규칙이 바뀌어 다시 읽어야 합니다");
                }
                mark_engine_staleness(code, registry, "codebase-memory", "코드", &mut reasons);
            }
            None => push_unique(&mut reasons, "읽은 코드 구조가 없습니다"),
        }
    }

    if has_db {
        match &snapshot.metadata.db {
            Some(db) => {
                let profile = db.profile_id.as_deref().and_then(|id| {
                    workspace
                        .db_profiles
                        .iter()
                        .find(|profile| profile.id == id)
                });
                match profile {
                    Some(profile) => {
                        if workspace.active_db_profile_id.as_deref() != Some(profile.id.as_str()) {
                            push_unique(&mut reasons, "활성 DB 연결이 바뀌었습니다");
                        }
                        if db.source_type != db_source_key(&profile.source) {
                            push_unique(&mut reasons, "DB 연결 유형이 바뀌었습니다");
                        }
                        if db.source_path.as_deref() != profile.path.as_deref() {
                            push_unique(&mut reasons, "DB 연결 경로가 바뀌었습니다");
                        } else if profile.path.is_some() {
                            let current_revision = db_source_revision(profile);
                            let source_exists = profile
                                .path
                                .as_deref()
                                .is_some_and(|path| Path::new(path).exists());
                            mark_source_revision_staleness(
                                db.source_revision.as_deref(),
                                current_revision.as_ref(),
                                source_exists,
                                "DB 소스 지문이 없어 다시 읽어야 합니다",
                                "DB 파일이 마지막 읽기 이후 바뀌었습니다",
                                "DB 파일 변경 상태를 확인할 수 없습니다",
                                &mut reasons,
                            );
                        }
                    }
                    None => push_unique(&mut reasons, "DB 연결을 찾을 수 없습니다"),
                }
                mark_engine_staleness(db, registry, "database-memory", "DB", &mut reasons);
            }
            None => push_unique(&mut reasons, "읽은 DB 구조가 없습니다"),
        }
    }

    if snapshot.metadata.migration.reindex_required {
        push_unique(&mut reasons, REINDEX_REASON);
    }
    reasons
}

pub(crate) fn save_inventory_snapshot(
    app_data_dir: impl AsRef<Path>,
    snapshot: &InventorySnapshot,
) -> Result<(), String> {
    validate_workspace_id(&snapshot.workspace_id)?;
    let app_data_dir = app_data_dir.as_ref();
    let path = snapshot_path(app_data_dir, &snapshot.workspace_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let snapshot = canonicalize_snapshot(snapshot.clone());
    let sequence = NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed);
    let temp = path.with_file_name(format!(
        "inventory-snapshot.{}.{}.{}.tmp.sqlite",
        std::process::id(),
        timestamp(),
        sequence
    ));
    if let Err(error) = sqlite_store::write_snapshot_database(&temp, &snapshot) {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    promote_snapshot_database(&path, &temp, &snapshot.workspace_id)?;
    migrate_legacy_snapshot_backup(app_data_dir, &snapshot.workspace_id, &path);
    invalidate_cached_snapshot(&path);
    invalidate_snapshot_freshness(&snapshot.workspace_id);
    super::linker::invalidate_candidate_links(&snapshot.workspace_id);
    Ok(())
}

pub(crate) fn replace_inventory_source(
    existing: Option<InventorySnapshot>,
    incoming: InventorySnapshot,
    source: &str,
) -> Result<InventorySnapshot, String> {
    if !matches!(source, "code" | "db") {
        return Err(format!("지원하지 않는 inventory 소스입니다: {source}"));
    }
    let Some(mut merged) = existing else {
        return Ok(canonicalize_snapshot(incoming));
    };
    if merged.workspace_id != incoming.workspace_id {
        return Err("합칠 inventory의 프로젝트 ID가 일치하지 않습니다".to_string());
    }

    let incoming_migration = incoming.metadata.migration.clone();
    let prefix = format!("{source}:");
    let removed_ids = merged
        .items
        .iter()
        .filter(|item| item.source == source)
        .map(|item| item.id.clone())
        .collect::<HashSet<_>>();
    let removed_link_ids = merged
        .links
        .iter()
        .filter(|link| removed_ids.contains(&link.from) || removed_ids.contains(&link.to))
        .map(|link| link.id.clone())
        .collect::<HashSet<_>>();
    merged.items.retain(|item| item.source != source);
    merged
        .links
        .retain(|link| !removed_ids.contains(&link.from) && !removed_ids.contains(&link.to));
    merged.metadata.gaps.retain(|gap| {
        !gap.id.starts_with(&format!("gap:{source}"))
            && !gap.kind.starts_with(source)
            && !gap.related_ids.iter().any(|id| {
                id.starts_with(&prefix) || removed_ids.contains(id) || removed_link_ids.contains(id)
            })
    });
    clear_resolved_migration(&mut merged, source);

    merged.items.extend(incoming.items);
    merged.links.extend(incoming.links);
    merged.metadata.gaps.extend(incoming.metadata.gaps);
    merge_migration(&mut merged.metadata.migration, incoming_migration);
    merged.saved_at = incoming.saved_at;
    merged.stale_reasons.clear();
    if source == "code" {
        merged.metadata.code = incoming.metadata.code;
        merged.metadata.architecture = incoming.metadata.architecture;
        merged.metadata.evidence = incoming.metadata.evidence;
    } else {
        merged.metadata.db = incoming.metadata.db;
    }
    Ok(canonicalize_snapshot(merged))
}

fn clear_resolved_migration(snapshot: &mut InventorySnapshot, source: &str) {
    if snapshot
        .metadata
        .migration
        .notes
        .iter()
        .any(|note| note == BACKUP_REINDEX_NOTE)
    {
        snapshot
            .metadata
            .migration
            .notes
            .retain(|note| note != BACKUP_REINDEX_NOTE);
        if snapshot.items.iter().any(|item| item.is_code()) {
            push_unique(
                &mut snapshot.metadata.migration.notes,
                BACKUP_CODE_REINDEX_NOTE,
            );
        }
        if snapshot.items.iter().any(|item| item.is_db()) {
            push_unique(
                &mut snapshot.metadata.migration.notes,
                BACKUP_DB_REINDEX_NOTE,
            );
        }
    }

    if snapshot.items.is_empty() {
        snapshot.metadata.migration = SnapshotMigration::default();
        return;
    }

    match source {
        "code" => snapshot.metadata.migration.notes.retain(|note| {
            note != V1_CODE_REINDEX_NOTE
                && note != UNSCORED_CODE_CALL_REINDEX_NOTE
                && note != BACKUP_CODE_REINDEX_NOTE
        }),
        "db" => snapshot
            .metadata
            .migration
            .notes
            .retain(|note| note != BACKUP_DB_REINDEX_NOTE),
        _ => {}
    }

    snapshot.metadata.migration.reindex_required = migration_has_blocker(snapshot);
}

fn migration_has_blocker(snapshot: &InventorySnapshot) -> bool {
    snapshot
        .metadata
        .migration
        .notes
        .iter()
        .any(|note| note != V1_MIGRATION_NOTE)
        || snapshot.metadata.gaps.iter().any(|gap| {
            matches!(
                gap.kind.as_str(),
                "node-conflict" | "relationship-conflict" | "unscored-code-call"
            )
        })
}

fn merge_migration(target: &mut SnapshotMigration, incoming: SnapshotMigration) {
    target.reindex_required |= incoming.reindex_required;
    target.source_schema_version = incoming
        .source_schema_version
        .or(target.source_schema_version);
    for note in incoming.notes {
        push_unique(&mut target.notes, &note);
    }
}

#[cfg(test)]
pub(crate) fn load_inventory_snapshot(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
) -> Result<InventorySnapshot, String> {
    Ok((*load_inventory_snapshot_cached(app_data_dir, workspace_id)?).clone())
}

pub(crate) fn load_inventory_snapshot_optional(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
) -> Result<Option<InventorySnapshot>, String> {
    Ok(
        load_inventory_snapshot_optional_cached(app_data_dir, workspace_id)?
            .map(|snapshot| (*snapshot).clone()),
    )
}

pub(crate) fn load_inventory_snapshot_optional_cached(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
) -> Result<Option<Arc<InventorySnapshot>>, String> {
    validate_workspace_id(workspace_id)?;
    let path = snapshot_path(app_data_dir.as_ref(), workspace_id);
    let archive = legacy_archive_snapshot_path(app_data_dir.as_ref(), workspace_id);
    let legacy = legacy_snapshot_path(app_data_dir.as_ref(), workspace_id);
    if !path.is_file()
        && !snapshot_backup_path(&path).is_file()
        && !archive.is_file()
        && !legacy_archive_snapshot_backup_path(&archive).is_file()
        && !legacy.is_file()
        && !legacy_snapshot_backup_path(&legacy).is_file()
    {
        return Ok(None);
    }
    load_inventory_snapshot_cached(app_data_dir, workspace_id).map(Some)
}

pub(crate) fn remove_db_inventory_snapshot(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
) -> Result<(), String> {
    let app_data_dir = app_data_dir.as_ref();
    let Some(mut snapshot) = load_inventory_snapshot_optional(app_data_dir, workspace_id)? else {
        return Ok(());
    };

    snapshot.items.retain(|item| item.source != "db");
    let retained_ids = snapshot
        .items
        .iter()
        .map(|item| item.id.as_str())
        .collect::<BTreeSet<_>>();
    snapshot.links.retain(|link| {
        retained_ids.contains(link.from.as_str()) && retained_ids.contains(link.to.as_str())
    });
    snapshot.metadata.db = None;
    snapshot.metadata.gaps.retain(|gap| {
        !gap.id.starts_with("gap:db")
            && !gap.kind.starts_with("db-")
            && gap
                .related_ids
                .iter()
                .all(|id| retained_ids.contains(id.as_str()))
    });
    // Removing the DB source must not hide an independent stale code source.
    // DB freshness reasons are recomputed after the source is removed, while
    // code and migration reasons still belong to the remaining snapshot.
    snapshot.stale_reasons.retain(|reason| {
        !reason.starts_with("DB ")
            && !reason.starts_with("활성 DB")
            && !reason.starts_with("읽은 DB")
    });

    let path = snapshot_path(app_data_dir, workspace_id);
    if snapshot.items.is_empty() {
        remove_inventory_snapshot(app_data_dir, workspace_id)?;
        return Ok(());
    }

    save_inventory_snapshot(app_data_dir, &snapshot)?;
    fs::copy(&path, snapshot_backup_path(&path))
        .map_err(|error| format!("DB 구조 백업을 정리하지 못했습니다: {error}"))?;
    Ok(())
}

pub(crate) fn remove_inventory_snapshot(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
) -> Result<(), String> {
    validate_workspace_id(workspace_id)?;
    let app_data_dir = app_data_dir.as_ref();
    let path = snapshot_path(app_data_dir, workspace_id);
    let archive = legacy_archive_snapshot_path(app_data_dir, workspace_id);
    let legacy = legacy_snapshot_path(app_data_dir, workspace_id);
    remove_file_if_exists(&path)?;
    remove_file_if_exists(&snapshot_backup_path(&path))?;
    remove_file_if_exists(&archive)?;
    remove_file_if_exists(&legacy_archive_snapshot_backup_path(&archive))?;
    remove_file_if_exists(&legacy)?;
    remove_file_if_exists(&legacy_snapshot_backup_path(&legacy))?;
    invalidate_cached_snapshot(&path);
    invalidate_snapshot_freshness(workspace_id);
    super::linker::invalidate_candidate_links(workspace_id);
    Ok(())
}

fn remove_file_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn load_inventory_snapshot_cached(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
) -> Result<Arc<InventorySnapshot>, String> {
    validate_workspace_id(workspace_id)?;
    let app_data_dir = app_data_dir.as_ref();
    let path = snapshot_path(app_data_dir, workspace_id);
    let primary = snapshot_file_state(&path);
    let backup_path = snapshot_backup_path(&path);
    let backup = snapshot_file_state(&backup_path);
    let legacy_archive_path = legacy_archive_snapshot_path(app_data_dir, workspace_id);
    let legacy_archive_primary = snapshot_file_state(&legacy_archive_path);
    let legacy_archive_backup =
        snapshot_file_state(&legacy_archive_snapshot_backup_path(&legacy_archive_path));
    let legacy_path = legacy_snapshot_path(app_data_dir, workspace_id);
    let legacy_primary = snapshot_file_state(&legacy_path);
    let legacy_backup = snapshot_file_state(&legacy_snapshot_backup_path(&legacy_path));
    let cache = SNAPSHOT_CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    if let Ok(cache) = cache.lock() {
        if let Some(entry) = cache.get(&path) {
            if entry.primary == primary
                && entry.backup == backup
                && entry.legacy_archive_primary == legacy_archive_primary
                && entry.legacy_archive_backup == legacy_archive_backup
                && entry.legacy_primary == legacy_primary
                && entry.legacy_backup == legacy_backup
            {
                return Ok(Arc::clone(&entry.snapshot));
            }
        }
    }

    let snapshot = Arc::new(load_inventory_snapshot_uncached(&path, workspace_id)?);
    if let Ok(mut cache) = cache.lock() {
        while cache.len() >= SNAPSHOT_CACHE_LIMIT && !cache.contains_key(&path) {
            let Some(evicted) = cache.iter().find_map(|(cached_path, entry)| {
                (Arc::strong_count(&entry.snapshot) == 1).then(|| cached_path.clone())
            }) else {
                break;
            };
            cache.remove(&evicted);
        }
        cache.insert(
            path,
            CachedSnapshot {
                primary,
                backup,
                legacy_archive_primary,
                legacy_archive_backup,
                legacy_primary,
                legacy_backup,
                snapshot: Arc::clone(&snapshot),
            },
        );
    }
    Ok(snapshot)
}

pub(crate) fn search_inventory_snapshot(
    app_data_dir: impl AsRef<Path>,
    workspace_id: &str,
    query: &str,
) -> Result<super::inventory_query::InventorySearchResult, String> {
    validate_workspace_id(workspace_id)?;
    let app_data_dir = app_data_dir.as_ref();
    let path = snapshot_path(app_data_dir, workspace_id);
    if sqlite_store::is_snapshot_database(&path) {
        if let Ok(result) = sqlite_store::search_snapshot_database(&path, workspace_id, query) {
            return Ok(result);
        }
    }
    let snapshot = load_inventory_snapshot_cached(app_data_dir, workspace_id)?;
    Ok(super::inventory_query::search_inventory(&snapshot, query))
}

fn load_inventory_snapshot_uncached(
    path: &Path,
    workspace_id: &str,
) -> Result<InventorySnapshot, String> {
    let archive = path.with_file_name("inventory-snapshot.json.zip");
    let legacy = path.with_file_name("inventory-snapshot.json");
    if path.is_file() {
        match load_snapshot_file(path, workspace_id) {
            Ok(snapshot) => return Ok(snapshot),
            Err(primary_error) => {
                for backup in [
                    snapshot_backup_path(path),
                    archive.clone(),
                    legacy_archive_snapshot_backup_path(&archive),
                    legacy.clone(),
                    legacy_snapshot_backup_path(&legacy),
                ] {
                    if let Ok(mut snapshot) = load_snapshot_file(&backup, workspace_id) {
                        mark_reindex_required(&mut snapshot, BACKUP_REINDEX_NOTE);
                        return Ok(snapshot);
                    }
                }
                return Err(format!(
                    "스냅샷을 열 수 없습니다: {primary_error}; 읽을 수 있는 백업이 없습니다"
                ));
            }
        }
    }
    if archive.is_file() {
        return load_snapshot_file(&archive, workspace_id);
    }
    if legacy.is_file() {
        return load_snapshot_file(&legacy, workspace_id);
    }
    for backup in [
        snapshot_backup_path(path),
        legacy_archive_snapshot_backup_path(&archive),
        legacy_snapshot_backup_path(&legacy),
    ] {
        if let Ok(mut snapshot) = load_snapshot_file(&backup, workspace_id) {
            mark_reindex_required(&mut snapshot, BACKUP_REINDEX_NOTE);
            return Ok(snapshot);
        }
    }
    Err("스냅샷과 읽을 수 있는 백업이 없습니다".to_string())
}

fn snapshot_file_state(path: &Path) -> SnapshotFileState {
    match fs::metadata(path) {
        Ok(metadata) => SnapshotFileState {
            modified: metadata.modified().ok(),
            length: Some(metadata.len()),
            digest: (!sqlite_store::is_snapshot_database(path))
                .then(|| fs::read(path).ok().map(|bytes| Sha256::digest(bytes).into()))
                .flatten(),
        },
        Err(_) => SnapshotFileState {
            modified: None,
            length: None,
            digest: None,
        },
    }
}

fn invalidate_cached_snapshot(path: &Path) {
    if let Some(cache) = SNAPSHOT_CACHE.get() {
        if let Ok(mut cache) = cache.lock() {
            cache.remove(path);
        }
    }
}

pub(crate) fn load_snapshot_file(
    path: &Path,
    workspace_id: &str,
) -> Result<InventorySnapshot, String> {
    if sqlite_store::is_snapshot_database(path) {
        return sqlite_store::read_snapshot_database(path, workspace_id)
            .map(canonicalize_snapshot);
    }
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let json = decode_snapshot_payload(&bytes)?;
    let value: Value = serde_json::from_slice(&json).map_err(|error| error.to_string())?;
    let version = value.get("schemaVersion");
    let mut snapshot = match version {
        None => {
            let mut snapshot: InventorySnapshot =
                serde_json::from_value(value).map_err(|error| error.to_string())?;
            snapshot.schema_version = 1;
            snapshot
        }
        Some(version) if version.as_u64() == Some(1) || version.as_u64() == Some(2) => {
            serde_json::from_value(value).map_err(|error| error.to_string())?
        }
        Some(version) => incompatible_snapshot(&value, version.as_u64()),
    };

    if snapshot.workspace_id != workspace_id {
        return Err("스냅샷 프로젝트 ID가 경로와 일치하지 않습니다".to_string());
    }
    snapshot = canonicalize_snapshot(snapshot);
    Ok(snapshot)
}

const SNAPSHOT_ARCHIVE_ENTRY: &str = "inventory-snapshot.json";
const MAX_SNAPSHOT_JSON_BYTES: u64 = 1024 * 1024 * 1024;

pub(crate) fn decode_snapshot_payload(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|byte| matches!(byte, b'{' | b'['))
    {
        return Ok(bytes.to_vec());
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|error| error.to_string())?;
    let mut entry = archive
        .by_name(SNAPSHOT_ARCHIVE_ENTRY)
        .map_err(|error| error.to_string())?;
    if entry.size() > MAX_SNAPSHOT_JSON_BYTES {
        return Err("압축 스냅샷이 안전한 해제 한도를 초과했습니다".to_string());
    }
    let mut json = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
    entry.read_to_end(&mut json).map_err(|error| error.to_string())?;
    Ok(json)
}

fn migrate_legacy_snapshot_backup(app_data_dir: &Path, workspace_id: &str, primary: &Path) {
    let archive = legacy_archive_snapshot_path(app_data_dir, workspace_id);
    let archive_backup = legacy_archive_snapshot_backup_path(&archive);
    let legacy = legacy_snapshot_path(app_data_dir, workspace_id);
    let legacy_backup = legacy_snapshot_backup_path(&legacy);
    let candidates = [&archive, &archive_backup, &legacy, &legacy_backup];
    if !candidates.iter().any(|path| path.is_file()) {
        return;
    }
    let backup = snapshot_backup_path(primary);
    if !backup.is_file() {
        if let Some(snapshot) = candidates
            .iter()
            .find_map(|path| load_snapshot_file(path, workspace_id).ok())
        {
            let sequence = NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed);
            let temp = backup.with_file_name(format!(
                "inventory-snapshot.legacy.{}.{}.tmp.sqlite",
                std::process::id(),
                sequence
            ));
            if sqlite_store::write_snapshot_database(&temp, &snapshot).is_ok() {
                let _ = fs::rename(&temp, &backup);
            }
            let _ = fs::remove_file(temp);
        }
    }
    if backup.is_file() && load_snapshot_file(&backup, workspace_id).is_ok() {
        for path in candidates {
            let _ = remove_file_if_exists(path);
        }
    }
}

fn incompatible_snapshot(value: &Value, version: Option<u64>) -> InventorySnapshot {
    let workspace_id = value
        .get("workspaceId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let saved_at = value
        .get("savedAt")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mut snapshot = InventorySnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        workspace_id,
        saved_at,
        metadata: SnapshotMetadata::default(),
        stale_reasons: Vec::new(),
        links: Vec::new(),
        items: Vec::new(),
    };
    snapshot.metadata.migration.source_schema_version =
        version.and_then(|value| u32::try_from(value).ok());
    mark_reindex_required(
        &mut snapshot,
        "지원하지 않는 스냅샷 버전은 최신 형식으로 해석하지 않았습니다.",
    );
    snapshot
}

fn promote_snapshot_database(path: &Path, temp: &Path, workspace_id: &str) -> Result<(), String> {
    let backup = snapshot_backup_path(path);
    let had_current = path.is_file();
    let rotate_current = had_current && load_snapshot_file(path, workspace_id).is_ok();
    if rotate_current {
        if backup.exists() {
            fs::remove_file(&backup).map_err(|error| {
                let _ = fs::remove_file(temp);
                error.to_string()
            })?;
        }
        fs::rename(path, &backup).map_err(|error| {
            let _ = fs::remove_file(temp);
            error.to_string()
        })?;
    } else if had_current {
        fs::remove_file(path).map_err(|error| {
            let _ = fs::remove_file(temp);
            error.to_string()
        })?;
    }

    if let Err(error) = fs::rename(temp, path) {
        if rotate_current {
            let _ = fs::rename(&backup, path);
        }
        let _ = fs::remove_file(temp);
        return Err(error.to_string());
    }
    Ok(())
}
