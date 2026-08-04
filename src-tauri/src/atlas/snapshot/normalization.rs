use super::*;

pub(super) fn canonicalize_snapshot(mut snapshot: InventorySnapshot) -> InventorySnapshot {
    let source_version = snapshot.schema_version;
    snapshot.schema_version = SNAPSHOT_SCHEMA_VERSION;
    if source_version == 1 {
        snapshot.metadata.migration.source_schema_version = Some(1);
        push_unique(&mut snapshot.metadata.migration.notes, V1_MIGRATION_NOTE);
        if snapshot.items.iter().any(|entry| entry.is_code()) {
            mark_reindex_required(&mut snapshot, V1_CODE_REINDEX_NOTE);
        }
    } else if source_version != SNAPSHOT_SCHEMA_VERSION {
        snapshot.metadata.migration.source_schema_version = Some(source_version);
        mark_reindex_required(
            &mut snapshot,
            "지원하지 않는 스냅샷 버전은 다시 읽어야 합니다.",
        );
        snapshot.items.clear();
        snapshot.links.clear();
        return snapshot;
    }

    normalize_snapshot_route_bindings(&mut snapshot);
    snapshot.items.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.source.cmp(&right.source))
            .then_with(|| left.name.cmp(&right.name))
    });
    let mut items = BTreeMap::<String, InventoryItem>::new();
    let mut gaps = Vec::new();
    for mut entry in std::mem::take(&mut snapshot.items) {
        normalize_item(&mut entry);
        if entry.id.is_empty() {
            gaps.push(gap(
                "gap:node:empty-id".to_string(),
                "invalid-node",
                "ID가 없는 노드를 제외했습니다.",
                Vec::new(),
            ));
            continue;
        }
        match items.entry(entry.id.clone()) {
            Entry::Vacant(slot) => {
                slot.insert(entry);
            }
            Entry::Occupied(mut slot) if compatible_items(slot.get(), &entry) => {
                merge_item(slot.get_mut(), entry);
            }
            Entry::Occupied(slot) => {
                let id = slot.key().clone();
                gaps.push(gap(
                    format!("gap:node-conflict:{id}"),
                    "node-conflict",
                    "같은 ID가 서로 다른 노드를 가리켜 다시 읽기가 필요합니다.",
                    vec![id],
                ));
                snapshot.metadata.migration.reindex_required = true;
            }
        }
    }

    let node_ids = items.keys().cloned().collect::<BTreeSet<_>>();
    for entry in items.values_mut() {
        if entry
            .parent_id
            .as_ref()
            .is_some_and(|parent| !node_ids.contains(parent))
        {
            let parent = entry.parent_id.take().unwrap_or_default();
            gaps.push(gap(
                format!("gap:parent:{}", entry.id),
                "dangling-parent",
                "존재하지 않는 상위 노드 참조를 제거했습니다.",
                vec![entry.id.clone(), parent],
            ));
        }
    }

    snapshot.links.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.from.cmp(&right.from))
            .then_with(|| left.to.cmp(&right.to))
    });
    let mut links = BTreeMap::<String, SnapshotLink>::new();
    let mut relationships = BTreeSet::new();
    let mut unscored_code_calls = 0usize;
    for mut link in std::mem::take(&mut snapshot.links) {
        if link.kind == "code_call"
            && !link
                .evidence
                .iter()
                .any(|evidence| evidence.kind == "engine-confidence")
        {
            link.truth_class = "unknown".to_string();
            link.evidence.push(Evidence {
                kind: "engine-confidence".to_string(),
                text: "unknown".to_string(),
            });
            link.evidence.push(Evidence {
                kind: "engine-confidence-score".to_string(),
                text: "점수 없음".to_string(),
            });
            unscored_code_calls += 1;
        }
        normalize_link(&mut link);
        if !node_ids.contains(&link.from) || !node_ids.contains(&link.to) {
            gaps.push(gap(
                format!("gap:link:{}", link.id),
                "dangling-relationship",
                "끝점이 없는 관계를 제외했습니다.",
                vec![link.from, link.to],
            ));
            continue;
        }
        let relationship = format!(
            "{}\0{}\0{}\0{}\0{}",
            link.kind,
            link.from,
            link.to,
            link.label.as_deref().unwrap_or_default(),
            link.engine_edge_type.as_deref().unwrap_or_default()
        );
        if !relationships.insert(relationship) {
            continue;
        }
        match links.entry(link.id.clone()) {
            Entry::Vacant(slot) => {
                slot.insert(link);
            }
            Entry::Occupied(slot) if slot.get() == &link => {}
            Entry::Occupied(slot) => {
                let id = slot.key().clone();
                gaps.push(gap(
                    format!("gap:link-conflict:{id}"),
                    "relationship-conflict",
                    "같은 관계 ID에 서로 다른 근거가 있어 다시 읽기가 필요합니다.",
                    vec![id],
                ));
                snapshot.metadata.migration.reindex_required = true;
            }
        }
    }

    if unscored_code_calls > 0 {
        gaps.push(gap(
            "gap:code-call-confidence".to_string(),
            "unscored-code-call",
            &format!(
                "엔진 신뢰도 정보가 없는 CALLS {unscored_code_calls}개를 확정 관계에서 제외했습니다."
            ),
            Vec::new(),
        ));
        mark_reindex_required(&mut snapshot, UNSCORED_CODE_CALL_REINDEX_NOTE);
    }

    snapshot.items = items.into_values().collect();
    snapshot.links = links.into_values().collect();
    snapshot.metadata.gaps.extend(gaps);
    snapshot
        .metadata
        .gaps
        .sort_by(|left, right| left.id.cmp(&right.id));
    snapshot
        .metadata
        .gaps
        .dedup_by(|left, right| left.id == right.id);
    if snapshot.metadata.migration.reindex_required {
        push_unique(&mut snapshot.stale_reasons, REINDEX_REASON);
    }
    snapshot
}

