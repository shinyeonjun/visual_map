use super::model::PreparedStaticOverview;
use super::schema::{
    CleanBundleError, CleanBundleGenerator, CleanBundleManifest, CleanBundleSource,
    CleanMetadataManifest, DatasetIndex, DatasetManifest, PartManifest, CLEAN_POLICY_VERSION,
    CLEAN_SCHEMA_VERSION,
};
use super::{storage, validate};
use crate::config::CleanPolicy;
use crate::model::AnalysisResult;
use serde::Serialize;
use std::fs;
use std::path::Path;

/// 이미 준비된 Clean 모델을 저장한다. 테스트와 내부 호출자가 사용할 수 있다.
pub(super) fn write_bundle(
    result: &AnalysisResult,
    prepared: &PreparedStaticOverview,
    output: &Path,
    policy: &CleanPolicy,
) -> Result<CleanBundleManifest, CleanBundleError> {
    if policy.part_target_bytes == 0 {
        return Err(CleanBundleError::InvalidPartTargetBytes);
    }
    validate::validate(prepared)?;

    if output.exists() && !output.is_dir() {
        return Err(CleanBundleError::InvalidOutputPath(output.to_path_buf()));
    }

    let staging = storage::staging_directory(output)?;
    fs::create_dir_all(&staging).map_err(|source| storage::io_error(&staging, source))?;

    let write_result = write_bundle_contents(result, prepared, policy, &staging);
    let (manifest, ()) = match write_result {
        Ok(value) => value,
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
    };

    if let Err(error) = write_json(&staging.join("manifest.json"), &manifest) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    if let Err(error) = storage::replace_directory(&staging, output) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    Ok(manifest)
}

fn write_bundle_contents(
    result: &AnalysisResult,
    prepared: &PreparedStaticOverview,
    policy: &CleanPolicy,
    root: &Path,
) -> Result<(CleanBundleManifest, ()), CleanBundleError> {
    let metadata_root = root.join("metadata");
    fs::create_dir_all(&metadata_root)
        .map_err(|source| storage::io_error(&metadata_root, source))?;

    let project_path = "metadata/project.json".to_string();
    let status_path = "metadata/status.json".to_string();
    let coverage_path = "metadata/coverage.json".to_string();
    let frameworks_path = "metadata/frameworks.json".to_string();
    let unassigned_path = "metadata/unassigned-unit-ids.json".to_string();

    write_json(&root.join(&project_path), &result.project)?;
    write_json(&root.join(&status_path), &result.status)?;
    write_json(&root.join(&coverage_path), &prepared.coverage)?;
    write_json(&root.join(&frameworks_path), &prepared.frameworks)?;
    write_json(&root.join(&unassigned_path), &prepared.unassigned_unit_ids)?;

    let datasets = vec![
        write_dataset(root, "domains", &prepared.domains, policy.part_target_bytes)?,
        write_dataset(
            root,
            "features",
            &prepared.features,
            policy.part_target_bytes,
        )?,
        write_dataset(
            root,
            "flows",
            &prepared.execution_flows.flows,
            policy.part_target_bytes,
        )?,
        write_dataset(
            root,
            "flow-links",
            &prepared.execution_flows.links,
            policy.part_target_bytes,
        )?,
        write_dataset(
            root,
            "relations",
            &prepared.relations,
            policy.part_target_bytes,
        )?,
        write_dataset(
            root,
            "references",
            &prepared.references,
            policy.part_target_bytes,
        )?,
        write_dataset(root, "units", &prepared.units, policy.part_target_bytes)?,
        write_dataset(
            root,
            "entrypoints",
            &prepared.entrypoints,
            policy.part_target_bytes,
        )?,
        write_dataset(
            root,
            "resources",
            &prepared.resources,
            policy.part_target_bytes,
        )?,
        write_dataset(
            root,
            "dynamic-boundaries",
            &prepared.dynamic_boundaries,
            policy.part_target_bytes,
        )?,
    ];

    let policy_hash =
        storage::sha256_hex(&serde_json::to_vec(policy).map_err(CleanBundleError::Serialization)?);
    let prepared_bytes = serde_json::to_vec(prepared).map_err(CleanBundleError::Serialization)?;
    let bundle_id = storage::bundle_id(result, &policy_hash, &prepared_bytes);
    let manifest = CleanBundleManifest {
        schema_version: CLEAN_SCHEMA_VERSION.to_string(),
        bundle_id,
        source: CleanBundleSource {
            analysis_id: result.analysis_id.clone(),
            project_id: result.project.project_id.clone(),
            snapshot_id: result.project.snapshot_id.clone(),
            source_schema_version: result.schema_version.clone(),
        },
        generator: CleanBundleGenerator {
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            policy_version: CLEAN_POLICY_VERSION.to_string(),
            policy_hash,
        },
        metadata: CleanMetadataManifest {
            project_path,
            status_path,
            coverage_path,
            frameworks_path,
            unassigned_unit_ids_path: unassigned_path,
        },
        datasets,
    };

    Ok((manifest, ()))
}

