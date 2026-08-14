//! 정적 분석 결과를 프론트엔드와 후속 처리에서 사용할 Clean bundle로 저장한다.
//
// PreparedStaticOverview가 Clean 데이터의 유일한 내부 모델이다. 이 모듈은
// 그 모델을 변경하지 않고 dataset별 JSON part로 나누어 저장한다. 따라서
// 저장 파일을 나누어도 도메인·기능·흐름의 ID 관계는 그대로 유지된다.

mod model;
mod prepare;
mod prepare_support;
mod reader;
mod schema;
mod storage;
mod validate;
mod writer;

use crate::config::CleanPolicy;
use crate::model::AnalysisResult;
use std::path::Path;

pub use model::{
    PreparedDomain, PreparedEntrypoint, PreparedFeature, PreparedReference, PreparedRelation,
    PreparedResource, PreparedStaticOverview, PreparedUnit,
};
pub use prepare::prepare;
pub use reader::{load, LoadedCleanBundle};
pub use schema::{
    CleanBundleError, CleanBundleGenerator, CleanBundleManifest, CleanBundleSource,
    CleanMetadataManifest, DatasetManifest, PartManifest, CLEAN_POLICY_VERSION,
    CLEAN_SCHEMA_VERSION,
};

/// raw 분석 결과에서 Clean bundle을 만든다.
pub fn write_from_result(
    result: &AnalysisResult,
    output: &Path,
    policy: &CleanPolicy,
) -> Result<CleanBundleManifest, CleanBundleError> {
    let overview = result
        .overview
        .as_ref()
        .ok_or(CleanBundleError::MissingOverview)?;
    let prepared = prepare(overview, &result.files);

    writer::write_bundle(result, &prepared, output, policy)
}

#[cfg(test)]
mod tests {
    use super::{write_from_result, CleanBundleManifest, CLEAN_SCHEMA_VERSION};
    use crate::config::CleanPolicy;
    use crate::model::{AnalysisResult, AnalysisStatus, ProjectContext, ProjectSummary};
    use crate::views::overview::OverviewResponse;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn clean_bundle은_메타데이터와_dataset_index를_분리해_저장한다() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("visual-map-clean-{suffix}"));
        let result = AnalysisResult {
            schema_version: "analysis-result.v1".into(),
            analysis_id: "analysis-1".into(),
            status: AnalysisStatus::Ready,
            project: ProjectContext {
                project_id: "project-1".into(),
                root_path: "C:/project".into(),
                snapshot_id: "snapshot-1".into(),
            },
            files: Vec::new(),
            summary: ProjectSummary::default(),
            diagnostics: Vec::new(),
            elapsed_ms: 1,
            overview: Some(OverviewResponse::default()),
        };
        let policy = CleanPolicy {
            part_target_bytes: 1024,
        };

        let manifest =
            write_from_result(&result, &root, &policy).expect("Clean bundle을 저장해야 한다");
        assert_eq!(manifest.schema_version, CLEAN_SCHEMA_VERSION);
        assert!(root.join("manifest.json").is_file());
        assert!(root.join("metadata/project.json").is_file());
        assert!(root.join("domains/index.json").is_file());
        assert!(root.join("features/index.json").is_file());

        let manifest_source = fs::read_to_string(root.join("manifest.json")).unwrap();
        let parsed: CleanBundleManifest = serde_json::from_str(&manifest_source).unwrap();
        assert_eq!(parsed.datasets.len(), 10);
        assert!(parsed.datasets.iter().all(|dataset| dataset.count == 0));

        fs::remove_dir_all(root).expect("테스트 bundle을 정리해야 한다");
    }
}