pub(super) fn normalize_snapshot_route_bindings(snapshot: &mut InventorySnapshot) {
    let handler_by_id = snapshot
        .items
        .iter()
        .filter(|item| item.is_code() && item.layer == "code")
        .map(|item| (item.id.clone(), item.clone()))
        .collect::<HashMap<_, _>>();
    let mut handlers_by_route = HashMap::<String, Vec<String>>::new();
    for link in snapshot
        .links
        .iter()
        .filter(|link| link.kind == "code_handle")
    {
        handlers_by_route
            .entry(link.from.clone())
            .or_default()
            .push(link.to.clone());
    }
    for handlers in handlers_by_route.values_mut() {
        handlers.sort();
        handlers.dedup();
    }

    let mut items = Vec::with_capacity(snapshot.items.len().max(snapshot.links.len()));
    for mut item in std::mem::take(&mut snapshot.items) {
        let Some(handler_ids) = (item.is_code() && item.layer == "api")
            .then(|| handlers_by_route.get(&item.id))
            .flatten()
        else {
            items.push(item);
            continue;
        };

        if handler_ids.len() == 1 {
            hydrate_snapshot_route(&mut item, handler_by_id.get(&handler_ids[0]), false);
            items.push(item);
            continue;
        }

        for handler_id in handler_ids {
            let mut binding = item.clone();
            let raw_route_id = item.id.strip_prefix("code:").unwrap_or(&item.id);
            let raw_handler_id = handler_id.strip_prefix("code:").unwrap_or(handler_id);
            let raw_binding_id = route_binding_id(raw_route_id, raw_handler_id);
            binding.id = format!("code:{raw_binding_id}");
            binding.qualified_name = Some(raw_binding_id);
            hydrate_snapshot_route(&mut binding, handler_by_id.get(handler_id), true);
            items.push(binding);
        }
    }
    snapshot.items = items;

    for link in snapshot
        .links
        .iter_mut()
        .filter(|link| link.kind == "code_handle")
    {
        if handlers_by_route
            .get(&link.from)
            .is_some_and(|handlers| handlers.len() > 1)
        {
            let raw_route_id = link.from.strip_prefix("code:").unwrap_or(&link.from);
            let raw_handler_id = link.to.strip_prefix("code:").unwrap_or(&link.to);
            let raw_binding_id = route_binding_id(raw_route_id, raw_handler_id);
            link.from = format!("code:{raw_binding_id}");
            link.id = format!("code-handle:{raw_binding_id}->{raw_handler_id}");
        }
    }
}

pub(super) fn hydrate_snapshot_route(
    route: &mut InventoryItem,
    handler: Option<&InventoryItem>,
    prefer_handler_location: bool,
) {
    let Some(handler) = handler else {
        return;
    };
    if prefer_handler_location || route.path.is_none() {
        route.path = handler.path.clone();
        route.location = handler.location.clone();
    }
}

