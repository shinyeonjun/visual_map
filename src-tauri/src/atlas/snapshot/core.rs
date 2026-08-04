const REINDEX_REASON: &str = "스냅샷 형식이 호환되지 않아 다시 읽어야 합니다";
const V1_MIGRATION_NOTE: &str = "Snapshot V1의 안전한 필드를 V2로 이전했습니다.";
const V1_CODE_REINDEX_NOTE: &str =
    "Snapshot V1 코드 항목은 이전 BM25 bucket 분류를 신뢰할 수 없어 다시 읽어야 합니다.";
const UNSCORED_CODE_CALL_REINDEX_NOTE: &str =
    "기존 CALLS에 엔진 신뢰도 정보가 없어 코드를 다시 읽어야 합니다.";
const BACKUP_REINDEX_NOTE: &str =
    "주 스냅샷 대신 이전 백업을 복구했습니다. 다시 읽어 최신 상태를 확인하세요.";
const BACKUP_CODE_REINDEX_NOTE: &str = "백업에서 복구한 코드 목록은 다시 읽어야 합니다.";
const BACKUP_DB_REINDEX_NOTE: &str = "백업에서 복구한 DB 구조는 다시 읽어야 합니다.";
const CODE_ADAPTER_VERSION: &str = "7";
const CONFIRMED_CODE_CALL_CONFIDENCE: u8 = 85;
const CANDIDATE_CODE_CALL_CONFIDENCE: u8 = 70;
static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(0);
static SNAPSHOT_CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedSnapshot>>> = OnceLock::new();
static FRESHNESS_CACHE: OnceLock<Mutex<HashMap<String, CachedFreshness>>> = OnceLock::new();
// Keep at most two idle inventories. Entries held by active commands may temporarily exceed
// the limit so concurrent workspace reads do not evict data still in use.
const SNAPSHOT_CACHE_LIMIT: usize = 2;
const FRESHNESS_CACHE_TTL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
struct SnapshotFileState {
    modified: Option<SystemTime>,
    length: Option<u64>,
    digest: Option<[u8; 32]>,
}

#[derive(Debug, Clone)]
struct CachedSnapshot {
    primary: SnapshotFileState,
    backup: SnapshotFileState,
    snapshot: Arc<InventorySnapshot>,
}

#[derive(Debug, Clone)]
struct CachedFreshness {
    snapshot_saved_at: String,
    snapshot_schema_version: u32,
    snapshot_stale_reasons: Vec<String>,
    workspace: Workspace,
    registry: EngineRegistry,
    checked_at: Instant,
    reasons: Vec<String>,
}

