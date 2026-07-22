use std::collections::{HashMap, HashSet, VecDeque};

use super::model::{
    ChangeIntent, Evidence, ImpactReviewBoard, ImpactReviewItem, ImpactReviewLane, InventoryItem,
    InventorySnapshot, SnapshotLink, SourceLocation, VisualEdge,
};
use super::projection_support::{assign_review_ranks, confidence_rank, safe_evidence, safe_text};

const DIRECT_REVIEW_LIMIT: usize = 12;
const CANDIDATE_REVIEW_LIMIT: usize = 10;
const UNKNOWN_REVIEW_LIMIT: usize = 8;
const CHECK_REVIEW_LIMIT: usize = 10;
type ImpactLinkIndex<'a> = HashMap<&'a str, Vec<&'a SnapshotLink>>;

pub(super) fn impact_review_board(
    snapshot: &InventorySnapshot,
    table: &InventoryItem,
    column: Option<&InventoryItem>,
    candidate_edges: &[VisualEdge],
    change_intent: Option<&ChangeIntent>,
) -> ImpactReviewBoard {
    let item_by_id = snapshot
        .items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let (links_by_from, links_by_to) = impact_link_indexes(snapshot);
    let mut direct = impact_direct_items(
        snapshot,
        table,
        column,
        &item_by_id,
        &links_by_from,
        &links_by_to,
    );
    direct.sort_by(|left, right| {
        direct_review_rank(&left.kind)
            .cmp(&direct_review_rank(&right.kind))
            .then_with(|| left.title.cmp(&right.title))
            .then_with(|| left.id.cmp(&right.id))
    });
    assign_review_ranks(&mut direct);

    let mut candidates = impact_candidate_items(candidate_edges, &item_by_id);
    candidates.sort_by(|left, right| {
        confidence_rank(left.confidence.as_deref().unwrap_or(""))
            .cmp(&confidence_rank(right.confidence.as_deref().unwrap_or("")))
            .then_with(|| {
                candidate_review_rank(&left.kind).cmp(&candidate_review_rank(&right.kind))
            })
            .then_with(|| left.title.cmp(&right.title))
            .then_with(|| left.id.cmp(&right.id))
    });
    assign_review_ranks(&mut candidates);

    let mut unknowns = impact_unknown_items(
        snapshot,
        table,
        column,
        &direct,
        &candidates,
        &item_by_id,
        &links_by_from,
    );
    assign_review_ranks(&mut unknowns);
    let mut checks =
        impact_check_items(snapshot, table, column, &direct, &candidates, change_intent);
    checks.sort_by(|left, right| {
        check_review_rank(&left.kind)
            .cmp(&check_review_rank(&right.kind))
            .then_with(|| left.title.cmp(&right.title))
    });
    assign_review_ranks(&mut checks);

    let subject = match column {
        Some(column) if table.id != column.id => format!("{}.{}", table.name, column.name),
        Some(column) => column.name.clone(),
        None => table.name.clone(),
    };
    let scope = if column.is_some() { "column" } else { "table" }.to_string();
    let lanes = vec![
        review_lane("direct", direct, DIRECT_REVIEW_LIMIT),
        review_lane("candidates", candidates, CANDIDATE_REVIEW_LIMIT),
        review_lane("unknowns", unknowns, UNKNOWN_REVIEW_LIMIT),
        review_lane("checks", checks, CHECK_REVIEW_LIMIT),
    ];
    let markdown_summary = impact_markdown_summary(&subject, change_intent, &lanes);

    ImpactReviewBoard {
        subject,
        scope,
        change_intent: change_intent.cloned(),
        lanes,
        markdown_summary,
    }
}

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
        let from = item_by_id
            .get(link.from.as_str())
            .map(|item| item.name.as_str())
            .unwrap_or(link.from.as_str());
        let to = item_by_id
            .get(link.to.as_str())
            .map(|item| item.name.as_str())
            .unwrap_or(link.to.as_str());
        items.push(ImpactReviewItem {
            id: format!("direct:{}", link.id),
            node_id: related_fk_node_id(link, table, item_by_id),
            kind: "foreign-key-reference".to_string(),
            title: link
                .label
                .clone()
                .unwrap_or_else(|| "Foreign key reference".to_string()),
            detail: safe_text(&format!("{from} → {to}")),
            truth_class: review_truth_class(&link.truth_class),
            confidence: None,
            rank: 0,
            evidence: safe_evidence(&link.evidence),
            location: None,
        });
    }

    for link in snapshot.links.iter().filter(|link| {
        link.truth_class == "confirmed"
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
    let confirmed = links.iter().any(|link| link.truth_class == "confirmed");
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

fn change_intent_checks(
    intent: &ChangeIntent,
    table: &InventoryItem,
    column: Option<&InventoryItem>,
) -> Vec<ImpactReviewItem> {
    let node_id = column
        .map(|column| column.id.clone())
        .or_else(|| Some(table.id.clone()));
    let target = column
        .map(|column| format!("{}.{}", table.name, column.name))
        .unwrap_or_else(|| table.name.clone());
    let value = intent
        .value
        .as_deref()
        .filter(|value| !value.trim().is_empty());
    let mut checks = Vec::new();
    let mut add = |id: &str, kind: &str, title: &str, detail: String| {
        checks.push(action_review_item(
            id,
            node_id.clone(),
            kind,
            title,
            &detail,
            None,
            Vec::new(),
        ));
    };

    match intent.kind.as_str() {
        "rename" => {
            if let Some(new_name) = value {
                add(
                    "check:change:rename-target",
                    "change-target",
                    "새 이름과 충돌 확인",
                    format!("{target} → {new_name}. 같은 스키마의 기존 컬럼과 충돌하는지 확인합니다."),
                );
            } else {
                add(
                    "check:change:rename-target",
                    "change-input",
                    "새 컬럼명 입력",
                    "새 이름이 없어 충돌 여부와 호환 경로를 아직 판단할 수 없습니다.".to_string(),
                );
            }
            add(
                "check:change:rename-contract",
                "change-contract",
                "구·신 이름 호환 기간 결정",
                "동적 SQL, 직렬화 이름, 외부 소비자가 남아 있을 수 있어 한 번에 이름을 바꿀지 단계적으로 전환할지 정합니다.".to_string(),
            );
        }
        "drop" => {
            add(
                "check:change:drop-usage",
                "change-contract",
                "삭제 전 읽기·쓰기 중단 확인",
                format!("{target}을 읽거나 쓰는 코드와 외부 소비자가 모두 제거됐는지 확인합니다."),
            );
            add(
                "check:change:drop-data",
                "change-data",
                "데이터 보존과 롤백 경로 결정",
                "실제 행 데이터는 분석하지 않았습니다. 삭제 전 백업·보존 기간과 되돌리기 절차를 확인합니다.".to_string(),
            );
        }
        "type" => {
            if let Some(new_type) = value {
                add(
                    "check:change:type-target",
                    "change-target",
                    "변환 가능 범위 확인",
                    format!("{target}의 값을 {new_type}(으)로 손실 없이 변환할 수 있는지 표본과 전체 데이터에서 확인합니다."),
                );
            } else {
                add(
                    "check:change:type-target",
                    "change-input",
                    "목표 타입 입력",
                    "목표 타입이 없어 캐스트·범위·정밀도 위험을 아직 판단할 수 없습니다.".to_string(),
                );
            }
            add(
                "check:change:type-contract",
                "change-contract",
                "바인딩·인덱스·직렬화 확인",
                "DB 드라이버 바인딩, API 직렬화, 비교·정렬, 관련 인덱스가 새 타입과 호환되는지 확인합니다.".to_string(),
            );
        }
        "nullability" => match value {
            Some("required") => {
                add(
                    "check:change:nullability-data",
                    "change-data",
                    "NULL 행과 백필 확인",
                    format!("{target}을 NOT NULL로 바꾸기 전에 기존 NULL 행과 모든 쓰기 경로의 기본값을 확인합니다."),
                );
                add(
                    "check:change:nullability-contract",
                    "change-contract",
                    "쓰기 검증 순서 확인",
                    "애플리케이션 검증과 백필을 먼저 배포한 뒤 DB 제약을 적용할지 순서를 결정합니다.".to_string(),
                );
            }
            Some("nullable") => add(
                "check:change:nullability-contract",
                "change-contract",
                "NULL 소비 경로 확인",
                format!("{target}이 NULL일 때 직렬화·계산·정렬·UI 소비자가 안전하게 처리하는지 확인합니다."),
            ),
            _ => add(
                "check:change:nullability-target",
                "change-input",
                "NULL 허용 방향 선택",
                "NULL 허용 또는 NOT NULL 중 목표가 없어 데이터와 쓰기 경로 위험을 판단할 수 없습니다.".to_string(),
            ),
        },
        _ => {}
    }
    checks
}

fn review_lane(id: &str, mut items: Vec<ImpactReviewItem>, limit: usize) -> ImpactReviewLane {
    let (order, title, description, tone, empty_message) = match id {
        "direct" => (
            1,
            "직접 영향",
            "DB에서 직접 읽은 제약·인덱스·참조 구조",
            "confirmed",
            "직접 영향 메타데이터가 없습니다. 영향 없음으로 확정하지 않습니다.",
        ),
        "candidates" => (
            2,
            "코드 영향 후보",
            "이름·경로 근거로 정렬한 코드·파일·API 후보",
            "candidate",
            "코드 후보를 찾지 못했습니다. 코드 영향 없음으로 확정하지 않습니다.",
        ),
        "unknowns" => (
            3,
            "확인 필요",
            "지원 범위·stale·누락·끊긴 경로",
            "unknown",
            "현재 snapshot에 기록된 추가 확인 항목은 없습니다.",
        ),
        "checks" => (
            4,
            "권장 확인",
            "수정 전에 열어볼 근거를 순서대로 정리",
            "action",
            "확정 근거가 부족해 권장 확인 순서를 만들 수 없습니다.",
        ),
        _ => unreachable!("review lane ids are fixed by the projection"),
    };
    let total = items.len();
    items.truncate(limit);
    ImpactReviewLane {
        id: id.to_string(),
        order,
        title: title.to_string(),
        description: description.to_string(),
        tone: tone.to_string(),
        total,
        hidden: total.saturating_sub(items.len()),
        empty_message: empty_message.to_string(),
        items,
    }
}

fn impact_markdown_summary(
    subject: &str,
    change_intent: Option<&ChangeIntent>,
    lanes: &[ImpactReviewLane],
) -> String {
    let mut lines = vec![format!("# 변경 영향 검토 — {}", markdown_text(subject))];
    if let Some(intent) = change_intent {
        lines.push(format!(
            "변경: {}",
            markdown_text(&change_intent_summary(intent))
        ));
    }
    for lane in lanes {
        lines.push(String::new());
        lines.push(format!("## {}. {}", lane.order, markdown_text(&lane.title)));
        if lane.items.is_empty() {
            lines.push(format!("- {}", markdown_text(&lane.empty_message)));
        } else {
            for item in &lane.items {
                let marker = item.confidence.as_deref().unwrap_or(&item.truth_class);
                let location = item
                    .location
                    .as_ref()
                    .map(review_location)
                    .unwrap_or_default();
                lines.push(format!(
                    "- [{}] {} — {}{}",
                    markdown_text(marker),
                    markdown_text(&item.title),
                    markdown_text(&item.detail),
                    markdown_text(&location)
                ));
            }
            if lane.hidden > 0 {
                lines.push(format!("- +{}개 접힘", lane.hidden));
            }
        }
    }
    safe_text(&lines.join("\n"))
}

fn change_intent_summary(intent: &ChangeIntent) -> String {
    let value = intent
        .value
        .as_deref()
        .filter(|value| !value.trim().is_empty());
    match intent.kind.as_str() {
        "rename" => value
            .map(|value| format!("이름 변경 → {value}"))
            .unwrap_or_else(|| "이름 변경 · 새 이름 미입력".to_string()),
        "drop" => "컬럼 삭제".to_string(),
        "type" => value
            .map(|value| format!("타입 변경 → {value}"))
            .unwrap_or_else(|| "타입 변경 · 목표 타입 미입력".to_string()),
        "nullability" => match value {
            Some("required") => "NULL 제약 변경 → NOT NULL".to_string(),
            Some("nullable") => "NULL 제약 변경 → NULL 허용".to_string(),
            _ => "NULL 제약 변경 · 방향 미선택".to_string(),
        },
        _ => "변경 종류 미확인".to_string(),
    }
}

fn review_location(location: &SourceLocation) -> String {
    match location.line {
        Some(line) => format!(" · {}:L{line}", location.path),
        None => format!(" · {}", location.path),
    }
}

pub(super) fn direct_object_kind(object: &InventoryItem, evidence: &[Evidence]) -> String {
    if matches!(object.kind.as_str(), "view" | "trigger" | "routine") {
        return object.kind.clone();
    }
    if object.kind == "index" {
        if evidence
            .iter()
            .any(|entry| entry.kind == "db-index-primary" && entry.text == "true")
        {
            return "primary-index".to_string();
        }
        if evidence
            .iter()
            .any(|entry| entry.kind == "db-index-unique" && entry.text == "true")
        {
            return "unique-index".to_string();
        }
        return "index".to_string();
    }
    evidence
        .iter()
        .find(|entry| entry.kind == "db-constraint-kind")
        .map(|entry| entry.text.replace('_', "-"))
        .or_else(|| {
            object
                .engine_label
                .as_deref()
                .and_then(|label| label.strip_prefix("Constraint:"))
                .map(|kind| kind.replace('_', "-"))
        })
        .unwrap_or_else(|| {
            if object.is_primary_key {
                "primary-key".to_string()
            } else if object.is_foreign_key {
                "foreign-key".to_string()
            } else {
                "constraint".to_string()
            }
        })
}

fn direct_kind_label(kind: &str) -> &str {
    match kind {
        "primary-key" => "PK",
        "foreign-key" | "foreign-key-reference" => "FK",
        "unique" => "UNIQUE",
        "check" => "CHECK",
        "primary-index" => "PRIMARY INDEX",
        "unique-index" => "UNIQUE INDEX",
        "index" => "INDEX",
        "view" => "VIEW",
        "trigger" => "TRIGGER",
        "routine" => "ROUTINE",
        _ => "CONSTRAINT",
    }
}

pub(super) fn direct_review_rank(kind: &str) -> u8 {
    match kind {
        "primary-key" => 0,
        "foreign-key" => 1,
        "foreign-key-reference" => 2,
        "unique" => 3,
        "check" => 4,
        "primary-index" => 5,
        "unique-index" => 6,
        "index" => 7,
        "view" => 8,
        "trigger" => 9,
        "routine" => 10,
        _ => 11,
    }
}

fn db_dependent_touches_focus(
    object: &InventoryItem,
    table: &InventoryItem,
    column: Option<&InventoryItem>,
    item_by_id: &HashMap<&str, &InventoryItem>,
    links_by_from: &ImpactLinkIndex<'_>,
    links_by_to: &ImpactLinkIndex<'_>,
) -> bool {
    let outgoing = links_by_from
        .get(object.id.as_str())
        .into_iter()
        .flatten()
        .any(|link| {
            link.kind == "db_dependency"
                && column.map_or_else(
                    || link_endpoint_belongs_to_table(link.to.as_str(), table, item_by_id),
                    |column| link.to == column.id,
                )
        });
    let incoming = column.is_none()
        && links_by_to
            .get(object.id.as_str())
            .into_iter()
            .flatten()
            .any(|link| link.kind == "db_trigger" && link.from == table.id);
    outgoing || incoming
}

fn link_endpoint_belongs_to_table(
    endpoint: &str,
    table: &InventoryItem,
    item_by_id: &HashMap<&str, &InventoryItem>,
) -> bool {
    endpoint == table.id
        || item_by_id
            .get(endpoint)
            .is_some_and(|item| item.parent_id.as_deref() == Some(table.id.as_str()))
}

fn candidate_review_rank(kind: &str) -> u8 {
    match kind {
        "repository" => 0,
        "function" | "method" | "handler" => 1,
        "service" => 2,
        "api" | "route" => 3,
        "file" => 4,
        _ => 5,
    }
}

fn candidate_detail(item: &InventoryItem) -> String {
    match item.kind.as_str() {
        "api" | "route" => "API",
        "repository" => "Repository",
        "function" | "method" | "handler" => "Function",
        "service" => "Service",
        "file" => "File",
        _ => "Code",
    }
    .to_string()
}

fn candidate_check_kind(item: &ImpactReviewItem) -> &'static str {
    let haystack = format!(
        "{} {} {}",
        item.kind,
        item.title,
        item.location
            .as_ref()
            .map(|location| location.path.as_str())
            .unwrap_or_default()
    )
    .to_ascii_lowercase();
    let tokens = haystack
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<HashSet<_>>();
    if ["migration", "migrations", "ddl", "schema"]
        .iter()
        .any(|token| tokens.contains(*token))
    {
        "migration"
    } else if ["test", "tests", "spec"]
        .iter()
        .any(|token| tokens.contains(*token))
    {
        "test"
    } else if matches!(
        item.kind.as_str(),
        "repository" | "query" | "mapper" | "dao"
    ) {
        "data-access"
    } else if matches!(item.kind.as_str(), "api" | "route") {
        "api"
    } else {
        "code"
    }
}

fn check_review_rank(kind: &str) -> u8 {
    match kind {
        "reindex" => 0,
        "change-input" => 1,
        "change-target" => 2,
        "change-data" => 3,
        "change-contract" => 4,
        "constraint" => 5,
        "migration" => 6,
        "migration-missing" => 7,
        "data-access" => 8,
        "code" => 9,
        "api" => 10,
        "test" => 11,
        "test-missing" => 12,
        "coverage" => 13,
        _ => 14,
    }
}

fn check_action_label(kind: &str) -> &str {
    match kind {
        "migration" => "마이그레이션 확인",
        "test" => "회귀 테스트 확인",
        "data-access" => "데이터 접근 확인",
        "api" => "API 경계 확인",
        _ => "코드 확인",
    }
}

fn unknown_review_item(id: String, kind: &str, title: &str, detail: &str) -> ImpactReviewItem {
    ImpactReviewItem {
        id,
        node_id: None,
        kind: kind.to_string(),
        title: title.to_string(),
        detail: safe_text(detail),
        truth_class: "unknown".to_string(),
        confidence: None,
        rank: 0,
        evidence: Vec::new(),
        location: None,
    }
}

fn action_review_item(
    id: &str,
    node_id: Option<String>,
    kind: &str,
    title: &str,
    detail: &str,
    location: Option<SourceLocation>,
    evidence: Vec<Evidence>,
) -> ImpactReviewItem {
    ImpactReviewItem {
        id: id.to_string(),
        node_id,
        kind: kind.to_string(),
        title: title.to_string(),
        detail: safe_text(detail),
        truth_class: "action".to_string(),
        confidence: None,
        rank: 0,
        evidence: safe_evidence(&evidence),
        location,
    }
}

fn markdown_text(value: &str) -> String {
    value
        .replace(['\r', '\n'], " ")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

fn review_truth_class(value: &str) -> String {
    match value {
        "confirmed" => "confirmed",
        "structural" => "structural",
        _ => "unknown",
    }
    .to_string()
}

fn impact_link_touches_focus(
    link: &SnapshotLink,
    table: &InventoryItem,
    column: Option<&InventoryItem>,
    item_by_id: &HashMap<&str, &InventoryItem>,
) -> bool {
    if let Some(column) = column {
        return link.from == column.id || link.to == column.id;
    }
    [link.from.as_str(), link.to.as_str()].iter().any(|id| {
        *id == table.id
            || item_by_id
                .get(*id)
                .is_some_and(|item| item.parent_id.as_deref() == Some(table.id.as_str()))
    })
}

fn related_fk_node_id(
    link: &SnapshotLink,
    table: &InventoryItem,
    item_by_id: &HashMap<&str, &InventoryItem>,
) -> Option<String> {
    [link.from.as_str(), link.to.as_str()]
        .iter()
        .find_map(|id| {
            let item = item_by_id.get(*id)?;
            let parent = item.parent_id.as_deref()?;
            (parent != table.id).then(|| parent.to_string())
        })
        .or_else(|| Some(link.from.clone()))
}

fn review_item_mentions_node(
    item: &ImpactReviewItem,
    node_id: &str,
    links_by_from: &ImpactLinkIndex<'_>,
) -> bool {
    item.node_id.as_deref() == Some(node_id)
        || item.node_id.as_deref().is_some_and(|object_id| {
            links_by_from
                .get(object_id)
                .into_iter()
                .flatten()
                .any(|link| {
                    link.to == node_id && matches!(link.kind.as_str(), "db_constraint" | "db_index")
                })
        })
}

fn impact_link_indexes(snapshot: &InventorySnapshot) -> (ImpactLinkIndex<'_>, ImpactLinkIndex<'_>) {
    let mut by_from = ImpactLinkIndex::new();
    let mut by_to = ImpactLinkIndex::new();
    for link in &snapshot.links {
        by_from.entry(link.from.as_str()).or_default().push(link);
        by_to.entry(link.to.as_str()).or_default().push(link);
    }
    (by_from, by_to)
}

fn api_reachable_code_ids(
    snapshot: &InventorySnapshot,
    links_by_from: &ImpactLinkIndex<'_>,
    max_depth: usize,
    limit: usize,
) -> HashSet<String> {
    let mut visited = snapshot
        .items
        .iter()
        .filter(|item| item.source == "code" && item.layer == "api")
        .map(|item| item.id.clone())
        .collect::<HashSet<_>>();
    let mut queue = visited
        .iter()
        .cloned()
        .map(|id| (id, 0usize))
        .collect::<VecDeque<_>>();
    while let Some((id, depth)) = queue.pop_front() {
        if depth >= max_depth || visited.len() >= limit {
            continue;
        }
        for link in links_by_from
            .get(id.as_str())
            .into_iter()
            .flatten()
            .filter(|link| {
                link.truth_class == "confirmed"
                    && matches!(link.kind.as_str(), "code_handle" | "code_call")
            })
        {
            if visited.insert(link.to.clone()) {
                queue.push_back((link.to.clone(), depth + 1));
            }
            if visited.len() >= limit {
                break;
            }
        }
    }
    visited
}