pub(super) fn code_item(
    entry: &CodeInventoryItem,
    kind: &str,
    layer: &str,
    fallback_project: &str,
) -> InventoryItem {
    let path = entry
        .file_path
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let project = if entry.project.is_empty() {
        fallback_project
    } else {
        &entry.project
    };
    let qualified_name = if entry.qualified_name.is_empty() {
        &entry.id
    } else {
        &entry.qualified_name
    };
    let engine_label = if entry.engine_label.is_empty() {
        &entry.kind
    } else {
        &entry.engine_label
    };
    let inferred_kind = detail_string(&entry.detail, &["role"]);
    let language = detail_string(&entry.detail, &["language"])
        .or_else(|| path.as_deref().and_then(language_for_path));
    let location = path.clone().map(|path| SourceLocation {
        path,
        line: entry.line,
        column: entry
            .column
            .or_else(|| detail_u64(&entry.detail, &["startColumn", "start_column", "column"])),
        end_line: entry.end_line,
        end_column: entry
            .end_column
            .or_else(|| detail_u64(&entry.detail, &["endColumn", "end_column"])),
    });

    InventoryItem {
        id: format!("code:{}", entry.id),
        kind: inferred_kind.unwrap_or_else(|| kind.to_string()),
        name: entry.name.clone(),
        layer: layer.to_string(),
        source: "code".to_string(),
        parent_id: None,
        path,
        qualified_name: non_empty(qualified_name),
        engine_label: non_empty(engine_label),
        language,
        role_basis: detail_string(&entry.detail, &["roleBasis", "role_basis"]),
        project_id: non_empty(project),
        group_id: detail_string(
            &entry.detail,
            &[
                "groupId",
                "group_id",
                "parentQualifiedName",
                "parent_qualified_name",
                "module",
                "namespace",
                "package",
            ],
        ),
        location,
        is_primary_key: false,
        is_foreign_key: false,
        nullable: None,
    }
}

fn language_for_path(path: &str) -> Option<String> {
    let extension = path.rsplit_once('.')?.1.to_ascii_lowercase();
    let language = match extension.as_str() {
        "ts" | "tsx" | "mts" | "cts" => "typescript",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "py" | "pyi" => "python",
        "java" => "java",
        "cs" => "csharp",
        "go" => "go",
        "rs" => "rust",
        "php" => "php",
        "rb" | "rake" => "ruby",
        "dart" => "dart",
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hpp" => "cpp",
        _ => return None,
    };
    Some(language.to_string())
}

pub(super) fn is_ui_route(entry: &CodeInventoryItem) -> bool {
    detail_string(&entry.detail, &["routeSurface", "route_surface"])
        .is_some_and(|surface| surface == "ui-navigation")
}

pub(super) fn resolve_db_table_key(
    stable_key: Option<&str>,
    schema: Option<&str>,
    name: Option<&str>,
    table_keys: &BTreeSet<String>,
    stable_table_keys: &BTreeMap<String, String>,
    named_table_keys: &BTreeMap<String, Vec<String>>,
) -> Option<String> {
    if let Some(table_key) = stable_key.and_then(|key| stable_table_keys.get(key)) {
        return Some(table_key.clone());
    }
    let name = name?;
    if let Some(schema) = schema.filter(|schema| !schema.is_empty()) {
        let table_key = db_table_key(Some(schema), name);
        if table_keys.contains(&table_key) {
            return Some(table_key);
        }
    }
    named_table_keys
        .get(name)
        .filter(|matches| matches.len() == 1)
        .and_then(|matches| matches.first().cloned())
}

pub(super) fn constraint_from_foreign_key(
    foreign_key: &DbForeignKey,
    direction: &str,
) -> DbConstraint {
    DbConstraint {
        key: foreign_key.key.clone(),
        name: foreign_key.name.clone(),
        kind: "foreign_key".to_string(),
        columns: foreign_key.columns.clone(),
        column_keys: foreign_key.column_keys.clone(),
        referenced_table_key: foreign_key.referenced_table_key.clone(),
        referenced_schema: foreign_key.referenced_schema.clone(),
        referenced_table: Some(foreign_key.referenced_table.clone()),
        referenced_columns: foreign_key.referenced_columns.clone(),
        referenced_column_keys: foreign_key.referenced_column_keys.clone(),
        expression: None,
        source: format!("foreign_keys.{direction}"),
    }
}

