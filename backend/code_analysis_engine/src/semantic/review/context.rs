//! `codex-context` 산출물을 의미 분석에 필요한 형태로 읽는다.

use super::ReviewError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewContext {
    pub schema_version: String,
    pub chunk_id: String,
    pub source_analysis_id: String,
    pub source_schema_version: String,
    pub project_id: String,
    pub project_profile: ReviewProjectProfile,
    pub global_summary: ReviewGlobalSummary,
    #[serde(default)]
    pub adjacent_domains: Vec<ReviewAdjacentDomain>,
    pub domains: Vec<ReviewDomain>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReviewProjectProfile {
    pub visible_unit_count: usize,
    pub entrypoint_count: usize,
    pub resource_count: usize,
    pub reference_count: usize,
    pub confirmed_reference_count: usize,
    pub entrypoint_density: f64,
    pub resource_density: f64,
    pub reference_resolution: f64,
    pub max_domain_unit_ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReviewGlobalSummary {
    #[serde(default)]
    pub domain_ids: Vec<String>,
    #[serde(default)]
    pub domain_labels: Vec<String>,
    pub represented_domain_count: usize,
    #[serde(default)]
    pub language_keys: Vec<String>,
    pub total_domains: usize,
    pub total_features: usize,
    #[serde(default)]
    pub total_feature_memberships: usize,
    pub total_flows: usize,
    #[serde(default)]
    pub total_flow_memberships: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewAdjacentDomain {
    pub domain_id: String,
    pub label: String,
    #[serde(default)]
    pub relation_kinds: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewDomain {
    pub domain_id: String,
    #[serde(default)]
    pub source_domain_ids: Vec<String>,
    pub current_label: String,
    #[serde(default)]
    pub role: Value,
    #[serde(default)]
    pub signal: Value,
    #[serde(default)]
    pub source_paths: Vec<String>,
    #[serde(default)]
    pub entrypoints: Vec<ReviewEntrypoint>,
    #[serde(default)]
    pub resources: Vec<ReviewResource>,
    #[serde(default)]
    pub features: Vec<ReviewFeature>,
    #[serde(default)]
    pub flows: Vec<ReviewFlow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewEntrypoint {
    pub id: String,
    pub unit_id: String,
    pub kind: Value,
    pub name: String,
    pub method: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewResource {
    pub id: String,
    pub kind: Value,
    pub name: String,
    pub mode: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFeature {
    pub id: String,
    pub current_label: String,
    pub visibility: Value,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub tags: Vec<Value>,
    #[serde(default)]
    pub symbols: Vec<String>,
    #[serde(default)]
    pub source_paths: Vec<String>,
    #[serde(default)]
    pub entrypoint_ids: Vec<String>,
    #[serde(default)]
    pub resource_ids: Vec<String>,
    #[serde(default)]
    pub flow_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFlow {
    pub id: String,
    #[serde(default)]
    pub feature_ids: Vec<String>,
    pub owner_unit_id: String,
    pub owner_name: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub steps: Vec<ReviewFlowStep>,
    #[serde(default)]
    pub edges: Vec<ReviewFlowEdge>,
    #[serde(default)]
    pub dynamic_boundary_ids: Vec<String>,
    #[serde(default)]
    pub selection_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFlowStep {
    pub id: String,
    pub kind: Value,
    pub label: String,
    pub target_unit_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFlowEdge {
    pub source_node_id: String,
    pub target_node_id: String,
    pub kind: Value,
    pub status: Value,
}

#[derive(Debug, Clone)]
pub struct ReviewInput {
    pub source_path: PathBuf,
    pub contexts: Vec<ReviewContext>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewManifest {
    pub chunks: Vec<ReviewChunkDescriptor>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewChunkDescriptor {
    pub chunk_id: String,
    pub file_name: String,
}

pub fn load(path: &Path) -> Result<ReviewInput, ReviewError> {
    let source = fs::read_to_string(path).map_err(|source| ReviewError::Read {
        path: path.to_path_buf(),
        source,
    })?;

    if let Ok(context) = serde_json::from_str::<ReviewContext>(&source) {
        return Ok(ReviewInput {
            source_path: path.to_path_buf(),
            contexts: vec![context],
        });
    }

    let manifest: ReviewManifest =
        serde_json::from_str(&source).map_err(|source| ReviewError::InvalidInput {
            path: path.to_path_buf(),
            message: format!("단일 context 또는 chunk manifest가 아닙니다: {source}"),
        })?;
    if manifest.chunks.is_empty() {
        return Err(ReviewError::InvalidInput {
            path: path.to_path_buf(),
            message: "chunk manifest에 분석 context가 없습니다.".into(),
        });
    }

    let chunk_directory = path.with_file_name(format!(
        "{}.chunks",
        path.file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("codex-context")
    ));
    let mut contexts = Vec::with_capacity(manifest.chunks.len());
    for descriptor in manifest.chunks {
        let chunk_path = chunk_directory.join(&descriptor.file_name);
        let chunk_source = fs::read_to_string(&chunk_path).map_err(|source| ReviewError::Read {
            path: chunk_path.clone(),
            source,
        })?;
        let context: ReviewContext =
            serde_json::from_str(&chunk_source).map_err(|source| ReviewError::InvalidInput {
                path: chunk_path.clone(),
                message: format!("chunk JSON을 해석하지 못했습니다: {source}"),
            })?;
        if context.chunk_id != descriptor.chunk_id {
            return Err(ReviewError::InvalidInput {
                path: chunk_path,
                message: format!(
                    "manifest와 chunk ID가 다릅니다: {} != {}",
                    descriptor.chunk_id, context.chunk_id
                ),
            });
        }
        contexts.push(context);
    }

    Ok(ReviewInput {
        source_path: path.to_path_buf(),
        contexts,
    })
}
