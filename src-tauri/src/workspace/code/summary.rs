fn code_inventory_summary(inventory: &CodeInventory) -> CodeInventorySummary {
    CodeInventorySummary {
        routes: inventory
            .routes
            .iter()
            .filter(|item| !code_item_is_ui(item))
            .count(),
        handlers: inventory.handlers.len(),
        services: inventory.services.len(),
        repositories: inventory.repositories.len(),
        functions: inventory.functions.len(),
        classes: inventory.classes.len(),
        modules: inventory.modules.len(),
        files: inventory.files.len(),
        unknown: inventory.unknown.len(),
    }
}

fn graph_rows(value: &serde_json::Value) -> Vec<&serde_json::Value> {
    if let Some(items) = value.as_array() {
        return items.iter().collect();
    }
    for key in ["items", "results", "rows", "data", "records"] {
        if let Some(items) = value.get(key).and_then(serde_json::Value::as_array) {
            return items.iter().collect();
        }
    }

    Vec::new()
}

fn code_call(value: &serde_json::Value) -> Option<CodeCall> {
    if let Some(items) = value.as_array() {
        let from = items.first().and_then(serde_json::Value::as_str)?;
        let to = items.get(1).and_then(serde_json::Value::as_str)?;
        return Some(CodeCall {
            from: from.to_string(),
            to: to.to_string(),
            confidence: items.get(2).and_then(code_call_confidence),
            strategy: items.get(3).and_then(optional_json_string),
            expression: items.get(4).and_then(optional_json_string),
            path: items.get(5).and_then(optional_json_string),
            range: items.get(6).and_then(code_call_range).unwrap_or_default(),
        });
    }

    let from = endpoint_string(
        value,
        &[
            "from",
            "caller",
            "source",
            "sourceQualifiedName",
            "source_qualified_name",
            "caller.qualified_name",
            "caller.qualifiedName",
            "a.qualified_name",
            "a.qualifiedName",
        ],
    )?;
    let to = endpoint_string(
        value,
        &[
            "to",
            "callee",
            "target",
            "targetQualifiedName",
            "target_qualified_name",
            "callee.qualified_name",
            "callee.qualifiedName",
            "b.qualified_name",
            "b.qualifiedName",
        ],
    )?;

    Some(CodeCall {
        from,
        to,
        confidence: value.get("confidence").and_then(code_call_confidence),
        strategy: value.get("strategy").and_then(optional_json_string),
        expression: value
            .get("call_expression")
            .or_else(|| value.get("callExpression"))
            .and_then(optional_json_string),
        path: value
            .get("path")
            .or_else(|| value.get("file_path"))
            .or_else(|| value.get("filePath"))
            .and_then(optional_json_string),
        range: value
            .get("range")
            .and_then(code_call_range)
            .unwrap_or_default(),
    })
}

fn code_call_rank(call: &CodeCall) -> (u8, bool, bool, &str, &str) {
    (
        call.confidence.unwrap_or(0),
        call.path.is_some(),
        !call.range.is_empty(),
        call.strategy.as_deref().unwrap_or_default(),
        call.expression.as_deref().unwrap_or_default(),
    )
}

fn code_call_confidence(value: &serde_json::Value) -> Option<u8> {
    let value = value
        .as_f64()
        .or_else(|| value.as_str()?.parse::<f64>().ok())?;
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    let percent = if value <= 1.0 { value * 100.0 } else { value };
    (percent <= 100.0).then(|| percent.round() as u8)
}

fn optional_json_string(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn code_call_range(value: &serde_json::Value) -> Option<Vec<i32>> {
    value
        .as_array()?
        .iter()
        .map(|item| i32::try_from(item.as_i64()?).ok())
        .collect()
}

fn endpoint_string(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        let candidate = value.get(key)?;
        candidate.as_str().map(str::to_string).or_else(|| {
            object_string(
                candidate,
                &["qualifiedName", "qualified_name", "id", "name"],
            )
        })
    })
}
