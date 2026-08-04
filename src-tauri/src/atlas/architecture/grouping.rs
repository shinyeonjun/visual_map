#[derive(Clone)]
struct ArchitectureModuleProjection {
    package_label: String,
    package_group_id: String,
    path: String,
    title: String,
    group_id: String,
}

#[derive(Default)]
struct ArchitectureModuleIndex {
    by_id: HashMap<String, ArchitectureModuleProjection>,
    file_modules: HashMap<String, String>,
}

impl ArchitectureModuleIndex {
    fn from_snapshot(snapshot: &InventorySnapshot) -> Self {
        let Some(nodes) = snapshot
            .metadata
            .architecture
            .as_ref()
            .and_then(|architecture| architecture.get("nodes"))
            .and_then(serde_json::Value::as_array)
        else {
            return Self::default();
        };

        let mut packages = HashMap::<String, (String, Option<String>)>::new();
        for node in nodes {
            if !architecture_node_string(node, &["kind"])
                .is_some_and(|kind| kind.eq_ignore_ascii_case("package"))
            {
                continue;
            }
            let Some(id) = architecture_node_string(node, &["id"]) else {
                continue;
            };
            let label = architecture_node_string(node, &["name", "label", "path"])
                .unwrap_or_else(|| "root".to_string());
            let path = architecture_node_string(node, &["path"])
                .map(|path| normalize_architecture_path(&path));
            packages.insert(id, (label, path));
        }

        let mut raw_modules = Vec::<(String, String, String, Option<String>)>::new();
        let mut file_modules = HashMap::new();
        for node in nodes {
            let kind = architecture_node_string(node, &["kind"]).unwrap_or_default();
            let Some(id) = architecture_node_string(node, &["id"]) else {
                continue;
            };
            if kind.eq_ignore_ascii_case("module") {
                let Some(path) = architecture_node_string(node, &["module_path", "modulePath", "path"])
                    .map(|path| normalize_architecture_path(&path))
                    .filter(|path| !path.is_empty())
                else {
                    continue;
                };
                let title = architecture_node_string(node, &["name", "label"])
                    .or_else(|| path.rsplit('/').next().map(str::to_string))
                    .unwrap_or_else(|| "root".to_string());
                let parent_id = architecture_node_string(node, &["parent_id", "parentId"]);
                raw_modules.push((id, path, title, parent_id));
            } else if kind.eq_ignore_ascii_case("file") {
                let (Some(path), Some(parent_id)) = (
                    architecture_node_string(node, &["path"]),
                    architecture_node_string(node, &["parent_id", "parentId"]),
                ) else {
                    continue;
                };
                file_modules.insert(normalize_architecture_path(&path), parent_id);
            }
        }

        let mut title_counts = HashMap::<String, usize>::new();
        for (_, _, title, parent_id) in &raw_modules {
            let package = parent_id
                .as_ref()
                .and_then(|id| packages.get(id))
                .map(|(label, _)| label.as_str())
                .unwrap_or("root");
            *title_counts
                .entry(format!("{}\0{}", package.to_ascii_lowercase(), title.to_ascii_lowercase()))
                .or_default() += 1;
        }

        let mut by_id = HashMap::new();
        for (id, path, title, parent_id) in raw_modules {
            let package = parent_id
                .as_ref()
                .and_then(|parent| packages.get(parent))
                .map(|(label, _)| label.clone())
                .or_else(|| {
                    packages.values().filter_map(|(label, package_path)| {
                        let package_path = package_path.as_deref()?;
                        path.starts_with(&format!("{package_path}/"))
                            .then_some((package_path.len(), label.clone()))
                    }).max_by_key(|(length, _)| *length).map(|(_, label)| label)
                })
                .unwrap_or_else(|| {
                    path.split('/').next().unwrap_or("root").to_string()
                });
            let package_group_id = format!("group:package:{}", slug(&package));
            let title_key = format!("{}\0{}", package.to_ascii_lowercase(), title.to_ascii_lowercase());
            let module_key = if title_counts.get(&title_key).copied().unwrap_or(0) > 1 {
                path.clone()
            } else {
                title.clone()
            };
            let group_id = format!(
                "group:module:{}:{}",
                slug(&package),
                slug(&module_key)
            );
            by_id.insert(
                id,
                ArchitectureModuleProjection {
                    package_label: package,
                    package_group_id,
                    path,
                    title,
                    group_id,
                },
            );
        }

        Self { by_id, file_modules }
    }

