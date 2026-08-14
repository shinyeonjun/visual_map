use serde::{Deserialize, Serialize};
use std::io;
use std::path::PathBuf;

pub const CLEAN_SCHEMA_VERSION: &str = "clean-static-bundle.v1";
pub const CLEAN_POLICY_VERSION: &str = "clean-policy.v1";

/// Clean bundle 전체의 진입점이다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanBundleManifest {
    pub schema_version: String,
    pub bundle_id: String,
    pub source: CleanBundleSource,
    pub generator: CleanBundleGenerator,
    pub metadata: CleanMetadataManifest,
    pub datasets: Vec<DatasetManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanBundleSource {
    pub analysis_id: String,
    pub project_id: String,
    pub snapshot_id: String,
    pub source_schema_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanBundleGenerator {
    pub engine_version: String,
    pub policy_version: String,
    pub policy_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanMetadataManifest {
    pub project_path: String,
    pub status_path: String,
    pub coverage_path: String,
    pub frameworks_path: String,
    pub unassigned_unit_ids_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetManifest {
    pub name: String,
    pub index_path: String,
    pub count: usize,
    pub parts: Vec<PartManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartManifest {
    pub path: String,
    pub count: usize,
    pub bytes: usize,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DatasetIndex {
    pub(super) schema_version: String,
    pub(super) name: String,
    pub(super) count: usize,
    pub(super) parts: Vec<PartManifest>,
}

#[derive(Debug)]
pub enum CleanBundleError {
    MissingOverview,
    InvalidPartTargetBytes,
    InvalidOutputPath(PathBuf),
    Validation(String),
    Io { path: PathBuf, source: io::Error },
    Serialization(serde_json::Error),
}

impl std::fmt::Display for CleanBundleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingOverview => write!(formatter, "분석 결과에 Overview가 없습니다."),
            Self::InvalidPartTargetBytes => {
                write!(formatter, "Clean partTargetBytes는 0보다 커야 합니다.")
            }
            Self::InvalidOutputPath(path) => write!(
                formatter,
                "Clean bundle 출력 경로가 파일입니다: {}",
                path.display()
            ),
            Self::Validation(message) => {
                write!(formatter, "Clean bundle 무결성 검증 실패: {message}")
            }
            Self::Io { path, source } => {
                write!(
                    formatter,
                    "Clean bundle 파일 작업 실패 ({}): {source}",
                    path.display()
                )
            }
            Self::Serialization(error) => {
                write!(formatter, "Clean bundle JSON 직렬화 실패: {error}")
            }
        }
    }
}

impl std::error::Error for CleanBundleError {}
