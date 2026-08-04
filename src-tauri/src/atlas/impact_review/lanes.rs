fn impact_direct_items(
    snapshot: &InventorySnapshot,
    table: &InventoryItem,
    column: Option<&InventoryItem>,
    item_by_id: &HashMap<&str, &InventoryItem>,
    links_by_from: &ImpactLinkIndex<'_>,
    links_by_to: &ImpactLinkIndex<'_>,
) -> Vec<ImpactReviewItem> {
    let relevant_objects = snapshot.items.iter().filter(|item| {
        let structural_object = item.parent_id.as_deref() == Some(table.id.as_str())
            && matches!(item.kind.as_str(), "constraint" | "index")
            && (column.is_none()
                || links_by_from
                    .get(item.id.as_str())
                    .into_iter()
                    .flatten()
                    .any(|link| {
                        matches!(link.kind.as_str(), "db_constraint" | "db_index")
                            && column.is_some_and(|column| link.to == column.id)
                    }));
        let dependent_object = matches!(item.kind.as_str(), "view" | "trigger" | "routine")
            && db_dependent_touches_focus(
                item,
                table,
                column,
                item_by_id,
                links_by_from,
                links_by_to,
            );
        structural_object || dependent_object
    });

    let mut items = relevant_objects
        .map(|object| {
            direct_object_review_item(
                object,
                table,
                column,
                item_by_id,
                links_by_from,
                links_by_to,
            )
        })
        .collect::<Vec<_>>();

    for link in snapshot.links.iter().filter(|link| {
        link.kind == "db_fk" && impact_link_touches_focus(link, table, column, item_by_id)
    }) {
        let detail = foreign_key_detail(link, item_by_id);
        let object_key = evidence_value(&link.evidence, "db-object-key");
        if let Some(existing) = object_key.and_then(|key| {
            items
                .iter_mut()
                .find(|item| evidence_value(&item.evidence, "db-object-key") == Some(key))
        }) {
            let mut evidence = existing.evidence.clone();
            evidence.extend(link.evidence.iter().cloned());
            existing.detail = detail;
            existing.evidence = safe_evidence(&evidence);
            continue;
        }
        items.push(ImpactReviewItem {
            id: format!("direct:{}", link.id),
            node_id: related_fk_node_id(link, table, item_by_id),
            kind: "foreign-key-reference".to_string(),
            title: link
                .label
                .clone()
                .unwrap_or_else(|| "Foreign key reference".to_string()),
            detail,
            truth_class: review_truth_class(&link.truth_class),
            confidence: None,
            rank: 0,
            evidence: safe_evidence(&link.evidence),
            location: None,
        });
    }

    for link in snapshot.links.iter().filter(|link| {
        link.is_confirmed()
            && matches!(
                link.kind.as_str(),
                "code_db_read" | "code_db_write" | "code_db_uses_column"
            )
            && semantic_link_touches_focus(link, table, column, item_by_id)
    }) {
        let Some(code) = item_by_id.get(link.from.as_str()).copied() else {
            continue;
        };
        let operation = match link.kind.as_str() {
            "code_db_read" => "정적 SQL 조회",
            "code_db_write" => "정적 SQL 변경",
            _ => "정적 SQL 컬럼 사용",
        };
        items.push(ImpactReviewItem {
            id: format!("direct:{}", link.id),
            node_id: Some(code.id.clone()),
            kind: link.kind.clone(),
            title: code.name.clone(),
            detail: operation.to_string(),
            truth_class: "confirmed".to_string(),
            confidence: None,
            rank: 0,
            evidence: safe_evidence(&link.evidence),
            location: code.location.clone(),
        });
    }

    let focus_columns = match column {
        Some(column) => vec![column],
        None => snapshot
            .items
            .iter()
            .filter(|item| {
                item.kind == "column" && item.parent_id.as_deref() == Some(table.id.as_str())
            })
            .collect(),
    };
    for focus_column in focus_columns {
        if focus_column.is_primary_key
            && !items.iter().any(|item| {
                item.kind == "primary-key"
                    && review_item_mentions_node(item, focus_column.id.as_str(), links_by_from)
            })
        {
            items.push(metadata_constraint_review_item(
                focus_column,
                "primary-key",
                "Primary key",
            ));
        }
        if focus_column.is_foreign_key
            && !items.iter().any(|item| {
                matches!(item.kind.as_str(), "foreign-key" | "foreign-key-reference")
                    && review_item_mentions_node(item, focus_column.id.as_str(), links_by_from)
            })
        {
            items.push(metadata_constraint_review_item(
                focus_column,
                "foreign-key",
                "Foreign key",
            ));
        }
    }

    let mut seen = HashSet::new();
    items.retain(|item| seen.insert(item.id.clone()));
    items
}

