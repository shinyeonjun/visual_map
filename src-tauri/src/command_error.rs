use serde::Serialize;

use crate::engine;

pub(crate) type CommandResult<T> = Result<T, CommandError>;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CommandError {
    code: &'static str,
    message: &'static str,
    detail: String,
    retryable: bool,
}

impl From<String> for CommandError {
    fn from(value: String) -> Self {
        let lower = value.to_ascii_lowercase();
        let (code, message, retryable) = if lower.contains("timed out")
            || lower.contains("timeout")
            || value.contains("시간이 초과")
        {
            ("timeout", "작업 시간이 초과되었습니다", true)
        } else if value.contains("최신이 아닙니다") || lower.contains("stale") {
            ("snapshot_stale", "읽기 결과가 현재 소스와 다릅니다", true)
        } else if value.contains("코드/DB 읽기 결과")
            || lower.contains("inventory snapshot")
            || (lower.contains("source") && lower.contains("snapshot"))
        {
            (
                "snapshot_missing",
                "코드 또는 DB를 먼저 읽어야 합니다",
                true,
            )
        } else if value.contains("읽기 도구가 없습니다") {
            (
                "engine_unavailable",
                "필요한 읽기 도구를 찾지 못했습니다",
                false,
            )
        } else if value.contains("프로젝트 폴더") && value.contains("찾을 수 없습니다")
        {
            ("invalid_path", "프로젝트 폴더를 확인하세요", false)
        } else if lower.contains("password")
            || lower.contains("access denied")
            || lower.contains("login failed")
            || lower.contains("ora-01017")
            || lower.contains("authentication")
        {
            ("authentication", "인증 정보를 확인하세요", true)
        } else if lower.contains("connection refused")
            || lower.contains("could not connect")
            || lower.contains("network")
        {
            ("connection", "연결 정보를 확인하세요", true)
        } else if value.contains("찾을 수 없습니다") || value.contains("존재하지 않습니다")
        {
            ("not_found", "대상을 찾을 수 없습니다", false)
        } else if value.contains("필요합니다")
            || value.contains("허용되지")
            || value.contains("지원하는")
            || value.contains("경로는 폴더")
        {
            ("invalid_input", "입력값을 확인하세요", false)
        } else {
            ("internal", "작업을 완료하지 못했습니다", true)
        };

        Self {
            code,
            message,
            detail: engine::redact_secrets(&value),
            retryable,
        }
    }
}

impl From<&str> for CommandError {
    fn from(value: &str) -> Self {
        value.to_string().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_errors_are_structured_and_redacted() {
        let error = CommandError::from(
            "postgres://app:secret@localhost/shop connection timed out".to_string(),
        );
        let json = serde_json::to_string(&error).unwrap();

        assert_eq!(error.code, "timeout");
        assert!(error.retryable);
        assert!(!json.contains("secret"));
        assert!(json.contains("[REDACTED]"));
    }
}
