use serde::{Deserialize, Serialize};

/// Codex 의미 분석 단계의 상태다.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SemanticStatus {
    Disabled,
    #[default]
    Skipped,
    Completed,
    Partial,
    Failed,
}

/// 의미 분석 provider가 구현할 공통 계약의 표식이다.
pub trait SemanticProvider {
    fn name(&self) -> &'static str;
}
