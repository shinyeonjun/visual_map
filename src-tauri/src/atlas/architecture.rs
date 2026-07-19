use std::collections::{HashMap, HashSet};

use super::model::{
    Evidence, InventoryItem, InventorySnapshot, SnapshotLink, VisualEdge, VisualMap, VisualNode,
};
use super::visual_map::visual_node;
pub(super) fn atlas_overview(snapshot: &InventorySnapshot, mode: String) -> VisualMap {
    let (groups, item_group, _) = atlas_groups(snapshot);
    let hidden = groups.len().saturating_sub(40);
    let visible_groups = groups.into_iter().take(40).collect::<Vec<_>>();
    let visible_ids = visible_groups
        .iter()
        .map(|group| group.id.clone())
        .collect::<HashSet<_>>();
    let nodes = visible_groups
        .iter()
        .map(atlas_group_node)
        .collect::<Vec<_>>();
    let (edges, hidden_edges) = atlas_group_edges(snapshot, &item_group, &visible_ids);
    let mut warnings = vec![if architecture_package_names(snapshot).is_empty() {
        format!(
            "구조 메타데이터가 없어 원본 항목 {}개를 경로·이름 기반 보조 그룹 {}개로 축약했습니다",
            snapshot.items.len(),
            nodes.len()
        )
    } else {
        format!(
            "원본 항목 {}개를 코드 엔진 패키지와 DB 스키마 기준 구조 영역 {}개로 축약했습니다",
            snapshot.items.len(),
            nodes.len()
        )
    }];
    let omitted_code_symbols = snapshot
        .items
        .iter()
        .filter(|item| item.source == "code" && item.layer == "code")
        .filter(|item| !architecture_member(item))
        .count();
    if omitted_code_symbols > 0 {
        warnings.push(format!(
            "필드·변수·데코레이터 등 하위 코드 심벌 {omitted_code_symbols}개는 구조 순위에서 제외하고 코드 검색에 보존했습니다"
        ));
    }
    if hidden > 0 {
        warnings.push(format!(
            "구조 영역 +{hidden}개는 중요도 순위 밖이라 접었습니다"
        ));
    }
    if hidden_edges > 0 {
        warnings.push(format!(
            "구조 영역 간 관계 +{hidden_edges}개는 우선순위 밖이라 접었습니다"
        ));
    }

    VisualMap {
        id: format!("map:{}:atlas", snapshot.workspace_id),
        workspace_id: snapshot.workspace_id.clone(),
        mode,
        focus: "overview".to_string(),
        nodes,
        edges,
        warnings,
        review_board: None,
        api_reading: None,
    }
}

