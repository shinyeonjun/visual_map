//! 정적 분석 파이프라인 진행 상태.

use super::stages::StaticAnalysisStage;

/// 장시간 분석의 단계별 진행 상태다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineProgress {
    pub stage: StaticAnalysisStage,
    pub detail: String,
}

impl PipelineProgress {
    pub fn new(stage: StaticAnalysisStage, detail: impl Into<String>) -> Self {
        Self {
            stage,
            detail: detail.into(),
        }
    }
}
