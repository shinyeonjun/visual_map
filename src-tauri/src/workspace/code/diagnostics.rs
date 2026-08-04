fn architecture_diagnostics(
    architecture: Option<&serde_json::Value>,
    project: &str,
) -> Vec<CodeInventoryGap> {
    architecture
        .and_then(|value| value.get("diagnostics"))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter(|diagnostic| is_coverage_diagnostic(diagnostic))
        .map(|diagnostic| {
            let message = diagnostic
                .get("message")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| diagnostic.to_string());
            let code = diagnostic
                .get("code")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown");
            let language = diagnostic
                .get("language")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .or_else(|| diagnostic_language(&message))
                .unwrap_or("project");
            let mut gap = CodeInventoryGap::new(
                code,
                format!("provider:{language}"),
                project,
                message,
            );
            gap.detail = diagnostic
                .get("detail")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            gap
        })
        .collect()
}

fn is_coverage_diagnostic(diagnostic: &serde_json::Value) -> bool {
    if let Some(code) = diagnostic.get("code").and_then(serde_json::Value::as_str) {
        let level = diagnostic
            .get("level")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        return level.eq_ignore_ascii_case("error")
            || matches!(
                code,
                "provider-missing"
                    | "provider-failed"
                    | "indexer-failed"
                    | "invalid-output"
                    | "empty-semantic"
                    | "missing-dependency-metadata"
                    | "missing-dependency"
                    | "missing-compile-context"
                    | "missing-external-tool"
                    | "missing-legacy-sdk"
                    | "provider-timeout"
                    | "provider-stopped"
                    | "partial-coverage"
                    | "large-workspace-partial"
                    | "java-source-fallback"
                    | "java-source-fallback-failed"
                    | "typescript-source-fallback"
                    | "workspace-too-large"
                    | "generated-code"
                    | "test-only"
                    | "unsupported-framework"
                    | "dynamic-registration"
                    | "stale-index"
                    | "snapshot-incompatible"
                    | "display-limit"
            );
    }

    // Legacy architecture payloads predate DiagnosticCode. Keep their old
    // shape readable, but never use human wording to classify a modern result.
    let kind = diagnostic
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let status = diagnostic
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let level = diagnostic
        .get("level")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let message = diagnostic
        .get("message")
        .or_else(|| diagnostic.get("detail"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();

    kind == "missing"
        || level == "error"
        || status.contains("missing")
        || status.contains("fail")
        || message.contains("provider-missing")
        || message.contains("providers root is not configured")
        || message.contains("needs native lsp")
        || message.contains("needs scip-")
        || message.contains("project model unavailable")
}

fn diagnostic_language(message: &str) -> Option<&str> {
    let (language, _) = message.split_once(':')?;
    let language = language.trim();
    (!language.is_empty() && !language.contains(char::is_whitespace)).then_some(language)
}

pub(crate) fn next_code_project_generation() -> String {
    let sequence = NEXT_CODE_PROJECT_GENERATION.fetch_add(1, Ordering::Relaxed);
    format!(
        "visual-map-{}-{}-{}",
        std::process::id(),
        timestamp(),
        sequence
    )
}

pub(crate) fn focused_code_search_with_operation(
    app_data_dir: impl AsRef<Path>,
    registry: &EngineRegistry,
    workspace_id: &str,
    identifier: &str,
    path_filter: Option<&str>,
    requested_limit: usize,
    operation_id: Option<&str>,
) -> Result<FocusedCodeSearch, String> {
    validate_workspace_id(workspace_id)?;
    let paths = base_paths(app_data_dir);
    let workspace = read_workspace_by_id(&paths.workspaces_dir, workspace_id)?;
    let code_cache_path = workspace_code_cache_path(&paths.workspaces_dir, workspace_id);
    let project = workspace
        .code_project
        .as_deref()
        .unwrap_or(workspace.name.as_str());
    CodebaseMemoryAdapter::new_with_provider_cache(
        registry,
        code_cache_path,
        paths.app_data_dir.join("providers"),
    )?
    .search_code_with_operation(
        project,
        identifier,
        path_filter,
        requested_limit,
        operation_id,
    )
}

pub(crate) fn split_inventory_nodes(
    nodes: &serde_json::Value,
) -> Result<(serde_json::Value, serde_json::Value, serde_json::Value), String> {
    let items = nodes
        .get("results")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "정규화된 코드 노드 응답에 results가 없습니다".to_string())?;
    let mut routes = Vec::new();
    let mut code = Vec::new();
    let mut files = Vec::new();

    for item in items {
        let label = object_string(item, &["label"])
            .ok_or_else(|| "정규화된 코드 노드에 label이 없습니다".to_string())?;
        match label.as_str() {
            "Route" => routes.push(item.clone()),
            "File" => files.push(item.clone()),
            label if CODE_NODE_LABELS.contains(&label) => code.push(item.clone()),
            _ => return Err(format!("허용되지 않은 코드 노드 label입니다: {label}")),
        }
    }

    Ok((
        serde_json::json!({ "total": routes.len(), "results": routes, "has_more": false }),
        serde_json::json!({ "total": code.len(), "results": code, "has_more": false }),
        serde_json::json!({ "total": files.len(), "results": files, "has_more": false }),
    ))
}

pub(crate) fn extract_code_inventory(
    project: String,
    architecture: Option<serde_json::Value>,
    routes_json: &serde_json::Value,
    services_json: &serde_json::Value,
    files_json: &serde_json::Value,
) -> Result<CodeInventory, String> {
    let mut seen = HashMap::new();
    let routes = extract_labeled_items(&project, routes_json, &["Route"], &mut seen)?
        .into_iter()
        .filter(|item| !code_item_is_test(item))
        .collect::<Vec<_>>();
    let code_items = extract_labeled_items(&project, services_json, CODE_NODE_LABELS, &mut seen)?;
    let files = extract_labeled_items(&project, files_json, &["File"], &mut seen)?;
    let handlers = category_items(&code_items, "handler");
    let normalized_services = category_items(&code_items, "service");
    let repositories = category_items(&code_items, "repository");
    let functions = category_items(&code_items, "function");
    let classes = category_items(&code_items, "class");
    let modules = category_items(&code_items, "module");
    let unknown = code_items
        .iter()
        .filter(|item| code_category(item) == "code")
        .cloned()
        .collect::<Vec<_>>();
    let summary = CodeInventorySummary {
        routes: routes.iter().filter(|item| !code_item_is_ui(item)).count(),
        handlers: handlers.len(),
        services: normalized_services.len(),
        repositories: repositories.len(),
        functions: functions.len(),
        classes: classes.len(),
        modules: modules.len(),
        files: files.len(),
        unknown: unknown.len(),
    };

    Ok(CodeInventory {
        project,
        routes,
        services: normalized_services,
        files,
        handlers,
        repositories,
        functions,
        classes,
        modules,
        unknown,
        summary,
        architecture,
        evidence: None,
        calls: Vec::new(),
        handles: Vec::new(),
        relation_gaps: Vec::new(),
        client_requests: Vec::new(),
        partial: false,
    })
}

#[cfg(test)]
pub(crate) fn extract_code_calls(
    calls_json: &serde_json::Value,
    inventory: &CodeInventory,
) -> Vec<CodeCall> {
    extract_code_calls_with_gaps(calls_json, inventory).0
}

pub(super) fn extract_code_calls_with_gaps(
    calls_json: &serde_json::Value,
    inventory: &CodeInventory,
) -> (Vec<CodeCall>, Vec<CodeInventoryGap>) {
    let known_items = inventory
        .routes
        .iter()
        .chain(inventory.handlers.iter())
        .chain(inventory.services.iter())
        .chain(inventory.repositories.iter())
        .chain(inventory.functions.iter())
        .chain(inventory.classes.iter())
        .chain(inventory.modules.iter())
        .chain(inventory.unknown.iter())
        .chain(inventory.files.iter())
        .map(|item| (item.id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let test_ids = known_items
        .values()
        .filter(|item| code_item_is_test(item))
        .map(|item| item.id.as_str())
        .collect::<HashSet<_>>();
    let mut calls_by_pair = HashMap::<(String, String), CodeCall>::new();
    let mut gaps = Vec::new();
    let mut seen_gaps = HashSet::new();
    for call in graph_rows(calls_json).into_iter().filter_map(code_call) {
        let known_from = known_items.contains_key(call.from.as_str());
        let known_to = known_items.contains_key(call.to.as_str());
        if !known_from || !known_to {
            let key = (call.from.clone(), call.to.clone());
            if seen_gaps.insert(key) {
                gaps.push(CodeInventoryGap::new(
                    "unresolved-call",
                    call.from.clone(),
                    call.to.clone(),
                    "codebase-memory CALLS 관계의 한쪽 또는 양쪽 끝점을 제품 인벤토리에서 찾지 못했습니다.",
                ));
            }
            continue;
        }
        if !test_ids.contains(call.from.as_str()) && test_ids.contains(call.to.as_str()) {
            continue;
        }
        let key = (call.from.clone(), call.to.clone());
        match calls_by_pair.entry(key) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(call);
            }
            std::collections::hash_map::Entry::Occupied(mut entry)
                if code_call_rank(&call) > code_call_rank(entry.get()) =>
            {
                entry.insert(call);
            }
            _ => {}
        }
    }
    let mut calls = calls_by_pair.into_values().collect::<Vec<_>>();
    calls.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| a.to.cmp(&b.to)));
    gaps.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| a.to.cmp(&b.to)));
    (calls, gaps)
}