pub(super) fn atlas_group_detail(
    snapshot: &InventorySnapshot,
    group_id: &str,
    mode: String,
) -> VisualMap {
    let (groups, _, item_evidence) = atlas_groups(snapshot);
    let Some(group) = groups.iter().find(|group| group.id == group_id) else {
        let mut map = atlas_overview(snapshot, mode);
        map.warnings
            .push("선택한 구조 영역을 찾지 못해 전체 구조를 표시합니다".to_string());
        return map;
    };

    let item_by_id = snapshot
        .items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let members = select_atlas_detail_members(&group.member_ids, &item_by_id, 35);
    let hidden = group.member_ids.len().saturating_sub(members.len());
    let visible_member_ids = members
        .iter()
        .map(|item| item.id.as_str())
        .collect::<HashSet<_>>();
    let mut nodes = Vec::with_capacity(members.len() + 1);
    nodes.push(atlas_group_node(group));
    nodes.extend(members.iter().map(|item| visual_node(item)));

    let mut edges = members
        .iter()
        .map(|item| VisualEdge {
            id: format!("group-contains:{group_id}->{}", item.id),
            from: group_id.to_string(),
            to: item.id.clone(),
            kind: "group_contains".to_string(),
            confidence: None,
            evidence: item_evidence
                .get(item.id.as_str())
                .map(|text| {
                    vec![Evidence {
                        kind: "group-evidence".to_string(),
                        text: text.clone(),
                    }]
                })
                .unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    let (member_edges, hidden_edges) =
        atlas_member_edges(snapshot, &item_by_id, &visible_member_ids);
    edges.extend(member_edges);
    edges.sort_by(|left, right| left.id.cmp(&right.id));

    let mut warnings = vec![format!(
        "{} 구조 영역 · API {} → 코드 {} → DB {} 순서로 표시합니다",
        group.title, group.api_count, group.code_count, group.db_count
    )];
    if hidden > 0 {
        warnings.push(format!(
            "구조 영역 항목 +{hidden}개는 상세 화면에서 접었습니다"
        ));
    }
    if hidden_edges > 0 {
        warnings.push(format!(
            "구조 영역 관계 +{hidden_edges}개는 우선순위 밖이라 접었습니다"
        ));
    }

    VisualMap {
        id: format!("map:{}:atlas:{group_id}", snapshot.workspace_id),
        workspace_id: snapshot.workspace_id.clone(),
        mode,
        focus: group_id.to_string(),
        nodes,
        edges,
        warnings,
        review_board: None,
        api_reading: None,
    }
}

pub(super) fn narrow_focus_map(snapshot: &InventorySnapshot, mode: String) -> VisualMap {
    VisualMap {
        id: format!("map:{}:narrow-focus:{mode}", snapshot.workspace_id),
        workspace_id: snapshot.workspace_id.clone(),
        mode,
        focus: "narrow-focus".to_string(),
        nodes: Vec::new(),
        edges: Vec::new(),
        warnings: vec![
            "결과가 너무 넓습니다. 왼쪽 목록에서 API/테이블 대상을 선택하거나 검색어를 좁히세요."
                .to_string(),
        ],
        review_board: None,
        api_reading: None,
    }
}

pub(super) fn mode_node_cap(mode: &str) -> usize {
    match mode {
        "atlas" | "explore" => 40,
        "api-flow" | "search-focus" => 32,
        "table-usage" | "column-impact" => 36,
        _ => 30,
    }
}

fn atlas_groups(
    snapshot: &InventorySnapshot,
) -> (
    Vec<AtlasGroup>,
    HashMap<String, String>,
    HashMap<String, String>,
) {
    let mut groups = HashMap::<String, AtlasGroup>::new();
    let mut item_group = HashMap::new();
    let mut item_evidence = HashMap::new();
    let packages = architecture_package_names(snapshot);

    for item in snapshot
        .items
        .iter()
        .filter(|item| architecture_member(item))
    {
        let Some(seed) = atlas_group_seed(item, &packages) else {
            continue;
        };
        let group_id = format!("group:{}:{}", seed.namespace, slug(&seed.key));
        item_group.insert(item.id.clone(), group_id.clone());
        item_evidence.insert(item.id.clone(), seed.evidence.clone());
        groups
            .entry(group_id.clone())
            .and_modify(|group| group.add(item, &seed))
            .or_insert_with(|| AtlasGroup::new(group_id, item, &seed));
    }

    // FK endpoints are columns, but the architecture card owns the table rather than every column.
    for item in snapshot.items.iter().filter(|item| item.kind == "column") {
        let Some(parent_id) = item.parent_id.as_deref() else {
            continue;
        };
        let Some(group_id) = item_group.get(parent_id).cloned() else {
            continue;
        };
        item_group.insert(item.id.clone(), group_id);
    }

    for link in snapshot
        .links
        .iter()
        .filter(|link| link.truth_class == "confirmed")
    {
        let Some(from) = item_group.get(&link.from) else {
            continue;
        };
        let Some(to) = item_group.get(&link.to) else {
            continue;
        };
        if let Some(group) = groups.get_mut(from) {
            group.confirmed_degree += 1;
        }
        if from != to {
            if let Some(group) = groups.get_mut(to) {
                group.confirmed_degree += 1;
            }
        }
    }

    let item_by_id = snapshot
        .items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let mut degrees = HashMap::<&str, usize>::new();
    for link in snapshot
        .links
        .iter()
        .filter(|link| link.truth_class == "confirmed")
    {
        *degrees.entry(link.from.as_str()).or_default() += 1;
        *degrees.entry(link.to.as_str()).or_default() += 1;
    }
    let mut groups = groups.into_values().collect::<Vec<_>>();
    for group in &mut groups {
        group.sort_members(&item_by_id, &degrees);
    }
    groups.sort_by(|a, b| {
        let a_has_product_surface = a.api_count > 0 || a.db_count > 0;
        let b_has_product_surface = b.api_count > 0 || b.db_count > 0;
        b_has_product_surface
            .cmp(&a_has_product_surface)
            .then_with(|| b.confirmed_degree.cmp(&a.confirmed_degree))
            .then_with(|| b.api_count.cmp(&a.api_count))
            .then_with(|| b.db_count.cmp(&a.db_count))
            .then_with(|| b.member_ids.len().cmp(&a.member_ids.len()))
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.id.cmp(&b.id))
    });
    (groups, item_group, item_evidence)
}

fn architecture_member(item: &InventoryItem) -> bool {
    item.source != "code"
        || item.layer == "api"
        || matches!(
            item.kind.as_str(),
            "handler"
                | "service"
                | "repository"
                | "function"
                | "method"
                | "class"
                | "module"
                | "file"
        )
}

fn architecture_package_names(snapshot: &InventorySnapshot) -> HashMap<String, String> {
    snapshot
        .metadata
        .architecture
        .as_ref()
        .and_then(|architecture| architecture.get("packages"))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|package| {
            package
                .as_str()
                .or_else(|| package.get("name").and_then(serde_json::Value::as_str))
        })
        .map(str::trim)
        .filter(|name| !name.is_empty() && name.len() <= 128)
        .take(512)
        .map(|name| (name.to_ascii_lowercase(), name.to_string()))
        .collect()
}