pub(super) fn append_db_constraint(
    snapshot: &mut InventorySnapshot,
    table_key: &str,
    profile_id: &str,
    constraint: &DbConstraint,
    column_ids: &BTreeSet<String>,
    stable_column_ids: &BTreeMap<String, String>,
) {
    let table_id = format!("db:table:{table_key}");
    let identity = constraint
        .key
        .clone()
        .or_else(|| constraint.name.clone())
        .unwrap_or_else(|| {
            format!(
                "{}:{:016x}",
                constraint.kind,
                stable_hash(&serde_json::to_string(constraint).unwrap_or_default())
            )
        });
    let constraint_id = format!("db:constraint:{table_key}:{identity}");
    let mut constraint_item = item(
        &constraint_id,
        "constraint",
        constraint.name.as_deref().unwrap_or(&constraint.kind),
        "data",
        "db",
        Some(&table_id),
        constraint.expression.as_deref(),
    );
    constraint_item.qualified_name = constraint.key.clone().or(Some(identity));
    constraint_item.engine_label = Some(format!("Constraint:{}", constraint.kind));
    constraint_item.project_id = Some(profile_id.to_string());
    constraint_item.is_primary_key = constraint.kind == "primary_key";
    constraint_item.is_foreign_key = constraint.kind == "foreign_key";
    snapshot.items.push(constraint_item);

    let edge_type = constraint.kind.to_ascii_uppercase();
    snapshot.links.push(db_evidence_link(
        table_id,
        constraint_id.clone(),
        "contains",
        constraint.name.clone(),
        &edge_type,
        "structural",
        constraint_evidence(constraint),
    ));
    let endpoint_count = constraint.columns.len().max(constraint.column_keys.len());
    for index in 0..endpoint_count {
        let column = constraint.columns.get(index);
        let stable_key = constraint.column_keys.get(index);
        let Some(column_id) = resolve_db_column_id(
            table_key,
            column.map(String::as_str),
            stable_key.map(String::as_str),
            column_ids,
            stable_column_ids,
        ) else {
            let endpoint = stable_key
                .or(column)
                .map(String::as_str)
                .unwrap_or("unknown");
            snapshot.metadata.gaps.push(gap(
                format!("gap:db-constraint-column:{constraint_id}:{endpoint}"),
                "db-constraint-missing-column",
                "Constraint column이 inventory에 없어 구조 관계를 만들지 않았습니다.",
                vec![constraint_id.clone()],
            ));
            continue;
        };
        let mut link = db_evidence_link(
            constraint_id.clone(),
            column_id,
            "db_constraint",
            constraint.name.clone(),
            &edge_type,
            "confirmed",
            constraint_evidence(constraint),
        );
        push_evidence(
            &mut link.evidence,
            "db-column-key",
            stable_key.map(String::as_str),
        );
        snapshot.links.push(link);
    }
}

pub(super) fn append_db_index(
    snapshot: &mut InventorySnapshot,
    table_key: &str,
    profile_id: &str,
    index: &DbIndex,
    column_ids: &BTreeSet<String>,
    stable_column_ids: &BTreeMap<String, String>,
) {
    let table_id = format!("db:table:{table_key}");
    let identity = index.key.as_deref().unwrap_or(&index.name);
    let index_id = format!("db:index:{table_key}:{identity}");
    let mut index_item = item(
        &index_id,
        "index",
        &index.name,
        "data",
        "db",
        Some(&table_id),
        index.predicate.as_deref().or(index.expression.as_deref()),
    );
    index_item.qualified_name = index.key.clone().or_else(|| Some(index.name.clone()));
    index_item.engine_label = Some("Index".to_string());
    index_item.project_id = Some(profile_id.to_string());
    index_item.is_primary_key = index.primary;
    snapshot.items.push(index_item);

    snapshot.links.push(db_evidence_link(
        table_id,
        index_id.clone(),
        "contains",
        Some(index.name.clone()),
        "INDEX",
        "structural",
        index_evidence(index),
    ));
    let endpoint_count = index.columns.len().max(index.column_keys.len());
    for ordinal in 0..endpoint_count {
        let column = index.columns.get(ordinal);
        let stable_key = index.column_keys.get(ordinal);
        let Some(column_id) = resolve_db_column_id(
            table_key,
            column.map(String::as_str),
            stable_key.map(String::as_str),
            column_ids,
            stable_column_ids,
        ) else {
            let endpoint = stable_key
                .or(column)
                .map(String::as_str)
                .unwrap_or("unknown");
            snapshot.metadata.gaps.push(gap(
                format!("gap:db-index-column:{index_id}:{endpoint}"),
                "db-index-missing-column",
                "Index column이 inventory에 없어 구조 관계를 만들지 않았습니다.",
                vec![index_id.clone()],
            ));
            continue;
        };
        let mut link = db_evidence_link(
            index_id.clone(),
            column_id,
            "db_index",
            Some(index.name.clone()),
            "INDEX",
            "confirmed",
            index_evidence(index),
        );
        push_evidence(
            &mut link.evidence,
            "db-column-key",
            stable_key.map(String::as_str),
        );
        snapshot.links.push(link);
    }
}

