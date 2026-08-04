fn find_caller(inventory: &CodeInventory, path: &str, line: u64) -> Option<String> {
    let items = inventory
        .routes
        .iter()
        .chain(&inventory.handlers)
        .chain(&inventory.services)
        .chain(&inventory.repositories)
        .chain(&inventory.functions)
        .chain(&inventory.classes)
        .chain(&inventory.modules)
        .chain(&inventory.unknown)
        .chain(&inventory.files);
    let mut candidates = items
        .filter(|item| {
            item.file_path
                .as_deref()
                .is_some_and(|file| normalize_path(file) == normalize_path(path))
                && item
                    .line
                    .is_some_and(|start| start <= line && item.end_line.unwrap_or(start) >= line)
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|item| {
        (
            (item.end_line.unwrap_or(item.line.unwrap_or(line)) - item.line.unwrap_or(line)),
            item_priority(item),
        )
    });
    candidates.first().map(|item| item.id.clone())
}

fn item_priority(item: &CodeInventoryItem) -> u8 {
    match item.kind.to_ascii_lowercase().as_str() {
        "handler" => 0,
        "function" | "method" => 1,
        "service" | "repository" => 2,
        "class" => 3,
        "route" => 4,
        "file" => 9,
        _ => 5,
    }
}

fn normalize_path(value: &str) -> String {
    value
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_ascii_lowercase()
}

fn request_id(path: &str, line: usize, request: &RawRequest) -> Option<String> {
    if request.url_expression.trim().is_empty() {
        return None;
    }
    let mut digest = Sha256::new();
    digest.update(path.as_bytes());
    digest.update([0]);
    digest.update(line.to_string().as_bytes());
    digest.update([0]);
    digest.update(request.client.as_bytes());
    digest.update([0]);
    digest.update(request.method.as_deref().unwrap_or("ANY").as_bytes());
    digest.update([0]);
    digest.update(request.url_expression.trim().as_bytes());
    let digest = format!("{:x}", digest.finalize());
    Some(format!("client-request:{path}:{line}:{}", &digest[..12]))
}
