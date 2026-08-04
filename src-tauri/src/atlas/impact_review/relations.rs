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
        "code_db_write" => 0,
        "code_db_read" => 1,
        "code_db_uses_column" => 2,
        "primary-key" => 3,
        "foreign-key" => 4,
        "foreign-key-reference" => 5,
        "unique" => 6,
        "check" => 7,
        "primary-index" => 8,
        "unique-index" => 9,
        "index" => 10,
        "view" => 11,
        "trigger" => 12,
        "routine" => 13,
        _ => 14,
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
        .filter(|item| item.is_code() && item.layer == "api")
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
                link.is_confirmed() && matches!(link.kind.as_str(), "code_handle" | "code_call")
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

