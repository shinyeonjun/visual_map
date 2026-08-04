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

