fn api_route_identity(route: &InventoryItem) -> (Option<String>, String) {
    let encoded_method = route
        .qualified_name
        .as_deref()
        .and_then(route_method_from_identity)
        .or_else(|| route_method_from_identity(&route.id));
    let named_method = route.name.split_once(' ').and_then(|(method, path)| {
        (path.starts_with('/')
            && !method.is_empty()
            && method
                .chars()
                .all(|character| character.is_ascii_alphabetic()))
        .then(|| method.to_ascii_uppercase())
    });
    let method = encoded_method.or(named_method);
    let subject = method
        .as_deref()
        .and_then(|method| route.name.strip_prefix(method))
        .and_then(|rest| rest.strip_prefix(' '))
        .filter(|rest| rest.starts_with('/'))
        .unwrap_or(&route.name)
        .to_string();

    (method, subject)
}

fn route_method_from_identity(identity: &str) -> Option<String> {
    let marker_start = identity.to_ascii_lowercase().find("__route__")? + "__route__".len();
    let method = identity.get(marker_start..)?.split_once("__")?.0;
    (!method.is_empty()
        && method
            .chars()
            .all(|character| character.is_ascii_alphabetic()))
    .then(|| method.to_ascii_uppercase())
}

fn db_relation_items(
    links: &[&SnapshotLink],
    item_by_id: &HashMap<&str, &InventoryItem>,
) -> Vec<ImpactReviewItem> {
    let mut items = Vec::<ImpactReviewItem>::new();
    let mut target_indexes = HashMap::<&str, usize>::new();
    for link in links {
        let Some(source) = item_by_id.get(link.from.as_str()).copied() else {
            continue;
        };
        let Some(target) = item_by_id.get(link.to.as_str()).copied() else {
            continue;
        };
        let operation = if link.kind == "code_db_read" {
            "조회"
        } else {
            "변경"
        };
        if let Some(index) = target_indexes.get(target.id.as_str()).copied() {
            items[index].evidence.extend(safe_evidence(&link.evidence));
            if !items[index].detail.contains(operation) {
                items[index].detail.push_str(&format!(" · {operation}"));
            }
            continue;
        }
        target_indexes.insert(target.id.as_str(), items.len());
        items.push(ImpactReviewItem {
            id: format!("api-db-relation:{}", target.id),
            node_id: Some(target.id.clone()),
            kind: link.kind.clone(),
            title: target.name.clone(),
            detail: safe_text(&format!(
                "{} 코드의 실행 가능한 정적 SQL이 이 테이블을 {operation}합니다.",
                source.name
            )),
            truth_class: "confirmed".to_string(),
            confidence: None,
            rank: 0,
            evidence: safe_evidence(&link.evidence),
            location: source.location.clone(),
        });
    }
    for item in &mut items {
        let mut seen = HashSet::new();
        item.evidence
            .retain(|entry| seen.insert((entry.kind.clone(), entry.text.clone())));
    }
    items
}

fn api_reading_step(
    item: &InventoryItem,
    incoming: Option<&SnapshotLink>,
    depth: usize,
    rank: usize,
    item_by_id: &HashMap<&str, &InventoryItem>,
    supplemental_evidence: &[Evidence],
) -> ApiReadingStep {
    let (lane, lane_basis) = api_reading_lane(item, incoming);
    let incoming_evidence = incoming
        .map(|link| confirmed_link_edge(link, item_by_id).evidence)
        .unwrap_or_default();
    let mut evidence = if incoming_evidence.is_empty() {
        vec![Evidence {
            kind: "engine-node".to_string(),
            text: "코드 엔진 inventory에서 Route 항목을 읽었습니다.".to_string(),
        }]
    } else {
        incoming_evidence.clone()
    };
    evidence.extend(safe_evidence(supplemental_evidence));
    ApiReadingStep {
        item: ImpactReviewItem {
            id: format!("api-step:{}", item.id),
            node_id: Some(item.id.clone()),
            kind: item.kind.clone(),
            title: item.name.clone(),
            detail: api_item_detail(item),
            truth_class: if incoming.is_some() {
                "confirmed".to_string()
            } else {
                "structural".to_string()
            },
            confidence: None,
            rank,
            evidence,
            location: item.location.clone(),
        },
        depth,
        lane: lane.to_string(),
        lane_basis: lane_basis.to_string(),
        incoming_evidence,
    }
}

fn api_reading_lane(
    item: &InventoryItem,
    incoming: Option<&SnapshotLink>,
) -> (&'static str, &'static str) {
    if incoming.is_none() || item.layer == "api" {
        return ("route", "engine-node");
    }
    if incoming.is_some_and(|link| link.kind == "code_handle") {
        return ("handler", "confirmed-handles");
    }
    let identity = format!(
        "{} {} {}",
        item.kind,
        item.engine_label.as_deref().unwrap_or_default(),
        item.name
    )
    .to_ascii_lowercase();
    if identity.contains("handler") || identity.contains("controller") {
        ("handler", "name-inferred")
    } else if ["repository", "query", "mapper", "dao"]
        .iter()
        .any(|token| identity.contains(token))
    {
        ("repository-query", "name-inferred")
    } else {
        ("service-function", "name-inferred")
    }
}

fn api_item_detail(item: &InventoryItem) -> String {
    item.location
        .as_ref()
        .map(|location| match location.line {
            Some(line) => format!("{}:{line}", location.path),
            None => location.path.clone(),
        })
        .unwrap_or_else(|| "소스 위치 정보 없음".to_string())
}

fn api_answer_item(
    id: &str,
    node_id: Option<String>,
    kind: &str,
    title: &str,
    detail: &str,
    truth_class: &str,
    location: Option<SourceLocation>,
) -> ImpactReviewItem {
    ImpactReviewItem {
        id: id.to_string(),
        node_id,
        kind: kind.to_string(),
        title: title.to_string(),
        detail: safe_text(detail),
        truth_class: truth_class.to_string(),
        confidence: None,
        rank: 0,
        evidence: Vec::new(),
        location,
    }
}