fn evidence_value<'a>(evidence: &'a [Evidence], kind: &str) -> Option<&'a str> {
    evidence
        .iter()
        .find(|entry| entry.kind == kind)
        .map(|entry| entry.text.as_str())
}

fn foreign_key_detail(link: &SnapshotLink, item_by_id: &HashMap<&str, &InventoryItem>) -> String {
    let endpoint = |id: &str| {
        let Some(item) = item_by_id.get(id).copied() else {
            return id.to_string();
        };
        let Some(table) = item
            .parent_id
            .as_deref()
            .and_then(|parent_id| item_by_id.get(parent_id).copied())
        else {
            return item.name.clone();
        };
        let table_name = table.group_id.as_deref().map_or_else(
            || table.name.clone(),
            |schema| format!("{schema}.{}", table.name),
        );
        format!("{table_name}.{}", item.name)
    };
    safe_text(&format!(
        "FK · {} → {}",
        endpoint(&link.from),
        endpoint(&link.to)
    ))
}

fn semantic_link_touches_focus(
    link: &SnapshotLink,
    table: &InventoryItem,
    column: Option<&InventoryItem>,
    item_by_id: &HashMap<&str, &InventoryItem>,
) -> bool {
    match column {
        Some(column) => link.to == column.id,
        None if link.to == table.id => true,
        None => item_by_id
            .get(link.to.as_str())
            .is_some_and(|item| item.parent_id.as_deref() == Some(table.id.as_str())),
    }
}

fn direct_object_review_item(
    object: &InventoryItem,
    table: &InventoryItem,
    column: Option<&InventoryItem>,
    item_by_id: &HashMap<&str, &InventoryItem>,
    links_by_from: &ImpactLinkIndex<'_>,
    links_by_to: &ImpactLinkIndex<'_>,
) -> ImpactReviewItem {
    let links = links_by_from
        .get(object.id.as_str())
        .into_iter()
        .flatten()
        .filter(|link| {
            if matches!(link.kind.as_str(), "db_constraint" | "db_index") {
                return column.is_none_or(|column| link.to == column.id);
            }
            link.kind == "db_dependency"
                && column.map_or_else(
                    || link_endpoint_belongs_to_table(link.to.as_str(), table, item_by_id),
                    |column| link.to == column.id,
                )
        })
        .chain(
            links_by_to
                .get(object.id.as_str())
                .into_iter()
                .flatten()
                .filter(|link| {
                    column.is_none()
                        && matches!(link.kind.as_str(), "contains" | "db_trigger")
                        && link.from == table.id
                }),
        )
        .collect::<Vec<_>>();
    let confirmed = links.iter().any(|link| link.is_confirmed());
    let evidence = links
        .iter()
        .flat_map(|link| link.evidence.iter().cloned())
        .collect::<Vec<_>>();
    let kind = direct_object_kind(object, &evidence);
    let mut columns = links
        .iter()
        .filter_map(|link| item_by_id.get(link.to.as_str()))
        .filter(|item| item.kind == "column")
        .map(|item| item.name.clone())
        .collect::<Vec<_>>();
    columns.sort();
    columns.dedup();
    let detail = if columns.is_empty() {
        direct_kind_label(&kind).to_string()
    } else {
        format!("{} · {}", direct_kind_label(&kind), columns.join(", "))
    };
    ImpactReviewItem {
        id: format!("direct:{}", object.id),
        node_id: Some(object.id.clone()),
        kind,
        title: object.name.clone(),
        detail: safe_text(&detail),
        truth_class: if confirmed { "confirmed" } else { "structural" }.to_string(),
        confidence: None,
        rank: 0,
        evidence: safe_evidence(&evidence),
        location: None,
    }
}

fn metadata_constraint_review_item(
    column: &InventoryItem,
    kind: &str,
    title: &str,
) -> ImpactReviewItem {
    ImpactReviewItem {
        id: format!("direct:metadata:{kind}:{}", column.id),
        node_id: Some(column.id.clone()),
        kind: kind.to_string(),
        title: format!("{title} · {}", column.name),
        detail: "컬럼 메타데이터에서 직접 읽음".to_string(),
        truth_class: "confirmed".to_string(),
        confidence: None,
        rank: 0,
        evidence: vec![Evidence {
            kind: "db-metadata".to_string(),
            text: format!("{} 컬럼의 {} 표시", column.name, title.to_ascii_uppercase()),
        }],
        location: None,
    }
}

