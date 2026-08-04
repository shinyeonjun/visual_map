fn extract_labeled_items(
    project: &str,
    value: &serde_json::Value,
    allowed_labels: &[&str],
    seen: &mut HashMap<String, String>,
) -> Result<Vec<CodeInventoryItem>, String> {
    let mut items = Vec::new();
    for value in value_items(value) {
        let item = code_item(project, value)?;
        if !allowed_labels.contains(&item.engine_label.as_str()) {
            return Err(format!(
                "코드 엔진 label 계약이 일치하지 않습니다: expected {}, got {} ({})",
                allowed_labels.join("|"),
                item.engine_label,
                item.qualified_name
            ));
        }
        if is_obvious_inventory_noise(&item) {
            continue;
        }

        match seen.get(&item.qualified_name) {
            Some(label) if label == &item.engine_label => continue,
            Some(label) => {
                return Err(format!(
                    "동일 qualified name에 서로 다른 label이 있습니다: {} ({label}, {})",
                    item.qualified_name, item.engine_label
                ));
            }
            None => {
                seen.insert(item.qualified_name.clone(), item.engine_label.clone());
                items.push(item);
            }
        }
    }
    items.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(items)
}

fn is_obvious_inventory_noise(item: &CodeInventoryItem) -> bool {
    (item.engine_label == "Route" && item.name.contains("://"))
        || (item.engine_label == "Decorator"
            && item.name.starts_with("#[")
            && !item.name.ends_with(']'))
}

fn code_item(project: &str, value: &serde_json::Value) -> Result<CodeInventoryItem, String> {
    let qualified_name = object_string(value, &["qualifiedName", "qualified_name", "id"])
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "코드 엔진 노드에 qualified_name이 없습니다".to_string())?;
    let engine_label = object_string(value, &["label", "kind"])
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("코드 엔진 노드에 label이 없습니다: {qualified_name}"))?;
    let name = object_string(value, &["name"])
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| qualified_name.clone());
    let file_path = object_string(value, &["filePath", "file_path", "path"])
        .filter(|value| !value.trim().is_empty());

    let mut detail = value.clone();
    if let (Some(detail), Some(properties)) = (
        detail.as_object_mut(),
        value
            .get("properties")
            .and_then(serde_json::Value::as_object),
    ) {
        for (key, property) in properties {
            detail
                .entry(key.clone())
                .or_insert_with(|| property.clone());
        }
    }

    Ok(CodeInventoryItem {
        id: qualified_name.clone(),
        kind: engine_label.clone(),
        name,
        project: project.to_string(),
        qualified_name,
        engine_label,
        file_path,
        line: positive_line(value, &["startLine", "start_line", "line"]),
        column: positive_line(value, &["startColumn", "start_column", "column"]),
        end_line: positive_line(value, &["endLine", "end_line"]),
        end_column: positive_line(value, &["endColumn", "end_column"]),
        detail,
    })
}

fn code_item_is_ui(item: &CodeInventoryItem) -> bool {
    object_string(&item.detail, &["routeSurface", "route_surface"])
        .is_some_and(|surface| surface == "ui-navigation")
}

fn positive_line(value: &serde_json::Value, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| value.get(key).and_then(positive_json_line))
}

fn category_items(items: &[CodeInventoryItem], category: &str) -> Vec<CodeInventoryItem> {
    items
        .iter()
        .filter(|item| code_category(item) == category)
        .cloned()
        .collect()
}

fn code_category(item: &CodeInventoryItem) -> &'static str {
    let kind = item.engine_label.to_ascii_lowercase();
    if matches!(
        kind.as_str(),
        "function" | "method" | "constructor" | "subroutine" | "procedure"
    ) {
        "function"
    } else if matches!(
        kind.as_str(),
        "class"
            | "struct"
            | "interface"
            | "trait"
            | "protocol"
            | "record"
            | "enum"
            | "type"
            | "union"
    ) {
        class_role(&item.name).unwrap_or("class")
    } else if matches!(kind.as_str(), "module" | "package" | "namespace") {
        "module"
    } else {
        "code"
    }
}