pub(super) fn append_db_dependent(
    snapshot: &mut InventorySnapshot,
    table_key: &str,
    table_schema: Option<&str>,
    profile_id: &str,
    dependent: &DbDependentObject,
    stable_column_ids: &BTreeMap<String, String>,
) {
    let table_id = format!("db:table:{table_key}");
    let dependent_id = format!(
        "db:{}:{}",
        dependent.kind,
        encode_db_identity_component(&dependent.key)
    );
    let parent_id = (dependent.kind == "trigger").then_some(table_id.as_str());
    let mut dependent_item = item(
        &dependent_id,
        &dependent.kind,
        &dependent.name,
        "data",
        "db",
        parent_id,
        None,
    );
    dependent_item.qualified_name = Some(dependent.key.clone());
    dependent_item.engine_label = Some(
        match dependent.kind.as_str() {
            "view" => "View",
            "trigger" => "Trigger",
            "routine" => "Routine",
            _ => "DB Object",
        }
        .to_string(),
    );
    dependent_item.project_id = Some(profile_id.to_string());
    dependent_item.group_id = if dependent.kind == "trigger" {
        table_schema.map(str::to_string)
    } else {
        None
    };
    snapshot.items.push(dependent_item);

    let evidence = dependent_evidence(dependent);
    if dependent.kind == "trigger" {
        snapshot.links.push(db_evidence_link(
            table_id,
            dependent_id,
            "db_trigger",
            Some(dependent.name.clone()),
            "TABLE_HAS_TRIGGER",
            "confirmed",
            evidence,
        ));
        return;
    }

    let column_edge_type = match dependent.kind.as_str() {
        "view" => "VIEW_DEPENDS_ON_COLUMN",
        "routine" => "ROUTINE_DEPENDS_ON_COLUMN",
        _ => "DB_OBJECT_DEPENDS_ON_COLUMN",
    };
    let table_edge_type = match dependent.kind.as_str() {
        "view" => "VIEW_DEPENDS_ON_TABLE",
        "routine" => "ROUTINE_DEPENDS_ON_TABLE",
        _ => "DB_OBJECT_DEPENDS_ON_TABLE",
    };
    let mut resolved_columns = 0usize;
    for column_key in &dependent.column_keys {
        let Some(column_id) = stable_column_ids.get(column_key).cloned() else {
            snapshot.metadata.gaps.push(gap(
                format!("gap:db-dependent-column:{dependent_id}:{column_key}"),
                "db-dependent-missing-column",
                "DB 의존 객체의 컬럼 endpoint가 inventory에 없어 해당 컬럼 관계를 만들지 않았습니다.",
                vec![dependent_id.clone(), table_id.clone()],
            ));
            continue;
        };
        let mut link = db_evidence_link(
            dependent_id.clone(),
            column_id,
            "db_dependency",
            Some(dependent.name.clone()),
            column_edge_type,
            "confirmed",
            evidence.clone(),
        );
        push_evidence(&mut link.evidence, "db-column-key", Some(column_key));
        snapshot.links.push(link);
        resolved_columns += 1;
    }

    if dependent.column_keys.is_empty() {
        snapshot.links.push(db_evidence_link(
            dependent_id,
            table_id,
            "db_dependency",
            Some(dependent.name.clone()),
            table_edge_type,
            "confirmed",
            evidence,
        ));
    } else if resolved_columns == 0 {
        let mut link = db_evidence_link(
            dependent_id,
            table_id,
            "db_dependency",
            Some(dependent.name.clone()),
            "DEPENDENCY_SCOPE",
            "structural",
            evidence,
        );
        link.evidence.push(Evidence {
            kind: "db-normalization".to_string(),
            text: "확정된 컬럼 endpoint를 복원하지 못해 의존 객체가 이 테이블 범위에 속한다는 사실만 보존했습니다."
                .to_string(),
        });
        snapshot.links.push(link);
    }
}

pub(super) fn dependent_evidence(dependent: &DbDependentObject) -> Vec<Evidence> {
    vec![
        Evidence {
            kind: "db-object-key".to_string(),
            text: dependent.key.clone(),
        },
        Evidence {
            kind: "db-dependent-kind".to_string(),
            text: dependent.kind.clone(),
        },
        Evidence {
            kind: "db-relation".to_string(),
            text: dependent.relation.clone(),
        },
        Evidence {
            kind: "db-column-keys".to_string(),
            text: serde_json::to_string(&dependent.column_keys)
                .unwrap_or_else(|_| "[]".to_string()),
        },
        Evidence {
            kind: "db-contract-field".to_string(),
            text: "dependents".to_string(),
        },
    ]
}

