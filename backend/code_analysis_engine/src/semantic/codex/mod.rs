//! Codex CLI를 통해 정적 분석 결과의 의미 보정을 요청하는 provider.

mod process;
mod response;

/// Codex CLI 의미 분석 provider의 기본 설정과 식별자다.
#[derive(Debug, Clone)]
pub struct CodexProvider {
    pub executable: String,
    pub timeout_ms: u64,
    /// Codex CLI stdin에 넣을 수 있는 최대 프롬프트 바이트 수다.
    pub max_input_bytes: usize,
    /// 테스트나 실행 래퍼가 필요한 환경에서만 사용하는 선행 인자다.
    pub command_prefix: Vec<String>,
}

impl Default for CodexProvider {
    fn default() -> Self {
        let policy = crate::config::SemanticPolicy::default();
        Self {
            executable: policy.codex_executable,
            timeout_ms: policy.codex_timeout_ms,
            max_input_bytes: policy.codex_max_input_bytes,
            command_prefix: Vec::new(),
        }
    }
}

impl crate::semantic::provider::SemanticProvider for CodexProvider {
    fn name(&self) -> &'static str {
        "codex"
    }
}

#[derive(Debug)]
pub enum CodexError {
    Spawn(String),
    Io(String),
    Timeout,
    InputTooLarge {
        actual_bytes: usize,
        max_bytes: usize,
    },
    Process(String),
    InvalidResponse(String),
}

impl std::fmt::Display for CodexError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(message) => write!(formatter, "Codex CLI 실행 실패: {message}"),
            Self::Io(message) => write!(formatter, "Codex CLI 입출력 실패: {message}"),
            Self::Timeout => write!(formatter, "Codex CLI 시간 제한 초과"),
            Self::InputTooLarge {
                actual_bytes,
                max_bytes,
            } => write!(
                formatter,
                "Codex CLI 입력이 너무 큽니다: {actual_bytes} bytes (최대 {max_bytes} bytes)"
            ),
            Self::Process(message) => write!(formatter, "Codex CLI가 실패했습니다: {message}"),
            Self::InvalidResponse(message) => {
                write!(formatter, "Codex 응답을 해석하지 못했습니다: {message}")
            }
        }
    }
}

impl std::error::Error for CodexError {}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use super::CodexProvider;
    #[cfg(windows)]
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[cfg(windows)]
    #[test]
    fn codex_provider가_읽기전용_cli_출력을_성공적으로_수집한다() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let script = std::env::temp_dir().join(format!("visual-map-fake-codex-{suffix}.cmd"));
        fs::write(
            &script,
            "@echo off\r\necho {\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"{\\\"domains\\\":[],\\\"merges\\\":[]}\"}}\r\n",
        )
        .expect("가짜 Codex 스크립트를 써야 한다");
        let root = std::env::temp_dir();
        let provider = CodexProvider {
            executable: "cmd.exe".into(),
            timeout_ms: 5_000,
            max_input_bytes: 1_000_000,
            command_prefix: vec!["/C".into(), script.to_string_lossy().into_owned()],
        };

        let output = provider
            .execute_prompt("{}", &root)
            .expect("Codex CLI 출력을 받아야 한다");
        assert!(!output.is_empty());
        fs::remove_file(script).expect("가짜 Codex 스크립트를 정리해야 한다");
    }
}