fn class_role(name: &str) -> Option<&'static str> {
    let compact = name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase();
    let role_name = compact.strip_suffix("impl").unwrap_or(&compact);

    if role_name.ends_with("handler") || role_name.ends_with("controller") {
        Some("handler")
    } else if role_name.ends_with("repository")
        || role_name.ends_with("repo")
        || role_name.ends_with("dao")
    {
        Some("repository")
    } else if role_name.ends_with("service") {
        Some("service")
    } else {
        None
    }
}

pub(super) fn assign_structural_roles(inventory: &mut CodeInventory) {
    let handler_ids = inventory
        .handles
        .iter()
        .map(|handle| handle.handler.clone())
        .collect::<HashSet<_>>();
    for item in all_code_items_mut(inventory) {
        if handler_ids.contains(&item.id) {
            set_inferred_role(item, "handler", "structural-handles");
        }
    }

    let mut db_ids = HashSet::new();
    for item in all_code_items_mut(inventory) {
        if item.engine_label.eq_ignore_ascii_case("repository")
            || item.detail.to_string().to_ascii_lowercase().contains("sql")
            || item.detail.to_string().to_ascii_lowercase().contains("db:")
        {
            db_ids.insert(item.id.clone());
            if item.detail.get("role").is_none() {
                set_inferred_role(item, "repository", "structural-db");
            }
        }
    }

    let mut outgoing = HashMap::<String, Vec<String>>::new();
    let mut incoming = HashMap::<String, Vec<String>>::new();
    for call in &inventory.calls {
        outgoing
            .entry(call.from.clone())
            .or_default()
            .push(call.to.clone());
        incoming
            .entry(call.to.clone())
            .or_default()
            .push(call.from.clone());
    }
    let can_reach_db = walk_ids(db_ids.iter().cloned(), &incoming);
    let reachable_from_handlers = walk_ids(handler_ids.iter().cloned(), &outgoing);
    let service_ids = reachable_from_handlers
        .intersection(&can_reach_db)
        .filter(|id| !db_ids.contains(*id) && !handler_ids.contains(*id))
        .cloned()
        .collect::<HashSet<_>>();
    for item in all_code_items_mut(inventory) {
        if service_ids.contains(&item.id) {
            set_inferred_role(item, "service", "structural-reachability");
        } else if item.detail.get("role").is_none() {
            if let Some(role) = class_role(&item.name) {
                set_inferred_role(item, role, "name-inferred");
            }
        }
    }
}

fn all_code_items_mut(inventory: &mut CodeInventory) -> Vec<&mut CodeInventoryItem> {
    let mut items = Vec::new();
    for group in [
        &mut inventory.routes,
        &mut inventory.services,
        &mut inventory.files,
        &mut inventory.handlers,
        &mut inventory.repositories,
        &mut inventory.functions,
        &mut inventory.classes,
        &mut inventory.modules,
        &mut inventory.unknown,
    ] {
        items.extend(group.iter_mut());
    }
    items
}

fn walk_ids<I>(starts: I, edges: &HashMap<String, Vec<String>>) -> HashSet<String>
where
    I: IntoIterator<Item = String>,
{
    let mut seen = HashSet::new();
    let mut queue = starts.into_iter().collect::<std::collections::VecDeque<_>>();
    while let Some(current) = queue.pop_front() {
        if !seen.insert(current.clone()) {
            continue;
        }
        for next in edges.get(&current).into_iter().flatten() {
            queue.push_back(next.clone());
        }
    }
    seen
}

fn set_inferred_role(item: &mut CodeInventoryItem, role: &str, basis: &str) {
    let Some(detail) = item.detail.as_object_mut() else {
        item.detail = serde_json::json!({});
        return set_inferred_role(item, role, basis);
    };
    detail.insert("role".to_string(), serde_json::Value::String(role.to_string()));
    detail.insert(
        "roleBasis".to_string(),
        serde_json::Value::String(basis.to_string()),
    );
}

pub(crate) fn code_project_from_index_stdout(stdout: &str, fallback: &str) -> String {
    engine_json_value(stdout)
        .and_then(|value| object_string(&value, &["project"]))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| fallback.to_string())
}
