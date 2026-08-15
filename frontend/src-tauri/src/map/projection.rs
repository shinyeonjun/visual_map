use crate::models::{MapDomain, MapFeature, MapStats, SemanticDomain};
use std::collections::HashMap;
use std::path::Path;

use super::clean::{load_clean_overview, CleanOverview, DomainJson};
use super::flow::build_feature_flows;
use super::status::{map_feature_status, map_status};

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
    let features_by_id = overview
        .features
        .iter()
        .map(|feature| (feature.id.as_str(), feature))
        .collect::<HashMap<_, _>>();
    let flows_by_id = overview
        .flows
        .iter()
        .map(|flow| (flow.id.as_str(), flow))
        .collect::<HashMap<_, _>>();
    let units_by_id = overview
        .units
        .iter()
        .map(|unit| (unit.id.as_str(), unit))
        .collect::<HashMap<_, _>>();

    let grouped = group_domains(&overview.domains, &canonical_by_source);
    let dependencies = build_dependencies(&overview, &canonical_by_source);

    let domains = grouped
        .into_iter()
        .map(|(canonical_id, members)| {
            project_domain(
                canonical_id,
                members,
                &semantic_by_id,
                &features_by_id,
                &flows_by_id,
                &units_by_id,
                &dependencies,
            )
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

fn group_domains<'a>(
    domains: &'a [DomainJson],
    canonical_by_source: &HashMap<String, String>,
) -> Vec<(String, Vec<&'a DomainJson>)> {
    let mut grouped: Vec<(String, Vec<&DomainJson>)> = Vec::new();
    let mut group_index: HashMap<String, usize> = HashMap::new();
    for domain in domains {
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
    grouped
}

fn build_dependencies(
    overview: &CleanOverview,
    canonical_by_source: &HashMap<String, String>,
) -> HashMap<String, Vec<String>> {
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
    dependencies
}

fn project_domain<'a>(
    canonical_id: String,
    members: Vec<&'a DomainJson>,
    semantic_by_id: &HashMap<&str, &SemanticDomain>,
    features_by_id: &HashMap<&str, &super::clean::FeatureJson>,
    flows_by_id: &HashMap<&str, &super::clean::FlowJson>,
    units_by_id: &HashMap<&str, &super::clean::UnitJson>,
    dependencies: &HashMap<String, Vec<String>>,
) -> MapDomain {
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
    let unit_count = members.iter().map(|domain| domain.unit_ids.len()).sum::<usize>();
    let feature_count = members.iter().map(|domain| domain.feature_ids.len()).sum::<usize>();
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
            flows: build_feature_flows(feature, flows_by_id, units_by_id),
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
