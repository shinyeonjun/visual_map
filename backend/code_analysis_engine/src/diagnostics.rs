use serde::{Deserialize, Serialize};
use std::path::Path;

/// 분석 중 발견된 문제의 심각도다.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

/// 파싱 전 단계에서도 프론트엔드가 표시할 수 있는 진단 정보다.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl Diagnostic {
    pub fn info(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Info,
            code: code.into(),
            message: message.into(),
            path: None,
        }
    }

    pub fn warning(code: impl Into<String>, message: impl Into<String>, path: &Path) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            code: code.into(),
            message: message.into(),
            path: Some(path.to_string_lossy().replace('\\', "/")),
        }
    }

    pub fn error(code: impl Into<String>, message: impl Into<String>, path: &Path) -> Self {
        Self {
            severity: DiagnosticSeverity::Error,
            code: code.into(),
            message: message.into(),
            path: Some(path.to_string_lossy().replace('\\', "/")),
        }
    }
}