fn write_dataset<T: Serialize>(
    root: &Path,
    name: &str,
    items: &[T],
    target_bytes: usize,
) -> Result<DatasetManifest, CleanBundleError> {
    let parts_root = root.join(name).join("parts");
    fs::create_dir_all(&parts_root).map_err(|source| storage::io_error(&parts_root, source))?;
    let mut parts = Vec::new();
    let mut current = Vec::<Vec<u8>>::new();
    let mut current_bytes = 4_usize; // [\n + \n]\n

    for item in items {
        let serialized = serde_json::to_vec(item).map_err(CleanBundleError::Serialization)?;
        let additional_bytes = serialized.len() + if current.is_empty() { 0 } else { 2 };
        if !current.is_empty() && current_bytes + additional_bytes > target_bytes {
            parts.push(write_part(&parts_root, name, parts.len(), &current)?);
            current.clear();
            current_bytes = 4;
        }
        current_bytes += serialized.len() + if current.is_empty() { 0 } else { 2 };
        current.push(serialized);
    }
    if !current.is_empty() {
        parts.push(write_part(&parts_root, name, parts.len(), &current)?);
    }

    let count = items.len();
    let index_path = format!("{name}/index.json");
    let index = DatasetIndex {
        schema_version: CLEAN_SCHEMA_VERSION.to_string(),
        name: name.to_string(),
        count,
        parts: parts.clone(),
    };
    write_json(&root.join(&index_path), &index)?;

    Ok(DatasetManifest {
        name: name.to_string(),
        index_path,
        count,
        parts,
    })
}

fn write_part(
    parts_root: &Path,
    dataset: &str,
    index: usize,
    items: &[Vec<u8>],
) -> Result<PartManifest, CleanBundleError> {
    let mut bytes = Vec::with_capacity(items.iter().map(Vec::len).sum::<usize>() + 8);
    bytes.extend_from_slice(b"[\n");
    for (item_index, item) in items.iter().enumerate() {
        if item_index > 0 {
            bytes.extend_from_slice(b",\n");
        }
        bytes.extend_from_slice(item);
    }
    bytes.extend_from_slice(b"\n]\n");

    let file_name = format!("part-{index:04}.json");
    let path = parts_root.join(&file_name);
    fs::write(&path, &bytes).map_err(|source| storage::io_error(&path, source))?;
    let relative_path = format!("{dataset}/parts/{file_name}");
    Ok(PartManifest {
        path: relative_path,
        count: items.len(),
        bytes: bytes.len(),
        sha256: storage::sha256_hex(&bytes),
    })
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), CleanBundleError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(CleanBundleError::Serialization)?;
    fs::write(path, bytes).map_err(|source| storage::io_error(path, source))
}