pub(crate) fn build_inventory_snapshot(
    workspace_id: String,
    code: Option<&CodeInventory>,
    db: Option<&DbInventory>,
) -> InventorySnapshot {
    let mut snapshot = InventorySnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        workspace_id,
        saved_at: timestamp(),
        metadata: SnapshotMetadata::default(),
        stale_reasons: Vec::new(),
        links: Vec::new(),
        items: Vec::new(),
    };

    if let Some(code) = code {
        snapshot.metadata.code = Some(SnapshotSourceMetadata {
            saved_at: snapshot.saved_at.clone(),
            engine_id: Some("codebase-memory".to_string()),
            engine_version: None,
            engine_checksum: None,
            adapter_version: Some(CODE_ADAPTER_VERSION.to_string()),
            contract_version: None,
            snapshot_key: None,
            limit_requested: None,
            limit_applied: None,
            limit_clamped: None,
            result_count: None,
            total_tables: None,
            truncated: None,
            source_revision: None,
            source_revision_label: None,
            source_path: None,
            source_type: "unknown".to_string(),
            profile_id: None,
        });
        snapshot.metadata.architecture = code.architecture.clone();
        snapshot.metadata.evidence = code.evidence.clone();
        snapshot.items.extend(code.routes.iter().map(|entry| {
            if is_ui_route(entry) {
                code_item(entry, "ui-route", "ui", &code.project)
            } else {
                code_item(entry, "api", "api", &code.project)
            }
        }));
        snapshot.items.extend(
            code.services
                .iter()
                .map(|entry| code_item(entry, "service", "code", &code.project)),
        );
        snapshot.items.extend(
            code.handlers
                .iter()
                .map(|entry| code_item(entry, "handler", "code", &code.project)),
        );
        snapshot.items.extend(
            code.repositories
                .iter()
                .map(|entry| code_item(entry, "repository", "code", &code.project)),
        );
        snapshot.items.extend(code.functions.iter().map(|entry| {
            let kind = match entry.engine_label.as_str() {
                "Method" => "method",
                _ => "function",
            };
            code_item(entry, kind, "code", &code.project)
        }));
        snapshot.items.extend(
            code.classes
                .iter()
                .map(|entry| code_item(entry, "class", "code", &code.project)),
        );
        snapshot.items.extend(
            code.modules
                .iter()
                .map(|entry| code_item(entry, "module", "code", &code.project)),
        );
        snapshot.items.extend(code.unknown.iter().map(|entry| {
            let engine_kind = entry.engine_label.to_ascii_lowercase();
            let kind = if entry.kind.eq_ignore_ascii_case("unknown") || engine_kind.is_empty() {
                "code"
            } else {
                &engine_kind
            };
            code_item(entry, kind, "code", &code.project)
        }));
        snapshot.items.extend(
            code.files
                .iter()
                .map(|entry| code_item(entry, "file", "code", &code.project)),
        );
        if let Some(architecture) = code.architecture.as_ref() {
            snapshot.links.extend(code_architecture_links(architecture));
        }
        snapshot.links.extend(code.calls.iter().map(code_call_link));
        snapshot
            .links
            .extend(client_request_links::build_client_request_links(code));
        let routes = code
            .routes
            .iter()
            .map(|route| (route.id.as_str(), route))
            .collect::<HashMap<_, _>>();
        snapshot.links.extend(code.handles.iter().map(|handle| {
            let mut link = confirmed_link(
                format!("code-handle:{}->{}", handle.route, handle.handler),
                format!("code:{}", handle.route),
                format!("code:{}", handle.handler),
                "code_handle",
                "HANDLES",
                "codebase-memory HANDLES: upstream handler→route was normalized to product route→handler",
            );
            link.direction = "outbound".to_string();
            if let Some(route) = routes.get(handle.route.as_str()).filter(|route| {
                detail_string(&route.detail, &["routePathSource"]).as_deref()
                    == Some("fastapi-static-mount")
            }) {
                let local = route
                    .detail
                    .get("localRoutePath")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| route.name.clone());
                let mounted = detail_string(&route.detail, &["mountedRoutePath"])
                    .unwrap_or_else(|| route.name.clone());
                let local_display = if local.is_empty() { "\"\"" } else { &local };
                link.evidence.push(Evidence {
                    kind: "route-mount".to_string(),
                    text: format!(
                        "FastAPI 정적 마운트: decorator 경로 `{local_display}`와 APIRouter/include_router prefix를 합성해 `{mounted}`를 확인했습니다."
                    ),
                });
            }
            link
        }));
        let code_item_ids = snapshot
            .items
            .iter()
            .filter(|item| item.is_code())
            .map(|item| item.id.as_str())
            .collect::<HashSet<_>>();
        snapshot
            .metadata
            .gaps
            .extend(code.relation_gaps.iter().map(|relation| {
                let related_ids = [relation.from.as_str(), relation.to.as_str()]
                    .into_iter()
                    .map(|endpoint| format!("code:{endpoint}"))
                    .filter(|id| code_item_ids.contains(id.as_str()))
                    .collect();
                gap(
                    relation.stable_id(),
                    &relation.kind,
                    &relation.message,
                    related_ids,
                )
            }));
    }

    if let Some(db) = db {
        snapshot.metadata.db = Some(SnapshotSourceMetadata {
            saved_at: snapshot.saved_at.clone(),
            engine_id: Some("database-memory".to_string()),
            engine_version: None,
            engine_checksum: None,
            adapter_version: None,
            contract_version: db.contract_version.clone(),
            snapshot_key: db.snapshot_key.clone(),
            limit_requested: db.limit_requested,
            limit_applied: db.limit_applied,
            limit_clamped: db.limit_clamped,
            result_count: db.result_count,
            total_tables: db.total_tables,
            truncated: db.truncated,
            source_revision: None,
            source_revision_label: None,
            source_path: None,
            source_type: "unknown".to_string(),
            profile_id: Some(db.profile_id.clone()),
        });
        for (index, warning) in db.capability_warnings.iter().enumerate() {
            let message = localized_db_capability_warning(warning);
            snapshot.metadata.gaps.push(gap(
                format!("gap:db-capability:{index}"),
                "db-capability",
                &message,
                Vec::new(),
            ));
        }
        snapshot.metadata.gaps.extend(db.gaps.iter().map(|entry| {
            gap(
                format!("gap:{}", entry.id),
                &entry.kind,
                &entry.message,
                entry
                    .table_key
                    .as_deref()
                    .map(|table_key| vec![format!("db:table:{table_key}")])
                    .unwrap_or_default(),
            )
        }));
        let table_keys = db
            .tables
            .iter()
            .map(|table| db_table_key(table.schema.as_deref(), &table.name))
            .collect::<BTreeSet<_>>();
        let stable_table_keys = db
            .tables
            .iter()
            .filter_map(|table| {
                table.key.as_ref().map(|key| {
                    (
                        key.clone(),
                        db_table_key(table.schema.as_deref(), &table.name),
                    )
                })
            })
            .collect::<BTreeMap<_, _>>();
        let mut named_table_keys = BTreeMap::<String, Vec<String>>::new();
        for table in &db.tables {
            named_table_keys
                .entry(table.name.clone())
                .or_default()
                .push(db_table_key(table.schema.as_deref(), &table.name));
        }
        let column_ids = db
            .tables
            .iter()
            .flat_map(|table| {
                let table_key = db_table_key(table.schema.as_deref(), &table.name);
                table
                    .columns
                    .iter()
                    .map(move |column| db_column_id(&table_key, &column.name))
            })
            .collect::<BTreeSet<_>>();
        let stable_column_ids = db
            .tables
            .iter()
            .flat_map(|table| {
                let table_key = db_table_key(table.schema.as_deref(), &table.name);
                table.columns.iter().filter_map(move |column| {
                    column
                        .key
                        .as_ref()
                        .map(|key| (key.clone(), db_column_id(&table_key, &column.name)))
                })
            })
            .collect::<BTreeMap<_, _>>();

        for table in &db.tables {
            let table_key = db_table_key(table.schema.as_deref(), &table.name);
            let table_id = format!("db:table:{table_key}");
            let mut table_item = item(
                &table_id,
                "table",
                &table.name,
                "data",
                "db",
                None,
                table.schema.as_deref(),
            );
            let display_table_name = db_qualified_table_name(table.schema.as_deref(), &table.name);
            table_item.qualified_name = table
                .key
                .clone()
                .or_else(|| Some(display_table_name.clone()));
            table_item.engine_label = Some("Table".to_string());
            table_item.project_id = Some(db.profile_id.clone());
            table_item.group_id = table.schema.clone();
            snapshot.items.push(table_item);

            snapshot.items.extend(table.columns.iter().map(|column| {
                let mut column_item = InventoryItem {
                    id: db_column_id(&table_key, &column.name),
                    kind: "column".to_string(),
                    name: column.name.clone(),
                    layer: "data".to_string(),
                    source: "db".to_string(),
                    parent_id: Some(table_id.clone()),
                    path: column.data_type.clone(),
                    qualified_name: column
                        .key
                        .clone()
                        .or_else(|| Some(format!("{display_table_name}.{}", column.name))),
                    engine_label: Some("Column".to_string()),
                    language: None,
                    role_basis: None,
                    project_id: Some(db.profile_id.clone()),
                    group_id: table.schema.clone(),
                    location: None,
                    is_primary_key: column.is_primary_key,
                    is_foreign_key: column.is_foreign_key,
                    nullable: column.nullable,
                };
                if column_item.qualified_name.as_deref() == Some("") {
                    column_item.qualified_name = None;
                }
                column_item
            }));

            for constraint in &table.constraints {
                append_db_constraint(
                    &mut snapshot,
                    &table_key,
                    &db.profile_id,
                    constraint,
                    &column_ids,
                    &stable_column_ids,
                );
            }
            for index in &table.indexes {
                append_db_index(
                    &mut snapshot,
                    &table_key,
                    &db.profile_id,
                    index,
                    &column_ids,
                    &stable_column_ids,
                );
            }
            for dependent in &table.dependents {
                append_db_dependent(
                    &mut snapshot,
                    &table_key,
                    table.schema.as_deref(),
                    &db.profile_id,
                    dependent,
                    &stable_column_ids,
                );
            }
        }

        let mut foreign_key_observations = db
            .tables
            .iter()
            .flat_map(|table| {
                table
                    .foreign_keys
                    .iter()
                    .map(move |foreign_key| (table, foreign_key, "outbound"))
                    .chain(
                        table
                            .inbound_foreign_keys
                            .iter()
                            .map(move |foreign_key| (table, foreign_key, "inbound")),
                    )
            })
            .collect::<Vec<_>>();
        foreign_key_observations.sort_by(|left, right| {
            (left.2 != "outbound")
                .cmp(&(right.2 != "outbound"))
                .then_with(|| left.1.key.cmp(&right.1.key))
                .then_with(|| left.1.name.cmp(&right.1.name))
        });
        for (table, foreign_key, direction) in foreign_key_observations {
            let current_table_key = db_table_key(table.schema.as_deref(), &table.name);
            let source_key = resolve_db_table_key(
                foreign_key.table_key.as_deref(),
                foreign_key.table_schema.as_deref(),
                foreign_key.table.as_deref(),
                &table_keys,
                &stable_table_keys,
                &named_table_keys,
            )
            .or_else(|| (direction == "outbound").then(|| current_table_key.clone()));
            let referenced_key = resolve_db_table_key(
                foreign_key.referenced_table_key.as_deref(),
                foreign_key.referenced_schema.as_deref(),
                Some(&foreign_key.referenced_table),
                &table_keys,
                &stable_table_keys,
                &named_table_keys,
            );
            let (Some(source_key), Some(referenced_key)) = (source_key, referenced_key) else {
                snapshot.metadata.gaps.push(gap(
                        format!(
                            "gap:db-fk-table:{}:{}",
                            current_table_key,
                            foreign_key.name.as_deref().unwrap_or("unnamed")
                        ),
                        "db-fk-unresolved-table",
                        "FK의 source 또는 target table을 유일하게 확인할 수 없어 관계를 만들지 않았습니다.",
                        vec![format!("db:table:{current_table_key}")],
                    ));
                continue;
            };

            let constraint = constraint_from_foreign_key(foreign_key, direction);
            append_db_constraint(
                &mut snapshot,
                &source_key,
                &db.profile_id,
                &constraint,
                &column_ids,
                &stable_column_ids,
            );
            let source_column_count = foreign_key.columns.len().max(foreign_key.column_keys.len());
            let referenced_column_count = foreign_key
                .referenced_columns
                .len()
                .max(foreign_key.referenced_column_keys.len());
            if source_column_count != referenced_column_count {
                snapshot.metadata.gaps.push(gap(
                    format!(
                        "gap:db-fk-columns:{}:{}",
                        source_key,
                        foreign_key.name.as_deref().unwrap_or("unnamed")
                    ),
                    "db-fk-column-mismatch",
                    "FK source/target column 수가 달라 확인 가능한 열 관계만 보존했습니다.",
                    vec![
                        format!("db:table:{source_key}"),
                        format!("db:table:{referenced_key}"),
                    ],
                ));
            }
            for ordinal in 0..source_column_count.min(referenced_column_count) {
                let source_column = foreign_key.columns.get(ordinal).map(String::as_str);
                let source_stable_key = foreign_key.column_keys.get(ordinal).map(String::as_str);
                let referenced_column = foreign_key
                    .referenced_columns
                    .get(ordinal)
                    .map(String::as_str);
                let referenced_stable_key = foreign_key
                    .referenced_column_keys
                    .get(ordinal)
                    .map(String::as_str);
                let from = resolve_db_column_id(
                    &source_key,
                    source_column,
                    source_stable_key,
                    &column_ids,
                    &stable_column_ids,
                );
                let to = resolve_db_column_id(
                    &referenced_key,
                    referenced_column,
                    referenced_stable_key,
                    &column_ids,
                    &stable_column_ids,
                );
                let (Some(from), Some(to)) = (from, to) else {
                    snapshot.metadata.gaps.push(gap(
                        format!(
                            "gap:db-fk-endpoint:{}:{}:{ordinal}",
                            source_key,
                            foreign_key.name.as_deref().unwrap_or("unnamed")
                        ),
                        "db-fk-missing-column",
                        "FK column endpoint가 inventory에 없어 관계를 만들지 않았습니다.",
                        vec![
                            format!("db:table:{source_key}"),
                            format!("db:table:{referenced_key}"),
                        ],
                    ));
                    continue;
                };
                let mut link = confirmed_link(
                    format!("db-fk:{from}->{to}"),
                    from,
                    to,
                    "db_fk",
                    "FOREIGN_KEY",
                    "database-memory foreign key metadata",
                );
                link.label = foreign_key.name.clone();
                if let Some(key) = foreign_key.key.as_deref() {
                    link.evidence.push(Evidence {
                        kind: "db-object-key".to_string(),
                        text: key.to_string(),
                    });
                }
                push_evidence(&mut link.evidence, "db-column-key", source_stable_key);
                push_evidence(
                    &mut link.evidence,
                    "db-referenced-column-key",
                    referenced_stable_key,
                );
                link.evidence.push(Evidence {
                    kind: "db-fk-direction".to_string(),
                    text: direction.to_string(),
                });
                snapshot.links.push(link);
            }
        }
    }

    canonicalize_snapshot(snapshot)
}
