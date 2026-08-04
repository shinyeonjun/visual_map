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

