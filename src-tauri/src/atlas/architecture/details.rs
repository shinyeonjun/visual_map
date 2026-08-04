struct AtlasGroupSeed {
    id: String,
    label: String,
    title_priority: u8,
    evidence: String,
    parent_id: Option<String>,
    parent_title: Option<String>,
    depth: usize,
    assigned_by: &'static str,
}

struct AtlasGroup {
    id: String,
    title: String,
    title_priority: u8,
    member_ids: Vec<String>,
    api_count: usize,
    code_count: usize,
    db_count: usize,
    confirmed_degree: usize,
    in_degree: usize,
    out_degree: usize,
    language_counts: HashMap<String, usize>,
    has_partial: bool,
    top_api: Vec<String>,
    top_code: Vec<String>,
    top_db: Vec<String>,
    handler_count: usize,
    service_count: usize,
    repository_count: usize,
    parent_id: Option<String>,
    depth: usize,
    assigned_by: &'static str,
}

impl AtlasGroup {
    fn new(id: String, item: &InventoryItem, seed: &AtlasGroupSeed) -> Self {
        let mut group = Self {
            id,
            title: seed.label.clone(),
            title_priority: seed.title_priority,
            member_ids: Vec::new(),
            api_count: 0,
            code_count: 0,
            db_count: 0,
            confirmed_degree: 0,
            in_degree: 0,
            out_degree: 0,
            language_counts: HashMap::new(),
            has_partial: false,
            top_api: Vec::new(),
            top_code: Vec::new(),
            top_db: Vec::new(),
            handler_count: 0,
            service_count: 0,
            repository_count: 0,
            parent_id: seed.parent_id.clone(),
            depth: seed.depth,
            assigned_by: seed.assigned_by,
        };
        group.add(item, seed);
        group
    }

    fn add(&mut self, item: &InventoryItem, seed: &AtlasGroupSeed) {
        if seed.title_priority < self.title_priority
            || (seed.title_priority == self.title_priority && seed.label < self.title)
        {
            self.title = seed.label.clone();
            self.title_priority = seed.title_priority;
        }
        self.member_ids.push(item.id.clone());
        let language = item
            .language
            .clone()
            .or_else(|| item.path.as_deref().and_then(infer_language_from_path))
            .unwrap_or_else(|| "unknown".to_string());
        *self.language_counts.entry(language).or_default() += 1;
        if item.layer == "api" {
            self.api_count += 1;
        } else if item.is_db() {
            self.db_count += 1;
        } else {
            self.code_count += 1;
        }
        match item.kind.as_str() {
            "handler" => self.handler_count += 1,
            "service" => self.service_count += 1,
            "repository" => self.repository_count += 1,
            _ => {}
        }
    }

    fn sort_members(
        &mut self,
        item_by_id: &HashMap<&str, &InventoryItem>,
        degrees: &HashMap<&str, usize>,
    ) {
        self.member_ids.sort_by(|left, right| {
            let left_item = item_by_id.get(left.as_str()).copied().unwrap();
            let right_item = item_by_id.get(right.as_str()).copied().unwrap();
            let left_order = atlas_member_order(left_item);
            let right_order = atlas_member_order(right_item);
            left_order
                .0
                .cmp(&right_order.0)
                .then_with(|| {
                    degrees
                        .get(right.as_str())
                        .unwrap_or(&0)
                        .cmp(degrees.get(left.as_str()).unwrap_or(&0))
                })
                .then_with(|| left_order.1.cmp(&right_order.1))
                .then_with(|| left_item.name.cmp(&right_item.name))
                .then_with(|| left_item.id.cmp(&right_item.id))
        });
        self.top_api = atlas_top_titles(&self.member_ids, item_by_id, "api");
        self.top_code = atlas_top_titles(&self.member_ids, item_by_id, "code");
        self.top_db = atlas_top_titles(&self.member_ids, item_by_id, "db");
    }
}

fn atlas_top_summary(items: &[String], total: usize) -> String {
    let mut summary = items.join(" · ");
    let hidden = total.saturating_sub(items.len());
    if hidden > 0 {
        if !summary.is_empty() {
            summary.push_str(" · ");
        }
        summary.push_str(&format!("+{hidden}"));
    }
    summary
}

fn select_atlas_detail_members<'a>(
    member_ids: &[String],
    item_by_id: &HashMap<&str, &'a InventoryItem>,
    limit: usize,
) -> Vec<&'a InventoryItem> {
    let members = member_ids
        .iter()
        .filter_map(|id| item_by_id.get(id.as_str()).copied())
        .collect::<Vec<_>>();
    if members.len() <= limit {
        return members;
    }

    let mut selected = HashSet::new();
    for layer in 0..=2 {
        for item in members
            .iter()
            .filter(|item| atlas_member_order(item).0 == layer)
            .take(4)
        {
            if selected.len() >= limit {
                break;
            }
            selected.insert(item.id.as_str());
        }
    }
    for item in &members {
        if selected.len() >= limit {
            break;
        }
        selected.insert(item.id.as_str());
    }
    members
        .into_iter()
        .filter(|item| selected.contains(item.id.as_str()))
        .collect()
}

fn atlas_member_order(item: &InventoryItem) -> (u8, u8) {
    let layer = if item.layer == "api" {
        0
    } else if item.is_code() {
        1
    } else {
        2
    };
    let kind = match item.kind.as_str() {
        "handler" => 0,
        "service" => 1,
        "repository" => 2,
        "function" | "method" => 3,
        "class" => 4,
        "file" => 5,
        "table" => 0,
        _ => 6,
    };
    (layer, kind)
}

fn atlas_top_titles(
    member_ids: &[String],
    item_by_id: &HashMap<&str, &InventoryItem>,
    bucket: &str,
) -> Vec<String> {
    member_ids
        .iter()
        .filter_map(|id| item_by_id.get(id.as_str()).copied())
        .filter(|item| match bucket {
            "api" => item.layer == "api",
            "code" => item.is_code() && item.layer != "api",
            "db" => item.is_db() && item.kind == "table",
            _ => false,
        })
        .map(|item| item.name.replace('|', "/"))
        .take(2)
        .collect()
}