fn structural_package(item: &InventoryItem, packages: &HashMap<String, String>) -> Option<String> {
    let mut matched = None;
    for value in [
        item.group_id.as_deref(),
        item.qualified_name.as_deref(),
        item.path.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        for part in value
            .split(['/', '\\', '.', ':'])
            .map(str::trim)
            .filter(|part| !part.is_empty())
        {
            if let Some(package) = packages.get(&part.to_ascii_lowercase()) {
                matched = Some(package.clone());
            }
        }
    }
    matched
}

fn structural_path_root(path: Option<&str>) -> Option<&str> {
    path?.split(['/', '\\']).find(|part| {
        !part.is_empty()
            && *part != "."
            && !part.ends_with(':')
            && !matches!(
                part.to_ascii_lowercase().as_str(),
                "src" | "source" | "lib" | "libs"
            )
    })
}

fn atlas_group_seed(
    item: &InventoryItem,
    packages: &HashMap<String, String>,
) -> Option<AtlasGroupSeed> {
    if !packages.is_empty() && item.source == "code" {
        let package = structural_package(item, packages);
        let (key, label, evidence) = match package {
            Some(package) => (
                package.to_ascii_lowercase(),
                package.clone(),
                format!("코드 엔진 architecture package `{package}` 기준으로 묶었습니다"),
            ),
            None => {
                let root = structural_path_root(item.path.as_deref()).unwrap_or("root");
                (
                    root.to_ascii_lowercase(),
                    root.to_string(),
                    format!(
                        "architecture package와 매칭되지 않아 소스 최상위 경로 `{root}` 기준으로 묶었습니다"
                    ),
                )
            }
        };
        return Some(AtlasGroupSeed {
            namespace: "package",
            key,
            label,
            title_priority: if item.layer == "api" { 0 } else { 1 },
            evidence,
        });
    }
    if !packages.is_empty() && item.source == "db" && item.kind == "table" {
        let schema = item
            .path
            .as_deref()
            .filter(|schema| !schema.is_empty())
            .unwrap_or("default");
        return Some(AtlasGroupSeed {
            namespace: "db-schema",
            key: schema.to_ascii_lowercase(),
            label: format!("DB · {schema}"),
            title_priority: 2,
            evidence: format!("DB 스키마 `{schema}` 경계 기준으로 묶었습니다"),
        });
    }
    if item.source == "code" && item.layer == "api" {
        let label = route_domain(&item.name).unwrap_or_else(|| "root".to_string());
        return Some(AtlasGroupSeed {
            namespace: "domain",
            key: canonical_domain(&label),
            label: label.clone(),
            title_priority: 0,
            evidence: format!(
                "구조 메타데이터가 없어 라우트 경로에서 보조 그룹 `{label}`을 만들었습니다"
            ),
        });
    }
    if item.source == "code" && item.layer == "code" {
        let label = item
            .group_id
            .as_deref()
            .and_then(group_id_domain)
            .or_else(|| item.path.as_deref().and_then(path_domain))
            .or_else(|| text_domain(&item.name))
            .unwrap_or_else(|| "code".to_string());
        let evidence_source = item
            .group_id
            .as_deref()
            .or(item.path.as_deref())
            .unwrap_or(&item.name);
        return Some(AtlasGroupSeed {
            namespace: "domain",
            key: canonical_domain(&label),
            label,
            title_priority: 1,
            evidence: format!("코드 경로/그룹 `{evidence_source}` 기준으로 묶었습니다"),
        });
    }
    if item.source == "db" && item.kind == "table" {
        let schema = item.path.as_deref().filter(|schema| !schema.is_empty());
        let label = schema
            .filter(|schema| !is_default_schema(schema))
            .and_then(text_domain)
            .or_else(|| text_domain(&item.name))
            .unwrap_or_else(|| schema.unwrap_or("database").to_string());
        let evidence = match schema {
            Some(schema) if !is_default_schema(schema) => {
                format!("DB 스키마 `{schema}` 기준으로 묶었습니다")
            }
            Some(schema) => format!("DB `{schema}.{}` 테이블명 기준으로 묶었습니다", item.name),
            None => format!("DB `{}` 테이블명 기준으로 묶었습니다", item.name),
        };
        return Some(AtlasGroupSeed {
            namespace: "domain",
            key: canonical_domain(&label),
            label,
            title_priority: 2,
            evidence,
        });
    }

    None
}

