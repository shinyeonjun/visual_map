use crate::models::{MapDomain, MapFeature, MapFlow, MapFlowStep, MapStats, SemanticDomain};
use serde::Deserialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CleanBundleManifest {
    metadata: CleanBundleMetadata,
    datasets: Vec<CleanDatasetManifest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CleanBundleMetadata {
    coverage_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CleanDatasetManifest {
    name: String,
    parts: Vec<CleanDatasetPart>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CleanDatasetPart {
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DomainJson {
    id: String,
    candidate_key: String,
    label: String,
    status: String,
    confidence_score: u32,
    unit_ids: Vec<String>,
    entrypoint_ids: Vec<String>,
    feature_ids: Vec<String>,
    evidence: Vec<EvidenceJson>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FeatureJson {
    id: String,
    label: String,
    kind: String,
    #[serde(default = "default_confirmed_status")]
    status: String,
    entrypoint_ids: Vec<String>,
    #[serde(default)]
    flow_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FlowJson {
    id: String,
    owner_unit_id: String,
    entry_node_id: String,
    nodes: Vec<FlowNodeJson>,
    edges: Vec<FlowEdgeJson>,
    #[serde(default)]
    dynamic_boundary_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FlowNodeJson {
    id: String,
    kind: String,
    label: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FlowEdgeJson {
    source_node_id: String,
    target_node_id: String,
    #[serde(default)]
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UnitJson {
    id: String,
    name: String,
    qualified_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RelationJson {
    source_domain_id: String,
    target_domain_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoverageJson {
    total_files: usize,
    total_units: usize,
    total_features: usize,
    total_execution_flows: usize,
    total_resources: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvidenceJson {
    kind: String,
    #[serde(alias = "description")]
    value: String,
}

struct CleanOverview {
    domains: Vec<DomainJson>,
    features: Vec<FeatureJson>,
    relations: Vec<RelationJson>,
    flows: Vec<FlowJson>,
    units: Vec<UnitJson>,
    coverage: CoverageJson,
}

pub(crate) fn build_map(
    clean_output_path: &Path,
    semantic_domains: &[SemanticDomain],
) -> Result<(Vec<MapDomain>, MapStats), String> {
    let overview = load_clean_overview(clean_output_path)?;
    if overview.domains.is_empty() {
        return Err("정적 분석에서 도메인을 찾지 못했습니다.".to_string());
    }

    let canonical_by_source = canonical_domain_map(semantic_domains);
    let semantic_by_id: HashMap<&str, &SemanticDomain> = semantic_domains
        .iter()
        .map(|domain| (domain.domain_id.as_str(), domain))
        .collect();
    let features_by_id: HashMap<&str, &FeatureJson> = overview
        .features
        .iter()
        .map(|feature| (feature.id.as_str(), feature))
        .collect();
    let flows_by_id: HashMap<&str, &FlowJson> = overview
        .flows
        .iter()
        .map(|flow| (flow.id.as_str(), flow))
        .collect();
    let units_by_id: HashMap<&str, &UnitJson> = overview
        .units
        .iter()
        .map(|unit| (unit.id.as_str(), unit))
        .collect();
    let mut grouped: Vec<(String, Vec<&DomainJson>)> = Vec::new();
    let mut group_index: HashMap<String, usize> = HashMap::new();
    for domain in &overview.domains {
        let canonical_id = canonical_by_source
            .get(domain.id.as_str())
            .cloned()
            .unwrap_or_else(|| domain.id.clone());
        if let Some(&index) = group_index.get(&canonical_id) {
            grouped[index].1.push(domain);
            continue;
        }
        group_index.insert(canonical_id.clone(), grouped.len());
        grouped.push((canonical_id, vec![domain]));
    }

    let mut dependencies: HashMap<String, Vec<String>> = HashMap::new();
    for relation in &overview.relations {
        let source = canonical_by_source
            .get(relation.source_domain_id.as_str())
            .cloned()
            .unwrap_or_else(|| relation.source_domain_id.clone());
        let target = canonical_by_source
            .get(relation.target_domain_id.as_str())
            .cloned()
            .unwrap_or_else(|| relation.target_domain_id.clone());
        if source == target {
            continue;
        }
        dependencies.entry(source).or_default().push(target);
    }
    for targets in dependencies.values_mut() {
        targets.sort();
        targets.dedup();
    }

    let domains = grouped
        .into_iter()
        .map(|(canonical_id, members)| {
            let representative = members
                .iter()
                .find(|domain| domain.id == canonical_id)
                .copied()
                .unwrap_or(members[0]);
            let semantic = semantic_by_id.get(canonical_id.as_str());
            let name = semantic
                .map(|item| item.name.clone())
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| representative.label.clone());
            let unit_count = members
                .iter()
                .map(|domain| domain.unit_ids.len())
                .sum::<usize>();
            let feature_count = members
                .iter()
                .map(|domain| domain.feature_ids.len())
                .sum::<usize>();
            let summary = semantic
                .and_then(|item| item.summary.clone())
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| {
                    format!(
                        "{} 도메인 · {} units · {} features",
                        representative.candidate_key, unit_count, feature_count
                    )
                });
            let mut feature_ids = members
                .iter()
                .flat_map(|domain| domain.feature_ids.iter())
                .cloned()
                .collect::<Vec<_>>();
            feature_ids.sort();
            feature_ids.dedup();
            let feature_items = feature_ids
                .iter()
                .filter_map(|feature_id| features_by_id.get(feature_id.as_str()))
                .map(|feature| MapFeature {
                    id: feature.id.clone(),
                    name: feature.label.clone(),
                    summary: None,
                    kind: feature.kind.clone(),
                    status: map_feature_status(&feature.status),
                    entrypoints: feature.entrypoint_ids.len(),
                    flows: build_feature_flows(feature, &flows_by_id, &units_by_id),
                })
                .collect::<Vec<_>>();
            let mut signals = members
                .iter()
                .flat_map(|domain| domain.evidence.iter())
                .map(|evidence| format!("{}: {}", evidence.kind, evidence.value))
                .collect::<Vec<_>>();
            if signals.is_empty() {
                signals.push(format!("domain:{}", representative.candidate_key));
            }
            signals.sort();
            signals.dedup();
            signals.truncate(8);
            let entrypoints = members
                .iter()
                .map(|domain| domain.entrypoint_ids.len())
                .sum::<usize>();

            MapDomain {
                domain_id: canonical_id.clone(),
                name,
                summary,
                status: map_status(&representative.status),
                confidence: representative.confidence_score.min(100),
                units: unit_count,
                features: feature_items.len(),
                entrypoints,
                dependencies: dependencies
                    .get(&canonical_id)
                    .cloned()
                    .unwrap_or_default(),
                signals,
                feature_items,
            }
        })
        .collect::<Vec<_>>();

    let stats = MapStats {
        files: overview.coverage.total_files,
        units: overview.coverage.total_units,
        features: overview.coverage.total_features,
        flows: overview.coverage.total_execution_flows,
        resources: overview.coverage.total_resources,
    };

    Ok((domains, stats))
}

fn load_clean_overview(clean_dir: &Path) -> Result<CleanOverview, String> {
    let manifest_path = clean_dir.join("manifest.json");
    let manifest_text = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("clean bundle manifest를 읽지 못했습니다: {error}"))?;
    let manifest: CleanBundleManifest = serde_json::from_str(&manifest_text)
        .map_err(|error| format!("clean bundle manifest 형식이 올바르지 않습니다: {error}"))?;

    let domains = load_dataset(clean_dir, &manifest, "domains")?;
    let features = load_dataset(clean_dir, &manifest, "features")?;
    let relations = load_dataset(clean_dir, &manifest, "relations")?;
    let flows = load_dataset(clean_dir, &manifest, "flows")?;
    let units = load_dataset(clean_dir, &manifest, "units")?;

    let coverage_path = clean_dir.join(&manifest.metadata.coverage_path);
    let coverage: CoverageJson = if coverage_path.exists() {
        let text = fs::read_to_string(&coverage_path)
            .map_err(|error| format!("coverage를 읽지 못했습니다: {error}"))?;
        serde_json::from_str(&text)
            .map_err(|error| format!("coverage 형식이 올바르지 않습니다: {error}"))?
    } else {
        CoverageJson {
            total_files: 0,
            total_units: 0,
            total_features: features.len(),
            total_execution_flows: 0,
            total_resources: 0,
        }
    };

    Ok(CleanOverview {
        domains,
        features,
        relations,
        flows,
        units,
        coverage,
    })
}

fn build_feature_flows(
    feature: &FeatureJson,
    flows_by_id: &HashMap<&str, &FlowJson>,
    units_by_id: &HashMap<&str, &UnitJson>,
) -> Vec<MapFlow> {
    feature
        .flow_ids
        .iter()
        .filter_map(|flow_id| flows_by_id.get(flow_id.as_str()).copied())
        .map(|flow| {
            let owner = units_by_id
                .get(flow.owner_unit_id.as_str())
                .map(|unit| unit.qualified_name.clone())
                .filter(|value| !value.trim().is_empty())
                .or_else(|| {
                    units_by_id
                        .get(flow.owner_unit_id.as_str())
                        .map(|unit| unit.name.clone())
                })
                .unwrap_or_else(|| flow.owner_unit_id.clone());
            let steps = ordered_flow_steps(flow);
            MapFlow {
                id: flow.id.clone(),
                owner,
                status: flow_status(flow, &steps),
                steps,
            }
        })
        .collect()
}

fn ordered_flow_steps(flow: &FlowJson) -> Vec<MapFlowStep> {
    let nodes_by_id: HashMap<&str, &FlowNodeJson> = flow
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let mut next: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
    for edge in &flow.edges {
        let status = edge.status.as_deref().unwrap_or("confirmed");
        next.entry(edge.source_node_id.as_str())
            .or_default()
            .push((edge.target_node_id.as_str(), status));
    }

    let mut visited = HashSet::new();
    let mut queue = VecDeque::from([(flow.entry_node_id.as_str(), None)]);
    let mut steps = Vec::new();
    while let Some((node_id, edge_status)) = queue.pop_front() {
        if !visited.insert(node_id) {
            continue;
        }
        let Some(node) = nodes_by_id.get(node_id) else {
            continue;
        };
        if !is_boundary_flow_kind(&node.kind) {
            steps.push(MapFlowStep {
                label: node.label.clone(),
                kind: node.kind.clone(),
                status: step_status(&node.kind, edge_status),
            });
        }
        if let Some(children) = next.get(node_id) {
            for (child, status) in children {
                if !visited.contains(child) {
                    queue.push_back((child, Some(*status)));
                }
            }
        }
    }
    steps
}

fn step_status(node_kind: &str, edge_status: Option<&str>) -> String {
    if node_kind == "dynamicBoundary" {
        return "candidate".to_string();
    }
    match edge_status {
        Some("confirmed") | None => "verified".to_string(),
        _ => "candidate".to_string(),
    }
}

fn flow_status(flow: &FlowJson, steps: &[MapFlowStep]) -> String {
    if !flow.dynamic_boundary_ids.is_empty() {
        return "candidate".to_string();
    }
    if flow.edges.iter().any(|edge| {
        edge.status
            .as_deref()
            .is_some_and(|status| status != "confirmed")
    }) {
        return "candidate".to_string();
    }
    if steps.iter().any(|step| step.status == "candidate") {
        return "candidate".to_string();
    }
    "verified".to_string()
}

fn default_confirmed_status() -> String {
    "confirmed".to_string()
}

fn map_feature_status(status: &str) -> String {
    match status {
        "confirmed" => "verified".to_string(),
        _ => "candidate".to_string(),
    }
}

fn is_boundary_flow_kind(kind: &str) -> bool {
    kind == "entry" || kind == "exit"
}

fn load_dataset<T: for<'de> Deserialize<'de>>(
    clean_dir: &Path,
    manifest: &CleanBundleManifest,
    name: &str,
) -> Result<Vec<T>, String> {
    let dataset = manifest
        .datasets
        .iter()
        .find(|dataset| dataset.name == name);
    let Some(dataset) = dataset else {
        return Ok(Vec::new());
    };
    let mut items = Vec::new();
    for part in &dataset.parts {
        let part_path = clean_dir.join(&part.path);
        let text = fs::read_to_string(&part_path)
            .map_err(|error| format!("{name} part를 읽지 못했습니다: {error}"))?;
        let part_items: Vec<T> = serde_json::from_str(&text)
            .map_err(|error| format!("{name} part 형식이 올바르지 않습니다: {error}"))?;
        items.extend(part_items);
    }
    Ok(items)
}

fn map_status(status: &str) -> String {
    match status {
        "confirmed" => "verified".to_string(),
        "ambiguous" => "shared".to_string(),
        _ => "candidate".to_string(),
    }
}

fn canonical_domain_map(semantic_domains: &[SemanticDomain]) -> HashMap<String, String> {
    let mut mapped = HashMap::new();
    for domain in semantic_domains {
        mapped
            .entry(domain.domain_id.clone())
            .or_insert_with(|| domain.domain_id.clone());
        for source_id in &domain.source_domain_ids {
            mapped
                .entry(source_id.clone())
                .or_insert_with(|| domain.domain_id.clone());
        }
    }
    mapped
}

#[cfg(test)]
mod tests {
    use super::build_map;
    use crate::models::SemanticDomain;
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct CleanFixture {
        root: PathBuf,
    }

    impl CleanFixture {
        fn new(suffix: &str) -> Self {
            let root = std::env::temp_dir().join(format!("visual-map-clean-{suffix}"));
            fs::create_dir_all(root.join("domains/parts")).expect("fixture 디렉터리를 만들어야 한다");
            fs::create_dir_all(root.join("features/parts")).expect("fixture 디렉터리를 만들어야 한다");
            fs::create_dir_all(root.join("relations/parts")).expect("fixture 디렉터리를 만들어야 한다");
            fs::create_dir_all(root.join("metadata")).expect("fixture 디렉터리를 만들어야 한다");
            Self { root }
        }

        fn write(&self, relative: &str, contents: &str) {
            let path = self.root.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("fixture 부모 디렉터리를 만들어야 한다");
            }
            let mut file = fs::File::create(path).expect("fixture를 써야 한다");
            file.write_all(contents.as_bytes())
                .expect("fixture를 써야 한다");
        }
    }

    impl Drop for CleanFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn clean_bundle에서_도메인_관계와_기능을_투영한다() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_string();
        let fixture = CleanFixture::new(&suffix);
        fixture.write(
            "manifest.json",
            r#"{
              "metadata": { "coveragePath": "metadata/coverage.json" },
              "datasets": [
                { "name": "domains", "parts": [{ "path": "domains/parts/part-0000.json" }] },
                { "name": "features", "parts": [{ "path": "features/parts/part-0000.json" }] },
                { "name": "relations", "parts": [{ "path": "relations/parts/part-0000.json" }] }
              ]
            }"#,
        );
        fixture.write(
            "domains/parts/part-0000.json",
            r#"[{
              "id": "domain-a",
              "candidateKey": "order",
              "label": "Order",
              "status": "confirmed",
              "confidenceScore": 82,
              "unitIds": ["unit-1"],
              "entrypointIds": ["entry-1"],
              "featureIds": ["feature-1"],
              "evidence": [{ "kind": "unit", "value": "OrderService" }]
            }]"#,
        );
        fixture.write(
            "features/parts/part-0000.json",
            r#"[{
              "id": "feature-1",
              "label": "Create Order",
              "kind": "endpoint",
              "entrypointIds": ["entry-1"]
            }]"#,
        );
        fixture.write(
            "relations/parts/part-0000.json",
            r#"[{
              "sourceDomainId": "domain-a",
              "targetDomainId": "domain-b"
            }]"#,
        );
        fixture.write(
            "metadata/coverage.json",
            r#"{
              "totalFiles": 3,
              "totalUnits": 4,
              "totalFeatures": 1,
              "totalExecutionFlows": 2,
              "totalResources": 1
            }"#,
        );

        let semantic = vec![SemanticDomain {
            domain_id: "domain-a".into(),
            source_domain_ids: vec!["domain-a".into()],
            name: "주문".into(),
            summary: Some("주문 처리".into()),
        }];
        let (domains, stats) =
            build_map(&fixture.root, &semantic).expect("지도를 만들어야 한다");
        assert_eq!(domains.len(), 1);
        assert_eq!(domains[0].name, "주문");
        assert_eq!(domains[0].summary, "주문 처리");
        assert_eq!(domains[0].status, "verified");
        assert_eq!(domains[0].confidence, 82);
        assert_eq!(domains[0].units, 1);
        assert_eq!(domains[0].features, 1);
        assert_eq!(domains[0].entrypoints, 1);
        assert_eq!(domains[0].dependencies, vec!["domain-b".to_string()]);
        assert_eq!(domains[0].feature_items[0].name, "Create Order");
        assert_eq!(stats.files, 3);
    }

    #[test]
    fn semantic이_비어_있어도_정적_라벨로_지도를_만든다() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_string();
        let fixture = CleanFixture::new(&format!("empty-semantic-{suffix}"));
        fixture.write(
            "manifest.json",
            r#"{
              "metadata": { "coveragePath": "metadata/coverage.json" },
              "datasets": [
                { "name": "domains", "parts": [{ "path": "domains/parts/part-0000.json" }] },
                { "name": "features", "parts": [{ "path": "features/parts/part-0000.json" }] },
                { "name": "relations", "parts": [{ "path": "relations/parts/part-0000.json" }] }
              ]
            }"#,
        );
        fixture.write(
            "domains/parts/part-0000.json",
            r#"[{
              "id": "domain-a",
              "candidateKey": "order",
              "label": "Order",
              "status": "confirmed",
              "confidenceScore": 82,
              "unitIds": ["unit-1"],
              "entrypointIds": ["entry-1"],
              "featureIds": ["feature-1"],
              "evidence": []
            }]"#,
        );
        fixture.write(
            "features/parts/part-0000.json",
            r#"[{
              "id": "feature-1",
              "label": "Create Order",
              "kind": "endpoint",
              "entrypointIds": ["entry-1"]
            }]"#,
        );
        fixture.write("relations/parts/part-0000.json", r#"[]"#);
        fixture.write(
            "metadata/coverage.json",
            r#"{
              "totalFiles": 1,
              "totalUnits": 1,
              "totalFeatures": 1,
              "totalExecutionFlows": 0,
              "totalResources": 0
            }"#,
        );

        let (domains, _) = build_map(&fixture.root, &[]).expect("지도를 만들어야 한다");
        assert_eq!(domains.len(), 1);
        assert_eq!(domains[0].name, "Order");
        assert_eq!(domains[0].feature_items[0].name, "Create Order");
    }

    #[test]
    fn 기능에_연결된_flow_단계를_순서대로_투영한다() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_string();
        let fixture = CleanFixture::new(&format!("flows-{suffix}"));
        fixture.write(
            "manifest.json",
            r#"{
              "metadata": { "coveragePath": "metadata/coverage.json" },
              "datasets": [
                { "name": "domains", "parts": [{ "path": "domains/parts/part-0000.json" }] },
                { "name": "features", "parts": [{ "path": "features/parts/part-0000.json" }] },
                { "name": "relations", "parts": [{ "path": "relations/parts/part-0000.json" }] },
                { "name": "flows", "parts": [{ "path": "flows/parts/part-0000.json" }] },
                { "name": "units", "parts": [{ "path": "units/parts/part-0000.json" }] }
              ]
            }"#,
        );
        fixture.write(
            "domains/parts/part-0000.json",
            r#"[{
              "id": "domain-a",
              "candidateKey": "order",
              "label": "Order",
              "status": "confirmed",
              "confidenceScore": 82,
              "unitIds": ["unit-1"],
              "entrypointIds": ["entry-1"],
              "featureIds": ["feature-1"],
              "evidence": []
            }]"#,
        );
        fixture.write(
            "features/parts/part-0000.json",
            r#"[{
              "id": "feature-1",
              "label": "Create Order",
              "kind": "endpoint",
              "entrypointIds": ["entry-1"],
              "flowIds": ["flow-1"]
            }]"#,
        );
        fixture.write("relations/parts/part-0000.json", r#"[]"#);
        fixture.write(
            "flows/parts/part-0000.json",
            r#"[{
              "id": "flow-1",
              "ownerUnitId": "unit-1",
              "entryNodeId": "node-entry",
              "exitNodeId": "node-exit",
              "nodes": [
                { "id": "node-entry", "kind": "entry", "label": "entry" },
                { "id": "node-call", "kind": "call", "label": "saveOrder()" },
                { "id": "node-return", "kind": "return", "label": "return" },
                { "id": "node-exit", "kind": "exit", "label": "exit" }
              ],
              "edges": [
                { "sourceNodeId": "node-entry", "targetNodeId": "node-call" },
                { "sourceNodeId": "node-call", "targetNodeId": "node-return" },
                { "sourceNodeId": "node-return", "targetNodeId": "node-exit" }
              ],
              "dynamicBoundaryIds": []
            }]"#,
        );
        fixture.write(
            "units/parts/part-0000.json",
            r#"[{
              "id": "unit-1",
              "name": "createOrder",
              "qualifiedName": "OrderService.createOrder"
            }]"#,
        );
        fixture.write(
            "metadata/coverage.json",
            r#"{
              "totalFiles": 1,
              "totalUnits": 1,
              "totalFeatures": 1,
              "totalExecutionFlows": 1,
              "totalResources": 0
            }"#,
        );

        let (domains, _) = build_map(&fixture.root, &[]).expect("지도를 만들어야 한다");
        let flows = &domains[0].feature_items[0].flows;
        assert_eq!(flows.len(), 1);
        assert_eq!(flows[0].owner, "OrderService.createOrder");
        assert_eq!(flows[0].steps.len(), 2);
        assert_eq!(flows[0].steps[0].label, "saveOrder()");
        assert_eq!(flows[0].steps[0].kind, "call");
        assert_eq!(flows[0].steps[1].kind, "return");
        assert_eq!(flows[0].status, "verified");
        assert_eq!(flows[0].steps[0].status, "verified");
    }

    #[test]
    fn 후보_기능과_미해결_flow_단계는_candidate로_표시한다() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_string();
        let fixture = CleanFixture::new(&format!("candidate-{suffix}"));
        fixture.write(
            "manifest.json",
            r#"{
              "metadata": { "coveragePath": "metadata/coverage.json" },
              "datasets": [
                { "name": "domains", "parts": [{ "path": "domains/parts/part-0000.json" }] },
                { "name": "features", "parts": [{ "path": "features/parts/part-0000.json" }] },
                { "name": "relations", "parts": [{ "path": "relations/parts/part-0000.json" }] },
                { "name": "flows", "parts": [{ "path": "flows/parts/part-0000.json" }] },
                { "name": "units", "parts": [{ "path": "units/parts/part-0000.json" }] }
              ]
            }"#,
        );
        fixture.write(
            "domains/parts/part-0000.json",
            r#"[{
              "id": "domain-a",
              "candidateKey": "order",
              "label": "Order",
              "status": "candidate",
              "confidenceScore": 42,
              "unitIds": ["unit-1"],
              "entrypointIds": ["entry-1"],
              "featureIds": ["feature-1"],
              "evidence": []
            }]"#,
        );
        fixture.write(
            "features/parts/part-0000.json",
            r#"[{
              "id": "feature-1",
              "label": "Create Order",
              "kind": "endpoint",
              "status": "candidate",
              "entrypointIds": ["entry-1"],
              "flowIds": ["flow-1"]
            }]"#,
        );
        fixture.write("relations/parts/part-0000.json", r#"[]"#);
        fixture.write(
            "flows/parts/part-0000.json",
            r#"[{
              "id": "flow-1",
              "ownerUnitId": "unit-1",
              "entryNodeId": "node-entry",
              "exitNodeId": "node-exit",
              "nodes": [
                { "id": "node-entry", "kind": "entry", "label": "entry" },
                { "id": "node-call", "kind": "dynamicBoundary", "label": "dispatch()" },
                { "id": "node-exit", "kind": "exit", "label": "exit" }
              ],
              "edges": [
                { "sourceNodeId": "node-entry", "targetNodeId": "node-call", "status": "dynamic" },
                { "sourceNodeId": "node-call", "targetNodeId": "node-exit" }
              ],
              "dynamicBoundaryIds": ["dyn-1"]
            }]"#,
        );
        fixture.write(
            "units/parts/part-0000.json",
            r#"[{
              "id": "unit-1",
              "name": "createOrder",
              "qualifiedName": "OrderService.createOrder"
            }]"#,
        );
        fixture.write(
            "metadata/coverage.json",
            r#"{
              "totalFiles": 1,
              "totalUnits": 1,
              "totalFeatures": 1,
              "totalExecutionFlows": 1,
              "totalResources": 0
            }"#,
        );

        let (domains, _) = build_map(&fixture.root, &[]).expect("지도를 만들어야 한다");
        assert_eq!(domains[0].status, "candidate");
        assert_eq!(domains[0].feature_items[0].status, "candidate");
        assert_eq!(domains[0].feature_items[0].flows[0].status, "candidate");
        assert_eq!(domains[0].feature_items[0].flows[0].steps[0].status, "candidate");
    }

    #[test]
    fn canonical_도메인은_정적_카드를_하나로_합친다() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_string();
        let fixture = CleanFixture::new(&format!("canonical-{suffix}"));
        fixture.write(
            "manifest.json",
            r#"{
              "metadata": { "coveragePath": "metadata/coverage.json" },
              "datasets": [
                { "name": "domains", "parts": [{ "path": "domains/parts/part-0000.json" }] },
                { "name": "features", "parts": [{ "path": "features/parts/part-0000.json" }] },
                { "name": "relations", "parts": [{ "path": "relations/parts/part-0000.json" }] }
              ]
            }"#,
        );
        fixture.write(
            "domains/parts/part-0000.json",
            r#"[
              {
                "id": "domain-a",
                "candidateKey": "auth",
                "label": "Auth",
                "status": "confirmed",
                "confidenceScore": 70,
                "unitIds": ["unit-1"],
                "entrypointIds": ["entry-1"],
                "featureIds": ["feature-1"],
                "evidence": []
              },
              {
                "id": "domain-b",
                "candidateKey": "session",
                "label": "Session",
                "status": "confirmed",
                "confidenceScore": 80,
                "unitIds": ["unit-2"],
                "entrypointIds": ["entry-2"],
                "featureIds": ["feature-2"],
                "evidence": []
              }
            ]"#,
        );
        fixture.write(
            "features/parts/part-0000.json",
            r#"[
              {
                "id": "feature-1",
                "label": "Login",
                "kind": "endpoint",
                "entrypointIds": ["entry-1"]
              },
              {
                "id": "feature-2",
                "label": "Session",
                "kind": "endpoint",
                "entrypointIds": ["entry-2"]
              }
            ]"#,
        );
        fixture.write(
            "relations/parts/part-0000.json",
            r#"[{
              "sourceDomainId": "domain-a",
              "targetDomainId": "domain-b"
            }]"#,
        );
        fixture.write(
            "metadata/coverage.json",
            r#"{
              "totalFiles": 2,
              "totalUnits": 2,
              "totalFeatures": 2,
              "totalExecutionFlows": 0,
              "totalResources": 0
            }"#,
        );

        let semantic = vec![SemanticDomain {
            domain_id: "domain-b".into(),
            source_domain_ids: vec!["domain-a".into(), "domain-b".into()],
            name: "인증과 세션".into(),
            summary: Some("사용자 인증".into()),
        }];
        let (domains, _) = build_map(&fixture.root, &semantic).expect("지도를 만들어야 한다");
        assert_eq!(domains.len(), 1);
        assert_eq!(domains[0].domain_id, "domain-b");
        assert_eq!(domains[0].name, "인증과 세션");
        assert_eq!(domains[0].units, 2);
        assert_eq!(domains[0].features, 2);
        assert!(domains[0].dependencies.is_empty());
    }
}
