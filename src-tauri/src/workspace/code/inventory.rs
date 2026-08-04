fn code_item_is_test(item: &CodeInventoryItem) -> bool {
    object_bool(&item.detail, &["is_test", "isTest"])
        || item.file_path.as_deref().is_some_and(is_test_file_path)
}

fn is_test_file_path(path: &str) -> bool {
    if path.split(['/', '\\']).any(|segment| {
        let segment = segment.to_ascii_lowercase();
        matches!(segment.as_str(), "test" | "tests" | "__tests__")
            || segment.ends_with(".tests")
            || segment.ends_with(".unittests")
            || segment.ends_with(".integrationtests")
    }) {
        return true;
    }

    let file_name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let lower = file_name.to_ascii_lowercase();
    let stem = lower
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(&lower);
    let original_stem = file_name
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(file_name);
    stem == "test"
        || stem == "tests"
        || stem.starts_with("test_")
        || stem.starts_with("test-")
        || stem.ends_with("_test")
        || stem.ends_with("_tests")
        || stem.ends_with(".test")
        || stem.ends_with(".tests")
        || stem.ends_with(".spec")
        || stem.ends_with(".specs")
        || stem.ends_with("_spec")
        || stem.ends_with("_specs")
        || original_stem.ends_with("Test")
        || original_stem.ends_with("Tests")
        || original_stem.ends_with("Spec")
        || original_stem.ends_with("Specs")
}

#[cfg(test)]
pub(crate) fn extract_code_handles(
    handles_json: &serde_json::Value,
    inventory: &CodeInventory,
) -> Vec<CodeHandle> {
    extract_code_handles_with_gaps(handles_json, inventory).0
}

fn extract_code_handles_with_gaps(
    handles_json: &serde_json::Value,
    inventory: &CodeInventory,
) -> (Vec<CodeHandle>, Vec<CodeInventoryGap>) {
    let route_ids = inventory
        .routes
        .iter()
        .map(|item| item.id.as_str())
        .collect::<HashSet<_>>();
    let handler_ids = inventory
        .handlers
        .iter()
        .chain(inventory.services.iter())
        .chain(inventory.repositories.iter())
        .chain(inventory.functions.iter())
        .chain(inventory.classes.iter())
        .chain(inventory.modules.iter())
        .chain(inventory.unknown.iter())
        .map(|item| item.id.as_str())
        .collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let mut gaps = Vec::new();
    let mut seen_gaps = HashSet::new();
    let mut handles = graph_rows(handles_json)
        .into_iter()
        .filter_map(code_call)
        .filter_map(|edge| {
            let known_handler = handler_ids.contains(edge.from.as_str());
            let known_route = route_ids.contains(edge.to.as_str());
            if !known_handler || !known_route {
                let key = (edge.from.clone(), edge.to.clone());
                if seen_gaps.insert(key) {
                    gaps.push(CodeInventoryGap::new(
                        "unresolved-handle",
                        edge.from,
                        edge.to,
                        "codebase-memory HANDLES 관계의 handler 또는 route를 제품 인벤토리에서 찾지 못했습니다.",
                    ));
                }
                return None;
            }
            Some(CodeHandle {
                handler: edge.from,
                route: edge.to,
            })
        })
        .filter(|handle| seen.insert((handle.route.clone(), handle.handler.clone())))
        .collect::<Vec<_>>();
    handles.sort_by(|a, b| {
        a.route
            .cmp(&b.route)
            .then_with(|| a.handler.cmp(&b.handler))
    });
    gaps.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| a.to.cmp(&b.to)));
    (handles, gaps)
}

pub(crate) fn attach_code_handles(handles_json: &serde_json::Value, inventory: &mut CodeInventory) {
    let (handles, gaps) = extract_code_handles_with_gaps(handles_json, inventory);
    inventory.relation_gaps.extend(gaps);
    attach_route_handles(handles, inventory);
}

pub(super) fn attach_route_handles(mut handles: Vec<CodeHandle>, inventory: &mut CodeInventory) {
    handles.sort_by(|left, right| {
        left.route
            .cmp(&right.route)
            .then_with(|| left.handler.cmp(&right.handler))
    });
    handles.dedup();
    let handler_ids = handles
        .iter()
        .map(|handle| handle.handler.as_str())
        .collect::<HashSet<_>>();

    move_confirmed_handlers(
        &mut inventory.services,
        &mut inventory.handlers,
        &handler_ids,
    );
    move_confirmed_handlers(
        &mut inventory.repositories,
        &mut inventory.handlers,
        &handler_ids,
    );
    move_confirmed_handlers(
        &mut inventory.functions,
        &mut inventory.handlers,
        &handler_ids,
    );
    move_confirmed_handlers(
        &mut inventory.classes,
        &mut inventory.handlers,
        &handler_ids,
    );
    move_confirmed_handlers(
        &mut inventory.modules,
        &mut inventory.handlers,
        &handler_ids,
    );
    move_confirmed_handlers(
        &mut inventory.unknown,
        &mut inventory.handlers,
        &handler_ids,
    );
    inventory.handlers.sort_by(|a, b| a.id.cmp(&b.id));
    let handles = normalize_route_bindings(&mut inventory.routes, &inventory.handlers, &handles);
    let handled_routes = handles
        .iter()
        .map(|handle| handle.route.as_str())
        .collect::<HashSet<_>>();
    inventory.routes.sort_by(|left, right| {
        (!handled_routes.contains(left.id.as_str()), left.id.as_str()).cmp(&(
            !handled_routes.contains(right.id.as_str()),
            right.id.as_str(),
        ))
    });
    inventory.handles = handles;
    inventory.summary = code_inventory_summary(inventory);
}