    fn module_for_item<'a>(&'a self, item: &InventoryItem) -> Option<&'a ArchitectureModuleProjection> {
        let raw_id = item.id.strip_prefix("code:").unwrap_or(&item.id);
        if let Some(module) = self.by_id.get(raw_id) {
            return Some(module);
        }
        if let Some(group_id) = item.group_id.as_deref() {
            let normalized_group = normalize_architecture_path(group_id);
            if let Some(module) = self.by_id.get(group_id).or_else(|| {
                self.by_id.values().find(|module| module.path == normalized_group)
            }) {
                return Some(module);
            }
        }
        let path = item.path.as_deref().map(normalize_architecture_path)?;
        if let Some(module_id) = self.file_modules.get(&path) {
            return self.by_id.get(module_id);
        }
        self.file_modules
            .iter()
            .filter(|(file_path, _)| path.ends_with(&format!("/{file_path}")))
            .max_by_key(|(file_path, _)| file_path.len())
            .and_then(|(_, module_id)| self.by_id.get(module_id))
    }
}

fn architecture_node_string(node: &serde_json::Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = node.get(*key).and_then(serde_json::Value::as_str) {
            if !value.trim().is_empty() {
                return Some(value.to_string());
            }
        }
    }
    node.get("properties").and_then(|properties| {
        keys.iter().find_map(|key| {
            properties
                .get(*key)
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string)
        })
    })
}

