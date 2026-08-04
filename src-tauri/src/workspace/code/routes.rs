fn normalize_route_bindings(
    routes: &mut Vec<CodeInventoryItem>,
    handlers: &[CodeInventoryItem],
    handles: &[CodeHandle],
) -> Vec<CodeHandle> {
    let handler_by_id = handlers
        .iter()
        .map(|handler| (handler.id.as_str(), handler))
        .collect::<HashMap<_, _>>();
    let mut handlers_by_route = HashMap::<&str, Vec<&str>>::new();
    for handle in handles {
        handlers_by_route
            .entry(handle.route.as_str())
            .or_default()
            .push(handle.handler.as_str());
    }
    for handler_ids in handlers_by_route.values_mut() {
        handler_ids.sort_unstable();
        handler_ids.dedup();
    }

    let mut normalized_routes = Vec::with_capacity(routes.len().max(handles.len()));
    let mut normalized_handles = Vec::with_capacity(handles.len());
    for mut route in std::mem::take(routes) {
        let Some(handler_ids) = handlers_by_route.get(route.id.as_str()) else {
            normalized_routes.push(route);
            continue;
        };

        if handler_ids.len() == 1 {
            let handler_id = handler_ids[0];
            hydrate_route_binding(&mut route, handler_by_id.get(handler_id).copied(), false);
            normalized_handles.push(CodeHandle {
                handler: handler_id.to_string(),
                route: route.id.clone(),
            });
            normalized_routes.push(route);
            continue;
        }

        for handler_id in handler_ids {
            let mut binding = route.clone();
            binding.id = route_binding_id(&route.id, handler_id);
            binding.qualified_name = binding.id.clone();
            hydrate_route_binding(&mut binding, handler_by_id.get(handler_id).copied(), true);
            normalized_handles.push(CodeHandle {
                handler: (*handler_id).to_string(),
                route: binding.id.clone(),
            });
            normalized_routes.push(binding);
        }
    }
    normalized_routes.sort_by(|left, right| left.id.cmp(&right.id));
    normalized_handles.sort_by(|left, right| {
        left.route
            .cmp(&right.route)
            .then_with(|| left.handler.cmp(&right.handler))
    });
    *routes = normalized_routes;
    normalized_handles
}

fn hydrate_route_binding(
    route: &mut CodeInventoryItem,
    handler: Option<&CodeInventoryItem>,
    prefer_handler_location: bool,
) {
    let Some(handler) = handler else {
        return;
    };
    if let Some(path) = object_string(&handler.detail, &["routePath", "route_path"])
        .filter(|value| !value.trim().is_empty())
    {
        route.name = path;
    }
    if prefer_handler_location || route.file_path.is_none() {
        route.file_path = handler.file_path.clone();
        route.line = handler.line;
        route.column = handler.column;
        route.end_line = handler.end_line;
        route.end_column = handler.end_column;
    }
}

pub(crate) fn route_binding_id(route_id: &str, handler_id: &str) -> String {
    format!("{route_id}#handler={handler_id}")
}

pub(crate) fn downgrade_unverified_routes(inventory: &mut CodeInventory) {
    let handled_routes = inventory
        .handles
        .iter()
        .map(|handle| handle.route.as_str())
        .collect::<HashSet<_>>();
    let (routes, mut unverified): (Vec<_>, Vec<_>) = std::mem::take(&mut inventory.routes)
        .into_iter()
        .partition(|route| route.file_path.is_some() || handled_routes.contains(route.id.as_str()));

    for route in &mut unverified {
        route.kind = "unknown".to_string();
    }
    inventory.routes = routes;
    inventory.unknown.append(&mut unverified);
    inventory
        .unknown
        .sort_by(|left, right| left.id.cmp(&right.id));
    inventory.summary = code_inventory_summary(inventory);
}

fn positive_json_line(value: &serde_json::Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|line| line.parse().ok()))
        .filter(|line| *line > 0)
}

fn move_confirmed_handlers(
    source: &mut Vec<CodeInventoryItem>,
    handlers: &mut Vec<CodeInventoryItem>,
    handler_ids: &HashSet<&str>,
) {
    let mut remaining = Vec::with_capacity(source.len());
    for item in std::mem::take(source) {
        if handler_ids.contains(item.id.as_str()) {
            handlers.push(item);
        } else {
            remaining.push(item);
        }
    }
    *source = remaining;
}