pub(super) fn resolve_db_column_id(
    table_key: &str,
    column: Option<&str>,
    stable_key: Option<&str>,
    column_ids: &BTreeSet<String>,
    stable_column_ids: &BTreeMap<String, String>,
) -> Option<String> {
    stable_key
        .and_then(|key| stable_column_ids.get(key).cloned())
        .or_else(|| {
            column
                .map(|column| db_column_id(table_key, column))
                .filter(|column_id| column_ids.contains(column_id))
        })
}

pub(super) fn constraint_evidence(constraint: &DbConstraint) -> Vec<Evidence> {
    let mut evidence = vec![
        Evidence {
            kind: "db-constraint-kind".to_string(),
            text: constraint.kind.clone(),
        },
        Evidence {
            kind: "db-columns".to_string(),
            text: serde_json::to_string(&constraint.columns).unwrap_or_else(|_| "[]".to_string()),
        },
        Evidence {
            kind: "db-referenced-columns".to_string(),
            text: serde_json::to_string(&constraint.referenced_columns)
                .unwrap_or_else(|_| "[]".to_string()),
        },
        Evidence {
            kind: "db-column-keys".to_string(),
            text: serde_json::to_string(&constraint.column_keys)
                .unwrap_or_else(|_| "[]".to_string()),
        },
        Evidence {
            kind: "db-referenced-column-keys".to_string(),
            text: serde_json::to_string(&constraint.referenced_column_keys)
                .unwrap_or_else(|_| "[]".to_string()),
        },
        Evidence {
            kind: "db-contract-field".to_string(),
            text: constraint.source.clone(),
        },
    ];
    push_evidence(&mut evidence, "db-object-key", constraint.key.as_deref());
    push_evidence(&mut evidence, "db-object-name", constraint.name.as_deref());
    push_evidence(
        &mut evidence,
        "db-referenced-table-key",
        constraint.referenced_table_key.as_deref(),
    );
    push_evidence(
        &mut evidence,
        "db-referenced-schema",
        constraint.referenced_schema.as_deref(),
    );
    push_evidence(
        &mut evidence,
        "db-referenced-table",
        constraint.referenced_table.as_deref(),
    );
    push_evidence(
        &mut evidence,
        "db-expression",
        constraint.expression.as_deref(),
    );
    evidence
}

pub(super) fn index_evidence(index: &DbIndex) -> Vec<Evidence> {
    let mut evidence = vec![
        Evidence {
            kind: "db-columns".to_string(),
            text: serde_json::to_string(&index.columns).unwrap_or_else(|_| "[]".to_string()),
        },
        Evidence {
            kind: "db-column-keys".to_string(),
            text: serde_json::to_string(&index.column_keys).unwrap_or_else(|_| "[]".to_string()),
        },
        Evidence {
            kind: "db-index-unique".to_string(),
            text: index.unique.to_string(),
        },
        Evidence {
            kind: "db-index-primary".to_string(),
            text: index.primary.to_string(),
        },
    ];
    push_evidence(&mut evidence, "db-object-key", index.key.as_deref());
    push_evidence(&mut evidence, "db-object-name", Some(&index.name));
    push_evidence(
        &mut evidence,
        "db-index-predicate",
        index.predicate.as_deref(),
    );
    push_evidence(
        &mut evidence,
        "db-index-expression",
        index.expression.as_deref(),
    );
    evidence
}

pub(super) fn push_evidence(evidence: &mut Vec<Evidence>, kind: &str, text: Option<&str>) {
    if let Some(text) = text.filter(|text| !text.is_empty()) {
        evidence.push(Evidence {
            kind: kind.to_string(),
            text: text.to_string(),
        });
    }
}

pub(super) fn db_evidence_link(
    from: String,
    to: String,
    kind: &str,
    label: Option<String>,
    engine_edge_type: &str,
    truth_class: &str,
    evidence: Vec<Evidence>,
) -> SnapshotLink {
    SnapshotLink {
        id: format!("{kind}:{from}->{to}"),
        from,
        to,
        kind: kind.to_string(),
        label,
        truth_class: truth_class.to_string(),
        direction: "outbound".to_string(),
        engine_edge_type: Some(engine_edge_type.to_string()),
        evidence,
    }
}

