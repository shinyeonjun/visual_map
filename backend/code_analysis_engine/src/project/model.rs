//! 프로젝트 스캔 결과의 내부 모델이다.

use crate::diagnostics::Diagnostic;
use crate::model::{AnalysisStatus, FileEntry, ProjectContext, ProjectSummary};

/// 프로젝트 스캐너가 반환하는 중간 결과다.
pub struct ScanOutput {
    pub analysis_id: String,
    pub status: AnalysisStatus,
    pub context: ProjectContext,
    pub files: Vec<FileEntry>,
    pub summary: ProjectSummary,
    pub diagnostics: Vec<Diagnostic>,
    pub elapsed_ms: u64,
}

#[derive(Debug)]
pub(super) enum FileReadIssue {
    TooLarge { size: u64, limit: u64 },
    Io(std::io::Error),
}