fn normalize_architecture_path(path: &str) -> String {
    path.replace('\\', "/")
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/")
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
    let modules = ArchitectureModuleIndex::from_snapshot(snapshot);

    for item in snapshot
        .items
        .iter()
        .filter(|item| architecture_member(item))
    {
        let Some(seed) = atlas_group_seed(item, &packages, &modules) else {
            continue;
        };
        let group_id = seed.id.clone();
        item_group.insert(item.id.clone(), group_id.clone());
        item_evidence.insert(item.id.clone(), seed.evidence.clone());
        if let Some(parent_id) = seed.parent_id.as_ref() {
            let parent_title = seed.parent_title.as_deref().unwrap_or("root");
            let parent_seed = package_group_seed(
                parent_title,
                "package",
                0,
                format!("MODULE `{}`의 상위 PACKAGE 기준으로 묶었습니다", seed.label),
            );
            groups
                .entry(parent_id.clone())
                .and_modify(|group| group.add(item, &parent_seed))
                .or_insert_with(|| AtlasGroup::new(parent_id.clone(), item, &parent_seed));
        }
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

    let mut group_degrees = HashMap::<String, (usize, usize)>::new();
    for link in snapshot.links.iter().filter(|link| link.is_confirmed()) {
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
            group_degrees
                .entry(from.clone())
                .or_default()
                .1 += 1;
            group_degrees
                .entry(to.clone())
                .or_default()
                .0 += 1;
        }
    }

    let item_by_id = snapshot
        .items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let mut degrees = HashMap::<&str, usize>::new();
    for link in snapshot.links.iter().filter(|link| link.is_confirmed()) {
        *degrees.entry(link.from.as_str()).or_default() += 1;
        *degrees.entry(link.to.as_str()).or_default() += 1;
    }
    let mut groups = groups.into_values().collect::<Vec<_>>();
    for group in &mut groups {
        group.has_partial = atlas_group_has_partial(snapshot, group, &item_group);
        if let Some((in_degree, out_degree)) = group_degrees.get(&group.id) {
            group.in_degree = *in_degree;
            group.out_degree = *out_degree;
        }
        group.sort_members(&item_by_id, &degrees);
    }
    groups.sort_by(|a, b| {
        let a_has_product_surface = a.api_count > 0 || a.db_count > 0;
        let b_has_product_surface = b.api_count > 0 || b.db_count > 0;
        a.depth
            .cmp(&b.depth)
            .then_with(|| a.parent_id.cmp(&b.parent_id))
            .then_with(|| b_has_product_surface
            .cmp(&a_has_product_surface)
            .then_with(|| b.confirmed_degree.cmp(&a.confirmed_degree))
            .then_with(|| b.api_count.cmp(&a.api_count))
            .then_with(|| b.db_count.cmp(&a.db_count))
            .then_with(|| b.member_ids.len().cmp(&a.member_ids.len()))
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.id.cmp(&b.id)))
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

fn infer_language_from_path(path: &str) -> Option<String> {
    let extension = path.rsplit_once('.')?.1.to_ascii_lowercase();
    let language = match extension.as_str() {
        "ts" | "tsx" | "mts" | "cts" => "typescript",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "py" | "pyi" => "python",
        "java" => "java",
        "cs" => "csharp",
        "go" => "go",
        "rs" => "rust",
        "php" => "php",
        "rb" | "rake" => "ruby",
        "dart" => "dart",
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hpp" => "cpp",
        _ => return None,
    };
    Some(language.to_string())
}

fn architecture_package_names(snapshot: &InventorySnapshot) -> HashMap<String, String> {
    let architecture = snapshot.metadata.architecture.as_ref();
    let legacy_packages = architecture
        .and_then(|architecture| architecture.get("packages"))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten();
    let indexed_packages = architecture
        .and_then(|architecture| architecture.get("nodes"))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter(|node| {
            node.get("kind")
                .or_else(|| node.get("label"))
                .and_then(serde_json::Value::as_str)
                .is_some_and(|kind| kind.eq_ignore_ascii_case("package"))
        });

    legacy_packages
        .chain(indexed_packages)
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

fn package_group_seed(
    label: &str,
    assigned_by: &'static str,
    title_priority: u8,
    evidence: String,
) -> AtlasGroupSeed {
    let label = if label.trim().is_empty() { "root" } else { label };
    AtlasGroupSeed {
        id: format!("group:package:{}", slug(label)),
        label: label.to_string(),
        title_priority,
        evidence,
        parent_id: None,
        parent_title: None,
        depth: 0,
        assigned_by,
    }
}

fn atlas_group_seed(
    item: &InventoryItem,
    packages: &HashMap<String, String>,
    modules: &ArchitectureModuleIndex,
) -> Option<AtlasGroupSeed> {
    if item.is_code() {
        if let Some(module) = modules.module_for_item(item) {
            return Some(AtlasGroupSeed {
                id: module.group_id.clone(),
                label: module.title.clone(),
                title_priority: if item.layer == "api" { 0 } else { 1 },
                evidence: format!(
                    "코드 엔진 MODULE path `{}` 기준으로 묶었습니다",
                    module.path
                ),
                parent_id: Some(module.package_group_id.clone()),
                parent_title: Some(module.package_label.clone()),
                depth: 1,
                assigned_by: "module-path",
            });
        }
    }
    if !packages.is_empty() && item.is_code() {
        let package = structural_package(item, packages);
        let assigned_by = if package.is_some() { "package" } else { "path-root" };
        let (label, evidence) = match package {
            Some(package) => (
                package.clone(),
                format!("코드 엔진 architecture package `{package}` 기준으로 묶었습니다"),
            ),
            None => {
                    let root = structural_path_root(item.path.as_deref()).unwrap_or("root");
                (
                    root.to_string(),
                    format!(
                        "architecture package와 매칭되지 않아 소스 최상위 경로 `{root}` 기준으로 묶었습니다"
                    ),
                )
            }
        };
        return Some(package_group_seed(
            &label,
            assigned_by,
            if item.layer == "api" { 0 } else { 1 },
            evidence,
        ));
    }
    if !packages.is_empty() && item.is_db() && item.kind == "table" {
        let schema = item
            .path
            .as_deref()
            .filter(|schema| !schema.is_empty())
            .unwrap_or("default");
        return Some(AtlasGroupSeed {
            id: format!("group:db-schema:{}", slug(schema)),
            label: format!("DB · {schema}"),
            title_priority: 2,
            evidence: format!("DB 스키마 `{schema}` 경계 기준으로 묶었습니다"),
            parent_id: None,
            parent_title: None,
            depth: 0,
            assigned_by: "package",
        });
    }
    if item.is_code() && item.layer == "api" {
        let label = route_domain(&item.name).unwrap_or_else(|| "root".to_string());
        return Some(AtlasGroupSeed {
            id: format!("group:domain:{}", slug(&canonical_domain(&label))),
            label: label.clone(),
            title_priority: 0,
            evidence: format!(
                "구조 메타데이터가 없어 라우트 경로에서 보조 그룹 `{label}`을 만들었습니다"
            ),
            parent_id: None,
            parent_title: None,
            depth: 0,
            assigned_by: "path-root",
        });
    }
    if item.is_code() && item.layer == "code" {
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
        let key = canonical_domain(&label);
        return Some(AtlasGroupSeed {
            id: format!("group:domain:{}", slug(&key)),
            label,
            title_priority: 1,
            evidence: format!("코드 경로/그룹 `{evidence_source}` 기준으로 묶었습니다"),
            parent_id: None,
            parent_title: None,
            depth: 0,
            assigned_by: "path-root",
        });
    }
    if item.is_db() && item.kind == "table" {
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
        let key = canonical_domain(&label);
        return Some(AtlasGroupSeed {
            id: format!("group:domain:{}", slug(&key)),
            label,
            title_priority: 2,
            evidence,
            parent_id: None,
            parent_title: None,
            depth: 0,
            assigned_by: "path-root",
        });
    }

    None
}

fn atlas_group_node(group: &AtlasGroup, depth: Option<usize>) -> VisualNode {
    let mut languages = group.language_counts.iter().collect::<Vec<_>>();
    languages.sort_by(|(left, left_count), (right, right_count)| {
        right_count.cmp(left_count).then_with(|| left.cmp(right))
    });
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
        parent_id: group.parent_id.clone(),
        depth: Some(group.depth),
        assigned_by: Some(group.assigned_by.to_string()),
        location: None,
        metrics: Some(VisualNodeMetrics {
            member_count: group.member_ids.len(),
            api_count: group.api_count,
            code_count: group.code_count,
            db_count: group.db_count,
            top_api: group.top_api.clone(),
            top_code: group.top_code.clone(),
            top_db: group.top_db.clone(),
            handler_count: group.handler_count,
            service_count: group.service_count,
            repository_count: group.repository_count,
            depth,
            in_degree: group.in_degree,
            out_degree: group.out_degree,
        }),
        coverage: Some(VisualNodeCoverage {
            languages: languages
                .into_iter()
                .map(|(language, _)| language.clone())
                .collect(),
            has_blind_spot: group.language_counts.contains_key("unknown"),
            has_partial: group.has_partial,
        }),
    }
}

fn atlas_group_has_partial(
    snapshot: &InventorySnapshot,
    group: &AtlasGroup,
    item_group: &HashMap<String, String>,
) -> bool {
    let db_projection_is_partial = snapshot.metadata.db.as_ref().is_some_and(|metadata| {
        metadata.truncated == Some(true) || metadata.limit_clamped == Some(true)
    });
    if db_projection_is_partial && group.db_count > 0 {
        return true;
    }

    snapshot.metadata.gaps.iter().any(|gap| {
        if !gap.related_ids.is_empty() {
            return gap
                .related_ids
                .iter()
                .any(|id| {
                    item_group.get(id) == Some(&group.id)
                        || group.member_ids.iter().any(|member_id| member_id == id)
                });
        }

        if gap.kind.starts_with("db-") {
            group.db_count > 0
        } else if gap.kind.starts_with("code")
            || gap.kind.starts_with("provider")
            || gap.kind.contains("call")
            || gap.kind.contains("handle")
            || gap.kind.starts_with("client-request")
            || gap.kind.starts_with("unscored")
        {
            group.code_count > 0
        } else {
            // A source-agnostic gap is a project-level warning. Keep it visible
            // on every non-empty area rather than silently presenting a clean map.
            group.code_count > 0 || group.db_count > 0
        }
    })
}

fn atlas_group_edges(
    snapshot: &InventorySnapshot,
    item_group: &HashMap<String, String>,
    visible_ids: &HashSet<String>,
) -> (Vec<VisualEdge>, usize) {
    let mut seen = HashSet::new();
    let mut weights = HashMap::<String, usize>::new();
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
                *weights.entry(id).or_default() += 1;
                return None;
            }
            weights.insert(id.clone(), 1);
            Some(VisualEdge {
                id,
                from: from.clone(),
                to: to.clone(),
                kind,
                confidence: None,
                evidence: link.evidence.clone(),
                weight: None,
            })
        })
        .collect::<Vec<_>>();
    edges.sort_by(|left, right| {
        atlas_projection_edge_rank(left)
            .cmp(&atlas_projection_edge_rank(right))
            .then_with(|| left.id.cmp(&right.id))
    });
    for edge in &mut edges {
        edge.weight = weights.get(&edge.id).copied();
    }
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
    let mut weights = HashMap::<String, usize>::new();
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
                *weights.entry(id).or_default() += 1;
                return None;
            }
            weights.insert(id.clone(), 1);
            Some(VisualEdge {
                id,
                from: from.to_string(),
                to: to.to_string(),
                kind,
                confidence: None,
                evidence: link.evidence.clone(),
                weight: None,
            })
        })
        .collect::<Vec<_>>();
    edges.sort_by(|left, right| {
        atlas_projection_edge_rank(left)
            .cmp(&atlas_projection_edge_rank(right))
            .then_with(|| left.id.cmp(&right.id))
    });
    for edge in &mut edges {
        edge.weight = weights.get(&edge.id).copied();
    }
    let hidden = edges.len().saturating_sub(64);
    edges.truncate(64);
    (edges, hidden)
}