pub(super) fn stable_hash(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

pub(super) fn code_call_link(call: &CodeCall) -> SnapshotLink {
    let (truth_class, confidence) = match call.confidence {
        Some(score) if score >= CONFIRMED_CODE_CALL_CONFIDENCE => ("confirmed", "high"),
        Some(score) if score >= CANDIDATE_CODE_CALL_CONFIDENCE => ("candidate", "medium"),
        Some(_) => ("unknown", "low"),
        None => ("unknown", "unknown"),
    };
    let mut evidence = vec![
        Evidence {
            kind: "engine-edge".to_string(),
            text: "codebase-memory CALLS".to_string(),
        },
        Evidence {
            kind: "engine-confidence".to_string(),
            text: confidence.to_string(),
        },
        Evidence {
            kind: "engine-confidence-score".to_string(),
            text: call
                .confidence
                .map(|score| format!("{score}%"))
                .unwrap_or_else(|| "점수 없음".to_string()),
        },
    ];
    if let Some(strategy) = call.strategy.as_deref() {
        evidence.push(Evidence {
            kind: "engine-strategy".to_string(),
            text: strategy.to_string(),
        });
    }
    if let Some(expression) = call.expression.as_deref() {
        evidence.push(Evidence {
            kind: "engine-callee".to_string(),
            text: expression.to_string(),
        });
    }
    if let Some(path) = call.path.as_deref() {
        evidence.push(Evidence {
            kind: "engine-source-path".to_string(),
            text: path.to_string(),
        });
    }
    if !call.range.is_empty() {
        evidence.push(Evidence {
            kind: "engine-source-range".to_string(),
            text: serde_json::to_string(&call.range).unwrap_or_default(),
        });
    }

    SnapshotLink {
        id: format!("code-call:{}->{}", call.from, call.to),
        from: format!("code:{}", call.from),
        to: format!("code:{}", call.to),
        kind: "code_call".to_string(),
        label: Some("CALLS".to_string()),
        truth_class: truth_class.to_string(),
        direction: "outbound".to_string(),
        engine_edge_type: Some("CALLS".to_string()),
        evidence,
    }
}

pub(super) fn code_architecture_links(architecture: &Value) -> Vec<SnapshotLink> {
    architecture
        .get("edges")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|edge| edge.get("level").and_then(Value::as_str) == Some("summary"))
        .filter(|edge| edge.get("kind").and_then(Value::as_str) != Some("CONTAINS"))
        .filter_map(|edge| {
            let from = edge.get("from").and_then(Value::as_str)?;
            let to = edge.get("to").and_then(Value::as_str)?;
            let edge_type = edge.get("kind").and_then(Value::as_str)?;
            let properties = edge.get("properties").and_then(Value::as_object);
            let resolution = properties
                .and_then(|properties| properties.get("resolution"))
                .and_then(Value::as_str);
            let truth_class = match resolution {
                Some("provider" | "handler" | "resolved") => "confirmed",
                Some("internal" | "external" | "db_memory") => "structural",
                Some("source-candidate") => "candidate",
                Some("runtime-dependent" | "unknown") => "unknown",
                _ if properties.is_some_and(|properties| properties.contains_key("framework")) => {
                    "structural"
                }
                _ => "unknown",
            };
            let mut evidence = vec![
                Evidence {
                    kind: "engine-edge".to_string(),
                    text: format!("codebase-memory architecture {edge_type}"),
                },
                Evidence {
                    kind: "architecture-level".to_string(),
                    text: "summary".to_string(),
                },
            ];
            if let Some(properties) = properties {
                for key in [
                    "resolution",
                    "strategy",
                    "confidence",
                    "framework",
                    "source",
                ] {
                    if let Some(value) = properties.get(key).and_then(Value::as_str) {
                        evidence.push(Evidence {
                            kind: format!("architecture-{key}"),
                            text: value.to_string(),
                        });
                    }
                }
            }
            for location in edge
                .get("evidence")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let path = location
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let range = location
                    .get("range")
                    .map(|range| range.to_string())
                    .unwrap_or_default();
                let note = location
                    .get("note")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                evidence.push(Evidence {
                    kind: "architecture-source".to_string(),
                    text: format!("{path}:{range} {note}").trim().to_string(),
                });
            }
            let id = edge
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("{edge_type}:{from}->{to}"));
            Some(SnapshotLink {
                id: format!("code-architecture:{id}"),
                from: format!("code:{from}"),
                to: format!("code:{to}"),
                kind: "code_architecture".to_string(),
                label: Some(edge_type.to_string()),
                truth_class: truth_class.to_string(),
                direction: "outbound".to_string(),
                engine_edge_type: Some(edge_type.to_string()),
                evidence,
            })
        })
        .collect()
}