fn atlas_group_node(group: &AtlasGroup) -> VisualNode {
    VisualNode {
        id: group.id.clone(),
        kind: "group-domain".to_string(),
        title: group.title.clone(),
        subtitle: Some(format!(
            "API {} · 코드 {} · DB {}|{}|{}|{}",
            group.api_count,
            group.code_count,
            group.db_count,
            atlas_top_summary(&group.top_api, group.api_count),
            atlas_top_summary(&group.top_code, group.code_count),
            atlas_top_summary(&group.top_db, group.db_count)
        )),
        layer: "mixed".to_string(),
        source: "projection".to_string(),
    }
}

fn atlas_group_edges(
    snapshot: &InventorySnapshot,
    item_group: &HashMap<String, String>,
    visible_ids: &HashSet<String>,
) -> (Vec<VisualEdge>, usize) {
    let mut seen = HashSet::new();
    let mut edges = snapshot
        .links
        .iter()
        .filter_map(|link| {
            let from = item_group.get(&link.from)?;
            let to = item_group.get(&link.to)?;
            if from == to || !visible_ids.contains(from) || !visible_ids.contains(to) {
                return None;
            }
            let kind = atlas_truth_kind(link, "group_")?;
            let id = format!("{kind}:{from}->{to}");
            if !seen.insert(id.clone()) {
                return None;
            }
            Some(VisualEdge {
                id,
                from: from.clone(),
                to: to.clone(),
                kind,
                confidence: None,
                evidence: link.evidence.clone(),
            })
        })
        .collect::<Vec<_>>();
    edges.sort_by(|left, right| {
        atlas_projection_edge_rank(left)
            .cmp(&atlas_projection_edge_rank(right))
            .then_with(|| left.id.cmp(&right.id))
    });
    let hidden = edges.len().saturating_sub(80);
    edges.truncate(80);
    (edges, hidden)
}

fn atlas_member_edges(
    snapshot: &InventorySnapshot,
    item_by_id: &HashMap<&str, &InventoryItem>,
    visible_ids: &HashSet<&str>,
) -> (Vec<VisualEdge>, usize) {
    let mut seen = HashSet::new();
    let mut edges = snapshot
        .links
        .iter()
        .filter_map(|link| {
            let from = atlas_visible_endpoint(&link.from, item_by_id, visible_ids)?;
            let to = atlas_visible_endpoint(&link.to, item_by_id, visible_ids)?;
            if from == to {
                return None;
            }
            let kind = atlas_truth_kind(link, "")?;
            let id = format!("atlas:{kind}:{from}->{to}");
            if !seen.insert(id.clone()) {
                return None;
            }
            Some(VisualEdge {
                id,
                from: from.to_string(),
                to: to.to_string(),
                kind,
                confidence: None,
                evidence: link.evidence.clone(),
            })
        })
        .collect::<Vec<_>>();
    edges.sort_by(|left, right| {
        atlas_projection_edge_rank(left)
            .cmp(&atlas_projection_edge_rank(right))
            .then_with(|| left.id.cmp(&right.id))
    });
    let hidden = edges.len().saturating_sub(64);
    edges.truncate(64);
    (edges, hidden)
}

fn atlas_truth_kind(link: &SnapshotLink, prefix: &str) -> Option<String> {
    match link.truth_class.as_str() {
        "confirmed" => Some(format!("{prefix}{}", link.kind)),
        "candidate" => Some(format!("candidate_{prefix}{}", link.kind)),
        "structural" | "" => Some(format!("structural_{prefix}{}", link.kind)),
        _ => None,
    }
}

fn atlas_projection_edge_rank(edge: &VisualEdge) -> u8 {
    if edge.kind.starts_with("candidate_") {
        2
    } else if edge.kind.starts_with("structural_") {
        1
    } else {
        0
    }
}

fn atlas_visible_endpoint<'a>(
    id: &'a str,
    item_by_id: &HashMap<&str, &'a InventoryItem>,
    visible_ids: &HashSet<&str>,
) -> Option<&'a str> {
    if visible_ids.contains(id) {
        return Some(id);
    }
    item_by_id
        .get(id)
        .and_then(|item| item.parent_id.as_deref())
        .filter(|parent| visible_ids.contains(parent))
}

