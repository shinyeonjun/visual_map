//! 도메인·모듈의 사람이 읽는 이름만 Codex에 요청하는 전용 분석기다.
//!
//! 이 모듈은 기존 도메인 설명·병합 분석과 분리되어 있다. 정적 분석이 만든
//! 그룹과 모듈 후보의 ID는 절대 바꾸지 않고, 이름 필드만 반환한다.

mod candidates;
mod context;
mod prompt;
mod provider;
mod response;
mod runner;

pub use context::{NameContext, NameContextArtifact, NameModuleContext};
pub use runner::{NameAnalysisResult, NameAnalyzer};

#[cfg(test)]
mod tests {
    use super::context::{build_context, NameContext, NameContextArtifact};
    use crate::model::{AnalysisResult, AnalysisStatus, ProjectContext, ProjectSummary};
    use crate::views::overview::model::OverviewResponse;

    #[test]
    fn 이름_컨텍스트는_실행그래프와_전체정적그래프를_포함하지_않는다() {
        let result = AnalysisResult {
            schema_version: "analysis-result.v1".into(),
            analysis_id: "analysis_1".into(),
            status: AnalysisStatus::Ready,
            project: ProjectContext {
                project_id: "project_1".into(),
                root_path: "D:/project".into(),
                snapshot_id: "snapshot_1".into(),
            },
            files: Vec::new(),
            summary: ProjectSummary::default(),
            diagnostics: Vec::new(),
            elapsed_ms: 0,
            overview: Some(OverviewResponse::default()),
        };

        let context = build_context(result.overview.as_ref().expect("overview"));
        let json = serde_json::to_string(&context).expect("컨텍스트 JSON");
        assert!(json.contains("domains"));
        assert!(json.contains("modules"));
        assert!(!json.contains("executionFlows"));
        assert!(!json.contains("staticGraph"));
    }

    #[test]
    fn 빈_이름_컨텍스트도_정상적으로_직렬화된다() {
        let context = NameContext {
            domains: Vec::new(),
            modules: Vec::new(),
        };
        let artifact = NameContextArtifact::single(context);
        assert_eq!(artifact.chunk_count, 1);
    }
}