pub(super) fn confirmed_link(
    id: String,
    from: String,
    to: String,
    kind: &str,
    engine_edge_type: &str,
    evidence: &str,
) -> SnapshotLink {
    SnapshotLink {
        id,
        from,
        to,
        kind: kind.to_string(),
        label: Some(engine_edge_type.to_string()),
        truth_class: "confirmed".to_string(),
        direction: "outbound".to_string(),
        engine_edge_type: Some(engine_edge_type.to_string()),
        evidence: vec![Evidence {
            kind: "engine-edge".to_string(),
            text: evidence.to_string(),
        }],
    }
}

pub(super) fn normalize_item(entry: &mut InventoryItem) {
    if entry.location.is_none() && entry.is_code() {
        entry.location = entry.path.clone().map(|path| SourceLocation {
            path,
            line: None,
            column: None,
            end_line: None,
            end_column: None,
        });
    }
}

pub(super) fn compatible_items(left: &InventoryItem, right: &InventoryItem) -> bool {
    left.kind == right.kind
        && left.name == right.name
        && left.layer == right.layer
        && left.source == right.source
        && compatible_option(left.path.as_ref(), right.path.as_ref())
        && compatible_option(left.qualified_name.as_ref(), right.qualified_name.as_ref())
        && compatible_option(left.engine_label.as_ref(), right.engine_label.as_ref())
        && compatible_option(left.project_id.as_ref(), right.project_id.as_ref())
}

pub(super) fn compatible_option<T: PartialEq>(left: Option<&T>, right: Option<&T>) -> bool {
    left.zip(right).is_none_or(|(left, right)| left == right)
}

pub(super) fn merge_item(target: &mut InventoryItem, source: InventoryItem) {
    target.parent_id = target.parent_id.take().or(source.parent_id);
    target.path = target.path.take().or(source.path);
    target.qualified_name = target.qualified_name.take().or(source.qualified_name);
    target.engine_label = target.engine_label.take().or(source.engine_label);
    target.project_id = target.project_id.take().or(source.project_id);
    target.group_id = target.group_id.take().or(source.group_id);
    target.location = merge_location(target.location.take(), source.location);
    target.is_primary_key |= source.is_primary_key;
    target.is_foreign_key |= source.is_foreign_key;
    target.nullable = target.nullable.or(source.nullable);
}

pub(super) fn merge_location(
    target: Option<SourceLocation>,
    source: Option<SourceLocation>,
) -> Option<SourceLocation> {
    match (target, source) {
        (Some(mut target), Some(source)) if target.path == source.path => {
            target.line = target.line.or(source.line);
            target.column = target.column.or(source.column);
            target.end_line = target.end_line.or(source.end_line);
            target.end_column = target.end_column.or(source.end_column);
            Some(target)
        }
        (Some(target), _) => Some(target),
        (None, source) => source,
    }
}

pub(super) fn normalize_link(link: &mut SnapshotLink) {
    if link.id.is_empty() {
        link.id = format!("{}:{}->{}", link.kind, link.from, link.to);
    }
    if link.truth_class.is_empty() {
        link.truth_class = match link.kind.as_str() {
            "code_call" | "code_handle" | "client_request" | "db_fk" => "confirmed",
            "contains" | "db_constraint" => "structural",
            kind if kind.starts_with("candidate") => "candidate",
            _ => "unknown",
        }
        .to_string();
    }
    if link.direction.is_empty() {
        link.direction = "outbound".to_string();
    }
    if link.engine_edge_type.is_none() {
        link.engine_edge_type = match link.kind.as_str() {
            "code_call" => Some("CALLS"),
            "code_handle" => Some("HANDLES"),
            "client_request" => Some("CLIENT_REQUEST"),
            "db_fk" => Some("FOREIGN_KEY"),
            _ => None,
        }
        .map(str::to_string);
    }
    if link.evidence.is_empty() {
        if let Some(edge_type) = link.engine_edge_type.as_deref() {
            link.evidence.push(Evidence {
                kind: "engine-edge".to_string(),
                text: edge_type.to_string(),
            });
        }
    }
    link.evidence.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.text.cmp(&right.text))
    });
    link.evidence.dedup();
}

pub(super) fn mark_reindex_required(snapshot: &mut InventorySnapshot, note: &str) {
    snapshot.metadata.migration.reindex_required = true;
    push_unique(&mut snapshot.metadata.migration.notes, note);
    push_unique(&mut snapshot.stale_reasons, REINDEX_REASON);
}

pub(super) fn gap(id: String, kind: &str, message: &str, related_ids: Vec<String>) -> SnapshotGap {
    SnapshotGap {
        id,
        kind: kind.to_string(),
        message: message.to_string(),
        related_ids,
    }
}