fn route_domain(value: &str) -> Option<String> {
    let path = value
        .split_whitespace()
        .find(|part| part.starts_with('/'))
        .unwrap_or(value);
    path.trim_start_matches('/')
        .split('/')
        .find_map(text_domain)
}

fn path_domain(value: &str) -> Option<String> {
    let mut parts = value
        .split(['/', '\\'])
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>();
    if parts.len() > 1 {
        // The final segment is normally a source file. Grouping by it merges every
        // `service.rs`/`index.ts` across the project instead of the owning folder.
        parts.pop();
    }
    parts.into_iter().rev().find_map(text_domain)
}

fn group_id_domain(value: &str) -> Option<String> {
    value
        .split(['/', '\\', '.', ':'])
        .filter(|part| !part.is_empty())
        .rev()
        .find_map(text_domain)
}

fn text_domain(value: &str) -> Option<String> {
    semantic_tokens(value).into_iter().next()
}

fn canonical_domain(value: &str) -> String {
    text_domain(value).unwrap_or_else(|| "other".to_string())
}

fn semantic_tokens(value: &str) -> Vec<String> {
    let mut words = String::with_capacity(value.len());
    let mut previous_lower = false;
    for character in value.chars() {
        if character.is_alphanumeric() {
            if character.is_uppercase() && previous_lower {
                words.push(' ');
            }
            for lower in character.to_lowercase() {
                words.push(lower);
            }
            previous_lower = character.is_lowercase();
        } else {
            words.push(' ');
            previous_lower = false;
        }
    }

    words
        .split_whitespace()
        .filter(|word| !is_generic_domain_word(word))
        .map(singular_domain)
        .collect()
}

fn singular_domain(value: &str) -> String {
    if value.len() > 3 && value.ends_with("ies") {
        return format!("{}y", &value[..value.len() - 3]);
    }
    for suffix in ["ches", "shes", "xes", "zes", "ses"] {
        if value.len() > suffix.len() && value.ends_with(suffix) {
            return value[..value.len() - 2].to_string();
        }
    }
    if value.len() > 2
        && value.ends_with('s')
        && !value.ends_with("ss")
        && !value.ends_with("us")
        && !value.ends_with("is")
    {
        return value[..value.len() - 1].to_string();
    }
    value.to_string()
}

fn is_generic_domain_word(value: &str) -> bool {
    value.len() < 2
        || value.chars().all(|character| character.is_ascii_digit())
        || (value.starts_with('v')
            && value[1..]
                .chars()
                .all(|character| character.is_ascii_digit()))
        || matches!(
            value,
            "api"
                | "app"
                | "apps"
                | "backend"
                | "code"
                | "common"
                | "controller"
                | "controllers"
                | "core"
                | "create"
                | "database"
                | "db"
                | "domain"
                | "domains"
                | "feature"
                | "features"
                | "find"
                | "get"
                | "handler"
                | "handlers"
                | "internal"
                | "id"
                | "java"
                | "kotlin"
                | "lib"
                | "libs"
                | "list"
                | "main"
                | "model"
                | "models"
                | "module"
                | "modules"
                | "package"
                | "packages"
                | "python"
                | "pkg"
                | "repository"
                | "repositories"
                | "repo"
                | "read"
                | "route"
                | "routes"
                | "schema"
                | "save"
                | "server"
                | "service"
                | "services"
                | "shared"
                | "source"
                | "src"
                | "test"
                | "tests"
                | "update"
                | "util"
                | "utils"
                | "write"
                | "delete"
                | "com"
                | "org"
                | "net"
                | "io"
                | "js"
                | "jsx"
                | "ts"
                | "tsx"
                | "rs"
                | "py"
                | "kt"
        )
}

fn is_default_schema(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "public" | "dbo" | "main" | "default"
    )
}

fn slug(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

struct AtlasGroupSeed {
    namespace: &'static str,
    key: String,
    label: String,
    title_priority: u8,
    evidence: String,
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
    top_api: Vec<String>,
    top_code: Vec<String>,
    top_db: Vec<String>,
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
            top_api: Vec::new(),
            top_code: Vec::new(),
            top_db: Vec::new(),
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
        if item.layer == "api" {
            self.api_count += 1;
        } else if item.source == "db" {
            self.db_count += 1;
        } else {
            self.code_count += 1;
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
    } else if item.source == "code" {
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
            "code" => item.source == "code" && item.layer != "api",
            "db" => item.source == "db" && item.kind == "table",
            _ => false,
        })
        .map(|item| item.name.replace('|', "/"))
        .take(2)
        .collect()
}
