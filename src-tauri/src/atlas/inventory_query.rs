use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use super::model::{InventoryItem, InventorySnapshot};

const BOOTSTRAP_CODE_ITEMS_PER_GROUP: usize = 100;
const BOOTSTRAP_DB_TABLES: usize = 100;
const BOOTSTRAP_DB_DEPENDENTS: usize = 200;
const SEARCH_RESULTS_PER_GROUP: usize = 4;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InventoryBootstrap {
    pub snapshot: InventorySnapshot,
    pub summary: InventorySummary,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InventorySummary {
    pub workspace_id: String,
    pub saved_at: String,
    pub total_items: usize,
    pub total_links: usize,
    pub sources: BTreeMap<String, InventorySourceSummary>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InventorySourceSummary {
    pub total: usize,
    pub groups: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InventorySearchResult {
    pub hits: Vec<InventorySearchHit>,
    pub total: usize,
    pub counts: BTreeMap<String, usize>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InventorySearchHit {
    pub group: String,
    pub item: InventoryItem,
}

pub(crate) fn inventory_bootstrap(snapshot: &InventorySnapshot) -> InventoryBootstrap {
    let handler_ids = confirmed_handler_ids(snapshot);
    let summary = inventory_summary(snapshot, &handler_ids);
    let mut group_counts = HashMap::<String, usize>::new();
    let retained_db_table_ids = snapshot
        .items
        .iter()
        .filter(|item| item.source == "db" && item.kind == "table")
        .take(BOOTSTRAP_DB_TABLES)
        .map(|item| item.id.clone())
        .collect::<HashSet<_>>();
    let mut retained_db_item_ids = retained_db_table_ids.clone();
    retained_db_item_ids.extend(
        snapshot
            .items
            .iter()
            .filter(|item| {
                item.source == "db"
                    && !matches!(item.kind.as_str(), "view" | "trigger" | "routine")
                    && item
                        .parent_id
                        .as_ref()
                        .is_some_and(|parent| retained_db_table_ids.contains(parent))
            })
            .map(|item| item.id.clone()),
    );
    let mut retained_db_dependents = snapshot
        .links
        .iter()
        .filter_map(|link| {
            if link.kind == "db_dependency" && retained_db_item_ids.contains(&link.to) {
                Some(link.from.clone())
            } else if link.kind == "db_trigger" && retained_db_table_ids.contains(&link.from) {
                Some(link.to.clone())
            } else {
                None
            }
        })
        .collect::<BTreeSet<_>>();
    retained_db_dependents.extend(
        snapshot
            .items
            .iter()
            .filter(|item| {
                item.source == "db"
                    && item.kind == "trigger"
                    && item
                        .parent_id
                        .as_ref()
                        .is_some_and(|parent| retained_db_table_ids.contains(parent))
            })
            .map(|item| item.id.clone()),
    );
    retained_db_item_ids.extend(
        retained_db_dependents
            .into_iter()
            .take(BOOTSTRAP_DB_DEPENDENTS),
    );
    let mut retained_ids = HashSet::new();
    let items = snapshot
        .items
        .iter()
        .filter(|item| {
            if item.source == "db" {
                let retained = retained_db_item_ids.contains(&item.id);
                if retained {
                    retained_ids.insert(item.id.clone());
                }
                return retained;
            }
            if item.source != "code" {
                retained_ids.insert(item.id.clone());
                return true;
            }
            let group = code_group(item, &handler_ids);
            let count = group_counts.entry(group).or_default();
            if *count >= BOOTSTRAP_CODE_ITEMS_PER_GROUP {
                return false;
            }
            *count += 1;
            retained_ids.insert(item.id.clone());
            true
        })
        .cloned()
        .collect();
    let links = snapshot
        .links
        .iter()
        .filter(|link| retained_ids.contains(&link.from) && retained_ids.contains(&link.to))
        .cloned()
        .collect();
    let bounded = InventorySnapshot {
        schema_version: snapshot.schema_version,
        workspace_id: snapshot.workspace_id.clone(),
        saved_at: snapshot.saved_at.clone(),
        metadata: snapshot.metadata.clone(),
        stale_reasons: snapshot.stale_reasons.clone(),
        links,
        items,
    };

    InventoryBootstrap {
        snapshot: bounded,
        summary,
    }
}

pub(crate) fn search_inventory(snapshot: &InventorySnapshot, query: &str) -> InventorySearchResult {
    let query = query.trim().to_lowercase();
    if query.chars().count() < 2 {
        return InventorySearchResult {
            hits: Vec::new(),
            total: 0,
            counts: BTreeMap::new(),
            truncated: false,
        };
    }

    let mut counts = BTreeMap::<String, usize>::new();
    let mut ranked = BTreeMap::<String, Vec<(u16, &InventoryItem)>>::new();
    for item in &snapshot.items {
        if item.source == "code" && !item.is_project_code_item() {
            continue;
        }
        let Some(group) = search_group(item) else {
            continue;
        };
        let score = search_score(item, &query);
        if score == 0 {
            continue;
        }
        *counts.entry(group.to_string()).or_default() += 1;
        let hits = ranked.entry(group.to_string()).or_default();
        hits.push((score, item));
        hits.sort_by(|(left_score, left), (right_score, right)| {
            right_score
                .cmp(left_score)
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.id.cmp(&right.id))
        });
        hits.truncate(SEARCH_RESULTS_PER_GROUP);
    }

    let total = counts.values().sum();
    let hits = ["api", "code", "file", "table", "db-object", "column"]
        .into_iter()
        .flat_map(|group| {
            ranked
                .remove(group)
                .unwrap_or_default()
                .into_iter()
                .map(move |(_, item)| InventorySearchHit {
                    group: group.to_string(),
                    item: item.clone(),
                })
        })
        .collect::<Vec<_>>();

    InventorySearchResult {
        truncated: total > hits.len(),
        hits,
        total,
        counts,
    }
}

fn inventory_summary(
    snapshot: &InventorySnapshot,
    handler_ids: &HashSet<String>,
) -> InventorySummary {
    let mut sources = BTreeMap::<String, InventorySourceSummary>::new();
    for item in &snapshot.items {
        let (source, group) = if item.source == "code" {
            ("code", code_group(item, handler_ids))
        } else {
            (item.source.as_str(), item.kind.clone())
        };
        let source_summary = sources.entry(source.to_string()).or_default();
        source_summary.total += 1;
        *source_summary.groups.entry(group).or_default() += 1;
    }
    InventorySummary {
        workspace_id: snapshot.workspace_id.clone(),
        saved_at: snapshot.saved_at.clone(),
        total_items: snapshot.items.len(),
        total_links: snapshot.links.len(),
        sources,
    }
}

fn confirmed_handler_ids(snapshot: &InventorySnapshot) -> HashSet<String> {
    snapshot
        .links
        .iter()
        .filter(|link| link.kind == "code_handle")
        .map(|link| link.to.clone())
        .collect()
}

fn code_group(item: &InventoryItem, handler_ids: &HashSet<String>) -> String {
    if item.layer == "api" {
        return "routes".to_string();
    }
    if item.kind == "file" {
        return "files".to_string();
    }
    if handler_ids.contains(&item.id) {
        return "handlers".to_string();
    }
    let text = format!("{} {}", item.kind, item.name).to_lowercase();
    if text.contains("handler") || text.contains("controller") {
        "handlers"
    } else if text.contains("repository") || text.contains("repo") || text.contains("dao") {
        "repositories"
    } else if text.contains("service") {
        "services"
    } else if text.contains("function") || text.contains("method") {
        "functions"
    } else if text.contains("class") {
        "classes"
    } else if text.contains("module") || text.contains("package") {
        "modules"
    } else {
        "unknown"
    }
    .to_string()
}

fn search_group(item: &InventoryItem) -> Option<&'static str> {
    if item.source == "code" {
        if item.layer == "api" {
            Some("api")
        } else if item.kind == "file" {
            Some("file")
        } else {
            Some("code")
        }
    } else if item.source == "db" && item.kind == "table" {
        Some("table")
    } else if item.source == "db" && item.kind == "column" {
        Some("column")
    } else if item.source == "db" && matches!(item.kind.as_str(), "view" | "trigger" | "routine") {
        Some("db-object")
    } else {
        None
    }
}

fn search_score(item: &InventoryItem, query: &str) -> u16 {
    [
        field_score(&item.name, query, 400, 300, 200),
        field_score(
            item.qualified_name.as_deref().unwrap_or_default(),
            query,
            350,
            250,
            150,
        ),
        field_score(
            item.path.as_deref().unwrap_or_default(),
            query,
            120,
            100,
            80,
        ),
        field_score(&item.id, query, 110, 90, 70),
    ]
    .into_iter()
    .max()
    .unwrap_or_default()
}

fn field_score(value: &str, query: &str, exact: u16, prefix: u16, contains: u16) -> u16 {
    let value = value.to_lowercase();
    if value == query {
        exact
    } else if value.starts_with(query) {
        prefix
    } else if value.contains(query) {
        contains
    } else {
        0
    }
}
