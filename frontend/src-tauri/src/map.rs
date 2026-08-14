use crate::models::{MapDomain, MapFeature, MapStats, SemanticDomain};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StaticAnalysisResult {
    summary: ProjectSummaryJson,
    overview: Option<OverviewJson>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectSummaryJson {
    total_files: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OverviewJson {
    domains: Vec<DomainJson>,
    features: Vec<FeatureJson>,
    relations: Vec<RelationJson>,
    coverage: CoverageJson,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DomainJson {
    id: String,
    key: String,
    label: String,
    status: String,
    confidence: ConfidenceJson,
    primary_unit_ids: Vec<String>,
    shared_unit_ids: Vec<String>,
    entrypoint_ids: Vec<String>,
    feature_ids: Vec<String>,
    evidence: Vec<EvidenceJson>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfidenceJson {
    score: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FeatureJson {
    id: String,
    label: String,
    kind: String,
    summary: Option<String>,
    entrypoint_ids: Vec<String>,
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
    description: String,
}

pub(crate) fn build_map(
    static_result_path: &Path,
    semantic_domains: &[SemanticDomain],
) -> Result<(Vec<MapDomain>, MapStats), String> {
    let source = fs::read_to_string(static_result_path)
        .map_err(|error| format!("정적 분석 결과를 읽지 못했습니다: {error}"))?;
    let result: StaticAnalysisResult = serde_json::from_str(&source)
        .map_err(|error| format!("정적 분석 결과 형식이 올바르지 않습니다: {error}"))?;
    let overview = result
        .overview
        .ok_or_else(|| "정적 분석 결과에 overview가 없습니다.".to_string())?;
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
                .map(|domain| domain.primary_unit_ids.len() + domain.shared_unit_ids.len())
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
                        representative.key, unit_count, feature_count
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
                    summary: feature.summary.clone(),
                    kind: feature.kind.clone(),
                    entrypoints: feature.entrypoint_ids.len(),
                })
                .collect::<Vec<_>>();
            let mut signals = members
                .iter()
                .flat_map(|domain| domain.evidence.iter())
                .map(|evidence| format!("{}: {}", evidence.kind, evidence.description))
                .collect::<Vec<_>>();
            if signals.is_empty() {
                signals.push(format!("domain:{}", representative.key));
            }
            signals.sort();
            signals.dedup();
            signals.truncate(8);
            let shared_unit_count = members
                .iter()
                .map(|domain| domain.shared_unit_ids.len())
                .sum::<usize>();
            let entrypoints = members
                .iter()
                .map(|domain| domain.entrypoint_ids.len())
                .sum::<usize>();

            MapDomain {
                domain_id: canonical_id.clone(),
                name,
                summary,
                status: map_status(&representative.status, shared_unit_count),
                confidence: representative.confidence.score.min(100),
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
        files: overview.coverage.total_files.max(result.summary.total_files),
        units: overview.coverage.total_units,
        features: overview.coverage.total_features,
        flows: overview.coverage.total_execution_flows,
        resources: overview.coverage.total_resources,
    };

    Ok((domains, stats))
}

fn map_status(status: &str, shared_unit_count: usize) -> String {
    if shared_unit_count > 0 {
        return "shared".to_string();
    }
    match status {
        "confirmed" => "verified".to_string(),
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
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn overview에서_도메인_관계와_기능을_투영한다() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("visual-map-map-{suffix}.json"));
        let json = r#"{
          "summary": { "totalFiles": 1 },
          "overview": {
            "domains": [{
              "id": "domain-a",
              "key": "order",
              "label": "Order",
              "status": "confirmed",
              "confidence": { "score": 82, "level": "high", "signalFamilies": [] },
              "primaryUnitIds": ["unit-1"],
              "sharedUnitIds": [],
              "entrypointIds": ["entry-1"],
              "featureIds": ["feature-1"],
              "resourceIds": [],
              "evidence": [{ "id": "ev-1", "kind": "unit", "description": "OrderService" }]
            }],
            "features": [{
              "id": "feature-1",
              "key": "create_order",
              "label": "Create Order",
              "kind": "endpoint",
              "status": "confirmed",
              "visibility": "userFacing",
              "confidence": { "level": "high", "resolvedEdgeCount": 1, "unresolvedEdgeCount": 0, "dynamicEdgeCount": 0, "evidenceCount": 1 },
              "domainIds": ["domain-a"],
              "unitIds": ["unit-1"],
              "reachableUnitCount": 1,
              "entrypointIds": ["entry-1"],
              "flowIds": [],
              "resourceIds": [],
              "dynamicBoundaryIds": [],
              "evidence": []
            }],
            "relations": [{
              "sourceDomainId": "domain-a",
              "targetDomainId": "domain-b",
              "kind": "call",
              "status": "confirmed",
              "weight": 1,
              "evidence": []
            }],
            "coverage": {
              "totalFiles": 3,
              "totalUnits": 4,
              "totalFeatures": 1,
              "totalExecutionFlows": 2,
              "totalResources": 1
            }
          }
        }"#;
        let mut file = std::fs::File::create(&path).expect("fixture를 써야 한다");
        file.write_all(json.as_bytes()).expect("fixture를 써야 한다");

        let semantic = vec![SemanticDomain {
            domain_id: "domain-a".into(),
            source_domain_ids: vec!["domain-a".into()],
            name: "주문".into(),
            summary: Some("주문 처리".into()),
        }];
        let (domains, stats) =
            build_map(&path, &semantic).expect("지도를 만들어야 한다");
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

        std::fs::remove_file(path).expect("fixture를 정리해야 한다");
    }

    #[test]
    fn canonical_도메인은_정적_카드를_하나로_합친다() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("visual-map-map-canonical-{suffix}.json"));
        let json = r#"{
          "summary": { "totalFiles": 1 },
          "overview": {
            "domains": [
              {
                "id": "domain-a",
                "key": "auth",
                "label": "Auth",
                "status": "confirmed",
                "confidence": { "score": 70, "level": "medium", "signalFamilies": [] },
                "primaryUnitIds": ["unit-1"],
                "sharedUnitIds": [],
                "entrypointIds": ["entry-1"],
                "featureIds": ["feature-1"],
                "resourceIds": [],
                "evidence": []
              },
              {
                "id": "domain-b",
                "key": "session",
                "label": "Session",
                "status": "confirmed",
                "confidence": { "score": 80, "level": "high", "signalFamilies": [] },
                "primaryUnitIds": ["unit-2"],
                "sharedUnitIds": [],
                "entrypointIds": ["entry-2"],
                "featureIds": ["feature-2"],
                "resourceIds": [],
                "evidence": []
              }
            ],
            "features": [
              {
                "id": "feature-1",
                "key": "login",
                "label": "Login",
                "kind": "endpoint",
                "status": "confirmed",
                "visibility": "userFacing",
                "confidence": { "level": "high", "resolvedEdgeCount": 1, "unresolvedEdgeCount": 0, "dynamicEdgeCount": 0, "evidenceCount": 1 },
                "domainIds": ["domain-a"],
                "unitIds": ["unit-1"],
                "reachableUnitCount": 1,
                "entrypointIds": ["entry-1"],
                "flowIds": [],
                "resourceIds": [],
                "dynamicBoundaryIds": [],
                "evidence": []
              },
              {
                "id": "feature-2",
                "key": "session",
                "label": "Session",
                "kind": "endpoint",
                "status": "confirmed",
                "visibility": "userFacing",
                "confidence": { "level": "high", "resolvedEdgeCount": 1, "unresolvedEdgeCount": 0, "dynamicEdgeCount": 0, "evidenceCount": 1 },
                "domainIds": ["domain-b"],
                "unitIds": ["unit-2"],
                "reachableUnitCount": 1,
                "entrypointIds": ["entry-2"],
                "flowIds": [],
                "resourceIds": [],
                "dynamicBoundaryIds": [],
                "evidence": []
              }
            ],
            "relations": [{
              "sourceDomainId": "domain-a",
              "targetDomainId": "domain-b",
              "kind": "call",
              "status": "confirmed",
              "weight": 1,
              "evidence": []
            }],
            "coverage": {
              "totalFiles": 2,
              "totalUnits": 2,
              "totalFeatures": 2,
              "totalExecutionFlows": 0,
              "totalResources": 0
            }
          }
        }"#;
        let mut file = std::fs::File::create(&path).expect("fixture를 써야 한다");
        file.write_all(json.as_bytes()).expect("fixture를 써야 한다");

        let semantic = vec![SemanticDomain {
            domain_id: "domain-b".into(),
            source_domain_ids: vec!["domain-a".into(), "domain-b".into()],
            name: "인증과 세션".into(),
            summary: Some("사용자 인증".into()),
        }];
        let (domains, _) = build_map(&path, &semantic).expect("지도를 만들어야 한다");
        assert_eq!(domains.len(), 1);
        assert_eq!(domains[0].domain_id, "domain-b");
        assert_eq!(domains[0].name, "인증과 세션");
        assert_eq!(domains[0].units, 2);
        assert_eq!(domains[0].features, 2);
        assert!(domains[0].dependencies.is_empty());

        std::fs::remove_file(path).expect("fixture를 정리해야 한다");
    }
}
