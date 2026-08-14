//! Clean bundle 디렉터리를 읽어 PreparedStaticOverview로 복원한다.

use super::model::PreparedStaticOverview;
use super::schema::{CleanBundleError, CleanBundleManifest};
use super::storage;
use crate::flow::{ExecutionFlow, ExecutionFlowGraph, FlowLink};
use crate::model::{AnalysisStatus, ProjectContext};
use std::fs;
use std::path::Path;

pub struct LoadedCleanBundle {
    pub manifest: CleanBundleManifest,
    pub prepared: PreparedStaticOverview,
    pub project: ProjectContext,
    pub status: AnalysisStatus,
}

pub fn load(clean_dir: &Path) -> Result<LoadedCleanBundle, CleanBundleError> {
    let manifest_path = clean_dir.join("manifest.json");
    let manifest_text = fs::read_to_string(&manifest_path)
        .map_err(|source| CleanBundleError::Io {
            path: manifest_path.clone(),
            source,
        })?;
    let manifest: CleanBundleManifest =
        serde_json::from_str(&manifest_text).map_err(CleanBundleError::Serialization)?;

    let project_path = clean_dir.join(&manifest.metadata.project_path);
    let project: ProjectContext = read_json(&project_path)?;

    let status_path = clean_dir.join(&manifest.metadata.status_path);
    let status: AnalysisStatus = read_json(&status_path)?;

    let domains = load_dataset(clean_dir, &manifest, "domains")?;
    let features = load_dataset(clean_dir, &manifest, "features")?;
    let relations = load_dataset(clean_dir, &manifest, "relations")?;
    let references = load_dataset(clean_dir, &manifest, "references")?;
    let units = load_dataset(clean_dir, &manifest, "units")?;
    let entrypoints = load_dataset(clean_dir, &manifest, "entrypoints")?;
    let resources = load_dataset(clean_dir, &manifest, "resources")?;
    let flows: Vec<ExecutionFlow> = load_dataset(clean_dir, &manifest, "flows")?;
    let flow_links: Vec<FlowLink> = load_dataset(clean_dir, &manifest, "flow-links")?;
    let dynamic_boundaries = load_dataset(clean_dir, &manifest, "dynamic-boundaries")?;

    let coverage_path = clean_dir.join(&manifest.metadata.coverage_path);
    let coverage = if coverage_path.exists() {
        read_json(&coverage_path)?
    } else {
        Default::default()
    };

    let frameworks_path = clean_dir.join(&manifest.metadata.frameworks_path);
    let frameworks: Vec<String> = if frameworks_path.exists() {
        read_json(&frameworks_path)?
    } else {
        Vec::new()
    };

    let unassigned_path = clean_dir.join(&manifest.metadata.unassigned_unit_ids_path);
    let unassigned_unit_ids: Vec<String> = if unassigned_path.exists() {
        read_json(&unassigned_path)?
    } else {
        Vec::new()
    };

    let prepared = PreparedStaticOverview {
        schema_version: "prepared-static-overview.v1".into(),
        domains,
        features,
        relations,
        references,
        units,
        entrypoints,
        resources,
        execution_flows: ExecutionFlowGraph {
            flows,
            links: flow_links,
        },
        dynamic_boundaries,
        frameworks,
        unassigned_unit_ids,
        coverage,
    };

    Ok(LoadedCleanBundle {
        manifest,
        prepared,
        project,
        status,
    })
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, CleanBundleError> {
    let text = fs::read_to_string(path).map_err(|source| CleanBundleError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(&text).map_err(CleanBundleError::Serialization)
}

fn load_dataset<T: serde::de::DeserializeOwned>(
    clean_dir: &Path,
    manifest: &CleanBundleManifest,
    name: &str,
) -> Result<Vec<T>, CleanBundleError> {
    let dataset = manifest
        .datasets
        .iter()
        .find(|dataset| dataset.name == name);
    let Some(dataset) = dataset else {
        return Ok(Vec::new());
    };
    let mut items = Vec::with_capacity(dataset.count);
    for part in &dataset.parts {
        let part_path = clean_dir.join(&part.path);
        let bytes = fs::read(&part_path).map_err(|source| CleanBundleError::Io {
            path: part_path.clone(),
            source,
        })?;
        if bytes.len() != part.bytes {
            return Err(CleanBundleError::Validation(format!(
                "{} part {} 바이트 길이가 manifest와 다릅니다",
                name, part.path
            )));
        }
        let digest = storage::sha256_hex(&bytes);
        if digest != part.sha256 {
            return Err(CleanBundleError::Validation(format!(
                "{} part {} sha256가 manifest와 다릅니다",
                name, part.path
            )));
        }
        let text = String::from_utf8(bytes).map_err(|error| CleanBundleError::Validation(format!(
            "{} part {} UTF-8이 아닙니다: {error}",
            name, part.path
        )))?;
        let part_items: Vec<T> =
            serde_json::from_str(&text).map_err(CleanBundleError::Serialization)?;
        items.extend(part_items);
    }
    Ok(items)
}
