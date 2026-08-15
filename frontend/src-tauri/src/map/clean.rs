use serde::Deserialize;
use std::fs;
use std::path::Path;

use super::status::default_confirmed_status;

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
pub(crate) struct DomainJson {
    pub id: String,
    pub candidate_key: String,
    pub label: String,
    pub status: String,
    pub confidence_score: u32,
    pub unit_ids: Vec<String>,
    pub entrypoint_ids: Vec<String>,
    pub feature_ids: Vec<String>,
    pub evidence: Vec<EvidenceJson>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FeatureJson {
    pub id: String,
    pub label: String,
    pub kind: String,
    #[serde(default = "default_confirmed_status")]
    pub status: String,
    pub entrypoint_ids: Vec<String>,
    #[serde(default)]
    pub flow_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FlowJson {
    pub id: String,
    pub owner_unit_id: String,
    pub entry_node_id: String,
    pub nodes: Vec<FlowNodeJson>,
    pub edges: Vec<FlowEdgeJson>,
    #[serde(default)]
    pub dynamic_boundary_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FlowNodeJson {
    pub id: String,
    pub kind: String,
    pub label: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FlowEdgeJson {
    pub source_node_id: String,
    pub target_node_id: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UnitJson {
    pub id: String,
    pub name: String,
    pub qualified_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RelationJson {
    pub source_domain_id: String,
    pub target_domain_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CoverageJson {
    pub total_files: usize,
    pub total_units: usize,
    pub total_features: usize,
    pub total_execution_flows: usize,
    pub total_resources: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EvidenceJson {
    pub kind: String,
    #[serde(alias = "description")]
    pub value: String,
}

pub(crate) struct CleanOverview {
    pub domains: Vec<DomainJson>,
    pub features: Vec<FeatureJson>,
    pub relations: Vec<RelationJson>,
    pub flows: Vec<FlowJson>,
    pub units: Vec<UnitJson>,
    pub coverage: CoverageJson,
}

pub(crate) fn load_clean_overview(clean_dir: &Path) -> Result<CleanOverview, String> {
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