fn impact_candidate_items(
    edges: &[VisualEdge],
    item_by_id: &HashMap<&str, &InventoryItem>,
) -> Vec<ImpactReviewItem> {
    let mut seen = HashSet::new();
    edges
        .iter()
        .filter_map(|edge| {
            let code = item_by_id.get(edge.from.as_str()).copied()?;
            if code.source != "code" || !seen.insert(code.id.as_str()) {
                return None;
            }
            Some(ImpactReviewItem {
                id: format!("candidate:{}", code.id),
                node_id: Some(code.id.clone()),
                kind: code.kind.clone(),
                title: code.name.clone(),
                detail: candidate_detail(code),
                truth_class: "candidate".to_string(),
                confidence: edge.confidence.clone(),
                rank: 0,
                evidence: safe_evidence(&edge.evidence),
                location: code.location.clone(),
            })
        })
        .collect()
}

fn impact_unknown_items(
    snapshot: &InventorySnapshot,
    table: &InventoryItem,
    column: Option<&InventoryItem>,
    direct: &[ImpactReviewItem],
    candidates: &[ImpactReviewItem],
    item_by_id: &HashMap<&str, &InventoryItem>,
    links_by_from: &ImpactLinkIndex<'_>,
) -> Vec<ImpactReviewItem> {
    let mut items = Vec::new();
    for (index, reason) in snapshot.stale_reasons.iter().enumerate() {
        items.push(unknown_review_item(
            format!("unknown:stale:{index}"),
            "stale",
            "Snapshot 재확인 필요",
            reason,
        ));
    }
    if snapshot.metadata.migration.reindex_required {
        items.push(unknown_review_item(
            "unknown:reindex".to_string(),
            "reindex",
            "재인덱싱 필요",
            "이 snapshot은 현재 계약으로 완전히 검증되지 않았습니다.",
        ));
    }
    for (index, note) in snapshot.metadata.migration.notes.iter().enumerate() {
        items.push(unknown_review_item(
            format!("unknown:migration-note:{index}"),
            "snapshot-migration",
            "Snapshot 변환 기록",
            note,
        ));
    }
    let candidate_ids = candidates
        .iter()
        .filter_map(|candidate| candidate.node_id.as_deref())
        .collect::<HashSet<_>>();
    let capability_gaps = snapshot
        .metadata
        .gaps
        .iter()
        .filter(|gap| gap.kind == "db-capability")
        .collect::<Vec<_>>();
    if !capability_gaps.is_empty() {
        let mut item = unknown_review_item(
            "unknown:db-capability".to_string(),
            "db-capability",
            "DB에서 확인하지 못하는 구조",
            &format!(
                "현재 DB 어댑터가 수집하지 않는 구조 정보가 {}종 있습니다. 실제 스키마에 해당 객체가 있을 때 영향 분석이 불완전할 수 있습니다.",
                capability_gaps.len()
            ),
        );
        item.evidence = capability_gaps
            .iter()
            .map(|gap| Evidence {
                kind: "db-capability".to_string(),
                text: safe_text(&gap.message),
            })
            .collect();
        items.push(item);
    }
    for gap in snapshot.metadata.gaps.iter().filter(|gap| {
        gap.kind != "db-capability"
            && (gap.related_ids.is_empty()
                || gap.related_ids.iter().any(|id| {
                    id == &table.id
                        || column.is_some_and(|column| id == &column.id)
                        || candidate_ids.contains(id.as_str())
                        || item_by_id.get(id.as_str()).is_some_and(|item| {
                            item.parent_id.as_deref() == Some(table.id.as_str())
                        })
                }))
    }) {
        items.push(unknown_review_item(
            format!("unknown:{}", gap.id),
            &gap.kind,
            if gap.kind.starts_with("code-search") {
                "코드 텍스트 근거 확인 필요"
            } else {
                "DB 메타데이터 누락"
            },
            &gap.message,
        ));
    }
    if snapshot.metadata.db.is_none() {
        items.push(unknown_review_item(
            "unknown:db-source".to_string(),
            "missing-source",
            "DB 출처 정보 없음",
            "DB snapshot 출처와 capability를 확인할 수 없습니다.",
        ));
    }
    if snapshot.metadata.code.is_none() {
        items.push(unknown_review_item(
            "unknown:code-source".to_string(),
            "missing-source",
            "코드 출처 정보 없음",
            "코드 후보의 snapshot 출처를 확인할 수 없습니다.",
        ));
    }
    if column.is_some_and(|column| column.parent_id.as_deref() != Some(table.id.as_str())) {
        items.push(unknown_review_item(
            "unknown:missing-parent-table".to_string(),
            "missing-db-parent",
            "상위 테이블 미확인",
            "컬럼의 상위 테이블 관계가 snapshot에 없어 테이블 단위 영향은 알 수 없습니다.",
        ));
    }
    if direct.is_empty() {
        items.push(unknown_review_item(
            "unknown:no-direct-facts".to_string(),
            "missing-db-facts",
            "직접 영향 미확인",
            "연결된 제약·인덱스를 읽지 못했습니다. 영향 없음으로 확정하지 않습니다.",
        ));
    }
    if candidates.is_empty() {
        items.push(unknown_review_item(
            "unknown:no-code-candidates".to_string(),
            "missing-code-candidates",
            "코드 영향 미확인",
            "이름·경로 근거 후보가 없습니다. 코드 영향 없음으로 확정하지 않습니다.",
        ));
    } else {
        let missing_locations = candidates
            .iter()
            .filter(|candidate| candidate.location.is_none())
            .count();
        if missing_locations > 0 {
            items.push(unknown_review_item(
                "unknown:missing-code-locations".to_string(),
                "missing-source-location",
                "소스 위치 누락",
                &format!("후보 {missing_locations}개의 파일·라인 위치를 확인할 수 없습니다."),
            ));
        }

        let reachable = api_reachable_code_ids(snapshot, links_by_from, 4, 20_000);
        let disconnected = candidates
            .iter()
            .filter_map(|candidate| candidate.node_id.as_deref())
            .filter(|node_id| !reachable.contains(*node_id))
            .count();
        if disconnected > 0 {
            items.push(unknown_review_item(
                "unknown:disconnected-api-path".to_string(),
                "disconnected-call-path",
                "API 경로 미연결",
                &format!("후보 {disconnected}개는 4 hop 내 확정 HANDLES/CALLS 경로가 없습니다."),
            ));
        }
    }
    let mut seen = HashSet::new();
    items.retain(|item| seen.insert(item.id.clone()));
    items
}

fn impact_check_items(
    snapshot: &InventorySnapshot,
    table: &InventoryItem,
    column: Option<&InventoryItem>,
    direct: &[ImpactReviewItem],
    candidates: &[ImpactReviewItem],
    change_intent: Option<&ChangeIntent>,
) -> Vec<ImpactReviewItem> {
    let mut checks = Vec::new();
    if let Some(intent) = change_intent {
        checks.extend(change_intent_checks(intent, table, column));
    }
    if !snapshot.stale_reasons.is_empty() || snapshot.metadata.migration.reindex_required {
        checks.push(action_review_item(
            "check:reindex",
            None,
            "reindex",
            "먼저 snapshot 다시 읽기",
            "stale 또는 migration 상태를 해소한 뒤 영향 범위를 다시 확인합니다.",
            None,
            Vec::new(),
        ));
    }
    for item in direct.iter().filter(|item| {
        matches!(
            item.kind.as_str(),
            "primary-key"
                | "foreign-key"
                | "foreign-key-reference"
                | "unique"
                | "check"
                | "index"
                | "unique-index"
                | "primary-index"
        )
    }) {
        checks.push(action_review_item(
            &format!("check:{}", item.id),
            item.node_id.clone(),
            "constraint",
            &format!("DB 정의 확인 · {}", item.title),
            &item.detail,
            None,
            item.evidence.clone(),
        ));
    }
    for candidate in candidates {
        let kind = candidate_check_kind(candidate);
        checks.push(action_review_item(
            &format!("check:{}", candidate.id),
            candidate.node_id.clone(),
            kind,
            &format!("{} · {}", check_action_label(kind), candidate.title),
            &candidate.detail,
            candidate.location.clone(),
            candidate.evidence.clone(),
        ));
    }
    if direct.is_empty() {
        checks.push(action_review_item(
            "check:db-coverage",
            Some(table.id.clone()),
            "coverage",
            "DB metadata coverage 확인",
            "adapter가 PK/FK/unique/check/index를 지원하는지 확인한 뒤 변경합니다.",
            None,
            Vec::new(),
        ));
    }
    if !candidates
        .iter()
        .any(|item| candidate_check_kind(item) == "migration")
    {
        checks.push(action_review_item(
            "check:migration-location",
            column
                .map(|column| column.id.clone())
                .or_else(|| Some(table.id.clone())),
            "migration-missing",
            "마이그레이션 위치 확인",
            "연결된 migration/DDL 파일 후보가 없어 저장소의 실제 스키마 변경 경로를 확인합니다.",
            None,
            Vec::new(),
        ));
    }
    if !candidates
        .iter()
        .any(|item| candidate_check_kind(item) == "test")
    {
        checks.push(action_review_item(
            "check:test-location",
            column
                .map(|column| column.id.clone())
                .or_else(|| Some(table.id.clone())),
            "test-missing",
            "회귀 테스트 위치 확인",
            "연결된 test/spec 파일 후보가 없어 제약·조회 동작을 검증할 테스트 위치를 확인합니다.",
            None,
            Vec::new(),
        ));
    }
    let mut seen = HashSet::new();
    checks.retain(|item| seen.insert(item.id.clone()));
    checks
}

